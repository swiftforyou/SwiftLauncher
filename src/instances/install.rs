use std::path::Path;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::download::{download_jobs_checked_with_progress, download_jobs_checked_with_progress_and_control, DownloadControl, DownloadJob};
use crate::error::AppError;
use crate::instances::LoaderKind;
use crate::storage::data_dir;

const VERSION_MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

#[derive(Debug, Clone)]
pub struct InstallProgress {
    pub status: String,
    pub progress: f32,
}

#[derive(Debug, Deserialize)]
struct VersionManifest {
    versions: Vec<ManifestVersion>,
}

#[derive(Debug, Deserialize)]
struct ManifestVersion {
    id: String,
    url: String,
}

#[derive(Debug, Deserialize)]
struct VersionJson {
    id: String,
    #[serde(rename = "assetIndex")]
    asset_index: AssetIndexInfo,
    downloads: VersionDownloads,
    #[serde(default)]
    libraries: Vec<Library>,
}

#[derive(Debug, Deserialize)]
struct LoaderVersion {
    loader: LoaderInfo,
}

#[derive(Debug, Deserialize)]
struct LoaderInfo {
    version: String,
}

#[derive(Debug, Deserialize)]
struct VersionDownloads {
    client: DownloadInfo,
}

#[derive(Debug, Deserialize)]
struct AssetIndexInfo {
    id: String,
    sha1: Option<String>,
    size: Option<u64>,
    url: String,
}

#[derive(Debug, Deserialize)]
struct DownloadInfo {
    sha1: Option<String>,
    size: Option<u64>,
    url: String,
}

#[derive(Debug, Deserialize)]
struct Library {
    name: String,
    downloads: Option<LibraryDownloads>,
    #[serde(default)]
    url: Option<String>,
    rules: Option<Vec<Rule>>,
    natives: Option<std::collections::BTreeMap<String, String>>,
}

#[derive(Debug, Deserialize)]
struct LibraryDownloads {
    artifact: Option<LibraryArtifact>,
    classifiers: Option<std::collections::BTreeMap<String, LibraryArtifact>>,
}

#[derive(Debug, Deserialize)]
struct LibraryArtifact {
    path: String,
    sha1: Option<String>,
    size: Option<u64>,
    url: String,
}

#[derive(Debug, Deserialize)]
struct Rule {
    action: String,
    os: Option<RuleOs>,
}

#[derive(Debug, Deserialize)]
struct RuleOs {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AssetIndex {
    objects: std::collections::BTreeMap<String, AssetObject>,
}

#[derive(Debug, Deserialize)]
struct AssetObject {
    hash: String,
    size: Option<u64>,
}

pub async fn install_minecraft_version(version_id: &str) -> Result<(), AppError> {
    install_minecraft_version_with_status(version_id, None).await
}

pub async fn install_loader_profile_with_status(
    loader: LoaderKind,
    minecraft_version: &str,
    loader_version: Option<String>,
    status_tx: Option<mpsc::UnboundedSender<InstallProgress>>,
) -> Result<Option<String>, AppError> {
    match loader {
        LoaderKind::Vanilla => Ok(None),
        LoaderKind::Fabric => install_fabric_like_profile(
            "Fabric",
            "https://meta.fabricmc.net/v2/versions/loader",
            minecraft_version,
            loader_version,
            status_tx,
        )
        .await
        .map(Some),
        LoaderKind::Quilt => install_fabric_like_profile(
            "Quilt",
            "https://meta.quiltmc.org/v3/versions/loader",
            minecraft_version,
            loader_version,
            status_tx,
        )
        .await
        .map(Some),
        LoaderKind::Forge | LoaderKind::NeoForge => Err(AppError::Instance(format!(
            "{loader} installer support is not wired yet. Fabric and Quilt are available now."
        ))),
    }
}

pub async fn install_minecraft_version_with_status_and_control(
    version_id: &str,
    status_tx: Option<mpsc::UnboundedSender<InstallProgress>>,
    control_rx: tokio::sync::watch::Receiver<DownloadControl>,
) -> Result<(), AppError> {
    install_minecraft_version_inner(version_id, status_tx, Some(control_rx)).await
}

pub async fn fetch_loader_versions(loader: LoaderKind, minecraft_version: &str) -> Result<Vec<String>, AppError> {
    match loader {
        LoaderKind::Vanilla => Ok(Vec::new()),
        LoaderKind::Fabric => fetch_fabric_like_loader_versions(
            "https://meta.fabricmc.net/v2/versions/loader",
            minecraft_version,
        )
        .await,
        LoaderKind::Quilt => fetch_fabric_like_loader_versions(
            "https://meta.quiltmc.org/v3/versions/loader",
            minecraft_version,
        )
        .await,
        LoaderKind::Forge | LoaderKind::NeoForge => Err(AppError::Instance(format!(
            "{loader} installer needs Forge processor support before version selection is enabled"
        ))),
    }
}

async fn fetch_fabric_like_loader_versions(endpoint: &str, minecraft_version: &str) -> Result<Vec<String>, AppError> {
    let versions = reqwest::Client::new()
        .get(format!("{endpoint}/{minecraft_version}"))
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<LoaderVersion>>()
        .await?;
    let versions = versions
        .into_iter()
        .map(|version| version.loader.version)
        .collect::<Vec<_>>();
    if versions.is_empty() {
        return Err(AppError::Instance(format!("No loader versions found for Minecraft {minecraft_version}")));
    }
    Ok(versions)
}

async fn install_fabric_like_profile(
    label: &str,
    endpoint: &str,
    minecraft_version: &str,
    selected_loader_version: Option<String>,
    status_tx: Option<mpsc::UnboundedSender<InstallProgress>>,
) -> Result<String, AppError> {
    let client = reqwest::Client::new();
    send_status(&status_tx, format!("Resolving {label} loader"), 0.96);
    let loader_version = match selected_loader_version.filter(|version| !version.trim().is_empty()) {
        Some(version) => version,
        None => fetch_fabric_like_loader_versions(endpoint, minecraft_version)
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| AppError::Instance(format!("{label} has no loader for Minecraft {minecraft_version}")))?,
    };

