// src-tauri/src/commands/game_launcher.rs
use tauri::{Manager, State};
use crate::commands::settings::SettingsState;

#[tauri::command]
pub fn launch_game(state: State<SettingsState>) -> Result<bool, String> {
    let settings = state.settings.lock().map_err(|e| e.to_string())?;

    let game_path = settings
        .game_path
        .clone()
        .ok_or_else(|| "Game path is not set".to_string())?;

    let wow_exe = std::path::Path::new(&game_path).join("Wow.exe");

    if !wow_exe.exists() {
        return Err("Wow.exe not found".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .arg("/C")
            .arg("start")
            .arg("")
            .arg(&wow_exe)
            .current_dir(&game_path)
            .spawn()
            .map_err(|e| format!("Failed to launch game: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("wine")
            .arg(&wow_exe)
            .current_dir(&game_path)
            .spawn()
            .map_err(|e| format!("Failed to launch game: {}", e))?;
    }

    #[cfg(target_os = "macos")]
    {
        return Err("macOS is not supported for WoW 3.3.5".to_string());
    }

    Ok(true)
}

#[tauri::command]
pub fn open_logs_folder(app: tauri::AppHandle) -> Result<(), String> {
    let logs_path = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("Failed to get config dir: {}", e))?
        .join("logs");

    std::fs::create_dir_all(&logs_path)
        .map_err(|e| format!("Failed to create logs dir: {}", e))?;

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&logs_path)
            .spawn()
            .map_err(|e| format!("Failed to open logs folder: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&logs_path)
            .spawn()
            .map_err(|e| format!("Failed to open logs folder: {}", e))?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&logs_path)
            .spawn()
            .map_err(|e| format!("Failed to open logs folder: {}", e))?;
    }

    Ok(())
}