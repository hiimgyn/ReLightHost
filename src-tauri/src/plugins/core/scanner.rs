use std::path::{Path, PathBuf};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};
use std::collections::HashSet;
use rayon::prelude::*;
use anyhow::Result;
use crate::plugins::types::{PluginInfo, PluginFormat};
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

// 4: one entry per sub-plugin. 5: bundle binaries named `<Name>.vst3`
// resolved (v4 stored the bundle dir) + moduleinfo.json with trailing commas.
const SCAN_CACHE_VERSION: u32 = 5;

pub struct PluginScanner {
    scan_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ScanCache {
    version: u32,
    plan_hash: u64,
    entries: Vec<ScanCacheEntry>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ScanCacheEntry {
    path: String,
    fingerprint: ScanFingerprint,
    plugins: Vec<PluginInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct ScanFingerprint {
    modified_ms: u64,
    len_bytes: u64,
    exists: bool,
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
                paths.push(PathBuf::from(&local_app_data).join("Programs\\Common\\VST3")); // per-user VST3 (SDK 3.7)
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
    #[allow(dead_code)]
    pub fn add_scan_path<P: AsRef<Path>>(&mut self, path: P) {
        self.scan_paths.push(path.as_ref().to_path_buf());
    }

    fn combined_scan_paths(&self, custom_paths: &[String]) -> Vec<PathBuf> {
        let mut roots = Vec::new();
        let mut seen = HashSet::new();

        for path in self
            .scan_paths
            .iter()
            .cloned()
            .chain(custom_paths.iter().map(PathBuf::from))
        {
            let key = Self::scan_path_key(&path);
            if seen.insert(key) {
                roots.push(path);
            }
        }

        roots
    }

    fn scan_path_key(path: &Path) -> String {
        let normalized = path.to_string_lossy().replace('\\', "/");
        #[cfg(target_os = "windows")]
        {
            normalized.to_lowercase()
        }
        #[cfg(not(target_os = "windows"))]
        {
            normalized
        }
    }

    fn plan_hash(roots: &[PathBuf]) -> u64 {
        let mut hasher = DefaultHasher::new();
        SCAN_CACHE_VERSION.hash(&mut hasher);
        for root in roots {
            Self::scan_path_key(root).hash(&mut hasher);
        }
        hasher.finish()
    }

    fn cache_path() -> PathBuf {
        dirs::config_local_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("ReLightHost")
            .join("plugin-scan-cache.json")
    }

    fn load_cache(&self, plan_hash: u64) -> Option<ScanCache> {
        let path = Self::cache_path();
        let content = fs::read_to_string(path).ok()?;
        let cache: ScanCache = serde_json::from_str(&content).ok()?;
        if cache.version != SCAN_CACHE_VERSION || cache.plan_hash != plan_hash {
            return None;
        }
        Some(cache)
    }

    fn save_cache(&self, cache: &ScanCache) {
        let path = Self::cache_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(cache) {
            let _ = fs::write(path, json);
        }
    }

    fn fingerprint_path(root: &Path) -> ScanFingerprint {
        // Recursively hash mtime + size of every file inside the directory
        // (not just the directory entry) so new plugins in subdirectories are detected.
        let mut modified_acc: u64 = 0;
        let mut len_acc: u64 = 0;
        let mut exists = false;

        fn walk(dir: &Path, modified_acc: &mut u64, len_acc: &mut u64, exists: &mut bool) {
            let Ok(rd) = fs::read_dir(dir) else { return };
            for entry in rd.flatten() {
                let p = entry.path();
                if let Ok(meta) = fs::metadata(&p) {
                    *exists = true;
                    let mtime = meta.modified().ok()
                        .and_then(system_time_to_ms)
                        .unwrap_or(0);
                    // FNV-style mix: XOR-rotate to avoid cancellation from equal values.
                    *modified_acc ^= mtime.wrapping_add(p.as_os_str().len() as u64);
                    *modified_acc = modified_acc.rotate_left(7);
                    *len_acc = len_acc.wrapping_add(meta.len());
                    if meta.is_dir() && !is_link_back_to_ancestor(&p, dir) {
                        walk(&p, modified_acc, len_acc, exists);
                    }
                }
            }
        }

        if root.is_dir() {
            walk(root, &mut modified_acc, &mut len_acc, &mut exists);
        } else if let Ok(meta) = fs::metadata(root) {
            exists = true;
            modified_acc = meta.modified().ok().and_then(system_time_to_ms).unwrap_or(0);
            len_acc = meta.len();
        }

        ScanFingerprint { modified_ms: modified_acc, len_bytes: len_acc, exists }
    }

    /// Scan all configured paths for plugins
    pub fn scan(&self, custom_paths: &[String]) -> Result<Vec<PluginInfo>> {
        let mut plugins = Vec::new();
        let roots = self.combined_scan_paths(custom_paths);
        let plan_hash = Self::plan_hash(&roots);

        // Always include built-in processors first so they appear at the top.
        plugins.extend(Self::builtin_plugins());

        let mut cache = self.load_cache(plan_hash).unwrap_or(ScanCache {
            version: SCAN_CACHE_VERSION,
            plan_hash,
            entries: Vec::new(),
        });
        let mut cache_dirty = false;

        for path in &roots {
            if path.exists() && path.is_dir() {
                let fingerprint = Self::fingerprint_path(path);
                let path_key = Self::scan_path_key(path);
                let cache_entry = cache.entries.iter().find(|entry| entry.path == path_key);

                if let Some(entry) = cache_entry {
                    if entry.fingerprint == fingerprint {
                        plugins.extend(entry.plugins.clone());
                        log::info!("Using cached plugin scan for '{}' ({} plugins)", path.display(), entry.plugins.len());
                        continue;
                    }
                }

                // Always allow loose DLLs at the top level of any scan root.
                // The DLL-skipping logic only activates when recursing into
                // non-bundle subdirectories of a VST3/CLAP path (see scan_directory).
                let failures_before = PROBE_FAILURES.load(std::sync::atomic::Ordering::Relaxed);
                if let Ok(found) = self.scan_directory(path, true) {
                    plugins.extend(found.clone());
                    cache_dirty = true;
                    cache.entries.retain(|entry| entry.path != path_key);
                    // A crashed/hung probe left file-name metadata behind:
                    // don't cache this root, so the next scan retries it.
                    if PROBE_FAILURES.load(std::sync::atomic::Ordering::Relaxed) == failures_before {
                        cache.entries.push(ScanCacheEntry { path: path_key, fingerprint, plugins: found });
                    } else {
                        log::info!("Not caching '{}': a plugin probe failed, next scan retries it", path.display());
                    }
                }
            }
        }

        if cache_dirty {
            self.save_cache(&cache);
        }

        let plugins = dedupe_plugins(plugins);
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
        // Collect directory entries first so we can process them in parallel.
        let entries: Vec<std::path::PathBuf> = fs::read_dir(dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| !is_link_back_to_ancestor(p, dir))
            .collect();

        // Process each entry in parallel. For directories we may recurse (nested
        // parallelism is supported by Rayon). Errors are converted to empty
        // results so scanning continues even if some paths fail.
        let results: Vec<Vec<PluginInfo>> = entries
            .into_par_iter()
            .map(|path| {
                // Directory
                if path.is_dir() {
                    // Check if this is a VST3 bundle (folder named *.vst3 on Windows)
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        if ext.eq_ignore_ascii_case("vst3") {
                            let plugins = self.scan_vst3_bundle(&path);
                            if !plugins.is_empty() {
                                return plugins;
                            }
                        }
                    }
                    // Check if a non-.vst3 folder is still a VST3 bundle by structure
                    #[cfg(target_os = "windows")]
                    if path.join("Contents").join("x86_64-win").is_dir() {
                        let plugins = self.scan_vst3_bundle(&path);
                        if !plugins.is_empty() {
                            return plugins;
                        }
                    }

                    // Recursively scan other subdirectories. Child recursion keeps
                    // the same semantics for suppressing loose DLLs under VST3/CLAP
                    // roots.
                    let child_allow_loose_dll = if Self::path_is_vst3_or_clap_root(dir) {
                        false
                    } else {
                        allow_loose_dll
                    };
                    self.scan_directory(&path, child_allow_loose_dll).unwrap_or_default()
                // File
                } else if path.is_file() {
                    if !allow_loose_dll {
                        let is_native_lib = path
                            .extension()
                            .and_then(|e| e.to_str())
                            .map(|e| matches!(e.to_lowercase().as_str(), "dll" | "so" | "dylib"))
                            .unwrap_or(false);
                        if is_native_lib {
                            return Vec::new();
                        }
                    }
                    self.scan_file(&path)
                } else {
                    Vec::new()
                }
            })
            .collect();

        // Flatten results
        let mut plugins: Vec<PluginInfo> = Vec::new();
        for v in results {
            plugins.extend(v);
        }

        Ok(plugins)
    }

    /// Scan a VST3 bundle directory (Windows: PluginName.vst3/Contents/x86_64-win/PluginName.dll)
    fn scan_vst3_bundle(&self, bundle_path: &Path) -> Vec<PluginInfo> {
        let Some(plugin_name) = bundle_path.file_stem().and_then(|s| s.to_str()) else {
            return Vec::new();
        };
        let id = format!("vst3::{}", plugin_name);

        // On Windows, the DLL lives at Contents/x86_64-win/<PluginName>.dll
        // Fall back to scanning the arch dir for any DLL if the expected name isn't found
        #[cfg(target_os = "windows")]
        let dll_path = vst3_bundle_binary(bundle_path)
            .unwrap_or_else(|| bundle_path.join("Contents").join("x86_64-win").join(format!("{}.vst3", plugin_name)));

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

        // Try to read real metadata with no code execution first:
        //   1. moduleInfo.json — Steinberg SDK ≥ 3.7, static JSON in the bundle.
        //   2. PE VERSIONINFO resource (Windows) — LOAD_LIBRARY_AS_DATAFILE so
        //      DllMain never runs; extracts FileDescription / CompanyName / FileVersion.
        //   3. IPluginFactory2 — loads the DLL as a last resort (executes DllMain).
        // Only moduleInfo.json lists every class without running plugin
        // code; the fallbacks describe a single (first) plugin.
        // ponytail: multi-class VST3s without moduleInfo.json show as one
        // plugin; enumerate factory classes in read_vst3_dll_info if needed.
        let probe_dll = || probe(ProbeKind::Vst3, &dll_path).into_iter().next();
        #[cfg(target_os = "windows")]
        let single_meta = || {
            if dll_path.exists() { read_vst3_pe_version_info(&dll_path).or_else(probe_dll) } else { None }
        };
        #[cfg(not(target_os = "windows"))]
        let single_meta = || if dll_path.exists() { probe_dll() } else { None };

        let metas = read_vst3_module_info(bundle_path)
            .or_else(|| single_meta().map(|m| vec![m]))
            .unwrap_or_else(|| vec![(
                plugin_name.to_string(),
                String::new(),
                String::new(),
                "Effect".to_string(),
            )]);

        sub_plugins(&id, effective_path, PluginFormat::VST3, metas)
    }

    /// Try to scan a single file as a plugin (VST2 .dll, CLAP .clap)
    fn scan_file(&self, path: &Path) -> Vec<PluginInfo> {
        self.scan_file_inner(path).unwrap_or_default()
    }

    fn scan_file_inner(&self, path: &Path) -> Option<Vec<PluginInfo>> {
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

        // A VST2 folder also holds 32-bit plugins and helper DLLs; list only
        // x64 images that export a VST2 entry point (headers only, no load).
        #[cfg(target_os = "windows")]
        if matches!(format, PluginFormat::VST) && !is_x64_vst2_dll(path) {
            log::debug!("Skipping non-x64 / non-VST2 DLL: {}", path.display());
            return None;
        }

        let filename = path.file_stem()?.to_str()?;
        let id = format!("{}::{}", format!("{:?}", format).to_lowercase(), filename);

        // For VST2, try to read real name/vendor/version from the binary.
        // For CLAP, briefly load the DLL to read the plugin descriptor.
        // For single-file VST3 (.vst3 files that are PE DLLs), try VERSIONINFO
        // resource first (no code execution), then IPluginFactory2 as fallback.
        let metas: Vec<(String, String, String, String)> = if matches!(format, PluginFormat::CLAP) {
            let found = probe(ProbeKind::Clap, path);
            if found.is_empty() {
                vec![(filename.to_string(), String::new(), String::new(), "Effect".to_string())]
            } else {
                found
            }
        } else {
            let single = if matches!(format, PluginFormat::VST) {
                probe(ProbeKind::Vst2, path)
                    .into_iter()
                    .next()
                    .unwrap_or_else(|| (filename.to_string(), String::new(), String::new(), "Effect".to_string()))
            } else if matches!(format, PluginFormat::VST3) {
                #[cfg(target_os = "windows")]
                let meta = read_vst3_pe_version_info(path)
                    .or_else(|| probe(ProbeKind::Vst3, path).into_iter().next());
                #[cfg(not(target_os = "windows"))]
                let meta = probe(ProbeKind::Vst3, path).into_iter().next();
                meta.unwrap_or_else(|| (filename.to_string(), String::new(), String::new(), "Effect".to_string()))
            } else {
                (filename.to_string(), String::new(), String::new(), "Effect".to_string())
            };
            vec![single]
        };

        Some(sub_plugins(&id, path.to_string_lossy().to_string(), format, metas))
    }
}

/// Drops repeats of the same plugin (overlapping scan roots, e.g. a custom
/// path inside Common Files\VST3) and makes ids unique when two different
/// files share a name (`VST3\Clear.vst3` and `VST3\Supertone\Clear.vst3`
/// would both be `vst3::Clear`). The first keeps its id; later ones get
/// `@<path>` appended.
fn dedupe_plugins(plugins: Vec<PluginInfo>) -> Vec<PluginInfo> {
    let mut seen_files = HashSet::new();
    let mut seen_ids = HashSet::new();
    plugins
        .into_iter()
        .filter(|p| seen_files.insert((PluginScanner::scan_path_key(Path::new(&p.path)), p.sub_index)))
        .map(|mut p| {
            if !seen_ids.insert(p.id.clone()) {
                p.id = format!("{}@{}", p.id, p.path);
                seen_ids.insert(p.id.clone());
            }
            p
        })
        .collect()
}

/// True for a symlink/junction pointing at `dir` itself or one of its
/// ancestors. Following it would walk the same tree again — every plugin
/// listed dozens of times, or unbounded recursion with long paths enabled.
fn is_link_back_to_ancestor(entry: &Path, dir: &Path) -> bool {
    let is_link = fs::symlink_metadata(entry).map(|m| m.file_type().is_symlink()).unwrap_or(false);
    if !is_link {
        return false;
    }
    match (fs::canonicalize(entry), fs::canonicalize(dir)) {
        (Ok(target), Ok(here)) => here.starts_with(&target),
        _ => true, // dangling / unresolvable link: nothing to scan behind it
    }
}

fn system_time_to_ms(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as u64)
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
        use crate::plugins::builtin::{noise_suppressor, deep_filter, compressor, voice};
        vec![
            PluginInfo {
                id:       noise_suppressor::ID.to_string(),
                name:     "Noise Suppressor (RNNoise)".to_string(),
                vendor:   "System".to_string(),
                version:  env!("CARGO_PKG_VERSION").to_string(),
                path:     noise_suppressor::ID.to_string(),
                format:   PluginFormat::Builtin,
                category: "Noise Reduction".to_string(),
            sub_index: 0,
            },
            PluginInfo {
                id:       deep_filter::ID.to_string(),
                name:     "AI Noise Suppressor (DeepFilterNet)".to_string(),
                vendor:   "System".to_string(),
                version:  env!("CARGO_PKG_VERSION").to_string(),
                path:     deep_filter::ID.to_string(),
                format:   PluginFormat::Builtin,
                category: "Noise Reduction".to_string(),
            sub_index: 0,
            },
            PluginInfo {
                id:       compressor::ID.to_string(),
                name:     "Compressor".to_string(),
                vendor:   "System".to_string(),
                version:  env!("CARGO_PKG_VERSION").to_string(),
                path:     compressor::ID.to_string(),
                format:   PluginFormat::Builtin,
                category: "Dynamics".to_string(),
            sub_index: 0,
            },
            PluginInfo {
                id:       voice::ID.to_string(),
                name:     "Voice Designer".to_string(),
                vendor:   "System".to_string(),
                version:  env!("CARGO_PKG_VERSION").to_string(),
                path:     voice::ID.to_string(),
                format:   PluginFormat::Builtin,
                category: "Voice".to_string(),
            sub_index: 0,
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

/// The Windows binary inside a VST3 bundle directory. SDK ≥ 3.6.10 bundles
/// name it `Contents/x86_64-win/<Name>.vst3`; older ones used `<Name>.dll`.
/// Prefers the bundle's own stem, then any `.vst3`/`.dll` in the arch dir.
pub fn vst3_bundle_binary(bundle_path: &Path) -> Option<PathBuf> {
    let arch_dir = bundle_path.join("Contents").join("x86_64-win");
    let stem = bundle_path.file_stem()?.to_str()?;
    for ext in ["vst3", "dll"] {
        let p = arch_dir.join(format!("{stem}.{ext}"));
        if p.is_file() {
            return Some(p);
        }
    }
    // Any `.vst3` before any `.dll`: a bundle can ship helper DLLs next to
    // its real binary, and read_dir order is arbitrary.
    let files: Vec<PathBuf> = fs::read_dir(&arch_dir).ok()?.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.is_file()).collect();
    ["vst3", "dll"].into_iter().find_map(|ext| {
        files.iter().find(|p| p.extension().and_then(|x| x.to_str()).is_some_and(|x| x.eq_ignore_ascii_case(ext))).cloned()
    })
}

/// `(name, vendor, version, category)` of every "Audio Module Class" in a
/// VST3 bundle's `moduleinfo.json` (VST3 SDK ≥ 3.7), in order. No DLL is loaded.
fn read_vst3_module_info(bundle_path: &Path) -> Option<Vec<(String, String, String, String)>> {
    let candidates = [
        bundle_path.join("Contents").join("Resources").join("moduleinfo.json"),
        bundle_path.join("Contents").join("moduleinfo.json"),
    ];
    let json_path = candidates.iter().find(|p| p.exists())?;
    let data = fs::read_to_string(json_path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&strip_trailing_commas(&data))
        .inspect_err(|e| log::warn!("Unreadable {}: {e}", json_path.display()))
        .ok()?;

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
    let mut out = Vec::new();
    for cls in classes {
        let cat = cls.get("Category").and_then(|s| s.as_str()).unwrap_or("");
        if !cat.contains("Audio Module Class") {
            continue;
        }
        let Some(name) = cls
            .get("Name")
            .and_then(|s| s.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
        else {
            continue;
        };
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
        out.push((name, vendor, version, category));
    }
    (!out.is_empty()).then_some(out)
}

/// The SDK's moduleinfo writer emits JSON5-style trailing commas
/// (`"Fx", ],`), which serde_json rejects. Drops every `,` whose next
/// non-whitespace char is `]` or `}`, outside string literals.
fn strip_trailing_commas(json: &str) -> String {
    let mut out = String::with_capacity(json.len());
    let (mut in_str, mut escaped) = (false, false);
    for (i, c) in json.char_indices() {
        if in_str {
            in_str = escaped || c != '"';
            escaped = !escaped && c == '\\';
        } else if c == '"' {
            in_str = true;
        } else if c == ',' && matches!(json[i + 1..].trim_start().chars().next(), Some(']' | '}')) {
            continue;
        }
        out.push(c);
    }
    out
}

/// One `PluginInfo` per plugin found in a file; ids of sub-plugins after the
/// first get a `#index` suffix so they stay unique.
fn sub_plugins(
    base_id: &str,
    path: String,
    format: PluginFormat,
    metas: Vec<(String, String, String, String)>,
) -> Vec<PluginInfo> {
    metas
        .into_iter()
        .enumerate()
        .map(|(i, (name, vendor, version, category))| PluginInfo {
            id: if i == 0 { base_id.to_string() } else { format!("{base_id}#{i}") },
            name,
            vendor,
            version,
            path: path.clone(),
            format,
            category,
            sub_index: i as u32,
        })
        .collect()
}

// ── PE VERSIONINFO reader (Windows; zero code execution) ─────────────────────

/// Reads the `VERSIONINFO` resource embedded in a VST3 DLL without executing
/// any plugin code.  The DLL is memory-mapped as a data file
/// (`LOAD_LIBRARY_AS_DATAFILE`, flags = 0x2) so `DllMain` is never called.
/// Falls back gracefully on any error. Returns `(name, vendor, version, category)`.
#[cfg(target_os = "windows")]
fn read_vst3_pe_version_info(dll_path: &Path) -> Option<(String, String, String, String)> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{FreeLibrary, HGLOBAL, HMODULE, HRSRC};
    use windows_sys::Win32::System::LibraryLoader::{
        FindResourceW, LoadLibraryExW, LoadResource, LockResource, SizeofResource,
        LOAD_LIBRARY_AS_DATAFILE,
    };

    let path_wide: Vec<u16> = OsStr::new(dll_path).encode_wide().chain(Some(0u16)).collect();

    // LOAD_LIBRARY_AS_DATAFILE: maps file as read-only data, DllMain never called.
    let hmod: HMODULE = unsafe {
        LoadLibraryExW(path_wide.as_ptr(), std::ptr::null_mut(), LOAD_LIBRARY_AS_DATAFILE)
    };
    if hmod.is_null() {
        return None;
    }

    // RAII guard — FreeLibrary is in Win32_Foundation in windows-sys 0.61.
    struct LibGuard(HMODULE);
    impl Drop for LibGuard {
        fn drop(&mut self) {
            unsafe { FreeLibrary(self.0); }
        }
    }
    let _guard = LibGuard(hmod);

    // MAKEINTRESOURCEW(n) = n as *const u16 (Win32 API contract: low 16-bit ID).
    // Resource name = 1 (first VERSIONINFO), resource type = RT_VERSION = 16.
    let hrsrc: HRSRC = unsafe { FindResourceW(hmod, 1u16 as *const u16, 16u16 as *const u16) };
    if hrsrc.is_null() {
        return None;
    }

    let size = unsafe { SizeofResource(hmod, hrsrc) } as usize;
    if size < 40 {
        return None;
    }

    let hdata: HGLOBAL = unsafe { LoadResource(hmod, hrsrc) };
    if hdata.is_null() {
        return None;
    }

    let ptr = unsafe { LockResource(hdata) } as *const u8;
    if ptr.is_null() {
        return None;
    }

    let data = unsafe { std::slice::from_raw_parts(ptr, size) };
    let (name, vendor, version) = vi_parse(data)?;
    if name.is_empty() {
        return None;
    }
    // Category cannot be derived from VERSIONINFO strings; default to "Effect".
    Some((name, vendor, version, "Effect".to_string()))
}

/// True when `path` is an x64 (AMD64) PE image whose export table names
/// `VSTPluginMain` or `main`. Plain file reads of the headers and the export
/// name table — the DLL is never loaded, so 32-bit plugins and helper DLLs
/// are told apart without running any of their code.
#[cfg(target_os = "windows")]
fn is_x64_vst2_dll(path: &Path) -> bool {
    pe_x64_exports_any(path, &[b"VSTPluginMain\0", b"main\0"])
}

/// `wanted` names include their trailing NUL and are at most 32 bytes.
#[cfg(target_os = "windows")]
fn pe_x64_exports_any(path: &Path, wanted: &[&[u8]]) -> bool {
    use std::io::{Read, Seek, SeekFrom};
    fn rd<const N: usize>(f: &mut fs::File, off: u64) -> Option<[u8; N]> {
        f.seek(SeekFrom::Start(off)).ok()?;
        let mut b = [0u8; N];
        f.read(&mut b).ok()?; // short read at EOF leaves zeros
        Some(b)
    }
    let u16_at = |f: &mut fs::File, off| rd::<2>(f, off).map(u16::from_le_bytes);
    let u32_at = |f: &mut fs::File, off| rd::<4>(f, off).map(u32::from_le_bytes);

    let check = || -> Option<bool> {
        let mut f = fs::File::open(path).ok()?;
        if rd::<2>(&mut f, 0)? != *b"MZ" { return None; }
        let pe = u32_at(&mut f, 0x3C)? as u64;
        if rd::<4>(&mut f, pe)? != *b"PE\0\0" { return None; }
        if u16_at(&mut f, pe + 4)? != 0x8664 { return Some(false); } // IMAGE_FILE_MACHINE_AMD64
        let n_sections = u16_at(&mut f, pe + 6)? as u64;
        let opt = pe + 24;
        let opt_size = u16_at(&mut f, pe + 20)? as u64;
        if u16_at(&mut f, opt)? != 0x20B { return Some(false); } // PE32+
        let export_rva = u32_at(&mut f, opt + 112)?; // data directory 0
        if export_rva == 0 { return Some(false); }
        // (virtual address, virtual size, raw offset) per section, for RVA → file offset.
        let sections: Vec<(u32, u32, u32)> = (0..n_sections.min(96))
            .filter_map(|i| {
                let s = opt + opt_size + i * 40;
                Some((u32_at(&mut f, s + 12)?, u32_at(&mut f, s + 8)?.max(u32_at(&mut f, s + 16)?), u32_at(&mut f, s + 20)?))
            })
            .collect();
        let to_off = |rva: u32| {
            sections.iter().find(|&&(va, size, _)| rva >= va && rva - va < size).map(|&(va, _, raw)| (rva - va) as u64 + raw as u64)
        };
        let dir = to_off(export_rva)?;
        let n_names = u32_at(&mut f, dir + 24)?.min(1 << 16) as u64;
        let names = to_off(u32_at(&mut f, dir + 32)?)?;
        for i in 0..n_names {
            let name_off = to_off(u32_at(&mut f, names + i * 4)?)?;
            let name = rd::<32>(&mut f, name_off)?;
            if wanted.iter().any(|w| name.starts_with(w)) {
                return Some(true);
            }
        }
        Some(false)
    };
    check().unwrap_or(false)
}

// ── VS_VERSION_INFO binary parser (pure Rust; no platform dependencies) ───────

#[cfg(target_os = "windows")]
fn vi_u16(data: &[u8], off: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*data.get(off)?, *data.get(off + 1)?]))
}

