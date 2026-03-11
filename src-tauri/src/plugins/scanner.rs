use std::path::{Path, PathBuf};
use std::fs;
use anyhow::Result;
use crate::plugins::types::{PluginInfo, PluginFormat};

pub struct PluginScanner {
    scan_paths: Vec<PathBuf>,
}

impl PluginScanner {
    pub fn new() -> Self {
        Self {
            scan_paths: Self::default_scan_paths(),
        }
    }

    /// Get default scan paths based on OS
    fn default_scan_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        #[cfg(target_os = "windows")]
        {
            // Common VST3 paths on Windows
            if let Ok(program_files) = std::env::var("ProgramFiles") {
                paths.push(PathBuf::from(&program_files).join("Common Files\\VST3"));
                paths.push(PathBuf::from(&program_files).join("VSTPlugins")); // VST2
                paths.push(PathBuf::from(&program_files).join("Steinberg\\VSTPlugins")); // VST2
            }
            
            // Common CLAP paths on Windows
            if let Ok(program_files) = std::env::var("ProgramFiles") {
                paths.push(PathBuf::from(program_files).join("Common Files\\CLAP"));
            }
            
            if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
                paths.push(PathBuf::from(&local_app_data).join("Programs\\Common\\CLAP"));
                paths.push(PathBuf::from(&local_app_data).join("Programs\\Common\\VST2")); // VST2
            }
        }

        #[cfg(target_os = "macos")]
        {
            paths.push(PathBuf::from("/Library/Audio/Plug-Ins/VST3"));
            paths.push(PathBuf::from("/Library/Audio/Plug-Ins/VST")); // VST2
            paths.push(PathBuf::from("/Library/Audio/Plug-Ins/CLAP"));
            if let Ok(home) = std::env::var("HOME") {
                paths.push(PathBuf::from(&home).join("Library/Audio/Plug-Ins/VST3"));
                paths.push(PathBuf::from(&home).join("Library/Audio/Plug-Ins/VST")); // VST2
                paths.push(PathBuf::from(&home).join("Library/Audio/Plug-Ins/CLAP"));
            }
        }

        #[cfg(target_os = "linux")]
        {
            paths.push(PathBuf::from("/usr/lib/vst3"));
            paths.push(PathBuf::from("/usr/lib/vst")); // VST2
            paths.push(PathBuf::from("/usr/lib/clap"));
            if let Ok(home) = std::env::var("HOME") {
                paths.push(PathBuf::from(&home).join(".vst3"));
                paths.push(PathBuf::from(&home).join(".vst")); // VST2
                paths.push(PathBuf::from(&home).join(".clap"));
            }
        }

        paths
    }

    /// Add custom scan path
    pub fn add_scan_path<P: AsRef<Path>>(&mut self, path: P) {
        self.scan_paths.push(path.as_ref().to_path_buf());
    }

    /// Scan all configured paths for plugins
    pub fn scan(&self) -> Result<Vec<PluginInfo>> {
        let mut plugins = Vec::new();

        // Always include built-in processors first so they appear at the top.
        plugins.extend(Self::builtin_plugins());

        for path in &self.scan_paths {
            if path.exists() && path.is_dir() {
                // Always allow loose DLLs at the top level of any scan root.
                // The DLL-skipping logic only activates when recursing into
                // non-bundle subdirectories of a VST3/CLAP path (see scan_directory).
                if let Ok(found) = self.scan_directory(path, true) {
                    plugins.extend(found);
                }
            }
        }

        log::info!("Found {} plugins", plugins.len());
        Ok(plugins)
    }

    /// Returns true when `path` is (or lives under) a VST3 or CLAP root directory.
    /// Such directories should only yield bundle-style plugins, never loose DLLs.
    fn path_is_vst3_or_clap_root(path: &Path) -> bool {
        path.components().any(|c| {
            c.as_os_str()
                .to_str()
                .map(|s| s.eq_ignore_ascii_case("VST3") || s.eq_ignore_ascii_case("CLAP"))
                .unwrap_or(false)
        })
    }

    /// Scan a specific directory for plugins.
    ///
    /// `allow_loose_dll`: when `false`, plain `.dll`/`.so`/`.dylib` files are
    /// skipped. This is set to `false` only when recursing into non-bundle
    /// subdirectories of a VST3/CLAP path, where DLLs are bundle-internal
    /// shared libraries rather than standalone VST2 plugins.
    fn scan_directory(&self, dir: &Path, allow_loose_dll: bool) -> Result<Vec<PluginInfo>> {
        let mut plugins = Vec::new();

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                // Check if this is a VST3 bundle (folder named *.vst3 on Windows)
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if ext.eq_ignore_ascii_case("vst3") {
                        if let Some(plugin) = self.scan_vst3_bundle(&path) {
                            plugins.push(plugin);
                            continue;
                        }
                    }
                }
                // Check if a non-.vst3 folder is still a VST3 bundle by structure
                // (some installers omit the .vst3 extension on the folder)
                #[cfg(target_os = "windows")]
                if path.join("Contents").join("x86_64-win").is_dir() {
                    if let Some(plugin) = self.scan_vst3_bundle(&path) {
                        plugins.push(plugin);
                        continue;
                    }
                }
                // Recursively scan other subdirectories.
                // If the current directory is inside a VST3/CLAP path, any non-bundle
                // subdir may contain companion/internal DLLs that are not VST2 plugins.
                // Suppress loose-DLL scanning for those subdirs only.
                let child_allow_loose_dll = if Self::path_is_vst3_or_clap_root(dir) {
                    false
                } else {
                    allow_loose_dll
                };
                if let Ok(sub_plugins) = self.scan_directory(&path, child_allow_loose_dll) {
                    plugins.extend(sub_plugins);
                }
            } else if path.is_file() {
                // In VST3/CLAP roots, skip bare native-library files.
                // They are VST3 bundle components or helper DLLs, not VST2 plugins.
                if !allow_loose_dll {
                    let is_native_lib = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|e| matches!(e.to_lowercase().as_str(), "dll" | "so" | "dylib"))
                        .unwrap_or(false);
                    if is_native_lib {
                        continue;
                    }
                }
                if let Some(plugin) = self.scan_file(&path) {
                    plugins.push(plugin);
                }
            }
        }

        Ok(plugins)
    }

    /// Scan a VST3 bundle directory (Windows: PluginName.vst3/Contents/x86_64-win/PluginName.dll)
    fn scan_vst3_bundle(&self, bundle_path: &Path) -> Option<PluginInfo> {
        let plugin_name = bundle_path.file_stem()?.to_str()?;
        let id = format!("vst3::{}", plugin_name);

        // On Windows, the DLL lives at Contents/x86_64-win/<PluginName>.dll
        // Fall back to scanning the arch dir for any DLL if the expected name isn't found
        #[cfg(target_os = "windows")]
        let dll_path = {
            let expected = bundle_path
                .join("Contents")
                .join("x86_64-win")
                .join(format!("{}.dll", plugin_name));
            if expected.exists() {
                expected
            } else {
                // Try any .dll in the arch dir (bundle may use a different DLL name)
                let arch_dir = bundle_path.join("Contents").join("x86_64-win");
                let found = fs::read_dir(&arch_dir).ok().and_then(|mut entries| {
                    entries.find_map(|e| {
                        let p = e.ok()?.path();
                        if p.extension().and_then(|x| x.to_str()) == Some("dll") {
                            Some(p)
                        } else {
                            None
                        }
                    })
                });
                found.unwrap_or(expected)
            }
        };

        #[cfg(target_os = "macos")]
        let dll_path = bundle_path.join("Contents").join("MacOS").join(plugin_name);

        #[cfg(target_os = "linux")]
        let dll_path = bundle_path.join("Contents").join("x86_64-linux").join(format!("{}.so", plugin_name));

        // Use bundle path for the record; resolve DLL path at launch time
        let effective_path = if dll_path.exists() {
            dll_path.to_string_lossy().to_string()
        } else {
            bundle_path.to_string_lossy().to_string()
        };

        // Try to read real metadata: moduleInfo.json first (no DLL load), then
        // factory API (requires loading the DLL briefly for IPluginFactory2).
        let (name, vendor, version, category) =
            read_vst3_module_info(bundle_path)
                .or_else(|| if dll_path.exists() { read_vst3_dll_info(&dll_path) } else { None })
                .unwrap_or_else(|| (
                    plugin_name.to_string(),
                    String::new(),
                    String::new(),
                    "Effect".to_string(),
                ));

        Some(PluginInfo {
            id,
            name,
            vendor,
            version,
            path: effective_path,
            format: PluginFormat::VST3,
            category,
        })
    }

    /// Try to scan a single file as a plugin (VST2 .dll, CLAP .clap)
    fn scan_file(&self, path: &Path) -> Option<PluginInfo> {
        let extension = path.extension()?.to_str()?;
        let format = PluginFormat::from_extension(extension)?;

        // Skip DLLs that live *inside* a VST3 bundle (handled by scan_vst3_bundle).
        // Only check parent directory components — not the file itself — to avoid
        // incorrectly skipping flat single-file .vst3 plugins.
        if cfg!(target_os = "windows") && (matches!(format, PluginFormat::VST3) || extension.eq_ignore_ascii_case("dll")) {
            let parent_is_bundle = path.parent().map(|parent| {
                parent.components().any(|c| {
                    c.as_os_str().to_str()
                        .map(|s| s.ends_with(".vst3") || s.eq_ignore_ascii_case("x86_64-win"))
                        .unwrap_or(false)
                })
            }).unwrap_or(false);
            if parent_is_bundle {
                return None;
            }
        }

        let filename = path.file_stem()?.to_str()?;
        let id = format!("{}::{}", format!("{:?}", format).to_lowercase(), filename);

        // For VST2, try to read real name/vendor/version from the binary.
        let (name, vendor, version) = if matches!(format, PluginFormat::VST) {
            read_vst2_metadata(&path)
                .unwrap_or_else(|| (filename.to_string(), String::new(), String::new()))
        } else {
            (filename.to_string(), String::new(), String::new())
        };

        Some(PluginInfo {
            id,
            name,
            vendor,
            version,
            path: path.to_string_lossy().to_string(),
            format,
            category: "Effect".to_string(),
        })
    }
}

