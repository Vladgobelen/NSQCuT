// src-tauri/src/lib.rs
mod commands;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Плагины
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        // Состояния (State)
        .manage(commands::addon_manager::AddonManagerState::new())
        // Команды (IPC handlers)
        .invoke_handler(tauri::generate_handler![
            // Settings
            commands::settings::init_settings,
            commands::settings::check_game,
            commands::settings::change_game_path,
            commands::settings::get_game_path,
            commands::settings::set_ptt_hotkey,
            commands::settings::get_ptt_hotkey,
            // Addon Manager
            commands::addon_manager::set_game_path,
            commands::addon_manager::load_addons,
            commands::addon_manager::toggle_addon,
            // Game Launcher
            commands::game_launcher::launch_game,
            commands::game_launcher::open_logs_folder,
        ])
        // Инициализация при старте
        .setup(|app| {
            // 1. Инициализируем настройки
            commands::settings::init_settings(app.app_handle().clone())?;

            // 2. Синхронизируем game_path между SettingsState и AddonManagerState
            let settings_state = app.state::<commands::settings::SettingsState>();
            let game_path = {
                let settings = settings_state.settings.lock().map_err(|e| e.to_string())?;
                settings.game_path.clone()
            };
            let addon_state = app.state::<commands::addon_manager::AddonManagerState>();
            addon_state.sync_game_path(game_path)?;

            // 🔥 3. ЗАПУСК ПРОВЕРКИ ОБНОВЛЕНИЙ ПРИ СТАРТЕ
            // ✅ Используем app_handle.state() вместо app.state() для 'static lifetime
            let app_handle = app.app_handle().clone();
            
            tauri::async_runtime::spawn(async move {
                // ✅ Получаем state через app_handle, а не через замыкание
                let state = app_handle.state::<commands::addon_manager::AddonManagerState>();
                let _ = commands::addon_manager::startup_update_check(&app_handle, &state).await;
                // После старта — запускаем регулярный чекер
                commands::addon_manager::start_update_checker(app_handle);
            });

            Ok(())
        })
        // Запуск приложения
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}