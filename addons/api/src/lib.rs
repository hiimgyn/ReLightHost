//! API shared by the host and the add-on crates.

pub mod rt;
#[cfg(windows)]
pub mod mmcss;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scale {
    Linear,
    Db,
    Log,
}

pub struct ParamSpec {
    pub id: u32,
    pub name: &'static str,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub unit: &'static str,
    pub scale: Scale,
    pub step: f32,
    pub choices: &'static [&'static str],
}

/// Rewrites stored `(param id, value)` pairs from an older add-on version.
pub type MigrateFn = fn(from: u32, params: &mut Vec<(u32, f32)>);

pub struct AddonDescriptor {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub version: u32,
    pub params: &'static [ParamSpec],
    pub meters: &'static [&'static str],
    /// Empty = any.
    pub sample_rates: &'static [u32],
    pub create: fn(sample_rate: f32) -> Option<Box<dyn AddonProcessor>>,
    pub migrate: Option<MigrateFn>,
}

pub trait AddonProcessor: Send {
    /// Must write every meter slot each call.
    fn process(&mut self, l: &mut [f32], r: &mut [f32], meters: &mut [f32]);
    fn set_parameter(&mut self, id: u32, value: f32);
    /// Called on a control thread while bypassed, never concurrently with process; may allocate.
    fn reset(&mut self) {}
    fn latency_samples(&self) -> u32 {
        0
    }
}

/// Descriptor sanity: at most 64 params, at most 8 meters, `p[i].id == i`.
pub const fn check_params(p: &[ParamSpec], meters: &[&str]) -> bool {
    if p.len() > 64 || meters.len() > 8 {
        return false;
    }
    let mut i = 0;
    while i < p.len() {
        if p[i].id != i as u32 {
            return false;
        }
        i += 1;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn spec(id: u32) -> ParamSpec {
        ParamSpec { id, name: "p", min: 0.0, max: 1.0, default: 0.0, unit: "", scale: Scale::Linear, step: 0.0, choices: &[] }
    }

    #[test]
    fn check_params_rejects_65_params_and_non_sequential_ids() {
        let ok: Vec<ParamSpec> = (0..64).map(spec).collect();
        assert!(check_params(&ok, &["a"; 8]));
        let too_many: Vec<ParamSpec> = (0..65).map(spec).collect();
        assert!(!check_params(&too_many, &[]));
        assert!(!check_params(&[spec(0), spec(2)], &[]));
        assert!(!check_params(&[spec(0)], &["a"; 9]));
    }
}
