//! VST2 editor hosting — Win32 host window + `effEditOpen` embedding.
//!
//! The editor lives on the plugin host thread (see core::host_thread), the
//! thread that created the plugin: JUCE VST2 plugins bind their
//! MessageManager to it, so an editor driven from another thread races its
//! own timers (the same split that crashed VST3 editors). The host thread's
//! message pump drives the window, like CLAP's and VST3's (gui/clap.rs,
//! gui/vst3.rs); there is no per-editor thread.
//!   1. create the host window, `effEditOpen(hwnd)` (the plugin creates a
//!      child window inside ours), refit to `effEditGetRect`, show
//!   2. a timer drives `effEditIdle`
//!   3. WM_CLOSE: `effEditClose` → destroy the host window (the editor goes
//!      before its parent window does)
//!   4. WM_NCDESTROY: release the plugin, then clear the GUI-open flag / HWND

use anyhow::Result;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicIsize};

use super::super::processor::vst2::RawPlugin;

// ── Platform stub for non-Windows ────────────────────────────────────────────

#[cfg(not(target_os = "windows"))]
pub(crate) fn open_vst2_gui(
    _plugin: Arc<Mutex<RawPlugin>>,
    plugin_name: &str,
    gui_flag: Arc<AtomicBool>,
    _gui_hwnd: Arc<AtomicIsize>,
) -> Result<()> {
    gui_flag.store(false, std::sync::atomic::Ordering::Release);
    Err(anyhow::anyhow!("VST2 GUI is only supported on Windows: {}", plugin_name))
}

// ── Windows implementation ────────────────────────────────────────────────────

/// Creates, opens and shows the editor on the plugin host thread and
/// returns once it is up. The window owns it from then on.
#[cfg(target_os = "windows")]
pub(crate) fn open_vst2_gui(
    plugin: Arc<Mutex<RawPlugin>>,
    plugin_name: &str,
    gui_flag: Arc<AtomicBool>,
    gui_hwnd: Arc<AtomicIsize>,
) -> Result<()> {
    let name = plugin_name.to_string();
    crate::core::host_thread::run_blocking(move || win::open(plugin, &name, gui_flag, gui_hwnd))
}

// ── Win32 GUI implementation ──────────────────────────────────────────────────

