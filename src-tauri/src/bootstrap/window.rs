use tauri::Manager;

pub fn setup_main_window(app: &mut tauri::App<tauri::Wry>) -> tauri::Result<()> {
    const RATIO: f64 = 860.0 / 560.0;
    const MIN_W: f64 = 800.0;
    const MIN_H: f64 = 520.0;

    if let Some(window) = app.get_webview_window("main") {
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

            win_w = win_w.max(MIN_W);
            win_h = win_h.max(MIN_H);

            let _ = window.set_size(tauri::LogicalSize::new(win_w, win_h));
        }

        if start_hidden {
            let _ = window.hide();
        }
    }

    Ok(())
}