#[cfg(target_os = "windows")]
fn vi_align4(n: usize) -> usize {
    (n + 3) & !3
}

/// Read a null-terminated UTF-16LE string from `data` starting at byte `off`.
/// Returns `(decoded_string, byte_offset_after_null_terminator)`.
#[cfg(target_os = "windows")]
fn vi_wstr(data: &[u8], mut off: usize) -> Option<(String, usize)> {
    let mut chars = Vec::new();
    loop {
        let c = vi_u16(data, off)?;
        off += 2;
        if c == 0 {
            break;
        }
        chars.push(c);
    }
    Some((String::from_utf16_lossy(&chars).to_string(), off))
}

/// Parse a raw `VS_VERSION_INFO` resource block.
/// Extracts the best available name, vendor, and version from `StringFileInfo`.
/// Name priority: InternalName → ProductName → FileDescription.
/// All structures follow the VS_VERSION_INFO wire format (DWORD-aligned blocks).
#[cfg(target_os = "windows")]
fn vi_parse(data: &[u8]) -> Option<(String, String, String)> {
    if data.len() < 6 {
        return None;
    }

    // ── Root: VS_VERSION_INFO ─────────────────────────────────────────────────
    let root_len     = vi_u16(data, 0)? as usize;
    let root_val_len = vi_u16(data, 2)? as usize; // VS_FIXEDFILEINFO byte count (usually 52)
    let (root_key, root_key_end) = vi_wstr(data, 6)?;
    if root_key != "VS_VERSION_INFO" {
        return None;
    }

    // Skip key (DWORD-aligned), skip Value (VS_FIXEDFILEINFO), DWORD-align again.
    let children_start = vi_align4(vi_align4(root_key_end) + root_val_len);
    let root_end       = root_len.min(data.len());

    // ── Iterate root children looking for "StringFileInfo" ────────────────────
    let mut child_off = children_start;
    let mut file_desc     = String::new();
    let mut product_name  = String::new();
    let mut internal_name = String::new();
    let mut company       = String::new();
    let mut file_ver      = String::new();

    while child_off + 6 <= root_end {
        let block_len = vi_u16(data, child_off)? as usize;
        if block_len == 0 {
            break;
        }

        let (block_key, block_key_end) = vi_wstr(data, child_off + 6)?;

        if block_key == "StringFileInfo" {
            let sf_end    = (child_off + block_len).min(root_end);
            let mut tbl_off = vi_align4(block_key_end); // first StringTable

            // ── StringTable loop ─────────────────────────────────────────────
            while tbl_off + 6 <= sf_end {
                let tbl_len = vi_u16(data, tbl_off)? as usize;
                if tbl_len == 0 {
                    break;
                }
                let tbl_end = (tbl_off + tbl_len).min(sf_end);
                // Skip StringTable lang+codepage key (e.g. "040904B0\0").
                let (_, tbl_key_end) = vi_wstr(data, tbl_off + 6)?;
                let mut str_off = vi_align4(tbl_key_end);

                // ── String entry loop ────────────────────────────────────────
                while str_off + 6 <= tbl_end {
                    let str_len = vi_u16(data, str_off)? as usize;
                    if str_len == 0 {
                        break;
                    }
                    let (entry_key, entry_key_end) = vi_wstr(data, str_off + 6)?;
                    let val_start = vi_align4(entry_key_end);
                    // Read value to null terminator (avoids wValueLength bytes-vs-WORDs ambiguity).
                    let value = vi_wstr(data, val_start)
                        .map(|(s, _)| s.trim().to_string())
                        .unwrap_or_default();

                    match entry_key.as_str() {
                        "FileDescription" => file_desc     = value,
                        "ProductName"     => product_name  = value,
                        "InternalName"    => internal_name = value,
                        "CompanyName"     => company       = value,
                        "FileVersion"     => file_ver      = value,
                        _ => {}
                    }

                    str_off = vi_align4(str_off + str_len);
                }

                tbl_off = vi_align4(tbl_off + tbl_len);
            }
        }
        // VarFileInfo or unknown block — skip.
        child_off = vi_align4(child_off + block_len);
    }

    // Pick best name: InternalName is cleanest, then ProductName, then FileDescription.
    let name = [internal_name, product_name, file_desc]
        .into_iter()
        .find(|s| !s.is_empty())
        .unwrap_or_default();
    if name.is_empty() { None } else { Some((name, company, file_ver)) }
}

