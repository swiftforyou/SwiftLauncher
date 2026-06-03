use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::download::{download_jobs_checked, DownloadJob};
use crate::error::AppError;
use crate::instances::{instance_root, Instance, InstanceRunState, LoaderKind};

const MANIFEST: &str = "swift-instance.json";
const FILES_PREFIX: &str = "files/";
const CURSEFORGE_API_BASE: &str = "https://api.curseforge.com/v1";

#[derive(Debug, Clone, Default)]
pub struct ImportOptions {
    pub curseforge_api_key: String,
}

pub async fn export_instance(instance: Instance, destination: PathBuf) -> Result<String, AppError> {
    tokio::task::spawn_blocking(move || export_blocking(&instance, &destination))
        .await
        .map_err(|error| AppError::Instance(error.to_string()))?
}

pub async fn import_instance(
    archive_path: PathBuf,
    options: ImportOptions,
) -> Result<Instance, AppError> {
    let detect_path = archive_path.clone();
    let kind = tokio::task::spawn_blocking(move || detect_import_kind(&detect_path))
        .await
        .map_err(|error| AppError::Instance(error.to_string()))??;

    match kind {
        ImportKind::Swift => {
            tokio::task::spawn_blocking(move || import_swift_blocking(&archive_path))
                .await
                .map_err(|error| AppError::Instance(error.to_string()))?
        }
        ImportKind::Modrinth => import_mrpack(archive_path).await,
        ImportKind::Prism => {
            tokio::task::spawn_blocking(move || import_prism_blocking(&archive_path))
                .await
                .map_err(|error| AppError::Instance(error.to_string()))?
        }
        ImportKind::CurseForge => import_curseforge_zip(archive_path, options).await,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImportKind {
    Swift,
    Modrinth,
    Prism,
    CurseForge,
}

fn export_blocking(instance: &Instance, destination: &Path) -> Result<String, AppError> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| AppError::Storage(error.to_string()))?;
    }
    let file = File::create(destination).map_err(|error| AppError::Storage(error.to_string()))?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);

    zip.start_file(MANIFEST, options)
        .map_err(|error| AppError::Storage(error.to_string()))?;
    let mut manifest = instance.clone();
    manifest.run_state = InstanceRunState::Idle;
    zip.write_all(&serde_json::to_vec_pretty(&manifest)?)
        .map_err(|error| AppError::Storage(error.to_string()))?;

    add_dir(&mut zip, &instance.path, &instance.path, options)?;
    zip.finish()
        .map_err(|error| AppError::Storage(error.to_string()))?;
    Ok(destination.display().to_string())
}

fn add_dir(
    zip: &mut ZipWriter<File>,
    root: &Path,
    dir: &Path,
    options: SimpleFileOptions,
) -> Result<(), AppError> {
    for entry in fs::read_dir(dir).map_err(|error| AppError::Storage(error.to_string()))? {
        let entry = entry.map_err(|error| AppError::Storage(error.to_string()))?;
        let path = entry.path();
        let metadata =
            fs::symlink_metadata(&path).map_err(|error| AppError::Storage(error.to_string()))?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            add_dir(zip, root, &path, options)?;
            continue;
        }
        if !metadata.is_file() {
            continue;
        }

        let relative = path
            .strip_prefix(root)
            .map_err(|error| AppError::Storage(error.to_string()))?;
        let zip_name = format!("{FILES_PREFIX}{}", path_to_zip_name(relative)?);
        zip.start_file(zip_name, options)
            .map_err(|error| AppError::Storage(error.to_string()))?;
        let mut input = File::open(&path).map_err(|error| AppError::Storage(error.to_string()))?;
        std::io::copy(&mut input, zip).map_err(|error| AppError::Storage(error.to_string()))?;
    }
    Ok(())
}

