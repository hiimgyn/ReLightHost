use tauri::{Emitter, Manager};

pub fn setup_main_window(app: &mut tauri::App<tauri::Wry>) -> tauri::Result<()> {
    const RATIO: f64 = 860.0 / 560.0;
    // Matches the hard floor in tauri.conf.json (minWidth/minHeight) — kept
    // in sync here too so this computation's own floor never disagrees with
    // the OS-enforced one.
    // Fits a 1280×720 logical desktop (1080p at 150 %, 1366×768 laptops)
    // with the taskbar showing.
    const MIN_W: f64 = 1024.0;
    const MIN_H: f64 = 640.0;

    if let Some(window) = app.get_webview_window("main") {
        let app_handle = app.handle().clone();
        window.on_window_event(move |event| {
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    let state = app_handle.state::<crate::AppState>();
                    if state.config_manager.get_minimize_to_tray() {
                        api.prevent_close();
                        if let Some(w) = app_handle.get_webview_window("main") {
                            let _ = w.emit("rh:window-visibility", false);
                            let _ = w.hide();
                        }
                    } else {
                        api.prevent_close();
                        crate::commands::system::shutdown_for_exit(&app_handle);
                        app_handle.exit(0);
                    }
                }
                tauri::WindowEvent::Resized(size) => {
                    if let Some(w) = app_handle.get_webview_window("main") {
                        let is_min = size.width == 0 && size.height == 0;
                        if is_min {
                            let _ = w.emit("rh:window-visibility", false);
                        } else if w.is_visible().unwrap_or(false) {
                            let _ = w.emit("rh:window-visibility", true);
                        }
                    }
                }
                _ => {}
            }
        });

        let start_hidden = std::env::args().any(|arg| arg == "--start-hidden");
        if let Ok(Some(monitor)) = window.primary_monitor() {
            let monitor: tauri::Monitor = monitor;
            let scale: f64 = monitor.scale_factor();
            let logical_w = monitor.size().width as f64 / scale;
            let logical_h = monitor.size().height as f64 / scale;

            let from_w = (logical_w * 0.65).round();
            let from_h = (from_w / RATIO).round();
            let (mut win_w, mut win_h): (f64, f64) = if from_h <= logical_h * 0.9 {
                (from_w, from_h)
            } else {
                let h = (logical_h * 0.9).round();
                ((h * RATIO).round(), h)
            };

            // Never larger than the screen (the OS minimum still wins on a
            // screen smaller than MIN_W × MIN_H).
            win_w = win_w.max(MIN_W).min(logical_w);
            win_h = win_h.max(MIN_H).min(logical_h);

            let _ = window.set_size(tauri::LogicalSize::new(win_w, win_h));
            // Re-center explicitly: resizing after creation keeps the
            // window's top-left corner fixed on most platforms, so the
            // `center: true` config alone (which only applies at the
            // original size) would drift off-center once this runs.
            let _ = window.center();
        }

        if start_hidden {
            let _ = window.emit("rh:window-visibility", false);
            let _ = window.hide();
        }
    }

    Ok(())
}