impl Default for PluginScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginScanner {
    /// Returns the list of processors built into the application.
    /// These are always available and do not need file scanning.
    pub fn builtin_plugins() -> Vec<PluginInfo> {
        use crate::plugins::types::PluginFormat;
        use crate::plugins::builtin::{noise_suppressor, compressor};
        vec![
            PluginInfo {
                id:       noise_suppressor::ID.to_string(),
                name:     "Noise Suppressor (RNNoise)".to_string(),
                vendor:   "Built-in".to_string(),
                version:  env!("CARGO_PKG_VERSION").to_string(),
                path:     noise_suppressor::ID.to_string(),
                format:   PluginFormat::Builtin,
                category: "Noise Reduction".to_string(),
            },
            PluginInfo {
                id:       compressor::ID.to_string(),
                name:     "Compressor".to_string(),
                vendor:   "Built-in".to_string(),
                version:  env!("CARGO_PKG_VERSION").to_string(),
                path:     compressor::ID.to_string(),
                format:   PluginFormat::Builtin,
                category: "Dynamics".to_string(),
            },
        ]
    }
}

// ── Plugin metadata helpers ───────────────────────────────────────────────────

/// Convert a null-terminated VST3 `char8` (`c_char`) array into a Rust `String`.
fn char8_to_string(buf: &[std::ffi::c_char]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let bytes: Vec<u8> = buf[..end].iter().map(|&c| c as u8).collect();
    String::from_utf8_lossy(&bytes).trim().to_string()
}

