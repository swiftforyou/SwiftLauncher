use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::download::{
    download_jobs_checked_with_progress_control_and_events, DownloadControl, DownloadEvent,
    DownloadJob,
};
use crate::error::AppError;
use crate::instances::install::InstallProgress;
use crate::instances::LoaderKind;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResourceProvider {
    Modrinth,
    CurseForge,
}

impl ResourceProvider {
    pub const ALL: [Self; 2] = [Self::Modrinth, Self::CurseForge];
}

impl std::fmt::Display for ResourceProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Modrinth => f.write_str("Modrinth"),
            Self::CurseForge => f.write_str("CurseForge"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModrinthKind {
    Mods,
    Modpacks,
    ResourcePacks,
    Shaders,
}

impl ModrinthKind {
    pub const ALL: [Self; 4] = [
        Self::Mods,
        Self::Modpacks,
        Self::ResourcePacks,
        Self::Shaders,
    ];

    fn project_type(self) -> &'static str {
        match self {
            Self::Mods => "mod",
            Self::Modpacks => "modpack",
            Self::ResourcePacks => "resourcepack",
            Self::Shaders => "shader",
        }
    }
}

impl std::fmt::Display for ModrinthKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Mods => f.write_str("Mods"),
            Self::Modpacks => f.write_str("Modpacks"),
            Self::ResourcePacks => f.write_str("Resource Packs"),
            Self::Shaders => f.write_str("Shaders"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledMod {
    pub id: String,
    pub name: String,
    pub version: String,
    pub enabled: bool,
    pub category: String,
    pub icon: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModrinthProject {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub author: String,
    pub description: String,
    pub downloads: u64,
    pub icon: Option<Vec<u8>>,
    pub categories: Vec<String>,
    pub loaders: Vec<String>,
    pub client_side: Option<String>,
    pub server_side: Option<String>,
    pub kind: ModrinthKind,
    pub provider: ResourceProvider,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModrinthProjectDetail {
    pub project_id: String,
    pub title: String,
    pub description: String,
    pub body: String,
    pub downloads: u64,
    pub icon: Option<Vec<u8>>,
    pub gallery: Vec<Vec<u8>>,
    pub kind: ModrinthKind,
    pub provider: ResourceProvider,
}

pub async fn list_mods(instance_path: &Path) -> Result<Vec<InstalledMod>, AppError> {
    let metadata = load_metadata(instance_path).await.unwrap_or_default();
    let mut mods = Vec::new();
    scan_installed_dir(instance_path, "mods", "Installed", &["jar"], &metadata, &mut mods).await?;
    scan_installed_dir(
        instance_path,
        "resourcepacks",
        "Resource Packs",
        &["zip"],
        &metadata,
        &mut mods,
    )
    .await?;
    scan_installed_dir(
        instance_path,
        "shaderpacks",
        "Shaders",
        &["zip"],
        &metadata,
        &mut mods,
    )
    .await?;
    scan_installed_dir(
        instance_path,
        "modpacks",
        "Modpacks",
        &["mrpack", "zip"],
        &metadata,
        &mut mods,
    )
    .await?;
    mods.sort_by(|a, b| {
        a.category
            .cmp(&b.category)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(mods)
}

pub async fn set_mod_enabled(
    instance_path: PathBuf,
    mod_id: String,
    enabled: bool,
) -> Result<Vec<InstalledMod>, AppError> {
    let source = resource_path(&instance_path, &mod_id)?;
    let file_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::Instance("invalid mod file name".into()))?;
    let target = if enabled {
        if file_name.ends_with(".jar.disabled") {
            source.with_file_name(file_name.trim_end_matches(".disabled"))
        } else {
            source.clone()
        }
    } else if file_name.ends_with(".jar")
        || file_name.ends_with(".zip")
        || file_name.ends_with(".mrpack")
    {
        source.with_file_name(format!("{file_name}.disabled"))
    } else {
        source.clone()
    };

    if source != target {
        if tokio::fs::metadata(&target).await.is_ok() {
            return Err(AppError::Instance(format!(
                "target mod file already exists: {}",
                target.display()
            )));
        }
        tokio::fs::rename(source, target).await?;
    }
    list_mods(&instance_path).await
}

pub async fn delete_mod(
    instance_path: PathBuf,
    mod_id: String,
) -> Result<Vec<InstalledMod>, AppError> {
    let path = resource_path(&instance_path, &mod_id)?;
    tokio::fs::remove_file(path).await?;
    list_mods(&instance_path).await
}

pub async fn import_mod(
    instance_path: PathBuf,
    source: PathBuf,
) -> Result<Vec<InstalledMod>, AppError> {
    let file_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .ok_or_else(|| AppError::Instance("invalid source mod file name".into()))?;
    if !file_name.ends_with(".jar") {
        return Err(AppError::Instance(
            "mod import only accepts .jar files".into(),
        ));
    }
    let mods_dir = instance_path.join("mods");
    tokio::fs::create_dir_all(&mods_dir).await?;
    let target = mods_dir.join(&file_name);
    if tokio::fs::metadata(&target).await.is_ok() {
        return Err(AppError::Instance(format!(
            "mod already exists: {file_name}"
        )));
    }
    tokio::fs::copy(source, target).await?;
    upsert_metadata(
        &instance_path,
        vec![ResourceInstallRecord {
            id: format!("mods/{file_name}"),
            title: Some(file_name.trim_end_matches(".jar").to_string()),
            category: "Installed".into(),
            icon: None,
        }],
    )
    .await?;
    list_mods(&instance_path).await
}

pub async fn set_mod_category(
    instance_path: PathBuf,
    mod_id: String,
    category: String,
) -> Result<Vec<InstalledMod>, AppError> {
    let category = clean_category(&category)?;
    upsert_metadata(
        &instance_path,
        vec![ResourceInstallRecord {
            id: metadata_key(&mod_id),
            title: None,
            category,
            icon: None,
        }],
    )
    .await?;
    list_mods(&instance_path).await
}

pub async fn add_mod_category(instance_path: PathBuf, category: String) -> Result<(), AppError> {
    let category = clean_category(&category)?;
    let mut metadata = load_metadata(&instance_path).await.unwrap_or_default();
    if !metadata.categories.iter().any(|item| item == &category) {
        metadata.categories.push(category);
        metadata.categories.sort();
    }
    save_metadata(&instance_path, &metadata).await
}

pub fn default_mod_categories() -> Vec<String> {
    ["Installed", "Dependencies", "Modpacks", "Resource Packs", "Shaders"]
        .into_iter()
        .map(str::to_string)
        .collect()
}

pub fn categories_from_installed(installed: &[InstalledMod]) -> Vec<String> {
    let mut out = default_mod_categories();
    for item in installed {
        if !out.iter().any(|category| category == &item.category) {
            out.push(item.category.clone());
        }
    }
    out.sort();
    out
}

async fn scan_installed_dir(
    instance_path: &Path,
    dir_name: &str,
    default_category: &str,
    extensions: &[&str],
    metadata: &ModsMetadata,
    out: &mut Vec<InstalledMod>,
) -> Result<(), AppError> {
    let dir = instance_path.join(dir_name);
    if tokio::fs::metadata(&dir).await.is_err() {
        return Ok(());
    }
    let mut entries = tokio::fs::read_dir(dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let enabled = !name.ends_with(".disabled");
        let active_name = name.trim_end_matches(".disabled");
        let Some(extension) = active_name.rsplit('.').next() else {
            continue;
        };
        if !extensions.contains(&extension) {
            continue;
        }
        let id = format!("{dir_name}/{name}");
        let key = metadata_key(&id);
        let meta = metadata.items.get(&key);
        let display_name = meta
            .and_then(|item| item.title.clone())
            .unwrap_or_else(|| {
                active_name
                    .trim_end_matches(".jar")
                    .trim_end_matches(".zip")
                    .trim_end_matches(".mrpack")
                    .to_string()
            });
        out.push(InstalledMod {
            id,
            name: display_name,
            version: "local".into(),
            enabled,
            category: meta
                .map(|item| item.category.clone())
                .filter(|category| !category.trim().is_empty())
                .unwrap_or_else(|| default_category.to_string()),
            icon: meta.and_then(|item| item.icon.clone()),
        });
    }
    Ok(())
}

fn metadata_key(id: &str) -> String {
    id.trim_start_matches("mods/")
        .trim_end_matches(".disabled")
        .to_string()
        .split_once('/')
        .map(|_| id.trim_end_matches(".disabled").to_string())
        .unwrap_or_else(|| format!("mods/{}", id.trim_end_matches(".disabled")))
}

fn clean_category(category: &str) -> Result<String, AppError> {
    let category = category.trim();
    if category.is_empty() {
        return Err(AppError::Instance("category name cannot be empty".into()));
    }
    if category.len() > 40 {
        return Err(AppError::Instance("category name too long".into()));
    }
    Ok(category.to_string())
}

async fn upsert_metadata(
    instance_path: &Path,
    records: Vec<ResourceInstallRecord>,
) -> Result<(), AppError> {
    let mut metadata = load_metadata(instance_path).await.unwrap_or_default();
    for category in default_mod_categories() {
        if !metadata.categories.iter().any(|item| item == &category) {
            metadata.categories.push(category);
        }
    }
    for record in records {
        if !metadata.categories.iter().any(|item| item == &record.category) {
            metadata.categories.push(record.category.clone());
        }
        let key = metadata_key(&record.id);
        let entry = metadata.items.entry(key).or_default();
        if let Some(title) = record.title {
            entry.title = Some(title);
        }
        entry.category = record.category;
        if record.icon.is_some() {
            entry.icon = record.icon;
        }
    }
    metadata.categories.sort();
    save_metadata(instance_path, &metadata).await
}

async fn load_metadata(instance_path: &Path) -> Result<ModsMetadata, AppError> {
    let path = metadata_path(instance_path);
    let bytes = match tokio::fs::read(path).await {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(ModsMetadata::default()),
        Err(error) => return Err(AppError::Storage(error.to_string())),
    };
    Ok(serde_json::from_slice(&bytes)?)
}

async fn save_metadata(instance_path: &Path, metadata: &ModsMetadata) -> Result<(), AppError> {
    let path = metadata_path(instance_path);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(path, serde_json::to_vec_pretty(metadata)?).await?;
    Ok(())
}

fn metadata_path(instance_path: &Path) -> PathBuf {
    instance_path.join(".swift").join("mods.json")
}

pub async fn search_modrinth(
    query: String,
    minecraft_version: String,
    loader: LoaderKind,
    kind: ModrinthKind,
) -> Result<Vec<ModrinthProject>, AppError> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let loader = modrinth_loader(loader).ok();
    let facets = match (kind, loader) {
        (ModrinthKind::Mods | ModrinthKind::Modpacks, Some(loader)) => {
            format!(
                r#"[["project_type:{}"],["versions:{minecraft_version}"],["categories:{loader}"]]"#,
                kind.project_type()
            )
        }
        _ => format!(
            r#"[["project_type:{}"],["versions:{minecraft_version}"]]"#,
            kind.project_type()
        ),
    };
    let base = modrinth_base_url();
    let url = format!(
        "{base}/v2/search?query={}&limit=20&index=downloads&facets={}",
        url_encode(query),
        url_encode(&facets),
    );
    let client = modrinth_client();
    let response = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<ModrinthSearchResponse>()
        .await?;
    let mut projects = Vec::new();
    for hit in response.hits {
        let icon = match hit.icon_url {
            Some(url) => fetch_image_bytes(&client, &url).await.ok(),
            None => None,
        };
        projects.push(ModrinthProject {
            project_id: hit.project_id,
            slug: hit.slug,
            title: hit.title,
            author: hit.author.unwrap_or_else(|| "unknown".into()),
            description: hit.description,
            downloads: hit.downloads,
            icon,
            categories: hit.display_categories.unwrap_or(hit.categories),
            loaders: hit
                .versions
                .into_iter()
                .filter(|value| matches!(value.as_str(), "fabric" | "forge" | "neoforge" | "quilt"))
                .collect(),
            client_side: hit.client_side,
            server_side: hit.server_side,
            kind,
            provider: ResourceProvider::Modrinth,
        });
    }
    Ok(projects)
}

pub async fn search_resources(
    provider: ResourceProvider,
    curseforge_api_key: String,
    query: String,
    minecraft_version: String,
    loader: LoaderKind,
    kind: ModrinthKind,
) -> Result<Vec<ModrinthProject>, AppError> {
    match provider {
        ResourceProvider::Modrinth => search_modrinth(query, minecraft_version, loader, kind).await,
        ResourceProvider::CurseForge => {
            search_curseforge(curseforge_api_key, query, minecraft_version, loader, kind).await
        }
    }
}

pub async fn modrinth_project_detail(
    project_id: String,
    kind: ModrinthKind,
) -> Result<ModrinthProjectDetail, AppError> {
    let client = modrinth_client();
    let base = modrinth_base_url();
    let project = client
        .get(format!("{base}/v2/project/{}", url_encode(&project_id)))
        .send()
        .await?
        .error_for_status()?
        .json::<ModrinthProjectResponse>()
        .await?;
    let icon = match &project.icon_url {
        Some(url) => fetch_image_bytes(&client, url).await.ok(),
        None => None,
    };
    let mut gallery = Vec::new();
    for image in project.gallery.iter().take(4) {
        if let Ok(bytes) = fetch_image_bytes(&client, &image.url).await {
            gallery.push(bytes);
        }
    }
    Ok(ModrinthProjectDetail {
        project_id: project.id,
        title: project.title,
        description: project.description,
        body: project.body,
        downloads: project.downloads,
        icon,
        gallery,
        kind,
        provider: ResourceProvider::Modrinth,
    })
}

pub async fn resource_project_detail(
    provider: ResourceProvider,
    curseforge_api_key: String,
    project_id: String,
    kind: ModrinthKind,
) -> Result<ModrinthProjectDetail, AppError> {
    match provider {
        ResourceProvider::Modrinth => modrinth_project_detail(project_id, kind).await,
        ResourceProvider::CurseForge => {
            curseforge_project_detail(curseforge_api_key, project_id, kind).await
        }
    }
}

pub async fn install_modrinth_project(
    kind: ModrinthKind,
    instance_path: PathBuf,
    minecraft_version: String,
    loader: LoaderKind,
    project_id: String,
) -> Result<Vec<InstalledMod>, AppError> {
    install_modrinth_project_with_status(
        kind,
        instance_path,
        minecraft_version,
        loader,
        project_id,
        None,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn install_resource_project_with_status(
    provider: ResourceProvider,
    curseforge_api_key: String,
    kind: ModrinthKind,
    instance_path: PathBuf,
    minecraft_version: String,
    loader: LoaderKind,
    project_id: String,
    status_tx: Option<mpsc::UnboundedSender<InstallProgress>>,
) -> Result<Vec<InstalledMod>, AppError> {
    match provider {
        ResourceProvider::Modrinth => {
            install_modrinth_project_with_status(
                kind,
                instance_path,
                minecraft_version,
                loader,
                project_id,
                status_tx,
            )
            .await
        }
        ResourceProvider::CurseForge => {
            install_curseforge_project_with_status(
                curseforge_api_key,
                kind,
                instance_path,
                minecraft_version,
                loader,
                project_id,
                status_tx,
            )
            .await
        }
    }
}

pub async fn install_modrinth_project_with_status(
    kind: ModrinthKind,
    instance_path: PathBuf,
    minecraft_version: String,
    loader: LoaderKind,
    project_id: String,
    status_tx: Option<mpsc::UnboundedSender<InstallProgress>>,
) -> Result<Vec<InstalledMod>, AppError> {
    send_install_status(
        &status_tx,
        format!("Resolving Modrinth {kind} project"),
        0.03,
    );
    if kind == ModrinthKind::Modpacks {
        install_modrinth_modpack(
            instance_path.clone(),
            minecraft_version,
            loader,
            project_id,
            status_tx,
        )
        .await?;
        return list_mods(&instance_path).await;
    }
    let loader = if kind == ModrinthKind::Mods {
        Some(modrinth_loader(loader)?)
    } else {
        None
    };
    let mut visited = BTreeSet::new();
    let mut jobs = Vec::new();
    let mut records = Vec::new();
    collect_modrinth_jobs(
        kind,
        &project_id,
        &minecraft_version,
        loader,
        &instance_path,
        &mut visited,
        &mut jobs,
        &mut records,
    )
    .await?;
    download_modrinth_jobs_with_status(kind, jobs, status_tx, 0.10, 0.95).await?;
    upsert_metadata(&instance_path, records).await?;
    list_mods(&instance_path).await
}

async fn install_modrinth_modpack(
    instance_path: PathBuf,
    minecraft_version: String,
    loader: LoaderKind,
    project_id: String,
    status_tx: Option<mpsc::UnboundedSender<InstallProgress>>,
) -> Result<(), AppError> {
    let loader = modrinth_loader(loader).ok();
    let version = compatible_modrinth_version(&project_id, &minecraft_version, loader).await?;
    let file = primary_file(version.files).ok_or_else(|| {
        AppError::Download(format!(
            "Modrinth modpack {project_id} has no downloadable files"
        ))
    })?;
    if !file.filename.ends_with(".mrpack") {
        return Err(AppError::Download(format!(
            "Modrinth modpack {project_id} primary file is not .mrpack: {}",
            file.filename
        )));
    }

    let pack_dir = instance_path.join("modpacks");
    tokio::fs::create_dir_all(&pack_dir).await?;
    let file_name = file.filename.clone();
    let pack_path = pack_dir.join(&file_name);
    send_install_status(&status_tx, "Downloading .mrpack", 0.08);
    download_modrinth_jobs_with_status(
        ModrinthKind::Modpacks,
        vec![DownloadJob {
            id: format!("modrinth-modpack:{project_id}:{file_name}"),
            url: file.url,
            destination_path: pack_path.clone(),
            expected_sha1: file.hashes.sha1,
            size_bytes: file.size,
        }],
        status_tx.clone(),
        0.08,
        0.18,
    )
    .await?;

    send_install_status(&status_tx, "Reading .mrpack index", 0.20);
    let index = read_mrpack_index(pack_path.clone()).await?;
    let jobs = index
        .files
        .into_iter()
        .filter(|file| file.env.client != Some(MrpackSideSupport::Unsupported))
        .map(|file| {
            let url = file.downloads.into_iter().next().ok_or_else(|| {
                AppError::Download(format!("mrpack file {} has no download URL", file.path))
            })?;
            let destination_path = safe_instance_child(&instance_path, &file.path)?;
            Ok(DownloadJob {
                id: format!("mrpack:{}", file.path),
                url,
                destination_path,
                expected_sha1: file.hashes.sha1,
                size_bytes: file.file_size,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    download_modrinth_jobs_with_status(ModrinthKind::Modpacks, jobs, status_tx.clone(), 0.22, 0.92)
        .await?;
    send_install_status(&status_tx, "Applying modpack overrides", 0.95);
    extract_mrpack_overrides(pack_path, instance_path.clone()).await?;
    upsert_metadata(
        &instance_path,
        vec![ResourceInstallRecord {
            id: format!("modpacks/{file_name}"),
            title: Some(project_id),
            category: "Modpacks".into(),
            icon: None,
        }],
    )
    .await
}

async fn search_curseforge(
    api_key: String,
    query: String,
    minecraft_version: String,
    loader: LoaderKind,
    kind: ModrinthKind,
) -> Result<Vec<ModrinthProject>, AppError> {
    let api_key = require_curseforge_key(&api_key)?;
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let mut url = format!(
        "{}/mods/search?gameId=432&classId={}&searchFilter={}&gameVersion={}&sortField=6&sortOrder=desc&pageSize=20",
        curseforge_base_url(),
        curseforge_class_id(kind),
        url_encode(query),
        url_encode(&minecraft_version),
    );
    if matches!(kind, ModrinthKind::Mods | ModrinthKind::Modpacks) {
        if let Some(loader_type) = curseforge_loader_type(loader) {
            url.push_str(&format!("&modLoaderType={loader_type}"));
        }
    }
    let client = curseforge_client();
    let response = client
        .get(url)
        .header("x-api-key", api_key)
        .send()
        .await?
        .error_for_status()?
        .json::<CurseForgeEnvelope<Vec<CurseForgeMod>>>()
        .await?;
    let mut projects = Vec::new();
    for item in response.data {
        let icon_url = item.logo.and_then(|logo| logo.thumbnail_url.or(logo.url));
        let icon = match icon_url {
            Some(url) => fetch_image_bytes(&client, &url).await.ok(),
            None => None,
        };
        projects.push(ModrinthProject {
            project_id: item.id.to_string(),
            slug: item.slug.unwrap_or_else(|| item.id.to_string()),
            title: item.name,
            author: item.authors.first().map(|author| author.name.clone()).unwrap_or_else(|| "unknown".into()),
            description: item.summary.unwrap_or_default(),
            downloads: item.download_count.unwrap_or_default() as u64,
            icon,
            categories: item.categories.into_iter().map(|category| category.name).collect(),
            loaders: Vec::new(),
            client_side: None,
            server_side: None,
            kind,
            provider: ResourceProvider::CurseForge,
        });
    }
    Ok(projects)
}

async fn curseforge_project_detail(
    api_key: String,
    project_id: String,
    kind: ModrinthKind,
) -> Result<ModrinthProjectDetail, AppError> {
    let api_key = require_curseforge_key(&api_key)?;
    let client = curseforge_client();
    let base = curseforge_base_url();
    let project = client
        .get(format!("{base}/mods/{project_id}"))
        .header("x-api-key", api_key)
        .send()
        .await?
        .error_for_status()?
        .json::<CurseForgeEnvelope<CurseForgeMod>>()
        .await?
        .data;
    let body = client
        .get(format!("{base}/mods/{project_id}/description"))
        .header("x-api-key", api_key)
        .send()
        .await?
        .error_for_status()?
        .json::<CurseForgeEnvelope<String>>()
        .await?
        .data;
    let icon_url = project
        .logo
        .as_ref()
        .and_then(|logo| logo.thumbnail_url.as_ref().or(logo.url.as_ref()));
    let icon = match icon_url {
        Some(url) => fetch_image_bytes(&client, url).await.ok(),
        None => None,
    };
    let mut gallery = Vec::new();
    for screenshot in project.screenshots.iter().take(4) {
        let url = screenshot.thumbnail_url.as_ref().or(screenshot.url.as_ref());
        if let Some(url) = url {
            if let Ok(bytes) = fetch_image_bytes(&client, url).await {
                gallery.push(bytes);
            }
        }
    }
    Ok(ModrinthProjectDetail {
        project_id,
        title: project.name,
        description: project.summary.unwrap_or_default(),
        body,
        downloads: project.download_count.unwrap_or_default() as u64,
        icon,
        gallery,
        kind,
        provider: ResourceProvider::CurseForge,
    })
}

#[allow(clippy::too_many_arguments)]
async fn install_curseforge_project_with_status(
    api_key: String,
    kind: ModrinthKind,
    instance_path: PathBuf,
    minecraft_version: String,
    loader: LoaderKind,
    project_id: String,
    status_tx: Option<mpsc::UnboundedSender<InstallProgress>>,
) -> Result<Vec<InstalledMod>, AppError> {
    let api_key = require_curseforge_key(&api_key)?;
    send_install_status(&status_tx, format!("Resolving CurseForge {kind}"), 0.04);
    let mut visited = BTreeSet::new();
    let mut jobs = Vec::new();
    let mut records = Vec::new();
    collect_curseforge_jobs(
        api_key,
        kind,
        &project_id,
        &minecraft_version,
        loader,
        &instance_path,
        &mut visited,
        &mut jobs,
        &mut records,
    )
    .await?;
    download_modrinth_jobs_with_status(kind, jobs, status_tx, 0.10, 0.95).await?;
    upsert_metadata(&instance_path, records).await?;
    list_mods(&instance_path).await
}

#[allow(clippy::too_many_arguments)]
async fn collect_curseforge_jobs(
    api_key: &str,
    kind: ModrinthKind,
    project_id: &str,
    minecraft_version: &str,
    loader: LoaderKind,
    instance_path: &Path,
    visited: &mut BTreeSet<String>,
    jobs: &mut Vec<DownloadJob>,
    records: &mut Vec<ResourceInstallRecord>,
) -> Result<(), AppError> {
    let client = curseforge_client();
    let mut stack = vec![(project_id.to_string(), false)];
    while let Some((project_id, is_dependency)) = stack.pop() {
        if !visited.insert(project_id.clone()) {
            continue;
        }
        let project = curseforge_mod(&client, api_key, &project_id).await.ok();
        let file = compatible_curseforge_file(&client, api_key, &project_id, minecraft_version, loader).await?;
        for dependency in &file.dependencies {
            if dependency.relation_type == 3 {
                let dep_id = dependency.mod_id.to_string();
                if !visited.contains(&dep_id) {
                    stack.push((dep_id, true));
                }
            }
        }
        let download_url = match file.download_url.filter(|url| !url.trim().is_empty()) {
            Some(url) => url,
            None => curseforge_download_url(&client, api_key, &project_id, file.id).await?,
        };
        let relative_path = format!("{}/{}", modrinth_install_dir(kind), file.file_name);
        let sha1 = file
            .hashes
            .iter()
            .find(|hash| hash.algo == 1)
            .map(|hash| hash.value.clone());
        jobs.push(DownloadJob {
            id: format!("curseforge:{project_id}:{}", file.file_name),
            url: download_url,
            destination_path: safe_instance_child(instance_path, &relative_path)?,
            expected_sha1: sha1,
            size_bytes: file.file_length,
        });
        let icon = match project
            .as_ref()
            .and_then(|item| item.logo.as_ref())
            .and_then(|logo| logo.thumbnail_url.as_ref().or(logo.url.as_ref()))
        {
            Some(url) => fetch_image_bytes(&client, url).await.ok(),
            None => None,
        };
        records.push(ResourceInstallRecord {
            id: relative_path,
            title: project.map(|item| item.name),
            category: if is_dependency {
                "Dependencies".into()
            } else {
                match kind {
                    ModrinthKind::Mods => "Installed".into(),
                    ModrinthKind::Modpacks => "Modpacks".into(),
                    ModrinthKind::ResourcePacks => "Resource Packs".into(),
                    ModrinthKind::Shaders => "Shaders".into(),
                }
            },
            icon,
        });
    }
    Ok(())
}

async fn download_modrinth_jobs_with_status(
    kind: ModrinthKind,
    jobs: Vec<DownloadJob>,
    status_tx: Option<mpsc::UnboundedSender<InstallProgress>>,
    start: f32,
    end: f32,
) -> Result<(), AppError> {
    let total = jobs.len();
    if total == 0 {
        send_install_status(&status_tx, format!("{kind} has no files to download"), end);
        return Ok(());
    }
    send_install_status(
        &status_tx,
        format!("Downloading {total} {kind} files"),
        start,
    );
    let (progress_tx, mut progress_rx) = mpsc::unbounded_channel();
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let (control_tx, control_rx) = tokio::sync::watch::channel(DownloadControl::Run);
    let status_for_forwarder = status_tx.clone();
    let span = (end - start).max(0.01);
    let forwarder = tokio::spawn(async move {
        let mut done = 0usize;
        let mut progress = start;
        let mut speed = 0u64;
        loop {
            tokio::select! {
                Some((current, count)) = progress_rx.recv() => {
                    done = current;
                    progress = start + progress_fraction(done, count) * span;
                    send_install_status(
                        &status_for_forwarder,
                        format!("{kind} files {done}/{count}"),
                        progress,
                    );
                }
                Some(event) = event_rx.recv() => {
                    if let Some(status) = download_event_status(kind, done, total, &event, &mut speed) {
                        send_install_status(&status_for_forwarder, status, progress);
                    }
                }
                else => break,
            }
        }
    });
    download_jobs_checked_with_progress_control_and_events(
        jobs,
        Some(progress_tx),
        control_rx,
        Some(event_tx),
    )
    .await?;
    drop(control_tx);
    let _ = forwarder.await;
    send_install_status(&status_tx, format!("{kind} install complete"), end);
    Ok(())
}

async fn read_mrpack_index(pack_path: PathBuf) -> Result<MrpackIndex, AppError> {
    tokio::task::spawn_blocking(move || {
        let file = std::fs::File::open(&pack_path).map_err(|error| {
            AppError::Download(format!("open mrpack {}: {error}", pack_path.display()))
        })?;
        let mut archive = zip::ZipArchive::new(file).map_err(|error| {
            AppError::Download(format!("read mrpack {}: {error}", pack_path.display()))
        })?;
        let mut index = archive.by_name("modrinth.index.json").map_err(|error| {
            AppError::Download(format!(
                "mrpack {} missing modrinth.index.json: {error}",
                pack_path.display()
            ))
        })?;
        let mut bytes = Vec::new();
        index
            .read_to_end(&mut bytes)
            .map_err(|error| AppError::Download(format!("read mrpack index: {error}")))?;
        serde_json::from_slice::<MrpackIndex>(&bytes).map_err(AppError::from)
    })
    .await
    .map_err(|error| AppError::Download(error.to_string()))?
}

async fn extract_mrpack_overrides(
    pack_path: PathBuf,
    instance_path: PathBuf,
) -> Result<(), AppError> {
    tokio::task::spawn_blocking(move || {
        let file = std::fs::File::open(&pack_path).map_err(|error| {
            AppError::Download(format!("open mrpack {}: {error}", pack_path.display()))
        })?;
        let mut archive = zip::ZipArchive::new(file).map_err(|error| {
            AppError::Download(format!("read mrpack {}: {error}", pack_path.display()))
        })?;
        for index in 0..archive.len() {
            let mut file = archive
                .by_index(index)
                .map_err(|error| AppError::Download(format!("read mrpack entry: {error}")))?;
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
            let destination = safe_instance_child(&instance_path, relative)?;
            if file.is_dir() || name.ends_with('/') {
                std::fs::create_dir_all(&destination)
                    .map_err(|error| AppError::Download(error.to_string()))?;
                continue;
            }
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| AppError::Download(error.to_string()))?;
            }
            let mut output = std::fs::File::create(&destination)
                .map_err(|error| AppError::Download(error.to_string()))?;
            std::io::copy(&mut file, &mut output)
                .map_err(|error| AppError::Download(error.to_string()))?;
        }
        Ok(())
    })
    .await
    .map_err(|error| AppError::Download(error.to_string()))?
}

#[allow(clippy::too_many_arguments)]
async fn collect_modrinth_jobs(
    kind: ModrinthKind,
    project_id: &str,
    minecraft_version: &str,
    loader: Option<&str>,
    instance_path: &Path,
    visited: &mut BTreeSet<String>,
    jobs: &mut Vec<DownloadJob>,
    records: &mut Vec<ResourceInstallRecord>,
) -> Result<(), AppError> {
    let mut stack = vec![(project_id.to_string(), false)];
    while let Some((project_id, is_dependency)) = stack.pop() {
        if !visited.insert(project_id.clone()) {
            continue;
        }
        let version = compatible_modrinth_version(&project_id, minecraft_version, loader).await?;
        for dependency in &version.dependencies {
            if dependency.dependency_type == "required" {
                if let Some(project_id) = &dependency.project_id {
                    if !visited.contains(project_id) {
                        stack.push((project_id.clone(), true));
                    }
                }
            }
        }
        let metadata = modrinth_project_metadata(&project_id).await.ok();
        let file = primary_file(version.files).ok_or_else(|| {
            AppError::Download(format!(
                "Modrinth project {project_id} has no downloadable files"
            ))
        })?;
        let relative_path = format!("{}/{}", modrinth_install_dir(kind), file.filename);
        jobs.push(DownloadJob {
            id: format!("modrinth:{project_id}:{}", file.filename),
            url: file.url,
            destination_path: safe_instance_child(instance_path, &relative_path)?,
            expected_sha1: file.hashes.sha1,
            size_bytes: file.size,
        });
        records.push(ResourceInstallRecord {
            id: relative_path,
            title: metadata.as_ref().map(|item| item.title.clone()),
            category: if is_dependency {
                "Dependencies".into()
            } else {
                match kind {
                    ModrinthKind::Mods => "Installed".into(),
                    ModrinthKind::Modpacks => "Modpacks".into(),
                    ModrinthKind::ResourcePacks => "Resource Packs".into(),
                    ModrinthKind::Shaders => "Shaders".into(),
                }
            },
            icon: metadata.and_then(|item| item.icon),
        });
    }
    Ok(())
}

async fn compatible_modrinth_version(
    project_id: &str,
    minecraft_version: &str,
    loader: Option<&str>,
) -> Result<ModrinthVersion, AppError> {
    let base = modrinth_base_url();
    let url = if let Some(loader) = loader {
        format!(
            "{base}/v2/project/{}/version?loaders={}&game_versions={}",
            url_encode(project_id),
            url_encode(&format!(r#"["{loader}"]"#)),
            url_encode(&format!(r#"["{minecraft_version}"]"#)),
        )
    } else {
        format!(
            "{base}/v2/project/{}/version?game_versions={}",
            url_encode(project_id),
            url_encode(&format!(r#"["{minecraft_version}"]"#)),
        )
    };
    modrinth_client()
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<ModrinthVersion>>()
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| {
            AppError::Download(format!(
                "no compatible Modrinth version found for {project_id}"
            ))
        })
}

async fn compatible_curseforge_file(
    client: &reqwest::Client,
    api_key: &str,
    project_id: &str,
    minecraft_version: &str,
    loader: LoaderKind,
) -> Result<CurseForgeFile, AppError> {
    let mut url = format!(
        "{}/mods/{project_id}/files?gameVersion={}&pageSize=50",
        curseforge_base_url(),
        url_encode(minecraft_version),
    );
    if let Some(loader_type) = curseforge_loader_type(loader) {
        url.push_str(&format!("&modLoaderType={loader_type}"));
    }
    client
        .get(url)
        .header("x-api-key", api_key)
        .send()
        .await?
        .error_for_status()?
        .json::<CurseForgeEnvelope<Vec<CurseForgeFile>>>()
        .await?
        .data
        .into_iter()
        .find(|file| file.file_status == Some(4) || file.file_status.is_none())
        .ok_or_else(|| {
            AppError::Download(format!(
                "no compatible CurseForge file found for {project_id}"
            ))
        })
}

async fn curseforge_mod(
    client: &reqwest::Client,
    api_key: &str,
    project_id: &str,
) -> Result<CurseForgeMod, AppError> {
    Ok(client
        .get(format!("{}/mods/{project_id}", curseforge_base_url()))
        .header("x-api-key", api_key)
        .send()
        .await?
        .error_for_status()?
        .json::<CurseForgeEnvelope<CurseForgeMod>>()
        .await?
        .data)
}

async fn curseforge_download_url(
    client: &reqwest::Client,
    api_key: &str,
    project_id: &str,
    file_id: u64,
) -> Result<String, AppError> {
    Ok(client
        .get(format!(
            "{}/mods/{project_id}/files/{file_id}/download-url",
            curseforge_base_url()
        ))
        .header("x-api-key", api_key)
        .send()
        .await?
        .error_for_status()?
        .json::<CurseForgeEnvelope<String>>()
        .await?
        .data)
}

async fn modrinth_project_metadata(project_id: &str) -> Result<ProjectMetadata, AppError> {
    let client = modrinth_client();
    let base = modrinth_base_url();
    let project = client
        .get(format!("{base}/v2/project/{}", url_encode(project_id)))
        .send()
        .await?
        .error_for_status()?
        .json::<ModrinthProjectResponse>()
        .await?;
    let icon = match &project.icon_url {
        Some(url) => fetch_image_bytes(&client, url).await.ok(),
        None => None,
    };
    Ok(ProjectMetadata {
        title: project.title,
        icon,
    })
}

fn modrinth_install_dir(kind: ModrinthKind) -> &'static str {
    match kind {
        ModrinthKind::Mods => "mods",
        ModrinthKind::ResourcePacks => "resourcepacks",
        ModrinthKind::Shaders => "shaderpacks",
        ModrinthKind::Modpacks => "modpacks",
    }
}

fn primary_file(files: Vec<ModrinthFile>) -> Option<ModrinthFile> {
    let first = files.first().cloned();
    files.into_iter().find(|file| file.primary).or(first)
}

fn resource_path(instance_path: &Path, resource_id: &str) -> Result<PathBuf, AppError> {
    let resource_id = if resource_id.contains('/') || resource_id.contains('\\') {
        resource_id.to_string()
    } else {
        format!("mods/{resource_id}")
    };
    let path = safe_instance_child(instance_path, &resource_id)?;
    let allowed_dir = resource_id.starts_with("mods/")
        || resource_id.starts_with("resourcepacks/")
        || resource_id.starts_with("shaderpacks/")
        || resource_id.starts_with("modpacks/");
    let allowed_ext = resource_id.ends_with(".jar")
        || resource_id.ends_with(".jar.disabled")
        || resource_id.ends_with(".zip")
        || resource_id.ends_with(".zip.disabled")
        || resource_id.ends_with(".mrpack")
        || resource_id.ends_with(".mrpack.disabled");
    if !allowed_dir || !allowed_ext {
        return Err(AppError::Instance("invalid installed resource id".into()));
    }
    Ok(path)
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ModsMetadata {
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    items: BTreeMap<String, ModMetadata>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ModMetadata {
    #[serde(default)]
    title: Option<String>,
    #[serde(default = "installed_category")]
    category: String,
    #[serde(default)]
    icon: Option<Vec<u8>>,
}

fn installed_category() -> String {
    "Installed".into()
}

struct ResourceInstallRecord {
    id: String,
    title: Option<String>,
    category: String,
    icon: Option<Vec<u8>>,
}

struct ProjectMetadata {
    title: String,
    icon: Option<Vec<u8>>,
}

#[derive(Debug, Deserialize)]
struct CurseForgeEnvelope<T> {
    data: T,
}

#[derive(Debug, Deserialize)]
struct CurseForgeMod {
    id: u64,
    name: String,
    slug: Option<String>,
    summary: Option<String>,
    #[serde(default, rename = "downloadCount")]
    download_count: Option<f64>,
    logo: Option<CurseForgeImage>,
    #[serde(default)]
    screenshots: Vec<CurseForgeImage>,
    #[serde(default)]
    authors: Vec<CurseForgeAuthor>,
    #[serde(default)]
    categories: Vec<CurseForgeCategory>,
}

#[derive(Debug, Deserialize)]
struct CurseForgeAuthor {
    name: String,
}

#[derive(Debug, Deserialize)]
struct CurseForgeCategory {
    name: String,
}

#[derive(Debug, Deserialize)]
struct CurseForgeImage {
    url: Option<String>,
    #[serde(rename = "thumbnailUrl")]
    thumbnail_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CurseForgeFile {
    id: u64,
    #[serde(rename = "fileName")]
    file_name: String,
    #[serde(rename = "downloadUrl")]
    download_url: Option<String>,
    #[serde(rename = "fileLength")]
    file_length: Option<u64>,
    #[serde(default)]
    hashes: Vec<CurseForgeHash>,
    #[serde(default)]
    dependencies: Vec<CurseForgeDependency>,
    #[serde(rename = "fileStatus")]
    file_status: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct CurseForgeHash {
    value: String,
    algo: u32,
}

#[derive(Debug, Deserialize)]
struct CurseForgeDependency {
    #[serde(rename = "modId")]
    mod_id: u64,
    #[serde(rename = "relationType")]
    relation_type: u32,
}

#[derive(Debug, Deserialize)]
struct ModrinthSearchResponse {
    hits: Vec<ModrinthHit>,
}

#[derive(Debug, Deserialize)]
struct ModrinthHit {
    project_id: String,
    slug: String,
    title: String,
    author: Option<String>,
    description: String,
    downloads: u64,
    icon_url: Option<String>,
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    display_categories: Option<Vec<String>>,
    #[serde(default)]
    versions: Vec<String>,
    #[serde(default)]
    client_side: Option<String>,
    #[serde(default)]
    server_side: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ModrinthProjectResponse {
    id: String,
    title: String,
    description: String,
    body: String,
    downloads: u64,
    icon_url: Option<String>,
    #[serde(default)]
    gallery: Vec<ModrinthGalleryImage>,
}

#[derive(Debug, Deserialize)]
struct ModrinthGalleryImage {
    url: String,
}

#[derive(Debug, Deserialize)]
struct ModrinthVersion {
    files: Vec<ModrinthFile>,
    #[serde(default)]
    dependencies: Vec<ModrinthDependency>,
}

#[derive(Debug, Clone, Deserialize)]
struct ModrinthFile {
    url: String,
    filename: String,
    hashes: ModrinthHashes,
    size: Option<u64>,
    #[serde(default)]
    primary: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct ModrinthHashes {
    sha1: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ModrinthDependency {
    project_id: Option<String>,
    dependency_type: String,
}

#[derive(Debug, Deserialize)]
struct MrpackIndex {
    #[serde(default)]
    files: Vec<MrpackFile>,
}

#[derive(Debug, Deserialize)]
struct MrpackFile {
    path: String,
    hashes: ModrinthHashes,
    #[serde(default)]
    env: MrpackEnv,
    downloads: Vec<String>,
    #[serde(default, rename = "fileSize")]
    file_size: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
struct MrpackEnv {
    client: Option<MrpackSideSupport>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum MrpackSideSupport {
    Required,
    Optional,
    Unsupported,
}

fn modrinth_loader(loader: LoaderKind) -> Result<&'static str, AppError> {
    match loader {
        LoaderKind::Fabric => Ok("fabric"),
        LoaderKind::Quilt => Ok("quilt"),
        LoaderKind::Forge => Ok("forge"),
        LoaderKind::NeoForge => Ok("neoforge"),
        LoaderKind::Vanilla => Err(AppError::Instance(
            "Modrinth mod install needs a mod loader instance".into(),
        )),
    }
}

fn safe_instance_child(root: &Path, relative: &str) -> Result<PathBuf, AppError> {
    let relative = relative.replace('\\', "/");
    let mut path = PathBuf::new();
    for component in Path::new(&relative).components() {
        match component {
            std::path::Component::Normal(part) => path.push(part),
            std::path::Component::CurDir => {}
            _ => {
                return Err(AppError::Download(format!(
                    "unsafe path in Modrinth pack: {relative}"
                )))
            }
        }
    }
    if path.as_os_str().is_empty() {
        return Err(AppError::Download("empty path in Modrinth pack".into()));
    }
    Ok(root.join(path))
}

fn send_install_status(
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
    kind: ModrinthKind,
    done: usize,
    total: usize,
    event: &DownloadEvent,
    speed: &mut u64,
) -> Option<String> {
    match event {
        DownloadEvent::Speed { bytes_per_second } => {
            *speed = *bytes_per_second;
            Some(format!("{kind} {done}/{total} @ {}", format_speed(*speed)))
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
            Some(format!("{kind} {done}/{total}: {name} {bytes}{suffix}"))
        }
        DownloadEvent::Complete { job_id } => Some(format!(
            "{kind} {done}/{total}: {} complete",
            compact_job_name(job_id)
        )),
        DownloadEvent::Failed { job_id, reason } => Some(format!(
            "{kind} {done}/{total}: {} failed: {reason}",
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

async fn fetch_image_bytes(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, AppError> {
    Ok(client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?
        .to_vec())
}

fn modrinth_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(format!("SwiftLauncher/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

fn modrinth_base_url() -> String {
    std::env::var("SWIFT_LAUNCHER_MODRINTH_BASE")
        .ok()
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "https://api.modrinth.com".to_string())
}

fn curseforge_base_url() -> String {
    std::env::var("SWIFT_LAUNCHER_CURSEFORGE_BASE")
        .ok()
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "https://api.curseforge.com/v1".to_string())
}

fn curseforge_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(format!("SwiftLauncher/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

fn require_curseforge_key(api_key: &str) -> Result<&str, AppError> {
    let api_key = api_key.trim();
    if api_key.is_empty() {
        return Err(AppError::Instance(
            "CurseForge needs API key. Paste it in Settings > Integrations.".into(),
        ));
    }
    Ok(api_key)
}

fn curseforge_class_id(kind: ModrinthKind) -> u32 {
    match kind {
        ModrinthKind::Mods => 6,
        ModrinthKind::Modpacks => 4471,
        ModrinthKind::ResourcePacks => 12,
        ModrinthKind::Shaders => 6552,
    }
}

fn curseforge_loader_type(loader: LoaderKind) -> Option<u32> {
    match loader {
        LoaderKind::Forge => Some(1),
        LoaderKind::Fabric => Some(4),
        LoaderKind::Quilt => Some(5),
        LoaderKind::NeoForge => Some(6),
        LoaderKind::Vanilla => None,
    }
}

fn url_encode(input: &str) -> String {
    let mut out = String::new();
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            b' ' => out.push_str("%20"),
            _ => {
                use std::fmt::Write;
                let _ = write!(&mut out, "%{byte:02X}");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::{BTreeSet, HashMap};
    use std::io::Write as IoWrite;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    async fn spawn_test_server(
        routes: HashMap<String, Vec<u8>>,
    ) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let routes = Arc::new(routes);
        let handle = tokio::spawn(async move {
            loop {
                let (mut socket, _) = match listener.accept().await {
                    Ok(value) => value,
                    Err(_) => break,
                };
                let mut buf = [0u8; 2048];
                let Ok(n) = socket.read(&mut buf).await else {
                    continue;
                };
                if n == 0 {
                    continue;
                }
                let request = String::from_utf8_lossy(&buf[..n]);
                let path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/");
                let path = path.split('?').next().unwrap_or("/");
                if let Some(body) = routes.get(path) {
                    let response =
                        format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len());
                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.write_all(body).await;
                } else {
                    let response = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
                    let _ = socket.write_all(response.as_bytes()).await;
                }
            }
        });
        (format!("http://{}", addr), handle)
    }

    fn temp_dir(prefix: &str) -> PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("swift-launcher-test-{prefix}-{pid}-{now}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn modrinth_collects_required_dependencies() {
        let root_json = json!([
            {
                "files": [
                    {"url": "http://example/root.jar", "filename": "root.jar", "hashes": {"sha1": "root"}, "primary": true}
                ],
                "dependencies": [
                    {"project_id": "lib-a", "dependency_type": "required"},
                    {"project_id": "lib-opt", "dependency_type": "optional"}
                ]
            }
        ]);
        let lib_a_json = json!([
            {
                "files": [
                    {"url": "http://example/lib-a.jar", "filename": "lib-a.jar", "hashes": {"sha1": "liba"}, "primary": true}
                ],
                "dependencies": [
                    {"project_id": "lib-b", "dependency_type": "required"}
                ]
            }
        ]);
        let lib_b_json = json!([
            {
                "files": [
                    {"url": "http://example/lib-b.jar", "filename": "lib-b.jar", "hashes": {"sha1": "libb"}, "primary": true}
                ],
                "dependencies": []
            }
        ]);

        let mut routes = HashMap::new();
        routes.insert(
            "/v2/project/root/version".to_string(),
            root_json.to_string().into_bytes(),
        );
        routes.insert(
            "/v2/project/lib-a/version".to_string(),
            lib_a_json.to_string().into_bytes(),
        );
        routes.insert(
            "/v2/project/lib-b/version".to_string(),
            lib_b_json.to_string().into_bytes(),
        );

        let (base, handle) = spawn_test_server(routes).await;
        std::env::set_var("SWIFT_LAUNCHER_MODRINTH_BASE", &base);

        let instance_path = temp_dir("modrinth-deps");
        let mut visited = BTreeSet::new();
        let mut jobs = Vec::new();
        let mut records = Vec::new();

        let result = collect_modrinth_jobs(
            ModrinthKind::Mods,
            "root",
            "1.20.1",
            Some("fabric"),
            &instance_path,
            &mut visited,
            &mut jobs,
            &mut records,
        )
        .await;

        std::env::remove_var("SWIFT_LAUNCHER_MODRINTH_BASE");
        handle.abort();

        assert!(result.is_ok());
        assert_eq!(visited.len(), 3);
        assert_eq!(jobs.len(), 3);

        let mut ids = jobs.iter().map(|job| job.id.clone()).collect::<Vec<_>>();
        ids.sort();
        assert!(ids.iter().any(|id| id.starts_with("modrinth:root:")));
        assert!(ids.iter().any(|id| id.starts_with("modrinth:lib-a:")));
        assert!(ids.iter().any(|id| id.starts_with("modrinth:lib-b:")));
        assert!(!ids.iter().any(|id| id.contains("lib-opt")));
    }

    #[tokio::test]
    async fn mrpack_index_and_overrides_are_applied() {
        let root = temp_dir("mrpack");
        let pack_path = root.join("pack.mrpack");
        let file = std::fs::File::create(&pack_path).unwrap();
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        zip.start_file("modrinth.index.json", options).unwrap();
        zip.write_all(
            br#"{
                "formatVersion": 1,
                "game": "minecraft",
                "versionId": "1.0.0",
                "name": "Pack",
                "files": [
                    {
                        "path": "mods/example.jar",
                        "hashes": {"sha1": "abc"},
                        "env": {"client": "required", "server": "required"},
                        "downloads": ["http://example/mod.jar"],
                        "fileSize": 123
                    },
                    {
                        "path": "mods/server-only.jar",
                        "hashes": {"sha1": "def"},
                        "env": {"client": "unsupported", "server": "required"},
                        "downloads": ["http://example/server.jar"]
                    }
                ]
            }"#,
        )
        .unwrap();
        zip.start_file("overrides/options.txt", options).unwrap();
        zip.write_all(b"guiScale:2").unwrap();
        zip.start_file("client-overrides/config/client.txt", options)
            .unwrap();
        zip.write_all(b"client").unwrap();
        zip.start_file("server-overrides/config/server.txt", options)
            .unwrap();
        zip.write_all(b"server").unwrap();
        zip.finish().unwrap();

        let index = read_mrpack_index(pack_path.clone()).await.unwrap();
        assert_eq!(index.files.len(), 2);
        assert_eq!(index.files[0].path, "mods/example.jar");
        assert_eq!(index.files[0].file_size, Some(123));
        assert_eq!(index.files[0].downloads[0], "http://example/mod.jar");
        assert_eq!(
            index.files[1].env.client,
            Some(MrpackSideSupport::Unsupported)
        );

        let instance = root.join("instance");
        extract_mrpack_overrides(pack_path, instance.clone())
            .await
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(instance.join("options.txt")).unwrap(),
            "guiScale:2"
        );
        assert_eq!(
            std::fs::read_to_string(instance.join("config/client.txt")).unwrap(),
            "client"
        );
        assert!(!instance.join("config/server.txt").exists());
    }

    #[test]
    fn safe_instance_child_rejects_escape_paths() {
        let root = PathBuf::from("/tmp/instance");
        assert!(safe_instance_child(&root, "mods/example.jar").is_ok());
        assert!(safe_instance_child(&root, "../evil.jar").is_err());
        assert!(safe_instance_child(&root, "/absolute.jar").is_err());
    }
}
