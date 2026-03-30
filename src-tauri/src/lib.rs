// src-tauri/src/lib.rs
mod commands;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(commands::addon_manager::AddonManagerState::new())
        .invoke_handler(tauri::generate_handler![
            commands::settings::init_settings,
            commands::settings::check_game,
            commands::settings::change_game_path,
            commands::settings::get_game_path,
            commands::settings::set_ptt_hotkey,
            commands::settings::get_ptt_hotkey,
            commands::addon_manager::set_game_path,
            commands::addon_manager::load_addons,
            commands::addon_manager::toggle_addon,
            commands::game_launcher::launch_game,
            commands::game_launcher::open_logs_folder,
        ])
        .setup(|app| {
            commands::settings::init_settings(app.app_handle().clone())?;
            let settings_state = app.state::<commands::settings::SettingsState>();
            let game_path = {
                let settings = settings_state.settings.lock().map_err(|e| e.to_string())?;
                settings.game_path.clone()
            };
            let addon_state = app.state::<commands::addon_manager::AddonManagerState>();
            addon_state.sync_game_path(game_path)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}