/// Parse a VST3 subCategories string (e.g. `"Fx|Dynamics|Compressor"`) into a
/// display category label. Strips the generic top-level `"Fx"` prefix.
fn parse_vst3_subcategory(subcats: &str) -> String {
    if subcats.is_empty() {
        return "Effect".to_string();
    }
    subcats
        .split('|')
        .map(str::trim)
        .find(|s| !s.eq_ignore_ascii_case("Fx") && !s.is_empty())
        .unwrap_or("Effect")
        .to_string()
}

/// Try to read `(name, vendor, version, category)` from a VST3 bundle's
/// `moduleInfo.json` (VST3 SDK ≥ 3.7). No DLL is loaded.
fn read_vst3_module_info(bundle_path: &Path) -> Option<(String, String, String, String)> {
    let candidates = [
        bundle_path.join("Contents").join("moduleInfo.json"),
        bundle_path.join("Contents").join("Resources").join("moduleInfo.json"),
    ];
    let json_path = candidates.iter().find(|p| p.exists())?;
    let data = fs::read_to_string(json_path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&data).ok()?;

    let factory_vendor = v
        .get("Factory Info")
        .and_then(|fi| fi.get("Vendor"))
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let factory_version = v
        .get("Version")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    let classes = v.get("Classes")?.as_array()?;
    for cls in classes {
        let cat = cls.get("Category").and_then(|s| s.as_str()).unwrap_or("");
        if !cat.contains("Audio Module Class") {
            continue;
        }
        let name = cls
            .get("Name")
            .and_then(|s| s.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)?;
        let vendor = cls
            .get("Vendor")
            .and_then(|s| s.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| factory_vendor.clone());
        let version = cls
            .get("Version")
            .and_then(|s| s.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| factory_version.clone());
        let subcats = cls
            .get("Sub Categories")
            .and_then(|a| a.as_array())
            .map(|arr| arr.iter().filter_map(|s| s.as_str()).collect::<Vec<_>>().join("|"))
            .unwrap_or_default();
        let category = parse_vst3_subcategory(&subcats);
        return Some((name, vendor, version, category));
    }
    None
}

