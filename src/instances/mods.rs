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
    scan_installed_dir(
        instance_path,
        "mods",
        "Installed",
        &["jar"],
        &metadata,
        &mut mods,
    )
    .await?;
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
    match tokio::fs::remove_file(path).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    remove_metadata_item(&instance_path, &mod_id).await?;
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
            project_id: None,
            sha1: None,
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
            project_id: None,
            sha1: None,
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
    [
        "Installed",
        "Dependencies",
        "Modpacks",
        "Resource Packs",
        "Shaders",
    ]
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
        let meta = metadata_item(metadata, &key);
        let display_name = meta.and_then(|item| item.title.clone()).unwrap_or_else(|| {
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
    let mut id = id.trim_start_matches('/').trim_end_matches(".disabled");
    while let Some(rest) = id.strip_prefix("mods/mods/") {
        id = rest;
    }
    if id.starts_with("mods/")
        || id.starts_with("resourcepacks/")
        || id.starts_with("shaderpacks/")
        || id.contains('/')
    {
        id.to_string()
    } else {
        format!("mods/{id}")
    }
}

fn legacy_metadata_key(key: &str) -> Option<String> {
    key.strip_prefix("mods/")
        .map(|_| format!("mods/{key}"))
        .filter(|legacy| legacy != key)
}

fn metadata_item<'a>(metadata: &'a ModsMetadata, key: &str) -> Option<&'a ModMetadata> {
    metadata
        .items
        .get(key)
        .or_else(|| legacy_metadata_key(key).and_then(|legacy| metadata.items.get(&legacy)))
}

fn is_manageable_resource_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    (lower.starts_with("mods/") && lower.ends_with(".jar"))
        || (lower.starts_with("resourcepacks/") && lower.ends_with(".zip"))
        || (lower.starts_with("shaderpacks/") && lower.ends_with(".zip"))
}

fn modpack_resource_category(path: &str) -> String {
    let lower = path.to_lowercase();
    if lower.starts_with("resourcepacks/") {
        "Resource Packs".into()
    } else if lower.starts_with("shaderpacks/") {
        "Shaders".into()
    } else {
        "Modpacks".into()
    }
}

fn resource_category(kind: ModrinthKind, is_dependency: bool) -> &'static str {
    if is_dependency {
        return "Dependencies";
    }
    match kind {
        ModrinthKind::Mods => "Installed",
        ModrinthKind::Modpacks => "Modpacks",
        ModrinthKind::ResourcePacks => "Resource Packs",
        ModrinthKind::Shaders => "Shaders",
    }
}

fn replacement_category(matches: &[InstalledResourceMatch], default_category: &str) -> String {
    matches
        .iter()
        .find_map(|item| {
            let category = item.category.trim();
            (!category.is_empty()).then_some(category.to_string())
        })
        .unwrap_or_else(|| default_category.to_string())
}

fn pretty_resource_name(path: &str) -> String {
    let name = path
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .trim_end_matches(".disabled")
        .trim_end_matches(".jar")
        .trim_end_matches(".zip")
        .trim_end_matches(".mrpack");
    let without_version = strip_common_version_suffix(name);
    without_version
        .split(['-', '_', '.'])
        .filter(|part| !part.trim().is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Debug, Default)]
struct InstalledResourceIndex {
    project_ids: BTreeSet<String>,
    titles: BTreeSet<String>,
    hashes: BTreeSet<String>,
    paths: BTreeSet<String>,
    entries: Vec<InstalledResourceEntry>,
}

#[derive(Debug, Clone)]
struct InstalledResourceEntry {
    path: String,
    project_id: Option<String>,
    title: Option<String>,
    sha1: Option<String>,
    category: String,
}

#[derive(Debug, Clone)]
struct InstalledResourceMatch {
    path: String,
    category: String,
}

