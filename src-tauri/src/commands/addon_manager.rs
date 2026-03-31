// src-tauri/src/commands/addon_manager.rs
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::time::{interval, Duration};

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
    } else if name == "NSQC3" {
        let nsqc3_path = full_path.join("NSQC3");
        let exists = nsqc3_path.exists();
        eprintln!("[DEBUG] NSQC3 dir check: {} -> {}", nsqc3_path.display(), exists);
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

    let _ = app.emit(
        "addon-install-started",
        serde_json::json!({ "name": &name, "install": install }),
    );

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

    if install && result.is_ok() && name == "NSQC" {
        eprintln!("[DEBUG] NSQC installed, triggering NSQC3 auto-reinstall");
        let _ = auto_update_nsqc3(&app, &state).await;
    }

    let app_clone = app.clone();
    let name_clone = name.clone();
    let success_clone = result.is_ok();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
        let _ = app_clone.emit(
            "addon-install-finished",
            serde_json::json!({ "name": name_clone, "success": success_clone }),
        );
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
        if check_addon_installed(&game_path.clone().unwrap_or_default(), "NSQC3", &addon.target_path) {
            eprintln!("[DEBUG] NSQC3 is installed, uninstalling first...");
            let _ = uninstall_addon(app, &addon, game_path.as_ref());
        }
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

        let temp_extract_dir = target_dir.join("_temp_extract");
        std::fs::create_dir_all(&temp_extract_dir)
            .map_err(|e| format!("Failed to create temp dir: {}", e))?;

        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|e| format!("Failed to read zip entry: {}", e))?;
            let outpath = temp_extract_dir.join(file.mangled_name());

            if file.name().ends_with('/') {
                std::fs::create_dir_all(&outpath)
                    .map_err(|e| format!("Failed to create dir: {}", e))?;
            } else {
                if let Some(p) = outpath.parent() {
                    std::fs::create_dir_all(p)
                        .map_err(|e| format!("Failed to create parent dir: {}", e))?;
                }
                let mut outfile = std::fs::File::create(&outpath)
                    .map_err(|e| format!("Failed to create file: {}", e))?;
                std::io::copy(&mut file, &mut outfile)
                    .map_err(|e| format!("Failed to extract file: {}", e))?;
            }
        }

        handle_github_archive_structure(&temp_extract_dir, &target_dir, &addon.name)?;
        let _ = std::fs::remove_dir_all(&temp_extract_dir);
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

fn handle_github_archive_structure(
    temp_dir: &PathBuf,
    target_dir: &PathBuf,
    addon_name: &str,
) -> Result<(), String> {
    if let Ok(entries) = std::fs::read_dir(temp_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() {
                let dir_name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");

                let expected_prefixes = [
                    format!("{}-main", addon_name),
                    format!("{}-master", addon_name),
                ];
                if expected_prefixes.iter().any(|p| dir_name == p) {
                    let final_path = target_dir.join(addon_name);
                    eprintln!(
                        "[DEBUG] Renaming {} -> {}",
                        path.display(),
                        final_path.display()
                    );
                    if final_path.exists() {
                        std::fs::remove_dir_all(&final_path)
                            .map_err(|e| format!("Failed to remove existing dir: {}", e))?;
                    }
                    std::fs::rename(&path, &final_path)
                        .map_err(|e| format!("Failed to rename dir: {}", e))?;
                    return Ok(());
                }
            }
        }
    }

    if let Ok(entries) = std::fs::read_dir(temp_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let src = entry.path();
            let dst = target_dir.join(entry.file_name());
            if src.is_dir() {
                copy_dir_all(&src, &dst)
                    .map_err(|e| format!("Failed to copy dir: {}", e))?;
            } else {
                std::fs::copy(&src, &dst)
                    .map_err(|e| format!("Failed to copy file: {}", e))?;
            }
        }
    }
    Ok(())
}

fn copy_dir_all(src: &PathBuf, dst: &PathBuf) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst.join(entry.file_name()))?;
        } else {
            std::fs::copy(entry.path(), dst.join(entry.file_name()))?;
        }
    }
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

// ============================================================================
// 🔁 ФОНОВАЯ ПРОВЕРКА ОБНОВЛЕНИЙ
// ============================================================================

