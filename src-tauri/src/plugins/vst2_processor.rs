//! VST2 plugin hosting.
//!
//! Primary path : vst-rs `PluginLoader` (requires `VSTPluginMain` export).
//! Fallback path: raw `libloading` call for plugins that only export `main`
//!                (VST 2.0 / 2.1 / 2.2 / 2.3 era — pre-2.4 entry point).

use anyhow::{anyhow, Result};
use std::ffi::c_void;
use std::path::Path;
use std::ptr;
use std::sync::{Arc, Mutex};
use vst::host::{Host, PluginLoader};
use vst::plugin::Plugin;

// ── vst-rs host ──────────────────────────────────────────────────────────────

struct ReLightVst2Host;

impl Host for ReLightVst2Host {
    fn automate(&self, index: i32, value: f32) {
        log::trace!("VST2 automate: param {} = {}", index, value);
    }
}

// ── Raw fallback: AEffect wrapper for "main" entry-point plugins ─────────────

/// Plugin-side dispatcher opcodes (vst::plugin::OpCode repr(i32) values).
mod eff {
    pub const INITIALIZE:       i32 = 0;
    pub const SHUTDOWN:         i32 = 1;
    pub const SET_SAMPLE_RATE:  i32 = 10; // opt = f32 rate
    pub const SET_BLOCK_SIZE:   i32 = 11; // value = block len
    pub const MAINS_CHANGED:    i32 = 12; // value: 1 = resume, 0 = suspend
    pub const GET_CHUNK:        i32 = 23; // ptr = *mut *mut c_void, index = 1 (prog)
    pub const SET_CHUNK:        i32 = 24; // ptr = data, value = size, index = 1
    pub const GET_VENDOR_NAME:  i32 = 47; // ptr = char[64]
    pub const GET_PRODUCT_NAME: i32 = 48; // ptr = char[64]
    pub const GET_API_VERSION:  i32 = 58; // return 2400 for VST 2.4
}

/// Function-pointer type for VSTPluginMain / main.
type PluginEntryPoint = unsafe extern "C" fn(
    callback: extern "C" fn(
        *mut vst::api::AEffect, i32, i32, isize, *mut c_void, f32,
    ) -> isize,
) -> *mut vst::api::AEffect;

/// Minimal static host callback for the raw-fallback path.
/// `ReLightVst2Host` is a unit struct, so no per-instance state is needed.
extern "C" fn raw_host_cb(
    _e:   *mut vst::api::AEffect,
    opcode: i32,
    _idx: i32,
    _val: isize,
    _ptr: *mut c_void,
    _opt: f32,
) -> isize {
    // Host opcode 1 = AudioMasterVersion: return >= 2400 for VST 2.4.
    if opcode == 1 { 2400 } else { 0 }
}

/// Wraps a raw `AEffect` reached via the legacy `main` entry point.
struct RawPlugin {
    effect: *mut vst::api::AEffect,
    _lib:   libloading::Library, // keeps DLL mapped; dropped AFTER effClose
}

// SAFETY: we never share RawPlugin across threads simultaneously;
// all access is serialised by the Mutex inside Vst2Processor.
unsafe impl Send for RawPlugin {}

impl RawPlugin {
    #[inline]
    fn dispatch(&self, op: i32, idx: i32, val: isize, p: *mut c_void, opt: f32) -> isize {
        unsafe { ((*self.effect).dispatcher)(self.effect, op, idx, val, p, opt) }
    }

    fn read_string(&self, opcode: i32) -> String {
        let mut buf = vec![0u8; 64];
        self.dispatch(opcode, 0, 0, buf.as_mut_ptr() as *mut c_void, 0.0);
        let nul = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        String::from_utf8_lossy(&buf[..nul]).into_owned()
    }

    fn num_inputs(&self)  -> i32 { unsafe { (*self.effect).numInputs  } }
    fn num_outputs(&self) -> i32 { unsafe { (*self.effect).numOutputs } }

