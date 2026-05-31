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
    #[serde(default)]
    pub curseforge_api_key: String,
}

impl Default for LauncherSettings {
    fn default() -> Self {
        let default_game_dir = data_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("instances");
        Self {
            theme_mode: ThemeMode::Dark,
            accent: Accent::Green,
            ui_scale: 100,
            default_java_path: "java".into(),
            default_ram_mb: 4096,
            global_jvm_args: "-XX:+UnlockExperimentalVMOptions".into(),
            default_game_dir,
            discord_presence: false,
            crash_reporter: true,
            curseforge_api_key: bundled_curseforge_api_key(),
        }
    }
}

pub fn load(store: &SledStore) -> Result<LauncherSettings, AppError> {
    let mut settings: LauncherSettings = store.get(KEY_SETTINGS)?.unwrap_or_default();
    if settings.curseforge_api_key.trim().is_empty() {
        settings.curseforge_api_key = bundled_curseforge_api_key();
    }
    Ok(settings)
}

pub fn save(store: &SledStore, settings: &LauncherSettings) -> Result<(), AppError> {
    store.set(KEY_SETTINGS, settings)
}

pub fn bundled_curseforge_api_key() -> String {
    option_env!("SWIFT_LAUNCHER_CURSEFORGE_API_KEY")
        .map(str::to_owned)
        .or_else(|| std::env::var("SWIFT_LAUNCHER_CURSEFORGE_API_KEY").ok())
        .unwrap_or_default()
}