/// Запускает фоновую проверку обновлений каждые 30 секунд
pub fn start_update_checker(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut interval = interval(Duration::from_secs(30));
        eprintln!("[UPDATE_CHECKER] Started, checking every 30s");

        loop {
            interval.tick().await;
            let state = app.state::<AddonManagerState>();

            if let Err(e) = check_for_updates(&app, &state).await {
                eprintln!("[UPDATE_CHECK] Error: {}", e);
            }
        }
    });
}

/// 🔥 ПРОВЕРКА ПРИ ЗАПУСКЕ: блокирует кнопку, проверяет, переустанавливает при необходимости
pub async fn startup_update_check(app: &AppHandle, state: &State<'_, AddonManagerState>) -> Result<bool, String> {
    eprintln!("[STARTUP_CHECK] Blocking launch button, checking for updates...");

    // 🔒 Блокируем кнопку запуска игры
    let _ = app.emit("launch-button-state", serde_json::json!({ "enabled": false }));

    let game_path = match state.get_game_path()? {
        Some(p) => p,
        None => {
            eprintln!("[STARTUP_CHECK] Game path not set, unblocking button");
            let _ = app.emit("launch-button-state", serde_json::json!({ "enabled": true }));
            return Ok(false);
        }
    };

    // 🔹 Читаем локальную версию
    let nsqc_vers_path = PathBuf::from(&game_path)
        .join("Interface")
        .join("AddOns")
        .join("NSQC")
        .join("vers");

    let local_version = match std::fs::read_to_string(&nsqc_vers_path) {
        Ok(content) => content.trim().to_string(),
        Err(_) => {
            eprintln!("[STARTUP_CHECK] Cannot read vers file, unblocking button");
            let _ = app.emit("launch-button-state", serde_json::json!({ "enabled": true }));
            return Ok(false);
        }
    };

    if local_version.is_empty() {
        let _ = app.emit("launch-button-state", serde_json::json!({ "enabled": true }));
        return Ok(false);
    }

    eprintln!("[STARTUP_CHECK] Local NSQC version: '{}'", local_version);

    // 🔹 Скачиваем HTML-страницу GitHub
    let github_url = "https://github.com/Vladgobelen/NSQC/blob/main/vers";
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let html_content = match client.get(github_url).send().await {
        Ok(r) => match r.text().await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[STARTUP_CHECK] Read error: {}", e);
                let _ = app.emit("launch-button-state", serde_json::json!({ "enabled": true }));
                return Ok(false);
            }
        },
        Err(e) => {
            eprintln!("[STARTUP_CHECK] Fetch error: {}", e);
            let _ = app.emit("launch-button-state", serde_json::json!({ "enabled": true }));
            return Ok(false);
        }
    };

    // 🔹 Ищем совпадение
    let found = html_content.contains(&local_version) ||
                html_content.contains(&html_escape(&local_version));

    if found {
        eprintln!("[STARTUP_CHECK] ✓ Version '{}' found — up to date", local_version);
        let _ = app.emit("launch-button-state", serde_json::json!({ "enabled": true }));
        return Ok(false); // Обновление не требуется
    }

    eprintln!("[STARTUP_CHECK] ✗ Version '{}' NOT found — UPDATE REQUIRED!", local_version);

    // 🔹 Принудительная переустановка
    let _ = force_reinstall_addons(app, state, &game_path).await;

    // 🔓 Разблокируем кнопку через 3 секунды после установки
    tokio::time::sleep(Duration::from_secs(3)).await;
    let _ = app.emit("launch-button-state", serde_json::json!({ "enabled": true }));

    Ok(true) // Обновление выполнено
}

