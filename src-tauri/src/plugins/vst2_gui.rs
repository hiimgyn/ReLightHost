//! VST2 editor GUI hosting on Windows.
//!
//! Opens the plugin editor inside a native Win32 host window, mirroring how
//! VST3 GUIs are hosted in vst3_gui.rs.  The key mechanics:
//!   1. Create a regular Win32 window as the "parent frame".
//!   2. Call `effEditOpen(hwnd)` — the plugin creates a child window inside ours.
//!   3. Run a standard blocking Win32 message loop.
//!   4. On close: `effEditClose`, then let the GUI thread exit naturally.
//!
//! The `SendableEditor` wrapper allows sending `Box<dyn Editor>` across threads.
//! vst-rs's concrete `EditorInstance` type only contains `Arc<Send+Sync>` + bool,
//! so this is safe even though `Editor` is not declared Send in the crate.

use anyhow::{anyhow, Result};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

use super::vst2_processor::SendableEditor;

// ── Platform stub for non-Windows ────────────────────────────────────────────

#[cfg(not(target_os = "windows"))]
pub fn open_vst2_gui(
    _editor_arc: Arc<Mutex<Option<SendableEditor>>>,
    plugin_name: &str,
    gui_flag: Arc<AtomicBool>,
    _gui_hwnd: Arc<AtomicIsize>,
) -> Result<()> {
    gui_flag.store(false, Ordering::Release);
    Err(anyhow!("VST2 GUI is only supported on Windows: {}", plugin_name))
}

// ── Windows implementation ────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
pub fn open_vst2_gui(
    editor_arc: Arc<Mutex<Option<SendableEditor>>>,
    plugin_name: &str,
    gui_flag: Arc<AtomicBool>,
    gui_hwnd: Arc<AtomicIsize>,
) -> Result<()> {
    // Verify the plugin has an editor before spawning a thread.
    {
        let guard = editor_arc.lock()
            .map_err(|_| anyhow!("VST2 editor mutex poisoned for '{}'", plugin_name))?;
        if guard.is_none() {
            gui_flag.store(false, Ordering::Release);
            return Err(anyhow!("'{}' has no VST2 editor", plugin_name));
        }
    }

    let editor_arc_clone = Arc::clone(&editor_arc);
    let name_owned = plugin_name.to_string();

    std::thread::Builder::new()
        .name(format!("vst2-gui-{}", plugin_name))
        .stack_size(4 * 1024 * 1024) // 4 MB — plenty for any plugin GUI
        .spawn(move || {
            // GuiFlagGuard clears the "GUI open" flag and the stored HWND when
            // this thread exits for any reason (normal close, panic, error).
            struct GuiFlagGuard(Arc<AtomicBool>, Arc<AtomicIsize>);
            impl Drop for GuiFlagGuard {
                fn drop(&mut self) {
                    self.1.store(0, Ordering::Release);
                    self.0.store(false, Ordering::Release);
                    crate::app_events::emit_plugin_chain_changed("gui_close", None);
                    log::debug!("VST2 GUI flag cleared");
                }
            }
            let _guard = GuiFlagGuard(gui_flag, Arc::clone(&gui_hwnd));

            if let Err(e) = win::run_vst2_editor_impl(&editor_arc_clone, &name_owned, &gui_hwnd) {
                log::error!("VST2 GUI error for '{}': {}", name_owned, e);
            }
        })
        .map_err(|e| anyhow!("Failed to spawn VST2 GUI thread: {}", e))?;

    Ok(())
}

// ── Win32 GUI implementation ──────────────────────────────────────────────────

#[cfg(target_os = "windows")]
mod win {
    use super::SendableEditor;
    use anyhow::{anyhow, Result};
    use std::ptr;
    use std::sync::{Arc, Mutex};
    use std::sync::atomic::{AtomicIsize, Ordering};
    #[allow(unused_imports)] // Method dispatch on Box<dyn Editor> vtable works without the trait in scope
    use vst::editor::Editor;
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    pub fn run_vst2_editor_impl(
        editor_arc: &Arc<Mutex<Option<SendableEditor>>>,
        plugin_name: &str,
        gui_hwnd_arc: &Arc<AtomicIsize>,
    ) -> Result<()> {
        // Obtain the editor. We hold the lock only long enough to open the window,
        // then release it so audio-state callers are not blocked.
        let mut editor_guard = editor_arc.lock()
            .map_err(|_| anyhow!("VST2 editor mutex poisoned for '{}'", plugin_name))?;

        let editor = editor_guard.as_mut()
            .ok_or_else(|| anyhow!("'{}' has no VST2 editor", plugin_name))?;

        // Query initial editor size.
        let (mut width, mut height) = editor.0.size();
        if width  <= 0 { width  = 640; }
        if height <= 0 { height = 480; }

        // Create the host window.
        let hwnd = create_host_window(plugin_name, width, height)?;

        // Publish HWND so PluginInstance::drop can post WM_CLOSE.
        gui_hwnd_arc.store(hwnd as isize, Ordering::Release);

        // Open the VST2 editor inside our window (effEditOpen).
        // The plugin creates a child window at coords (0, 0) inside hwnd.
        let opened = editor.0.open(hwnd);
        if !opened {
            unsafe { DestroyWindow(hwnd); }
            return Err(anyhow!("'{}': VST2 effEditOpen returned false", plugin_name));
        }

        // Some plugins report the correct size only after open().
        let (post_w, post_h) = editor.0.size();
        if post_w > 10 && post_h > 10 && (post_w != width || post_h != height) {
            let style = WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
            let mut wr = RECT { left: 0, top: 0, right: post_w, bottom: post_h };
            unsafe {
                AdjustWindowRect(&mut wr, style, 0);
                SetWindowPos(
                    hwnd,
                    ptr::null_mut(),
                    0, 0,
                    wr.right - wr.left,
                    wr.bottom - wr.top,
                    SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
        }

        log::info!("'{}' VST2 GUI opened ({}x{})", plugin_name, width, height);
        unsafe {
            ShowWindow(hwnd, SW_SHOW);
            SetForegroundWindow(hwnd);
        }

        // Release the editor lock before entering the message loop so that
        // other threads (e.g. preset save) are not blocked for the entire
        // runtime of the GUI.
        drop(editor_guard);

        // ── Message loop ──────────────────────────────────────────────────────
        // Standard blocking Win32 loop.  Plugin child-windows get WM_PAINT etc.
        // via DispatchMessageW without any additional idle hacks.
        unsafe {
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        // ── Cleanup ───────────────────────────────────────────────────────────
        // Re-acquire the editor lock to call effEditClose.
        // The plugin should destroy its child window during close().
        if let Ok(mut guard) = editor_arc.lock() {
            if let Some(ref mut ed) = *guard {
                ed.0.close();
            }
        }

        log::debug!("'{}' VST2 GUI cleanup complete", plugin_name);
        Ok(())
    }

    /// Window procedure for the VST2 plugin host window.
    ///
    /// Minimal — VST2 plugins manage their own child windows.
    unsafe extern "system" fn vst2_wnd_proc(
        hwnd: windows_sys::Win32::Foundation::HWND,
        msg: u32,
        wparam: windows_sys::Win32::Foundation::WPARAM,
        lparam: windows_sys::Win32::Foundation::LPARAM,
    ) -> windows_sys::Win32::Foundation::LRESULT {
        match msg {
            WM_DESTROY => {
                PostQuitMessage(0);
                0
            }
            WM_ERASEBKGND => 1, // prevent flicker behind plugin child window
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
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

            Ok(hwnd)
        }
    }
}
