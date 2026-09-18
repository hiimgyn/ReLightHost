//! VST2 plugin hosting — raw FFI, no external crate.
//!
//! The `vst` crate (vst-rs) is deprecated upstream: Steinberg discontinued
//! VST2 licensing in 2018 and the crate is no longer maintained. VST 2.4's
//! ABI is small and has been frozen since ~2006 (`AEffect` C struct +
//! opcode-based dispatcher), so — mirroring how `clap.rs` already hosts CLAP
//! without a dependency — we talk to it directly via `libloading`.
//!
//! Lifecycle:
//!   1. dlopen() the DLL, resolve `VSTPluginMain` (2.4) or legacy `main`.
//!   2. entry(host_callback) → `AEffect*`; validate magic + API version.
//!   3. effOpen → effSetSampleRate/effSetBlockSize → effMainsChanged(1).
//!   4. processReplacing() per audio block.
//!   5. Drop: effEditClose (if open) → effMainsChanged(0) → effClose.

use anyhow::{anyhow, Result};
use std::ffi::c_void;
use std::ptr;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicIsize};

// ── Raw VST 2.4 ABI ──────────────────────────────────────────────────────────

const VST_MAGIC: i32 = 0x5673_7450_u32 as i32; // 'VstP'
const EFF_FLAGS_HAS_EDITOR: i32 = 1 << 0;

/// Layout fixed by the VST 2.4 SDK (`aeffect.h`). Field names are
/// snake_case here; the ABI only cares about offsets/types.
#[repr(C)]
pub(crate) struct AEffect {
    magic: i32,
    dispatcher: unsafe extern "C" fn(*mut AEffect, i32, i32, isize, *mut c_void, f32) -> isize,
    process: unsafe extern "C" fn(*mut AEffect, *const *const f32, *mut *mut f32, i32),
    set_parameter: unsafe extern "C" fn(*mut AEffect, i32, f32),
    get_parameter: unsafe extern "C" fn(*mut AEffect, i32) -> f32,
    num_programs: i32,
    num_params: i32,
    num_inputs: i32,
    num_outputs: i32,
    flags: i32,
    resvd1: isize,
    resvd2: isize,
    initial_delay: i32,
    real_qualities: i32,
    off_qualities: i32,
    io_ratio: f32,
    object: *mut c_void,
    user: *mut c_void,
    unique_id: i32,
    version: i32,
    process_replacing: unsafe extern "C" fn(*mut AEffect, *const *const f32, *mut *mut f32, i32),
    process_double_replacing: unsafe extern "C" fn(*mut AEffect, *const *const f64, *mut *mut f64, i32),
    future: [u8; 56],
}

/// `ERect` as filled in by `effEditGetRect` (top/left/bottom/right, 16-bit).
#[repr(C)]
struct ERect { top: i16, left: i16, bottom: i16, right: i16 }

/// Dispatcher opcodes actually used by this host (VST 2.4 spec).
mod eff {
    pub const OPEN:              i32 = 0;
    pub const CLOSE:             i32 = 1;
    pub const SET_SAMPLE_RATE:   i32 = 10;
    pub const SET_BLOCK_SIZE:    i32 = 11;
    pub const MAINS_CHANGED:     i32 = 12; // value: 1 = resume, 0 = suspend
    pub const EDIT_GET_RECT:     i32 = 13; // ptr = *mut *mut ERect (out)
    pub const EDIT_OPEN:         i32 = 14; // ptr = parent HWND
    pub const EDIT_CLOSE:        i32 = 15;
    pub const GET_CHUNK:         i32 = 23; // ptr = *mut *mut c_void, index = 1 (prog)
    pub const SET_CHUNK:         i32 = 24; // ptr = data, value = size, index = 1
    pub const GET_VENDOR_NAME:   i32 = 47; // ptr = char[64]
    pub const GET_PRODUCT_NAME:  i32 = 48; // ptr = char[64]
    pub const GET_VST_VERSION:   i32 = 58; // return 2400 for VST 2.4
}

/// Function-pointer type for `VSTPluginMain` / legacy `main`.
type PluginEntryPoint = unsafe extern "C" fn(
    callback: extern "C" fn(*mut AEffect, i32, i32, isize, *mut c_void, f32) -> isize,
) -> *mut AEffect;

