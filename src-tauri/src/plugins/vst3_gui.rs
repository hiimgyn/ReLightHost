/// VST3 GUI hosting — proper IPlugFrame + Win32 message loop.
///
/// Design mirrors LightHost (JUCE DocumentWindow) behaviour:
///   - Native title bar with close/minimise buttons
///   - Resizable when plugin reports canResize() == kResultOk
///   - IPlugFrame host so plugins can resize their own window
///   - Standard Win32 blocking GetMessageW loop (no idle hacks)
///   - WM_SIZE → IPlugView::onSize() for host-driven resizes
///
/// VST3 SDK reference: pluginterfaces/gui/iplugview.h

#[cfg(target_os = "windows")]
pub mod win {
    use anyhow::{anyhow, Result};
    use std::cell::Cell;
    use std::ffi::c_void;
    use std::ptr;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use vst3::{Class, ComPtr, ComWrapper};
    use vst3::Steinberg::{
        kResultOk,
        kPlatformTypeHWND,
        IPlugFrame, IPlugFrameTrait,
        IPlugView, IPlugViewTrait,
        ViewRect,
    };
    use vst3::Steinberg::Vst::{IEditController, IEditControllerTrait};
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};

    // Thread-local state shared between GUI setup, WndProc, and IPlugFrame.
    // Safe because the GUI runs on a single dedicated thread.
    thread_local! {
        /// Host HWND as isize. Used by IPlugFrame::resizeView().
        static TL_HWND: Cell<isize> = Cell::new(0);
        /// Closure that calls IPlugView::onSize() with new client dims.
        /// Captures a live ComPtr<IPlugView> — IPlugViewTrait is only callable
        /// via SmartPtr (ComPtr), NOT via raw *mut IPlugView.
        static TL_ON_SIZE: std::cell::RefCell<Option<Box<dyn Fn(i32, i32)>>> =
            std::cell::RefCell::new(None);
    }

    // ── IPlugFrame host implementation ────────────────────────────────────────
    // Provided to the plugin via view.setFrame() so it can request window
    // resizes (e.g. when the user drags a resize handle inside the plugin UI).
    // resizeView() mirrors JUCE DocumentWindow::resized():
    //   1. Resize the outer Win32 frame → Windows sends WM_SIZE synchronously
    //   2. WndProc WM_SIZE calls IPlugView::onSize() to confirm the new dims
    struct HostPlugFrame;
    unsafe impl Send for HostPlugFrame {}
    unsafe impl Sync for HostPlugFrame {}

    impl Class for HostPlugFrame {
        type Interfaces = (IPlugFrame,);
    }

    #[allow(non_snake_case)]
    impl IPlugFrameTrait for HostPlugFrame {
        unsafe fn resizeView(
            &self,
            _view: *mut IPlugView,
            new_size: *mut ViewRect,
        ) -> i32 {
            if new_size.is_null() {
                return -1; // kInvalidArgument
            }
            let rect = *new_size;
            let content_w = (rect.right  - rect.left).max(100);
            let content_h = (rect.bottom - rect.top ).max(100);

            TL_HWND.with(|cell| {
                let hwnd = cell.get() as windows_sys::Win32::Foundation::HWND;
                if hwnd.is_null() { return; }
                // AdjustWindowRect converts client-area size to outer frame size.
                // SetWindowPos sends WM_SIZE synchronously → WndProc → onSize().
                let style = WS_OVERLAPPEDWINDOW & !WS_MAXIMIZEBOX;
                let mut wr = RECT { left: 0, top: 0, right: content_w, bottom: content_h };
                AdjustWindowRect(&mut wr, style, 0);
                SetWindowPos(
                    hwnd,
                    ptr::null_mut(),
                    0, 0,
                    wr.right  - wr.left,
                    wr.bottom - wr.top,
                    SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                );
            });

            kResultOk
        }
    }

    // ── Public entry point ────────────────────────────────────────────────────

    pub fn open_gui_window(
        controller: &ComPtr<IEditController>,
        plugin_name: &str,
        gui_flag: Arc<AtomicBool>,
    ) -> Result<()> {
        let controller_clone = controller.clone();
        let name_owned = plugin_name.to_string();

        std::thread::Builder::new()
            .name(format!("vst3-gui-{}", plugin_name))
            .spawn(move || {
                struct GuiFlagGuard(Arc<AtomicBool>);
                impl Drop for GuiFlagGuard {
                    fn drop(&mut self) {
                        self.0.store(false, Ordering::Release);
                        log::debug!("GUI flag cleared");
                    }
                }
                let _guard = GuiFlagGuard(gui_flag);

                if let Err(e) = run_gui_window_impl(&controller_clone, &name_owned) {
                    log::error!("VST3 GUI error for '{}': {}", name_owned, e);
                }
            })
            .map_err(|e| anyhow!("Failed to spawn GUI thread: {}", e))?;

        Ok(())
    }

    // ── GUI window lifecycle ──────────────────────────────────────────────────

    fn run_gui_window_impl(
        controller: &ComPtr<IEditController>,
        plugin_name: &str,
    ) -> Result<()> {
        use std::ffi::CString;

        // COM must be initialised on the same thread as the GUI (same as JUCE
        // ScopedCoInitialiser with COINIT_APARTMENTTHREADED).
        struct ComScope(bool);
        impl Drop for ComScope {
            fn drop(&mut self) {
                if self.0 { unsafe { CoUninitialize(); } }
            }
        }
        let _com = ComScope({
            let hr = unsafe { CoInitializeEx(ptr::null(), COINIT_APARTMENTTHREADED as u32) };
            hr == 0_i32 || hr == 1_i32
        });

        // 1. Create the editor view
        let view_name = CString::new("editor").map_err(|_| anyhow!("CString error"))?;
        let view_raw = unsafe { controller.createView(view_name.as_ptr()) };
        if view_raw.is_null() {
            return Err(anyhow!("'{}' returned null IPlugView", plugin_name));
        }
        let view = unsafe {
            ComPtr::<IPlugView>::from_raw(view_raw)
                .ok_or_else(|| anyhow!("Failed to wrap IPlugView for '{}'", plugin_name))?
        };

        // 2. Check HWND platform support
        if unsafe { view.isPlatformTypeSupported(kPlatformTypeHWND) } != kResultOk {
            return Err(anyhow!("'{}' does not support HWND platform", plugin_name));
        }

        // 3. Query initial size
        let mut view_rect = ViewRect { left: 0, top: 0, right: 800, bottom: 600 };
        unsafe { view.getSize(&mut view_rect) };
        let width  = (view_rect.right  - view_rect.left).max(400);
        let height = (view_rect.bottom - view_rect.top ).max(300);

        // 4. Choose window style based on canResize()
        // Mirrors LightHost: PluginWindow::setResizable(canResize()).
        let can_resize = unsafe { view.canResize() } == kResultOk;
        let window_style = if can_resize {
            WS_OVERLAPPEDWINDOW & !WS_MAXIMIZEBOX
        } else {
            WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX
        };
        log::debug!("'{}': initial {}x{}, can_resize={}", plugin_name, width, height, can_resize);

        // 5. Create the host Win32 window
        let hwnd = create_host_window(plugin_name, width, height, window_style)?;

        // 6. Publish HWND for IPlugFrame and install the onSize callback.
        // IPlugViewTrait requires SmartPtr (ComPtr), not raw *mut IPlugView,
        // so capture a clone of the ComPtr in a closure.
        TL_HWND.set(hwnd as isize);
        let view_for_size = view.clone();
        TL_ON_SIZE.with(|cell| {
            *cell.borrow_mut() = Some(Box::new(move |w: i32, h: i32| {
                let mut rect = ViewRect { left: 0, top: 0, right: w, bottom: h };
                unsafe { view_for_size.onSize(&mut rect); }
            }));
        });

        // 7. Attach IPlugFrame so plugin can call resizeView()
        // Mirrors JUCE's PluginWindow passing itself as its ComponentListener.
        let frame = ComWrapper::new(HostPlugFrame);
        if let Some(frame_ptr) = frame.to_com_ptr::<IPlugFrame>() {
            unsafe { view.setFrame(frame_ptr.as_ptr()); }
        }

        // 8. Attach plugin view to our HWND
        let attach_res = unsafe { view.attached(hwnd, kPlatformTypeHWND) };
        if attach_res != kResultOk {
            log::warn!("'{}': IPlugView::attached returned {}", plugin_name, attach_res);
        }

        // 9. Confirm size — many plugins don't render until onSize() is called
        let mut client_rect = ViewRect { left: 0, top: 0, right: width, bottom: height };
        let size_res = unsafe { view.onSize(&mut client_rect) };
        if size_res != kResultOk {
            log::warn!("'{}': IPlugView::onSize returned {}", plugin_name, size_res);
        }

        log::info!("'{}' GUI opened ({}x{}, resizable={})", plugin_name, width, height, can_resize);
        unsafe {
            ShowWindow(hwnd, SW_SHOW);
            SetForegroundWindow(hwnd);
        }
        // Brief settle — some plugins create child windows during attached()
        std::thread::sleep(std::time::Duration::from_millis(10));

        // 10. Message loop — blocks until the window is closed
        // Standard Win32 blocking loop, identical to JUCE on Windows.
        // Plugin child windows receive WM_TIMER / WM_PAINT via DispatchMessageW.
        // No explicit host idle callback is required for VST3 on Windows.
        run_message_loop();

        // 11. Cleanup (window already destroyed by WM_CLOSE -> DefWindowProc)
        unsafe { view.setFrame(ptr::null_mut()); }
        let _ = unsafe { view.removed() };
        // Clear thread-locals — releases the ComPtr clone before view drops
        TL_ON_SIZE.with(|cell| *cell.borrow_mut() = None);
        TL_HWND.set(0);

        log::debug!("'{}' GUI cleanup complete", plugin_name);
        Ok(())
    }

    // ── Win32 helpers ─────────────────────────────────────────────────────────

    /// Window procedure for the plugin host window.
    ///
    /// WM_SIZE calls IPlugView::onSize() so the plugin can adjust its layout
    /// when the user (or IPlugFrame::resizeView) resizes the outer window.
    unsafe extern "system" fn host_wnd_proc(
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
            WM_SIZE if wparam != 1 => {
                // wparam == 1 is SIZE_MINIMIZED — skip onSize for minimise
                let client_w = (lparam & 0xFFFF) as i32;
                let client_h = ((lparam >> 16) & 0xFFFF) as i32;
                if client_w > 0 && client_h > 0 {
                    TL_ON_SIZE.with(|cell| {
                        if let Some(ref f) = *cell.borrow() {
                            f(client_w, client_h);
                        }
                    });
                }
                0
            }
            WM_ERASEBKGND => 1, // prevent background flicker behind plugin UI
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    fn create_host_window(
        title: &str,
        width: i32,
        height: i32,
        style: u32,
    ) -> Result<*mut c_void> {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        let class_name: Vec<u16> = OsStr::new("ReLightHostVST3")
            .encode_wide().chain(Some(0)).collect();
        let window_title: Vec<u16> = OsStr::new(title)
            .encode_wide().chain(Some(0)).collect();

        unsafe {
            let hinstance = GetModuleHandleW(ptr::null());

            let wc = WNDCLASSW {
                style: CS_DBLCLKS,
                lpfnWndProc: Some(host_wnd_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: hinstance,
                hIcon: ptr::null_mut(),
                hCursor: LoadCursorW(ptr::null_mut(), IDC_ARROW),
                hbrBackground: 6 as _, // COLOR_WINDOW + 1
                lpszMenuName: ptr::null(),
                lpszClassName: class_name.as_ptr(),
            };
            // RegisterClassW fails silently if the class is already registered
            RegisterClassW(&wc);

            // AdjustWindowRect ensures client area matches the plugin dimensions
            let mut rect = RECT { left: 0, top: 0, right: width, bottom: height };
            AdjustWindowRect(&mut rect, style, 0);

            let hwnd = CreateWindowExW(
                0,
                class_name.as_ptr(),
                window_title.as_ptr(),
                style,
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
            Ok(hwnd)
        }
    }

    /// Standard Win32 blocking message loop.
    ///
    /// GetMessageW returns 0 for WM_QUIT (posted by WndProc on WM_DESTROY)
    /// and -1 on error.
    fn run_message_loop() {
        unsafe {
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub mod win {
    use anyhow::{anyhow, Result};
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use vst3::ComPtr;
    use vst3::Steinberg::Vst::IEditController;

    pub fn open_gui_window(
        _controller: &ComPtr<IEditController>,
        plugin_name: &str,
        _gui_flag: Arc<AtomicBool>,
    ) -> Result<()> {
        log::warn!("VST3 GUI launching only supported on Windows. Plugin: {}", plugin_name);
        Err(anyhow!("Plugin GUI launching not supported on this platform"))
    }
}