/// Briefly load a VST3 DLL and query `IPluginFactory2` for real metadata.
/// Falls back to `IPluginFactory` (name + factory vendor only) if unavailable.
fn read_vst3_dll_info(dll_path: &Path) -> Option<(String, String, String, String)> {
    #[cfg(target_os = "windows")]
    {
        read_vst3_dll_info_win(dll_path)
    }
    #[cfg(not(target_os = "windows"))]
    {
        read_vst3_dll_info_generic(dll_path)
    }
}

/// Windows-specific VST3 DLL metadata reader.
/// Uses `LOAD_WITH_ALTERED_SEARCH_PATH` so that the plugin's own directory
/// is searched first when loading its companion DLLs (otherwise LoadLibraryW
/// only searches the app directory, system dirs, and PATH — missing local deps).
#[cfg(target_os = "windows")]
fn read_vst3_dll_info_win(dll_path: &Path) -> Option<(String, String, String, String)> {
    use libloading::os::windows::{Library as WinLib, LOAD_WITH_ALTERED_SEARCH_PATH};
    use vst3::Steinberg::{
        kResultOk,
        IPluginFactory, IPluginFactoryTrait,
        IPluginFactory2, IPluginFactory2Trait,
        PClassInfo, PClassInfo2, PFactoryInfo,
    };
    use vst3::ComPtr;

    // Load with LOAD_WITH_ALTERED_SEARCH_PATH:
    // Windows will search the DLL's own directory for its dependencies,
    // which is essential for plugins that bundle companion DLLs alongside them.
    let lib = unsafe { WinLib::load_with_flags(dll_path, LOAD_WITH_ALTERED_SEARCH_PATH) }
        .inspect_err(|e| log::warn!("VST3 scan: failed to load '{}': {e}", dll_path.display()))
        .ok()?;

    type GetFactory = unsafe extern "system" fn() -> *mut IPluginFactory;
    // SAFETY: we obtain and immediately call the symbol before `lib` can move.
    let factory_ptr: *mut IPluginFactory = {
        let sym: libloading::os::windows::Symbol<GetFactory> =
            unsafe { lib.get(b"GetPluginFactory\0") }
                .inspect_err(|e| log::warn!("VST3 scan: no GetPluginFactory in '{}': {e}", dll_path.display()))
                .ok()?;
        unsafe { sym() }
    };

    if factory_ptr.is_null() {
        log::warn!("VST3 scan: GetPluginFactory returned null for '{}'", dll_path.display());
        return None;
    }

    // ComPtr::from_raw takes ownership of the existing reference (no AddRef).
    // factory is declared AFTER lib → dropped BEFORE lib (reverse drop order). ✓
    let factory = unsafe { ComPtr::<IPluginFactory>::from_raw(factory_ptr)? };

    // Factory-level vendor fallback.
    let mut fi: PFactoryInfo = unsafe { std::mem::zeroed() };
    let factory_vendor = if unsafe { factory.getFactoryInfo(&mut fi) } == kResultOk {
        char8_to_string(&fi.vendor)
    } else {
        String::new()
    };

    let n = unsafe { factory.countClasses() };

    // IPluginFactory2: per-class vendor / version / sub-categories.
    if let Some(factory2) = factory.cast::<IPluginFactory2>() {
        for i in 0..n {
            let mut ci2: PClassInfo2 = unsafe { std::mem::zeroed() };
            if unsafe { factory2.getClassInfo2(i, &mut ci2) } != kResultOk { continue; }
            if !char8_to_string(&ci2.category).starts_with("Audio Module Class") { continue; }
            let name = char8_to_string(&ci2.name);
            if name.is_empty() { continue; }
            let vendor   = { let v = char8_to_string(&ci2.vendor); if v.is_empty() { factory_vendor.clone() } else { v } };
            let version  = char8_to_string(&ci2.version);
            let category = parse_vst3_subcategory(&char8_to_string(&ci2.subCategories));
            log::debug!("VST3 scan: '{}' → name={name:?} vendor={vendor:?} v={version:?}", dll_path.display());
            return Some((name, vendor, version, category));
        }
    }

    // IPluginFactory fallback: name + factory vendor only.
    for i in 0..n {
        let mut ci: PClassInfo = unsafe { std::mem::zeroed() };
        if unsafe { factory.getClassInfo(i, &mut ci) } != kResultOk { continue; }
        if !char8_to_string(&ci.category).starts_with("Audio Module Class") { continue; }
        let name = char8_to_string(&ci.name);
        if !name.is_empty() {
            log::debug!("VST3 scan (factory1 fallback): '{}' → name={name:?}", dll_path.display());
            return Some((name, factory_vendor, String::new(), "Effect".to_string()));
        }
    }

    log::warn!("VST3 scan: no Audio Module Class found in '{}'", dll_path.display());
    None
}

