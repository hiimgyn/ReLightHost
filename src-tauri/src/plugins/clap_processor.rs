//! CLAP plugin hosting — audio processing, state save/restore, GUI bridge.
//!
//! The CLAP (CLever Audio Plugin) API uses a single shared library with a
//! `clap_entry` symbol.  No COM — just raw C function-pointer structs.
//!
//! Lifecycle:
//!   1. dlopen() the .clap file  (libloading)
//!   2. entry.init(path)
//!   3. factory.create(host, plugin_id) → clap_plugin_t*
//!   4. plugin.init() → activate() → start_processing()
//!   5. process() per audio block  (real-time, lock-free input/output arrays)
//!   6. Drop: stop_processing → deactivate → destroy → entry.deinit
//!
//! All raw CLAP structs are defined here so no external crate is needed.

use anyhow::{anyhow, Result};
use std::ffi::{CStr, CString, c_char, c_void};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

// ── CLAP ABI constants ───────────────────────────────────────────────────────

const CLAP_PLUGIN_FACTORY_ID : &[u8] = b"clap.plugin-factory\0";
const CLAP_EXT_STATE         : &[u8] = b"clap.state\0";
const CLAP_EXT_GUI           : &[u8] = b"clap.gui\0";
pub  const CLAP_WINDOW_API_WIN32 : &[u8] = b"win32\0";

// ── Raw CLAP C ABI structs ────────────────────────────────────────────────────

#[repr(C)] #[derive(Clone, Copy)]
pub struct ClapVersion { pub major: u32, pub minor: u32, pub revision: u32 }

#[repr(C)]
pub struct ClapPluginDescriptor {
    pub clap_version : ClapVersion,
    pub id           : *const c_char,
    pub name         : *const c_char,
    pub vendor       : *const c_char,
    pub url          : *const c_char,
    pub manual_url   : *const c_char,
    pub support_url  : *const c_char,
    pub version      : *const c_char,
    pub description  : *const c_char,
    pub features     : *const *const c_char,
}

#[repr(C)]
struct ClapPluginEntry {
    clap_version : ClapVersion,
    init         : Option<unsafe extern "C" fn(*const c_char) -> bool>,
    deinit       : Option<unsafe extern "C" fn()>,
    get_factory  : Option<unsafe extern "C" fn(*const c_char) -> *const c_void>,
}

#[repr(C)]
pub struct ClapHost {
    pub clap_version     : ClapVersion,
    pub host_data        : *mut c_void,
    pub name             : *const c_char,
    pub vendor           : *const c_char,
    pub url              : *const c_char,
    pub version          : *const c_char,
    pub get_extension    : Option<unsafe extern "C" fn(*const ClapHost, *const c_char) -> *const c_void>,
    pub request_restart  : Option<unsafe extern "C" fn(*const ClapHost)>,
    pub request_process  : Option<unsafe extern "C" fn(*const ClapHost)>,
    pub request_callback : Option<unsafe extern "C" fn(*const ClapHost)>,
}

#[repr(C)]
pub struct ClapPlugin {
    pub desc             : *const ClapPluginDescriptor,
    pub plugin_data      : *mut c_void,
    pub init             : Option<unsafe extern "C" fn(*const ClapPlugin) -> bool>,
    pub destroy          : Option<unsafe extern "C" fn(*const ClapPlugin)>,
    pub activate         : Option<unsafe extern "C" fn(*const ClapPlugin, f64, u32, u32) -> bool>,
    pub deactivate       : Option<unsafe extern "C" fn(*const ClapPlugin)>,
    pub start_processing : Option<unsafe extern "C" fn(*const ClapPlugin) -> bool>,
    pub stop_processing  : Option<unsafe extern "C" fn(*const ClapPlugin)>,
    pub reset            : Option<unsafe extern "C" fn(*const ClapPlugin)>,
    pub process          : Option<unsafe extern "C" fn(*const ClapPlugin, *const ClapProcess) -> i32>,
    pub get_extension    : Option<unsafe extern "C" fn(*const ClapPlugin, *const c_char) -> *const c_void>,
    pub on_main_thread   : Option<unsafe extern "C" fn(*const ClapPlugin)>,
}

