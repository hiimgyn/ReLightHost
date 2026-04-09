use tauri::{include_image, Emitter, Manager};

const TRAY_ID: &str = "main";
const TRAY_TOOLTIP_ACTIVE: &str = "ReLightHost";
const TRAY_TOOLTIP_MUTED: &str = "ReLightHost (Muted)";
const TRAY_MENU_MUTE: &str = "Mute Audio";
const TRAY_MENU_UNMUTE: &str = "Unmute Audio";

pub(crate) fn sync_audio_tray_state(
    app: &tauri::AppHandle<tauri::Wry>,
    tray_state: &crate::TrayState,
    muted: bool,
) {
    let menu_text = if muted { TRAY_MENU_UNMUTE } else { TRAY_MENU_MUTE };
    if let Err(error) = tray_state.mute_item.set_text(menu_text) {
        log::warn!("Failed to update tray mute label: {error}");
    }

    let tooltip = if muted {
        TRAY_TOOLTIP_MUTED
    } else {
        TRAY_TOOLTIP_ACTIVE
    };

    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let icon = if muted {
            tray_state.audio_tray_icon_muted.clone()
        } else {
            tray_state.audio_tray_icon.clone()
        };

        if let Err(error) = tray.set_icon(Some(icon)) {
            log::warn!("Failed to update tray icon: {error}");
        }

        if let Err(error) = tray.set_tooltip(Some(tooltip)) {
            log::warn!("Failed to update tray tooltip: {error}");
        }
    } else {
        log::warn!("Tray icon '{TRAY_ID}' was not found while syncing mute state");
    }

    if let Err(error) = app.emit("tray-mute-changed", muted) {
        log::warn!("Failed to emit tray mute state change: {error}");
    }
}

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

    let audio_tray_icon = include_image!("../public/logo.png");
    let audio_tray_icon_muted = include_image!("../public/logo_muted.png");

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
        .tooltip(TRAY_TOOLTIP_ACTIVE)
        .icon(audio_tray_icon.clone())
        .menu(&menu)
        .on_menu_event(
            |app: &tauri::AppHandle<tauri::Wry>, event: tauri::menu::MenuEvent| {
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
                        drop(manager);

                        let tray_state = app.state::<crate::TrayState>();
                        sync_audio_tray_state(app, &tray_state, new_muted);

                        crate::save_audio_session_to_disk(&state);
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
                        crate::commands::system::shutdown_for_exit(app);
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
        audio_tray_icon,
        audio_tray_icon_muted,
    });

    let muted = app.state::<crate::AppState>().audio_manager.read().is_muted();
    let tray_state = app.state::<crate::TrayState>();
    sync_audio_tray_state(app.handle(), &tray_state, muted);

    Ok(())
}
