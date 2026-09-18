//! Sandboxed VST3 host process — spawned by `SandboxedVst3Processor`
//! (`app_lib::plugins::processor::vst3_sandbox`) for plugins that have
//! repeatedly crashed in-process. Speaks the protocol in
//! `vst3_sandbox::protocol` over its inherited stdin/stdout.
//!
//! Deliberately thin: all the actual VST3 hosting logic (load, process,
//! state, GUI) is `app_lib::plugins::processor::vst3::Vst3Processor` —
//! exactly the same code the in-process path uses, so every workaround
//! already tuned for fragile plugins (COM init, warmup windows, DPI
//! handling) applies here unchanged. This binary only speaks the wire
//! protocol and forwards to it.

#[cfg(target_os = "windows")]
fn main() {
    win::run();
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("vst3_sandbox_host is Windows-only");
    std::process::exit(1);
}

#[cfg(target_os = "windows")]
mod win {
    use app_lib::plugins::processor::vst3::Vst3Processor;
    use app_lib::plugins::processor::vst3_sandbox::protocol::{
        self, ControlRequest, ControlResponse,
    };
    use std::io::{stdin, stdout, Stdout};
    use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
    use std::sync::{Arc, Mutex};

    pub fn run() {
        // Match the main app's DPI awareness so plugin window sizing
        // (GetDpiForWindow etc., used by gui::vst3::win) behaves the same
        // as it does in-process.
        unsafe {
            use windows_sys::Win32::UI::HiDpi::{
                SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
            };
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }

        let stdout: Arc<Mutex<Stdout>> = Arc::new(Mutex::new(stdout()));
        let mut plugin: Option<Vst3Processor> = None;
        let mut stdin_handle = stdin();

        loop {
            let (tag, payload) = match protocol::read_frame(&mut stdin_handle) {
                Ok(f) => f,
                Err(_) => break, // host closed the pipe — exit quietly
            };

            match tag {
                protocol::TAG_CONTROL => {
                    let Ok(req) = protocol::decode_control_request(&payload) else { continue };
                    if let ControlRequest::Shutdown = req {
                        break;
                    }
                    handle_control(req, &mut plugin, &stdout);
                }
                protocol::TAG_PROCESS_REQUEST => {
                    let Some((mut left, mut right)) = protocol::decode_process_block(&payload) else {
                        continue;
                    };
                    if let Some(ref mut p) = plugin {
                        p.process_stereo(&mut left, &mut right);
                    }
                    let resp = protocol::encode_process_block(&left, &right);
                    send(&stdout, protocol::TAG_PROCESS_RESPONSE, &resp);
                }
                _ => {}
            }
        }

        // Let Vst3Processor::drop run (deactivate/close the plugin) before
        // the process exits, rather than relying on the OS to tear down
        // resources — mirrors how the in-process host shuts a plugin down.
        drop(plugin);
    }

    fn send(stdout: &Arc<Mutex<Stdout>>, tag: u8, payload: &[u8]) {
        if let Ok(mut out) = stdout.lock() {
            let _ = protocol::write_frame(&mut *out, tag, payload);
        }
    }

    fn send_control(stdout: &Arc<Mutex<Stdout>>, resp: &ControlResponse) {
        if let Ok(mut out) = stdout.lock() {
            let _ = protocol::write_control_response(&mut *out, resp);
        }
    }

    fn handle_control(req: ControlRequest, plugin: &mut Option<Vst3Processor>, stdout: &Arc<Mutex<Stdout>>) {
        match req {
            ControlRequest::Load { plugin_path, sample_rate, block_size } => {
                match Vst3Processor::load(&plugin_path, sample_rate, block_size) {
                    Ok(p) => {
                        *plugin = Some(p);
                        send_control(stdout, &ControlResponse::Loaded { name: plugin_path });
                    }
                    Err(e) => {
                        send_control(stdout, &ControlResponse::LoadFailed { error: e.to_string() });
                    }
                }
            }
            ControlRequest::GetState => {
                let data = plugin.as_ref().map(|p| p.get_state()).unwrap_or_default();
                send_control(stdout, &ControlResponse::State { data });
            }
            ControlRequest::SetState { data } => {
                if let Some(p) = plugin.as_ref() {
                    p.set_state(&data);
                }
                send_control(stdout, &ControlResponse::Ack);
            }
            ControlRequest::SetParameter { param_id, normalized } => {
                if let Some(p) = plugin.as_ref() {
                    p.set_param_normalized(param_id, normalized);
                }
            }
            ControlRequest::OpenGui => {
                let Some(p) = plugin.as_ref() else {
                    send_control(stdout, &ControlResponse::GuiOpenFailed {
                        error: "no plugin loaded".into(),
                    });
                    return;
                };

                let gui_flag = Arc::new(AtomicBool::new(true));
                let gui_hwnd = Arc::new(AtomicIsize::new(0));

                // Reuse the exact in-process GUI path unmodified — it
                // spawns its own thread and returns immediately once the
                // window/attach sequence is under way.
                if let Err(e) = p.open_gui("Sandboxed VST3", gui_flag.clone(), gui_hwnd.clone(), false, None) {
                    send_control(stdout, &ControlResponse::GuiOpenFailed { error: e.to_string() });
                    return;
                }

                // Report the HWND back once the GUI thread has created its
                // window, then watch for the flag clearing (window closed)
                // and report that too. Both happen on a helper thread so
                // this control-message handler returns promptly.
                let stdout_watch = Arc::clone(stdout);
                std::thread::spawn(move || {
                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                    let mut hwnd = 0isize;
                    while std::time::Instant::now() < deadline {
                        hwnd = gui_hwnd.load(Ordering::Acquire);
                        if hwnd != 0 { break; }
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                    send_control(&stdout_watch, &ControlResponse::GuiHwnd { hwnd });

                    while gui_flag.load(Ordering::Acquire) {
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                    send_control(&stdout_watch, &ControlResponse::GuiClosed);
                });
            }
            ControlRequest::Shutdown => unreachable!("handled by the caller before dispatch"),
        }
    }
}
