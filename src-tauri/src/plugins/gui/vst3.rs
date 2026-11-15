//! VST3 editor hosting — Win32 host window + `IPlugView` embedding.
//!
//! VST3 makes every IEditController / IPlugView call a UI-thread call, and
//! the UI thread is the one that created the plugin: the plugin host thread
//! (see core::host_thread). JUCE plugins bind their MessageManager (hidden
//! message window, timers, async callbacks) to it, so an editor driven from
//! any other thread races its own timers — the Xvox/Clear crashes. The
//! editor therefore lives on the host thread, driven by its message pump,
//! like CLAP's (gui/clap.rs); there is no per-editor thread.
//!   1. `createView` → `setFrame` → create the host window
//!   2. show → `setContentScaleFactor` → `attached` → fit window to `getSize`
//!   3. WM_CLOSE: `setFrame(null)` → `removed` → destroy the host window
//!      (the view goes before its parent window does)
//!   4. WM_NCDESTROY: release the view, then clear the GUI-open flag / HWND
//!
//! Component ↔ controller connection and state sync happen at load
//! (processor::vst3), not here.
#[cfg(target_os = "windows")]
pub mod win {
    use anyhow::{anyhow, Result};
    use std::cell::Cell;
    use std::ptr;
    use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
    use std::sync::Arc;
    use vst3::{Class, ComPtr, ComWrapper};
    use vst3::Steinberg::{
        kInvalidArgument, kPlatformTypeHWND, kResultFalse, kResultOk,
        IPlugFrame, IPlugFrameTrait,
        IPlugView, IPlugViewTrait,
        IPlugViewContentScaleSupport, IPlugViewContentScaleSupportTrait,
        ViewRect,
    };
    use vst3::Steinberg::Vst::{IEditController, IEditControllerTrait};
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    /// Moves the plugin's COM pointers into `host_thread::run_blocking`.
    /// Callers (launch_plugin) are already on the host thread, where it runs
    /// inline, so in practice nothing crosses threads.
    struct OnHostThread<T>(T);
    unsafe impl<T> Send for OnHostThread<T> {}
    impl<T> OnHostThread<T> {
        fn into_inner(self) -> T { self.0 }
    }

    // ── IPlugFrame: lets the plugin resize its own window ─────────────────────
    struct HostPlugFrame {
        /// Host window; 0 once the view is detached (late calls are no-ops).
        hwnd: AtomicIsize,
    }

    impl Class for HostPlugFrame {
        type Interfaces = (IPlugFrame,);
    }

    #[allow(non_snake_case)]
    impl IPlugFrameTrait for HostPlugFrame {
        unsafe fn resizeView(&self, _view: *mut IPlugView, new_size: *mut ViewRect) -> i32 {
            if new_size.is_null() {
                return kInvalidArgument;
            }
            let hwnd = self.hwnd.load(Ordering::Acquire) as HWND;
            if hwnd.is_null() {
                return kResultFalse;
            }
            let r = *new_size;
            // WM_SIZE (sent synchronously, same thread) calls view.onSize().
            resize_client(hwnd, r.right - r.left, r.bottom - r.top);
            kResultOk
        }
    }

    /// Per-window state, owned by the window (GWLP_USERDATA) and freed on
    /// WM_NCDESTROY.
    struct Editor {
        view: ComPtr<IPlugView>,
        frame: ComWrapper<HostPlugFrame>,
        /// Kept until the view is released: the view belongs to it.
        _controller: ComPtr<IEditController>,
        attached: Cell<bool>,
        closed: Cell<bool>,
        gui_flag: Arc<AtomicBool>,
        gui_hwnd: Arc<AtomicIsize>,
    }

    impl Editor {
        /// `setFrame(null)` + `removed()`, while the parent window still exists.
        unsafe fn close_view(&self) {
            if self.closed.replace(true) {
                return;
            }
            self.frame.hwnd.store(0, Ordering::Release);
            self.view.setFrame(ptr::null_mut());
            if self.attached.replace(false) {
                self.view.removed();
            }
        }
    }

    unsafe fn editor<'a>(hwnd: HWND) -> Option<&'a Editor> {
        (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Editor).as_ref()
    }

    // ── Public entry point ────────────────────────────────────────────────────

