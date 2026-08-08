use std::fs;
use std::path::PathBuf;
use tracing::{error, info};
use vietnamese_core::EngineConfig;

fn get_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|mut p| {
        p.push("VKey-rs");
        p.push("config.toml");
        p
    })
}

pub fn load_config() -> EngineConfig {
    let path = match get_config_path() {
        Some(p) => p,
        None => {
            warn_no_config_dir();
            return EngineConfig::default();
        }
    };

    if !path.exists() {
        info!("Config file not found at {:?}, using defaults", path);
        return EngineConfig::default();
    }

    match fs::read_to_string(&path) {
        Ok(content) => match toml::from_str(&content) {
            Ok(config) => {
                info!("Successfully loaded config from {:?}", path);
                config
            }
            Err(e) => {
                error!("Failed to parse config file: {:?}. Error: {}", path, e);
                EngineConfig::default()
            }
        },
        Err(e) => {
            error!("Failed to read config file: {:?}. Error: {}", path, e);
            EngineConfig::default()
        }
    }
}

pub fn save_config(config: &EngineConfig) {
    let path = match get_config_path() {
        Some(p) => p,
        None => {
            warn_no_config_dir();
            return;
        }
    };

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            error!(
                "Failed to create config directory {:?}. Error: {}",
                parent, e
            );
            return;
        }
    }

    match toml::to_string_pretty(config) {
        Ok(content) => {
            if let Err(e) = fs::write(&path, content) {
                error!("Failed to write config file to {:?}. Error: {}", path, e);
            } else {
                info!("Successfully saved config to {:?}", path);
            }
        }
        Err(e) => {
            error!("Failed to serialize config to TOML. Error: {}", e);
        }
    }
}

fn warn_no_config_dir() {
    error!("Could not locate user configuration directory (config_dir). Using defaults.");
}