/// Non-Windows generic fallback (macOS / Linux).
#[cfg(not(target_os = "windows"))]
fn read_vst3_dll_info_generic(dll_path: &Path) -> Option<(String, String, String, String)> {
    use libloading::{Library, Symbol};
    use vst3::Steinberg::{
        kResultOk,
        IPluginFactory, IPluginFactoryTrait,
        IPluginFactory2, IPluginFactory2Trait,
        PClassInfo, PClassInfo2, PFactoryInfo,
    };
    use vst3::ComPtr;

    let lib = unsafe { Library::new(dll_path) }
        .inspect_err(|e| log::warn!("VST3 scan: failed to load '{}': {e}", dll_path.display()))
        .ok()?;
    type GetFactory = unsafe extern "system" fn() -> *mut IPluginFactory;
    let factory_ptr: *mut IPluginFactory = {
        let sym: Symbol<GetFactory> = unsafe { lib.get(b"GetPluginFactory\0") }.ok()?;
        unsafe { sym() }
    };
    if factory_ptr.is_null() { return None; }
    let factory = unsafe { ComPtr::<IPluginFactory>::from_raw(factory_ptr)? };

    let mut fi: PFactoryInfo = unsafe { std::mem::zeroed() };
    let factory_vendor = if unsafe { factory.getFactoryInfo(&mut fi) } == kResultOk {
        char8_to_string(&fi.vendor)
    } else { String::new() };
    let n = unsafe { factory.countClasses() };

    if let Some(factory2) = factory.cast::<IPluginFactory2>() {
        for i in 0..n {
            let mut ci2: PClassInfo2 = unsafe { std::mem::zeroed() };
            if unsafe { factory2.getClassInfo2(i, &mut ci2) } != kResultOk { continue; }
            if !char8_to_string(&ci2.category).starts_with("Audio Module Class") { continue; }
            let name = char8_to_string(&ci2.name);
            if name.is_empty() { continue; }
            let vendor   = { let v = char8_to_string(&ci2.vendor); if v.is_empty() { factory_vendor.clone() } else { v } };
            let version  = char8_to_string(&ci2.version);
            let category = parse_vst3_subcategory(&char8_to_string(&ci2.subCategories));
            return Some((name, vendor, version, category));
        }
    }
    for i in 0..n {
        let mut ci: PClassInfo = unsafe { std::mem::zeroed() };
        if unsafe { factory.getClassInfo(i, &mut ci) } != kResultOk { continue; }
        if !char8_to_string(&ci.category).starts_with("Audio Module Class") { continue; }
        let name = char8_to_string(&ci.name);
        if !name.is_empty() { return Some((name, factory_vendor, String::new(), "Effect".to_string())); }
    }
    None
}