/// A scan step that has to run plugin code to read metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeKind {
    Vst2,
    Clap,
    Vst3,
}

impl ProbeKind {
    fn arg(self) -> &'static str {
        match self {
            Self::Vst2 => "vst2",
            Self::Clap => "clap",
            Self::Vst3 => "vst3",
        }
    }

    fn from_arg(arg: &str) -> Option<Self> {
        match arg {
            "vst2" => Some(Self::Vst2),
            "clap" => Some(Self::Clap),
            "vst3" => Some(Self::Vst3),
            _ => None,
        }
    }
}

type Meta = (String, String, String, String);

/// `<exe> --scan-one <kind> <path>` → the probe to run.
fn parse_scan_one_args(args: &[String]) -> Option<(ProbeKind, String)> {
    match args {
        [_, flag, kind, path, ..] if flag == "--scan-one" => Some((ProbeKind::from_arg(kind)?, path.clone())),
        _ => None,
    }
}

/// Child-process entry point: when started as `--scan-one <kind> <path>`,
/// probes that one file, prints its metadata as JSON on stdout and returns
/// true (the caller exits). Returns false for a normal app start.
pub fn scan_one_cli(args: &[String]) -> bool {
    let Some((kind, path)) = parse_scan_one_args(args) else { return false };
    let metas = probe_in_process(kind, Path::new(&path));
    // Plugins may print to stdout too; the result is the line behind the
    // sentinel (leading newline in case the plugin left a partial line).
    println!("\n{PROBE_SENTINEL}{}", serde_json::to_string(&metas).unwrap_or_else(|_| "[]".into()));
    true
}

