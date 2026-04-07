pub mod core;
pub use core::instance;
pub use core::scanner;
pub use core::types;
pub mod gui;
pub mod processor;
pub use core::crash_protection;
pub mod builtin;

pub use scanner::PluginScanner;
pub use instance::PluginInstanceManager;
pub use types::*;
pub use crash_protection::PluginStatus;