async fn check_for_updates(
    app: &AppHandle,
    state: &State<'_, AddonManagerState>,
) -> Result<(), String> {
    let game_path = match state.get_game_path()? {
        Some(p) => p,
        None => return Ok(()),
    };

    let nsqc_vers_path = PathBuf::from(&game_path)
        .join("Interface")
        .join("AddOns")
        .join("NSQC")
        .join("vers");

    let local_version = match std::fs::read_to_string(&nsqc_vers_path) {
        Ok(content) => content.trim().to_string(),
        Err(_) => return Ok(()),
    };

    if local_version.is_empty() {
        return Ok(());
    }

    eprintln!("[UPDATE_CHECK] Local NSQC version: '{}'", local_version);

    let github_url = "https://github.com/Vladgobelen/NSQC/blob/main/vers";
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let html_content = match client.get(github_url).send().await {
        Ok(r) => match r.text().await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[UPDATE_CHECK] Read error: {}", e);
                return Ok(());
            }
        },
        Err(e) => {
            eprintln!("[UPDATE_CHECK] Fetch error: {}", e);
            return Ok(());
        }
    };

    let found = html_content.contains(&local_version) ||
                html_content.contains(&html_escape(&local_version));

    if found {
        eprintln!("[UPDATE_CHECK] ✓ Version '{}' found — up to date", local_version);
        return Ok(());
    }

    eprintln!("[UPDATE_CHECK] ✗ Version '{}' NOT found — UPDATE REQUIRED!", local_version);

    let _ = force_reinstall_addons(app, state, &game_path).await;

    Ok(())
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
     .replace('"', "&quot;")
     .replace('\'', "&#39;")
}

async fn force_reinstall_addons(
    app: &AppHandle,
    state: &State<'_, AddonManagerState>,
    game_path: &str,
) -> Result<(), String> {
    eprintln!("[REINSTALL] Starting forced reinstall: NSQC → NSQC3");

    let addons_config = match fetch_addons_config().await {
        Ok(c) => c,
        Err(e) => return Err(format!("Config fetch failed: {}", e)),
    };

    for addon_name in &["NSQC", "NSQC3"] {
        if let Some(config) = addons_config.get(*addon_name) {
            let link = config.get("link").and_then(|v| v.as_str()).unwrap_or("");
            let target_path = config.get("target_path").and_then(|v| v.as_str()).unwrap_or("");
            let description = config.get("description").and_then(|v| v.as_str()).unwrap_or("");
            let is_zip = if let Some(val) = config.get("is_zip").and_then(|v| v.as_bool()) {
                val
            } else {
                !link.to_lowercase().ends_with(".mpq")
            };

            let addon = AddonInfo {
                name: addon_name.to_string(),
                description: description.to_string(),
                installed: true,
                needs_update: false,
                being_processed: true,
                updating: true,
                link: link.to_string(),
                target_path: target_path.to_string(),
                is_zip,
            };

            {
                let mut addons = state.addons.lock().map_err(|e| e.to_string())?;
                if let Some(a) = addons.get_mut(*addon_name) {
                    a.being_processed = true;
                    a.updating = true;
                }
            }

            let _ = app.emit("addon-install-started", serde_json::json!({ "name": addon_name, "install": false }));

            eprintln!("[REINSTALL] Uninstalling {}", addon_name);
            let _ = uninstall_addon(app, &addon, Some(&game_path.to_string()));
            let _ = app.emit("progress", serde_json::json!({ "name": addon_name, "progress": 0.5 }));

            tokio::time::sleep(Duration::from_millis(300)).await;

            eprintln!("[REINSTALL] Installing {}", addon_name);
            let _ = install_addon(app, &addon, Some(&game_path.to_string())).await;
            let _ = app.emit("progress", serde_json::json!({ "name": addon_name, "progress": 1.0 }));

            {
                let mut addons = state.addons.lock().map_err(|e| e.to_string())?;
                if let Some(a) = addons.get_mut(*addon_name) {
                    a.being_processed = false;
                    a.updating = false;
                    a.installed = true;
                    a.needs_update = false;
                }
            }

            let _ = app.emit("operation-finished", serde_json::json!({ "name": addon_name, "success": true }));
            let _ = app.emit("addon-install-finished", serde_json::json!({ "name": addon_name, "success": true }));

            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    }

    let _ = load_addons(app.clone(), state.clone()).await;
    eprintln!("[REINSTALL] Completed");
    Ok(())
}

async fn fetch_addons_config() -> Result<serde_json::Map<String, serde_json::Value>, String> {
    let url = "https://raw.githubusercontent.com/Vladgobelen/NSQCu/main/addons.json";
    let client = reqwest::Client::new();

    let response = client
        .get(url)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("Config fetch: {}", e))?;

    let config: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Config parse: {}", e))?;

    config
        .get("addons")
        .and_then(|v| v.as_object().map(|o| o.clone()))
        .ok_or_else(|| "No 'addons' in config".to_string())
}