#[repr(C)]
struct ClapPluginFactory {
    get_plugin_count      : Option<unsafe extern "C" fn(*const ClapPluginFactory) -> u32>,
    get_plugin_descriptor : Option<unsafe extern "C" fn(*const ClapPluginFactory, u32) -> *const ClapPluginDescriptor>,
    create                : Option<unsafe extern "C" fn(*const ClapPluginFactory, *const ClapHost, *const c_char) -> *const ClapPlugin>,
}

#[repr(C)]
pub struct ClapAudioBuffer {
    pub data32        : *mut *mut f32,
    pub data64        : *mut *mut f64,
    pub channel_count : u32,
    pub latency       : u32,
    pub constant_mask : u64,
}

#[repr(C)]
pub struct ClapInputEvents {
    pub ctx  : *mut c_void,
    pub size : Option<unsafe extern "C" fn(*const ClapInputEvents) -> u32>,
    pub get  : Option<unsafe extern "C" fn(*const ClapInputEvents, u32) -> *const u8>,
}

#[repr(C)]
pub struct ClapOutputEvents {
    pub ctx      : *mut c_void,
    pub try_push : Option<unsafe extern "C" fn(*const ClapOutputEvents, *const u8) -> bool>,
}

#[repr(C)]
pub struct ClapProcess {
    pub steady_time         : i64,
    pub frames_count        : u32,
    pub transport           : *const c_void,
    pub audio_inputs        : *const ClapAudioBuffer,
    pub audio_outputs       : *mut ClapAudioBuffer,
    pub audio_inputs_count  : u32,
    pub audio_outputs_count : u32,
    pub in_events           : *const ClapInputEvents,
    pub out_events          : *mut ClapOutputEvents,
}

// State extension
#[repr(C)]
pub struct ClapOStream {
    pub ctx   : *mut c_void,
    pub write : Option<unsafe extern "C" fn(*const ClapOStream, *const c_void, u64) -> i64>,
}

#[repr(C)]
pub struct ClapIStream {
    pub ctx  : *mut c_void,
    pub read : Option<unsafe extern "C" fn(*const ClapIStream, *mut c_void, u64) -> i64>,
}

#[repr(C)]
pub struct ClapPluginState {
    pub save : Option<unsafe extern "C" fn(*const ClapPlugin, *const ClapOStream) -> bool>,
    pub load : Option<unsafe extern "C" fn(*const ClapPlugin, *const ClapIStream) -> bool>,
}

// GUI extension
/// CLAP window descriptor — the `specific` field holds the platform window handle:
///   Win32  → HWND (usize)
///   Cocoa  → NSView* (usize)
///   X11    → Window (usize)
#[repr(C)]
pub struct ClapWindow {
    pub api      : *const c_char,
    pub specific : usize,
}

#[repr(C)]
pub struct ClapPluginGui {
    pub is_api_supported  : Option<unsafe extern "C" fn(*const ClapPlugin, *const c_char, bool) -> bool>,
    pub get_preferred_api : Option<unsafe extern "C" fn(*const ClapPlugin, *mut *const c_char, *mut bool) -> bool>,
    pub create            : Option<unsafe extern "C" fn(*const ClapPlugin, *const c_char, bool) -> bool>,
    pub destroy           : Option<unsafe extern "C" fn(*const ClapPlugin)>,
    pub set_scale         : Option<unsafe extern "C" fn(*const ClapPlugin, f64) -> bool>,
    pub get_size          : Option<unsafe extern "C" fn(*const ClapPlugin, *mut u32, *mut u32) -> bool>,
    pub can_resize        : Option<unsafe extern "C" fn(*const ClapPlugin) -> bool>,
    pub get_resize_hints  : Option<unsafe extern "C" fn(*const ClapPlugin, *mut c_void) -> bool>,
    pub adjust_size       : Option<unsafe extern "C" fn(*const ClapPlugin, *mut u32, *mut u32) -> bool>,
    pub set_size          : Option<unsafe extern "C" fn(*const ClapPlugin, u32, u32) -> bool>,
    pub set_parent        : Option<unsafe extern "C" fn(*const ClapPlugin, *const ClapWindow) -> bool>,
    pub set_transient     : Option<unsafe extern "C" fn(*const ClapPlugin, *const ClapWindow) -> bool>,
    pub suggest_title     : Option<unsafe extern "C" fn(*const ClapPlugin, *const c_char)>,
    pub show              : Option<unsafe extern "C" fn(*const ClapPlugin) -> bool>,
    pub hide              : Option<unsafe extern "C" fn(*const ClapPlugin) -> bool>,
}

