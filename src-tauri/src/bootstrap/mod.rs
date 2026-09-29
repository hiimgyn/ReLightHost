pub mod tray;
pub mod window;

pub fn setup(app: &mut tauri::App<tauri::Wry>) -> Result<(), Box<dyn std::error::Error>> {
    crate::app_events::init_app_handle(app.handle().clone());

    // Every build logs to a file (stdout + app log dir, one rotated file):
    // this host runs third-party plugins that crash, and a release user
    // needs something to attach to a bug report.
    app.handle().plugin(
        tauri_plugin_log::Builder::default()
            .clear_format()
            .format(|out, message, record| {
                let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
                let thread = std::thread::current();
                out.finish(format_args!(
                    "[{}][{}][{}][thread={:?} name={}] {}",
                    timestamp,
                    record.level(),
                    record.target(),
                    thread.id(),
                    thread.name().unwrap_or("unnamed"),
                    message
                ));
            })
            .level(if cfg!(debug_assertions) { log::LevelFilter::Debug } else { log::LevelFilter::Info })
            .max_file_size(2 * 1024 * 1024)
            .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepOne)
            .build(),
    )?;

    tray::setup_tray(app)?;
    window::setup_main_window(app)?;
    Ok(())
}
