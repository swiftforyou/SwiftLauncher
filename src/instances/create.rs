use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use tokio::sync::mpsc;

use crate::instances::install::InstallProgress;
use crate::download::DownloadControl;
use crate::error::AppError;
use crate::instances::{install, instance_root, Instance, InstanceRunState, LoaderKind};

pub async fn create_instance(name: String, version: String, loader: LoaderKind) -> Result<Instance, AppError> {
    create_instance_with_status(name, version, loader, None, None).await
}

pub async fn create_instance_with_status(
    name: String,
    version: String,
    loader: LoaderKind,
    loader_version_choice: Option<String>,
    status_tx: Option<mpsc::UnboundedSender<InstallProgress>>,
) -> Result<Instance, AppError> {
    create_instance_with_status_and_control(name, version, loader, loader_version_choice, status_tx, None).await
}

pub async fn create_instance_with_status_and_control(
    name: String,
    version: String,
    loader: LoaderKind,
    loader_version_choice: Option<String>,
    status_tx: Option<mpsc::UnboundedSender<InstallProgress>>,
    control_rx: Option<tokio::sync::watch::Receiver<DownloadControl>>,
) -> Result<Instance, AppError> {
    if matches!(loader, LoaderKind::Forge | LoaderKind::NeoForge) {
        return Err(AppError::Instance(format!(
            "{loader} installer is not implemented yet. Choose Fabric, Quilt, or Vanilla for now."
        )));
    }
    send_status(&status_tx, "Preparing instance folders", 0.03);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| AppError::Instance(e.to_string()))?
        .as_secs();
    let safe_name = sanitize_name(&name);
    let id = format!("{safe_name}-{now}");
    let path = instance_root()?.join(&id);
    tokio::fs::create_dir_all(&path).await?;
    tokio::fs::create_dir_all(path.join("mods")).await?;
    tokio::fs::create_dir_all(path.join("logs")).await?;
    tokio::fs::create_dir_all(path.join("screenshots")).await?;
    tokio::fs::create_dir_all(path.join("resourcepacks")).await?;
    if let Some(control_rx) = control_rx {
        install::install_minecraft_version_with_status_and_control(&version, status_tx.clone(), control_rx).await?;
    } else {
        install::install_minecraft_version_with_status(&version, status_tx.clone()).await?;
    }
    let loader_version = install::install_loader_profile_with_status(loader, &version, loader_version_choice, status_tx.clone()).await?;
    send_status(&status_tx, "Saving instance metadata", 0.98);

    Ok(Instance {
        id,
        name,
        minecraft_version: version,
        loader,
        loader_version,
        path,
        artwork_path: None,
        last_played_unix: None,
        playtime_seconds: 0,
        ram_mb: 4096,
        java_path: "java".into(),
        jvm_args: String::new(),
        resolution_width: 1280,
        resolution_height: 720,
        fullscreen: false,
        game_dir_override: String::new(),
        server: String::new(),
        run_state: InstanceRunState::Idle,
    })
}

fn send_status(status_tx: &Option<mpsc::UnboundedSender<InstallProgress>>, status: impl Into<String>, progress: f32) {
    if let Some(tx) = status_tx {
        let _ = tx.send(InstallProgress {
            status: status.into(),
            progress,
        });
    }
}

fn sanitize_name(input: &str) -> String {
    let out: String = input
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
        .collect();
    if out.is_empty() { "instance".into() } else { out }
}

#[derive(Debug, Deserialize)]
struct VersionManifest {
    versions: Vec<ManifestVersion>,
}

#[derive(Debug, Deserialize)]
pub struct ManifestVersion {
    pub id: String,
    pub url: String,
}

pub async fn fetch_available_versions() -> Result<Vec<String>, AppError> {
    let response = reqwest::Client::new()
        .get("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json")
        .send()
        .await?
        .error_for_status()?
        .json::<VersionManifest>()
        .await?;

    let versions = response
        .versions
        .into_iter()
        .map(|version| version.id)
        .collect::<Vec<_>>();

    if versions.is_empty() {
        return Err(AppError::Network("Mojang version manifest did not include versions".into()));
    }

    Ok(versions)
}

pub fn fallback_versions() -> Vec<String> {
    [
        "1.21.8", "1.21.7", "1.21.6", "1.21.5", "1.21.4", "1.21.3", "1.21.2", "1.21.1", "1.21",
        "1.20.6", "1.20.4", "1.20.1", "1.19.4", "1.19.2", "1.18.2", "1.17.1", "1.16.5", "1.12.2", "1.8.9",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}