// ── No-op host callbacks ─────────────────────────────────────────────────────

unsafe extern "C" fn host_get_extension(_h: *const ClapHost, _id: *const c_char) -> *const c_void { std::ptr::null() }
unsafe extern "C" fn host_request_restart  (_h: *const ClapHost) {}
unsafe extern "C" fn host_request_process  (_h: *const ClapHost) {}
unsafe extern "C" fn host_request_callback (_h: *const ClapHost) {}

// ── Empty event queues (required by CLAP spec; may not be null) ──────────────

unsafe extern "C" fn evts_size(_: *const ClapInputEvents) -> u32 { 0 }
unsafe extern "C" fn evts_get (_: *const ClapInputEvents, _: u32) -> *const u8 { std::ptr::null() }
unsafe extern "C" fn evts_try_push(_: *const ClapOutputEvents, _: *const u8) -> bool { true }

// ── Send wrapper for raw plugin pointer ──────────────────────────────────────

/// Wraps `*const ClapPlugin` to allow moving it across threads.
/// Safety: CLAP plugins are designed for multi-threaded hosts; the audio
/// callback (process) and GUI thread (gui.show/hide) are called from different
/// threads per the CLAP spec.  We serialise all other mutations via `Mutex`.
struct RawPlugin(*const ClapPlugin);
unsafe impl Send for RawPlugin {}
unsafe impl Sync for RawPlugin {}

// ── Stable host memory ───────────────────────────────────────────────────────

/// Box-allocated struct that holds the `ClapHost` vtable AND the C-string data
/// it points to.  Because `CString` stores its bytes on the heap independently,
/// the pointers stored in `ClapHost` remain valid through any move of this struct.
struct HostBox {
    host  : ClapHost,
    _name : CString,
    _vend : CString,
    _url  : CString,
    _ver  : CString,
}

// ── State I/O helpers ────────────────────────────────────────────────────────

/// Context struct for the ostream write callback.
struct WriteCtx(Vec<u8>);

unsafe extern "C" fn state_write_cb(
    stream : *const ClapOStream,
    buf    : *const c_void,
    size   : u64,
) -> i64 {
    if stream.is_null() || buf.is_null() { return -1; }
    let ctx = &mut *((*stream).ctx as *mut WriteCtx);
    let slice = std::slice::from_raw_parts(buf as *const u8, size as usize);
    ctx.0.extend_from_slice(slice);
    size as i64
}

/// Context struct for the istream read callback.
struct ReadCtx {
    ptr : *const u8,
    len : usize,
    pos : usize,
}

unsafe extern "C" fn state_read_cb(
    stream : *const ClapIStream,
    buf    : *mut c_void,
    size   : u64,
) -> i64 {
    if stream.is_null() || buf.is_null() { return -1; }
    let ctx   = &mut *((*stream).ctx as *mut ReadCtx);
    let avail  = ctx.len.saturating_sub(ctx.pos);
    let to_read = (size as usize).min(avail);
    if to_read > 0 {
        std::ptr::copy_nonoverlapping(ctx.ptr.add(ctx.pos), buf as *mut u8, to_read);
        ctx.pos += to_read;
    }
    to_read as i64
}

// ── Public processor type ────────────────────────────────────────────────────