/// Minimal host callback: plugins query `audioMasterVersion` (opcode 1) during
/// init to confirm VST 2.4 support; every other opcode this host doesn't act
/// on can safely return 0.
extern "C" fn host_callback(
    _effect: *mut AEffect, opcode: i32, _index: i32, _value: isize, _ptr: *mut c_void, _opt: f32,
) -> isize {
    if opcode == 1 { 2400 } else { 0 }
}

/// Owns a loaded plugin's `AEffect*` and the DLL keeping it mapped.
pub(crate) struct RawPlugin {
    effect: *mut AEffect,
    _lib: libloading::Library, // keeps DLL mapped; dropped AFTER effClose
    editor_open: bool,
}

// SAFETY: all access to `effect` is serialised by the Mutex<RawPlugin> in
// Vst2Processor — the audio thread and the GUI thread never touch it at once.
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

    fn num_inputs(&self)  -> i32 { unsafe { (*self.effect).num_inputs  } }
    fn num_outputs(&self) -> i32 { unsafe { (*self.effect).num_outputs } }
    pub(crate) fn has_editor(&self) -> bool { unsafe { (*self.effect).flags & EFF_FLAGS_HAS_EDITOR != 0 } }

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

    /// Editor size via `effEditGetRect`. (0, 0) if unavailable.
    pub(crate) fn editor_size(&self) -> (i32, i32) {
        let mut rect_ptr: *mut ERect = ptr::null_mut();
        let ok = self.dispatch(
            eff::EDIT_GET_RECT, 0, 0,
            &mut rect_ptr as *mut *mut ERect as *mut c_void,
            0.0,
        );
        if ok != 0 && !rect_ptr.is_null() {
            // SAFETY: plugin-owned ERect, valid for the duration of this call.
            let r = unsafe { &*rect_ptr };
            ((r.right - r.left) as i32, (r.bottom - r.top) as i32)
        } else {
            (0, 0)
        }
    }

    /// `effEditOpen` — `parent` is a native window handle (HWND on Windows).
    pub(crate) fn editor_open(&mut self, parent: *mut c_void) -> bool {
        let ok = self.dispatch(eff::EDIT_OPEN, 0, 0, parent, 0.0) != 0;
        self.editor_open = ok;
        ok
    }

    pub(crate) fn editor_close(&mut self) {
        if self.editor_open {
            self.dispatch(eff::EDIT_CLOSE, 0, 0, ptr::null_mut(), 0.0);
            self.editor_open = false;
        }
    }
}

impl Drop for RawPlugin {
    fn drop(&mut self) {
        // Close the editor before shutdown to avoid use-after-free if the
        // plugin frees editor-related state during effClose.
        self.editor_close();
        self.dispatch(eff::MAINS_CHANGED, 0, 0, ptr::null_mut(), 0.0);
        self.dispatch(eff::CLOSE, 0, 0, ptr::null_mut(), 0.0);
        // _lib drops here, unloading the DLL.
    }
}

// ── Public Vst2Processor ─────────────────────────────────────────────────────

/// Wraps a loaded VST2 plugin instance and exposes the same surface as
/// `Vst3Processor` so `PluginInstance` can treat them uniformly.
pub struct Vst2Processor {
    /// Shared with the GUI thread (see `gui::vst2::open_vst2_gui`) so editor
    /// open/close never races a concurrent `process_stereo` call.
    plugin: Arc<Mutex<RawPlugin>>,
    /// Scratch buffers — avoids per-block heap allocation.
    in_l:  Vec<f32>,
    in_r:  Vec<f32>,
    out_l: Vec<f32>,
    out_r: Vec<f32>,
}

