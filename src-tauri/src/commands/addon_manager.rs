// src-tauri/src/commands/addon_manager.rs
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AddonInfo {
    pub name: String,
    pub description: String,
    pub installed: bool,
    pub needs_update: bool,
    pub being_processed: bool,
    pub updating: bool,
    pub link: String,
    pub target_path: String,
    pub is_zip: bool,
}

pub struct AddonManagerState {
    pub addons: Mutex<IndexMap<String, AddonInfo>>,
    pub game_path: Mutex<Option<String>>,
}

impl AddonManagerState {
    pub fn new() -> Self {
        Self {
            addons: Mutex::new(IndexMap::new()),
            game_path: Mutex::new(None),
        }
    }

    pub fn set_game_path(&self, path: String) -> Result<(), String> {
        let mut gp = self.game_path.lock().map_err(|e| e.to_string())?;
        *gp = Some(path);
        Ok(())
    }

    pub fn get_game_path(&self) -> Result<Option<String>, String> {
        let gp = self.game_path.lock().map_err(|e| e.to_string())?;
        Ok(gp.clone())
    }

    pub fn sync_game_path(&self, path: Option<String>) -> Result<(), String> {
        let mut gp = self.game_path.lock().map_err(|e| e.to_string())?;
        *gp = path;
        Ok(())
    }
}

#[tauri::command]
pub fn set_game_path(state: State<'_, AddonManagerState>, path: String) -> Result<(), String> {
    eprintln!("[DEBUG] set_game_path called: {}", path);
    state.set_game_path(path)
}

#[tauri::command]
pub async fn load_addons(
    _app: AppHandle,
    state: State<'_, AddonManagerState>,
) -> Result<IndexMap<String, AddonInfo>, String> {
    eprintln!("[DEBUG] load_addons called");
    let config_url = "https://raw.githubusercontent.com/Vladgobelen/NSQCu/main/addons.json";

    let client = reqwest::Client::builder()
        .user_agent("NightWatchUpdater/1.0")
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    eprintln!("[DEBUG] Fetching config from: {}", config_url);

    let response = client
        .get(config_url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch addons config: {}", e))?;

    eprintln!("[DEBUG] Response status: {}", response.status());

    let config: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse addons config JSON: {}", e))?;

    let addons_config = config
        .get("addons")
        .ok_or("No 'addons' field in config")?
        .as_object()
        .ok_or("'addons' is not an object")?;

    eprintln!("[DEBUG] Found {} addons in config", addons_config.len());

    let game_path = state.get_game_path()?.unwrap_or_default();
    let mut addons = IndexMap::new();

    for (name, config_data) in addons_config {
        let link = config_data.get("link").and_then(|v| v.as_str()).unwrap_or("");
        let description = config_data
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let target_path = config_data
            .get("target_path")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        
        // 🔧 Проверка расширения: если не указан is_zip, определяем по расширению ссылки
        let is_zip = if let Some(val) = config_data.get("is_zip").and_then(|v| v.as_bool()) {
            val
        } else {
            !link.to_lowercase().ends_with(".mpq")
        };

        let installed = check_addon_installed(&game_path, name, target_path);

        eprintln!("[DEBUG] Addon {}: installed={}, link={}", name, installed, link);

        addons.insert(
            name.clone(),
            AddonInfo {
                name: name.clone(),
                description: description.to_string(),
                installed,
                needs_update: false,
                being_processed: false,
                updating: false,
                link: link.to_string(),
                target_path: target_path.to_string(),
                is_zip,
            },
        );
    }

    {
        let mut state_addons = state.addons.lock().map_err(|e| e.to_string())?;
        *state_addons = addons.clone();
    }

    eprintln!("[DEBUG] load_addons returning {} addons", addons.len());
    Ok(addons)
}

fn check_addon_installed(game_path: &str, name: &str, target_path: &str) -> bool {
    if game_path.is_empty() {
        eprintln!("[DEBUG] check_addon_installed: game_path is empty");
        return false;
    }
    let full_path = PathBuf::from(game_path).join(target_path);
    eprintln!("[DEBUG] Checking installed: {} at {}", name, full_path.display());

    if name == "NSQC" {
        let vers_path = full_path.join("NSQC").join("vers");
        let exists = vers_path.exists();
        eprintln!("[DEBUG] NSQC vers check: {} -> {}", vers_path.display(), exists);
        exists
    } else {
        if !full_path.exists() {
            eprintln!("[DEBUG] Target dir not exists: {}", full_path.display());
            return false;
        }
        if let Ok(entries) = std::fs::read_dir(&full_path) {
            let found = entries.filter_map(|e| e.ok()).any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .to_lowercase()
                    .contains(&name.to_lowercase())
            });
            eprintln!("[DEBUG] {} found in dir: {}", name, found);
            found
        } else {
            false
        }
    }
}

