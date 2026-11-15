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
use std::sync::atomic::{AtomicU32, Ordering};

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
    pub const EDIT_IDLE:         i32 = 19;
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

/// Sample rate / block size reported through the host callback. Process-wide
/// rather than per `AEffect`: every plugin in the chain runs at the same
/// rate and block size.
static HOST_SAMPLE_RATE: AtomicU32 = AtomicU32::new(48_000);
static HOST_BLOCK_SIZE: AtomicU32 = AtomicU32::new(512);

fn set_host_audio_config(sample_rate: f64, block_size: usize) {
    HOST_SAMPLE_RATE.store(sample_rate as u32, Ordering::Relaxed);
    HOST_BLOCK_SIZE.store(block_size as u32, Ordering::Relaxed);
}

/// Minimal host callback: `audioMasterVersion` (1) confirms VST 2.4 support;
/// `audioMasterGetSampleRate` (16) / `audioMasterGetBlockSize` (17) are
/// answered because some plugins read them instead of waiting for
/// `effSetSampleRate`. Every other opcode can safely return 0.
extern "C" fn host_callback(
    _effect: *mut AEffect, opcode: i32, _index: i32, _value: isize, _ptr: *mut c_void, _opt: f32,
) -> isize {
    match opcode {
        1 => 2400,
        16 => HOST_SAMPLE_RATE.load(Ordering::Relaxed) as isize,
        17 => HOST_BLOCK_SIZE.load(Ordering::Relaxed) as isize,
        _ => 0,
    }
}

/// Owns a loaded plugin's `AEffect*` and the DLL keeping it mapped.
pub(crate) struct RawPlugin {
    effect: *mut AEffect,
    _lib: libloading::Library, // keeps DLL mapped; dropped AFTER effClose
    editor_open: bool,
}

// SAFETY: all access to `effect` is serialised by the Mutex<RawPlugin> in
// Vst2Processor — the audio thread and the plugin host thread (editor, state)
// never touch it at once. `EditorIdle` is the documented exception.
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

    /// Handle for `effEditIdle` calls from the editor's idle timer.
    pub(crate) fn editor_idle_handle(&self) -> EditorIdle {
        EditorIdle(self.effect)
    }

    pub(crate) fn editor_close(&mut self) {
        if self.editor_open {
            self.dispatch(eff::EDIT_CLOSE, 0, 0, ptr::null_mut(), 0.0);
            self.editor_open = false;
        }
    }
}

/// `effEditIdle` sender for the editor's idle timer (plugin host thread).
/// Called without the plugin mutex on purpose: the audio thread `try_lock`s
/// that mutex every block, and holding it for a repaint would drop blocks to
/// dry audio. Plugins expect editor calls concurrently with processing (as in
/// every VST2 host); the editor window closes before the plugin is dropped.
pub(crate) struct EditorIdle(pub(crate) *mut AEffect);

unsafe impl Send for EditorIdle {}

impl EditorIdle {
    pub(crate) fn idle(&self) {
        unsafe { ((*self.0).dispatcher)(self.0, eff::EDIT_IDLE, 0, 0, ptr::null_mut(), 0.0) };
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
    /// Shared with the editor window (see `gui::vst2`) so editor open/close
    /// never races a concurrent `process_stereo` call.
    plugin: Arc<Mutex<RawPlugin>>,
    /// Scratch buffers — avoids per-block heap allocation.
    in_l:  Vec<f32>,
    in_r:  Vec<f32>,
    out_l: Vec<f32>,
    out_r: Vec<f32>,
    /// Channels past the first two: inputs read `zeros`, outputs write
    /// `discard`. One pointer per channel the plugin declares
    /// (max(numInputs, numOutputs, 2)), filled per chunk — pre-allocated
    /// so the audio thread never allocates.
    zeros:   Vec<f32>,
    discard: Vec<f32>,
    in_ptrs:  Vec<*const f32>,
    out_ptrs: Vec<*mut f32>,
    /// Block size negotiated via `effSetBlockSize` at load(). The real-time
    /// callback's actual block size can exceed this at runtime (see
    /// audio::manager's `BufferSize::Default`), so `process_stereo` chunks
    /// to this size instead of passing the raw block through — same fix
    /// already applied to vst3.rs for the identical host-contract issue.
    max_block: usize,
}

// SAFETY: the pointer vecs only ever point into this struct's own buffers
// and are rewritten before each use; the plugin itself is behind `plugin`.
unsafe impl Send for Vst2Processor {}

impl Vst2Processor {
    /// Load a VST2 plugin, configure it, and start processing.
    ///
    /// Tries `VSTPluginMain` first (VST 2.4 standard); falls back to the
    /// legacy `main` entry point (pre-2.4 plugins) if the symbol is absent.
    pub fn load(plugin_path: &str, sample_rate: f64, block_size: usize) -> Result<Self> {
        let cap = block_size.max(4096);
        set_host_audio_config(sample_rate, block_size);

        // DLL load + entry point run under the shared library-load lock (see
        // plugins::core::LIBRARY_LOAD_LOCK); effOpen and later run outside it.
        let load_guard = crate::plugins::core::LIBRARY_LOAD_LOCK.lock();
        // SAFETY: loading an external DLL from a user-configured plugin path.
        let lib = unsafe { libloading::Library::new(plugin_path) }
            .map_err(|e| anyhow!("Cannot open DLL '{}': {}", plugin_path, e))?;

        let entry: libloading::Symbol<PluginEntryPoint> = unsafe {
            lib.get(b"VSTPluginMain\0").or_else(|_| lib.get(b"main\0"))
        }.map_err(|_| anyhow!(
            "VST2: '{}' exports neither 'VSTPluginMain' nor 'main'", plugin_path
        ))?;

        let effect = unsafe { entry(host_callback) };
        drop(load_guard);
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

        Ok(Self::new(raw, cap, block_size))
    }

