use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/azookey.rs"));
    include!(concat!(env!("OUT_DIR"), "/window.rs"));
    pub const FILE_DESCRIPTOR_SET: &[u8] =
        tonic::include_file_descriptor_set!("azookey_service_descriptor");
}

fn get_config_root() -> PathBuf {
    let appdata = PathBuf::from(std::env::var("APPDATA").unwrap());
    appdata.join("Azookey")
}

const SETTINGS_FILENAME: &str = "settings.json";

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ZenzaiConfig {
    #[serde(default)]
    pub enable: bool,
    #[serde(default)]
    pub profile: String,
    #[serde(default)]
    pub personalization: bool,
    #[serde(default)]
    pub personalization_path: String,
    #[serde(default = "default_zenzai_backend")]
    pub backend: String,
    #[serde(default = "default_zenzai_inference_limit")]
    pub inference_limit: usize,
    #[serde(default)]
    pub model_path: String,
    #[serde(default)]
    pub command_path: String,
    #[serde(default = "default_zenzai_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub prediction: bool,
    #[serde(default = "default_zenzai_prediction_token_limit")]
    pub prediction_token_limit: usize,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MagicConversionConfig {
    #[serde(default)]
    pub enable: bool,
    #[serde(default)]
    pub command_path: String,
    #[serde(default = "default_magic_conversion_timeout_ms")]
    pub timeout_ms: u64,
}

impl Default for MagicConversionConfig {
    fn default() -> Self {
        Self {
            enable: false,
            command_path: String::new(),
            timeout_ms: default_magic_conversion_timeout_ms(),
        }
    }
}

impl Default for ZenzaiConfig {
    fn default() -> Self {
        ZenzaiConfig {
            enable: false,
            profile: "".to_string(),
            personalization: false,
            personalization_path: "".to_string(),
            backend: default_zenzai_backend(),
            inference_limit: default_zenzai_inference_limit(),
            model_path: "".to_string(),
            command_path: "".to_string(),
            timeout_ms: default_zenzai_timeout_ms(),
            prediction: false,
            prediction_token_limit: default_zenzai_prediction_token_limit(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LearningConfig {
    #[serde(default = "default_enabled")]
    pub enable: bool,
}

impl Default for LearningConfig {
    fn default() -> Self {
        LearningConfig {
            enable: default_enabled(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ConversionConfig {
    #[serde(default = "default_enabled")]
    pub live_conversion: bool,
    #[serde(default = "default_enabled")]
    pub prediction: bool,
    #[serde(default = "default_enabled")]
    pub typo_correction: bool,
    #[serde(default = "default_input_style")]
    pub input_style: String,
    #[serde(default)]
    pub custom_input_table_path: String,
    #[serde(default = "default_enabled")]
    pub candidate_number_selection: bool,
}

impl Default for ConversionConfig {
    fn default() -> Self {
        ConversionConfig {
            live_conversion: default_enabled(),
            prediction: default_enabled(),
            typo_correction: default_enabled(),
            input_style: default_input_style(),
            custom_input_table_path: String::new(),
            candidate_number_selection: default_enabled(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AppConfig {
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub zenzai: ZenzaiConfig,
    #[serde(default)]
    pub magic_conversion: MagicConversionConfig,
    #[serde(default)]
    pub learning: LearningConfig,
    #[serde(default)]
    pub conversion: ConversionConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            version: default_version(),
            zenzai: ZenzaiConfig::default(),
            magic_conversion: MagicConversionConfig::default(),
            learning: LearningConfig::default(),
            conversion: ConversionConfig::default(),
        }
    }
}

fn default_version() -> String {
    "0.1.0".to_string()
}

fn default_zenzai_backend() -> String {
    "cpu".to_string()
}

fn default_zenzai_inference_limit() -> usize {
    1
}

fn default_zenzai_timeout_ms() -> u64 {
    1500
}

fn default_zenzai_prediction_token_limit() -> usize {
    32
}

fn default_magic_conversion_timeout_ms() -> u64 {
    1500
}

fn default_enabled() -> bool {
    true
}

fn default_input_style() -> String {
    "default".to_string()
}

impl AppConfig {
    pub fn write(&self) {
        let config_path = get_config_root().join(SETTINGS_FILENAME);
        let config_str = serde_json::to_string_pretty(self).unwrap();
        std::fs::write(config_path, config_str).unwrap();
    }

    pub fn read() -> Self {
        let config_path = get_config_root().join(SETTINGS_FILENAME);
        if !config_path.exists() {
            return AppConfig::default();
        }
        let config_str = std::fs::read_to_string(config_path).unwrap();
        serde_json::from_str(&config_str).unwrap()
    }

    pub fn new() -> Self {
        let config_path = get_config_root();
        if !config_path.exists() {
            std::fs::create_dir_all(&config_path).unwrap();
        }
        let config = AppConfig::read();
        config.write();
        config
    }
}
