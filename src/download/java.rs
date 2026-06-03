use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::error::AppError;
use crate::storage::data_dir;

#[derive(Debug, Clone)]
pub struct JavaInfo {
    pub path: PathBuf,
    pub major: u32,
    pub version_line: String,
}

pub async fn detect_java(java_path: impl AsRef<Path>) -> Result<JavaInfo, AppError> {
    let java_path = java_path.as_ref();
    let output = Command::new(java_path).arg("-version").output().await?;
    let text = if output.stderr.is_empty() {
        String::from_utf8_lossy(&output.stdout).to_string()
    } else {
        String::from_utf8_lossy(&output.stderr).to_string()
    };
    let version_line = text
        .lines()
        .next()
        .unwrap_or("java version unknown")
        .to_string();
    let major = parse_java_major(&version_line).ok_or_else(|| {
        AppError::Process(format!(
            "could not parse Java version from `{version_line}`"
        ))
    })?;

    Ok(JavaInfo {
        path: java_path.to_path_buf(),
        major,
        version_line,
    })
}

pub async fn ensure_suitable_java(
    configured_java: &str,
    required_major: u32,
) -> Result<JavaInfo, AppError> {
    if let Some(info) = managed_java(required_major).await? {
        return Ok(info);
    }

    if let Ok(info) = detect_java(configured_java).await {
        if java_is_suitable(info.major, required_major) {
            return Ok(info);
        }
    }

    for candidate in java_candidates(required_major) {
        if let Ok(info) = detect_java(&candidate).await {
            if java_is_suitable(info.major, required_major) {
                return Ok(info);
            }
        }
    }

    download_managed_java(required_major).await?;
    managed_java(required_major).await?.ok_or_else(|| {
        AppError::Process(format!(
            "Java {required_major} was downloaded but no java binary was found"
        ))
    })
}

pub async fn suitable_java_or_prompt(
    java_path: &str,
    minecraft_version: &str,
) -> Result<String, AppError> {
    let required = required_java_for_minecraft_version(minecraft_version);
    let info = ensure_suitable_java(java_path, required).await?;
    Ok(info.path.to_string_lossy().to_string())
}

pub fn required_java_for_minecraft_version(version: &str) -> u32 {
    let mut parts = version.split('.');
    let major = parts
        .next()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(1);
    let minor = parts
        .next()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0);
    let patch = parts
        .next()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0);

    if major >= 26 {
        25
    } else if major > 1 || minor >= 20 && patch >= 5 || minor >= 21 {
        21
    } else if minor >= 18 {
        17
    } else if minor >= 17 {
        16
    } else {
        8
    }
}

fn parse_java_major(line: &str) -> Option<u32> {
    let quoted = line.split('"').nth(1)?;
    if let Some(rest) = quoted.strip_prefix("1.") {
        return rest.split('.').next()?.parse().ok();
    }
    quoted.split('.').next()?.parse().ok()
}

fn java_is_suitable(found: u32, required: u32) -> bool {
    if required >= 21 {
        found == required
    } else {
        found >= required
    }
}

async fn managed_java(required_major: u32) -> Result<Option<JavaInfo>, AppError> {
    let java = managed_java_dir(required_major)?
        .join("bin")
        .join(java_binary_name());
    if tokio::fs::metadata(&java).await.is_ok() {
        return detect_java(java).await.map(Some);
    }
    Ok(None)
}

pub async fn download_managed_java(required_major: u32) -> Result<JavaInfo, AppError> {
    let url = adoptium_binary_url(required_major)?;
    let archive_path = data_dir()?
        .join("java")
        .join(format!("temurin-{required_major}-{}", archive_extension()));
    if let Some(parent) = archive_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let mut response = reqwest::Client::new()
        .get(url)
        .send()
        .await?
        .error_for_status()?;
    let mut file = tokio::fs::File::create(&archive_path).await?;
    while let Some(chunk) = response.chunk().await? {
        file.write_all(&chunk).await?;
    }
    file.flush().await?;

    let destination = managed_java_dir(required_major)?;
    let archive_path_for_extract = archive_path.clone();
    tokio::task::spawn_blocking(move || {
        extract_java_archive(&archive_path_for_extract, &destination)
    })
    .await
    .map_err(|error| AppError::Process(error.to_string()))??;
    managed_java(required_major).await?.ok_or_else(|| {
        AppError::Process(format!(
            "Java {required_major} was downloaded but no java binary was found"
        ))
    })
}

fn extract_java_archive(archive_path: &Path, destination: &Path) -> Result<(), AppError> {
    if destination.exists() {
        std::fs::remove_dir_all(destination)
            .map_err(|error| AppError::Process(error.to_string()))?;
    }
    std::fs::create_dir_all(destination).map_err(|error| AppError::Process(error.to_string()))?;

    if archive_path.extension().and_then(|ext| ext.to_str()) == Some("zip") {
        extract_zip(archive_path, destination)
    } else {
        extract_tar_gz(archive_path, destination)
    }
}

