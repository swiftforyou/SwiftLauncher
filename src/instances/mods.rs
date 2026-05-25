use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::download::{download_jobs_checked, DownloadJob};
use crate::error::AppError;
use crate::instances::LoaderKind;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledMod {
    pub id: String,
    pub name: String,
    pub version: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModrinthProject {
    pub project_id: String,
    pub title: String,
    pub description: String,
    pub downloads: u64,
}

pub async fn list_mods(instance_path: &Path) -> Result<Vec<InstalledMod>, AppError> {
    let mut mods = Vec::new();
    let mods_dir = instance_path.join("mods");
    if tokio::fs::metadata(&mods_dir).await.is_err() {
        return Ok(mods);
    }
    let mut entries = tokio::fs::read_dir(mods_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.ends_with(".jar") || name.ends_with(".jar.disabled") {
            mods.push(InstalledMod {
                id: name.to_string(),
                name: name.trim_end_matches(".disabled").trim_end_matches(".jar").to_string(),
                version: "local".into(),
                enabled: name.ends_with(".jar"),
            });
        }
    }
    mods.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(mods)
}

pub async fn set_mod_enabled(instance_path: PathBuf, mod_id: String, enabled: bool) -> Result<Vec<InstalledMod>, AppError> {
    let mods_dir = instance_path.join("mods");
    let source = mod_path(&mods_dir, &mod_id)?;
    let file_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::Instance("invalid mod file name".into()))?;
    let target = if enabled {
        if file_name.ends_with(".jar.disabled") {
            mods_dir.join(file_name.trim_end_matches(".disabled"))
        } else {
            source.clone()
        }
    } else if file_name.ends_with(".jar") {
        mods_dir.join(format!("{file_name}.disabled"))
    } else {
        source.clone()
    };

    if source != target {
        if tokio::fs::metadata(&target).await.is_ok() {
            return Err(AppError::Instance(format!("target mod file already exists: {}", target.display())));
        }
        tokio::fs::rename(source, target).await?;
    }
    list_mods(&instance_path).await
}

pub async fn delete_mod(instance_path: PathBuf, mod_id: String) -> Result<Vec<InstalledMod>, AppError> {
    let mods_dir = instance_path.join("mods");
    let path = mod_path(&mods_dir, &mod_id)?;
    tokio::fs::remove_file(path).await?;
    list_mods(&instance_path).await
}

pub async fn import_mod(instance_path: PathBuf, source: PathBuf) -> Result<Vec<InstalledMod>, AppError> {
    let file_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::Instance("invalid source mod file name".into()))?;
    if !file_name.ends_with(".jar") {
        return Err(AppError::Instance("mod import only accepts .jar files".into()));
    }
    let mods_dir = instance_path.join("mods");
    tokio::fs::create_dir_all(&mods_dir).await?;
    let target = mods_dir.join(file_name);
    if tokio::fs::metadata(&target).await.is_ok() {
        return Err(AppError::Instance(format!("mod already exists: {file_name}")));
    }
    tokio::fs::copy(source, target).await?;
    list_mods(&instance_path).await
}

pub async fn search_modrinth(query: String, minecraft_version: String, loader: LoaderKind) -> Result<Vec<ModrinthProject>, AppError> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let loader = modrinth_loader(loader)?;
    let facets = format!(r#"[["project_type:mod"],["versions:{minecraft_version}"],["categories:{loader}"]]"#);
    let base = modrinth_base_url();
    let url = format!(
        "{base}/v2/search?query={}&limit=12&index=relevance&facets={}",
        url_encode(query),
        url_encode(&facets),
    );
    let response = modrinth_client()
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<ModrinthSearchResponse>()
        .await?;
    Ok(response
        .hits
        .into_iter()
        .map(|hit| ModrinthProject {
            project_id: hit.project_id,
            title: hit.title,
            description: hit.description,
            downloads: hit.downloads,
        })
        .collect())
}

pub async fn install_modrinth_project(
    instance_path: PathBuf,
    minecraft_version: String,
    loader: LoaderKind,
    project_id: String,
) -> Result<Vec<InstalledMod>, AppError> {
    let loader = modrinth_loader(loader)?;
    let mut visited = BTreeSet::new();
    let mut jobs = Vec::new();
    collect_modrinth_jobs(&project_id, &minecraft_version, loader, &instance_path, &mut visited, &mut jobs).await?;
    download_jobs_checked(jobs).await?;
    list_mods(&instance_path).await
}