    send_status(&status_tx, format!("Downloading {label} loader {loader_version} profile"), 0.965);
    let profile = client
        .get(format!("{endpoint}/{minecraft_version}/{loader_version}/profile/json"))
        .send()
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;
    let profile_id = json_string(&profile, "id")
        .ok_or_else(|| AppError::Instance(format!("{label} profile did not include id")))?;
    let libraries = serde_json::from_value::<Vec<Library>>(profile.get("libraries").cloned().unwrap_or_default())?;

    let root = data_dir()?;
    let profile_dir = root.join("versions").join(&profile_id);
    tokio::fs::create_dir_all(&profile_dir).await?;
    write_json(&profile_dir.join(format!("{profile_id}.json")), &profile).await?;

    let libraries_dir = root.join("libraries");
    let jobs = libraries
        .iter()
        .filter_map(|library| {
            if let Some(ref downloads) = library.downloads {
                if let Some(ref artifact) = downloads.artifact {
                    if !artifact.url.is_empty() {
                        return Some(DownloadJob {
                            id: format!("loader-library:{}", library.name),
                            url: artifact.url.clone(),
                            destination_path: libraries_dir.join(&artifact.path),
                            expected_sha1: artifact.sha1.clone(),
                            size_bytes: artifact.size,
                        });
                    }
                }
            }
            if let Some(ref repo_url) = library.url {
                let path = maven_name_to_path(&library.name);
                let download_url = format!("{}/{}", repo_url.trim_end_matches('/'), path);
                return Some(DownloadJob {
                    id: format!("loader-library:{}", library.name),
                    url: download_url,
                    destination_path: libraries_dir.join(&path),
                    expected_sha1: None,
                    size_bytes: None,
                });
            }
            None
        })
        .collect::<Vec<_>>();
    send_status(&status_tx, format!("Downloading {label} loader libraries"), 0.975);
    download_jobs_checked_with_progress(jobs, None).await?;
    Ok(profile_id)
}

fn json_string(value: &serde_json::Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(str::to_string)
}

fn maven_name_to_path(name: &str) -> String {
    let parts: Vec<&str> = name.split(':').collect();
    let group = parts.first().unwrap_or(&"");
    let artifact = parts.get(1).unwrap_or(&"");
    let version = parts.get(2).unwrap_or(&"");
    let group_path = group.replace('.', "/");
    format!("{group_path}/{artifact}/{version}/{artifact}-{version}.jar")
}

pub async fn install_minecraft_version_with_status(
    version_id: &str,
    status_tx: Option<mpsc::UnboundedSender<InstallProgress>>,
) -> Result<(), AppError> {
    install_minecraft_version_inner(version_id, status_tx, None).await
}