fn detect_import_kind(archive_path: &Path) -> Result<ImportKind, AppError> {
    let file = File::open(archive_path).map_err(|error| AppError::Storage(error.to_string()))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| AppError::Storage(error.to_string()))?;
    if archive.by_name(MANIFEST).is_ok() {
        return Ok(ImportKind::Swift);
    }
    if archive.by_name("modrinth.index.json").is_ok() {
        return Ok(ImportKind::Modrinth);
    }
    let names = archive_names(&mut archive)?;
    if names.iter().any(|name| {
        name == "instance.cfg"
            || name.ends_with("/instance.cfg")
            || name == "mmc-pack.json"
            || name.ends_with("/mmc-pack.json")
    }) {
        return Ok(ImportKind::Prism);
    }
    if archive.by_name("manifest.json").is_ok()
        || names.iter().any(|name| name.ends_with("/manifest.json"))
    {
        return Ok(ImportKind::CurseForge);
    }
    Err(AppError::Instance(
        "unsupported import archive: expected Swift zip, .mrpack, Prism/MultiMC zip, or CurseForge zip".into(),
    ))
}

fn import_swift_blocking(archive_path: &Path) -> Result<Instance, AppError> {
    let file = File::open(archive_path).map_err(|error| AppError::Storage(error.to_string()))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| AppError::Storage(error.to_string()))?;
    let mut manifest = String::new();
    archive
        .by_name(MANIFEST)
        .map_err(|error| AppError::Storage(error.to_string()))?
        .read_to_string(&mut manifest)
        .map_err(|error| AppError::Storage(error.to_string()))?;
    let mut instance: Instance = serde_json::from_str(&manifest)?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| AppError::Storage(error.to_string()))?
        .as_secs();
    let safe = sanitize_name(&instance.name);
    instance.id = format!("{safe}-imported-{now}");
    instance.path = instance_root()?.join(&instance.id);
    instance.run_state = InstanceRunState::Idle;
    fs::create_dir_all(&instance.path).map_err(|error| AppError::Storage(error.to_string()))?;

    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|error| AppError::Storage(error.to_string()))?;
        let Some(name) = file.enclosed_name() else {
            continue;
        };
        let name = path_to_zip_name(&name)?;
        if !name.starts_with(FILES_PREFIX) || name == FILES_PREFIX {
            continue;
        }
        let relative = &name[FILES_PREFIX.len()..];
        let output_path = instance.path.join(relative);
        if file.is_dir() {
            fs::create_dir_all(output_path)
                .map_err(|error| AppError::Storage(error.to_string()))?;
        } else {
            if let Some(parent) = output_path.parent() {
                fs::create_dir_all(parent).map_err(|error| AppError::Storage(error.to_string()))?;
            }
            let mut output =
                File::create(output_path).map_err(|error| AppError::Storage(error.to_string()))?;
            std::io::copy(&mut file, &mut output)
                .map_err(|error| AppError::Storage(error.to_string()))?;
        }
    }

    Ok(instance)
}