/// A loaded and activated CLAP plugin instance, ready for audio processing.
///
/// Drop order matters:
///   plugin callbacks (stop/deactivate/destroy) must run BEFORE the library is
///   unloaded, so `_lib` is declared LAST — Rust drops fields in declaration order.
pub struct ClapProcessor {
    plugin    : RawPlugin,
    state_ext : Option<*const ClapPluginState>,
    gui_ext   : Option<*const ClapPluginGui>,
    deinit_fn : Option<unsafe extern "C" fn()>,
    _host     : Box<HostBox>,     // must outlive `plugin`
    _lib      : libloading::Library, // unloaded LAST
}

unsafe impl Send for ClapProcessor {}

impl ClapProcessor {
    /// Load a `.clap` file and initialise the first plugin for audio processing.
    pub fn load(path: &str, sample_rate: f64, block_size: usize) -> Result<Self> {
        unsafe {
            let lib = libloading::Library::new(path)
                .map_err(|e| anyhow!("Failed to load CLAP '{}': {}", path, e))?;

            // Resolve `clap_entry` export.
            let entry_sym: libloading::Symbol<*const ClapPluginEntry> = lib
                .get(b"clap_entry\0")
                .map_err(|_| anyhow!("'{}' has no clap_entry export", path))?;
            let entry = &**entry_sym;

            if entry.clap_version.major != 1 {
                return Err(anyhow!(
                    "Unsupported CLAP major version {} in '{}'",
                    entry.clap_version.major, path
                ));
            }

            // Build a pinned host struct (Box keeps it at a stable address).
            let name = CString::new("ReLightHost").unwrap();
            let vend = CString::new("ReLightHost").unwrap();
            let url  = CString::new("https://github.com").unwrap();
            let ver  = CString::new(env!("CARGO_PKG_VERSION")).unwrap();

            let host_box = Box::new(HostBox {
                host: ClapHost {
                    clap_version     : ClapVersion { major: 1, minor: 0, revision: 0 },
                    host_data        : std::ptr::null_mut(),
                    name             : name.as_ptr(),
                    vendor           : vend.as_ptr(),
                    url              : url.as_ptr(),
                    version          : ver.as_ptr(),
                    get_extension    : Some(host_get_extension),
                    request_restart  : Some(host_request_restart),
                    request_process  : Some(host_request_process),
                    request_callback : Some(host_request_callback),
                },
                _name: name, _vend: vend, _url: url, _ver: ver,
            });

            // Initialise entry.
            let path_cs = CString::new(path).unwrap_or_default();
            if let Some(init_fn) = entry.init {
                if !init_fn(path_cs.as_ptr()) {
                    return Err(anyhow!("clap_entry.init() failed for '{}'", path));
                }
            }
            let deinit_fn = entry.deinit;

            // Get the plugin factory.
            let get_factory = entry.get_factory
                .ok_or_else(|| anyhow!("'{}' has no get_factory", path))?;
            let factory = get_factory(CLAP_PLUGIN_FACTORY_ID.as_ptr() as *const c_char)
                as *const ClapPluginFactory;
            if factory.is_null() {
                return Err(anyhow!("'{}' has no CLAP plugin factory", path));
            }

            let count_fn = (*factory).get_plugin_count
                .ok_or_else(|| anyhow!("factory has no get_plugin_count in '{}'", path))?;
            if count_fn(factory) == 0 {
                return Err(anyhow!("'{}' contains no plugins", path));
            }

            // Grab the first plugin descriptor.
            let desc_fn = (*factory).get_plugin_descriptor
                .ok_or_else(|| anyhow!("factory has no get_plugin_descriptor"))?;
            let desc = desc_fn(factory, 0);
            if desc.is_null() {
                return Err(anyhow!("null plugin descriptor in '{}'", path));
            }

            let plugin_id = (*desc).id;
            let name_str = if (*desc).name.is_null() { path.to_string() }
                else { CStr::from_ptr((*desc).name).to_string_lossy().into_owned() };

            // Create the plugin instance.
            let create_fn = (*factory).create
                .ok_or_else(|| anyhow!("factory has no create fn"))?;
            let plugin = create_fn(factory, &host_box.host as *const ClapHost, plugin_id);
            if plugin.is_null() {
                return Err(anyhow!("factory.create() returned null for '{}'", name_str));
            }

            // init → activate → start_processing
            if let Some(f) = (*plugin).init {
                if !f(plugin) {
                    if let Some(d) = (*plugin).destroy { d(plugin); }
                    return Err(anyhow!("plugin.init() failed for '{}'", name_str));
                }
            }
            let bs = block_size as u32;
            if let Some(f) = (*plugin).activate {
                if !f(plugin, sample_rate, 1, bs.max(4096)) {
                    log::warn!("CLAP activate() returned false for '{}'; continuing", name_str);
                }
            }
            if let Some(f) = (*plugin).start_processing {
                if !f(plugin) {
                    log::warn!("CLAP start_processing() returned false for '{}'", name_str);
                }
            }

            // Cache optional extension pointers.
            let get_ext = (*plugin).get_extension;
            let state_ext = get_ext.and_then(|f| {
                let p = f(plugin, CLAP_EXT_STATE.as_ptr() as *const c_char);
                if p.is_null() { None } else { Some(p as *const ClapPluginState) }
            });
            let gui_ext = get_ext.and_then(|f| {
                let p = f(plugin, CLAP_EXT_GUI.as_ptr() as *const c_char);
                if p.is_null() { None } else { Some(p as *const ClapPluginGui) }
            });

            log::info!("CLAP processor ready for '{}'", name_str);
            Ok(Self {
                plugin    : RawPlugin(plugin),
                state_ext,
                gui_ext,
                deinit_fn,
                _host     : host_box,
                _lib      : lib,
            })
        }
    }

