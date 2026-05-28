use std::path::{Path, PathBuf};

use tokio::process::Command;

use crate::error::AppError;

pub fn available_bytes_at(path: &Path) -> Option<u64> {
    let path = if path.exists() {
        path.to_path_buf()
    } else {
        path.parent()?.to_path_buf()
    };
    fs2::statvfs(&path).ok().map(|stat| stat.available_space())
}

pub async fn ensure_disk_space(path: &Path, required: u64) -> Result<(), AppError> {
    let Some(available) = available_bytes_at(path) else {
        return Ok(());
    };
    if available < required.saturating_add(64 * 1024 * 1024) {
        return Err(AppError::Download(format!(
            "not enough disk space near {} (need ~{} MB, available ~{} MB)",
            path.display(),
            required / 1024 / 1024,
            available / 1024 / 1024
        )));
    }
    Ok(())
}

pub async fn open_path(path: PathBuf) -> Result<String, AppError> {
    if tokio::fs::metadata(&path).await.is_err() {
        return Err(AppError::Process(format!(
            "path does not exist: {}",
            path.display()
        )));
    }
    run_open_command(path.to_string_lossy().to_string()).await
}

pub async fn open_url(url: String) -> Result<String, AppError> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(AppError::Process(
            "only http/https URLs can be opened".into(),
        ));
    }
    run_open_command(url).await
}

pub async fn pick_file(
    title: &'static str,
    filters: Vec<(&'static str, Vec<&'static str>)>,
) -> Option<PathBuf> {
    tokio::task::spawn_blocking(move || {
        let mut dialog = rfd::FileDialog::new().set_title(title);
        for (name, extensions) in filters {
            dialog = dialog.add_filter(name, &extensions);
        }
        dialog.pick_file()
    })
    .await
    .ok()
    .flatten()
}

pub async fn save_file(
    title: &'static str,
    file_name: String,
    filters: Vec<(&'static str, Vec<&'static str>)>,
) -> Option<PathBuf> {
    tokio::task::spawn_blocking(move || {
        let mut dialog = rfd::FileDialog::new()
            .set_title(title)
            .set_file_name(&file_name);
        for (name, extensions) in filters {
            dialog = dialog.add_filter(name, &extensions);
        }
        dialog.save_file()
    })
    .await
    .ok()
    .flatten()
}

async fn run_open_command(target: String) -> Result<String, AppError> {
    let mut command = platform_open_command();
    command.arg(&target);
    let status = command.status().await?;
    if status.success() {
        Ok(format!("opened {target}"))
    } else {
        Err(AppError::Process(format!(
            "open command failed for {target}: {status}"
        )))
    }
}

fn platform_open_command() -> Command {
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer")
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open")
    }
}