async fn import_mrpack(archive_path: PathBuf) -> Result<Instance, AppError> {
    let index_path = archive_path.clone();
    let index = tokio::task::spawn_blocking(move || read_mrpack_index_blocking(&index_path))
        .await
        .map_err(|error| AppError::Instance(error.to_string()))??;
    let mut instance = new_instance(
        index.name.as_deref().unwrap_or("Modrinth Pack"),
        index
            .dependencies
            .get("minecraft")
            .map(String::as_str)
            .unwrap_or("latest"),
        loader_from_mrpack(&index),
        loader_version_from_mrpack(&index),
    )?;
    create_instance_dirs(&instance.path)?;

    let jobs = index
        .files
        .into_iter()
        .filter(|file| file.env.client != Some(SideSupport::Unsupported))
        .map(|file| {
            let url = file.downloads.into_iter().next().ok_or_else(|| {
                AppError::Download(format!("mrpack file {} has no download URL", file.path))
            })?;
            Ok(DownloadJob {
                id: format!("mrpack:{}", file.path),
                url,
                destination_path: safe_child(&instance.path, &file.path)?,
                expected_sha1: file.hashes.sha1,
                size_bytes: file.file_size,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    download_jobs_checked(jobs).await?;

    let overrides_path = archive_path.clone();
    let instance_path = instance.path.clone();
    tokio::task::spawn_blocking(move || {
        extract_mrpack_overrides_blocking(&overrides_path, &instance_path)
    })
    .await
    .map_err(|error| AppError::Instance(error.to_string()))??;

    instance.run_state = InstanceRunState::Idle;
    Ok(instance)
}

async fn import_curseforge_zip(
    archive_path: PathBuf,
    options: ImportOptions,
) -> Result<Instance, AppError> {
    let manifest_path = archive_path.clone();
    let (manifest, manifest_zip_name) =
        tokio::task::spawn_blocking(move || read_curseforge_manifest_blocking(&manifest_path))
            .await
            .map_err(|error| AppError::Instance(error.to_string()))??;
    let (loader, loader_version) = loader_from_curseforge(&manifest);
    let mut instance = new_instance(
        manifest.name.as_deref().unwrap_or("CurseForge Pack"),
        &manifest.minecraft.version,
        loader,
        loader_version,
    )?;
    create_instance_dirs(&instance.path)?;

    let required_files = manifest
        .files
        .iter()
        .filter(|file| file.required)
        .collect::<Vec<_>>();
    if !required_files.is_empty() && options.curseforge_api_key.trim().is_empty() {
        return Err(AppError::Instance(
            "CurseForge import needs API key. Paste it in Settings > Integrations.".into(),
        ));
    }

    let client = reqwest::Client::new();
    let mut jobs = Vec::new();
    for file in required_files {
        let resolved = resolve_curseforge_file(
            &client,
            options.curseforge_api_key.trim(),
            file.project_id,
            file.file_id,
        )
        .await?;
        let file_name = resolved
            .file_name
            .filter(|value| !value.trim().is_empty())
            .or(resolved.display_name)
            .ok_or_else(|| {
                AppError::Download(format!(
                    "CurseForge file {}/{} has no filename",
                    file.project_id, file.file_id
                ))
            })?;
        let download_url = match resolved.download_url.filter(|url| !url.trim().is_empty()) {
            Some(url) => url,
            None => {
                fetch_curseforge_download_url(
                    &client,
                    options.curseforge_api_key.trim(),
                    file.project_id,
                    file.file_id,
                )
                .await?
            }
        };
        let sha1 = resolved
            .hashes
            .iter()
            .find(|hash| hash.algo == 1)
            .map(|hash| hash.value.clone());
        jobs.push(DownloadJob {
            id: format!("curseforge:{}:{}", file.project_id, file.file_id),
            url: download_url,
            destination_path: safe_child(&instance.path, &format!("mods/{file_name}"))?,
            expected_sha1: sha1,
            size_bytes: resolved.file_length,
        });
    }
    download_jobs_checked(jobs).await?;

    let overrides = manifest.overrides.unwrap_or_else(|| "overrides".into());
    let archive_for_overrides = archive_path.clone();
    let instance_path = instance.path.clone();
    tokio::task::spawn_blocking(move || {
        extract_curseforge_overrides_blocking(
            &archive_for_overrides,
            &instance_path,
            &manifest_zip_name,
            &overrides,
        )
    })
    .await
    .map_err(|error| AppError::Instance(error.to_string()))??;

    instance.run_state = InstanceRunState::Idle;
    Ok(instance)
}

fn import_prism_blocking(archive_path: &Path) -> Result<Instance, AppError> {
    let file = File::open(archive_path).map_err(|error| AppError::Storage(error.to_string()))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| AppError::Storage(error.to_string()))?;
    let names = archive_names(&mut archive)?;
    let instance_cfg = find_archive_name(&names, "instance.cfg")
        .ok_or_else(|| AppError::Storage("Prism archive missing instance.cfg".into()))?;
    let prefix = archive_parent_prefix(&instance_cfg);
    let cfg_text = read_archive_text(&mut archive, &instance_cfg)?;
    let mmc_pack_name = format!("{prefix}mmc-pack.json");
    let pack = read_archive_text(&mut archive, &mmc_pack_name)
        .ok()
        .and_then(|text| serde_json::from_str::<PrismPack>(&text).ok());

    let name = cfg_value(&cfg_text, "name")
        .or_else(|| archive_path.file_stem().and_then(|value| value.to_str()))
        .unwrap_or("Prism Instance");
    let minecraft_version = pack
        .as_ref()
        .and_then(prism_minecraft_version)
        .or_else(|| cfg_value(&cfg_text, "IntendedVersion").map(str::to_string))
        .unwrap_or_else(|| "latest".into());
    let (loader, loader_version) = pack
        .as_ref()
        .map(prism_loader)
        .unwrap_or((LoaderKind::Vanilla, None));
    let instance = new_instance(name, &minecraft_version, loader, loader_version)?;
    create_instance_dirs(&instance.path)?;

    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|error| AppError::Storage(error.to_string()))?;
        let Some(enclosed) = file.enclosed_name() else {
            continue;
        };
        let name = path_to_zip_name(&enclosed)?;
        if !name.starts_with(&prefix) {
            continue;
        }
        let mut relative = name[prefix.len()..].to_string();
        if relative == "instance.cfg" || relative == "mmc-pack.json" || relative.is_empty() {
            continue;
        }
        if let Some(stripped) = relative
            .strip_prefix(".minecraft/")
            .or_else(|| relative.strip_prefix("minecraft/"))
        {
            relative = stripped.to_string();
        }
        if relative.is_empty() {
            continue;
        }
        let output_path = safe_child(&instance.path, &relative)?;
        if file.is_dir() || name.ends_with('/') {
            fs::create_dir_all(output_path)
                .map_err(|error| AppError::Storage(error.to_string()))?;
        } else {
            if let Some(parent) = output_path.parent() {
                fs::create_dir_all(parent).map_err(|error| AppError::Storage(error.to_string()))?;
            }
            let mut output =
                File::create(output_path).map_err(|error| AppError::Storage(error.to_string()))?;
            std::io::copy(&mut file, &mut output)
                .map_err(|error| AppError::Storage(error.to_string()))?;
        }
    }
    Ok(instance)
}

fn path_to_zip_name(path: &Path) -> Result<String, AppError> {
    let mut parts = Vec::new();
    for component in path.components() {
        let std::path::Component::Normal(part) = component else {
            return Err(AppError::Storage("invalid archive path".into()));
        };
        let Some(part) = part.to_str() else {
            return Err(AppError::Storage("archive path must be utf-8".into()));
        };
        parts.push(part);
    }
    Ok(parts.join("/"))
}

fn archive_names(archive: &mut ZipArchive<File>) -> Result<Vec<String>, AppError> {
    let mut names = Vec::new();
    for index in 0..archive.len() {
        let file = archive
            .by_index(index)
            .map_err(|error| AppError::Storage(error.to_string()))?;
        names.push(file.name().replace('\\', "/"));
    }
    Ok(names)
}

fn find_archive_name(names: &[String], suffix: &str) -> Option<String> {
    names
        .iter()
        .find(|name| name.as_str() == suffix || name.ends_with(&format!("/{suffix}")))
        .cloned()
}

fn archive_parent_prefix(name: &str) -> String {
    name.rsplit_once('/')
        .map(|(prefix, _)| format!("{prefix}/"))
        .unwrap_or_default()
}

fn read_archive_text(archive: &mut ZipArchive<File>, name: &str) -> Result<String, AppError> {
    let mut text = String::new();
    archive
        .by_name(name)
        .map_err(|error| AppError::Storage(error.to_string()))?
        .read_to_string(&mut text)
        .map_err(|error| AppError::Storage(error.to_string()))?;
    Ok(text)
}

fn cfg_value<'a>(input: &'a str, key: &str) -> Option<&'a str> {
    input.lines().find_map(|line| {
        let (left, right) = line.split_once('=')?;
        (left.trim() == key).then(|| right.trim())
    })
}