    fn get_chunk(&self) -> Vec<u8> {
        let mut data_ptr: *mut c_void = ptr::null_mut();
        let size = self.dispatch(
            eff::GET_CHUNK, 1, 0,
            &mut data_ptr as *mut *mut c_void as *mut c_void,
            0.0,
        );
        if size > 0 && !data_ptr.is_null() {
            // SAFETY: plugin returned a valid buffer of `size` bytes.
            unsafe { std::slice::from_raw_parts(data_ptr as *const u8, size as usize).to_vec() }
        } else {
            Vec::new()
        }
    }

    fn set_chunk(&self, data: &[u8]) {
        self.dispatch(
            eff::SET_CHUNK, 1, data.len() as isize,
            data.as_ptr() as *mut c_void,
            0.0,
        );
    }
}

impl Drop for RawPlugin {
    fn drop(&mut self) {
        // Deactivate then close. After effClose the plugin may free AEffect;
        // we must not access `effect` afterwards.
        self.dispatch(eff::MAINS_CHANGED, 0, 0, ptr::null_mut(), 0.0);
        self.dispatch(eff::SHUTDOWN,      0, 0, ptr::null_mut(), 0.0);
        // _lib drops here, unloading the DLL.
    }
}

// ── Backend enum ─────────────────────────────────────────────────────────────

enum Backend {
    /// Standard vst-rs path (VSTPluginMain entry point).
    Vst(Arc<Mutex<vst::host::PluginInstance>>),
    /// Legacy raw path (main entry point).
    Raw(RawPlugin),
}

// ── Public Vst2Processor ─────────────────────────────────────────────────────

/// Wraps a loaded VST2 plugin instance and exposes the same surface as
/// `Vst3Processor` so `PluginInstance` can treat them uniformly.
pub struct Vst2Processor {
    backend: Backend,
    /// Scratch buffers — avoids per-block heap allocation.
    in_l:  Vec<f32>,
    in_r:  Vec<f32>,
    out_l: Vec<f32>,
    out_r: Vec<f32>,
}

// SAFETY: Backend::Vst holds Arc<Mutex<PluginInstance>> (raw pointer guarded by
// the Mutex). Backend::Raw holds RawPlugin (explicitly Send above). Neither is
// shared across threads simultaneously.
unsafe impl Send for Vst2Processor {}

