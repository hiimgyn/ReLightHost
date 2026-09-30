//! CLAP GUI hosting — Win32 host window + `clap_plugin_gui_t` embedding.
//!
//! Every `gui.*` call is `[main-thread]` in CLAP — the thread that created
//! the plugin, i.e. the plugin host thread (see core::host_thread). So the
//! editor lives there too: its host window is created on that thread and
//! driven by the host thread's message pump; there is no per-editor thread.
//!   1. `gui.create(plugin, "win32", false)` → `set_scale` → `get_size`
//!   2. create the host window, `gui.set_parent`, `gui.show`
//!   3. WM_CLOSE: `gui.hide` → `gui.destroy` → destroy the host window
//!      (the plugin's embedded view goes before its parent does)
//!   4. WM_NCDESTROY: clear the GUI-open flag / HWND

use anyhow::{anyhow, Result};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

// ── Non-Windows stub ─────────────────────────────────────────────────────────

#[cfg(not(target_os = "windows"))]
pub fn open_clap_gui(
    _plugin_raw  : usize,
    _gui_ext_raw : usize,
    plugin_name  : &str,
    gui_flag     : Arc<AtomicBool>,
    _gui_hwnd    : Arc<AtomicIsize>,
) -> Result<()> {
    gui_flag.store(false, Ordering::Release);
    Err(anyhow!("CLAP GUI is only supported on Windows: {}", plugin_name))
}

// ── Windows implementation ───────────────────────────────────────────────────

#[cfg(target_os = "windows")]
pub fn open_clap_gui(
    plugin_raw  : usize,
    gui_ext_raw : usize,
    plugin_name : &str,
    gui_flag    : Arc<AtomicBool>,
    gui_hwnd    : Arc<AtomicIsize>,
) -> Result<()> {
    if gui_ext_raw == 0 {
        gui_flag.store(false, Ordering::Release);
        return Err(anyhow!("Null CLAP GUI extension for '{}'", plugin_name));
    }
    let name = plugin_name.to_string();
    crate::core::host_thread::run_blocking(move || unsafe {
        win::open(plugin_raw as *const _, gui_ext_raw as *const _, &name, gui_flag, gui_hwnd)
    })
}

// ── Win32 window implementation ───────────────────────────────────────────────

#[cfg(target_os = "windows")]
mod win {
    use anyhow::{anyhow, Result};
    use std::cell::Cell;
    use std::ffi::CString;
    use std::ptr;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    use super::super::super::processor::clap::{ClapPlugin, ClapPluginGui, ClapWindow, CLAP_WINDOW_API_WIN32};

    const CLASS_NAME: &[u16] = &[
        b'R' as u16, b'e' as u16, b'L' as u16, b'i' as u16, b'g' as u16,
        b'h' as u16, b't' as u16, b'C' as u16, b'L' as u16, b'A' as u16,
        b'P' as u16, 0,
    ];

    /// Per-window state, owned by the window (GWLP_USERDATA) and freed on
    /// WM_NCDESTROY.
    struct Editor {
        plugin: *const ClapPlugin,
        gui_ext: *const ClapPluginGui,
        gui_flag: Arc<AtomicBool>,
        gui_hwnd: Arc<AtomicIsize>,
        /// hide + destroy already sent.
        closed: Cell<bool>,
    }

    impl Editor {
        unsafe fn close_view(&self) {
            if self.closed.replace(true) {
                return;
            }
            if let Some(hide) = (*self.gui_ext).hide { hide(self.plugin); }
            if let Some(destroy) = (*self.gui_ext).destroy { destroy(self.plugin); }
        }
    }

    unsafe fn editor<'a>(hwnd: HWND) -> Option<&'a Editor> {
        (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Editor).as_ref()
    }

    unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        match msg {
            WM_SIZE => {
                let (w, h) = ((lparam & 0xffff) as u32, ((lparam >> 16) & 0xffff) as u32);
                if let Some(ed) = editor(hwnd) {
                    if w > 0 && h > 0 && !ed.closed.get() {
                        if let Some(set_size) = (*ed.gui_ext).set_size { set_size(ed.plugin, w, h); }
                    }
                }
                0
            }
            WM_CLOSE => {
                if let Some(ed) = editor(hwnd) {
                    ed.close_view();
                }
                DestroyWindow(hwnd);
                0
            }
            WM_NCDESTROY => {
                let raw = SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) as *mut Editor;
                if !raw.is_null() {
                    let ed = Box::from_raw(raw);
                    ed.close_view();
                    ed.gui_hwnd.store(0, Ordering::Release);
                    ed.gui_flag.store(false, Ordering::Release);
                    crate::app_events::emit_plugin_chain_changed("gui_close", None);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    /// Creates, embeds and shows the editor. Must run on the plugin host thread.
    pub(super) unsafe fn open(
        plugin: *const ClapPlugin,
        gui_ext: *const ClapPluginGui,
        name: &str,
        gui_flag: Arc<AtomicBool>,
        gui_hwnd: Arc<AtomicIsize>,
    ) -> Result<()> {
        if let Some(is_supported) = (*gui_ext).is_api_supported {
            if !is_supported(plugin, CLAP_WINDOW_API_WIN32.as_ptr() as *const _, false) {
                return Err(anyhow!("'{}' does not support Win32 embedded GUI", name));
            }
        }
        let create = (*gui_ext).create.ok_or_else(|| anyhow!("No gui.create for '{}'", name))?;
        if !create(plugin, CLAP_WINDOW_API_WIN32.as_ptr() as *const _, false) {
            return Err(anyhow!("gui.create() failed for '{}'", name));
        }

        // Scale for the system DPI (plugins that measure DPI themselves
        // ignore this).
        let dpi = windows_sys::Win32::UI::HiDpi::GetDpiForSystem();
        if let Some(set_scale) = (*gui_ext).set_scale {
            if dpi > 0 { set_scale(plugin, dpi as f64 / 96.0); }
        }
        let (mut plug_w, mut plug_h) = (640u32, 400u32);
        if let Some(get_size) = (*gui_ext).get_size {
            get_size(plugin, &mut plug_w, &mut plug_h);
        }

        let hinstance = GetModuleHandleW(ptr::null());
        // App icon embedded in the exe by tauri_build (resource ID 32512).
        let hicon = LoadIconW(hinstance, 32512 as _);
        let wc = WNDCLASSW {
            style         : CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc   : Some(wnd_proc),
            cbClsExtra    : 0,
            cbWndExtra    : 0,
            hInstance     : hinstance,
            hIcon         : hicon,
            hCursor       : LoadCursorW(ptr::null_mut(), IDC_ARROW),
            hbrBackground : 6 as _, // COLOR_WINDOW + 1
            lpszMenuName  : ptr::null(),
            lpszClassName : CLASS_NAME.as_ptr(),
        };
        RegisterClassW(&wc); // fails harmlessly if already registered

        let title_wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        let style = WS_OVERLAPPEDWINDOW & !WS_THICKFRAME & !WS_MAXIMIZEBOX;
        let mut rect = windows_sys::Win32::Foundation::RECT { left: 0, top: 0, right: plug_w as i32, bottom: plug_h as i32 };
        AdjustWindowRect(&mut rect, style, 0);
        let hwnd = CreateWindowExW(
            0, CLASS_NAME.as_ptr(), title_wide.as_ptr(), style,
            CW_USEDEFAULT, CW_USEDEFAULT, rect.right - rect.left, rect.bottom - rect.top,
            ptr::null_mut(), ptr::null_mut(), hinstance, ptr::null_mut(),
        );
        if hwnd.is_null() {
            if let Some(d) = (*gui_ext).destroy { d(plugin); }
            return Err(anyhow!("CreateWindowExW failed for '{}'", name));
        }
        if !hicon.is_null() {
            SendMessageW(hwnd, WM_SETICON, ICON_BIG as _, hicon as _);
            SendMessageW(hwnd, WM_SETICON, ICON_SMALL as _, hicon as _);
        }

        // From here on the window owns the editor state: any failure below
        // goes through DestroyWindow → WM_NCDESTROY, which destroys the view
        // and clears the flags.
        gui_hwnd.store(hwnd as isize, Ordering::Release);
        let ed = Box::new(Editor { plugin, gui_ext, gui_flag, gui_hwnd, closed: Cell::new(false) });
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(ed) as isize);

        let win32_api = CString::new("win32").unwrap();
        let clap_win = ClapWindow { api: win32_api.as_ptr(), specific: hwnd as usize };
        let embedded = (*gui_ext).set_parent.map(|set_parent| set_parent(plugin, &clap_win)).unwrap_or(false);
        if !embedded {
            DestroyWindow(hwnd);
            return Err(anyhow!("gui.set_parent() failed for '{}'", name));
        }

        ShowWindow(hwnd, SW_SHOW);
        if let Some(show) = (*gui_ext).show { show(plugin); }
        Ok(())
    }
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;
    use crate::plugins::processor::clap::{ClapPlugin, ClapPluginGui, ClapWindow};
    use parking_lot::Mutex;
    use std::thread::ThreadId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{IsWindow, PostMessageW, WM_CLOSE};

    static CALLS: Mutex<Vec<(&'static str, ThreadId, bool)>> = Mutex::new(Vec::new());
    static PARENT: Mutex<usize> = Mutex::new(0);

    fn record(name: &'static str) {
        let parent = *PARENT.lock();
        let alive = parent != 0 && unsafe { IsWindow(parent as _) } != 0;
        CALLS.lock().push((name, std::thread::current().id(), alive));
    }
    unsafe extern "C" fn create(_: *const ClapPlugin, _: *const std::ffi::c_char, _: bool) -> bool { record("create"); true }
    unsafe extern "C" fn set_parent(_: *const ClapPlugin, w: *const ClapWindow) -> bool {
        *PARENT.lock() = (*w).specific;
        record("set_parent");
        true
    }
    unsafe extern "C" fn show(_: *const ClapPlugin) -> bool { record("show"); true }
    unsafe extern "C" fn hide(_: *const ClapPlugin) -> bool { record("hide"); true }
    unsafe extern "C" fn destroy(_: *const ClapPlugin) { record("destroy"); }

    #[test]
    fn gui_runs_on_the_host_thread_and_is_destroyed_before_its_window() {
        let plugin: ClapPlugin = unsafe { std::mem::zeroed() };
        let mut gui: ClapPluginGui = unsafe { std::mem::zeroed() };
        gui.create = Some(create);
        gui.set_parent = Some(set_parent);
        gui.show = Some(show);
        gui.hide = Some(hide);
        gui.destroy = Some(destroy);
        let flag = Arc::new(AtomicBool::new(true));
        let hwnd = Arc::new(AtomicIsize::new(0));

        open_clap_gui(&plugin as *const _ as usize, &gui as *const _ as usize, "fake", Arc::clone(&flag), Arc::clone(&hwnd)).unwrap();
        let host = crate::core::host_thread::run_blocking(|| std::thread::current().id());
        let win = hwnd.load(Ordering::Acquire);
        assert_ne!(win, 0, "host window published");

        unsafe { PostMessageW(win as _, WM_CLOSE, 0, 0) };
        crate::core::host_thread::run_blocking(crate::core::host_thread::wait_a_moment);

        let calls = CALLS.lock().clone();
        let names: Vec<_> = calls.iter().map(|c| c.0).collect();
        assert_eq!(names, ["create", "set_parent", "show", "hide", "destroy"]);
        assert!(calls.iter().all(|c| c.1 == host), "every gui.* call on the plugin host thread");
        assert!(calls[3].2 && calls[4].2, "hide/destroy while the host window still exists");
        assert!(!flag.load(Ordering::Acquire), "gui flag cleared");
        assert_eq!(hwnd.load(Ordering::Acquire), 0);
    }
}