    /// Creates, attaches and shows the editor on the plugin host thread and
    /// returns once it is up. The window owns it from then on.
    pub fn open_gui_window(
        controller: &ComPtr<IEditController>,
        plugin_name: &str,
        gui_flag: Arc<AtomicBool>,
        gui_hwnd: Arc<AtomicIsize>,
    ) -> Result<()> {
        let controller = OnHostThread(controller.clone());
        let name = plugin_name.to_string();
        crate::core::host_thread::run_blocking(move || {
            let controller = controller.into_inner();
            unsafe { open(&controller, &name, gui_flag, gui_hwnd) }
        })
    }

    unsafe fn open(
        controller: &ComPtr<IEditController>,
        name: &str,
        gui_flag: Arc<AtomicBool>,
        gui_hwnd: Arc<AtomicIsize>,
    ) -> Result<()> {
        let view_raw = controller.createView(c"editor".as_ptr());
        let view = ComPtr::<IPlugView>::from_raw(view_raw)
            .ok_or_else(|| anyhow!("'{}' has no editor (createView returned null)", name))?;
        if view.isPlatformTypeSupported(kPlatformTypeHWND) != kResultOk {
            return Err(anyhow!("'{}' does not support HWND platform", name));
        }

        let mut rect = ViewRect { left: 0, top: 0, right: 800, bottom: 600 };
        view.getSize(&mut rect);
        let (w, h) = ((rect.right - rect.left).max(1), (rect.bottom - rect.top).max(1));
        let can_resize = view.canResize() == kResultOk;
        let style = if can_resize {
            WS_OVERLAPPEDWINDOW & !WS_MAXIMIZEBOX
        } else {
            WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX
        };
        let hwnd = create_host_window(name, w, h, style)?;

        // From here on the window owns the editor: any failure below goes
        // through DestroyWindow → WM_NCDESTROY, which detaches the view and
        // clears the flags.
        gui_hwnd.store(hwnd as isize, Ordering::Release);
        let frame = ComWrapper::new(HostPlugFrame { hwnd: AtomicIsize::new(hwnd as isize) });
        if let Some(f) = frame.to_com_ptr::<IPlugFrame>() {
            view.setFrame(f.as_ptr());
        }
        let ed = Box::new(Editor {
            view: view.clone(),
            frame,
            _controller: controller.clone(),
            attached: Cell::new(false),
            closed: Cell::new(false),
            gui_flag,
            gui_hwnd,
        });
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(ed) as isize);

        // Visible before attached(): JUCE 7+ Direct2D editors check
        // IsWindowVisible(parent) there and skip creating their renderer.
        ShowWindow(hwnd, SW_SHOW);
        SetForegroundWindow(hwnd);
        set_scale(&view, GetDpiForWindow(hwnd));

        let res = view.attached(hwnd, kPlatformTypeHWND);
        // Looked up again: the plugin may have pumped messages inside
        // attached() (e.g. a licence dialog) and the window closed meanwhile.
        match editor(hwnd) {
            Some(ed) if res == kResultOk => ed.attached.set(true),
            Some(_) => {
                DestroyWindow(hwnd);
                return Err(anyhow!("'{}' refused to attach its editor ({:#x})", name, res));
            }
            None => return Err(anyhow!("'{}' editor was closed while opening", name)),
        }

        // Scale/attach may have changed the editor's size; then give the
        // view its final client size (WM_SIZE is skipped when unchanged).
        fit_to_view(hwnd, &view);
        let mut client: RECT = std::mem::zeroed();
        GetClientRect(hwnd, &mut client);
        let mut r = ViewRect { left: 0, top: 0, right: client.right, bottom: client.bottom };
        view.onSize(&mut r);