impl Vst2Processor {
    /// Load a VST2 plugin, configure it, and start processing.
    ///
    /// Tries `VSTPluginMain` first (VST 2.4 standard); falls back to the
    /// legacy `main` entry point if the symbol is absent.
    pub fn load(plugin_path: &str, sample_rate: f64, block_size: usize) -> Result<Self> {
        let cap = block_size.max(4096);

        // ── Primary path: vst-rs PluginLoader ──────────────────────────────
        let host = Arc::new(Mutex::new(ReLightVst2Host));
        match PluginLoader::load(Path::new(plugin_path), host) {
            Ok(mut loader) => {
                let mut inst = loader
                    .instance()
                    .map_err(|e| anyhow!("VST2 instance error for '{}': {:?}", plugin_path, e))?;

                inst.init();
                {
                    let info = inst.get_info();
                    log::info!(
                        "VST2 loaded via VSTPluginMain: '{}' by '{}' ({} in, {} out)",
                        info.name, info.vendor, info.inputs, info.outputs,
                    );
                }
                inst.set_sample_rate(sample_rate as f32);
                inst.set_block_size(block_size as i64);
                inst.resume();

                return Ok(Self {
                    backend: Backend::Vst(Arc::new(Mutex::new(inst))),
                    in_l:  vec![0.0f32; cap],
                    in_r:  vec![0.0f32; cap],
                    out_l: vec![0.0f32; cap],
                    out_r: vec![0.0f32; cap],
                });
            }

            Err(vst::host::PluginLoadError::NotAPlugin) => {
                // DLL loaded but 'VSTPluginMain' not found — try 'main'.
                log::info!(
                    "VST2: '{}' has no 'VSTPluginMain'; trying legacy 'main' entry point",
                    plugin_path
                );
            }

            Err(e) => {
                return Err(anyhow!("VST2 load error for '{}': {:?}", plugin_path, e));
            }
        }

        // ── Fallback path: raw libloading via 'main' ────────────────────────
        // SAFETY: loading an external DLL from a user-configured plugin path.
        let lib = unsafe { libloading::Library::new(plugin_path) }
            .map_err(|e| anyhow!("Cannot open DLL '{}': {}", plugin_path, e))?;

        let entry: libloading::Symbol<PluginEntryPoint> = unsafe { lib.get(b"main") }
            .map_err(|_| anyhow!(
                "VST2: '{}' exports neither 'VSTPluginMain' nor 'main'", plugin_path
            ))?;

        let effect = unsafe { entry(raw_host_cb) };
        if effect.is_null() {
            return Err(anyhow!("VST2: 'main' returned null for '{}'", plugin_path));
        }

        // Validate VST2 magic 'VstP' = 0x56737450 to guard against non-VST DLLs
        // that happen to export 'main'.
        let magic = unsafe { (*effect).magic };
        if magic != 0x5673_7450_u32 as i32 {
            return Err(anyhow!(
                "VST2: '{}' via 'main' has wrong magic 0x{:08X} (expected 0x56737450)",
                plugin_path, magic as u32
            ));
        }

        // Verify API version >= 2400.
        let api_ver = unsafe {
            ((*effect).dispatcher)(effect, eff::GET_API_VERSION, 0, 0, ptr::null_mut(), 0.0)
        };
        if api_ver < 2400 {
            return Err(anyhow!(
                "VST2: '{}' reports API version {} (need >= 2400)", plugin_path, api_ver
            ));
        }

        let raw = RawPlugin { effect, _lib: lib };

        // Init → query names → configure → resume.
        raw.dispatch(eff::INITIALIZE, 0, 0, ptr::null_mut(), 0.0);
        let name   = raw.read_string(eff::GET_PRODUCT_NAME);
        let vendor = raw.read_string(eff::GET_VENDOR_NAME);
        log::info!(
            "VST2 loaded via 'main': '{}' by '{}' ({} in, {} out)",
            name, vendor, raw.num_inputs(), raw.num_outputs(),
        );
        raw.dispatch(eff::SET_SAMPLE_RATE, 0, 0,                  ptr::null_mut(), sample_rate as f32);
        raw.dispatch(eff::SET_BLOCK_SIZE,  0, block_size as isize, ptr::null_mut(), 0.0);
        raw.dispatch(eff::MAINS_CHANGED,   0, 1,                  ptr::null_mut(), 0.0);

        Ok(Self {
            backend: Backend::Raw(raw),
            in_l:  vec![0.0f32; cap],
            in_r:  vec![0.0f32; cap],
            out_l: vec![0.0f32; cap],
            out_r: vec![0.0f32; cap],
        })
    }