/// Prefix of the `--scan-one` child's result line.
const PROBE_SENTINEL: &str = "@@relighthost-scan-result@@";

/// Probes that crashed, hung or returned nothing readable. `scan` compares
/// it around each root and skips caching a root whose metadata is only the
/// file-name fallback, so the next scan retries.
static PROBE_FAILURES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Reads the child's stdout line by line until the sentinel line, ignoring
/// whatever the plugin printed. `None` when stdout closes without one.
fn read_probe_result(out: impl std::io::Read) -> Option<Vec<Meta>> {
    use std::io::BufRead;
    std::io::BufReader::new(out)
        .split(b'\n')
        .map_while(Result::ok)
        .find_map(|line| serde_json::from_slice(line.strip_prefix(PROBE_SENTINEL.as_bytes())?).ok())
}

/// Runs a `--scan-one` child and returns its result, or `None` when it
/// crashed, hung past `timeout`, or exited without a result line. stdout is
/// drained on a thread while waiting, so neither a chatty plugin (full
/// pipe) nor a helper process that inherited the pipe can stall the scan.
// ponytail: a helper that inherited the pipe outlives the kill and keeps
// the reader thread parked until it exits; a kill-on-close Job object
// (windows-sys feature Win32_System_JobObjects) would take it down too.
fn run_probe(mut cmd: std::process::Command, timeout: std::time::Duration) -> Option<Vec<Meta>> {
    use std::process::Stdio;
    use std::sync::mpsc::{self, RecvTimeoutError};
    use std::time::{Duration, Instant};

    let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn()
        .inspect_err(|e| log::warn!("Could not start scan helper: {e}"))
        .ok()?;
    let stdout = child.stdout.take()?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        if let Some(metas) = read_probe_result(stdout) {
            let _ = tx.send(metas);
        }
    });

    let deadline = Instant::now() + timeout;
    let mut exited_at: Option<Instant> = None;
    let result = loop {
        match rx.recv_timeout(Duration::from_millis(20)) {
            Ok(metas) => break Some(metas),
            Err(RecvTimeoutError::Disconnected) => break None, // stdout closed, no result
            Err(RecvTimeoutError::Timeout) => {}
        }
        if exited_at.is_none() && matches!(child.try_wait(), Ok(Some(_))) {
            exited_at = Some(Instant::now());
        }
        // Exited but the pipe is still open (a helper inherited it): give
        // the last bytes a moment, then stop waiting for a result.
        if exited_at.is_some_and(|t| t.elapsed() > Duration::from_secs(1)) {
            break None;
        }
        if Instant::now() >= deadline {
            log::warn!("Scan helper hung (>{timeout:?})");
            break None;
        }
    };
    let _ = child.kill();
    if let Ok(status) = child.wait() {
        if result.is_none() && !status.success() {
            log::warn!("Scan helper exited with {status}");
        }
    }
    result
}

