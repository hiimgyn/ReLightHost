/// VST3 plugin GUI launcher for Windows
///
/// Opens the plugin's native editor window by loading the VST3 DLL,
/// creating the plugin instance, and attaching its IPlugView to a Win32 window.

use anyhow::{anyhow, Result};
use std::path::Path;

#[cfg(target_os = "windows")]
pub fn launch_vst3_gui(plugin_path: &str, plugin_name: &str) -> Result<()> {
    let path = Path::new(plugin_path);
    if !path.exists() {
        return Err(anyhow!("Plugin file not found: {}", plugin_path));
    }

    log::info!("Launching VST3 GUI: {} at {}", plugin_name, plugin_path);

    // Spawn on a dedicated thread - VST3 GUI and COM must run on the same thread,
    // and we need a Win32 message loop there.
    let path_owned = plugin_path.to_string();
    let name_owned = plugin_name.to_string();

    std::thread::Builder::new()
        .name(format!("vst3-gui-{}", plugin_name))
        .spawn(move || {
            if let Err(e) = run_vst3_window(&path_owned, &name_owned) {
                log::error!("VST3 GUI error for {}: {}", name_owned, e);
            }
        })
        .map_err(|e| anyhow!("Failed to spawn GUI thread: {}", e))?;

    Ok(())
}

