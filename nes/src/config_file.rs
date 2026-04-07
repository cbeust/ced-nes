use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::{error, info};

// Name of the config file, which is saved under whatever dirs::config_dir() returns
const CONFIG_DIR: &str = "ced-nes";
const CONFIG_FILE: &str = "config.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EmulatorConfig {
    pub rom_dir: Option<String>,
}

// C:\\users\\cedric\\t

impl Default for EmulatorConfig {
    fn default() -> Self {
        // let mut dir = dirs::home_dir().unwrap();
        // dir.push("t");
        // dir.push("roms");
        // Self { rom_dir: Some(dir.to_string_lossy().into()) }
        Self { rom_dir: None }
    }
}
impl EmulatorConfig {
    /// If a EmulatorConfig already exists, read it; if not, create it with defaults, then return it.
    pub fn read_or_create() -> Result<EmulatorConfig, String> {
        let file_name = Self::config_file_name();
        if ! Path::new(&file_name).exists() {
            let cfg = EmulatorConfig::default();
            cfg.save().unwrap();
            Ok(cfg)
        } else {
            match fs::read_to_string(file_name) {
                Ok(s) => { Ok(serde_json::from_str::<EmulatorConfig>(&s).unwrap()) }
                Err(e) => { Err(e.to_string()) }
            }
        }
    }

    /// Returns the fully qualified path to the config file
    /// Creates the directory that contains it if it doesn't already exist
    pub(crate) fn config_file_name() -> String {
        let mut path = dirs::config_dir().unwrap();
        path.push(CONFIG_DIR);
        if !path.exists() {
            let _ = fs::create_dir_all(&path);
        }
        path.push(CONFIG_FILE);
        path.to_string_lossy().into()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let p = Self::config_file_name();
        let serialized = serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string());
        let mut f = File::create(p.clone())?;
        info!("Created {}", p);
        f.write_all(serialized.as_bytes())
    }
}
