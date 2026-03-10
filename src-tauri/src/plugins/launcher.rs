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

    // Spawn on a dedicated thread — VST3 GUI and COM must run on the same thread,
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
    use std::ffi::{c_void, CString};
    use std::ptr;
    use libloading::{Library, Symbol};

    // VST3 FUID type: 16 bytes / 4 u32s
    type Tuid = [u8; 16];

    // IPluginFactory
    #[repr(C)]
    struct IPluginFactory {
        vtbl: *const IPluginFactoryVtbl,
    }
    #[repr(C)]
    #[allow(dead_code)]
    struct PClassInfo {
        cid: Tuid,
        cardinality: i32,
        category: [i8; 32],
        name: [i8; 64],
    }
    #[repr(C)]
    #[allow(dead_code)]
    struct PFactoryInfo {
        vendor: [i8; 64],
        url: [i8; 256],
        email: [i8; 128],
        flags: i32,
    }
    #[repr(C)]
    struct IPluginFactoryVtbl {
        query_interface: unsafe extern "system" fn(*mut IPluginFactory, *const Tuid, *mut *mut c_void) -> i32,
        add_ref: unsafe extern "system" fn(*mut IPluginFactory) -> u32,
        release: unsafe extern "system" fn(*mut IPluginFactory) -> u32,
        get_factory_info: unsafe extern "system" fn(*mut IPluginFactory, *mut PFactoryInfo) -> i32,
        count_classes: unsafe extern "system" fn(*mut IPluginFactory) -> i32,
        get_class_info: unsafe extern "system" fn(*mut IPluginFactory, i32, *mut PClassInfo) -> i32,
        create_instance: unsafe extern "system" fn(*mut IPluginFactory, *const i8, *const i8, *mut *mut c_void) -> i32,
    }

    // IPlugView
    #[repr(C)]
    struct IPlugView {
        vtbl: *const IPlugViewVtbl,
    }
    #[repr(C)]
    #[allow(dead_code)]
    struct ViewRect {
        left: i32, top: i32, right: i32, bottom: i32,
    }
    #[repr(C)]
    struct IPlugViewVtbl {
        query_interface: unsafe extern "system" fn(*mut IPlugView, *const Tuid, *mut *mut c_void) -> i32,
        add_ref: unsafe extern "system" fn(*mut IPlugView) -> u32,
        release: unsafe extern "system" fn(*mut IPlugView) -> u32,
        is_platform_type_supported: unsafe extern "system" fn(*mut IPlugView, *const i8) -> i32,
        attached: unsafe extern "system" fn(*mut IPlugView, *mut c_void, *const i8) -> i32,
        removed: unsafe extern "system" fn(*mut IPlugView) -> i32,
        on_wheel: unsafe extern "system" fn(*mut IPlugView, f32) -> i32,
        on_key_down: unsafe extern "system" fn(*mut IPlugView, i16, i16, i16) -> i32,
        on_key_up: unsafe extern "system" fn(*mut IPlugView, i16, i16, i16) -> i32,
        get_size: unsafe extern "system" fn(*mut IPlugView, *mut ViewRect) -> i32,
        on_size: unsafe extern "system" fn(*mut IPlugView, *mut ViewRect) -> i32,
        on_focus: unsafe extern "system" fn(*mut IPlugView, u8) -> i32,
        set_frame: unsafe extern "system" fn(*mut IPlugView, *mut c_void) -> i32,
        can_resize: unsafe extern "system" fn(*mut IPlugView) -> i32,
        check_size_constraint: unsafe extern "system" fn(*mut IPlugView, *mut ViewRect) -> i32,
    }

    // Generic vtable prefix — every VST3 interface starts with these 3 slots
    #[repr(C)]
    struct FUnknownVtbl {
        query_interface: unsafe extern "system" fn(*mut c_void, *const Tuid, *mut *mut c_void) -> i32,
        add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
        release: unsafe extern "system" fn(*mut c_void) -> u32,
    }
    #[repr(C)]
    struct FUnknown {
        vtbl: *const FUnknownVtbl,
    }

    // IComponent vtable — extends IPluginBase (IUnknown + initialize/terminate)
    // We only need slot 5: getControllerClassId
    #[repr(C)]
    struct IComponentVtbl {
        // 0-2: IFUnknown
        query_interface: unsafe extern "system" fn(*mut c_void, *const Tuid, *mut *mut c_void) -> i32,
        add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
        release: unsafe extern "system" fn(*mut c_void) -> u32,
        // 3-4: IPluginBase
        initialize: unsafe extern "system" fn(*mut c_void, *mut c_void) -> i32,
        terminate: unsafe extern "system" fn(*mut c_void) -> i32,
        // 5: IComponent
        get_controller_class_id: unsafe extern "system" fn(*mut c_void, *mut Tuid) -> i32,
    }
    #[repr(C)]
    struct IComponent {
        vtbl: *const IComponentVtbl,
    }

    // IEditController interface
    #[repr(C)]
    struct IEditController {
        vtbl: *const IEditControllerVtbl,
    }
    #[repr(C)]
    struct IEditControllerVtbl {
        // 0-2: IUnknown
        query_interface: unsafe extern "system" fn(*mut c_void, *const Tuid, *mut *mut c_void) -> i32,
        add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
        release: unsafe extern "system" fn(*mut c_void) -> u32,
        // 3-4: IPluginBase
        initialize: unsafe extern "system" fn(*mut c_void, *mut c_void) -> i32,
        terminate: unsafe extern "system" fn(*mut c_void) -> i32,
        // 5-16: IEditController
        set_component_state: unsafe extern "system" fn(*mut c_void, *mut c_void) -> i32,
        set_state: unsafe extern "system" fn(*mut c_void, *mut c_void) -> i32,
        get_state: unsafe extern "system" fn(*mut c_void, *mut c_void) -> i32,
        get_parameter_count: unsafe extern "system" fn(*mut c_void) -> i32,
        get_parameter_info: unsafe extern "system" fn(*mut c_void, i32, *mut c_void) -> i32,
        get_param_string_by_value: unsafe extern "system" fn(*mut c_void, u32, f64, *mut c_void) -> i32,
        get_param_value_by_string: unsafe extern "system" fn(*mut c_void, u32, *const i16, *mut f64) -> i32,
        normalized_param_to_plain: unsafe extern "system" fn(*mut c_void, u32, f64) -> f64,
        plain_param_to_normalized: unsafe extern "system" fn(*mut c_void, u32, f64) -> f64,
        get_param_normalized: unsafe extern "system" fn(*mut c_void, u32) -> f64,
        set_param_normalized: unsafe extern "system" fn(*mut c_void, u32, f64) -> i32,
        set_component_handler: unsafe extern "system" fn(*mut c_void, *mut c_void) -> i32,
        // 17: create_view
        create_view: unsafe extern "system" fn(*mut c_void, *const i8) -> *mut IPlugView,
    }

    // VST3 IIDs from the official SDK (pluginterfaces/vst/)
    // IComponent:     E831FF31-F2D54301-928EBBEE-25697802
    const ICOMPONENT_IID: Tuid = [
        0xE8, 0x31, 0xFF, 0x31, 0xF2, 0xD5, 0x43, 0x01,
        0x92, 0x8E, 0xBB, 0xEE, 0x25, 0x69, 0x78, 0x02,
    ];
    // IEditController: DCD7BBE3-7742448D-A874AACC-979C9D2B
    const IEDIT_CONTROLLER_IID: Tuid = [
        0xDC, 0xD7, 0xBB, 0xE3, 0x77, 0x42, 0x44, 0x8D,
        0xA8, 0x74, 0xAA, 0xCC, 0x97, 0x9C, 0x9D, 0x2B,
    ];

    // Helper: call queryInterface on any VST3 object and return the requested interface
    unsafe fn qi(obj: *mut c_void, iid: &Tuid) -> Option<*mut c_void> {
        let fu = obj as *mut FUnknown;
        let mut out: *mut c_void = ptr::null_mut();
        let hr = ((*(*fu).vtbl).query_interface)(obj, iid, &mut out);
        if hr == 0 && !out.is_null() { Some(out) } else { None }
    }
    // Helper: release any VST3 object
    unsafe fn rel(obj: *mut c_void) {
        let fu = obj as *mut FUnknown;
        ((*(*fu).vtbl).release)(obj);
    }

    // Load DLL
    let lib = unsafe { Library::new(plugin_path) }
        .map_err(|e| anyhow!("Failed to load plugin DLL '{}': {}", plugin_path, e))?;

    // Get factory function
    type GetPluginFactory = unsafe extern "system" fn() -> *mut IPluginFactory;
    let get_factory: Symbol<GetPluginFactory> = unsafe { lib.get(b"GetPluginFactory\0") }
        .map_err(|e| anyhow!("Plugin '{}' has no GetPluginFactory export: {}", plugin_path, e))?;

    let factory = unsafe { get_factory() };
    if factory.is_null() {
        return Err(anyhow!("GetPluginFactory returned null"));
    }

    // Find the audio processor class
    let class_count = unsafe { ((*(*factory).vtbl).count_classes)(factory) };
    log::info!("Plugin '{}' has {} class(es)", plugin_name, class_count);

    // Phase 1 – scan all factory classes.
    // Collect the first "Audio Module Class" CID (processor) and the first
    // "Component Controller Class" CID (standalone controller, if present).
    let mut audio_module_cid: Option<Tuid> = None;
    let mut factory_ctrl_cid: Option<Tuid> = None;

    for i in 0..class_count {
        let mut class_info = PClassInfo {
            cid: [0u8; 16],
            cardinality: 0,
            category: [0i8; 32],
            name: [0i8; 64],
        };
        unsafe { ((*(*factory).vtbl).get_class_info)(factory, i, &mut class_info) };

        let category = class_info.category.map(|b| b as u8);
        let cat_bytes = &category[..];

        if cat_bytes.starts_with(b"Audio Module Class") {
            if audio_module_cid.is_none() {
                log::debug!("Plugin '{}': found Audio Module Class at index {}", plugin_name, i);
                audio_module_cid = Some(class_info.cid);
            }
        } else if cat_bytes.starts_with(b"Component Controller Class") {
            if factory_ctrl_cid.is_none() {
                log::debug!("Plugin '{}': found Component Controller Class at index {}", plugin_name, i);
                factory_ctrl_cid = Some(class_info.cid);
            }
        }
    }

    // Phase 2 – obtain an IEditController pointer.
    // Helper: try to create an instance for `cid` using several IID strategies, then QI.
    let try_create_ec = |cid: &Tuid| -> Option<*mut IEditController> {
        let cid_ptr = cid.as_ptr() as *const i8;
        let ec_iid_ptr = IEDIT_CONTROLLER_IID.as_ptr() as *const i8;
        let comp_iid_ptr = ICOMPONENT_IID.as_ptr() as *const i8;

        // Strategy A: ask for IEditController IID directly
        let mut obj: *mut c_void = ptr::null_mut();
        let ra = unsafe { ((*(*factory).vtbl).create_instance)(factory, cid_ptr, ec_iid_ptr, &mut obj) };
        if ra == 0 && !obj.is_null() {
            // Might already be IEditController, or might need QI
            if let Some(ec) = unsafe { qi(obj, &IEDIT_CONTROLLER_IID) } {
                unsafe { rel(obj) };
                return Some(ec as *mut IEditController);
            }
            // The object IS IEditController (factory skipped internal QI)
            return Some(obj as *mut IEditController);
        }

        // Strategy B: create with IComponent IID, then QI for IEditController
        let mut obj2: *mut c_void = ptr::null_mut();
        let rb = unsafe { ((*(*factory).vtbl).create_instance)(factory, cid_ptr, comp_iid_ptr, &mut obj2) };
        if rb == 0 && !obj2.is_null() {
            if let Some(ec) = unsafe { qi(obj2, &IEDIT_CONTROLLER_IID) } {
                unsafe { rel(obj2) };
                return Some(ec as *mut IEditController);
            }
            unsafe { rel(obj2) };
        }

        None
    };

    let mut controller_ptr: *mut IEditController = ptr::null_mut();

    if let Some(comp_cid) = audio_module_cid {
        let cid_ptr = comp_cid.as_ptr() as *const i8;
        let iid_ptr = ICOMPONENT_IID.as_ptr() as *const i8;
        let mut component: *mut c_void = ptr::null_mut();
        let result = unsafe {
            ((*(*factory).vtbl).create_instance)(factory, cid_ptr, iid_ptr, &mut component)
        };

        if result == 0 && !component.is_null() {
            // Path A: single-component — the object itself implements IEditController
            if let Some(ec) = unsafe { qi(component, &IEDIT_CONTROLLER_IID) } {
                log::debug!("Plugin '{}': single-component design (QI succeeded)", plugin_name);
                unsafe { rel(component) };
                controller_ptr = ec as *mut IEditController;
            } else {
                // Path B: separate controller — ask IComponent for its controller CID
                let mut ctrl_cid: Tuid = [0u8; 16];
                let ic = component as *mut IComponent;
                let gc_result = unsafe {
                    ((*(*ic).vtbl).get_controller_class_id)(component, &mut ctrl_cid)
                };
                log::debug!(
                    "Plugin '{}': getControllerClassId returned {}, cid={:?}",
                    plugin_name, gc_result, &ctrl_cid[..]
                );
                unsafe { rel(component) };

                if gc_result == 0 && ctrl_cid != [0u8; 16] {
                    if let Some(ec) = try_create_ec(&ctrl_cid) {
                        log::debug!("Plugin '{}': controller created via getControllerClassId", plugin_name);
                        controller_ptr = ec;
                    }
                }
            }
        } else {
            log::debug!("Plugin '{}': create_instance for processor failed ({})", plugin_name, result);
        }
    }

    // Path C: factory has an explicit "Component Controller Class" entry — use it directly
    if controller_ptr.is_null() {
        if let Some(ctrl_cid) = factory_ctrl_cid {
            log::debug!("Plugin '{}': trying Component Controller Class from factory", plugin_name);
            if let Some(ec) = try_create_ec(&ctrl_cid) {
                log::debug!("Plugin '{}': controller created via factory Component Controller Class", plugin_name);
                controller_ptr = ec;
            }
        }
    }

    if controller_ptr.is_null() {
        unsafe { rel(factory as *mut c_void) };
        drop(lib);
        return Err(anyhow!("Could not create IEditController for '{}'", plugin_name));
    }

    // Initialize the controller with a null host context
    unsafe { ((*(*controller_ptr).vtbl).initialize)(controller_ptr as *mut c_void, ptr::null_mut()) };

    // Create the editor view
    let view_name = CString::new("editor").map_err(|_| anyhow!("CString error"))?;
    let view = unsafe { ((*(*controller_ptr).vtbl).create_view)(controller_ptr as *mut c_void, view_name.as_ptr()) };

    if view.is_null() {
        unsafe {
            ((*(*controller_ptr).vtbl).terminate)(controller_ptr as *mut c_void);
            rel(controller_ptr as *mut c_void);
            rel(factory as *mut c_void);
        }
        drop(lib);
        return Err(anyhow!("Plugin '{}' returned null IPlugView", plugin_name));
    }

    // Check if HWND is supported
    let platform_type = CString::new("HWND").map_err(|_| anyhow!("CString error"))?;
    let supported = unsafe {
        ((*(*view).vtbl).is_platform_type_supported)(view, platform_type.as_ptr())
    };
    if supported != 0 {
        unsafe {
            rel(view as *mut c_void);
            ((*(*controller_ptr).vtbl).terminate)(controller_ptr as *mut c_void);
            rel(controller_ptr as *mut c_void);
            rel(factory as *mut c_void);
        }
        drop(lib);
        return Err(anyhow!("Plugin '{}' does not support HWND platform", plugin_name));
    }

    // Get the plugin view size
    let mut view_rect = ViewRect { left: 0, top: 0, right: 800, bottom: 600 };
    unsafe { ((*(*view).vtbl).get_size)(view, &mut view_rect) };
    let width = (view_rect.right - view_rect.left).max(400);
    let height = (view_rect.bottom - view_rect.top).max(300);

    // Create a Win32 window to host the plugin
    let hwnd = create_host_window(plugin_name, width, height)?;

    // Attach the plugin view to our window
    let attached_result = unsafe {
        ((*(*view).vtbl).attached)(view, hwnd, platform_type.as_ptr())
    };
    if attached_result != 0 {
        log::warn!("IPlugView::attached returned {} for '{}'", attached_result, plugin_name);
    }

    log::info!("Plugin '{}' GUI window opened ({}x{})", plugin_name, width, height);

    // Run message loop — this blocks until the window is closed
    run_message_loop(hwnd);

    // Cleanup
    unsafe {
        ((*(*view).vtbl).removed)(view);
        rel(view as *mut c_void);
        ((*(*controller_ptr).vtbl).terminate)(controller_ptr as *mut c_void);
        rel(controller_ptr as *mut c_void);
        rel(factory as *mut c_void);
    }
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
        // Ignore error if class is already registered
        RegisterClassW(&wc);

        let hwnd = CreateWindowExW(
            0,                           // dwExStyle
            class_name.as_ptr(),         // lpClassName
            window_title.as_ptr(),       // lpWindowName
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            width + 16,
            height + 39,
            std::ptr::null_mut(),        // hWndParent (null)
            std::ptr::null_mut(),        // hMenu (null)
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
