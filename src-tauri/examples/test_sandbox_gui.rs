//! Manual, throwaway verification for the VST3 sandbox feature — not part
//! of the app, not wired into any build. Loads Clear.vst3 through
//! `SandboxedVst3Processor`, opens its GUI, processes a few seconds of
//! silent audio through it, then closes it. Run with:
//!   cargo run --example test_sandbox_gui

use app_lib::plugins::processor::vst3_sandbox::SandboxedVst3Processor;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

const PLUGIN_PATH: &str = r"C:\Program Files\Common Files\VST3\Supertone\Clear.vst3";

fn main() {
    env_logger_init();

    println!("Loading '{}' sandboxed...", PLUGIN_PATH);
    let proc = SandboxedVst3Processor::load(PLUGIN_PATH, 48000.0, 512)
        .expect("sandboxed load failed");
    println!("Loaded OK.");

    println!("Processing 20 silent blocks...");
    for i in 0..20 {
        let mut left = vec![0.0f32; 512];
        let mut right = vec![0.0f32; 512];
        proc.process_stereo(&mut left, &mut right);
        println!("  block {i}: left[0]={} right[0]={}", left[0], right[0]);
        std::thread::sleep(Duration::from_millis(20));
    }

    println!("Opening GUI...");
    let gui_flag: Arc<AtomicBool> = Arc::new(AtomicBool::new(true));
    let gui_hwnd: Arc<AtomicIsize> = Arc::new(AtomicIsize::new(0));
    proc.open_gui("Clear (sandboxed test)", gui_flag.clone(), gui_hwnd.clone())
        .expect("open_gui failed");

    // Wait for the HWND to show up (reported asynchronously by the child).
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let hwnd = gui_hwnd.load(Ordering::Acquire);
        if hwnd != 0 {
            println!("GUI HWND reported: {hwnd:#x}");
            break;
        }
        if std::time::Instant::now() > deadline {
            println!("Timed out waiting for GUI HWND");
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    println!("Keeping GUI open for 60s so it can be screenshotted...");
    for i in 0..600 {
        let mut left = vec![0.0f32; 512];
        let mut right = vec![0.0f32; 512];
        proc.process_stereo(&mut left, &mut right);
        if i % 25 == 0 {
            println!("  still running, gui_flag={}", gui_flag.load(Ordering::Acquire));
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    println!("Closing via WM_CLOSE to stored HWND...");
    let hwnd = gui_hwnd.load(Ordering::Acquire);
    if hwnd != 0 {
        #[cfg(target_os = "windows")]
        unsafe {
            use windows_sys::Win32::Foundation::HWND;
            use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};
            PostMessageW(hwnd as HWND, WM_CLOSE, 0, 0);
        }
    }
    let close_wait_start = std::time::Instant::now();
    let close_deadline = close_wait_start + Duration::from_secs(15);
    loop {
        if !gui_flag.load(Ordering::Acquire) {
            println!("gui_flag cleared (GuiClosed round-trip confirmed) after {:?}", close_wait_start.elapsed());
            break;
        }
        if std::time::Instant::now() > close_deadline {
            println!("gui_flag NEVER cleared within 15s — GuiClosed notification did not arrive");
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }

    println!("Dropping processor (should terminate the child process)...");
    drop(proc);
    std::thread::sleep(Duration::from_secs(1));
    println!("Done.");
}

fn env_logger_init() {
    // No env_logger dependency in this project — just route `log` crate
    // output to stderr with a minimal format, good enough for this
    // throwaway example.
    struct SimpleLogger;
    impl log::Log for SimpleLogger {
        fn enabled(&self, _m: &log::Metadata) -> bool { true }
        fn log(&self, r: &log::Record) {
            eprintln!("[{}] {}", r.level(), r.args());
        }
        fn flush(&self) {}
    }
    static LOGGER: SimpleLogger = SimpleLogger;
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Info);
}