#[tauri::command]
pub async fn toggle_addon(
    app: AppHandle,
    state: State<'_, AddonManagerState>,
    name: String,
    install: bool,
) -> Result<bool, String> {
    eprintln!("[DEBUG] toggle_addon: name={}, install={}", name, install);
    
    // 🔧 Уведомляем фронтенд о начале операции (для блокировки кнопки запуска)
    let _ = app.emit("addon-install-started", serde_json::json!({ "name": &name, "install": install }));

    let (addon, game_path) = {
        let addons = state.addons.lock().map_err(|e| e.to_string())?;
        let addon = addons.get(&name).cloned().ok_or("Addon not found")?;
        let gp = state.game_path.lock().map_err(|e| e.to_string())?;
        (addon, gp.clone())
    };

    {
        let mut addons = state.addons.lock().map_err(|e| e.to_string())?;
        if let Some(a) = addons.get_mut(&name) {
            a.being_processed = true;
            a.updating = true;
        }
    }

    let _ = app.emit(
        "progress",
        serde_json::json!({ "name": &name, "progress": 0.1 }),
    );

    let result = if install {
        install_addon(&app, &addon, game_path.as_ref()).await
    } else {
        uninstall_addon(&app, &addon, game_path.as_ref())
    };

    {
        let mut addons = state.addons.lock().map_err(|e| e.to_string())?;
        if let Some(a) = addons.get_mut(&name) {
            a.being_processed = false;
            a.updating = false;
        }
    }

    // 🔧 Если устанавливали NSQC, автоматически переустанавливаем и NSQC3
    if install && result.is_ok() && name == "NSQC" {
        eprintln!("[DEBUG] NSQC installed, triggering NSQC3 auto-reinstall");
        let _ = auto_update_nsqc3(&app, &state).await;
    }

    // 🔧 Уведомляем фронтенд об окончании с задержкой 3 секунды (для блокировки кнопки запуска)
    let app_clone = app.clone();
    let name_clone = name.clone();
    let success_clone = result.is_ok();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
        let _ = app_clone.emit("addon-install-finished", serde_json::json!({ "name": name_clone, "success": success_clone }));
    });

    match result {
        Ok(_) => {
            let _ = app.emit(
                "operation-finished",
                serde_json::json!({ "name": &name, "success": true }),
            );
            Ok(true)
        }
        Err(e) => {
            eprintln!("[DEBUG] toggle_addon error: {}", e);
            let _ = app.emit(
                "operation-error",
                serde_json::json!({ "message": e.clone() }),
            );
            Err(e)
        }
    }
}

// 🔧 Функция для авто-переустановки NSQC3 при установке/обновлении NSQC
async fn auto_update_nsqc3(
    app: &AppHandle,
    state: &State<'_, AddonManagerState>,
) -> Result<(), String> {
    eprintln!("[DEBUG] auto_update_nsqc3: checking NSQC3");
    
    let (nsqc3_addon, game_path) = {
        let addons = state.addons.lock().map_err(|e| e.to_string())?;
        let addon = addons.get("NSQC3").cloned();
        let gp = state.game_path.lock().map_err(|e| e.to_string())?;
        (addon, gp.clone())
    };

    if let Some(addon) = nsqc3_addon {
        let target_dir = PathBuf::from(game_path.clone().unwrap_or_default()).join(&addon.target_path);
        
        // 🔧 Шаг 1: Если NSQC3 установлен — сначала удаляем его
        if check_addon_installed(&game_path.clone().unwrap_or_default(), "NSQC3", &addon.target_path) {
            eprintln!("[DEBUG] NSQC3 is installed, uninstalling first...");
            let _ = uninstall_addon(app, &addon, game_path.as_ref());
        }
        
        // 🔧 Шаг 2: Устанавливаем свежий NSQC3
        eprintln!("[DEBUG] Installing fresh NSQC3...");
        let _ = install_addon(app, &addon, game_path.as_ref()).await;
    } else {
        eprintln!("[DEBUG] NSQC3 not found in addons list");
    }
    
    Ok(())
}