    fn new(raw: RawPlugin, cap: usize, block_size: usize) -> Self {
        let channels = raw.num_inputs().max(raw.num_outputs()).max(2) as usize;
        Self {
            plugin: Arc::new(Mutex::new(raw)),
            in_l:  vec![0.0f32; cap],
            in_r:  vec![0.0f32; cap],
            out_l: vec![0.0f32; cap],
            out_r: vec![0.0f32; cap],
            zeros:   vec![0.0f32; cap],
            discard: vec![0.0f32; cap],
            in_ptrs:  vec![ptr::null(); channels],
            out_ptrs: vec![ptr::null_mut(); channels],
            max_block: block_size.max(1),
        }
    }

    /// Process one stereo block in-place through the VST2 plugin.
    pub fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]) {
        let total = left.len().min(right.len());
        if total == 0 { return; }

        let mut offset = 0;
        while offset < total {
            let n = (total - offset).min(self.max_block);
            self.process_chunk(&mut left[offset..offset + n], &mut right[offset..offset + n]);
            offset += n;
        }
    }

    fn process_chunk(&mut self, left: &mut [f32], right: &mut [f32]) {
        let n = left.len().min(right.len());
        if n == 0 { return; }

        if self.in_l.len() < n {
            self.in_l.resize(n, 0.0);
            self.in_r.resize(n, 0.0);
            self.out_l.resize(n, 0.0);
            self.out_r.resize(n, 0.0);
            self.zeros.resize(n, 0.0);
            self.discard.resize(n, 0.0);
        }

        self.in_l[..n].copy_from_slice(&left[..n]);
        self.in_r[..n].copy_from_slice(&right[..n]);

        // try_lock: non-blocking so the audio callback never stalls. If an
        // editor call or a state save/restore holds the lock, pass this chunk
        // through dry — copying out_l/out_r here would replay the previous
        // chunk's output (an audible repeat).
        let Ok(plugin) = self.plugin.try_lock() else { return };
        {
            let out_ch = plugin.num_outputs().clamp(1, 2) as usize;
            if self.in_ptrs.len() > 2 {
                self.zeros[..n].fill(0.0); // a plugin may have written to it
            }
            for (i, p) in self.in_ptrs.iter_mut().enumerate() {
                *p = match i { 0 => self.in_l.as_ptr(), 1 => self.in_r.as_ptr(), _ => self.zeros.as_ptr() };
            }
            for (i, p) in self.out_ptrs.iter_mut().enumerate() {
                *p = match i { 0 => self.out_l.as_mut_ptr(), 1 => self.out_r.as_mut_ptr(), _ => self.discard.as_mut_ptr() };
            }

            // SAFETY: one pointer per declared channel, each valid for `n`
            // samples; processReplacing is guaranteed non-null by the
            // VST 2.4 API-version check in load().
            unsafe {
                ((*plugin.effect).process_replacing)(
                    plugin.effect,
                    self.in_ptrs.as_ptr(),
                    self.out_ptrs.as_mut_ptr(),
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

    /// Latency the plugin reports (`AEffect::initialDelay`).
    pub fn latency_samples(&self) -> u32 {
        self.plugin.lock().map(|p| unsafe { (*p.effect).initial_delay.max(0) as u32 }).unwrap_or(0)
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

    /// The plugin, for opening its editor (gui::vst2) without holding the
    /// `Vst2Processor` mutex meanwhile.
    pub(crate) fn editor_handle(&self) -> Arc<Mutex<RawPlugin>> {
        Arc::clone(&self.plugin)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_callback_reports_current_sample_rate_and_block_size() {
        set_host_audio_config(44_100.0, 256);
        assert_eq!(host_callback(ptr::null_mut(), 16, 0, 0, ptr::null_mut(), 0.0), 44_100);
        assert_eq!(host_callback(ptr::null_mut(), 17, 0, 0, ptr::null_mut(), 0.0), 256);
        assert_eq!(host_callback(ptr::null_mut(), 1, 0, 0, ptr::null_mut(), 0.0), 2400);
    }
}

#[cfg(test)]
mod idle_tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize};

    static LAST_OPCODE: AtomicI32 = AtomicI32::new(-1);
    unsafe extern "C" fn dispatcher(_: *mut AEffect, op: i32, _: i32, _: isize, _: *mut c_void, _: f32) -> isize {
        LAST_OPCODE.store(op, Ordering::SeqCst);
        0
    }
    unsafe extern "C" fn process(_: *mut AEffect, _: *const *const f32, _: *mut *mut f32, _: i32) {}
    unsafe extern "C" fn process_f64(_: *mut AEffect, _: *const *const f64, _: *mut *mut f64, _: i32) {}
    unsafe extern "C" fn set_param(_: *mut AEffect, _: i32, _: f32) {}
    unsafe extern "C" fn get_param(_: *mut AEffect, _: i32) -> f32 { 0.0 }

    /// 4-in/4-out plugin: copies every input to its output, so it touches
    /// all four pointers on both sides.
    unsafe extern "C" fn quad_copy(_: *mut AEffect, ins: *const *const f32, outs: *mut *mut f32, n: i32) {
        for ch in 0..4 {
            std::ptr::copy_nonoverlapping(*ins.add(ch), *outs.add(ch), n as usize);
        }
    }

    /// Doesn't touch LAST_OPCODE (the idle test runs in parallel).
    unsafe extern "C" fn quiet(_: *mut AEffect, _: i32, _: i32, _: isize, _: *mut c_void, _: f32) -> isize { 0 }

    #[cfg(target_os = "windows")]
    #[test]
    fn plugin_with_more_than_two_channels_gets_a_valid_pointer_for_each() {
        let effect = Box::into_raw(Box::new(AEffect {
            magic: VST_MAGIC, dispatcher: quiet, process, set_parameter: set_param, get_parameter: get_param,
            num_programs: 0, num_params: 0, num_inputs: 4, num_outputs: 4, flags: 0,
            resvd1: 0, resvd2: 0, initial_delay: 0, real_qualities: 0, off_qualities: 0, io_ratio: 1.0,
            object: ptr::null_mut(), user: ptr::null_mut(), unique_id: 0, version: 0,
            process_replacing: quad_copy, process_double_replacing: process_f64, future: [0; 56],
        }));
        let lib: libloading::Library = libloading::os::windows::Library::this().unwrap().into();
        let mut proc = Vst2Processor::new(RawPlugin { effect, _lib: lib, editor_open: false }, 64, 64);
        let (mut l, mut r) = (vec![0.5f32; 64], vec![-0.5f32; 64]);
        proc.process_stereo(&mut l, &mut r);
        assert_eq!((l[63], r[63]), (0.5, -0.5));
        assert!(proc.zeros.iter().all(|&z| z == 0.0), "extra inputs read silence");
        drop(proc); // effClose via the fake dispatcher
        drop(unsafe { Box::from_raw(effect) });
    }

    /// Editor opcodes seen by `recording`: (opcode, thread, host window alive).
    static EDITOR_CALLS: parking_lot::Mutex<Vec<(i32, std::thread::ThreadId, bool)>> = parking_lot::Mutex::new(Vec::new());
    static PARENT: AtomicIsize = AtomicIsize::new(0);

    #[cfg(target_os = "windows")]
    unsafe extern "C" fn recording(_: *mut AEffect, op: i32, _: i32, _: isize, p: *mut c_void, _: f32) -> isize {
        use windows_sys::Win32::UI::WindowsAndMessaging::IsWindow;
        if op == eff::EDIT_OPEN {
            PARENT.store(p as isize, Ordering::SeqCst);
        }
        if matches!(op, eff::EDIT_OPEN | eff::EDIT_IDLE | eff::EDIT_CLOSE | eff::EDIT_GET_RECT) {
            let alive = IsWindow(PARENT.load(Ordering::SeqCst) as _) != 0;
            EDITOR_CALLS.lock().push((op, std::thread::current().id(), alive));
        }
        (op == eff::EDIT_OPEN) as isize
    }

    /// JUCE binds its message thread to the thread that created the plugin
    /// (the plugin host thread), so the editor must be driven from there.
    #[cfg(target_os = "windows")]
    #[test]
    fn editor_runs_on_the_host_thread_and_closes_before_its_window() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};
        use crate::core::host_thread::{run_blocking, wait_a_moment};
        let effect = Box::into_raw(Box::new(AEffect {
            magic: VST_MAGIC, dispatcher: recording, process, set_parameter: set_param, get_parameter: get_param,
            num_programs: 0, num_params: 0, num_inputs: 2, num_outputs: 2, flags: EFF_FLAGS_HAS_EDITOR,
            resvd1: 0, resvd2: 0, initial_delay: 0, real_qualities: 0, off_qualities: 0, io_ratio: 1.0,
            object: ptr::null_mut(), user: ptr::null_mut(), unique_id: 0, version: 0,
            process_replacing: process, process_double_replacing: process_f64, future: [0; 56],
        }));
        let lib: libloading::Library = libloading::os::windows::Library::this().unwrap().into();
        let plugin = Arc::new(Mutex::new(RawPlugin { effect, _lib: lib, editor_open: false }));
        let flag = Arc::new(AtomicBool::new(true));
        let hwnd = Arc::new(AtomicIsize::new(0));

        crate::plugins::gui::vst2::open_vst2_gui(Arc::clone(&plugin), "fake", Arc::clone(&flag), Arc::clone(&hwnd)).unwrap();
        let host = run_blocking(|| std::thread::current().id());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !EDITOR_CALLS.lock().iter().any(|c| c.0 == eff::EDIT_IDLE) && std::time::Instant::now() < deadline {
            run_blocking(wait_a_moment); // the host pump delivers the idle timer
        }
        let win = hwnd.load(Ordering::Acquire);
        assert_ne!(win, 0, "host window published");
        unsafe { PostMessageW(win as _, WM_CLOSE, 0, 0) };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while flag.load(Ordering::Acquire) && std::time::Instant::now() < deadline {
            run_blocking(wait_a_moment);
        }

        let calls = EDITOR_CALLS.lock().clone();
        let ops: Vec<i32> = calls.iter().map(|c| c.0).collect();
        assert!(ops.contains(&eff::EDIT_OPEN) && ops.contains(&eff::EDIT_IDLE), "{ops:?}");
        assert_eq!(ops.last(), Some(&eff::EDIT_CLOSE), "{ops:?}");
        assert!(calls.iter().all(|c| c.1 == host), "every editor call on the plugin host thread: {calls:?}");
        assert!(calls.last().unwrap().2, "effEditClose while the host window still exists");
        assert!(!flag.load(Ordering::Acquire), "gui flag cleared");
        assert_eq!(hwnd.load(Ordering::Acquire), 0);
        drop(plugin); // effClose via the fake dispatcher
        drop(unsafe { Box::from_raw(effect) });
    }

    #[test]
    fn editor_idle_sends_eff_edit_idle() {
        let mut effect = AEffect {
            magic: VST_MAGIC, dispatcher, process, set_parameter: set_param, get_parameter: get_param,
            num_programs: 0, num_params: 0, num_inputs: 2, num_outputs: 2, flags: 0,
            resvd1: 0, resvd2: 0, initial_delay: 0, real_qualities: 0, off_qualities: 0, io_ratio: 1.0,
            object: ptr::null_mut(), user: ptr::null_mut(), unique_id: 0, version: 0,
            process_replacing: process, process_double_replacing: process_f64, future: [0; 56],
        };
        EditorIdle(&mut effect).idle();
        assert_eq!(LAST_OPCODE.load(Ordering::SeqCst), 19, "effEditIdle");
    }
}