#[cfg(target_os = "windows")]
fn run_vst3_window(plugin_path: &str, plugin_name: &str) -> Result<()> {
    use std::ffi::CString;
    use std::ptr;
    use libloading::{Library, Symbol};

    use vst3::Steinberg::{
        kResultOk, kPlatformTypeHWND,
        IPluginFactory, IPluginFactoryTrait,
        IPluginBaseTrait,
        IPlugView, IPlugViewTrait,
        PClassInfo,
    };
    use vst3::Steinberg::Vst::{
        IComponent, IComponentTrait,
        IEditController, IEditControllerTrait,
    };
    use vst3::{ComPtr, Interface};

    // Load DLL
    let lib = unsafe { Library::new(plugin_path) }
        .map_err(|e| anyhow!("Failed to load plugin DLL '{}': {}", plugin_path, e))?;

    // Optional InitDll() call
    type BoolFn = unsafe extern "system" fn() -> bool;
    if let Ok(init_dll) = unsafe { lib.get::<BoolFn>(b"InitDll\0") } {
        if !unsafe { init_dll() } {
            log::warn!("InitDll() returned false for '{}'", plugin_name);
        }
    }

    // Get factory
    type GetPluginFactory = unsafe extern "system" fn() -> *mut IPluginFactory;
    let get_factory: Symbol<GetPluginFactory> = unsafe { lib.get(b"GetPluginFactory\0") }
        .map_err(|e| anyhow!("Plugin '{}' has no GetPluginFactory export: {}", plugin_path, e))?;

    let factory_ptr = unsafe { get_factory() };
    if factory_ptr.is_null() {
        return Err(anyhow!("GetPluginFactory returned null"));
    }
    let factory = unsafe {
        ComPtr::<IPluginFactory>::from_raw(factory_ptr)
            .ok_or_else(|| anyhow!("Failed to wrap IPluginFactory"))?
    };

    // Scan factory classes
    let class_count = unsafe { factory.countClasses() };
    log::info!("Plugin '{}' has {} class(es)", plugin_name, class_count);

    let mut audio_module_cid: Option<vst3::Steinberg::TUID> = None;
    let mut factory_ctrl_cid: Option<vst3::Steinberg::TUID> = None;

    for i in 0..class_count {
        let mut ci: PClassInfo = unsafe { std::mem::zeroed() };
        if unsafe { factory.getClassInfo(i, &mut ci) } == kResultOk {
            let cat: &[u8] = unsafe {
                std::slice::from_raw_parts(ci.category.as_ptr() as *const u8, ci.category.len())
            };
            if cat.starts_with(b"Audio Module Class") && audio_module_cid.is_none() {
                audio_module_cid = Some(ci.cid);
            } else if cat.starts_with(b"Component Controller Class") && factory_ctrl_cid.is_none() {
                factory_ctrl_cid = Some(ci.cid);
            }
        }
    }

    // Helper: try multiple strategies to create an IEditController from a CID
    let try_create_ec = |cid: &vst3::Steinberg::TUID| -> Option<ComPtr<IEditController>> {
        // Strategy A: ask for IEditController IID directly
        let mut ec_ptr: *mut IEditController = ptr::null_mut();
        let ra = unsafe {
            factory.createInstance(
                cid.as_ptr() as *const i8,
                IEditController::IID.as_ptr() as *const i8,
                &mut ec_ptr as *mut _ as *mut _,
            )
        };
        if ra == kResultOk && !ec_ptr.is_null() {
            return unsafe { ComPtr::<IEditController>::from_raw(ec_ptr) };
        }

        // Strategy B: create with IComponent IID, then QI for IEditController
        let mut comp_ptr: *mut IComponent = ptr::null_mut();
        let rb = unsafe {
            factory.createInstance(
                cid.as_ptr() as *const i8,
                IComponent::IID.as_ptr() as *const i8,
                &mut comp_ptr as *mut _ as *mut _,
            )
        };
        if rb == kResultOk && !comp_ptr.is_null() {
            if let Some(comp) = unsafe { ComPtr::<IComponent>::from_raw(comp_ptr) } {
                if let Some(ec) = comp.cast::<IEditController>() {
                    return Some(ec);
                }
            }
        }

        // Strategy C: FUnknown IID fallback
        let mut raw: *mut vst3::Steinberg::FUnknown = ptr::null_mut();
        let rc = unsafe {
            factory.createInstance(
                cid.as_ptr() as *const i8,
                vst3::Steinberg::FUnknown::IID.as_ptr() as *const i8,
                &mut raw as *mut _ as *mut _,
            )
        };
        if rc == kResultOk && !raw.is_null() {
            if let Some(fu) = unsafe { ComPtr::<vst3::Steinberg::FUnknown>::from_raw(raw) } {
                return fu.cast::<IEditController>();
            }
        }

        None
    };

    // Find IEditController
    let mut controller: Option<ComPtr<IEditController>> = None;

    if let Some(comp_cid) = audio_module_cid {
        // Try creating the component and checking for single-component design or separate controller
        let mut comp_ptr: *mut IComponent = ptr::null_mut();
        let result = unsafe {
            factory.createInstance(
                comp_cid.as_ptr() as *const i8,
                IComponent::IID.as_ptr() as *const i8,
                &mut comp_ptr as *mut _ as *mut _,
            )
        };

        let component: Option<ComPtr<IComponent>> = if result == kResultOk && !comp_ptr.is_null() {
            unsafe { ComPtr::<IComponent>::from_raw(comp_ptr) }
        } else {
            // FUnknown fallback
            let mut raw: *mut vst3::Steinberg::FUnknown = ptr::null_mut();
            let r2 = unsafe {
                factory.createInstance(
                    comp_cid.as_ptr() as *const i8,
                    vst3::Steinberg::FUnknown::IID.as_ptr() as *const i8,
                    &mut raw as *mut _ as *mut _,
                )
            };
            if r2 == kResultOk && !raw.is_null() {
                unsafe { ComPtr::<vst3::Steinberg::FUnknown>::from_raw(raw) }
                    .and_then(|fu| fu.cast::<IComponent>())
            } else {
                None
            }
        };

        if let Some(comp) = component {
            // Single-component: the component itself implements IEditController
            if let Some(ec) = comp.cast::<IEditController>() {
                log::debug!("Plugin '{}': single-component design", plugin_name);
                controller = Some(ec);
            } else {
                // Separate controller: ask component for controller CID
                let mut ctrl_cid: vst3::Steinberg::TUID = [0i8; 16];
                let gc_result = unsafe { comp.getControllerClassId(&mut ctrl_cid) };
                log::debug!("Plugin '{}': getControllerClassId -> {}", plugin_name, gc_result);

                if gc_result == kResultOk && ctrl_cid != [0i8; 16] {
                    controller = try_create_ec(&ctrl_cid);
                    if controller.is_some() {
                        log::debug!("Plugin '{}': controller via getControllerClassId", plugin_name);
                    }
                }
            }
        }
    }

    // Fallback: factory has a dedicated Component Controller Class entry
    if controller.is_none() {
        if let Some(ctrl_cid) = factory_ctrl_cid {
            log::debug!("Plugin '{}': trying Component Controller Class from factory", plugin_name);
            controller = try_create_ec(&ctrl_cid);
        }
    }

    let controller = controller
        .ok_or_else(|| anyhow!("Could not create IEditController for '{}'", plugin_name))?;

    // Initialize controller
    unsafe { controller.initialize(ptr::null_mut()) };

    // Create editor view
    let view_name = CString::new("editor").map_err(|_| anyhow!("CString error"))?;
    let view_raw = unsafe { controller.createView(view_name.as_ptr()) };
    if view_raw.is_null() {
        unsafe { controller.terminate() };
        return Err(anyhow!("Plugin '{}' returned null IPlugView", plugin_name));
    }
    let view = unsafe {
        ComPtr::<IPlugView>::from_raw(view_raw)
            .ok_or_else(|| anyhow!("Failed to wrap IPlugView"))?
    };

    // Check HWND support
    let supported = unsafe { view.isPlatformTypeSupported(kPlatformTypeHWND) };
    if supported != kResultOk {
        unsafe { controller.terminate() };
        return Err(anyhow!("Plugin '{}' does not support HWND platform", plugin_name));
    }

    // Get view size
    let mut view_rect = vst3::Steinberg::ViewRect { left: 0, top: 0, right: 800, bottom: 600 };
    unsafe { view.getSize(&mut view_rect) };
    let width = (view_rect.right - view_rect.left).max(400);
    let height = (view_rect.bottom - view_rect.top).max(300);

    // Create host window
    let hwnd = create_host_window(plugin_name, width, height)?;

    // Attach plugin view
    let attached_result = unsafe {
        view.attached(hwnd, kPlatformTypeHWND)
    };
    if attached_result != kResultOk {
        log::warn!("IPlugView::attached returned {} for '{}'", attached_result, plugin_name);
    }

    log::info!("Plugin '{}' GUI window opened ({}x{})", plugin_name, width, height);

    // Run message loop (blocks until window closed)
    run_message_loop(hwnd);

    // Cleanup (ComPtr handles Release on drop automatically)
    unsafe {
        view.removed();
        controller.terminate();
    }

    drop(view);
    drop(controller);
    drop(factory);
    drop(lib);

    Ok(())
}