fn new_instance(
    name: &str,
    minecraft_version: &str,
    loader: LoaderKind,
    loader_version: Option<String>,
) -> Result<Instance, AppError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| AppError::Storage(error.to_string()))?
        .as_secs();
    let safe = sanitize_name(name);
    let id = format!("{safe}-imported-{now}");
    Ok(Instance {
        id: id.clone(),
        name: name.to_string(),
        minecraft_version: minecraft_version.to_string(),
        loader,
        loader_version,
        path: instance_root()?.join(&id),
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

fn create_instance_dirs(path: &Path) -> Result<(), AppError> {
    fs::create_dir_all(path).map_err(|error| AppError::Storage(error.to_string()))?;
    for dir in [
        "mods",
        "logs",
        "screenshots",
        "resourcepacks",
        "shaderpacks",
    ] {
        fs::create_dir_all(path.join(dir)).map_err(|error| AppError::Storage(error.to_string()))?;
    }
    Ok(())
}

fn safe_child(root: &Path, relative: &str) -> Result<PathBuf, AppError> {
    let relative = relative.replace('\\', "/");
    let mut out = PathBuf::new();
    for component in Path::new(&relative).components() {
        match component {
            std::path::Component::Normal(part) => out.push(part),
            std::path::Component::CurDir => {}
            _ => {
                return Err(AppError::Storage(format!(
                    "unsafe archive path: {relative}"
                )))
            }
        }
    }
    if out.as_os_str().is_empty() {
        return Err(AppError::Storage("empty archive path".into()));
    }
    Ok(root.join(out))
}

#[derive(Debug, Deserialize)]
struct MrpackIndex {
    name: Option<String>,
    #[serde(default)]
    dependencies: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    files: Vec<MrpackFile>,
}

#[derive(Debug, Deserialize)]
struct MrpackFile {
    path: String,
    hashes: MrpackHashes,
    #[serde(default)]
    env: MrpackEnv,
    downloads: Vec<String>,
    #[serde(default, rename = "fileSize")]
    file_size: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct MrpackHashes {
    sha1: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct MrpackEnv {
    client: Option<SideSupport>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum SideSupport {
    Required,
    Optional,
    Unsupported,
}

fn read_mrpack_index_blocking(pack_path: &Path) -> Result<MrpackIndex, AppError> {
    let file = File::open(pack_path).map_err(|error| AppError::Storage(error.to_string()))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| AppError::Storage(error.to_string()))?;
    let mut text = String::new();
    archive
        .by_name("modrinth.index.json")
        .map_err(|error| AppError::Storage(error.to_string()))?
        .read_to_string(&mut text)
        .map_err(|error| AppError::Storage(error.to_string()))?;
    Ok(serde_json::from_str(&text)?)
}

fn read_curseforge_manifest_blocking(
    pack_path: &Path,
) -> Result<(CurseForgeManifest, String), AppError> {
    let file = File::open(pack_path).map_err(|error| AppError::Storage(error.to_string()))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| AppError::Storage(error.to_string()))?;
    let names = archive_names(&mut archive)?;
    let manifest_name = find_archive_name(&names, "manifest.json")
        .ok_or_else(|| AppError::Storage("CurseForge archive missing manifest.json".into()))?;
    let text = read_archive_text(&mut archive, &manifest_name)?;
    Ok((serde_json::from_str(&text)?, manifest_name))
}

async fn resolve_curseforge_file(
    client: &reqwest::Client,
    api_key: &str,
    project_id: u64,
    file_id: u64,
) -> Result<CurseForgeApiFile, AppError> {
    let url = format!(
        "{}/mods/{}/files/{}",
        curseforge_api_base(),
        project_id,
        file_id
    );
    let response = client
        .get(url)
        .header("x-api-key", api_key)
        .send()
        .await
        .map_err(|error| AppError::Network(error.to_string()))?
        .error_for_status()
        .map_err(|error| AppError::Network(error.to_string()))?;
    let envelope = response
        .json::<CurseForgeEnvelope<CurseForgeApiFile>>()
        .await
        .map_err(|error| AppError::Network(error.to_string()))?;
    Ok(envelope.data)
}

async fn fetch_curseforge_download_url(
    client: &reqwest::Client,
    api_key: &str,
    project_id: u64,
    file_id: u64,
) -> Result<String, AppError> {
    let url = format!(
        "{}/mods/{}/files/{}/download-url",
        curseforge_api_base(),
        project_id,
        file_id
    );
    let response = client
        .get(url)
        .header("x-api-key", api_key)
        .send()
        .await
        .map_err(|error| AppError::Network(error.to_string()))?
        .error_for_status()
        .map_err(|error| AppError::Network(error.to_string()))?;
    let envelope = response
        .json::<CurseForgeEnvelope<String>>()
        .await
        .map_err(|error| AppError::Network(error.to_string()))?;
    Ok(envelope.data)
}

fn curseforge_api_base() -> String {
    std::env::var("SWIFT_LAUNCHER_CURSEFORGE_API_BASE")
        .unwrap_or_else(|_| CURSEFORGE_API_BASE.into())
}

fn extract_mrpack_overrides_blocking(
    pack_path: &Path,
    instance_path: &Path,
) -> Result<(), AppError> {
    let file = File::open(pack_path).map_err(|error| AppError::Storage(error.to_string()))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| AppError::Storage(error.to_string()))?;
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|error| AppError::Storage(error.to_string()))?;
        let name = file.name().replace('\\', "/");
        let Some(relative) = name
            .strip_prefix("overrides/")
            .or_else(|| name.strip_prefix("client-overrides/"))
        else {
            continue;
        };
        if relative.is_empty() {
            continue;
        }
        let output_path = safe_child(instance_path, relative)?;
        if file.is_dir() || name.ends_with('/') {
            fs::create_dir_all(output_path)
                .map_err(|error| AppError::Storage(error.to_string()))?;
            continue;
        }
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).map_err(|error| AppError::Storage(error.to_string()))?;
        }
        let mut output =
            File::create(output_path).map_err(|error| AppError::Storage(error.to_string()))?;
        std::io::copy(&mut file, &mut output)
            .map_err(|error| AppError::Storage(error.to_string()))?;
    }
    Ok(())
}