impl InstalledResourceIndex {
    async fn load(instance_path: &Path) -> Result<Self, AppError> {
        let metadata = load_metadata(instance_path).await.unwrap_or_default();
        let mut index = Self::default();
        for (path, item) in &metadata.items {
            if !metadata_resource_exists(instance_path, path).await {
                continue;
            }
            index
                .paths
                .insert(path.trim_end_matches(".disabled").to_string());
            if let Some(title) = &item.title {
                index.insert_title(title);
            }
            if let Some(project_id) = &item.project_id {
                index.insert_project_id(project_id);
            }
            if let Some(sha1) = &item.sha1 {
                index.insert_sha1(sha1);
            }
            index.entries.push(InstalledResourceEntry {
                path: path.trim_end_matches(".disabled").to_string(),
                project_id: item.project_id.as_deref().map(normalize_resource_identity),
                title: item.title.as_deref().map(normalize_resource_identity),
                sha1: item
                    .sha1
                    .as_deref()
                    .map(|value| value.trim().to_ascii_lowercase()),
                category: item.category.clone(),
            });
        }
        for item in list_mods(instance_path).await.unwrap_or_default() {
            let path = metadata_key(&item.id);
            let title = normalize_resource_identity(&item.name);
            index.paths.insert(path.clone());
            index.insert_title(&item.name);
            index.entries.push(InstalledResourceEntry {
                path,
                project_id: None,
                title: (!title.is_empty()).then_some(title),
                sha1: None,
                category: item.category,
            });
        }
        Ok(index)
    }

    fn contains_any(
        &self,
        project_id: Option<&str>,
        title: Option<&str>,
        sha1: Option<&str>,
        relative_path: Option<&str>,
    ) -> bool {
        project_id
            .map(normalize_resource_identity)
            .filter(|value| !value.is_empty())
            .is_some_and(|value| self.project_ids.contains(&value))
            || title
                .map(normalize_resource_identity)
                .filter(|value| !value.is_empty())
                .is_some_and(|value| self.titles.contains(&value))
            || sha1
                .map(|value| value.trim().to_ascii_lowercase())
                .filter(|value| !value.is_empty())
                .is_some_and(|value| self.hashes.contains(&value))
            || relative_path
                .map(metadata_key)
                .is_some_and(|value| self.paths.contains(&value))
            || relative_path
                .map(pretty_resource_name)
                .map(|value| normalize_resource_identity(&value))
                .filter(|value| !value.is_empty())
                .is_some_and(|value| self.titles.contains(&value))
    }

    fn contains_exact(&self, sha1: Option<&str>, relative_path: Option<&str>) -> bool {
        let sha1 = sha1
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty());
        if let Some(sha1) = sha1 {
            return self.hashes.contains(&sha1);
        }
        relative_path
            .map(metadata_key)
            .is_some_and(|value| self.paths.contains(&value))
    }

    fn matching_replacements(
        &self,
        project_id: Option<&str>,
        title: Option<&str>,
        sha1: Option<&str>,
        relative_path: &str,
    ) -> Vec<InstalledResourceMatch> {
        let identities = resource_identities(project_id, title, relative_path);
        let sha1 = sha1.map(|value| value.trim().to_ascii_lowercase());
        let path = metadata_key(relative_path);

        self.entries
            .iter()
            .filter(|entry| entry.path != path)
            .filter(|entry| entry.sha1.as_deref() != sha1.as_deref())
            .filter(|entry| !entry.identities().is_disjoint(&identities))
            .map(|entry| InstalledResourceMatch {
                path: entry.path.clone(),
                category: entry.category.clone(),
            })
            .collect()
    }

    fn insert_candidate(
        &mut self,
        project_id: Option<&str>,
        title: Option<&str>,
        sha1: Option<&str>,
        relative_path: &str,
        category: &str,
    ) {
        self.paths.insert(metadata_key(relative_path));
        self.insert_project_id_opt(project_id);
        self.insert_title_opt(title);
        self.insert_sha1_opt(sha1);
        self.insert_title(&pretty_resource_name(relative_path));
        self.entries.push(InstalledResourceEntry {
            path: metadata_key(relative_path),
            project_id: project_id.map(normalize_resource_identity),
            title: title.map(normalize_resource_identity),
            sha1: sha1.map(|value| value.trim().to_ascii_lowercase()),
            category: category.to_string(),
        });
    }

    fn insert_project_id_opt(&mut self, project_id: Option<&str>) {
        if let Some(project_id) = project_id {
            self.insert_project_id(project_id);
        }
    }

    fn insert_title_opt(&mut self, title: Option<&str>) {
        if let Some(title) = title {
            self.insert_title(title);
        }
    }

    fn insert_sha1_opt(&mut self, sha1: Option<&str>) {
        if let Some(sha1) = sha1 {
            self.insert_sha1(sha1);
        }
    }

    fn insert_project_id(&mut self, project_id: &str) {
        let normalized = normalize_resource_identity(project_id);
        if !normalized.is_empty() {
            self.project_ids.insert(normalized);
        }
    }

    fn insert_title(&mut self, title: &str) {
        let normalized = normalize_resource_identity(title);
        if !normalized.is_empty() {
            self.titles.insert(normalized);
        }
    }

    fn insert_sha1(&mut self, sha1: &str) {
        let sha1 = sha1.trim().to_ascii_lowercase();
        if !sha1.is_empty() {
            self.hashes.insert(sha1);
        }
    }
}

