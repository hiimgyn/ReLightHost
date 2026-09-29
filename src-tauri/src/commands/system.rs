use crate::AppState;
use tauri::Manager;

pub(crate) fn shutdown_for_exit(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();

    crate::core::autosave::flush_on_exit(
        &state.plugin_manager,
        &state.preset_manager,
        &state.autosave.last_hash,
    );

    if let Err(e) = state.audio_manager.read().stop() {
        log::warn!("Failed to stop audio during shutdown: {e}");
    }

    state.plugin_manager.read().clear();
}

#[derive(serde::Serialize)]
pub struct SystemStats {
    cpu_percent: f32,
    ram_percent: f32,
    ram_used_mb: u64,
    ram_total_mb: u64,
}

#[derive(serde::Serialize)]
pub struct UpdateInfo {
    available: bool,
    version: Option<String>,
    notes: Option<String>,
}

#[tauri::command(async)]
pub fn get_system_stats(state: tauri::State<'_, AppState>) -> Result<SystemStats, String> {
    use sysinfo::{Pid, ProcessRefreshKind};
    let pid = Pid::from_u32(std::process::id());
    let mut sys = state.sys_info.write();
    sys.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing().with_cpu().with_memory(),
    );
    let num_cpus = sys.cpus().len().max(1) as f32;
    if let Some(proc) = sys.process(pid) {
        let total_mem = sys.total_memory();
        let proc_mem = proc.memory();
        let cpu_pct = (proc.cpu_usage() / num_cpus).min(100.0);
        let ram_pct = if total_mem > 0 {
            (proc_mem as f32 / total_mem as f32) * 100.0
        } else {
            0.0
        };
        Ok(SystemStats {
            cpu_percent: cpu_pct,
            ram_percent: ram_pct,
            ram_used_mb: proc_mem / 1024 / 1024,
            ram_total_mb: total_mem / 1024 / 1024,
        })
    } else {
        Ok(SystemStats {
            cpu_percent: 0.0,
            ram_percent: 0.0,
            ram_used_mb: 0,
            ram_total_mb: 0,
        })
    }
}

/// Only web links — the OS shell would happily "open" a local path or
/// file:// URL (i.e. run it) if one ever reached this command.
fn is_allowed_external_url(url: &str) -> bool {
    url.starts_with("https://")
}

#[tauri::command]
pub fn open_external_url(url: String) -> Result<(), String> {
    if !is_allowed_external_url(&url) {
        return Err(format!("Refusing to open non-https URL: {url}"));
    }
    webbrowser::open(&url).map_err(|e| format!("Failed to open external URL: {e}"))?;
    Ok(())
}

#[tauri::command]
pub async fn check_for_update(app: tauri::AppHandle) -> Result<UpdateInfo, String> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app.updater().map_err(|e| e.to_string())?;
    match updater.check().await {
        Ok(Some(update)) => Ok(UpdateInfo {
            available: true,
            version: Some(update.version.clone()),
            notes: update.body.clone(),
        }),
        Ok(None) => Ok(UpdateInfo {
            available: false,
            version: None,
            notes: None,
        }),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub async fn install_update(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app.updater().map_err(|e| e.to_string())?;
    if let Some(update) = updater.check().await.map_err(|e| e.to_string())? {
        let app_handle = app.clone();
        update
            .download_and_install(|_chunk, _total| {}, move || {
                shutdown_for_exit(&app_handle);
            })
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn quit_app(app: tauri::AppHandle) {
    shutdown_for_exit(&app);
    app.exit(0);
}

#[cfg(test)]
mod tests {
    use super::is_allowed_external_url;

    #[test]
    fn only_https_urls_may_be_opened() {
        assert!(is_allowed_external_url("https://github.com/HiimGyn/ReLightHost"));
        assert!(!is_allowed_external_url("http://example.com"));
        assert!(!is_allowed_external_url("file:///C:/Windows/System32/calc.exe"));
        assert!(!is_allowed_external_url(r"C:\Windows\System32\calc.exe"));
    }
}
