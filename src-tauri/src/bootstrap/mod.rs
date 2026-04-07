pub mod tray;
pub mod window;

pub fn setup(app: &mut tauri::App<tauri::Wry>) -> Result<(), Box<dyn std::error::Error>> {
    crate::app_events::init_app_handle(app.handle().clone());

    if cfg!(debug_assertions) {
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
                .level(log::LevelFilter::Debug)
                .build(),
        )?;
    }

    tray::setup_tray(app)?;
    window::setup_main_window(app)?;
    Ok(())
}
