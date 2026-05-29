use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tokio::sync::mpsc;

use crate::download::{
    download_jobs_checked_with_progress, download_jobs_checked_with_progress_control_and_events,
    DownloadControl, DownloadEvent, DownloadJob,
};
use crate::error::AppError;
use crate::instances::LoaderKind;
use crate::storage::data_dir;

const VERSION_MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const FORGE_METADATA_URL: &str =
    "https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml";
const FORGE_MAVEN_BASE: &str = "https://maven.minecraftforge.net/net/minecraftforge/forge";
const NEOFORGE_METADATA_URL: &str =
    "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml";
const NEOFORGE_MAVEN_BASE: &str = "https://maven.neoforged.net/releases/net/neoforged/neoforge";

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
        LoaderKind::Forge => install_forge_like_profile(
            "Forge",
            minecraft_version,
            loader_version,
            FORGE_MAVEN_BASE,
            forge_installer_file,
            infer_forge_profile_id,
            status_tx,
        )
        .await
        .map(Some),
        LoaderKind::NeoForge => install_forge_like_profile(
            "NeoForge",
            minecraft_version,
            loader_version,
            NEOFORGE_MAVEN_BASE,
            neoforge_installer_file,
            infer_neoforge_profile_id,
            status_tx,
        )
        .await
        .map(Some),
    }
}

pub async fn install_minecraft_version_with_status_and_control(
    version_id: &str,
    status_tx: Option<mpsc::UnboundedSender<InstallProgress>>,
    control_rx: tokio::sync::watch::Receiver<DownloadControl>,
) -> Result<(), AppError> {
    install_minecraft_version_inner(version_id, status_tx, Some(control_rx)).await
}

pub async fn fetch_loader_versions(
    loader: LoaderKind,
    minecraft_version: &str,
) -> Result<Vec<String>, AppError> {
    match loader {
        LoaderKind::Vanilla => Ok(Vec::new()),
        LoaderKind::Fabric => {
            fetch_fabric_like_loader_versions(
                "https://meta.fabricmc.net/v2/versions/loader",
                minecraft_version,
            )
            .await
        }
        LoaderKind::Quilt => {
            fetch_fabric_like_loader_versions(
                "https://meta.quiltmc.org/v3/versions/loader",
                minecraft_version,
            )
            .await
        }
        LoaderKind::Forge => fetch_forge_versions(minecraft_version).await,
        LoaderKind::NeoForge => fetch_neoforge_versions(minecraft_version).await,
    }
}

async fn fetch_fabric_like_loader_versions(
    endpoint: &str,
    minecraft_version: &str,
) -> Result<Vec<String>, AppError> {
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
        return Err(AppError::Instance(format!(
            "No loader versions found for Minecraft {minecraft_version}"
        )));
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
    let loader_version = match selected_loader_version.filter(|version| !version.trim().is_empty())
    {
        Some(version) => version,
        None => fetch_fabric_like_loader_versions(endpoint, minecraft_version)
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| {
                AppError::Instance(format!(
                    "{label} has no loader for Minecraft {minecraft_version}"
                ))
            })?,
    };

    send_status(
        &status_tx,
        format!("Downloading {label} loader {loader_version} profile"),
        0.965,
    );
    let profile = client
        .get(format!(
            "{endpoint}/{minecraft_version}/{loader_version}/profile/json"
        ))
        .send()
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;
    let profile_id = json_string(&profile, "id")
        .ok_or_else(|| AppError::Instance(format!("{label} profile did not include id")))?;
    let libraries = serde_json::from_value::<Vec<Library>>(
        profile.get("libraries").cloned().unwrap_or_default(),
    )?;

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
    send_status(
        &status_tx,
        format!("Downloading {label} loader libraries"),
        0.975,
    );
    download_jobs_checked_with_progress(jobs, None).await?;
    Ok(profile_id)
}

