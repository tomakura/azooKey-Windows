mod ipc;

use serde::{Deserialize, Serialize};
use shared::AppConfig;
use std::{path::PathBuf, sync::Mutex};

#[derive(Debug)]
pub struct AppState {
    settings: Mutex<AppConfig>,
    ipc: Mutex<Option<ipc::IPCService>>,
}

impl AppState {
    fn new() -> Self {
        AppState {
            settings: Mutex::new(AppConfig::new()),
            ipc: Mutex::new(ipc::IPCService::new().ok()),
        }
    }
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn get_config(state: tauri::State<AppState>) -> AppConfig {
    let config = state.settings.lock().unwrap();
    config.clone()
}

fn notify_server_config_update(state: &tauri::State<AppState>) -> Result<bool, String> {
    let mut ipc = state.ipc.lock().map_err(|error| error.to_string())?;
    if ipc.is_none() {
        *ipc = ipc::IPCService::new().ok();
    }
    if let Some(service) = ipc.as_mut() {
        match service.update_config() {
            Ok(()) => return Ok(true),
            Err(error) if ipc::is_connection_error(&error) => *ipc = None,
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(false)
}

#[tauri::command]
fn update_config(state: tauri::State<AppState>, new_config: AppConfig) -> Result<bool, String> {
    let mut config = state.settings.lock().unwrap();
    new_config.try_write().map_err(|error| error.to_string())?;
    match notify_server_config_update(&state) {
        Ok(applied) => {
            *config = new_config;
            Ok(applied)
        }
        Err(error) => {
            config
                .try_write()
                .map_err(|restore| format!("{error}; restoring settings: {restore}"))?;
            Err(error)
        }
    }
}

#[tauri::command]
async fn restart_engine(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let service = state.ipc.lock().map_err(|error| error.to_string())?.take();
    let connected = tauri::async_runtime::spawn_blocking(move || {
        let exe = std::env::current_exe().map_err(|error| error.to_string())?;
        let directory = exe
            .parent()
            .ok_or("設定アプリのフォルダーを確認できません")?;
        ipc::restart_engine(service, directory).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())??;
    *state.ipc.lock().map_err(|error| error.to_string())? = Some(connected);
    Ok(())
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct UserDictionaryEntry {
    reading: String,
    text: String,
    part_of_speech: String,
}

fn user_dictionary_path() -> Result<PathBuf, String> {
    let appdata = std::env::var_os("APPDATA").ok_or("APPDATA is not set")?;
    Ok(PathBuf::from(appdata)
        .join("Azookey")
        .join("user_dictionary.tsv"))
}

fn input_table_path() -> Result<PathBuf, String> {
    let appdata = std::env::var_os("APPDATA").ok_or("APPDATA is not set")?;
    Ok(PathBuf::from(appdata)
        .join("Azookey")
        .join("input_table.tsv"))
}

#[tauri::command]
fn get_user_dictionary() -> Result<Vec<UserDictionaryEntry>, String> {
    let path = user_dictionary_path()?;
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.to_string()),
    };

    let entries = content
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(3, '\t');
            let reading = fields.next()?.trim();
            let text = fields.next()?.trim();
            let part_of_speech = fields.next().map(str::trim).unwrap_or_default();
            if reading.is_empty() || text.is_empty() {
                return None;
            }
            Some(UserDictionaryEntry {
                reading: reading.to_string(),
                text: text.to_string(),
                part_of_speech: part_of_speech.to_string(),
            })
        })
        .collect();

    Ok(entries)
}

#[tauri::command]
fn update_user_dictionary(
    state: tauri::State<AppState>,
    entries: Vec<UserDictionaryEntry>,
) -> Result<bool, String> {
    let path = user_dictionary_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let content = entries
        .into_iter()
        .map(|entry| {
            let reading = entry.reading.trim();
            let text = entry.text.trim();
            let part_of_speech = entry.part_of_speech.trim();
            if reading.is_empty()
                || text.is_empty()
                || [reading, text, part_of_speech]
                    .iter()
                    .any(|field| field.contains(['\t', '\r', '\n']))
            {
                return Err("Invalid dictionary entry".to_string());
            }
            Ok(format!("{reading}\t{text}\t{part_of_speech}"))
        })
        .collect::<Result<Vec<_>, String>>()?
        .join("\n");
    std::fs::write(path, content).map_err(|error| error.to_string())?;
    notify_server_config_update(&state)
}

#[tauri::command]
fn clear_learning_data(state: tauri::State<AppState>) -> Result<(), String> {
    let Ok(mut ipc) = state.ipc.lock() else {
        return Err("failed to lock ipc state".to_string());
    };
    if ipc.is_none() {
        *ipc = ipc::IPCService::new().ok();
    }
    let Some(service) = ipc.as_mut() else {
        return Err("azookey server is not running".to_string());
    };
    if let Err(error) = service.reset_learning() {
        *ipc = None;
        return Err(error.to_string());
    }

    Ok(())
}

#[tauri::command]
fn get_input_table() -> Result<String, String> {
    let path = input_table_path()?;
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(content),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error.to_string()),
    }
}

#[tauri::command]
fn update_input_table(state: tauri::State<AppState>, content: String) -> Result<(), String> {
    let path = input_table_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let validated = content
        .lines()
        .enumerate()
        .map(|(index, line)| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return Ok(line.to_string());
            }
            let fields: Vec<_> = line.split('\t').collect();
            if fields.len() != 2 || fields.iter().any(|field| field.trim().is_empty()) {
                return Err(format!(
                    "Invalid input table row {}: expected key<TAB>value",
                    index + 1
                ));
            }
            Ok(format!("{}\t{}", fields[0].trim(), fields[1].trim()))
        })
        .collect::<Result<Vec<_>, String>>()?
        .join("\n");
    std::fs::write(path, validated).map_err(|error| error.to_string())?;
    notify_server_config_update(&state)?;

    Ok(())
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct Capability {
    cpu: bool,
    cuda: bool,
    vulkan: bool,
}

#[tauri::command]
fn check_capability() -> Capability {
    let directory = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let system = PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32");
    Capability {
        cpu: directory.join("llama_cpu/llama.dll").is_file(),
        cuda: directory.join("llama_cuda/llama.dll").is_file()
            && system.join("nvcuda.dll").is_file(),
        vulkan: directory.join("llama_vulkan/llama.dll").is_file()
            && system.join("vulkan-1.dll").is_file(),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_state = AppState::new();

    tauri::Builder::default()
        .manage(app_state)
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            get_config,
            update_config,
            restart_engine,
            get_user_dictionary,
            update_user_dictionary,
            clear_learning_data,
            get_input_table,
            update_input_table,
            check_capability
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
