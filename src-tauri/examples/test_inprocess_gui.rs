//! Same manual check as test_sandbox_gui.rs but through the plain
//! in-process Vst3Processor — used to tell whether a blank editor is a
//! sandbox regression or a pre-existing issue. Throwaway, not built by
//! default. Run with: cargo run --example test_inprocess_gui

use app_lib::plugins::processor::vst3::Vst3Processor;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

const PLUGIN_PATH: &str = r"C:\Program Files\Common Files\VST3\Supertone\Clear.vst3";

fn main() {
    println!("Loading '{}' in-process...", PLUGIN_PATH);
    let mut proc = Vst3Processor::load(PLUGIN_PATH, 48000.0, 512).expect("load failed");
    println!("Loaded OK.");

    println!("Processing 20 silent blocks (warmup)...");
    for _ in 0..20 {
        let mut left = vec![0.0f32; 512];
        let mut right = vec![0.0f32; 512];
        proc.process_stereo(&mut left, &mut right);
        std::thread::sleep(Duration::from_millis(20));
    }

    println!("Opening GUI (same params as the sandboxed host: sync_component_state=false, restored_state_blob=None)...");
    let gui_flag: Arc<AtomicBool> = Arc::new(AtomicBool::new(true));
    let gui_hwnd: Arc<AtomicIsize> = Arc::new(AtomicIsize::new(0));
    proc.open_gui("In-process VST3", gui_flag.clone(), gui_hwnd.clone(), false, None)
        .expect("open_gui failed");

    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let hwnd = gui_hwnd.load(Ordering::Acquire);
        if hwnd != 0 {
            println!("GUI HWND: {hwnd:#x}");
            break;
        }
        if std::time::Instant::now() > deadline { println!("Timed out waiting for HWND"); break; }
        std::thread::sleep(Duration::from_millis(50));
    }

    println!("Keeping GUI open for 10s so it can be screenshotted...");
    for i in 0..100 {
        let mut left = vec![0.0f32; 512];
        let mut right = vec![0.0f32; 512];
        proc.process_stereo(&mut left, &mut right);
        if i % 25 == 0 {
            println!("  still running, gui_flag={}", gui_flag.load(Ordering::Acquire));
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    println!("Closing via WM_CLOSE to stored HWND (same-process this time)...");
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
            println!("gui_flag cleared after {:?}", close_wait_start.elapsed());
            break;
        }
        if std::time::Instant::now() > close_deadline {
            println!("gui_flag NEVER cleared within 15s");
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}
