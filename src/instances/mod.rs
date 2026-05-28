pub mod archive;
pub mod crash;
pub mod create;
pub mod install;
pub mod launch;
pub mod launch_monitor;
pub mod mods;
pub mod screenshots;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::storage::{data_dir, SledStore, KEY_INSTANCE_PREFIX};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoaderKind {
    Vanilla,
    Fabric,
    Forge,
    NeoForge,
    Quilt,
}

impl std::fmt::Display for LoaderKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Vanilla => f.write_str("Vanilla"),
            Self::Fabric => f.write_str("Fabric"),
            Self::Forge => f.write_str("Forge"),
            Self::NeoForge => f.write_str("NeoForge"),
            Self::Quilt => f.write_str("Quilt"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortMode {
    Name,
    LastPlayed,
    Version,
}

impl std::fmt::Display for SortMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Name => f.write_str("Name"),
            Self::LastPlayed => f.write_str("Last played"),
            Self::Version => f.write_str("Version"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstanceTab {
    Overview,
    Mods,
    Files,
    Settings,
    Logs,
}

impl std::fmt::Display for InstanceTab {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Overview => f.write_str("Overview"),
            Self::Mods => f.write_str("Mods"),
            Self::Files => f.write_str("Files"),
            Self::Settings => f.write_str("Settings"),
            Self::Logs => f.write_str("Logs"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstanceRunState {
    Idle,
    Preparing,
    Running,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instance {
    pub id: String,
    pub name: String,
    pub minecraft_version: String,
    pub loader: LoaderKind,
    #[serde(default)]
    pub loader_version: Option<String>,
    pub path: PathBuf,
    pub artwork_path: Option<PathBuf>,
    pub last_played_unix: Option<u64>,
    pub playtime_seconds: u64,
    pub ram_mb: u32,
    pub java_path: String,
    pub jvm_args: String,
    pub resolution_width: u32,
    pub resolution_height: u32,
    pub fullscreen: bool,
    pub game_dir_override: String,
    pub server: String,
    pub run_state: InstanceRunState,
}

#[derive(Clone)]
pub struct InstanceManager {
    store: SledStore,
}

impl InstanceManager {
    pub fn new(store: SledStore) -> Self {
        Self { store }
    }

    pub fn list(&self) -> Result<Vec<Instance>, AppError> {
        let mut instances = self.store.scan_prefix::<Instance>(KEY_INSTANCE_PREFIX)?;
        instances.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(instances)
    }

    pub fn save(&self, instance: &Instance) -> Result<(), AppError> {
        self.store
            .set(&format!("{KEY_INSTANCE_PREFIX}{}", instance.id), instance)
    }

    pub fn delete(&self, id: &str) -> Result<(), AppError> {
        self.store.delete(&format!("{KEY_INSTANCE_PREFIX}{id}"))
    }
}

pub fn instance_root() -> Result<PathBuf, AppError> {
    Ok(data_dir()?.join("instances"))
}