#[cfg(target_os = "windows")]
fn create_host_window(title: &str, width: i32, height: i32) -> Result<*mut std::ffi::c_void> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;

    let class_name: Vec<u16> = OsStr::new("ReLightHostVST3").encode_wide().chain(Some(0)).collect();
    let window_title: Vec<u16> = OsStr::new(title).encode_wide().chain(Some(0)).collect();

    unsafe {
        let hinstance = GetModuleHandleW(std::ptr::null());

        let wc = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(DefWindowProcW),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: std::ptr::null_mut(),
            hCursor: std::ptr::null_mut(),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
        };
        // Ignore error if class already registered
        RegisterClassW(&wc);

        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            window_title.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            width + 16,
            height + 39,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hinstance,
            std::ptr::null(),
        );

        if hwnd.is_null() {
            return Err(anyhow!("CreateWindowExW failed to create window"));
        }
        Ok(hwnd)
    }
}

#[cfg(target_os = "windows")]
fn run_message_loop(hwnd: *mut std::ffi::c_void) {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    let mut msg: MSG = unsafe { std::mem::zeroed() };
    loop {
        let ret = unsafe { GetMessageW(&mut msg, hwnd, 0, 0) };
        match ret {
            -1 | 0 => break,
            _ => unsafe {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn launch_vst3_gui(plugin_path: &str, plugin_name: &str) -> Result<()> {
    log::warn!("VST3 GUI launching is only supported on Windows. Plugin: {}", plugin_name);
    Err(anyhow!("Plugin GUI launching is not yet supported on this platform"))
}