/// Runs a probe in this process (the `--scan-one` child, and tests).
fn probe_in_process(kind: ProbeKind, path: &Path) -> Vec<Meta> {
    let found = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        with_code_load_lock(|| match kind {
            ProbeKind::Vst2 => read_vst2_metadata(path).map(|(n, v, ver)| vec![(n, v, ver, "Effect".to_string())]),
            ProbeKind::Clap => crate::plugins::processor::clap::read_clap_metadata(path)
                .map(|all| all.into_iter().map(|(n, v, ver)| (n, v, ver, "Effect".to_string())).collect()),
            ProbeKind::Vst3 => read_vst3_dll_info_unlocked(path).map(|m| vec![m]),
        })
    }));
    match found {
        Ok(metas) => metas.unwrap_or_default(),
        Err(_) => {
            log::error!("Plugin panicked while being scanned: {}", path.display());
            Vec::new()
        }
    }
}

/// Reads metadata that needs plugin code to run. In the app this happens in
/// a child process (`--scan-one`), so a plugin that crashes or hangs while
/// being probed takes only that child down — the scan logs it and carries
/// on with file-name metadata. Tests probe in-process (the test binary
/// can't act as the child).
fn probe(kind: ProbeKind, path: &Path) -> Vec<Meta> {
    #[cfg(test)]
    {
        probe_in_process(kind, path)
    }
    #[cfg(not(test))]
    {
        probe_out_of_process(kind, path)
    }
}