fn extract_curseforge_overrides_blocking(
    pack_path: &Path,
    instance_path: &Path,
    manifest_zip_name: &str,
    overrides: &str,
) -> Result<(), AppError> {
    let file = File::open(pack_path).map_err(|error| AppError::Storage(error.to_string()))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| AppError::Storage(error.to_string()))?;
    let prefix = archive_parent_prefix(manifest_zip_name);
    let overrides_prefix = format!(
        "{}{}/",
        prefix,
        overrides.trim_matches('/').trim_matches('\\')
    );

    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|error| AppError::Storage(error.to_string()))?;
        let name = file.name().replace('\\', "/");
        let Some(relative) = name.strip_prefix(&overrides_prefix) else {
            continue;
        };
        if relative.is_empty() {
            continue;
        }
        let output_path = safe_child(instance_path, relative)?;
        if file.is_dir() || name.ends_with('/') {
            fs::create_dir_all(output_path)
                .map_err(|error| AppError::Storage(error.to_string()))?;
            continue;
        }
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).map_err(|error| AppError::Storage(error.to_string()))?;
        }
        let mut output =
            File::create(output_path).map_err(|error| AppError::Storage(error.to_string()))?;
        std::io::copy(&mut file, &mut output)
            .map_err(|error| AppError::Storage(error.to_string()))?;
    }
    Ok(())
}

