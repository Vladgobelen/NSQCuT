// src-tauri/src/commands/voice_window.rs
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use url::Url;

/// Данные для создания окна войс-чата
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceWindowConfig {
    pub url: String,
    pub title: String,
    pub width: f64,
    pub height: f64,
}

impl Default for VoiceWindowConfig {
    fn default() -> Self {
        Self {
            url: "https://ns.fiber-gate.ru".to_string(),
            title: "Щебетало".to_string(),
            width: 900.0,
            height: 700.0,
        }
    }
}

/// Открывает или показывает окно войс-чата
#[tauri::command]
pub async fn open_voice_window(
    app: tauri::AppHandle,
    config: Option<VoiceWindowConfig>,
) -> Result<(), String> {
    let cfg = config.unwrap_or_default();

    // Если окно уже существует — просто показываем его
    if let Some(window) = app.get_webview_window("voice_chat") {
        // Синхронизируем позицию с главным окном для эффекта "одного окна"
        if let Some(main) = app.get_webview_window("main") {
            if let (Ok(main_pos), Ok(main_size)) = (main.outer_position(), main.inner_size()) {
                let _ = window.set_position(tauri::PhysicalPosition::new(main_pos.x, main_pos.y));
                let _ =
                    window.set_size(tauri::PhysicalSize::new(main_size.width, main_size.height));
            }
        }
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }

    // 🔥 В Tauri v2 WebviewUrl — это enum
    let webview_url = if cfg.url.starts_with("http://") || cfg.url.starts_with("https://") {
        let parsed = Url::parse(&cfg.url).map_err(|e| format!("Invalid URL: {}", e))?;
        WebviewUrl::External(parsed)
    } else {
        WebviewUrl::App(cfg.url.into())
    };

    // 🔥 URL передаётся ТОЛЬКО в new(), метода .url() больше нет
    let window = WebviewWindowBuilder::new(&app, "voice_chat", webview_url)
        .title(&cfg.title)
        .inner_size(cfg.width, cfg.height)
        .min_inner_size(400.0, 600.0)
        .resizable(true)
        .decorations(true)
        .focused(true)
        .visible(true)
        .build()
        .map_err(|e| format!("Failed to create voice window: {}", e))?;

    // Подписываемся на событие закрытия окна, чтобы уведомить главное окно
    let app_clone = app.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Destroyed = event {
            // Уведомляем главное окно, что войс закрылся
            if let Some(main) = app_clone.get_webview_window("main") {
                let _ = main.emit("voice-window-closed", ());
            }
        }
    });

    // Синхронизируем размер/позицию с главным окном при создании
    if let Some(main) = app.get_webview_window("main") {
        if let (Ok(main_pos), Ok(main_size)) = (main.outer_position(), main.inner_size()) {
            let _ = window.set_position(tauri::PhysicalPosition::new(main_pos.x, main_pos.y));
            let _ = window.set_size(tauri::PhysicalSize::new(main_size.width, main_size.height));
        }
    }

    Ok(())
}

/// Скрывает окно войс-чата (не закрывает, чтобы не терять состояние)
#[tauri::command]
pub fn hide_voice_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("voice_chat") {
        window.hide().map_err(|e| e.to_string())?;
        // Показываем главное окно
        if let Some(main) = app.get_webview_window("main") {
            main.show().map_err(|e| e.to_string())?;
            main.set_focus().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Полностью закрывает окно войс-чата (освобождает ресурсы)
#[tauri::command]
pub fn close_voice_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("voice_chat") {
        window.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Переключает состояние микрофона (двусторонняя связь)
#[tauri::command]
pub fn toggle_mic_in_voice(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("voice_chat") {
        window
            .emit("toggle-mic-command", ())
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Отправляет произвольное событие в окно войс-чата
#[tauri::command]
pub fn send_to_voice_window(
    app: tauri::AppHandle,
    event: String,
    payload: serde_json::Value,
) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("voice_chat") {
        window.emit(&event, payload).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Проверяет, открыто ли окно войс-чата
#[tauri::command]
pub fn is_voice_window_open(app: tauri::AppHandle) -> Result<bool, String> {
    Ok(app.get_webview_window("voice_chat").is_some())
}