impl InstalledResourceEntry {
    fn identities(&self) -> BTreeSet<String> {
        let mut identities = BTreeSet::new();
        push_identity(&mut identities, self.project_id.as_deref());
        push_identity(&mut identities, self.title.as_deref());
        push_identity(&mut identities, Some(&pretty_resource_name(&self.path)));
        identities
    }
}

fn resource_identities(
    project_id: Option<&str>,
    title: Option<&str>,
    relative_path: &str,
) -> BTreeSet<String> {
    let mut identities = BTreeSet::new();
    push_identity(&mut identities, project_id);
    push_identity(&mut identities, title);
    push_identity(&mut identities, Some(&pretty_resource_name(relative_path)));
    identities
}

fn push_identity(identities: &mut BTreeSet<String>, value: Option<&str>) {
    if let Some(value) = value {
        let value = normalize_resource_identity(value);
        if !value.is_empty() {
            identities.insert(value);
        }
    }
}

fn normalize_resource_identity(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn strip_common_version_suffix(name: &str) -> &str {
    let mut end = name.len();
    for separator in ['+', '-'] {
        if let Some((head, tail)) = name.rsplit_once(separator) {
            if tail.chars().any(|ch| ch.is_ascii_digit()) {
                end = head.len();
            }
        }
    }
    &name[..end]
}

fn modrinth_project_id_from_cdn_url(url: &str) -> Option<&str> {
    let marker = "/data/";
    let start = url.find(marker)? + marker.len();
    let rest = &url[start..];
    let (project_id, _) = rest.split_once("/versions/")?;
    if project_id.is_empty() {
        None
    } else {
        Some(project_id)
    }
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
        if !metadata
            .categories
            .iter()
            .any(|item| item == &record.category)
        {
            metadata.categories.push(record.category.clone());
        }
        let key = metadata_key(&record.id);
        if let Some(legacy) = legacy_metadata_key(&key) {
            metadata.items.remove(&legacy);
        }
        let entry = metadata.items.entry(key).or_default();
        if let Some(title) = record.title {
            entry.title = Some(title);
        }
        entry.category = record.category;
        if record.icon.is_some() {
            entry.icon = record.icon;
        }
        if record.project_id.is_some() {
            entry.project_id = record.project_id;
        }
        if record.sha1.is_some() {
            entry.sha1 = record.sha1;
        }
    }
    metadata.categories.sort();
    save_metadata(instance_path, &metadata).await
}

async fn remove_metadata_item(instance_path: &Path, id: &str) -> Result<(), AppError> {
    let mut metadata = load_metadata(instance_path).await.unwrap_or_default();
    remove_metadata_keys(&mut metadata, id);
    save_metadata(instance_path, &metadata).await
}

async fn remove_replaced_resources(
    instance_path: &Path,
    paths: &BTreeSet<String>,
) -> Result<(), AppError> {
    if paths.is_empty() {
        return Ok(());
    }
    let mut metadata = load_metadata(instance_path).await.unwrap_or_default();
    for path in paths {
        remove_metadata_keys(&mut metadata, path);
        for candidate in [path.to_string(), format!("{path}.disabled")] {
            if let Ok(file_path) = safe_instance_child(instance_path, &candidate) {
                match tokio::fs::remove_file(file_path).await {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
        }
    }
    save_metadata(instance_path, &metadata).await
}

async fn metadata_resource_exists(instance_path: &Path, key: &str) -> bool {
    let enabled = safe_instance_child(instance_path, key)
        .ok()
        .is_some_and(|path| path.exists());
    if enabled {
        return true;
    }
    let disabled_key = if key.ends_with(".disabled") {
        key.to_string()
    } else {
        format!("{key}.disabled")
    };
    safe_instance_child(instance_path, &disabled_key)
        .ok()
        .is_some_and(|path| path.exists())
}

async fn load_metadata(instance_path: &Path) -> Result<ModsMetadata, AppError> {
    let path = metadata_path(instance_path);
    let bytes = match tokio::fs::read(path).await {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ModsMetadata::default())
        }
        Err(error) => return Err(AppError::Storage(error.to_string())),
    };
    let mut metadata: ModsMetadata = serde_json::from_slice(&bytes)?;
    normalize_metadata_keys(&mut metadata);
    Ok(metadata)
}