async fn fetch_forge_versions(minecraft_version: &str) -> Result<Vec<String>, AppError> {
    let xml = reqwest::Client::new()
        .get(FORGE_METADATA_URL)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let prefix = format!("{minecraft_version}-");
    let versions = maven_versions(&xml)
        .into_iter()
        .filter(|version| version.starts_with(&prefix))
        .rev()
        .collect::<Vec<_>>();
    if versions.is_empty() {
        return Err(AppError::Instance(format!(
            "Forge has no loader versions for Minecraft {minecraft_version}"
        )));
    }
    Ok(versions)
}

async fn fetch_neoforge_versions(minecraft_version: &str) -> Result<Vec<String>, AppError> {
    let xml = reqwest::Client::new()
        .get(NEOFORGE_METADATA_URL)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let Some(prefix) = neoforge_version_prefix(minecraft_version) else {
        return Err(AppError::Instance(format!(
            "NeoForge version mapping is unknown for Minecraft {minecraft_version}"
        )));
    };
    let versions = maven_versions(&xml)
        .into_iter()
        .filter(|version| version.starts_with(&prefix))
        .rev()
        .collect::<Vec<_>>();
    if versions.is_empty() {
        return Err(AppError::Instance(format!(
            "NeoForge has no loader versions for Minecraft {minecraft_version}"
        )));
    }
    Ok(versions)
}

async fn install_forge_like_profile(
    label: &str,
    minecraft_version: &str,
    selected_loader_version: Option<String>,
    maven_base: &str,
    installer_file: fn(&str) -> String,
    infer_profile_id: fn(&str, &str) -> String,
    status_tx: Option<mpsc::UnboundedSender<InstallProgress>>,
) -> Result<String, AppError> {
    send_status(&status_tx, format!("Resolving {label} installer"), 0.96);
    let loader_version = match selected_loader_version.filter(|version| !version.trim().is_empty())
    {
        Some(version) => version,
        None => match label {
            "Forge" => fetch_forge_versions(minecraft_version).await?,
            _ => fetch_neoforge_versions(minecraft_version).await?,
        }
        .into_iter()
        .next()
        .ok_or_else(|| {
            AppError::Instance(format!(
                "{label} has no loader for Minecraft {minecraft_version}"
            ))
        })?,
    };

    let file_name = installer_file(&loader_version);
    let installer_url = format!("{maven_base}/{loader_version}/{file_name}");
    let installer_path = data_dir()?
        .join("installers")
        .join(label.to_ascii_lowercase())
        .join(&loader_version)
        .join(&file_name);

    let expected_sha1 = fetch_optional_text(&format!("{installer_url}.sha1"))
        .await
        .ok()
        .and_then(|value| value.split_whitespace().next().map(str::to_string));
    send_status(
        &status_tx,
        format!("Downloading {label} installer {loader_version}"),
        0.965,
    );
    download_jobs_checked_with_progress(
        vec![DownloadJob {
            id: format!("{}-installer:{loader_version}", label.to_ascii_lowercase()),
            url: installer_url,
            destination_path: installer_path.clone(),
            expected_sha1,
            size_bytes: None,
        }],
        None,
    )
    .await?;

    let profile_id = read_installer_profile_id(installer_path.clone())
        .await
        .unwrap_or_else(|_| infer_profile_id(minecraft_version, &loader_version));
    let version_json_path = data_dir()?
        .join("versions")
        .join(&profile_id)
        .join(format!("{profile_id}.json"));
    if tokio::fs::metadata(&version_json_path).await.is_ok() {
        send_status(
            &status_tx,
            format!("{label} profile {profile_id} already installed"),
            0.99,
        );
        return Ok(profile_id);
    }

    send_status(
        &status_tx,
        format!("Running {label} installer processors"),
        0.975,
    );
    run_loader_installer(label, minecraft_version, &installer_path).await?;
    if tokio::fs::metadata(&version_json_path).await.is_err() {
        return Err(AppError::Instance(format!(
            "{label} installer finished but profile {profile_id} was not written"
        )));
    }
    send_status(&status_tx, format!("{label} profile ready"), 0.99);
    Ok(profile_id)
}