    /// Process a stereo buffer in-place through the CLAP plugin.
    pub fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]) {
        unsafe {
            let process_fn = match (*self.plugin.0).process {
                Some(f) => f,
                None    => return,
            };
            let frames = left.len().min(right.len()) as u32;

            // Channel pointer arrays — input AND output point to the same
            // buffers (in-place processing, which CLAP natively supports).
            let mut ch_ptrs: [*mut f32; 2] = [left.as_mut_ptr(), right.as_mut_ptr()];
            let null64: *mut *mut f64       = std::ptr::null_mut();

            let in_buf = ClapAudioBuffer {
                data32: ch_ptrs.as_mut_ptr(), data64: null64,
                channel_count: 2, latency: 0, constant_mask: 0,
            };
            let mut out_buf = ClapAudioBuffer {
                data32: ch_ptrs.as_mut_ptr(), data64: null64,
                channel_count: 2, latency: 0, constant_mask: 0,
            };

            let in_evts  = ClapInputEvents  { ctx: std::ptr::null_mut(), size: Some(evts_size),     get:      Some(evts_get) };
            let mut out_evts = ClapOutputEvents { ctx: std::ptr::null_mut(), try_push: Some(evts_try_push) };

            let proc_data = ClapProcess {
                steady_time        : -1,
                frames_count       : frames,
                transport          : std::ptr::null(),
                audio_inputs       : &in_buf,
                audio_outputs      : &mut out_buf,
                audio_inputs_count : 1,
                audio_outputs_count: 1,
                in_events          : &in_evts,
                out_events         : &mut out_evts,
            };

            process_fn(self.plugin.0, &proc_data);
        }
    }

    /// Serialise plugin state (CLAP state extension `save`).
    pub fn get_state(&self) -> Vec<u8> {
        let ext = match self.state_ext { Some(e) => e, None => return Vec::new() };
        unsafe {
            let save_fn = match (*ext).save { Some(f) => f, None => return Vec::new() };
            let mut ctx = WriteCtx(Vec::new());
            let stream  = ClapOStream { ctx: &mut ctx as *mut WriteCtx as *mut c_void, write: Some(state_write_cb) };
            save_fn(self.plugin.0, &stream);
            ctx.0
        }
    }

    /// Restore plugin state (CLAP state extension `load`).
    pub fn set_state(&self, data: &[u8]) {
        if data.is_empty() { return; }
        let ext = match self.state_ext { Some(e) => e, None => return };
        unsafe {
            let load_fn = match (*ext).load { Some(f) => f, None => return };
            let mut ctx = ReadCtx { ptr: data.as_ptr(), len: data.len(), pos: 0 };
            let stream  = ClapIStream { ctx: &mut ctx as *mut ReadCtx as *mut c_void, read: Some(state_read_cb) };
            load_fn(self.plugin.0, &stream);
        }
    }

    /// Expose the raw plugin pointer (as usize) for the GUI thread.
    /// Safe as long as `ClapProcessor` outlives the GUI thread — the
    /// `gui_open` AtomicBool in `PluginInstance` guarantees this ordering.
    pub fn raw_plugin_usize(&self) -> usize { self.plugin.0 as usize }

    /// Expose the raw GUI extension pointer (as usize) for the GUI thread.
    pub fn raw_gui_ext_usize(&self) -> Option<usize> {
        self.gui_ext.map(|p| p as usize)
    }

    /// Open the native GUI.  Spawns a platform GUI thread.
    pub fn open_gui(
        &self,
        plugin_name : &str,
        gui_flag    : Arc<AtomicBool>,
        gui_hwnd    : Arc<AtomicIsize>,
    ) -> Result<()> {
        let gui_ext_usize = match self.raw_gui_ext_usize() {
            Some(p) => p,
            None => {
                gui_flag.store(false, Ordering::Release);
                return Err(anyhow!("'{}' has no CLAP GUI extension", plugin_name));
            }
        };
        crate::plugins::clap_gui::open_clap_gui(
            self.raw_plugin_usize(),
            gui_ext_usize,
            plugin_name,
            gui_flag,
            gui_hwnd,
        )
    }
}