async fn install_addon(
    app: &AppHandle,
    addon: &AddonInfo,
    game_path: Option<&String>,
) -> Result<(), String> {
    eprintln!("[DEBUG] install_addon: {}", addon.name);
    let game_path = game_path.ok_or("Game path not set")?;

    let _ = app.emit(
        "progress",
        serde_json::json!({ "name": addon.name, "progress": 0.15 }),
    );

    let client = reqwest::Client::builder()
        .user_agent("NightWatchUpdater/1.0")
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    eprintln!("[DEBUG] Downloading: {}", addon.link);

    let response = client
        .get(&addon.link)
        .send()
        .await
        .map_err(|e| format!("Failed to download addon: {}", e))?;

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    eprintln!("[DEBUG] Downloaded {} bytes", bytes.len());

    let _ = app.emit(
        "progress",
        serde_json::json!({ "name": addon.name, "progress": 0.75 }),
    );

    let target_dir = PathBuf::from(game_path).join(&addon.target_path);
    std::fs::create_dir_all(&target_dir)
        .map_err(|e| format!("Failed to create target dir: {}", e))?;

    // 🔧 Проверка расширения файла для определения типа архива
    let is_zip = if addon.link.to_lowercase().ends_with(".mpq") {
        false
    } else {
        addon.is_zip
    };

    if is_zip {
        eprintln!("[DEBUG] Extracting ZIP to: {}", target_dir.display());
        let cursor = std::io::Cursor::new(&bytes);
        let mut archive =
            zip::ZipArchive::new(cursor).map_err(|e| format!("Failed to open zip: {}", e))?;

        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|e| format!("Failed to read zip entry: {}", e))?;
            let outpath = target_dir.join(file.mangled_name());

            if file.name().ends_with('/') {
                std::fs::create_dir_all(&outpath)
                    .map_err(|e| format!("Failed to create dir: {}", e))?;
            } else {
                if let Some(p) = outpath.parent() {
                    std::fs::create_dir_all(p)
                        .map_err(|e| format!("Failed to create parent dir: {}", e))?;
                }
                let mut outfile =
                    std::fs::File::create(&outpath)
                        .map_err(|e| format!("Failed to create file: {}", e))?;
                std::io::copy(&mut file, &mut outfile)
                    .map_err(|e| format!("Failed to extract file: {}", e))?;
            }
        }
    } else {
        eprintln!("[DEBUG] Saving MPQ file");
        let mpq_path = target_dir.join(
            PathBuf::from(&addon.link)
                .file_name()
                .ok_or("Invalid filename")?,
        );
        let mut outfile = std::fs::File::create(&mpq_path)
            .map_err(|e| format!("Failed to create mpq file: {}", e))?;
        outfile
            .write_all(&bytes)
            .map_err(|e| format!("Failed to write mpq: {}", e))?;
    }

    let _ = app.emit(
        "progress",
        serde_json::json!({ "name": addon.name, "progress": 1.0 }),
    );
    eprintln!("[DEBUG] install_addon completed: {}", addon.name);
    Ok(())
}

fn uninstall_addon(
    app: &AppHandle,
    addon: &AddonInfo,
    game_path: Option<&String>,
) -> Result<(), String> {
    eprintln!("[DEBUG] uninstall_addon: {}", addon.name);
    let game_path = game_path.ok_or("Game path not set")?;
    let target_dir = PathBuf::from(game_path).join(&addon.target_path);

    if !target_dir.exists() {
        eprintln!("[DEBUG] Target dir not exists, nothing to uninstall");
        return Ok(());
    }

    let entries = std::fs::read_dir(&target_dir)
        .map_err(|e| format!("Failed to read dir: {}", e))?;

    let items_to_remove: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .to_lowercase()
                .contains(&addon.name.to_lowercase())
        })
        .collect();

    eprintln!("[DEBUG] Items to remove: {}", items_to_remove.len());

    for (i, item) in items_to_remove.iter().enumerate() {
        let progress = 0.1 + 0.8 * ((i + 1) as f64 / items_to_remove.len() as f64);
        let _ = app.emit(
            "progress",
            serde_json::json!({ "name": addon.name, "progress": progress }),
        );

        let path = item.path();
        if path.is_dir() {
            std::fs::remove_dir_all(&path)
                .map_err(|e| format!("Failed to remove dir: {}", e))?;
        } else {
            std::fs::remove_file(&path)
                .map_err(|e| format!("Failed to remove file: {}", e))?;
        }
    }

    let _ = app.emit(
        "progress",
        serde_json::json!({ "name": addon.name, "progress": 1.0 }),
    );
    eprintln!("[DEBUG] uninstall_addon completed: {}", addon.name);
    Ok(())
}