async fn install_minecraft_version_inner(
    version_id: &str,
    status_tx: Option<mpsc::UnboundedSender<InstallProgress>>,
    control_rx: Option<tokio::sync::watch::Receiver<DownloadControl>>,
) -> Result<(), AppError> {
    send_status(&status_tx, format!("Resolving Minecraft {version_id}"), 0.05);
    let client = reqwest::Client::new();
    let root = data_dir()?;
    let versions_dir = root.join("versions").join(version_id);
    let libraries_dir = root.join("libraries");
    let assets_dir = root.join("assets");
    if install_marker_valid(&versions_dir, version_id).await {
        send_status(&status_tx, format!("Minecraft {version_id} already installed"), 1.0);
        return Ok(());
    }
    tokio::fs::create_dir_all(&versions_dir).await?;
    tokio::fs::create_dir_all(&libraries_dir).await?;
    tokio::fs::create_dir_all(assets_dir.join("indexes")).await?;
    tokio::fs::create_dir_all(assets_dir.join("objects")).await?;

    let version_url = version_url(&client, version_id).await?;
    send_status(&status_tx, "Downloading version metadata", 0.08);
    let version_value: serde_json::Value = get_json(&client, &version_url).await?;
    let version_json: VersionJson = serde_json::from_value(version_value.clone())?;
    let version_json_path = versions_dir.join(format!("{version_id}.json"));
    write_json(&version_json_path, &version_value).await?;

    let mut jobs = Vec::new();
    jobs.push(DownloadJob {
        id: format!("client:{version_id}"),
        url: version_json.downloads.client.url.clone(),
        destination_path: versions_dir.join(format!("{version_id}.jar")),
        expected_sha1: version_json.downloads.client.sha1.clone(),
        size_bytes: version_json.downloads.client.size,
    });

    for library in &version_json.libraries {
        if !library_allowed_on_current_os(library) {
            continue;
        }
        if let Some(artifact) = library.downloads.as_ref().and_then(|downloads| downloads.artifact.as_ref()) {
            if !artifact.url.is_empty() {
                jobs.push(DownloadJob {
                    id: format!("library:{}", library.name),
                    url: artifact.url.clone(),
                    destination_path: libraries_dir.join(&artifact.path),
                    expected_sha1: artifact.sha1.clone(),
                    size_bytes: artifact.size,
                });
            }
        }

        if let Some(classifier_name) = native_classifier_for_current_os(library) {
            if let Some(artifact) = library
                .downloads
                .as_ref()
                .and_then(|downloads| downloads.classifiers.as_ref())
                .and_then(|classifiers| classifiers.get(&classifier_name))
            {
                if !artifact.url.is_empty() {
                    jobs.push(DownloadJob {
                        id: format!("native:{}:{classifier_name}", library.name),
                        url: artifact.url.clone(),
                        destination_path: libraries_dir.join(&artifact.path),
                        expected_sha1: artifact.sha1.clone(),
                        size_bytes: artifact.size,
                    });
                }
            }
        }
    }

    let asset_index_path = assets_dir.join("indexes").join(format!("{}.json", version_json.asset_index.id));
    jobs.push(DownloadJob {
        id: format!("asset-index:{}", version_json.asset_index.id),
        url: version_json.asset_index.url.clone(),
        destination_path: asset_index_path.clone(),
        expected_sha1: version_json.asset_index.sha1.clone(),
        size_bytes: version_json.asset_index.size,
    });

    let core_total = jobs.len();
    send_status(&status_tx, format!("Downloading and verifying {core_total} core files"), 0.12);
    let (core_tx, mut core_rx) = mpsc::unbounded_channel();
    let status_for_core = status_tx.clone();
    let core_forwarder = tokio::spawn(async move {
        while let Some((done, total)) = core_rx.recv().await {
            let progress = 0.12 + progress_fraction(done, total) * 0.18;
            send_status(&status_for_core, format!("Core files {done}/{total}"), progress);
        }
    });
    if let Some(control_rx) = &control_rx {
        download_jobs_checked_with_progress_and_control(jobs, Some(core_tx), control_rx.clone()).await?;
    } else {
        download_jobs_checked_with_progress(jobs, Some(core_tx)).await?;
    }
    let _ = core_forwarder.await;

    send_status(&status_tx, "Reading asset index", 0.32);
    let asset_index: AssetIndex = read_json(&asset_index_path).await?;
    let asset_jobs = asset_index
        .objects
        .into_iter()
        .map(|(name, object)| {
            let prefix = object
                .hash
                .get(0..2)
                .ok_or_else(|| AppError::Download(format!("asset {name} has invalid hash")))?;
            Ok(DownloadJob {
                id: format!("asset:{name}"),
                url: format!("https://resources.download.minecraft.net/{}/{}", prefix, object.hash),
                destination_path: assets_dir.join("objects").join(prefix).join(&object.hash),
                expected_sha1: Some(object.hash),
                size_bytes: object.size,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;

    let asset_total = asset_jobs.len();
    send_status(&status_tx, format!("Downloading and verifying {asset_total} assets"), 0.35);
    let (asset_tx, mut asset_rx) = mpsc::unbounded_channel();
    let status_for_assets = status_tx.clone();
    let asset_forwarder = tokio::spawn(async move {
        while let Some((done, total)) = asset_rx.recv().await {
            let progress = 0.35 + progress_fraction(done, total) * 0.60;
            send_status(&status_for_assets, format!("Assets {done}/{total}"), progress);
        }
    });
    if let Some(control_rx) = &control_rx {
        download_jobs_checked_with_progress_and_control(asset_jobs, Some(asset_tx), control_rx.clone()).await?;
    } else {
        download_jobs_checked_with_progress(asset_jobs, Some(asset_tx)).await?;
    }
    let _ = asset_forwarder.await;
    send_status(&status_tx, "Writing install marker", 0.97);
    write_install_marker(&versions_dir, version_id).await
}

fn send_status(status_tx: &Option<mpsc::UnboundedSender<InstallProgress>>, status: impl Into<String>, progress: f32) {
    if let Some(tx) = status_tx {
        let _ = tx.send(InstallProgress {
            status: status.into(),
            progress: progress.clamp(0.0, 1.0),
        });
    }
}

fn progress_fraction(done: usize, total: usize) -> f32 {
    if total == 0 {
        1.0
    } else {
        done as f32 / total as f32
    }
}

async fn version_url(client: &reqwest::Client, version_id: &str) -> Result<String, AppError> {
    let manifest: VersionManifest = get_json(client, VERSION_MANIFEST_URL).await?;
    manifest
        .versions
        .into_iter()
        .find(|version| version.id == version_id)
        .map(|version| version.url)
        .ok_or_else(|| AppError::Download(format!("Minecraft version {version_id} not found in Mojang manifest")))
}

async fn get_json<T: for<'de> Deserialize<'de>>(client: &reqwest::Client, url: &str) -> Result<T, AppError> {
    Ok(client.get(url).send().await?.error_for_status()?.json().await?)
}

async fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, AppError> {
    let bytes = tokio::fs::read(path).await?;
    Ok(serde_json::from_slice(&bytes)?)
}

async fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let bytes = serde_json::to_vec_pretty(value)?;
    tokio::fs::write(path, bytes).await?;
    Ok(())
}

async fn write_install_marker(version_dir: &Path, version_id: &str) -> Result<(), AppError> {
    let marker = serde_json::json!({
        "version": version_id,
        "installedAtUnix": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| AppError::Download(error.to_string()))?
            .as_secs()
    });
    write_json(&version_dir.join("swift-installed.json"), &marker).await
}

async fn install_marker_valid(version_dir: &Path, version_id: &str) -> bool {
    let marker_path = version_dir.join("swift-installed.json");
    let json_path = version_dir.join(format!("{version_id}.json"));
    let jar_path = version_dir.join(format!("{version_id}.jar"));
    if tokio::fs::metadata(marker_path).await.is_err() || tokio::fs::metadata(&jar_path).await.is_err() {
        return false;
    }
    let Ok(value) = read_json::<serde_json::Value>(&json_path).await else {
        return false;
    };
    value
        .get("mainClass")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|main_class| !main_class.trim().is_empty())
}

fn library_allowed_on_current_os(library: &Library) -> bool {
    let Some(rules) = &library.rules else {
        return true;
    };

    let mut allowed = false;
    for rule in rules {
        if rule_matches_current_os(rule) {
            allowed = rule.action == "allow";
        }
    }
    allowed
}

fn rule_matches_current_os(rule: &Rule) -> bool {
    let Some(os) = &rule.os else {
        return true;
    };
    let Some(name) = &os.name else {
        return true;
    };
    name == current_os_name()
}

fn current_os_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "windows"
    }
    #[cfg(target_os = "macos")]
    {
        "osx"
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        "linux"
    }
}

