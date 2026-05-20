mod ipc;

use serde::{Deserialize, Serialize};
use shared::AppConfig;
use std::{path::PathBuf, sync::Mutex};

#[derive(Debug)]
pub struct AppState {
    settings: Mutex<AppConfig>,
    ipc: ipc::IPCService,
}

impl AppState {
    fn new() -> Self {
        AppState {
            settings: Mutex::new(AppConfig::new()),
            ipc: ipc::IPCService::new().unwrap(),
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

#[tauri::command]
fn update_config(state: tauri::State<AppState>, new_config: AppConfig) {
    let mut config = state.settings.lock().unwrap();
    *config = new_config;
    config.write();

    state.ipc.clone().update_config().unwrap();
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

fn learning_data_path() -> Result<PathBuf, String> {
    let appdata = std::env::var_os("APPDATA").ok_or("APPDATA is not set")?;
    Ok(PathBuf::from(appdata)
        .join("Azookey")
        .join("memory")
        .join("learning.tsv"))
}

#[tauri::command]
fn get_user_dictionary() -> Result<Vec<UserDictionaryEntry>, String> {
    let path = user_dictionary_path()?;
    let Ok(content) = std::fs::read_to_string(path) else {
        return Ok(Vec::new());
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
) -> Result<(), String> {
    let path = user_dictionary_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let content = entries
        .into_iter()
        .filter_map(|entry| {
            let reading = entry.reading.trim().replace(['\t', '\r', '\n'], "");
            let text = entry.text.trim().replace(['\t', '\r', '\n'], "");
            let part_of_speech = entry.part_of_speech.trim().replace(['\t', '\r', '\n'], "");
            if reading.is_empty() || text.is_empty() {
                return None;
            }
            Some(format!("{reading}\t{text}\t{part_of_speech}"))
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(path, content).map_err(|error| error.to_string())?;
    state
        .ipc
        .clone()
        .update_config()
        .map_err(|error| error.to_string())?;

    Ok(())
}

#[tauri::command]
fn clear_learning_data(state: tauri::State<AppState>) -> Result<(), String> {
    let path = learning_data_path()?;
    if path.exists() {
        std::fs::remove_file(path).map_err(|error| error.to_string())?;
    }
    state
        .ipc
        .clone()
        .update_config()
        .map_err(|error| error.to_string())?;

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
    // cuda:
    // cudart64_12.dll
    // cublas64_12.dll

    // vulkan:
    // vulkan-1.dllの存在確認

    let mut capability = Capability {
        cpu: true,
        cuda: false,
        vulkan: false,
    };

    // Check for CUDA availability
    let cuda_files = ["cudart64_12.dll", "cublas64_12.dll"];
    let cuda_available = cuda_files.iter().all(|file| {
        // Check if the file exists in system path or in the current directory
        std::env::var("PATH")
            .unwrap_or_default()
            .split(';')
            .map(PathBuf::from)
            .chain(std::iter::once(std::env::current_dir().unwrap_or_default()))
            .any(|path| path.join(file).exists())
    });
    capability.cuda = cuda_available;

    // Check for Vulkan availability
    let vulkan_file = "vulkan-1.dll";
    let vulkan_available = std::env::var("PATH")
        .unwrap_or_default()
        .split(';')
        .map(PathBuf::from)
        .chain(std::iter::once(std::env::current_dir().unwrap_or_default()))
        .any(|path| path.join(vulkan_file).exists());
    capability.vulkan = vulkan_available;

    capability
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
            get_user_dictionary,
            update_user_dictionary,
            clear_learning_data,
            check_capability
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
