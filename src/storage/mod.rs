pub mod accounts;
pub mod settings;

use std::path::PathBuf;

use serde::{de::DeserializeOwned, Serialize};

use crate::error::AppError;

pub const KEY_ACTIVE_ACCOUNT: &str = "accounts:active";
pub const KEY_ACCOUNT_PREFIX: &str = "accounts:";
pub const KEY_CLIENT_TOKEN: &str = "auth:yggdrasil:client_token";
pub const KEY_INSTANCE_PREFIX: &str = "instances:";
pub const KEY_SETTINGS: &str = "settings:launcher";

#[derive(Clone)]
pub struct SledStore {
    db: sled::Db,
}

impl SledStore {
    pub fn open() -> Result<Self, AppError> {
        let path = data_dir()?.join("swift-launcher.sled");
        Self::open_at(path)
    }

    pub fn open_at(path: PathBuf) -> Result<Self, AppError> {
        std::fs::create_dir_all(&path).map_err(|e| AppError::Storage(e.to_string()))?;
        Ok(Self { db: sled::open(path)? })
    }

    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, AppError> {
        match self.db.get(key)? {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            None => Ok(None),
        }
    }

    pub fn set<T: Serialize>(&self, key: &str, value: &T) -> Result<(), AppError> {
        let bytes = serde_json::to_vec(value)?;
        self.db.insert(key, bytes)?;
        self.db.flush()?;
        Ok(())
    }

    pub fn delete(&self, key: &str) -> Result<(), AppError> {
        self.db.remove(key)?;
        self.db.flush()?;
        Ok(())
    }

    pub fn scan_prefix<T: DeserializeOwned>(&self, prefix: &str) -> Result<Vec<T>, AppError> {
        let mut out = Vec::new();
        for item in self.db.scan_prefix(prefix) {
            let (_, value) = item?;
            out.push(serde_json::from_slice(&value)?);
        }
        Ok(out)
    }

    pub fn scan_prefix_excluding<T: DeserializeOwned>(&self, prefix: &str, excluded_keys: &[&str]) -> Result<Vec<T>, AppError> {
        let mut out = Vec::new();
        for item in self.db.scan_prefix(prefix) {
            let (key, value) = item?;
            let key = String::from_utf8_lossy(&key);
            if excluded_keys.iter().any(|excluded| key.as_ref() == *excluded) {
                continue;
            }
            out.push(serde_json::from_slice(&value)?);
        }
        Ok(out)
    }
}

pub fn yggdrasil_client_token() -> Result<String, AppError> {
    let path = data_dir()?.join("yggdrasil-client-token");
    if let Ok(token) = std::fs::read_to_string(&path) {
        let token = token.trim().to_string();
        if !token.is_empty() {
            return Ok(token);
        }
    }
    let token = generate_client_token();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| AppError::Storage(error.to_string()))?;
    }
    std::fs::write(path, &token).map_err(|error| AppError::Storage(error.to_string()))?;
    Ok(token)
}

fn generate_client_token() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("swift-launcher-{now}-{}", std::process::id())
}

pub fn data_dir() -> Result<PathBuf, AppError> {
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join("Library").join("Application Support"));
    #[cfg(all(unix, not(target_os = "macos")))]
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local").join("share")));

    base.map(|path| path.join("swift-launcher"))
        .ok_or_else(|| AppError::Storage("could not resolve platform data directory".into()))
}