async fn run_loader_installer(
    label: &str,
    minecraft_version: &str,
    installer_path: &Path,
) -> Result<(), AppError> {
    let root = data_dir()?;
    let java = crate::download::java::ensure_suitable_java(
        "java",
        crate::download::java::required_java_for_minecraft_version(minecraft_version),
    )
    .await?;
    let output = Command::new(&java.path)
        .arg("-jar")
        .arg(installer_path)
        .arg("--installClient")
        .arg(&root)
        .current_dir(&root)
        .output()
        .await?;
    if output.status.success() {
        return Ok(());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(AppError::Process(format!(
        "{label} installer failed: {}\n{}{}",
        output.status, stdout, stderr
    )))
}

async fn read_installer_profile_id(installer_path: PathBuf) -> Result<String, AppError> {
    tokio::task::spawn_blocking(move || {
        let file = std::fs::File::open(installer_path)
            .map_err(|error| AppError::Storage(error.to_string()))?;
        let mut zip =
            zip::ZipArchive::new(file).map_err(|error| AppError::Storage(error.to_string()))?;
        if let Ok(mut version_json) = zip.by_name("version.json") {
            let mut text = String::new();
            std::io::Read::read_to_string(&mut version_json, &mut text)
                .map_err(|error| AppError::Storage(error.to_string()))?;
            if let Some(id) = serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|value| {
                    value
                        .get("id")
                        .and_then(|id| id.as_str())
                        .map(str::to_string)
                })
            {
                return Ok(id);
            }
        }
        if let Ok(mut profile_json) = zip.by_name("install_profile.json") {
            let mut text = String::new();
            std::io::Read::read_to_string(&mut profile_json, &mut text)
                .map_err(|error| AppError::Storage(error.to_string()))?;
            if let Some(id) = serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|value| {
                    value
                        .get("versionInfo")
                        .and_then(|version| version.get("id"))
                        .and_then(|id| id.as_str())
                        .or_else(|| value.get("profile").and_then(|id| id.as_str()))
                        .map(str::to_string)
                })
            {
                return Ok(id);
            }
        }
        Err(AppError::Instance("installer profile id missing".into()))
    })
    .await
    .map_err(|error| AppError::Instance(error.to_string()))?
}

fn maven_versions(xml: &str) -> Vec<String> {
    let mut versions = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<version>") {
        rest = &rest[start + "<version>".len()..];
        let Some(end) = rest.find("</version>") else {
            break;
        };
        let version = rest[..end].trim();
        if !version.is_empty() {
            versions.push(version.to_string());
        }
        rest = &rest[end + "</version>".len()..];
    }
    versions
}

fn forge_installer_file(version: &str) -> String {
    format!("forge-{version}-installer.jar")
}

fn neoforge_installer_file(version: &str) -> String {
    format!("neoforge-{version}-installer.jar")
}

fn infer_forge_profile_id(minecraft_version: &str, loader_version: &str) -> String {
    let forge_build = loader_version
        .split_once('-')
        .map(|(_, build)| build)
        .unwrap_or(loader_version);
    format!("{minecraft_version}-forge-{forge_build}")
}

fn infer_neoforge_profile_id(_minecraft_version: &str, loader_version: &str) -> String {
    format!("neoforge-{loader_version}")
}

fn neoforge_version_prefix(minecraft_version: &str) -> Option<String> {
    let mut parts = minecraft_version.split('.');
    let major = parts.next()?.parse::<u32>().ok()?;
    let minor = parts.next()?.parse::<u32>().ok()?;
    let patch = parts.next().unwrap_or("0").parse::<u32>().ok()?;
    if major != 1 || minor < 20 {
        return None;
    }
    Some(format!("{minor}.{patch}."))
}