async fn collect_modrinth_jobs(
    project_id: &str,
    minecraft_version: &str,
    loader: &str,
    instance_path: &Path,
    visited: &mut BTreeSet<String>,
    jobs: &mut Vec<DownloadJob>,
) -> Result<(), AppError> {
    let mut stack = vec![project_id.to_string()];
    while let Some(project_id) = stack.pop() {
        if !visited.insert(project_id.clone()) {
            continue;
        }
        let version = compatible_modrinth_version(&project_id, minecraft_version, loader).await?;
        for dependency in &version.dependencies {
            if dependency.dependency_type == "required" {
                if let Some(project_id) = &dependency.project_id {
                    if !visited.contains(project_id) {
                        stack.push(project_id.clone());
                    }
                }
            }
        }
        let file = primary_file(version.files)
            .ok_or_else(|| AppError::Download(format!("Modrinth project {project_id} has no downloadable files")))?;
        jobs.push(DownloadJob {
            id: format!("modrinth:{project_id}:{}", file.filename),
            url: file.url,
            destination_path: instance_path.join("mods").join(&file.filename),
            expected_sha1: file.hashes.sha1,
            size_bytes: file.size,
        });
    }
    Ok(())
}

async fn compatible_modrinth_version(project_id: &str, minecraft_version: &str, loader: &str) -> Result<ModrinthVersion, AppError> {
    let base = modrinth_base_url();
    let url = format!(
        "{base}/v2/project/{}/version?loaders={}&game_versions={}",
        url_encode(project_id),
        url_encode(&format!(r#"["{loader}"]"#)),
        url_encode(&format!(r#"["{minecraft_version}"]"#)),
    );
    modrinth_client()
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<ModrinthVersion>>()
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| AppError::Download(format!("no compatible Modrinth version found for {project_id}")))
}

fn primary_file(files: Vec<ModrinthFile>) -> Option<ModrinthFile> {
    let first = files.first().cloned();
    files.into_iter().find(|file| file.primary).or(first)
}

fn mod_path(mods_dir: &Path, mod_id: &str) -> Result<PathBuf, AppError> {
    if mod_id.contains('/') || mod_id.contains('\\') || mod_id == "." || mod_id == ".." {
        return Err(AppError::Instance("invalid mod id".into()));
    }
    if !(mod_id.ends_with(".jar") || mod_id.ends_with(".jar.disabled")) {
        return Err(AppError::Instance("mod file must be .jar or .jar.disabled".into()));
    }
    Ok(mods_dir.join(mod_id))
}

#[derive(Debug, Deserialize)]
struct ModrinthSearchResponse {
    hits: Vec<ModrinthHit>,
}

#[derive(Debug, Deserialize)]
struct ModrinthHit {
    project_id: String,
    title: String,
    description: String,
    downloads: u64,
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

fn modrinth_loader(loader: LoaderKind) -> Result<&'static str, AppError> {
    match loader {
        LoaderKind::Fabric => Ok("fabric"),
        LoaderKind::Quilt => Ok("quilt"),
        LoaderKind::Forge => Ok("forge"),
        LoaderKind::NeoForge => Ok("neoforge"),
        LoaderKind::Vanilla => Err(AppError::Instance("Modrinth mod install needs a mod loader instance".into())),
    }
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

fn url_encode(input: &str) -> String {
    let mut out = String::new();
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
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
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    async fn spawn_test_server(routes: HashMap<String, Vec<u8>>) -> (String, tokio::task::JoinHandle<()>) {
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
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                        body.len()
                    );
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
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
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
        routes.insert("/v2/project/root/version".to_string(), root_json.to_string().into_bytes());
        routes.insert("/v2/project/lib-a/version".to_string(), lib_a_json.to_string().into_bytes());
        routes.insert("/v2/project/lib-b/version".to_string(), lib_b_json.to_string().into_bytes());

        let (base, handle) = spawn_test_server(routes).await;
        std::env::set_var("SWIFT_LAUNCHER_MODRINTH_BASE", &base);

        let instance_path = temp_dir("modrinth-deps");
        let mut visited = BTreeSet::new();
        let mut jobs = Vec::new();

        let result = collect_modrinth_jobs(
            "root",
            "1.20.1",
            "fabric",
            &instance_path,
            &mut visited,
            &mut jobs,
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
}
