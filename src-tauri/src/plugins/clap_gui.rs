//! CLAP GUI hosting — Win32 host window + `clap_plugin_gui_t` embedding.
//!
//! Flow mirrors vst2_gui.rs:
//!   1. Spawn a dedicated GUI thread  (CoInitializeEx for COM compat)
//!   2. `gui.create(plugin, "win32", false)` — create embedded view
//!   3. `gui.get_size` → size our host window
//!   4. `gui.set_parent(plugin, &ClapWindow{api="win32", hwnd})` — embed
//!   5. `gui.show` → standard Win32 GetMessageW loop
//!   6. WM_CLOSE: `gui.hide` → `gui.destroy` → set gui_flag=false

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
    use super::clap_processor::{ClapPlugin, ClapPluginGui};

    // Verify the GUI extension pointer is non-null before spawning.
    if gui_ext_raw == 0 {
        gui_flag.store(false, Ordering::Release);
        return Err(anyhow!("Null CLAP GUI extension for '{}'", plugin_name));
    }

    let name_owned = plugin_name.to_string();

    std::thread::Builder::new()
        .name(format!("clap-gui-{}", plugin_name))
        .stack_size(4 * 1024 * 1024)
        .spawn(move || {
            // GuiFlagGuard clears the open flag and HWND on any exit path.
            struct GuiFlagGuard(Arc<AtomicBool>, Arc<AtomicIsize>);
            impl Drop for GuiFlagGuard {
                fn drop(&mut self) {
                    self.1.store(0, Ordering::Release);
                    self.0.store(false, Ordering::Release);
                }
            }
            let _guard = GuiFlagGuard(gui_flag, gui_hwnd.clone());

            let plugin  = plugin_raw  as *const ClapPlugin;
            let gui_ext = gui_ext_raw as *const ClapPluginGui;

            if let Err(e) = win::run_clap_editor(plugin, gui_ext, &name_owned, gui_hwnd) {
                log::error!("CLAP GUI error for '{}': {}", name_owned, e);
            }
        })
        .map_err(|e| anyhow!("Failed to spawn CLAP GUI thread: {}", e))?;

    Ok(())
}

// ── Win32 window implementation ───────────────────────────────────────────────

#[cfg(target_os = "windows")]
mod win {
    use anyhow::{anyhow, Result};
    use std::ffi::{CString, c_void};
    use std::sync::Arc;
    use std::ptr;
    use std::sync::atomic::{AtomicIsize, Ordering};

    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    use super::super::clap_processor::{ClapPlugin, ClapPluginGui, ClapWindow, CLAP_WINDOW_API_WIN32};

    const CLASS_NAME: &[u16] = &[
        b'R' as u16, b'e' as u16, b'L' as u16, b'i' as u16, b'g' as u16,
        b'h' as u16, b't' as u16, b'C' as u16, b'L' as u16, b'A' as u16,
        b'P' as u16, 0,
    ];

    /// Thread-local HWND (isize) used by WndProc → gui.set_size.
    use std::cell::Cell;
    thread_local! {
        static TL_HWND: Cell<isize> = Cell::new(0);
        /// Resize callback: (width, height) → gui.set_size.
        static TL_RESIZE: std::cell::RefCell<Option<Box<dyn Fn(u32, u32)>>> =
            std::cell::RefCell::new(None);
    }

    /// Posted to self once the message loop is running, to call gui.show from
    /// within the loop (avoids a ShowWindow-vs-GetMessage ordering problem).
    const WM_CLAP_SHOW: u32 = WM_USER + 1;