impl Drop for ClapProcessor {
    fn drop(&mut self) {
        // Tear down in reverse-activation order.
        // `_lib` is dropped AFTER this block (it is declared last in the struct).
        unsafe {
            let p = self.plugin.0;
            if let Some(f) = (*p).stop_processing { f(p); }
            if let Some(f) = (*p).deactivate      { f(p); }
            if let Some(f) = (*p).destroy         { f(p); }
            if let Some(f) = self.deinit_fn       { f();  }
        }
    }
}

// ── CLAP metadata reader (used by scanner — loads DLL briefly) ───────────────

/// Read `(name, vendor, version)` from a `.clap` file by briefly loading it.
/// Returns `None` if the file cannot be interrogated.
pub fn read_clap_metadata(path: &std::path::Path) -> Option<(String, String, String)> {
    unsafe {
        let lib  = libloading::Library::new(path).ok()?;
        let sym  : libloading::Symbol<*const ClapPluginEntry> = lib.get(b"clap_entry\0").ok()?;
        let entry = &**sym;

        if entry.clap_version.major != 1 { return None; }

        // init — required before calling get_factory
        let path_cs = CString::new(path.to_string_lossy().as_ref()).ok()?;
        if let Some(f) = entry.init {
            if !f(path_cs.as_ptr()) { return None; }
        }

        let result = (|| -> Option<(String, String, String)> {
            let get_factory = entry.get_factory?;
            let factory = get_factory(CLAP_PLUGIN_FACTORY_ID.as_ptr() as *const c_char)
                as *const ClapPluginFactory;
            if factory.is_null() { return None; }

            let count_fn = (*factory).get_plugin_count?;
            if count_fn(factory) == 0 { return None; }

            let desc_fn = (*factory).get_plugin_descriptor?;
            let desc    = desc_fn(factory, 0);
            if desc.is_null() { return None; }

            let rs = |ptr: *const c_char| -> String {
                if ptr.is_null() { String::new() }
                else { CStr::from_ptr(ptr).to_string_lossy().into_owned() }
            };

            let name    = rs((*desc).name);
            let vendor  = rs((*desc).vendor);
            let version = rs((*desc).version);

            let name = if name.is_empty() {
                path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string()
            } else { name };

            Some((name, vendor, version))
        })();

        // Always deinit after reading metadata.
        if let Some(f) = entry.deinit { f(); }
        result
    }
}
