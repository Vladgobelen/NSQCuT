// src-tauri/src/commands/settings.rs
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};

/// Настройки приложения
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct AppSettings {
    pub game_path: Option<String>,
    pub ptt_hotkey: Option<String>,
}

/// Состояние настроек (хранится в Tauri state)
pub struct SettingsState {
    pub settings: Mutex<AppSettings>,
    pub config_path: PathBuf,
}

impl SettingsState {
    /// Создаёт новое состояние с путём к конфигу
    pub fn new(config_path: PathBuf) -> Self {
        Self {
            settings: Mutex::new(AppSettings::default()),
            config_path,
        }
    }

    /// Загружает настройки из файла или создаёт дефолтные
    pub fn load_or_create(&self) -> Result<AppSettings, String> {
        if self.config_path.exists() {
            let content = std::fs::read_to_string(&self.config_path)
                .map_err(|e| format!("Failed to read settings: {}", e))?;
            serde_json::from_str(&content)
                .map_err(|e| format!("Failed to parse settings: {}", e))
        } else {
            // Создаём дефолтные настройки
            let default = AppSettings::default();
            self.save(&default)?;
            Ok(default)
        }
    }

    /// Сохраняет настройки на диск
    pub fn save(&self, settings: &AppSettings) -> Result<(), String> {
        // Создаём директорию если не существует
        if let Some(parent) = self.config_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config dir: {}", e))?;
        }
        
        let json = serde_json::to_string_pretty(settings)
            .map_err(|e| format!("Failed to serialize settings: {}", e))?;
        std::fs::write(&self.config_path, json)
            .map_err(|e| format!("Failed to write settings: {}", e))?;
        Ok(())
    }

    /// Обновляет настройки в памяти и на диске
    pub fn update(&self, updater: impl FnOnce(&mut AppSettings)) -> Result<(), String> {
        let mut settings = self.settings.lock()
            .map_err(|e| format!("Failed to lock settings: {}", e))?;
        updater(&mut settings);
        drop(settings); // Освобождаем лок перед сохранением
        self.save(&self.settings.lock()
            .map_err(|e| format!("Failed to lock settings: {}", e))?.clone())
    }
}

/// Инициализация настроек при старте приложения
#[tauri::command]
pub fn init_settings(app: AppHandle) -> Result<(), String> {
    let config_dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("Failed to get config dir: {}", e))?;

    let config_path = config_dir.join("settings.json");
    let state = SettingsState::new(config_path);
    
    // Загружаем или создаём настройки
    let settings = state.load_or_create()?;
    
    // Обновляем состояние в памяти
    *state.settings.lock()
        .map_err(|e| format!("Failed to lock settings: {}", e))? = settings;
    
    // Регистрируем state в Tauri
    app.manage(state);
    
    Ok(())
}

/// Проверка: существует ли Wow.exe по сохранённому пути
#[tauri::command]
pub fn check_game(state: State<SettingsState>) -> Result<bool, String> {
    let settings = state.settings.lock()
        .map_err(|e| format!("Failed to lock settings: {}", e))?;
    
    if let Some(ref path) = settings.game_path {
        let wow_exe = std::path::Path::new(path).join("Wow.exe");
        Ok(wow_exe.exists())
    } else {
        Ok(false)
    }
}

/// Изменение пути к игре
#[tauri::command]
pub fn change_game_path(
    state: State<SettingsState>,
    new_path: String,
) -> Result<bool, String> {
    // Проверяем что файл существует
    let wow_exe = std::path::Path::new(&new_path).join("Wow.exe");
    if !wow_exe.exists() {
        return Err(format!("Wow.exe not found in: {}", new_path));
    }
    
    // Обновляем настройки
    state.update(|settings| {
        settings.game_path = Some(new_path);
    })?;
    
    Ok(true)
}

/// Получение текущего пути к игре
#[tauri::command]
pub fn get_game_path(state: State<SettingsState>) -> Result<Option<String>, String> {
    let settings = state.settings.lock()
        .map_err(|e| format!("Failed to lock settings: {}", e))?;
    Ok(settings.game_path.clone())
}

/// Установка хоткея для PTT (Push-to-Talk)
#[tauri::command]
pub fn set_ptt_hotkey(
    state: State<SettingsState>,
    hotkey: Option<String>,
) -> Result<(), String> {
    state.update(|settings| {
        settings.ptt_hotkey = hotkey;
    })?;
    Ok(())
}

/// Получение текущего хоткея PTT
#[tauri::command]
pub fn get_ptt_hotkey(state: State<SettingsState>) -> Result<Option<String>, String> {
    let settings = state.settings.lock()
        .map_err(|e| format!("Failed to lock settings: {}", e))?;
    Ok(settings.ptt_hotkey.clone())
}