    unsafe extern "system" fn wnd_proc(
        hwnd   : HWND,
        msg    : u32,
        wparam : WPARAM,
        lparam : LPARAM,
    ) -> LRESULT {
        match msg {
            WM_CLAP_SHOW => {
                ShowWindow(hwnd, SW_SHOW);
                0
            }
            WM_SIZE => {
                let w = (lparam & 0xffff) as u32;
                let h = ((lparam >> 16) & 0xffff) as u32;
                if w > 0 && h > 0 {
                    TL_RESIZE.with(|r| {
                        if let Some(ref f) = *r.borrow() { f(w, h); }
                    });
                }
                0
            }
            WM_CLOSE => {
                DestroyWindow(hwnd);
                0
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    // The resize callback closure needs its own `unsafe {}` to call the fn pointer,
    // but Rust's unused_unsafe lint fires because it's nested inside the outer
    // `unsafe` block of run_clap_editor. The inner unsafe IS required.
    #[allow(unused_unsafe)]
    pub fn run_clap_editor(
        plugin    : *const ClapPlugin,
        gui_ext   : *const ClapPluginGui,
        name      : &str,
        gui_hwnd  : Arc<AtomicIsize>,
    ) -> Result<()> {
        unsafe {
            // COM apartment for plugins that use COM internally (e.g. some Windows audio plugins).
            struct ComGuard;
            impl Drop for ComGuard { fn drop(&mut self) { unsafe { CoUninitialize(); } } }
            CoInitializeEx(ptr::null::<c_void>(), COINIT_APARTMENTTHREADED as u32);
            let _com = ComGuard;

            // ── Check Win32 API is supported ──────────────────────────────────
            if let Some(is_supported) = (*gui_ext).is_api_supported {
                if !is_supported(plugin, CLAP_WINDOW_API_WIN32.as_ptr() as *const _, false) {
                    return Err(anyhow!("'{}' does not support Win32 embedded GUI", name));
                }
            }

            // ── Create the CLAP GUI view ──────────────────────────────────────
            let create = (*gui_ext).create
                .ok_or_else(|| anyhow!("No gui.create for '{}'", name))?;
            if !create(plugin, CLAP_WINDOW_API_WIN32.as_ptr() as *const _, false) {
                return Err(anyhow!("gui.create() failed for '{}'", name));
            }

            // ── Get initial plugin size, fall back to 640×400 ─────────────────
            let (mut plug_w, mut plug_h) = (640u32, 400u32);
            if let Some(get_size) = (*gui_ext).get_size {
                get_size(plugin, &mut plug_w, &mut plug_h);
            }

            // ── Register Win32 class (idempotent) ─────────────────────────────
            let hinstance = GetModuleHandleW(std::ptr::null());
            // Load the app icon embedded in the exe by tauri_build (resource ID 1).
            let hicon = LoadIconW(hinstance, 1usize as *const u16);
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
            RegisterClassW(&wc); // may fail if already registered — that's fine

            // Compute window rect from client size.
            let title_wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
            let style = WS_OVERLAPPEDWINDOW & !WS_THICKFRAME & !WS_MAXIMIZEBOX;
            let mut rect = windows_sys::Win32::Foundation::RECT {
                left: 0, top: 0,
                right: plug_w as i32,
                bottom: plug_h as i32,
            };
            AdjustWindowRect(&mut rect, style, 0);
            let win_w = (rect.right  - rect.left) as i32;
            let win_h = (rect.bottom - rect.top)  as i32;

            // ── Create host window ────────────────────────────────────────────
            let hwnd = CreateWindowExW(
                0,
                CLASS_NAME.as_ptr(),
                title_wide.as_ptr(),
                style,
                CW_USEDEFAULT, CW_USEDEFAULT,
                win_w, win_h,
                ptr::null_mut(), ptr::null_mut(), hinstance, ptr::null_mut(),
            );
            if hwnd.is_null() {
                if let Some(d) = (*gui_ext).destroy { d(plugin); }
                return Err(anyhow!("CreateWindowExW failed for '{}'", name));
            }

            // Publish HWND so PluginInstance::drop can send WM_CLOSE.
            gui_hwnd.store(hwnd as isize, Ordering::Release);

            // Apply the app icon to both title bar and taskbar.
            if !hicon.is_null() {
                SendMessageW(hwnd, WM_SETICON, ICON_BIG as _, hicon as _);
                SendMessageW(hwnd, WM_SETICON, ICON_SMALL as _, hicon as _);
            }
            TL_HWND.with(|c| c.set(hwnd as isize));

            // ── Install resize callback ───────────────────────────────────────
            if let Some(set_size_fn) = (*gui_ext).set_size {
                TL_RESIZE.with(|r| {
                    *r.borrow_mut() = Some(Box::new(move |w, h| unsafe {
                        set_size_fn(plugin, w, h);
                    }));
                });
            }

            // ── Embed plugin into our window ──────────────────────────────────
            let win32_api_cstr = CString::new("win32").unwrap();
            let clap_win = ClapWindow {
                api      : win32_api_cstr.as_ptr(),
                specific : hwnd as usize,
            };
            let set_parent = (*gui_ext).set_parent
                .ok_or_else(|| anyhow!("No gui.set_parent for '{}'", name))?;
            if !set_parent(plugin, &clap_win) {
                DestroyWindow(hwnd);
                if let Some(d) = (*gui_ext).destroy { d(plugin); }
                return Err(anyhow!("gui.set_parent() failed for '{}'", name));
            }

            // Post WM_CLAP_SHOW so gui.show() is called from inside the loop.
            PostMessageW(hwnd, WM_CLAP_SHOW, 0, 0);

            // ── Message loop ─────────────────────────────────────────────────
            let mut msg = std::mem::zeroed::<MSG>();
            loop {
                let ret = GetMessageW(&mut msg, ptr::null_mut(), 0, 0);
                if ret == 0 || ret == -1 { break; }

                // WM_CLAP_SHOW: call gui.show from the message loop thread.
                if msg.hwnd == hwnd && msg.message == WM_CLAP_SHOW {
                    if let Some(show) = (*gui_ext).show { show(plugin); }
                }

                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            // ── Cleanup ───────────────────────────────────────────────────────
            if let Some(hide)    = (*gui_ext).hide    { hide(plugin);    }
            if let Some(destroy) = (*gui_ext).destroy { destroy(plugin); }

            TL_RESIZE.with(|r| *r.borrow_mut() = None);

            Ok(())
        }
    }
}
