pub mod crash_protection;
pub mod instance;
pub mod scope;
pub mod scanner;
pub mod types;

/// Serialises every step that maps a plugin library and runs its entry code
/// (DLL load, `VSTPluginMain`, `clap_entry.init` + factory create, VST3
/// `GetPluginFactory` + `createInstance`) — across all formats and the
/// scanner. Plugin code *after* instance creation (VST3 `initialize()`,
/// CLAP `init`/`activate`) runs outside it, so parallel restore still
/// overlaps the slow part.
pub(crate) static LIBRARY_LOAD_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());