        log::info!("'{}' editor open ({}x{}, resizable={})", name, client.right, client.bottom, can_resize);
        Ok(())
    }

    // ── Win32 helpers ─────────────────────────────────────────────────────────

    unsafe fn set_scale(view: &ComPtr<IPlugView>, dpi: u32) {
        if dpi == 0 {
            return;
        }
        if let Some(s) = view.cast::<IPlugViewContentScaleSupport>() {
            s.setContentScaleFactor(dpi as f32 / 96.0);
        }
    }

    unsafe fn fit_to_view(hwnd: HWND, view: &ComPtr<IPlugView>) {
        let mut r = ViewRect { left: 0, top: 0, right: 0, bottom: 0 };
        if view.getSize(&mut r) == kResultOk && r.right > r.left && r.bottom > r.top {
            resize_client(hwnd, r.right - r.left, r.bottom - r.top);
        }
    }

    /// Resizes `hwnd` so its client area is `w`×`h`. The frame is measured
    /// from the live window: AdjustWindowRect uses the system DPI's metrics,
    /// which are wrong on a monitor with another scale (per-monitor v2).
    unsafe fn resize_client(hwnd: HWND, w: i32, h: i32) {
        let (mut wr, mut cr): (RECT, RECT) = (std::mem::zeroed(), std::mem::zeroed());
        GetWindowRect(hwnd, &mut wr);
        GetClientRect(hwnd, &mut cr);
        let frame_w = (wr.right - wr.left) - cr.right;
        let frame_h = (wr.bottom - wr.top) - cr.bottom;
        SetWindowPos(
            hwnd, ptr::null_mut(), 0, 0,
            w.max(1) + frame_w, h.max(1) + frame_h,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }

    unsafe extern "system" fn host_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        match msg {
            WM_SIZE => {
                // wparam 1 = SIZE_MINIMIZED
                let (w, h) = ((lparam & 0xffff) as i32, ((lparam >> 16) & 0xffff) as i32);
                if let Some(ed) = editor(hwnd) {
                    if wparam != 1 && w > 0 && h > 0 && ed.attached.get() {
                        let mut r = ViewRect { left: 0, top: 0, right: w, bottom: h };
                        ed.view.onSize(&mut r);
                    }
                }
                0
            }
            WM_DPICHANGED => {
                // Moved to a monitor with another scale: tell the view, then
                // size the window to whatever the view now wants.
                if let Some(ed) = editor(hwnd) {
                    set_scale(&ed.view, (wparam & 0xffff) as u32);
                    fit_to_view(hwnd, &ed.view);
                }
                0
            }
            WM_CLOSE => {
                if let Some(ed) = editor(hwnd) {
                    ShowWindow(hwnd, SW_HIDE);
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
                    // Releases the view, then the controller reference —
                    // before the flag tells PluginInstance it may tear down.
                    drop(ed);
                    gui_hwnd.store(0, Ordering::Release);
                    flag.store(false, Ordering::Release);
                    crate::app_events::emit_plugin_chain_changed("gui_close", None);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_ERASEBKGND => 1, // the plugin paints the whole client area
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    fn create_host_window(title: &str, width: i32, height: i32, style: u32) -> Result<HWND> {
        let class_name: Vec<u16> = "ReLightHostVST3".encode_utf16().chain(Some(0)).collect();
        let window_title: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();

        unsafe {
            let hinstance = GetModuleHandleW(ptr::null());
            // App icon embedded in the exe by tauri_build (resource ID 32512).
            let hicon = LoadIconW(hinstance, 32512 as _);
            let wc = WNDCLASSW {
                style: CS_DBLCLKS,
                lpfnWndProc: Some(host_wnd_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: hinstance,
                hIcon: hicon,
                hCursor: LoadCursorW(ptr::null_mut(), IDC_ARROW),
                hbrBackground: ptr::null_mut(),
                lpszMenuName: ptr::null(),
                lpszClassName: class_name.as_ptr(),
            };
            RegisterClassW(&wc); // fails harmlessly if already registered

            let mut rect = RECT { left: 0, top: 0, right: width, bottom: height };
            AdjustWindowRect(&mut rect, style, 0);
            let hwnd = CreateWindowExW(
                0,
                class_name.as_ptr(),
                window_title.as_ptr(),
                style | WS_CLIPCHILDREN,
                CW_USEDEFAULT, CW_USEDEFAULT,
                rect.right - rect.left,
                rect.bottom - rect.top,
                ptr::null_mut(),
                ptr::null_mut(),
                hinstance,
                ptr::null(),
            );
            if hwnd.is_null() {
                return Err(anyhow!("CreateWindowExW failed for '{}'", title));
            }
            if !hicon.is_null() {
                SendMessageW(hwnd, WM_SETICON, ICON_BIG as _, hicon as _);
                SendMessageW(hwnd, WM_SETICON, ICON_SMALL as _, hicon as _);
            }
            Ok(hwnd)
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub mod win {
    use anyhow::{anyhow, Result};
    use std::sync::atomic::{AtomicBool, AtomicIsize};
    use std::sync::Arc;
    use vst3::ComPtr;
    use vst3::Steinberg::Vst::IEditController;

    pub fn open_gui_window(
        _controller: &ComPtr<IEditController>,
        plugin_name: &str,
        _gui_flag: Arc<AtomicBool>,
        _gui_hwnd: Arc<AtomicIsize>,
    ) -> Result<()> {
        log::warn!("VST3 GUI launching only supported on Windows. Plugin: {}", plugin_name);
        Err(anyhow!("Plugin GUI launching not supported on this platform"))
    }
}