#[cfg(target_os = "windows")]
mod win {
    use super::super::super::processor::vst2::EditorIdle;
    use super::RawPlugin;
    use anyhow::{anyhow, Result};
    use std::cell::Cell;
    use std::ptr;
    use std::sync::{Arc, Mutex};
    use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetModuleHandleA, LoadLibraryA, GetProcAddress};
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    /// Timer driving `effEditIdle` — VST2 editors expect the host to call it
    /// regularly (meters, animations and some plugins' whole repaint hang
    /// off it).
    const IDLE_TIMER: usize = 1;
    const IDLE_INTERVAL_MS: u32 = 30;

    /// Per-window state, owned by the window (GWLP_USERDATA) and freed on
    /// WM_NCDESTROY.
    struct Editor {
        plugin: Arc<Mutex<RawPlugin>>,
        idle: EditorIdle,
        /// effEditClose already sent.
        closed: Cell<bool>,
        gui_flag: Arc<AtomicBool>,
        gui_hwnd: Arc<AtomicIsize>,
    }

    impl Editor {
        /// `effEditClose`, while the parent window still exists.
        fn close_view(&self) {
            if self.closed.replace(true) {
                return;
            }
            if let Ok(mut p) = self.plugin.lock() {
                p.editor_close();
            }
        }
    }

    unsafe fn editor<'a>(hwnd: HWND) -> Option<&'a Editor> {
        (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Editor).as_ref()
    }

    /// Must run on the plugin host thread.
    pub(super) fn open(
        plugin: Arc<Mutex<RawPlugin>>,
        plugin_name: &str,
        gui_flag: Arc<AtomicBool>,
        gui_hwnd: Arc<AtomicIsize>,
    ) -> Result<()> {
        // The plugin mutex is held only for these editor calls, not for the
        // editor's lifetime: the audio thread try_locks it every block.
        let mut guard = plugin.lock()
            .map_err(|_| anyhow!("VST2 plugin mutex poisoned for '{}'", plugin_name))?;
        if !guard.has_editor() {
            return Err(anyhow!("'{}' has no VST2 editor", plugin_name));
        }
        let (mut width, mut height) = guard.editor_size();
        if width  <= 0 { width  = 640; }
        if height <= 0 { height = 480; }

        let hwnd = create_host_window(plugin_name, width, height)?;
        // The plugin creates a child window at (0, 0) inside hwnd.
        if !guard.editor_open(hwnd) {
            unsafe { DestroyWindow(hwnd); }
            return Err(anyhow!("'{}': VST2 effEditOpen returned false", plugin_name));
        }
        // The plugin may pump messages inside effEditOpen (a licence
        // dialog) and the window can be closed meanwhile.
        if unsafe { IsWindow(hwnd) } == 0 {
            guard.editor_close();
            return Err(anyhow!("'{}' editor was closed while opening", plugin_name));
        }
        let idle = guard.editor_idle_handle();

        // Some plugins report the correct size only after open().
        let (post_w, post_h) = guard.editor_size();
        drop(guard);
        if post_w > 10 && post_h > 10 && (post_w != width || post_h != height) {
            let style = WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
            let mut wr = RECT { left: 0, top: 0, right: post_w, bottom: post_h };
            unsafe {
                let dpi = windows_sys::Win32::UI::HiDpi::GetDpiForWindow(hwnd as _);
                if dpi == 0 || !try_adjust_window_rect_for_dpi(&mut wr as *mut RECT, style, 0, dpi) {
                    AdjustWindowRect(&mut wr, style, 0);
                }
                SetWindowPos(
                    hwnd, ptr::null_mut(), 0, 0,
                    wr.right - wr.left, wr.bottom - wr.top,
                    SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
        }

        // From here on the window owns the editor: closing it (WM_CLOSE, or
        // DestroyWindow → WM_NCDESTROY) closes the editor and clears the flags.
        gui_hwnd.store(hwnd as isize, Ordering::Release);
        let ed = Box::new(Editor { plugin, idle, closed: Cell::new(false), gui_flag, gui_hwnd });
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(ed) as isize);
            ShowWindow(hwnd, SW_SHOW);
            SetForegroundWindow(hwnd);
            SetTimer(hwnd, IDLE_TIMER, IDLE_INTERVAL_MS, None);
        }
        log::info!("'{}' VST2 GUI opened ({}x{})", plugin_name, width, height);
        Ok(())
    }

    unsafe extern "system" fn vst2_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        match msg {
            WM_TIMER if wparam == IDLE_TIMER => {
                if let Some(ed) = editor(hwnd) {
                    if !ed.closed.get() {
                        ed.idle.idle();
                    }
                }
                0
            }
            WM_CLOSE => {
                KillTimer(hwnd, IDLE_TIMER);
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
                    let (flag, gui_hwnd) = (Arc::clone(&ed.gui_flag), Arc::clone(&ed.gui_hwnd));
                    // Releases the plugin reference before the flag tells
                    // PluginInstance it may tear down.
                    drop(ed);
                    gui_hwnd.store(0, Ordering::Release);
                    flag.store(false, Ordering::Release);
                    crate::app_events::emit_plugin_chain_changed("gui_close", None);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_ERASEBKGND => 1, // prevent flicker behind plugin child window
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    // Runtime resolver for AdjustWindowRectForDpi (available on Win10+).
    // Falls back to returning false when not present.
    unsafe fn try_adjust_window_rect_for_dpi(rect: *mut RECT, style: u32, menu: i32, dpi: u32) -> bool {
        let lib_name = b"user32.dll\0";
        let fn_name = b"AdjustWindowRectForDpi\0";
        let mut hmod = GetModuleHandleA(lib_name.as_ptr());
        if hmod.is_null() {
            hmod = LoadLibraryA(lib_name.as_ptr());
        }
        if hmod.is_null() { return false; }
        let proc = GetProcAddress(hmod, fn_name.as_ptr());
        let proc = match proc { Some(p) => p, None => return false };
        let f: extern "system" fn(*mut RECT, u32, i32, u32) -> i32 = std::mem::transmute(proc);
        let r = f(rect, style, menu, dpi);
        r != 0
    }

    fn create_host_window(title: &str, width: i32, height: i32) -> Result<windows_sys::Win32::Foundation::HWND> {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        let class_name: Vec<u16> = OsStr::new("ReLightHostVST2")
            .encode_wide().chain(Some(0)).collect();
        let window_title: Vec<u16> = OsStr::new(title)
            .encode_wide().chain(Some(0)).collect();

        // Fixed, non-resizable window — VST2 plugins don't have a resize protocol.
        let style = WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;

        unsafe {
            let hinstance = GetModuleHandleW(ptr::null());

            // Load the app icon embedded in the exe by tauri_build (resource ID 32512).
            let hicon = LoadIconW(hinstance, 32512 as _);

            let wc = WNDCLASSW {
                style: CS_DBLCLKS,
                lpfnWndProc: Some(vst2_wnd_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: hinstance,
                hIcon: hicon,
                hCursor: LoadCursorW(ptr::null_mut(), IDC_ARROW),
                hbrBackground: 6 as _, // COLOR_WINDOW + 1
                lpszMenuName: ptr::null(),
                lpszClassName: class_name.as_ptr(),
            };
            // RegisterClassW is idempotent — silently fails if class already exists.
            RegisterClassW(&wc);

                let mut wr = RECT { left: 0, top: 0, right: width, bottom: height };
                AdjustWindowRect(&mut wr, style, 0);

                let hwnd = CreateWindowExW(
                    0,
                    class_name.as_ptr(),
                    window_title.as_ptr(),
                    style,
                    CW_USEDEFAULT, CW_USEDEFAULT,
                    wr.right  - wr.left,
                    wr.bottom - wr.top,
                    ptr::null_mut(),
                    ptr::null_mut(),
                    hinstance,
                    ptr::null(),
                );

            if hwnd.is_null() {
                return Err(anyhow!("CreateWindowExW failed for VST2 plugin '{}'", title));
            }

            // Set both the big (title bar) and small (taskbar) icons.
            if !hicon.is_null() {
                SendMessageW(hwnd, WM_SETICON, ICON_BIG as _, hicon as _);
                SendMessageW(hwnd, WM_SETICON, ICON_SMALL as _, hicon as _);
            }

            // Correct outer size to account for window DPI so the client area
            // matches the requested width/height on per-monitor setups.
            let dpi = windows_sys::Win32::UI::HiDpi::GetDpiForWindow(hwnd as _);
            let mut corrected = RECT { left: 0, top: 0, right: width, bottom: height };
            if dpi != 0 {
                if !try_adjust_window_rect_for_dpi(&mut corrected as *mut RECT, style, 0, dpi) {
                    AdjustWindowRect(&mut corrected, style, 0);
                }
            } else {
                AdjustWindowRect(&mut corrected, style, 0);
            }
            SetWindowPos(
                hwnd,
                ptr::null_mut(),
                0,
                0,
                corrected.right - corrected.left,
                corrected.bottom - corrected.top,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            );

            Ok(hwnd)
        }
    }
}
