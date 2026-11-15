//! Host registry of built-in add-ons.
use crate::plugins::types::{PluginFormat, PluginInfo, PluginParameter};
use relight_addon_api::AddonDescriptor;

pub static ADDONS: &[&AddonDescriptor] = &[
    &relight_addon_noise_suppressor::ADDON,
    &relight_addon_deep_filter::ADDON,
    &relight_addon_compressor::ADDON,
    &relight_addon_voice::ADDON,
];

pub fn find(id: &str) -> Option<&'static AddonDescriptor> {
    ADDONS.iter().copied().find(|d| d.id == id)
}

/// f32 -> f64 through its shortest decimal form, so 0.1 stays 0.1.
fn f(v: f32) -> f64 {
    v.to_string().parse().unwrap_or(v as f64)
}

pub fn initial_params(d: &AddonDescriptor) -> Vec<PluginParameter> {
    d.params
        .iter()
        .map(|p| PluginParameter {
            id: p.id,
            name: p.name.into(),
            value: f(p.default),
            min: f(p.min),
            max: f(p.max),
            default: f(p.default),
        })
        .collect()
}

pub fn plugin_info(d: &AddonDescriptor) -> PluginInfo {
    PluginInfo {
        id: d.id.into(),
        name: d.name.into(),
        vendor: "ReLightHost".into(),
        version: d.version.to_string(),
        path: d.id.into(),
        format: PluginFormat::Builtin,
        category: d.category.into(),
        sub_index: 0,
    }
}

/// Golden renders: pin the built-in DSP's exact output so a refactor can
/// prove nothing changed. Regenerate with `GOLDEN_WRITE=1`.
#[cfg(test)]
mod golden_tests {
    use super::{find, initial_params};
    use relight_addon_api::AddonProcessor;
    use std::path::PathBuf;

    const SR: usize = 48_000;
    const BLOCK: usize = 480;

    fn lcg(seed: u32, n: usize) -> Vec<f32> {
        let mut s = seed;
        (0..n)
            .map(|_| {
                s = s.wrapping_mul(1664525).wrapping_add(1013904223);
                (s >> 8) as f32 / 16_777_216.0 * 2.0 - 1.0
            })
            .collect()
    }

    /// 0.5 s stereo: AM tones plus noise, a -60 to -6 dB ramp and a silent gap.
    fn golden_input() -> (Vec<f32>, Vec<f32>) {
        let n = SR / 2;
        let chan = |freq: f32, seed: u32| -> Vec<f32> {
            let noise = lcg(seed, n);
            (0..n)
                .map(|i| {
                    let t = i as f32 / SR as f32;
                    let tau = std::f32::consts::TAU;
                    let mut v = 0.3 * (tau * freq * t).sin() * (1.0 + 0.5 * (tau * 3.0 * t).sin())
                        + 0.05 * noise[i];
                    if (6000..8400).contains(&i) {
                        let db = -60.0 + 54.0 * (i - 6000) as f32 / 2400.0;
                        v *= 10f32.powf(db / 20.0);
                    }
                    if (16000..18400).contains(&i) {
                        v = 0.0;
                    }
                    v
                })
                .collect()
        };
        (chan(220.0, 1), chan(330.0, 2))
    }

    fn render(p: &mut dyn AddonProcessor) -> Vec<f32> {
        let (mut l, mut r) = golden_input();
        for (cl, cr) in l.chunks_mut(BLOCK).zip(r.chunks_mut(BLOCK)) {
            p.process(cl, cr, &mut [0.0]);
        }
        l.iter().zip(&r).flat_map(|(a, b)| [*a, *b]).collect()
    }

    fn golden_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../addons/golden")
    }

    fn write_mode() -> bool {
        std::env::var("GOLDEN_WRITE").is_ok_and(|v| v == "1")
    }

    /// Write (GOLDEN_WRITE=1) or compare (max |diff| <= 1e-6) one rendered case.
    fn check_or_write(case: &str, out: &[f32]) {
        let path = golden_dir().join(format!("{case}.f32"));
        if write_mode() {
            std::fs::create_dir_all(golden_dir()).unwrap();
            std::fs::write(&path, out.iter().flat_map(|s| s.to_le_bytes()).collect::<Vec<u8>>()).unwrap();
            return;
        }
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let want: Vec<f32> = bytes.chunks_exact(4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).collect();
        assert_eq!(want.len(), out.len(), "{case}: length");
        let max = want.iter().zip(out).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
        assert!(max <= 1e-6, "{case}: max diff {max}");
    }

    fn run(id: &str, short: &str, custom: &[(u32, f32)]) {
        for (suffix, params) in [("default", &[][..]), ("custom", custom)] {
            let mut p = (find(id).unwrap().create)(SR as f32).unwrap();
            for &(pid, v) in params {
                p.set_parameter(pid, v);
            }
            check_or_write(&format!("{short}_{suffix}"), &render(p.as_mut()));
        }
    }

    #[test]
    #[ignore]
    fn golden_noise_suppressor() {
        run(relight_addon_noise_suppressor::ID, "noise_suppressor", &[(0, 0.6), (1, 0.4), (2, 0.5)]);
    }

    #[test]
    #[ignore]
    fn golden_compressor() {
        run(
            relight_addon_compressor::ID,
            "compressor",
            &[(0, -30.0), (1, 6.0), (2, 5.0), (3, 200.0), (4, 6.0), (5, 6.0), (6, 0.8)],
        );
    }

    #[test]
    #[ignore]
    fn golden_voice() {
        run(relight_addon_voice::ID, "voice", &[(0, 6.0), (1, -3.0), (2, 4.0), (3, 0.5), (4, 0.4), (5, -3.0)]);
    }

    #[test]
    fn builtin_param_tables_match_golden() {
        let ids = [
            relight_addon_noise_suppressor::ID,
            relight_addon_deep_filter::ID,
            relight_addon_compressor::ID,
            relight_addon_voice::ID,
        ];
        let got: serde_json::Value = ids
            .iter()
            .map(|id| (id.to_string(), serde_json::to_value(initial_params(find(id).unwrap())).unwrap()))
            .collect::<serde_json::Map<_, _>>()
            .into();
        let path = golden_dir().join("params.json");
        if write_mode() {
            std::fs::create_dir_all(golden_dir()).unwrap();
            std::fs::write(&path, serde_json::to_string_pretty(&got).unwrap()).unwrap();
            return;
        }
        let want: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(want, got);
    }
}