/// Try to load a VST2 DLL and read `(name, vendor, version)` from its `AEffect`.
/// The plugin instance is shut down via `effClose` before the DLL is unloaded.
fn read_vst2_metadata(dll_path: &Path) -> Option<(String, String, String)> {
    use libloading::{Library, Symbol};
    use std::ffi::c_void;
    use vst::api::{AEffect, DispatcherProc};

    // Minimal audioMaster callback: only handles `audioMasterVersion` (opcode 1).
    extern "C" fn scan_host_cb(
        _e: *mut AEffect, opcode: i32, _: i32, _: isize, _: *mut c_void, _: f32,
    ) -> isize {
        if opcode == 1 { 2400 } else { 0 }
    }

    type PluginEntry = unsafe extern "C" fn(
        extern "C" fn(*mut AEffect, i32, i32, isize, *mut c_void, f32) -> isize,
    ) -> *mut AEffect;

    let lib = unsafe { Library::new(dll_path) }.ok()?;
    let entry: Symbol<PluginEntry> = unsafe {
        lib.get(b"VSTPluginMain\0")
            .or_else(|_| lib.get(b"main\0"))
            .ok()?
    };
    let effect = unsafe { entry(scan_host_cb) };
    if effect.is_null() {
        return None;
    }
    // Validate VST2 magic number (kEffectMagic = 0x56737450)
    if unsafe { (*effect).magic } != vst::api::consts::VST_MAGIC {
        return None;
    }

    let dispatch: DispatcherProc = unsafe { (*effect).dispatcher };

    // Inner helper: call a string-returning opcode.
    fn read_str(dispatch: DispatcherProc, effect: *mut AEffect, opcode: i32) -> String {
        let mut buf = [0u8; 64];
        dispatch(effect, opcode, 0, 0, buf.as_mut_ptr() as *mut c_void, 0.0);
        let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        String::from_utf8_lossy(&buf[..end]).trim().to_string()
    }

    let effect_name = read_str(dispatch, effect, 45); // effGetEffectName
    let vendor      = read_str(dispatch, effect, 47); // effGetVendorString
    let product     = read_str(dispatch, effect, 48); // effGetProductString

    // effGetVendorVersion (49) returns a packed integer; format as "major.minor".
    let ver_int = dispatch(effect, 49, 0, 0, std::ptr::null_mut(), 0.0);
    let version = if ver_int > 0 {
        format!("{}.{}", ver_int / 1000, (ver_int % 1000) / 10)
    } else {
        String::new()
    };

    // effClose (plugin opcode 1) — let the plugin clean up before DLL unload.
    dispatch(effect, 1, 0, 0, std::ptr::null_mut(), 0.0);

    let name = if !effect_name.is_empty() {
        effect_name
    } else if !product.is_empty() {
        product
    } else {
        return None;
    };

    Some((name, vendor, version))
}