#[cfg(not(test))]
fn probe_out_of_process(kind: ProbeKind, path: &Path) -> Vec<Meta> {
    use std::process::Command;
    use std::time::Duration;
    // Generous: loader-style plugins (Supertone Clear: 260 KB stub + 25 MB
    // model) take 6-30 s just to hand out their factory, longer while other
    // probes run alongside. A failed probe falls back to file-name metadata
    // and is not cached (PROBE_FAILURES), so the next scan retries it.
    const TIMEOUT: Duration = Duration::from_secs(60);

    let Ok(exe) = std::env::current_exe() else { return probe_in_process(kind, path) };
    let mut cmd = Command::new(exe);
    cmd.arg("--scan-one").arg(kind.arg()).arg(path);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    run_probe(cmd, TIMEOUT).unwrap_or_else(|| {
        PROBE_FAILURES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        log::warn!("Plugin crashed or hung while being scanned, skipped its metadata: {}", path.display());
        Vec::new()
    })
}

/// Serializes every scan step that executes plugin code (DLL load +
/// entry point / factory). The directory walk and static metadata reads
/// (moduleinfo.json, VERSIONINFO) stay parallel; running several plugins'
/// DllMain/initializers at once in-process multiplies the chance that one
/// misbehaving DLL takes the app down mid-scan.
// ponytail: in-process + serialized; out-of-process scanning (spawn self
// with --scan-one <path>) is the real isolation if scan crashes persist.
fn with_code_load_lock<T>(f: impl FnOnce() -> T) -> T {
    // Shared with plugin loading, so a scan (now an async command) never runs
    // plugin entry code alongside a plugin being loaded into the chain.
    let _guard = crate::plugins::core::LIBRARY_LOAD_LOCK.lock();
    f()
}

