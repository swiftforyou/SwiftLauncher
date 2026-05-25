use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::error::AppError;
use crate::instances::{instance_root, Instance, InstanceRunState};

const MANIFEST: &str = "swift-instance.json";
const FILES_PREFIX: &str = "files/";

pub async fn export_instance(instance: Instance, destination: PathBuf) -> Result<String, AppError> {
    tokio::task::spawn_blocking(move || export_blocking(&instance, &destination))
        .await
        .map_err(|error| AppError::Instance(error.to_string()))?
}

pub async fn import_instance(archive_path: PathBuf) -> Result<Instance, AppError> {
    tokio::task::spawn_blocking(move || import_blocking(&archive_path))
        .await
        .map_err(|error| AppError::Instance(error.to_string()))?
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
    zip.finish().map_err(|error| AppError::Storage(error.to_string()))?;
    Ok(destination.display().to_string())
}

fn add_dir(zip: &mut ZipWriter<File>, root: &Path, dir: &Path, options: SimpleFileOptions) -> Result<(), AppError> {
    for entry in fs::read_dir(dir).map_err(|error| AppError::Storage(error.to_string()))? {
        let entry = entry.map_err(|error| AppError::Storage(error.to_string()))?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|error| AppError::Storage(error.to_string()))?;
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

fn import_blocking(archive_path: &Path) -> Result<Instance, AppError> {
    let file = File::open(archive_path).map_err(|error| AppError::Storage(error.to_string()))?;
    let mut archive = ZipArchive::new(file).map_err(|error| AppError::Storage(error.to_string()))?;
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
            fs::create_dir_all(output_path).map_err(|error| AppError::Storage(error.to_string()))?;
        } else {
            if let Some(parent) = output_path.parent() {
                fs::create_dir_all(parent).map_err(|error| AppError::Storage(error.to_string()))?;
            }
            let mut output = File::create(output_path).map_err(|error| AppError::Storage(error.to_string()))?;
            std::io::copy(&mut file, &mut output).map_err(|error| AppError::Storage(error.to_string()))?;
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
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
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
        let mut instance = unique_instance("archive-test");
        tokio::fs::create_dir_all(instance.path.join("mods")).await.unwrap();
        tokio::fs::write(instance.path.join("mods").join("mod.jar"), b"mod-data")
            .await
            .unwrap();

        let zip_path = instance.path.with_extension("zip");
        let exported = export_instance(instance.clone(), zip_path.clone()).await.unwrap();
        assert_eq!(exported, zip_path.display().to_string());

        let imported = import_instance(zip_path.clone()).await.unwrap();
        assert_ne!(imported.id, instance.id);
        assert_eq!(imported.name, instance.name);
        let imported_mod = imported.path.join("mods").join("mod.jar");
        let bytes = tokio::fs::read(imported_mod).await.unwrap();
        assert_eq!(bytes, b"mod-data");
    }
}