impl Vst2Processor {
    /// Load a VST2 plugin, configure it, and start processing.
    ///
    /// Tries `VSTPluginMain` first (VST 2.4 standard); falls back to the
    /// legacy `main` entry point (pre-2.4 plugins) if the symbol is absent.
    pub fn load(plugin_path: &str, sample_rate: f64, block_size: usize) -> Result<Self> {
        let cap = block_size.max(4096);

        // SAFETY: loading an external DLL from a user-configured plugin path.
        let lib = unsafe { libloading::Library::new(plugin_path) }
            .map_err(|e| anyhow!("Cannot open DLL '{}': {}", plugin_path, e))?;

        let entry: libloading::Symbol<PluginEntryPoint> = unsafe {
            lib.get(b"VSTPluginMain\0").or_else(|_| lib.get(b"main\0"))
        }.map_err(|_| anyhow!(
            "VST2: '{}' exports neither 'VSTPluginMain' nor 'main'", plugin_path
        ))?;

        let effect = unsafe { entry(host_callback) };
        if effect.is_null() {
            return Err(anyhow!("VST2: entry point returned null for '{}'", plugin_path));
        }

        // Validate VST2 magic 'VstP' to guard against non-VST DLLs that
        // happen to export a matching entry-point symbol name.
        let magic = unsafe { (*effect).magic };
        if magic != VST_MAGIC {
            return Err(anyhow!(
                "VST2: '{}' has wrong magic 0x{:08X} (expected 0x56737450)",
                plugin_path, magic as u32
            ));
        }

        let raw = RawPlugin { effect, _lib: lib, editor_open: false };

        let api_ver = raw.dispatch(eff::GET_VST_VERSION, 0, 0, ptr::null_mut(), 0.0);
        if api_ver < 2400 {
            return Err(anyhow!(
                "VST2: '{}' reports API version {} (need >= 2400)", plugin_path, api_ver
            ));
        }

        // Init → query names → configure → resume.
        raw.dispatch(eff::OPEN, 0, 0, ptr::null_mut(), 0.0);
        let name   = raw.read_string(eff::GET_PRODUCT_NAME);
        let vendor = raw.read_string(eff::GET_VENDOR_NAME);
        log::info!(
            "VST2 loaded: '{}' by '{}' ({} in, {} out)",
            name, vendor, raw.num_inputs(), raw.num_outputs(),
        );
        raw.dispatch(eff::SET_SAMPLE_RATE, 0, 0,                  ptr::null_mut(), sample_rate as f32);
        raw.dispatch(eff::SET_BLOCK_SIZE,  0, block_size as isize, ptr::null_mut(), 0.0);
        raw.dispatch(eff::MAINS_CHANGED,   0, 1,                  ptr::null_mut(), 0.0);

        Ok(Self {
            plugin: Arc::new(Mutex::new(raw)),
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

        // try_lock: non-blocking so the audio callback never stalls. If the
        // GUI thread or a state save/restore holds the lock, this block is
        // skipped (out_l/out_r keep whatever they held from the last
        // successful process() call, same as before this rewrite).
        if let Ok(plugin) = self.plugin.try_lock() {
            let out_ch = plugin.num_outputs().clamp(1, 2) as usize;
            let in_ptrs:  [*const f32; 2] = [self.in_l.as_ptr(),      self.in_r.as_ptr()];
            let out_ptrs: [*mut   f32; 2] = [self.out_l.as_mut_ptr(), self.out_r.as_mut_ptr()];

            // SAFETY: pointers valid for `n` samples; processReplacing is
            // guaranteed non-null by the VST 2.4 API-version check in load().
            unsafe {
                ((*plugin.effect).process_replacing)(
                    plugin.effect,
                    in_ptrs.as_ptr(),
                    out_ptrs.as_ptr() as *mut *mut f32,
                    n as i32,
                );
            }
            if out_ch < 2 {
                // Mono-output plugin: duplicate L → R.
                self.out_r[..n].copy_from_slice(&self.out_l[..n]);
            }
        }

        left[..n].copy_from_slice(&self.out_l[..n]);
        right[..n].copy_from_slice(&self.out_r[..n]);
    }

    /// Snapshot the plugin preset as raw bytes.
    pub fn get_state(&mut self) -> Vec<u8> {
        self.plugin.lock().ok()
            .map(|p| p.get_chunk())
            .unwrap_or_default()
    }

    /// Restore the plugin preset from raw bytes.
    pub fn set_state(&mut self, data: &[u8]) {
        if let Ok(p) = self.plugin.lock() {
            p.set_chunk(data);
        }
    }

    /// Open the plugin's native editor GUI on a dedicated thread.
    /// Returns immediately; the GUI runs on its own thread.
    pub fn open_gui(
        &self,
        plugin_name: &str,
        gui_flag: Arc<AtomicBool>,
        gui_hwnd: Arc<AtomicIsize>,
    ) -> Result<()> {
        crate::plugins::gui::vst2::open_vst2_gui(
            Arc::clone(&self.plugin), plugin_name, gui_flag, gui_hwnd,
        )
    }
}