fn loader_from_mrpack(index: &MrpackIndex) -> LoaderKind {
    if index.dependencies.contains_key("fabric-loader") {
        LoaderKind::Fabric
    } else if index.dependencies.contains_key("quilt-loader") {
        LoaderKind::Quilt
    } else if index.dependencies.contains_key("neoforge") {
        LoaderKind::NeoForge
    } else if index.dependencies.contains_key("forge") {
        LoaderKind::Forge
    } else {
        LoaderKind::Vanilla
    }
}

fn loader_version_from_mrpack(index: &MrpackIndex) -> Option<String> {
    for key in ["fabric-loader", "quilt-loader", "neoforge", "forge"] {
        if let Some(version) = index.dependencies.get(key) {
            return Some(version.clone());
        }
    }
    None
}

fn loader_from_curseforge(manifest: &CurseForgeManifest) -> (LoaderKind, Option<String>) {
    let Some(mod_loader) = manifest
        .minecraft
        .mod_loaders
        .iter()
        .find(|loader| loader.primary)
        .or_else(|| manifest.minecraft.mod_loaders.first())
    else {
        return (LoaderKind::Vanilla, None);
    };
    let id = mod_loader.id.as_str();
    for (prefix, kind) in [
        ("fabric-", LoaderKind::Fabric),
        ("quilt-", LoaderKind::Quilt),
        ("neoforge-", LoaderKind::NeoForge),
        ("neoForge-", LoaderKind::NeoForge),
        ("forge-", LoaderKind::Forge),
    ] {
        if let Some(version) = id.strip_prefix(prefix) {
            return (kind, Some(version.to_string()));
        }
    }
    (LoaderKind::Vanilla, None)
}