fn read_vst3_dll_info_unlocked(dll_path: &Path) -> Option<(String, String, String, String)> {
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

    // Truncated AEffect: only the leading two fields (magic, dispatcher) are
    // ever read here, so the full ~20-field VST 2.4 struct layout isn't
    // needed — see plugins::processor::vst2 for the complete definition used
    // by the actual host/processor.
    #[repr(C)]
    struct AEffect {
        magic: i32,
        dispatcher: DispatcherProc,
    }
    type DispatcherProc = unsafe extern "C" fn(*mut AEffect, i32, i32, isize, *mut c_void, f32) -> isize;
    const VST_MAGIC: i32 = 0x5673_7450_u32 as i32; // 'VstP'

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
    if unsafe { (*effect).magic } != VST_MAGIC {
        return None;
    }

    let dispatch: DispatcherProc = unsafe { (*effect).dispatcher };

    // Inner helper: call a string-returning opcode.
    fn read_str(dispatch: DispatcherProc, effect: *mut AEffect, opcode: i32) -> String {
        let mut buf = [0u8; 64];
        unsafe { dispatch(effect, opcode, 0, 0, buf.as_mut_ptr() as *mut c_void, 0.0) };
        let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        String::from_utf8_lossy(&buf[..end]).trim().to_string()
    }

    let effect_name = read_str(dispatch, effect, 45); // effGetEffectName
    let vendor      = read_str(dispatch, effect, 47); // effGetVendorString
    let product     = read_str(dispatch, effect, 48); // effGetProductString

    // effGetVendorVersion (49) returns a packed integer; format as "major.minor".
    let ver_int = unsafe { dispatch(effect, 49, 0, 0, std::ptr::null_mut(), 0.0) };
    let version = if ver_int > 0 {
        format!("{}.{}", ver_int / 1000, (ver_int % 1000) / 10)
    } else {
        String::new()
    };

    // effClose (plugin opcode 1) — let the plugin clean up before DLL unload.
    unsafe { dispatch(effect, 1, 0, 0, std::ptr::null_mut(), 0.0) };

    let name = if !effect_name.is_empty() {
        effect_name
    } else if !product.is_empty() {
        product
    } else {
        return None;
    };

    Some((name, vendor, version))
}

#[cfg(test)]
mod multi_plugin_tests {
    use super::*;

    #[test]
    fn every_audio_class_in_a_vst3_bundle_becomes_its_own_plugin() {
        let bundle = std::env::temp_dir().join(format!("rh-multi-{}", std::process::id())).join("Suite.vst3");
        let contents = bundle.join("Contents");
        std::fs::create_dir_all(&contents).unwrap();
        std::fs::write(
            contents.join("moduleInfo.json"),
            r#"{
                "Version": "1.2.3",
                "Factory Info": { "Vendor": "Acme" },
                "Classes": [
                    { "Category": "Audio Module Class", "Name": "Suite EQ", "Sub Categories": ["Fx", "EQ"] },
                    { "Category": "Component Controller Class", "Name": "Suite EQ Controller" },
                    { "Category": "Audio Module Class", "Name": "Suite Comp", "Sub Categories": ["Fx", "Dynamics"] }
                ]
            }"#,
        )
        .unwrap();

        let plugins = PluginScanner::new().scan_vst3_bundle(&bundle);
        let _ = std::fs::remove_dir_all(bundle.parent().unwrap());

        assert_eq!(plugins.len(), 2);
        assert_eq!((plugins[0].name.as_str(), plugins[0].sub_index), ("Suite EQ", 0));
        assert_eq!((plugins[1].name.as_str(), plugins[1].sub_index), ("Suite Comp", 1));
        assert_ne!(plugins[0].id, plugins[1].id, "ids must be unique per sub-plugin");
        assert_eq!(plugins[1].vendor, "Acme");
    }

    #[test]
    fn sdk_moduleinfo_with_trailing_commas_parses() {
        let sdk = r#"{ "Name": "a, ]\" }", "Classes": [ { "Sub Categories": [ "Fx", ], }, ], }"#;
        let v: serde_json::Value = serde_json::from_str(&strip_trailing_commas(sdk)).unwrap();
        assert_eq!(v["Name"], "a, ]\" }", "commas inside strings are kept");
        assert_eq!(v["Classes"][0]["Sub Categories"][0], "Fx");
    }

    #[test]
    fn same_named_plugins_in_two_folders_get_distinct_ids_and_repeats_are_dropped() {
        let p = |path: &str| sub_plugins("vst3::Clear", path.into(), PluginFormat::VST3, vec![("Clear".into(), String::new(), String::new(), "Effect".into())]);
        let all = [p("C:/VST3/Clear.vst3"), p("C:/VST3/Supertone/Clear.vst3"), p("C:/VST3/Supertone/Clear.vst3")].concat();
        let out = dedupe_plugins(all);
        assert_eq!(out.len(), 2, "overlapping roots list a file once");
        assert_eq!(out[0].id, "vst3::Clear");
        assert_eq!(out[1].id, "vst3::Clear@C:/VST3/Supertone/Clear.vst3");
    }
}

