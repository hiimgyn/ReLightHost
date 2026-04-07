use std::time::Duration;

/// Debounce rapid plugin-chain UI mutations before writing autosave.
pub const AUTOSAVE_DEBOUNCE: Duration = Duration::from_millis(200);

/// Wait before replaying VST3 binary state so initialization completes.
pub const VST3_STATE_REPLAY_DELAY: Duration = Duration::from_millis(1_000);

/// Extra delay when output device is a Voicemeeter ASIO Insert driver.
/// Voicemeeter must finish its own startup before our ASIO stream connects.
pub const VOICEMEETER_STARTUP_DELAY_MS: u64 = 2_000;

/// VST3 plugins need extra warm-up time before the ASIO stream opens.
pub const VST3_STARTUP_DELAY_MS: u64 = 4_000;

/// After the stream opens, block VST3 process() for this long to let it settle.
pub const VST3_POST_START_SETTLE_MS: u64 = 8_000;

/// Timeout for graceful plugin GUI close request before giving up.
pub const GUI_CLOSE_TIMEOUT: Duration = Duration::from_secs(3);
