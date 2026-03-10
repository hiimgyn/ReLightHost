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
    use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
    use std::sync::Arc;
    use vst3::{Class, ComPtr, ComWrapper};
    use vst3::Steinberg::{
        kResultOk,
        kPlatformTypeHWND,
        IPlugFrame, IPlugFrameTrait,
        IPlugView, IPlugViewTrait,
        ViewRect,
    };
    use vst3::Steinberg::Vst::{
        IComponent, IComponentTrait,
        IConnectionPoint, IConnectionPointTrait,
        IEditController, IEditControllerTrait,
    };
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
        static TL_ON_SIZE: std::cell::RefCell<Option<Box<dyn Fn(i32, i32)>>> =
            std::cell::RefCell::new(None);
        /// Initial (width, height) applied via WM_VST_ATTACH once the loop runs.
        static TL_INITIAL_SIZE: Cell<(i32, i32)> = Cell::new((0, 0));
        /// The IPlugView clone to be attached inside the message loop.
        /// Stored by setup, consumed exactly once by the WM_VST_ATTACH handler.
        static TL_PENDING_VIEW: std::cell::RefCell<Option<ComPtr<IPlugView>>> =
            std::cell::RefCell::new(None);
    }

    /// Posted BEFORE starting the message loop.
    /// The handler calls IPlugView::attached(), onSize(), and ShowWindow() so
    /// that these run while the pump is active — this prevents deadlocks caused
    /// by plugins that SendMessage back to our HWND from their own threads
    /// during attached() initialization (e.g. JUCE-based plugins).
    const WM_VST_ATTACH: u32 = WM_USER + 1;

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
        component: &ComPtr<IComponent>,
        plugin_name: &str,
        gui_flag: Arc<AtomicBool>,
        gui_hwnd: Arc<AtomicIsize>,
    ) -> Result<()> {
        let controller_clone = controller.clone();
        let component_clone  = component.clone();
        let name_owned = plugin_name.to_string();

        std::thread::Builder::new()
            .name(format!("vst3-gui-{}", plugin_name))
            .spawn(move || {
                struct GuiFlagGuard(Arc<AtomicBool>, Arc<AtomicIsize>);
                impl Drop for GuiFlagGuard {
                    fn drop(&mut self) {
                        // Clear HWND first so PluginInstance::drop's PostMessageW
                        // is not duplicated if the window was already closed.
                        self.1.store(0, Ordering::Release);
                        self.0.store(false, Ordering::Release);
                        log::debug!("GUI flag cleared");
                    }
                }
                let _guard = GuiFlagGuard(gui_flag, Arc::clone(&gui_hwnd));

                if let Err(e) = run_gui_window_impl(&controller_clone, &component_clone, &name_owned, &gui_hwnd) {
                    log::error!("VST3 GUI error for '{}': {}", name_owned, e);
                }
            })
            .map_err(|e| anyhow!("Failed to spawn GUI thread: {}", e))?;

        Ok(())
    }

    // ── GUI window lifecycle ──────────────────────────────────────────────────

    fn run_gui_window_impl(
        controller: &ComPtr<IEditController>,
        component:  &ComPtr<IComponent>,
        plugin_name: &str,
        gui_hwnd_arc: &Arc<AtomicIsize>,
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

        // ── Pre-createView setup (VST3 hosting protocol) ─────────────────────
        // 1. Connect component ↔ controller via IConnectionPoint so the plugin
        //    can sync internal state.  Done here (lazily on the GUI thread)
        //    rather than during load() to avoid racing with the audio callback,
        //    which would corrupt the stack on non-thread-safe plugins.
        if let (Some(comp_cp), Some(ctrl_cp)) = (
            component.cast::<IConnectionPoint>(),
            controller.cast::<IConnectionPoint>(),
        ) {
            unsafe {
                let _ = comp_cp.connect(ctrl_cp.as_ptr());
                let _ = ctrl_cp.connect(comp_cp.as_ptr());
            }
            log::debug!("'{}': IConnectionPoint connected (component ↔ controller)", plugin_name);
        }

        // 2. Sync controller parameters to the component's current state.
        //    Many plugins (e.g. HY-Delay4) require this before createView.
        {
            use vst3::Steinberg::IBStream;
            use vst3::ComWrapper;
            // Reuse the same IBStream type from the outer module via a local alias.
            // We just need something that writes into a Vec.
            struct WriteBuf {
                buf: std::cell::RefCell<Vec<u8>>,
                pos: std::cell::Cell<usize>,
            }
            impl vst3::Class for WriteBuf {
                type Interfaces = (IBStream,);
            }
            #[allow(non_snake_case)]
            impl vst3::Steinberg::IBStreamTrait for WriteBuf {
                unsafe fn read(&self, buffer: *mut std::ffi::c_void, numBytes: i32, numBytesRead: *mut i32) -> i32 {
                    let borrow = self.buf.borrow();
                    let n = (numBytes.max(0)) as usize;
                    let avail = borrow.len().saturating_sub(self.pos.get());
                    let to_read = n.min(avail);
                    if to_read > 0 {
                        std::ptr::copy_nonoverlapping(
                            borrow[self.pos.get()..].as_ptr(),
                            buffer as *mut u8,
                            to_read,
                        );
                        self.pos.set(self.pos.get() + to_read);
                    }
                    if !numBytesRead.is_null() { *numBytesRead = to_read as i32; }
                    0
                }
                unsafe fn write(&self, buffer: *mut std::ffi::c_void, numBytes: i32, numBytesWritten: *mut i32) -> i32 {
                    let mut borrow = self.buf.borrow_mut();
                    let n = (numBytes.max(0)) as usize;
                    let pos = self.pos.get();
                    let end = pos + n;
                    if end > borrow.len() { borrow.resize(end, 0); }
                    std::ptr::copy_nonoverlapping(buffer as *const u8, borrow[pos..end].as_mut_ptr(), n);
                    self.pos.set(end);
                    if !numBytesWritten.is_null() { *numBytesWritten = n as i32; }
                    0
                }
                unsafe fn seek(&self, pos: i64, mode: i32, result: *mut i64) -> i32 {
                    let len = self.buf.borrow().len();
                    let new_pos = match mode {
                        0 => pos.max(0) as usize,
                        1 => (self.pos.get() as i64 + pos).max(0) as usize,
                        2 => (len as i64 + pos).max(0) as usize,
                        _ => return 0x80004005u32 as i32,
                    };
                    self.pos.set(new_pos);
                    if !result.is_null() { *result = new_pos as i64; }
                    0
                }
                unsafe fn tell(&self, pos: *mut i64) -> i32 {
                    if !pos.is_null() { *pos = self.pos.get() as i64; }
                    0
                }
            }
            let ws = ComWrapper::new(WriteBuf {
                buf: std::cell::RefCell::new(Vec::new()),
                pos: std::cell::Cell::new(0),
            });
            if let Some(state_ptr) = ws.to_com_ptr::<IBStream>() {
                unsafe { component.getState(state_ptr.as_ptr()); }
                // Rewind for reading
                ws.pos.set(0);
                if let Some(read_ptr) = ws.to_com_ptr::<IBStream>() {
                    unsafe { controller.setComponentState(read_ptr.as_ptr()); }
                }
            }
        }

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

        // 5. Create the host Win32 window (hidden until the message loop fires).
        let hwnd = create_host_window(plugin_name, width, height, window_style)?;
        // Publish HWND so PluginInstance::drop can post WM_CLOSE before
        // Vst3Processor::terminate() runs, preventing STATUS_ACCESS_VIOLATION.
        gui_hwnd_arc.store(hwnd as isize, Ordering::Release);

        // 6. Install the onSize callback and store initial dimensions.
        TL_HWND.set(hwnd as isize);
        let view_for_size = view.clone();
        TL_ON_SIZE.with(|cell| {
            *cell.borrow_mut() = Some(Box::new(move |w: i32, h: i32| {
                let mut rect = ViewRect { left: 0, top: 0, right: w, bottom: h };
                unsafe { view_for_size.onSize(&mut rect); }
            }));
        });
        TL_INITIAL_SIZE.with(|c| c.set((width, height)));

        // 7. Attach IPlugFrame so the plugin can call resizeView().
        let frame = ComWrapper::new(HostPlugFrame);
        if let Some(frame_ptr) = frame.to_com_ptr::<IPlugFrame>() {
            unsafe { view.setFrame(frame_ptr.as_ptr()); }
        }

        // 8. Store a clone of the view for the WM_VST_ATTACH handler and post
        //    the attach message.  The handler will call IPlugView::attached(),
        //    onSize(), and ShowWindow() while the message pump is already active.
        //    This prevents deadlocks: if the plugin spawns threads during
        //    attached() that SendMessage to our HWND, those sends are serviced
        //    by the running pump instead of blocking the GUI thread.
        TL_PENDING_VIEW.with(|c| *c.borrow_mut() = Some(view.clone()));
        unsafe { PostMessageW(hwnd, WM_VST_ATTACH, 0, 0); }

        log::info!("'{}' entering GUI loop ({}x{}, resizable={})", plugin_name, width, height, can_resize);

        // 9. Message loop — blocks until WM_QUIT (posted on WM_DESTROY).
        //    WM_VST_ATTACH fires first (attached → onSize → ShowWindow).
        run_message_loop();

        // 10. Cleanup — window destroyed by WM_CLOSE → DefWindowProc.
        unsafe { view.setFrame(ptr::null_mut()); }
        let _ = unsafe { view.removed() };
        // Clear thread-locals — releases ComPtr clones before view drops.
        // (TL_PENDING_VIEW may still hold a clone if WM_VST_ATTACH never fired.)
        TL_PENDING_VIEW.with(|c| *c.borrow_mut() = None);
        TL_ON_SIZE.with(|cell| *cell.borrow_mut() = None);
        TL_INITIAL_SIZE.with(|c| c.set((0, 0)));
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
            // First message in the loop: call IPlugView::attached() while the
            // pump is running, then apply initial size and reveal the window.
            // Running attached() here (not before the loop) ensures any
            // cross-thread SendMessage calls from the plugin's init threads are
            // serviced without deadlocking the GUI thread.
            m if m == WM_VST_ATTACH => {
                let view_opt = TL_PENDING_VIEW.with(|c| c.borrow_mut().take());
                if let Some(view) = view_opt {
                    let attach_res = view.attached(hwnd, kPlatformTypeHWND);
                    if attach_res != 0 {
                        // kResultOk == 0; non-zero means failure.  Many plugins
                        // still render correctly — warn and continue.
                        log::warn!(
                            "VST3 IPlugView::attached() returned {} (continuing)",
                            attach_res
                        );
                    }
                    let (w, h) = TL_INITIAL_SIZE.with(|c| c.get());
                    if w > 0 && h > 0 {
                        TL_ON_SIZE.with(|cell| {
                            if let Some(ref f) = *cell.borrow() { f(w, h); }
                        });
                    }
                } else {
                    log::warn!("WM_VST_ATTACH fired but no pending view — closing");
                    PostQuitMessage(1);
                }
                ShowWindow(hwnd, SW_SHOW);
                SetForegroundWindow(hwnd);
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

            // Load the app icon embedded in the exe by tauri_build (resource ID 1).
            let hicon = LoadIconW(hinstance, 1 as _);

            let wc = WNDCLASSW {
                style: CS_DBLCLKS,
                lpfnWndProc: Some(host_wnd_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: hinstance,
                hIcon: hicon,
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

            // Set both the big (title bar) and small (taskbar) icons.
            if !hicon.is_null() {
                SendMessageW(hwnd, WM_SETICON, ICON_BIG as _, hicon as _);
                SendMessageW(hwnd, WM_SETICON, ICON_SMALL as _, hicon as _);
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
