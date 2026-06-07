use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ring::digest;
use serde::Deserialize;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::mpsc;

use crate::auth::{AuthProvider, Session};
use crate::download::java;
use crate::error::AppError;
use crate::instances::{install, Instance, LoaderKind};
use crate::storage::data_dir;

const AUTHLIB_INJECTOR_LATEST_URL: &str = "https://authlib-injector.yushi.moe/artifact/latest.json";

pub async fn validate_java(java_path: &str) -> Result<String, AppError> {
    let output = Command::new(java_path).arg("-version").output().await?;
    let text = String::from_utf8_lossy(&output.stderr).to_string();
    if output.status.success() || text.contains("version") {
        Ok(text.lines().next().unwrap_or("java detected").to_string())
    } else {
        Err(AppError::Process(
            "java executable did not report a version".into(),
        ))
    }
}

pub async fn launch_instance(instance: Instance, session: Session) -> Result<String, AppError> {
    let instance_name = instance.name.clone();
    let (mut command, java_version_line) = prepare_launch_command(instance, session).await?;
    let child = command.spawn()?;
    let pid = child.id().unwrap_or_default();
    drop(child);
    Ok(format!(
        "{instance_name} launched with pid {pid} via {java_version_line}"
    ))
}

pub async fn prepare_launch_command(
    instance: Instance,
    session: Session,
) -> Result<(Command, String), AppError> {
    prepare_launch_command_with_status(instance, session, None).await
}

pub async fn prepare_launch_command_with_status(
    instance: Instance,
    session: Session,
    status_tx: Option<mpsc::UnboundedSender<install::InstallProgress>>,
) -> Result<(Command, String), AppError> {
    send_prepare_status(
        &status_tx,
        format!("Checking Minecraft {}", instance.minecraft_version),
        0.02,
    );
    install::install_minecraft_version_with_status(&instance.minecraft_version, status_tx.clone())
        .await?;

    send_prepare_status(&status_tx, "Resolving launch profile", 0.96);
    let root = data_dir()?;
    let game_dir = effective_game_dir(&instance);
    let effective_version = effective_version_id(&instance)?;
    let version_dir = root.join("versions").join(&effective_version);
    let version_json_path = version_dir.join(format!("{effective_version}.json"));
    let mut version_json = read_version_json(&version_json_path).await?;
    let parent_version = if let Some(parent) = version_json.inherits_from.clone() {
        let parent_path = root
            .join("versions")
            .join(&parent)
            .join(format!("{parent}.json"));
        Some(read_version_json(&parent_path).await?)
    } else {
        None
    };
    if let Some(parent) = &parent_version {
        version_json.merge_parent(parent);
    }
    let required_java = version_json
        .java_version
        .as_ref()
        .map(|version| version.major_version)
        .unwrap_or_else(|| java::required_java_for_minecraft_version(&instance.minecraft_version));
    send_prepare_status(&status_tx, format!("Checking Java {required_java}"), 0.975);
    let java = java::ensure_suitable_java(&instance.java_path, required_java).await?;
    let natives_dir = version_dir.join("natives");
    tokio::fs::create_dir_all(&game_dir).await?;
    tokio::fs::create_dir_all(&natives_dir).await?;

    let libraries_dir = root.join("libraries");
    send_prepare_status(&status_tx, "Extracting native libraries", 0.985);
    extract_natives(&version_json, &libraries_dir, &natives_dir).await?;

    send_prepare_status(&status_tx, "Building launch command", 0.992);
    let classpath = build_classpath(&version_json, &libraries_dir, &root, &effective_version)?;
    let variables = LaunchVariables::new(
        &instance,
        &session,
        &root,
        &game_dir,
        &natives_dir,
        &classpath,
        &version_json,
    );
    let (jvm_args, mut game_args) = build_arguments(&version_json, &variables, &instance)?;
    append_instance_game_options(&instance, &mut game_args);

    let mut command = Command::new(&java.path);
    command.current_dir(&game_dir);
    command.arg(format!("-Xmx{}M", instance.ram_mb));
    command.arg(format!("-Xms{}M", instance.ram_mb.min(1024)));
    if let Some(agent_arg) = authlib_injector_arg(&session, &root).await? {
        command.arg(agent_arg);
    }
    for arg in split_extra_args(&instance.jvm_args) {
        command.arg(arg);
    }
    for arg in jvm_args {
        command.arg(arg);
    }
    command.arg(&version_json.main_class);
    for arg in game_args {
        command.arg(arg);
    }

    send_prepare_status(&status_tx, "Launch command ready", 1.0);
    Ok((command, java.version_line))
}

