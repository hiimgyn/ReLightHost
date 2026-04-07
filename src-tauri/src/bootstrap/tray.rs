use tauri::Manager;

pub fn setup_tray(app: &mut tauri::App<tauri::Wry>) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let show_item = MenuItem::with_id(app, "show", "Show ReLightHost", true, None::<&str>)?;
    let mute_item = MenuItem::with_id(app, "toggle_mute", "Mute Audio", true, None::<&str>)?;
    let loopback_item =
        MenuItem::with_id(app, "toggle_loopback", "Enable Hardware Out", true, None::<&str>)?;
    let audio_item =
        MenuItem::with_id(app, "audio_settings", "Audio Settings…", true, None::<&str>)?;
    let app_item =
        MenuItem::with_id(app, "app_settings", "Application Settings…", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Exit", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let sep3 = PredefinedMenuItem::separator(app)?;

    let menu = Menu::with_items(
        app,
        &[
            &show_item,
            &sep1,
            &mute_item,
            &loopback_item,
            &sep2,
            &audio_item,
            &app_item,
            &sep3,
            &quit_item,
        ],
    )?;

    let tray = TrayIconBuilder::with_id("main")
        .tooltip("ReLightHost")
        .icon(
            app.default_window_icon()
                .cloned()
                .unwrap_or_else(|| tauri::image::Image::new(&[] as &[u8], 0, 0)),
        )
        .menu(&menu)
        .on_menu_event(
            |app: &tauri::AppHandle<tauri::Wry>, event: tauri::menu::MenuEvent| {
                use tauri::Emitter;
                match event.id.as_ref() {
                    "show" => {
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.show();
                            let _ = win.unminimize();
                            let _ = win.set_focus();
                        }
                    }
                    "toggle_mute" => {
                        let state = app.state::<crate::AppState>();
                        let manager = state.audio_manager.read();
                        let new_muted = !manager.is_muted();
                        manager.set_muted(new_muted);
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.emit("tray-mute-changed", new_muted);
                        }
                        let tray_state = app.state::<crate::TrayState>();
                        let new_text = if new_muted { "Unmute Audio" } else { "Mute Audio" };
                        let _ = tray_state.mute_item.set_text(new_text);
                        if let Some(tray) = app.tray_by_id("main") {
                            let tooltip = if new_muted {
                                "ReLightHost (Muted)"
                            } else {
                                "ReLightHost"
                            };
                            let _ = tray.set_tooltip(Some(tooltip));
                        }
                    }
                    "toggle_loopback" => {
                        let state = app.state::<crate::AppState>();
                        let manager = state.audio_manager.read();
                        let new_enabled = !manager.is_loopback_enabled();
                        let _ = manager.set_loopback(new_enabled);
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.emit("tray-loopback-changed", new_enabled);
                        }
                        let tray_state = app.state::<crate::TrayState>();
                        let new_text = if new_enabled {
                            "Disable Hardware Out"
                        } else {
                            "Enable Hardware Out"
                        };
                        let _ = tray_state.loopback_item.set_text(new_text);
                    }
                    "audio_settings" => {
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.show();
                            let _ = win.unminimize();
                            let _ = win.set_focus();
                            let _ = win.emit("tray-open-audio-settings", ());
                        }
                    }
                    "app_settings" => {
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.show();
                            let _ = win.unminimize();
                            let _ = win.set_focus();
                            let _ = win.emit("tray-open-app-settings", ());
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                }
            },
        )
        .on_tray_icon_event(|tray: &tauri::tray::TrayIcon, event: TrayIconEvent| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                if let Some(window) = tray.app_handle().get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;
    let _ = tray;

    app.manage(crate::TrayState {
        mute_item,
        loopback_item,
    });

    Ok(())
}
