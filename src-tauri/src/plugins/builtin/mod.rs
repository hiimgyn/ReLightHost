/// Registry of built-in audio processors compiled directly into ReLightHost.
///
/// Each built-in implements `BuiltinProcessor` and is identified by a stable
/// string ID stored as `PluginInfo::path` so it survives presets and sessions.

pub mod noise_suppressor;
pub mod compressor;

pub use noise_suppressor::NoiseSuppressor;
pub use compressor::Compressor;

use crate::plugins::types::PluginParameter;

// ── Trait ─────────────────────────────────────────────────────────────────────

/// Common interface for all built-in processors.
///
/// Object-safe (`dyn BuiltinProcessor`) and `Send` so it can live inside a
/// `Mutex<Option<Box<dyn BuiltinProcessor>>>` on `PluginInstance`.
pub trait BuiltinProcessor: Send {
    /// Process a stereo buffer in-place.  Expected signal range: -1.0..1.0.
    fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]);

    /// Set a parameter by ID to its raw (non-normalised) value, already
    /// clamped to the `[min, max]` range declared in `builtin_initial_params`.
    fn set_parameter(&mut self, id: u32, value: f32);

    /// Voice-activity probability from the last processed frame (0.0 – 1.0).
    /// Only meaningful for `NoiseSuppressor`; returns 0.0 for all others.
    fn get_vad(&self) -> f32 { 0.0 }
}

// ── Factory ───────────────────────────────────────────────────────────────────

/// Construct the built-in processor identified by `id`.
/// Returns `None` if `id` is not registered.
pub fn create_builtin(id: &str, sample_rate: f32) -> Option<Box<dyn BuiltinProcessor>> {
    match id {
        noise_suppressor::ID => Some(Box::new(NoiseSuppressor::new())),
        compressor::ID       => Some(Box::new(Compressor::new(sample_rate))),
        _                    => None,
    }
}

// ── Default parameters ────────────────────────────────────────────────────────

/// Return the initial `PluginParameter` list for the given built-in `id`.
/// Values are already set to defaults; the host writes these to `PluginInstance`
/// so they appear in the frontend and can be persisted in presets.
pub fn builtin_initial_params(id: &str) -> Vec<PluginParameter> {
    match id {
        noise_suppressor::ID => vec![
            PluginParameter { id: 0, name: "Mix".into(),                value: 1.0,  min:   0.0, max:  1.0,  default: 1.0  },
            PluginParameter { id: 1, name: "VAD Gate Threshold".into(), value: 0.0,  min:   0.0, max:  1.0,  default: 0.0  },
            PluginParameter { id: 2, name: "Gate Attenuation".into(),   value: 0.0,  min:   0.0, max:  1.0,  default: 0.0  },
            PluginParameter { id: 3, name: "Output Gain".into(),        value: 0.0,  min: -24.0, max: 12.0,  default: 0.0  },
        ],
        compressor::ID => vec![
            PluginParameter { id: 0, name: "Threshold".into(),   value: -18.0, min: -60.0, max:    0.0, default: -18.0 },
            PluginParameter { id: 1, name: "Ratio".into(),       value:   4.0, min:   1.0, max:   20.0, default:   4.0 },
            PluginParameter { id: 2, name: "Attack".into(),      value:  10.0, min:   0.1, max:  200.0, default:  10.0 },
            PluginParameter { id: 3, name: "Release".into(),     value: 100.0, min:  10.0, max: 2000.0, default: 100.0 },
            PluginParameter { id: 4, name: "Makeup Gain".into(), value:   0.0, min:   0.0, max:   30.0, default:   0.0 },
            PluginParameter { id: 5, name: "Knee".into(),        value:   3.0, min:   0.0, max:   12.0, default:   3.0 },
            PluginParameter { id: 6, name: "Mix".into(),         value:   1.0, min:   0.0, max:    1.0, default:   1.0 },
        ],
        _ => vec![],
    }
}