fn send_prepare_status(
    status_tx: &Option<mpsc::UnboundedSender<install::InstallProgress>>,
    status: impl Into<String>,
    progress: f32,
) {
    if let Some(tx) = status_tx {
        let _ = tx.send(install::InstallProgress {
            status: status.into(),
            progress: progress.clamp(0.0, 1.0),
        });
    }
}

async fn authlib_injector_arg(session: &Session, root: &Path) -> Result<Option<String>, AppError> {
    let server = match session.provider {
        AuthProvider::Microsoft => return Ok(None),
        AuthProvider::ElyBy => "ely.by",
        AuthProvider::LittleSkin => "https://littleskin.cn/api/yggdrasil",
    };
    let jar = root.join("authlib").join("authlib-injector.jar");
    ensure_authlib_injector(&jar).await?;
    Ok(Some(format!("-javaagent:{}={server}", path_string(&jar))))
}

#[derive(Debug, Deserialize)]
struct AuthlibArtifact {
    #[serde(rename = "download_url")]
    download_url: String,
    checksums: AuthlibChecksums,
}

#[derive(Debug, Deserialize)]
struct AuthlibChecksums {
    sha256: String,
}

async fn download_authlib_injector(destination: &Path) -> Result<(), AppError> {
    let client = reqwest::Client::new();
    let artifact = fetch_authlib_artifact(&client).await?;
    download_authlib_artifact(&client, destination, &artifact).await
}

async fn ensure_authlib_injector(destination: &Path) -> Result<(), AppError> {
    let client = reqwest::Client::new();
    let artifact = fetch_authlib_artifact(&client).await?;
    if authlib_matches(destination, &artifact.checksums.sha256).await? {
        return Ok(());
    }
    download_authlib_artifact(&client, destination, &artifact).await
}

async fn fetch_authlib_artifact(client: &reqwest::Client) -> Result<AuthlibArtifact, AppError> {
    Ok(client
        .get(authlib_latest_url())
        .send()
        .await?
        .error_for_status()?
        .json::<AuthlibArtifact>()
        .await?)
}

fn authlib_latest_url() -> String {
    std::env::var("SWIFT_LAUNCHER_AUTHLIB_LATEST_URL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| AUTHLIB_INJECTOR_LATEST_URL.to_string())
}

async fn authlib_matches(destination: &Path, expected_sha256: &str) -> Result<bool, AppError> {
    if tokio::fs::metadata(destination).await.is_err() {
        return Ok(false);
    }
    Ok(sha256_file(destination).await? == expected_sha256.to_lowercase())
}

async fn download_authlib_artifact(
    client: &reqwest::Client,
    destination: &Path,
    artifact: &AuthlibArtifact,
) -> Result<(), AppError> {
    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let temp_path = destination.with_extension("part");
    let mut response = client
        .get(&artifact.download_url)
        .send()
        .await?
        .error_for_status()?;
    let mut context = digest::Context::new(&digest::SHA256);
    let mut file = tokio::fs::File::create(&temp_path).await?;
    while let Some(chunk) = response.chunk().await? {
        context.update(&chunk);
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    drop(file);
    let actual_sha256 = digest_to_hex(context.finish().as_ref());
    if actual_sha256 != artifact.checksums.sha256.to_lowercase() {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return Err(AppError::Download(format!(
            "authlib-injector sha256 mismatch: expected {}, got {actual_sha256}",
            artifact.checksums.sha256
        )));
    }
    let _ = tokio::fs::remove_file(destination).await;
    tokio::fs::rename(&temp_path, destination).await?;
    Ok(())
}

async fn sha256_file(path: &Path) -> Result<String, AppError> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        use std::io::Read;
        let mut file =
            std::fs::File::open(path).map_err(|error| AppError::Download(error.to_string()))?;
        let mut context = digest::Context::new(&digest::SHA256);
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = file
                .read(&mut buffer)
                .map_err(|error| AppError::Download(error.to_string()))?;
            if read == 0 {
                break;
            }
            context.update(&buffer[..read]);
        }
        Ok(digest_to_hex(context.finish().as_ref()))
    })
    .await
    .map_err(|error| AppError::Download(error.to_string()))?
}