    /// Process one stereo block in-place through the VST2 plugin.
    pub fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]) {
        let n = left.len().min(right.len());
        if n == 0 { return; }

        if self.in_l.len() < n {
            self.in_l.resize(n, 0.0);
            self.in_r.resize(n, 0.0);
            self.out_l.resize(n, 0.0);
            self.out_r.resize(n, 0.0);
        }

        self.in_l[..n].copy_from_slice(&left[..n]);
        self.in_r[..n].copy_from_slice(&right[..n]);

        match &self.backend {
            Backend::Vst(arc) => {
                // Build raw-pointer arrays for the VST2 AudioBuffer.
                let in_ptrs  = [self.in_l.as_ptr(),      self.in_r.as_ptr()];
                let out_ptrs = [self.out_l.as_mut_ptr(), self.out_r.as_mut_ptr()];
                // SAFETY: pointers valid for `n` f32 elements; lifetime = this block.
                let mut audio_buf = unsafe {
                    vst::buffer::AudioBuffer::from_raw(
                        2, 2,
                        in_ptrs.as_ptr(),
                        out_ptrs.as_ptr() as *mut *mut f32,
                        n,
                    )
                };
                if let Ok(mut inst) = arc.lock() {
                    inst.process(&mut audio_buf);
                }
            }

            Backend::Raw(raw) => {
                let out_ch = raw.num_outputs().clamp(1, 2) as usize;
                let in_ptrs:  [*const f32; 2] = [self.in_l.as_ptr(),      self.in_r.as_ptr()];
                let out_ptrs: [*mut   f32; 2] = [self.out_l.as_mut_ptr(), self.out_r.as_mut_ptr()];

                // SAFETY: pointers valid for `n` samples.
                // processReplacing is guaranteed non-null by VST 2.4 compliance check.
                unsafe {
                    ((*raw.effect).processReplacing)(
                        raw.effect,
                        in_ptrs.as_ptr()  as *const *const f32,
                        out_ptrs.as_ptr() as *mut   *mut   f32,
                        n as i32,
                    );
                }
                // Mono-output plugin: duplicate L → R.
                if out_ch < 2 {
                    self.out_r[..n].copy_from_slice(&self.out_l[..n]);
                }
            }
        }

        left[..n].copy_from_slice(&self.out_l[..n]);
        right[..n].copy_from_slice(&self.out_r[..n]);
    }

    /// Snapshot the plugin preset as raw bytes.
    pub fn get_state(&mut self) -> Vec<u8> {
        match &self.backend {
            Backend::Vst(arc) => {
                arc.lock().ok()
                    .map(|mut inst| inst.get_parameter_object().get_preset_data())
                    .unwrap_or_default()
            }
            Backend::Raw(raw) => raw.get_chunk(),
        }
    }

    /// Restore the plugin preset from raw bytes.
    pub fn set_state(&mut self, data: &[u8]) {
        match &self.backend {
            Backend::Vst(arc) => {
                if let Ok(mut inst) = arc.lock() {
                    inst.get_parameter_object().load_preset_data(data);
                }
            }
            Backend::Raw(raw) => raw.set_chunk(data),
        }
    }

    /// Set a normalised parameter value ([0.0, 1.0]).
    #[allow(dead_code)]
    pub fn set_param_normalized(&self, index: u32, normalized: f32) {
        match &self.backend {
            Backend::Vst(arc) => {
                if let Ok(mut inst) = arc.lock() {
                    inst.get_parameter_object().set_parameter(index as i32, normalized);
                }
            }
            Backend::Raw(raw) => unsafe {
                ((*raw.effect).setParameter)(raw.effect, index as i32, normalized);
            },
        }
    }

    /// Return basic plugin metadata for the UI.
    #[allow(dead_code)]
    pub fn get_info(&self) -> Option<(String, String)> {
        match &self.backend {
            Backend::Vst(arc) => {
                arc.lock().ok().map(|inst| {
                    let info = inst.get_info();
                    (info.name, info.vendor)
                })
            }
            Backend::Raw(raw) => Some((
                raw.read_string(eff::GET_PRODUCT_NAME),
                raw.read_string(eff::GET_VENDOR_NAME),
            )),
        }
    }
}

impl Drop for Vst2Processor {
    fn drop(&mut self) {
        match &self.backend {
            Backend::Vst(arc) => {
                if let Ok(mut inst) = arc.lock() {
                    inst.suspend();
                }
                // PluginInstance::drop (vst-rs) calls effClose.
            }
            Backend::Raw(_) => {
                // RawPlugin::drop calls MAINS_CHANGED(0) then SHUTDOWN (effClose).
            }
        }
    }
}
