use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::storage::{data_dir, SledStore, KEY_SETTINGS};
use crate::theme::{Accent, ThemeMode};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LauncherSettings {
    pub theme_mode: ThemeMode,
    pub accent: Accent,
    pub ui_scale: u16,
    pub default_java_path: String,
    pub default_ram_mb: u32,
    pub global_jvm_args: String,
    pub default_game_dir: PathBuf,
    pub discord_presence: bool,
    pub crash_reporter: bool,
}

impl Default for LauncherSettings {
    fn default() -> Self {
        let default_game_dir = data_dir().unwrap_or_else(|_| PathBuf::from(".")).join("instances");
        Self {
            theme_mode: ThemeMode::Dark,
            accent: Accent::Pink,
            ui_scale: 100,
            default_java_path: "java".into(),
            default_ram_mb: 4096,
            global_jvm_args: "-XX:+UnlockExperimentalVMOptions".into(),
            default_game_dir,
            discord_presence: false,
            crash_reporter: true,
        }
    }
}

pub fn load(store: &SledStore) -> Result<LauncherSettings, AppError> {
    Ok(store.get(KEY_SETTINGS)?.unwrap_or_default())
}

pub fn save(store: &SledStore, settings: &LauncherSettings) -> Result<(), AppError> {
    store.set(KEY_SETTINGS, settings)
}