fn remove_metadata_keys(metadata: &mut ModsMetadata, id: &str) {
    let key = metadata_key(id);
    metadata.items.remove(&key);
    if let Some(legacy) = legacy_metadata_key(&key) {
        metadata.items.remove(&legacy);
    }
}

fn normalize_metadata_keys(metadata: &mut ModsMetadata) {
    let items = std::mem::take(&mut metadata.items);
    for (key, item) in items {
        let key = metadata_key(&key);
        let entry = metadata.items.entry(key).or_default();
        merge_metadata(entry, item);
    }
}

fn merge_metadata(target: &mut ModMetadata, source: ModMetadata) {
    if target.title.is_none() {
        target.title = source.title;
    }
    if target.icon.is_none() {
        target.icon = source.icon;
    }
    if target.project_id.is_none() {
        target.project_id = source.project_id;
    }
    if target.category.trim().is_empty() || target.category == installed_category() {
        target.category = source.category;
    }
    if target.sha1.is_none() {
        target.sha1 = source.sha1;
    }
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

pub async fn check_mod_installed(instance_path: &Path, project_id: &str) -> Result<bool, AppError> {
    let metadata = load_metadata(instance_path).await.unwrap_or_default();

    // Check if project_id exists in metadata
    for (key, entry) in &metadata.items {
        if let Some(title) = &entry.title {
            // Check if the title matches the project_id (for Modrinth projects)
            if title == project_id {
                return Ok(true);
            }
        }
        // Also check the key itself
        if key.contains(project_id) {
            return Ok(true);
        }
    }

    // Check if mod file exists in mods directory
    let mods_dir = instance_path.join("mods");
    if tokio::fs::metadata(&mods_dir).await.is_ok() {
        let mut entries = tokio::fs::read_dir(&mods_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();
            if name.contains(project_id) {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

pub async fn get_installed_mod_ids(instance_path: &Path) -> Result<BTreeSet<String>, AppError> {
    let metadata = load_metadata(instance_path).await.unwrap_or_default();
    let mut ids = BTreeSet::new();

    for (key, entry) in &metadata.items {
        if let Some(title) = &entry.title {
            ids.insert(title.clone());
        }
        ids.insert(key.clone());
    }

    Ok(ids)
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
    let base = modrinth_base_url();
    let client = modrinth_client();
    let candidates = minecraft_version_candidates(&minecraft_version);
    let mut response = ModrinthSearchResponse { hits: Vec::new() };
    for candidate in candidates {
        let facets = modrinth_facets(kind, loader, &candidate);
        let url = format!(
            "{base}/v2/search?query={}&limit=20&index=downloads&facets={}",
            url_encode(query),
            url_encode(&facets),
        );
        response = client
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .json::<ModrinthSearchResponse>()
            .await?;
        if !response.hits.is_empty() {
            break;
        }
    }
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

fn modrinth_facets(kind: ModrinthKind, loader: Option<&str>, minecraft_version: &str) -> String {
    match (kind, loader) {
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
    }
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
    let mut replacements = BTreeSet::new();
    let mut installed = InstalledResourceIndex::load(&instance_path).await?;
    collect_modrinth_jobs(
        kind,
        &project_id,
        &minecraft_version,
        loader,
        &instance_path,
        &mut installed,
        &mut visited,
        &mut jobs,
        &mut records,
        &mut replacements,
    )
    .await?;
    download_modrinth_jobs_with_status(kind, jobs, status_tx, 0.10, 0.95).await?;
    remove_replaced_resources(&instance_path, &replacements).await?;
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

    let pack_dir = instance_path.join(".swift").join("modpacks");
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
    let mut jobs = Vec::new();
    let mut records = Vec::new();
    let mut replacements = BTreeSet::new();
    let mut installed = InstalledResourceIndex::load(&instance_path).await?;
    let modpack_files = index
        .files
        .into_iter()
        .filter(|file| file.env.client != Some(MrpackSideSupport::Unsupported));
    for file in modpack_files {
        let url = file.downloads.first().cloned().ok_or_else(|| {
            AppError::Download(format!("mrpack file {} has no download URL", file.path))
        })?;
        let destination_path = safe_instance_child(&instance_path, &file.path)?;
        if is_manageable_resource_path(&file.path) {
            let project_id = modrinth_project_id_from_cdn_url(&url).map(str::to_string);
            let metadata = match project_id.as_deref() {
                Some(project_id) => modrinth_project_metadata(project_id).await.ok(),
                None => None,
            };
            let title = metadata
                .as_ref()
                .map(|item| item.title.as_str())
                .unwrap_or_else(|| file.path.as_str());
            let sha1 = file.hashes.sha1.clone();
            if installed.contains_exact(sha1.as_deref(), Some(&file.path)) {
                continue;
            }
            let matches = installed.matching_replacements(
                project_id.as_deref(),
                Some(title),
                sha1.as_deref(),
                &file.path,
            );
            replacements.extend(matches.iter().map(|item| item.path.clone()));
            let category = replacement_category(&matches, &modpack_resource_category(&file.path));
            installed.insert_candidate(
                project_id.as_deref(),
                Some(title),
                sha1.as_deref(),
                &file.path,
                &category,
            );
            jobs.push(DownloadJob {
                id: format!("mrpack:{}", file.path),
                url: url.clone(),
                destination_path,
                expected_sha1: sha1.clone(),
                size_bytes: file.file_size,
            });
            records.push(ResourceInstallRecord {
                id: file.path.clone(),
                title: metadata
                    .as_ref()
                    .map(|item| item.title.clone())
                    .or_else(|| Some(pretty_resource_name(&file.path))),
                category,
                icon: metadata.and_then(|item| item.icon),
                project_id,
                sha1,
            });
        } else {
            jobs.push(DownloadJob {
                id: format!("mrpack:{}", file.path),
                url,
                destination_path,
                expected_sha1: file.hashes.sha1,
                size_bytes: file.file_size,
            });
        }
    }
    download_modrinth_jobs_with_status(ModrinthKind::Modpacks, jobs, status_tx.clone(), 0.22, 0.92)
        .await?;
    remove_replaced_resources(&instance_path, &replacements).await?;
    send_install_status(&status_tx, "Applying modpack overrides", 0.95);
    extract_mrpack_overrides(pack_path, instance_path.clone()).await?;
    upsert_metadata(&instance_path, records).await
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
            author: item
                .authors
                .first()
                .map(|author| author.name.clone())
                .unwrap_or_else(|| "unknown".into()),
            description: item.summary.unwrap_or_default(),
            downloads: item.download_count.unwrap_or_default() as u64,
            icon,
            categories: item
                .categories
                .into_iter()
                .map(|category| category.name)
                .collect(),
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
        let url = screenshot
            .thumbnail_url
            .as_ref()
            .or(screenshot.url.as_ref());
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
    let mut replacements = BTreeSet::new();
    let mut installed = InstalledResourceIndex::load(&instance_path).await?;
    collect_curseforge_jobs(
        api_key,
        kind,
        &project_id,
        &minecraft_version,
        loader,
        &instance_path,
        &mut installed,
        &mut visited,
        &mut jobs,
        &mut records,
        &mut replacements,
    )
    .await?;
    download_modrinth_jobs_with_status(kind, jobs, status_tx, 0.10, 0.95).await?;
    remove_replaced_resources(&instance_path, &replacements).await?;
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
    installed: &mut InstalledResourceIndex,
    visited: &mut BTreeSet<String>,
    jobs: &mut Vec<DownloadJob>,
    records: &mut Vec<ResourceInstallRecord>,
    replacements: &mut BTreeSet<String>,
) -> Result<(), AppError> {
    let client = curseforge_client();
    let mut stack = vec![(project_id.to_string(), false)];
    while let Some((project_id, is_dependency)) = stack.pop() {
        if !visited.insert(project_id.clone()) {
            continue;
        }
        let project = curseforge_mod(&client, api_key, &project_id).await.ok();
        let file =
            compatible_curseforge_file(&client, api_key, &project_id, minecraft_version, loader)
                .await?;
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
        if installed.contains_exact(sha1.as_deref(), Some(&relative_path)) {
            continue;
        }
        let matches = installed.matching_replacements(
            Some(&project_id),
            project.as_ref().map(|item| item.name.as_str()),
            sha1.as_deref(),
            &relative_path,
        );
        replacements.extend(matches.iter().map(|item| item.path.clone()));
        let default_category = resource_category(kind, is_dependency);
        let category = replacement_category(&matches, default_category);
        installed.insert_candidate(
            Some(&project_id),
            project.as_ref().map(|item| item.name.as_str()),
            sha1.as_deref(),
            &relative_path,
            &category,
        );
        jobs.push(DownloadJob {
            id: format!("curseforge:{project_id}:{}", file.file_name),
            url: download_url,
            destination_path: safe_instance_child(instance_path, &relative_path)?,
            expected_sha1: sha1.clone(),
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
            category,
            icon,
            project_id: Some(project_id),
            sha1,
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
    installed: &mut InstalledResourceIndex,
    visited: &mut BTreeSet<String>,
    jobs: &mut Vec<DownloadJob>,
    records: &mut Vec<ResourceInstallRecord>,
    replacements: &mut BTreeSet<String>,
) -> Result<(), AppError> {
    let mut stack: Vec<(String, Option<String>, bool)> =
        vec![(project_id.to_string(), None, false)];
    while let Some((project_id, version_id, is_dependency)) = stack.pop() {
        if !visited.insert(project_id.clone()) {
            continue;
        }
        let metadata = modrinth_project_metadata(&project_id).await.ok();
        let version = match version_id {
            Some(version_id) => modrinth_version_by_id(&version_id).await?,
            None => compatible_modrinth_version(&project_id, minecraft_version, loader).await?,
        };
        for dependency in &version.dependencies {
            if dependency.dependency_type == "required" {
                if let Some(project_id) = &dependency.project_id {
                    if !visited.contains(project_id) {
                        stack.push((project_id.clone(), dependency.version_id.clone(), true));
                    }
                }
            }
        }
        let file = primary_file(version.files).ok_or_else(|| {
            AppError::Download(format!(
                "Modrinth project {project_id} has no downloadable files"
            ))
        })?;
        let relative_path = format!("{}/{}", modrinth_install_dir(kind), file.filename);
        let sha1 = file.hashes.sha1.clone();
        if installed.contains_exact(sha1.as_deref(), Some(&relative_path)) {
            continue;
        }
        let matches = installed.matching_replacements(
            Some(&project_id),
            metadata.as_ref().map(|item| item.title.as_str()),
            sha1.as_deref(),
            &relative_path,
        );
        replacements.extend(matches.iter().map(|item| item.path.clone()));
        let default_category = resource_category(kind, is_dependency);
        let category = replacement_category(&matches, default_category);
        installed.insert_candidate(
            Some(&project_id),
            metadata.as_ref().map(|item| item.title.as_str()),
            sha1.as_deref(),
            &relative_path,
            &category,
        );
        jobs.push(DownloadJob {
            id: format!("modrinth:{project_id}:{}", file.filename),
            url: file.url,
            destination_path: safe_instance_child(instance_path, &relative_path)?,
            expected_sha1: sha1.clone(),
            size_bytes: file.size,
        });
        records.push(ResourceInstallRecord {
            id: relative_path,
            title: metadata.as_ref().map(|item| item.title.clone()),
            category,
            icon: metadata.and_then(|item| item.icon),
            project_id: Some(project_id),
            sha1,
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
    let client = modrinth_client();
    for candidate in minecraft_version_candidates(minecraft_version) {
        let url = if let Some(loader) = loader {
            format!(
                "{base}/v2/project/{}/version?loaders={}&game_versions={}",
                url_encode(project_id),
                url_encode(&format!(r#"["{loader}"]"#)),
                url_encode(&format!(r#"["{candidate}"]"#)),
            )
        } else {
            format!(
                "{base}/v2/project/{}/version?game_versions={}",
                url_encode(project_id),
                url_encode(&format!(r#"["{candidate}"]"#)),
            )
        };
        let versions = client
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .json::<Vec<ModrinthVersion>>()
            .await?;
        if let Some(version) = versions.into_iter().next() {
            return Ok(version);
        }
    }
    Err(AppError::Download(format!(
        "no compatible Modrinth version found for {project_id}"
    )))
}

async fn modrinth_version_by_id(version_id: &str) -> Result<ModrinthVersion, AppError> {
    let base = modrinth_base_url();
    modrinth_client()
        .get(format!("{base}/v2/version/{}", url_encode(version_id)))
        .send()
        .await?
        .error_for_status()?
        .json::<ModrinthVersion>()
        .await
        .map_err(Into::into)
}

fn minecraft_version_candidates(version: &str) -> Vec<String> {
    let trimmed = version.trim();
    let mut candidates = Vec::new();
    push_unique(&mut candidates, trimmed);
    let normalized = trimmed.split_once('-').map_or(trimmed, |(head, _)| head);
    match normalized {
        "26.1" => {
            push_unique(&mut candidates, "26.1.2");
            push_unique(&mut candidates, "26.1.1");
            push_unique(&mut candidates, "26.1.0");
            push_unique(&mut candidates, "1.21.8");
        }
        "26.1.0" | "26.1.1" | "26.1.2" => {
            push_unique(&mut candidates, "26.1");
            push_unique(&mut candidates, "1.21.8");
        }
        _ => {}
    }
    candidates
}

fn push_unique(candidates: &mut Vec<String>, value: &str) {
    if !value.is_empty() && !candidates.iter().any(|candidate| candidate == value) {
        candidates.push(value.to_string());
    }
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
    #[serde(default)]
    project_id: Option<String>,
    #[serde(default)]
    sha1: Option<String>,
}

fn installed_category() -> String {
    "Installed".into()
}

struct ResourceInstallRecord {
    id: String,
    title: Option<String>,
    category: String,
    icon: Option<Vec<u8>>,
    project_id: Option<String>,
    sha1: Option<String>,
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
    #[serde(default)]
    version_id: Option<String>,
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
    use std::sync::OnceLock;
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

    async fn modrinth_env_guard() -> tokio::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
            .lock()
            .await
    }

    #[tokio::test]
    async fn modrinth_collects_required_dependencies() {
        let _env_guard = modrinth_env_guard().await;
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
        let mut replacements = BTreeSet::new();
        let mut installed = InstalledResourceIndex::default();

        let result = collect_modrinth_jobs(
            ModrinthKind::Mods,
            "root",
            "1.20.1",
            Some("fabric"),
            &instance_path,
            &mut installed,
            &mut visited,
            &mut jobs,
            &mut records,
            &mut replacements,
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
    async fn modrinth_replaces_dependency_when_installed_hash_differs() {
        let _env_guard = modrinth_env_guard().await;
        let root_json = json!([
            {
                "files": [
                    {"url": "http://example/not-enough-vulkan.jar", "filename": "not-enough-vulkan.jar", "hashes": {"sha1": "root"}, "primary": true}
                ],
                "dependencies": [
                    {"project_id": "vulkanmod", "version_id": "vulkan-new", "dependency_type": "required"}
                ]
            }
        ]);
        let vulkan_json = json!({
            "files": [
                {"url": "http://example/vulkanmod.jar", "filename": "vulkanmod.jar", "hashes": {"sha1": "vulkan"}, "primary": true}
            ],
            "dependencies": []
        });
        let root_project = json!({
            "id": "root",
            "title": "Not Enough Vulkan",
            "description": "",
            "body": "",
            "downloads": 1,
            "icon_url": null,
            "gallery": []
        });
        let vulkan_project = json!({
            "id": "vulkanmod",
            "title": "VulkanMod",
            "description": "",
            "body": "",
            "downloads": 1,
            "icon_url": null,
            "gallery": []
        });

        let mut routes = HashMap::new();
        routes.insert(
            "/v2/project/root/version".to_string(),
            root_json.to_string().into_bytes(),
        );
        routes.insert(
            "/v2/version/vulkan-new".to_string(),
            vulkan_json.to_string().into_bytes(),
        );
        routes.insert(
            "/v2/project/root".to_string(),
            root_project.to_string().into_bytes(),
        );
        routes.insert(
            "/v2/project/vulkanmod".to_string(),
            vulkan_project.to_string().into_bytes(),
        );

        let (base, handle) = spawn_test_server(routes).await;
        std::env::set_var("SWIFT_LAUNCHER_MODRINTH_BASE", &base);

        let instance_path = temp_dir("modrinth-dedupe");
        let mut visited = BTreeSet::new();
        let mut jobs = Vec::new();
        let mut records = Vec::new();
        let mut replacements = BTreeSet::new();
        let mut installed = InstalledResourceIndex::default();
        installed.insert_candidate(
            Some("vulkanmod"),
            Some("VulkanMod"),
            Some("old-vulkan"),
            "mods/vulkanmod-old.jar",
            "Modpacks",
        );

        let result = collect_modrinth_jobs(
            ModrinthKind::Mods,
            "root",
            "1.20.1",
            Some("fabric"),
            &instance_path,
            &mut installed,
            &mut visited,
            &mut jobs,
            &mut records,
            &mut replacements,
        )
        .await;

        std::env::remove_var("SWIFT_LAUNCHER_MODRINTH_BASE");
        handle.abort();

        assert!(result.is_ok());
        assert_eq!(jobs.len(), 2);
        assert!(jobs.iter().any(|job| job.id.starts_with("modrinth:root:")));
        assert!(jobs.iter().any(|job| job.id.contains("vulkanmod")));
        assert!(
            replacements.contains("mods/vulkanmod-old.jar"),
            "{replacements:?}"
        );
        assert_eq!(records.len(), 2);
        let vulkan_record = records
            .iter()
            .find(|record| record.project_id.as_deref() == Some("vulkanmod"))
            .unwrap();
        assert_eq!(vulkan_record.category, "Modpacks");
    }

    #[tokio::test]
    async fn stale_metadata_does_not_mark_missing_mod_installed() {
        let instance_path = temp_dir("stale-metadata");
        upsert_metadata(
            &instance_path,
            vec![ResourceInstallRecord {
                id: "mods/vulkanmod.jar".into(),
                title: Some("VulkanMod".into()),
                category: "Modpacks".into(),
                icon: None,
                project_id: Some("vulkanmod".into()),
                sha1: Some("deadbeef".into()),
            }],
        )
        .await
        .unwrap();

        let index = InstalledResourceIndex::load(&instance_path).await.unwrap();
        assert!(!index.contains_any(
            Some("vulkanmod"),
            Some("VulkanMod"),
            Some("deadbeef"),
            Some("mods/vulkanmod.jar"),
        ));
    }

    #[tokio::test]
    async fn deleting_mod_removes_metadata_record() {
        let instance_path = temp_dir("delete-metadata");
        std::fs::create_dir_all(instance_path.join("mods")).unwrap();
        std::fs::write(instance_path.join("mods/vulkanmod.jar"), b"jar").unwrap();
        upsert_metadata(
            &instance_path,
            vec![ResourceInstallRecord {
                id: "mods/vulkanmod.jar".into(),
                title: Some("VulkanMod".into()),
                category: "Modpacks".into(),
                icon: None,
                project_id: Some("vulkanmod".into()),
                sha1: None,
            }],
        )
        .await
        .unwrap();

        let mods = delete_mod(instance_path.clone(), "mods/vulkanmod.jar".into())
            .await
            .unwrap();
        let metadata = load_metadata(&instance_path).await.unwrap();

        assert!(mods.is_empty());
        assert!(!metadata.items.contains_key("mods/vulkanmod.jar"));
    }

    #[tokio::test]
    async fn legacy_double_mods_metadata_still_labels_installed_mod() {
        let instance_path = temp_dir("legacy-metadata");
        std::fs::create_dir_all(instance_path.join("mods")).unwrap();
        std::fs::write(instance_path.join("mods/vulkanmod-0.6.3.jar"), b"jar").unwrap();
        let mut metadata = ModsMetadata::default();
        metadata.items.insert(
            "mods/mods/vulkanmod-0.6.3.jar".into(),
            ModMetadata {
                title: Some("VulkanMod".into()),
                category: "Modpacks".into(),
                icon: Some(vec![1, 2, 3]),
                project_id: Some("vulkanmod".into()),
                sha1: Some("abc".into()),
            },
        );
        save_metadata(&instance_path, &metadata).await.unwrap();

        let mods = list_mods(&instance_path).await.unwrap();

        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].name, "VulkanMod");
        assert_eq!(mods[0].category, "Modpacks");
        assert_eq!(mods[0].icon, Some(vec![1, 2, 3]));
    }

    #[test]
    fn dependency_replacement_matches_old_filename_title() {
        let mut installed = InstalledResourceIndex::default();
        installed.insert_candidate(
            None,
            Some("vulkanmod-0.6.3"),
            Some("old-vulkan"),
            "mods/vulkanmod-0.6.3.jar",
            "Modpacks",
        );

        let matches = installed.matching_replacements(
            Some("vulkanmod"),
            Some("VulkanMod"),
            Some("new-vulkan"),
            "mods/vulkanmod-0.6.7.jar",
        );

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].path, "mods/vulkanmod-0.6.3.jar");
        assert_eq!(replacement_category(&matches, "Dependencies"), "Modpacks");
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

    #[test]
    fn mrpack_resource_names_and_modrinth_urls_are_cleaned() {
        assert_eq!(
            modrinth_project_id_from_cdn_url(
                "https://cdn.modrinth.com/data/AANobbMI/versions/1/example.jar"
            ),
            Some("AANobbMI")
        );
        assert_eq!(
            pretty_resource_name("mods/armour-durability-1.2.3+mc1.21.jar"),
            "Armour Durability"
        );
        assert!(is_manageable_resource_path("mods/vulkanmod.jar"));
        assert!(!is_manageable_resource_path("config/options.txt"));
    }
}