fn sha256_hex(bytes: &[u8]) -> String {
    let hash = digest::digest(&digest::SHA256, bytes);
    digest_to_hex(hash.as_ref())
}

fn digest_to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use std::time::{SystemTime, UNIX_EPOCH};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    async fn spawn_test_server(
        routes: HashMap<String, Vec<u8>>,
    ) -> (String, Arc<AtomicUsize>, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let routes = Arc::new(routes);
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_for_task = hits.clone();
        let handle = tokio::spawn(async move {
            loop {
                let (mut socket, _) = match listener.accept().await {
                    Ok(value) => value,
                    Err(_) => break,
                };
                hits_for_task.fetch_add(1, Ordering::Relaxed);
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
        (format!("http://{}", addr), hits, handle)
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
    async fn authlib_matches_sha256() {
        let dir = temp_dir("authlib-match");
        let jar = dir.join("authlib-injector.jar");
        tokio::fs::write(&jar, b"good-jar").await.unwrap();
        assert!(authlib_matches(&jar, &sha256_hex(b"good-jar"))
            .await
            .unwrap());
        assert!(!authlib_matches(&jar, &sha256_hex(b"bad-jar"))
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn authlib_redownloads_corrupt_existing_jar() {
        let dir = temp_dir("authlib-redownload");
        let jar = dir.join("authlib-injector.jar");
        tokio::fs::write(&jar, b"corrupt").await.unwrap();
        let good = b"good-jar".to_vec();
        let latest = serde_json::json!({
            "download_url": "http://placeholder/authlib.jar",
            "checksums": {"sha256": sha256_hex(&good)}
        })
        .to_string();

        let mut routes = HashMap::new();
        routes.insert("/latest.json".to_string(), latest.into_bytes());
        routes.insert("/authlib.jar".to_string(), good.clone());
        let (base, hits, handle) = spawn_test_server(routes).await;
        let artifact = AuthlibArtifact {
            download_url: format!("{base}/authlib.jar"),
            checksums: AuthlibChecksums {
                sha256: sha256_hex(&good),
            },
        };

        ensure_authlib_injector_with_artifact(&jar, artifact)
            .await
            .unwrap();
        handle.abort();

        assert_eq!(tokio::fs::read(&jar).await.unwrap(), good);
        assert!(hits.load(Ordering::Relaxed) >= 1);
    }

    async fn ensure_authlib_injector_with_artifact(
        destination: &Path,
        artifact: AuthlibArtifact,
    ) -> Result<(), AppError> {
        let client = reqwest::Client::new();
        if authlib_matches(destination, &artifact.checksums.sha256).await? {
            return Ok(());
        }
        download_authlib_artifact(&client, destination, &artifact).await
    }
}

#[derive(Debug, Deserialize)]
struct VersionJson {
    id: String,
    #[serde(rename = "inheritsFrom")]
    inherits_from: Option<String>,
    #[serde(rename = "type", default)]
    version_type: String,
    #[serde(rename = "mainClass", default)]
    main_class: String,
    #[serde(rename = "javaVersion")]
    java_version: Option<JavaVersion>,
    #[serde(default, rename = "assetIndex")]
    asset_index: AssetIndexInfo,
    #[serde(default)]
    libraries: Vec<Library>,
    arguments: Option<Arguments>,
    #[serde(rename = "minecraftArguments")]
    minecraft_arguments: Option<String>,
}

impl VersionJson {
    fn merge_parent(&mut self, parent: &VersionJson) {
        if self.version_type.is_empty() {
            self.version_type = parent.version_type.clone();
        }
        if self.java_version.is_none() {
            self.java_version = parent.java_version.clone();
        }
        if self.asset_index.id.is_empty() {
            self.asset_index = parent.asset_index.clone();
        }
        match (&mut self.arguments, &parent.arguments) {
            (Some(child), Some(parent_args)) => child.prepend_parent(parent_args),
            (None, Some(parent_args)) => self.arguments = Some(parent_args.clone()),
            _ => {}
        }
        if self.minecraft_arguments.is_none() {
            self.minecraft_arguments = parent.minecraft_arguments.clone();
        }
        if self.main_class.is_empty() {
            self.main_class = parent.main_class.clone();
        }
        let mut libraries = parent.libraries.clone();
        libraries.extend(self.libraries.clone());
        self.libraries = libraries;
    }
}

#[derive(Debug, Clone, Deserialize)]
struct JavaVersion {
    #[serde(rename = "majorVersion")]
    major_version: u32,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct AssetIndexInfo {
    #[serde(default)]
    id: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Arguments {
    #[serde(default)]
    game: Vec<Argument>,
    #[serde(default)]
    jvm: Vec<Argument>,
}

impl Arguments {
    fn prepend_parent(&mut self, parent: &Arguments) {
        let mut game = parent.game.clone();
        game.extend(self.game.clone());
        self.game = game;

        let mut jvm = parent.jvm.clone();
        jvm.extend(self.jvm.clone());
        self.jvm = jvm;
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum Argument {
    String(String),
    Ruled {
        rules: Option<Vec<Rule>>,
        value: ArgumentValue,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum ArgumentValue {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Clone, Deserialize)]
struct Library {
    name: String,
    downloads: Option<LibraryDownloads>,
    #[serde(default)]
    url: Option<String>,
    rules: Option<Vec<Rule>>,
    natives: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize)]
struct LibraryDownloads {
    artifact: Option<LibraryArtifact>,
    classifiers: Option<BTreeMap<String, LibraryArtifact>>,
}

#[derive(Debug, Clone, Deserialize)]
struct LibraryArtifact {
    path: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Rule {
    action: String,
    os: Option<RuleOs>,
    features: Option<BTreeMap<String, bool>>,
}

#[derive(Debug, Clone, Deserialize)]
struct RuleOs {
    name: Option<String>,
}

struct LaunchVariables {
    values: BTreeMap<String, String>,
}

impl LaunchVariables {
    fn new(
        instance: &Instance,
        session: &Session,
        root: &Path,
        game_dir: &Path,
        natives_dir: &Path,
        classpath: &str,
        version_json: &VersionJson,
    ) -> Self {
        let user_type = match session.provider {
            AuthProvider::Microsoft => "msa",
            AuthProvider::ElyBy | AuthProvider::LittleSkin => "legacy",
        };

        let mut values = BTreeMap::new();
        values.insert("auth_player_name".into(), session.username.clone());
        values.insert("version_name".into(), instance.minecraft_version.clone());
        values.insert("game_directory".into(), path_string(game_dir));
        values.insert("assets_root".into(), path_string(&root.join("assets")));
        values.insert(
            "assets_index_name".into(),
            version_json.asset_index.id.clone(),
        );
        values.insert("auth_uuid".into(), session.uuid.clone());
        values.insert("auth_access_token".into(), session.access_token.clone());
        values.insert("clientid".into(), String::new());
        values.insert("auth_xuid".into(), String::new());
        values.insert("user_type".into(), user_type.into());
        values.insert("version_type".into(), version_json.version_type.clone());
        values.insert("natives_directory".into(), path_string(natives_dir));
        values.insert("launcher_name".into(), "SwiftLauncher".into());
        values.insert("launcher_version".into(), env!("CARGO_PKG_VERSION").into());
        values.insert("classpath".into(), classpath.to_string());
        values.insert("classpath_separator".into(), classpath_separator().into());
        values.insert(
            "library_directory".into(),
            path_string(&root.join("libraries")),
        );
        values.insert(
            "resolution_width".into(),
            instance.resolution_width.to_string(),
        );
        values.insert(
            "resolution_height".into(),
            instance.resolution_height.to_string(),
        );

        Self { values }
    }

    fn apply(&self, input: &str) -> String {
        let mut out = input.to_string();
        for (key, value) in &self.values {
            out = out.replace(&format!("${{{key}}}"), value);
        }
        out
    }
}

async fn read_version_json(path: &Path) -> Result<VersionJson, AppError> {
    let bytes = tokio::fs::read(path).await?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn build_arguments(
    version: &VersionJson,
    variables: &LaunchVariables,
    instance: &Instance,
) -> Result<(Vec<String>, Vec<String>), AppError> {
    let features = LaunchFeatures::from_instance(instance);
    if let Some(arguments) = &version.arguments {
        let jvm = expand_arguments(&arguments.jvm, variables, &features);
        let game = expand_arguments(&arguments.game, variables, &features);
        return Ok((jvm, game));
    }

    let game = version
        .minecraft_arguments
        .as_deref()
        .unwrap_or("")
        .split_whitespace()
        .map(|arg| variables.apply(arg))
        .collect::<Vec<_>>();
    let jvm = [
        "-Djava.library.path=${natives_directory}",
        "-cp",
        "${classpath}",
    ]
    .into_iter()
    .map(|arg| variables.apply(arg))
    .collect();
    Ok((jvm, game))
}

fn expand_arguments(
    arguments: &[Argument],
    variables: &LaunchVariables,
    features: &LaunchFeatures,
) -> Vec<String> {
    let mut out = Vec::new();
    for argument in arguments {
        match argument {
            Argument::String(value) => out.push(variables.apply(value)),
            Argument::Ruled { rules, value } if rules_allowed(rules.as_deref(), features) => {
                match value {
                    ArgumentValue::One(value) => out.push(variables.apply(value)),
                    ArgumentValue::Many(values) => {
                        out.extend(values.iter().map(|value| variables.apply(value)))
                    }
                }
            }
            Argument::Ruled { .. } => {}
        }
    }
    out
}

fn build_classpath(
    version: &VersionJson,
    libraries_dir: &Path,
    root: &Path,
    version_id: &str,
) -> Result<String, AppError> {
    let mut entries = Vec::new();
    for library in &version.libraries {
        if !library_allowed(library) {
            continue;
        }
        let path = if let Some(ref downloads) = library.downloads {
            if let Some(ref artifact) = downloads.artifact {
                artifact.path.clone()
            } else {
                continue;
            }
        } else if library.url.is_some() {
            maven_name_to_path(&library.name)
        } else {
            continue;
        };
        entries.push(path_string(&libraries_dir.join(&path)));
    }
    let client_version = version.inherits_from.as_deref().unwrap_or(version_id);
    entries.push(path_string(
        &root
            .join("versions")
            .join(client_version)
            .join(format!("{client_version}.jar")),
    ));
    Ok(entries.join(classpath_separator()))
}

fn maven_name_to_path(name: &str) -> String {
    let parts: Vec<&str> = name.split(':').collect();
    let group = parts.first().unwrap_or(&"");
    let artifact = parts.get(1).unwrap_or(&"");
    let version = parts.get(2).unwrap_or(&"");
    let group_path = group.replace('.', "/");
    format!("{group_path}/{artifact}/{version}/{artifact}-{version}.jar")
}

fn effective_version_id(instance: &Instance) -> Result<String, AppError> {
    match instance.loader {
        LoaderKind::Vanilla => Ok(instance.minecraft_version.clone()),
        LoaderKind::Fabric | LoaderKind::Quilt | LoaderKind::Forge | LoaderKind::NeoForge => {
            instance.loader_version.clone().ok_or_else(|| {
                AppError::Instance(format!(
                    "{} profile missing for Minecraft {}. Reinstall this instance.",
                    instance.loader, instance.minecraft_version
                ))
            })
        }
    }
}

async fn extract_natives(
    version: &VersionJson,
    libraries_dir: &Path,
    natives_dir: &Path,
) -> Result<(), AppError> {
    let mut native_jars = Vec::new();
    for library in &version.libraries {
        if !library_allowed(library) {
            continue;
        }
        let Some(classifier_name) = native_classifier_for_current_os(library) else {
            continue;
        };
        let Some(artifact) = library
            .downloads
            .as_ref()
            .and_then(|downloads| downloads.classifiers.as_ref())
            .and_then(|classifiers| classifiers.get(&classifier_name))
        else {
            continue;
        };
        native_jars.push(libraries_dir.join(&artifact.path));
    }

    let natives_dir = natives_dir.to_path_buf();
    tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&natives_dir).map_err(|e| AppError::Process(e.to_string()))?;
        for jar in native_jars {
            extract_native_jar(&jar, &natives_dir)?;
        }
        Ok::<(), AppError>(())
    })
    .await
    .map_err(|e| AppError::Process(e.to_string()))?
}

fn extract_native_jar(jar_path: &Path, destination: &Path) -> Result<(), AppError> {
    let file = std::fs::File::open(jar_path).map_err(|e| AppError::Process(e.to_string()))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| AppError::Process(e.to_string()))?;
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|e| AppError::Process(e.to_string()))?;
        let name = file.name();
        if name.ends_with('/') || name.starts_with("META-INF/") {
            continue;
        }
        let Some(file_name) = Path::new(name).file_name() else {
            continue;
        };
        let output_path = destination.join(file_name);
        let mut output =
            std::fs::File::create(output_path).map_err(|e| AppError::Process(e.to_string()))?;
        std::io::copy(&mut file, &mut output).map_err(|e| AppError::Process(e.to_string()))?;
    }
    Ok(())
}

fn library_allowed(library: &Library) -> bool {
    rules_allowed(library.rules.as_deref(), &LaunchFeatures::default())
}

fn rules_allowed(rules: Option<&[Rule]>, features: &LaunchFeatures) -> bool {
    let Some(rules) = rules else {
        return true;
    };

    let mut allowed = false;
    for rule in rules {
        if rule_matches_current_context(rule, features) {
            allowed = rule.action == "allow";
        }
    }
    allowed
}

fn rule_matches_current_context(rule: &Rule, features: &LaunchFeatures) -> bool {
    if let Some(required) = &rule.features {
        for (name, expected) in required {
            if features.get(name) != *expected {
                return false;
            }
        }
    }

    let Some(os) = &rule.os else {
        return true;
    };
    let Some(name) = &os.name else {
        return true;
    };
    name == current_os_name()
}

#[derive(Debug, Clone, Copy, Default)]
struct LaunchFeatures {
    has_custom_resolution: bool,
}

impl LaunchFeatures {
    fn from_instance(instance: &Instance) -> Self {
        Self {
            has_custom_resolution: instance.resolution_width > 0 && instance.resolution_height > 0,
        }
    }

    fn get(&self, name: &str) -> bool {
        match name {
            "has_custom_resolution" => self.has_custom_resolution,
            "is_demo_user" => false,
            "has_quick_plays_support" => false,
            "is_quick_play_singleplayer" => false,
            "is_quick_play_multiplayer" => false,
            "is_quick_play_realms" => false,
            _ => false,
        }
    }
}

fn effective_game_dir(instance: &Instance) -> PathBuf {
    if instance.game_dir_override.trim().is_empty() {
        instance.path.clone()
    } else {
        PathBuf::from(instance.game_dir_override.trim())
    }
}

fn append_instance_game_options(instance: &Instance, game_args: &mut Vec<String>) {
    if instance.fullscreen && !game_args.iter().any(|arg| arg == "--fullscreen") {
        game_args.push("--fullscreen".into());
    }

    let world = instance.quick_play_world.trim();
    if !world.is_empty() {
        game_args.push("--quickPlaySingleplayer".into());
        game_args.push(world.into());
        return;
    }

    let quick_server = instance.quick_play_server.trim();
    if !quick_server.is_empty() {
        game_args.push("--quickPlayMultiplayer".into());
        game_args.push(quick_server.into());
        return;
    }

    let server = instance.server.trim();
    if server.is_empty() {
        return;
    }
    let (host, port) = split_server(server);
    if !host.is_empty() {
        game_args.push("--server".into());
        game_args.push(host.into());
    }
    if let Some(port) = port {
        game_args.push("--port".into());
        game_args.push(port.to_string());
    }
}

fn split_server(server: &str) -> (&str, Option<u16>) {
    let Some((host, port)) = server.rsplit_once(':') else {
        return (server, None);
    };
    match port.parse::<u16>() {
        Ok(port) => (host, Some(port)),
        Err(_) => (server, None),
    }
}

fn native_classifier_for_current_os(library: &Library) -> Option<String> {
    let classifier = library.natives.as_ref()?.get(current_os_name())?;
    Some(classifier.replace("${arch}", current_arch_bits()))
}

fn split_extra_args(input: &str) -> Vec<String> {
    input.split_whitespace().map(str::to_string).collect()
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn classpath_separator() -> &'static str {
    if cfg!(target_os = "windows") {
        ";"
    } else {
        ":"
    }
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

fn current_arch_bits() -> &'static str {
    if std::mem::size_of::<usize>() == 8 {
        "64"
    } else {
        "32"
    }
}