fn extract_tar_gz(archive_path: &Path, destination: &Path) -> Result<(), AppError> {
    let file =
        std::fs::File::open(archive_path).map_err(|error| AppError::Process(error.to_string()))?;
    let decoder = GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    for entry in archive
        .entries()
        .map_err(|error| AppError::Process(error.to_string()))?
    {
        let mut entry = entry.map_err(|error| AppError::Process(error.to_string()))?;
        let path = entry
            .path()
            .map_err(|error| AppError::Process(error.to_string()))?;
        let stripped = strip_first_component(&path);
        if stripped.as_os_str().is_empty() {
            continue;
        }
        let target = destination.join(stripped);
        entry
            .unpack(target)
            .map_err(|error| AppError::Process(error.to_string()))?;
    }
    Ok(())
}

fn extract_zip(archive_path: &Path, destination: &Path) -> Result<(), AppError> {
    let file =
        std::fs::File::open(archive_path).map_err(|error| AppError::Process(error.to_string()))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|error| AppError::Process(error.to_string()))?;
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|error| AppError::Process(error.to_string()))?;
        let Some(enclosed) = file.enclosed_name() else {
            continue;
        };
        let stripped = strip_first_component(&enclosed);
        if stripped.as_os_str().is_empty() {
            continue;
        }
        let target = destination.join(stripped);
        if file.is_dir() {
            std::fs::create_dir_all(target)
                .map_err(|error| AppError::Process(error.to_string()))?;
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| AppError::Process(error.to_string()))?;
            }
            let mut output = std::fs::File::create(target)
                .map_err(|error| AppError::Process(error.to_string()))?;
            std::io::copy(&mut file, &mut output)
                .map_err(|error| AppError::Process(error.to_string()))?;
        }
    }
    Ok(())
}

fn strip_first_component(path: &Path) -> PathBuf {
    path.components().skip(1).collect()
}

pub fn managed_java_root() -> Result<PathBuf, AppError> {
    Ok(data_dir()?.join("java"))
}

pub fn managed_java_dir(required_major: u32) -> Result<PathBuf, AppError> {
    Ok(data_dir()?
        .join("java")
        .join(format!("temurin-{required_major}")))
}

fn java_candidates(required_major: u32) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(java_home) = std::env::var_os("JAVA_HOME") {
        candidates.push(
            PathBuf::from(java_home)
                .join("bin")
                .join(java_binary_name()),
        );
    }
    candidates.push(PathBuf::from("java"));
    candidates.push(PathBuf::from(format!(
        "/usr/lib/jvm/java-{required_major}-openjdk/bin/java"
    )));
    candidates.push(PathBuf::from(format!(
        "/usr/lib/jvm/java-{required_major}-openjdk-amd64/bin/java"
    )));
    candidates.push(PathBuf::from(format!(
        "/usr/lib/jvm/temurin-{required_major}-jdk/bin/java"
    )));
    candidates
}

fn adoptium_binary_url(required_major: u32) -> Result<String, AppError> {
    Ok(format!(
        "https://api.adoptium.net/v3/binary/latest/{required_major}/ga/{}/{}/jre/hotspot/normal/eclipse",
        adoptium_os()?,
        adoptium_arch()?
    ))
}

fn adoptium_os() -> Result<&'static str, AppError> {
    #[cfg(target_os = "linux")]
    {
        Ok("linux")
    }
    #[cfg(target_os = "windows")]
    {
        Ok("windows")
    }
    #[cfg(target_os = "macos")]
    {
        Ok("mac")
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        Err(AppError::Process(
            "unsupported OS for managed Java download".into(),
        ))
    }
}

fn adoptium_arch() -> Result<&'static str, AppError> {
    match std::env::consts::ARCH {
        "x86_64" => Ok("x64"),
        "aarch64" => Ok("aarch64"),
        "arm" => Ok("arm"),
        other => Err(AppError::Process(format!(
            "unsupported architecture for managed Java download: {other}"
        ))),
    }
}

fn archive_extension() -> &'static str {
    if cfg!(target_os = "windows") {
        "zip"
    } else {
        "tar.gz"
    }
}

fn java_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "java.exe"
    } else {
        "java"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minecraft_26_uses_java_25() {
        assert_eq!(required_java_for_minecraft_version("26.1"), 25);
    }

    #[test]
    fn modern_minecraft_rejects_too_new_java() {
        assert!(java_is_suitable(25, 25));
        assert!(!java_is_suitable(26, 25));
        assert!(java_is_suitable(17, 8));
    }
}