fn native_classifier_for_current_os(library: &Library) -> Option<String> {
    let classifier = library.natives.as_ref()?.get(current_os_name())?;
    Some(classifier.replace("${arch}", current_arch_bits()))
}

fn current_arch_bits() -> &'static str {
    if std::mem::size_of::<usize>() == 8 {
        "64"
    } else {
        "32"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::data_dir;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_version_dir() -> std::path::PathBuf {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let pid = std::process::id();
        data_dir()
            .unwrap()
            .join("test-versions")
            .join(format!("test-{pid}-{now}"))
    }

    #[tokio::test]
    async fn install_marker_validates_minimum_files() {
        let version_id = "1.99.0";
        let version_dir = temp_version_dir();
        tokio::fs::create_dir_all(&version_dir).await.unwrap();

        let json_path = version_dir.join(format!("{version_id}.json"));
        let jar_path = version_dir.join(format!("{version_id}.jar"));
        tokio::fs::write(&json_path, br#"{"mainClass":"net.minecraft.client.main.Main"}"#)
            .await
            .unwrap();
        tokio::fs::write(&jar_path, b"jar").await.unwrap();

        write_install_marker(&version_dir, version_id).await.unwrap();

        assert!(install_marker_valid(&version_dir, version_id).await);
    }
}
