// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
  // Request Per‑Monitor v2 DPI awareness on Windows so calls to
  // `GetDpiForWindow` and DPI-aware window-sizing APIs return correct values
  // on multi-monitor setups. Silently ignore on other platforms.
  #[cfg(target_os = "windows")]
  unsafe {
    use windows_sys::Win32::UI::HiDpi::SetProcessDpiAwarenessContext;
    use windows_sys::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2;
    // Best-effort: if unavailable on older Windows the call is a no-op.
    let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
  }

  // Plugin-scan helper child (see plugins::core::scanner::probe): do the
  // one probe and exit before any app/audio/tray setup.
  if app_lib::run_scan_helper_if_requested() {
    return;
  }

  app_lib::run();
}
