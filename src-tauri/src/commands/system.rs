use crate::AppState;

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

#[tauri::command]
pub fn get_system_stats(state: tauri::State<AppState>) -> Result<SystemStats, String> {
    use sysinfo::{Pid, ProcessRefreshKind};
    let pid = Pid::from_u32(std::process::id());
    let mut sys = state.sys_info.write();
    sys.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::new().with_cpu().with_memory(),
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
        update
            .download_and_install(|_chunk, _total| {}, || {})
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}