#[cfg(test)]
mod probe_tests {
    use super::*;

    #[test]
    fn scan_one_arguments_are_recognised() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(matches!(
            parse_scan_one_args(&args(&["relighthost.exe", "--scan-one", "clap", "C:/P.clap"])),
            Some((ProbeKind::Clap, ref p)) if p == "C:/P.clap"
        ));
        assert!(parse_scan_one_args(&args(&["relighthost.exe"])).is_none());
        for kind in [ProbeKind::Vst2, ProbeKind::Clap, ProbeKind::Vst3] {
            assert_eq!(ProbeKind::from_arg(kind.arg()), Some(kind), "child args round-trip");
        }
        assert!(parse_scan_one_args(&args(&["relighthost.exe", "--scan-one", "bogus", "x"])).is_none());
    }

    #[test]
    fn probing_a_missing_file_finds_nothing() {
        assert!(probe_in_process(ProbeKind::Vst2, Path::new("C:/does/not/exist.dll")).is_empty());
    }

    #[test]
    fn probe_result_is_found_behind_plugin_noise() {
        let out = format!("plugin says hi\n{{not json\n\n{PROBE_SENTINEL}[[\"N\",\"V\",\"1\",\"Effect\"]]\r\ntrailing");
        let metas = read_probe_result(out.as_bytes()).unwrap();
        assert_eq!(metas, vec![("N".into(), "V".into(), "1".into(), "Effect".into())]);
        assert!(read_probe_result(&b"[[\"N\",\"V\",\"1\",\"Effect\"]]\n"[..]).is_none(), "no sentinel, no result");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn probe_child_that_hangs_or_leaves_a_helper_holding_the_pipe_does_not_stall() {
        use std::os::windows::process::CommandExt;
        use std::time::{Duration, Instant};
        let cmd = |script: String| {
            let mut c = std::process::Command::new("cmd");
            c.raw_arg(script);
            c
        };

        // Hangs: no result, and back within the timeout.
        let t = Instant::now();
        assert!(run_probe(cmd("/C ping -n 6 127.0.0.1".into()), Duration::from_millis(500)).is_none());
        assert!(t.elapsed() < Duration::from_secs(3), "timeout not enforced: {:?}", t.elapsed());

        // A background helper inherits stdout and outlives the child; the
        // result line is still read straight away.
        let t = Instant::now();
        let script = format!(r#"/C start /b ping -n 6 127.0.0.1 & echo {PROBE_SENTINEL}[["N","V","1","Effect"]]"#);
        let metas = run_probe(cmd(script), Duration::from_secs(20)).expect("result behind noise");
        assert_eq!(metas[0].0, "N");
        assert!(t.elapsed() < Duration::from_secs(3), "waited on the helper: {:?}", t.elapsed());

        // Exits without a result line (a crash): None, not a parse of junk.
        assert!(run_probe(cmd("/C echo [] & exit 3".into()), Duration::from_secs(20)).is_none());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn only_x64_dlls_with_a_matching_export_pass_the_header_check() {
        let sys = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        let k64 = Path::new(&sys).join(r"System32\kernel32.dll");
        let k32 = Path::new(&sys).join(r"SysWOW64\kernel32.dll");
        assert!(pe_x64_exports_any(&k64, &[b"Sleep\0"]), "x64 export table read");
        assert!(!is_x64_vst2_dll(&k64), "helper DLL without a VST2 entry point");
        if k32.exists() {
            assert!(!pe_x64_exports_any(&k32, &[b"Sleep\0"]), "32-bit image rejected");
        }
        assert!(!is_x64_vst2_dll(Path::new("C:/does/not/exist.dll")));
    }

    #[test]
    fn bundle_fallback_prefers_any_vst3_over_a_helper_dll() {
        let bundle = std::env::temp_dir().join(format!("rh-bin-{}", std::process::id())).join("Odd.vst3");
        let arch = bundle.join("Contents").join("x86_64-win");
        std::fs::create_dir_all(&arch).unwrap();
        for f in ["aaa_helper.dll", "zzz_real.vst3"] {
            std::fs::write(arch.join(f), b"x").unwrap();
        }
        let bin = vst3_bundle_binary(&bundle);
        let _ = std::fs::remove_dir_all(bundle.parent().unwrap());
        assert_eq!(bin.unwrap().file_name().unwrap(), "zzz_real.vst3");
    }
}

#[cfg(all(test, target_os = "windows"))]
mod link_cycle_tests {
    use super::*;

    #[test]
    fn a_junction_back_to_an_ancestor_is_not_followed() {
        let root = std::env::temp_dir().join(format!("rh-loop-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let contents = root.join("One.vst3").join("Contents");
        std::fs::create_dir_all(&contents).unwrap();
        std::fs::write(
            contents.join("moduleInfo.json"),
            r#"{"Classes":[{"Category":"Audio Module Class","Name":"One"}]}"#,
        )
        .unwrap();
        let link = root.join("loop");
        let made = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&root)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !made {
            eprintln!("mklink /J unavailable — skipping");
            return;
        }

        let found = PluginScanner::new().scan_directory(&root, true).unwrap();
        let _fingerprint = PluginScanner::fingerprint_path(&root); // must terminate
        let _ = std::fs::remove_dir(&link);
        let _ = std::fs::remove_dir_all(&root);

        assert_eq!(found.iter().filter(|p| p.name == "One").count(), 1, "found via the loop: {}", found.len());
    }
}