#[derive(Debug, Deserialize)]
struct CurseForgeManifest {
    name: Option<String>,
    minecraft: CurseForgeMinecraft,
    #[serde(default)]
    files: Vec<CurseForgeFileRef>,
    overrides: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CurseForgeMinecraft {
    version: String,
    #[serde(default, rename = "modLoaders")]
    mod_loaders: Vec<CurseForgeModLoader>,
}

#[derive(Debug, Deserialize)]
struct CurseForgeModLoader {
    id: String,
    #[serde(default)]
    primary: bool,
}

#[derive(Debug, Deserialize)]
struct CurseForgeFileRef {
    #[serde(rename = "projectID")]
    project_id: u64,
    #[serde(rename = "fileID")]
    file_id: u64,
    #[serde(default = "default_true")]
    required: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
struct CurseForgeEnvelope<T> {
    data: T,
}

#[derive(Debug, Deserialize)]
struct CurseForgeApiFile {
    #[serde(rename = "displayName")]
    display_name: Option<String>,
    #[serde(rename = "fileName")]
    file_name: Option<String>,
    #[serde(rename = "downloadUrl")]
    download_url: Option<String>,
    #[serde(rename = "fileLength")]
    file_length: Option<u64>,
    #[serde(default)]
    hashes: Vec<CurseForgeHash>,
}

#[derive(Debug, Deserialize)]
struct CurseForgeHash {
    value: String,
    algo: u32,
}

#[derive(Debug, Deserialize)]
struct PrismPack {
    #[serde(default)]
    components: Vec<PrismComponent>,
}

#[derive(Debug, Deserialize)]
struct PrismComponent {
    uid: String,
    version: Option<String>,
}

fn prism_minecraft_version(pack: &PrismPack) -> Option<String> {
    pack.components
        .iter()
        .find(|component| component.uid == "net.minecraft")
        .and_then(|component| component.version.clone())
}

fn prism_loader(pack: &PrismPack) -> (LoaderKind, Option<String>) {
    for component in &pack.components {
        let loader = match component.uid.as_str() {
            "net.fabricmc.fabric-loader" => LoaderKind::Fabric,
            "org.quiltmc.quilt-loader" => LoaderKind::Quilt,
            "net.minecraftforge" => LoaderKind::Forge,
            "net.neoforged" => LoaderKind::NeoForge,
            _ => continue,
        };
        return (loader, component.version.clone());
    }
    (LoaderKind::Vanilla, None)
}

fn sanitize_name(input: &str) -> String {
    let out = input
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
        .collect::<String>();
    if out.is_empty() {
        "instance".into()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instances::{Instance, InstanceRunState, LoaderKind};
    use crate::storage::data_dir;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_instance(name: &str) -> Instance {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let id = format!("{name}-{now}");
        let root = data_dir().unwrap().join("test-instances");
        let path = root.join(&id);
        Instance {
            id,
            name: name.to_string(),
            minecraft_version: "1.20.1".into(),
            loader: LoaderKind::Vanilla,
            loader_version: None,
            path,
            artwork_path: None,
            last_played_unix: None,
            playtime_seconds: 0,
            ram_mb: 2048,
            java_path: "java".into(),
            jvm_args: String::new(),
            resolution_width: 1280,
            resolution_height: 720,
            fullscreen: false,
            game_dir_override: String::new(),
            server: String::new(),
            run_state: InstanceRunState::Idle,
        }
    }

    #[tokio::test]
    async fn export_then_import_roundtrip() {
        let instance = unique_instance("archive-test");
        tokio::fs::create_dir_all(instance.path.join("mods"))
            .await
            .unwrap();
        tokio::fs::write(instance.path.join("mods").join("mod.jar"), b"mod-data")
            .await
            .unwrap();

        let zip_path = instance.path.with_extension("zip");
        let exported = export_instance(instance.clone(), zip_path.clone())
            .await
            .unwrap();
        assert_eq!(exported, zip_path.display().to_string());

        let imported = import_instance(zip_path.clone(), ImportOptions::default())
            .await
            .unwrap();
        assert_ne!(imported.id, instance.id);
        assert_eq!(imported.name, instance.name);
        let imported_mod = imported.path.join("mods").join("mod.jar");
        let bytes = tokio::fs::read(imported_mod).await.unwrap();
        assert_eq!(bytes, b"mod-data");
    }

    #[tokio::test]
    async fn prism_zip_import_reads_metadata_and_files() {
        let temp = data_dir().unwrap().join("archive-prism-test");
        tokio::fs::create_dir_all(&temp).await.unwrap();
        let zip_path = temp.join("prism.zip");
        let file = File::create(&zip_path).unwrap();
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        zip.start_file("Prism/instance.cfg", options).unwrap();
        zip.write_all(b"name=Imported Prism\nIntendedVersion=1.20.1\n")
            .unwrap();
        zip.start_file("Prism/mmc-pack.json", options).unwrap();
        zip.write_all(
            br#"{
                "components": [
                    {"uid":"net.minecraft","version":"1.20.1"},
                    {"uid":"net.fabricmc.fabric-loader","version":"0.16.10"}
                ]
            }"#,
        )
        .unwrap();
        zip.start_file("Prism/.minecraft/mods/example.jar", options)
            .unwrap();
        zip.write_all(b"mod").unwrap();
        zip.finish().unwrap();

        let imported = import_instance(zip_path, ImportOptions::default())
            .await
            .unwrap();
        assert_eq!(imported.name, "Imported Prism");
        assert_eq!(imported.minecraft_version, "1.20.1");
        assert_eq!(imported.loader, LoaderKind::Fabric);
        assert_eq!(imported.loader_version.as_deref(), Some("0.16.10"));
        assert_eq!(
            tokio::fs::read(imported.path.join("mods/example.jar"))
                .await
                .unwrap(),
            b"mod"
        );
    }
}