async fn fetch_optional_text(url: &str) -> Result<String, AppError> {
    Ok(reqwest::Client::new()
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?)
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
    send_status(
        &status_tx,
        format!("Resolving Minecraft {version_id}"),
        0.05,
    );
    let client = reqwest::Client::new();
    let root = data_dir()?;
    let versions_dir = root.join("versions").join(version_id);
    let libraries_dir = root.join("libraries");
    let assets_dir = root.join("assets");
    if install_marker_valid(&versions_dir, version_id).await {
        send_status(
            &status_tx,
            format!("Minecraft {version_id} already installed"),
            1.0,
        );
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
        if let Some(artifact) = library
            .downloads
            .as_ref()
            .and_then(|downloads| downloads.artifact.as_ref())
        {
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

    let asset_index_path = assets_dir
        .join("indexes")
        .join(format!("{}.json", version_json.asset_index.id));
    jobs.push(DownloadJob {
        id: format!("asset-index:{}", version_json.asset_index.id),
        url: version_json.asset_index.url.clone(),
        destination_path: asset_index_path.clone(),
        expected_sha1: version_json.asset_index.sha1.clone(),
        size_bytes: version_json.asset_index.size,
    });

    let core_total = jobs.len();
    send_status(
        &status_tx,
        format!("Downloading and verifying {core_total} core files"),
        0.12,
    );
    let (core_tx, mut core_rx) = mpsc::unbounded_channel();
    let (core_event_tx, mut core_event_rx) = mpsc::unbounded_channel();
    let status_for_core = status_tx.clone();
    let core_forwarder = tokio::spawn(async move {
        let mut done = 0usize;
        let total = core_total;
        let mut progress = 0.12;
        let mut speed = 0u64;
        loop {
            tokio::select! {
                Some((current, count)) = core_rx.recv() => {
                    done = current;
                    progress = 0.12 + progress_fraction(done, count) * 0.18;
                    send_status(
                        &status_for_core,
                        format!("Core files {done}/{count}"),
                        progress,
                    );
                }
                Some(event) = core_event_rx.recv() => {
                    if let Some(status) = download_event_status("Core", done, total, &event, &mut speed) {
                        send_status(&status_for_core, status, progress);
                    }
                }
                else => break,
            }
        }
    });
    if let Some(control_rx) = &control_rx {
        download_jobs_checked_with_progress_control_and_events(
            jobs,
            Some(core_tx),
            control_rx.clone(),
            Some(core_event_tx),
        )
        .await?;
    } else {
        let (control_tx, control_rx) = tokio::sync::watch::channel(DownloadControl::Run);
        download_jobs_checked_with_progress_control_and_events(
            jobs,
            Some(core_tx),
            control_rx,
            Some(core_event_tx),
        )
        .await?;
        drop(control_tx);
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
                url: format!(
                    "https://resources.download.minecraft.net/{}/{}",
                    prefix, object.hash
                ),
                destination_path: assets_dir.join("objects").join(prefix).join(&object.hash),
                expected_sha1: Some(object.hash),
                size_bytes: object.size,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;

    let asset_total = asset_jobs.len();
    send_status(
        &status_tx,
        format!("Downloading and verifying {asset_total} assets"),
        0.35,
    );
    let (asset_tx, mut asset_rx) = mpsc::unbounded_channel();
    let (asset_event_tx, mut asset_event_rx) = mpsc::unbounded_channel();
    let status_for_assets = status_tx.clone();
    let asset_forwarder = tokio::spawn(async move {
        let mut done = 0usize;
        let total = asset_total;
        let mut progress = 0.35;
        let mut speed = 0u64;
        loop {
            tokio::select! {
                Some((current, count)) = asset_rx.recv() => {
                    done = current;
                    progress = 0.35 + progress_fraction(done, count) * 0.60;
                    send_status(
                        &status_for_assets,
                        format!("Assets {done}/{count}"),
                        progress,
                    );
                }
                Some(event) = asset_event_rx.recv() => {
                    if let Some(status) = download_event_status("Assets", done, total, &event, &mut speed) {
                        send_status(&status_for_assets, status, progress);
                    }
                }
                else => break,
            }
        }
    });
    if let Some(control_rx) = &control_rx {
        download_jobs_checked_with_progress_control_and_events(
            asset_jobs,
            Some(asset_tx),
            control_rx.clone(),
            Some(asset_event_tx),
        )
        .await?;
    } else {
        let (control_tx, control_rx) = tokio::sync::watch::channel(DownloadControl::Run);
        download_jobs_checked_with_progress_control_and_events(
            asset_jobs,
            Some(asset_tx),
            control_rx,
            Some(asset_event_tx),
        )
        .await?;
        drop(control_tx);
    }
    let _ = asset_forwarder.await;
    send_status(&status_tx, "Writing install marker", 0.97);
    write_install_marker(&versions_dir, version_id).await
}

fn send_status(
    status_tx: &Option<mpsc::UnboundedSender<InstallProgress>>,
    status: impl Into<String>,
    progress: f32,
) {
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

fn download_event_status(
    phase: &str,
    done: usize,
    total: usize,
    event: &DownloadEvent,
    speed: &mut u64,
) -> Option<String> {
    match event {
        DownloadEvent::Speed { bytes_per_second } => {
            *speed = *bytes_per_second;
            Some(format!("{phase} {done}/{total} @ {}", format_speed(*speed)))
        }
        DownloadEvent::Progress(progress) => {
            let name = compact_job_name(&progress.job_id);
            let bytes = match progress.total_bytes {
                Some(total_bytes) => format!(
                    "{}/{}",
                    format_bytes(progress.downloaded_bytes),
                    format_bytes(total_bytes)
                ),
                None => format_bytes(progress.downloaded_bytes),
            };
            let suffix = if *speed > 0 {
                format!(" @ {}", format_speed(*speed))
            } else {
                String::new()
            };
            Some(format!("{phase} {done}/{total}: {name} {bytes}{suffix}"))
        }
        DownloadEvent::Complete { job_id } => Some(format!(
            "{phase} {done}/{total}: {} complete",
            compact_job_name(job_id)
        )),
        DownloadEvent::Failed { job_id, reason } => Some(format!(
            "{phase} {done}/{total}: {} failed: {reason}",
            compact_job_name(job_id)
        )),
    }
}

fn compact_job_name(job_id: &str) -> String {
    let raw = job_id.rsplit(':').next().unwrap_or(job_id);
    let short = raw.rsplit('/').next().unwrap_or(raw);
    if short.len() > 54 {
        format!("...{}", &short[short.len().saturating_sub(51)..])
    } else {
        short.to_string()
    }
}

fn format_speed(bytes_per_second: u64) -> String {
    format!("{}/s", format_bytes(bytes_per_second))
}

fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let bytes = bytes as f64;
    if bytes >= GB {
        format!("{:.1} GB", bytes / GB)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes / MB)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes / KB)
    } else {
        format!("{bytes:.0} B")
    }
}

async fn version_url(client: &reqwest::Client, version_id: &str) -> Result<String, AppError> {
    let manifest: VersionManifest = get_json(client, VERSION_MANIFEST_URL).await?;
    manifest
        .versions
        .into_iter()
        .find(|version| version.id == version_id)
        .map(|version| version.url)
        .ok_or_else(|| {
            AppError::Download(format!(
                "Minecraft version {version_id} not found in Mojang manifest"
            ))
        })
}

async fn get_json<T: for<'de> Deserialize<'de>>(
    client: &reqwest::Client,
    url: &str,
) -> Result<T, AppError> {
    Ok(client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
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
    if tokio::fs::metadata(marker_path).await.is_err()
        || tokio::fs::metadata(&jar_path).await.is_err()
    {
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
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
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
        tokio::fs::write(
            &json_path,
            br#"{"mainClass":"net.minecraft.client.main.Main"}"#,
        )
        .await
        .unwrap();
        tokio::fs::write(&jar_path, b"jar").await.unwrap();

        write_install_marker(&version_dir, version_id)
            .await
            .unwrap();

        assert!(install_marker_valid(&version_dir, version_id).await);
    }
}
