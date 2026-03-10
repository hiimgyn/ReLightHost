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

        for path in &self.scan_paths {
            if path.exists() && path.is_dir() {
                if let Ok(found) = self.scan_directory(path) {
                    plugins.extend(found);
                }
            }
        }

        log::info!("Found {} plugins", plugins.len());
        Ok(plugins)
    }

    /// Scan a specific directory for plugins
    fn scan_directory(&self, dir: &Path) -> Result<Vec<PluginInfo>> {
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
                // Recursively scan other subdirectories
                if let Ok(sub_plugins) = self.scan_directory(&path) {
                    plugins.extend(sub_plugins);
                }
            } else if path.is_file() {
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

        Some(PluginInfo {
            id,
            name: plugin_name.to_string(),
            vendor: "Unknown".to_string(),
            version: "1.0.0".to_string(),
            path: effective_path,
            format: PluginFormat::VST3,
            category: "Effect".to_string(),
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

        Some(PluginInfo {
            id,
            name: filename.to_string(),
            vendor: "Unknown".to_string(),
            version: "1.0.0".to_string(),
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
