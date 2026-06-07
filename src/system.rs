use std::path::{Path, PathBuf};

use tokio::process::Command;

use crate::error::AppError;

#[derive(Debug, Clone, Default)]
pub struct SystemTelemetry {
    pub memory_used_bytes: Option<u64>,
    pub memory_total_bytes: Option<u64>,
    pub disk_used_bytes: Option<u64>,
    pub disk_total_bytes: Option<u64>,
    pub cpu_usage_percent: Option<f32>,
}

#[derive(Debug, Clone, Copy)]
pub struct CpuSample {
    idle: u64,
    total: u64,
}

pub fn available_bytes_at(path: &Path) -> Option<u64> {
    let path = if path.exists() {
        path.to_path_buf()
    } else {
        path.parent()?.to_path_buf()
    };
    fs2::statvfs(&path).ok().map(|stat| stat.available_space())
}

pub async fn read_system_telemetry(
    disk_path: PathBuf,
    previous_cpu: Option<CpuSample>,
) -> (SystemTelemetry, Option<CpuSample>) {
    tokio::task::spawn_blocking(move || read_system_telemetry_blocking(&disk_path, previous_cpu))
        .await
        .unwrap_or_default()
}

fn read_system_telemetry_blocking(
    disk_path: &Path,
    previous_cpu: Option<CpuSample>,
) -> (SystemTelemetry, Option<CpuSample>) {
    let (memory_used_bytes, memory_total_bytes) = read_memory_usage();
    let (disk_used_bytes, disk_total_bytes) = disk_usage_at(disk_path);
    let cpu_sample = read_cpu_sample();
    let cpu_usage_percent = previous_cpu
        .zip(cpu_sample)
        .and_then(|(previous, current)| cpu_usage_between(previous, current));

    (
        SystemTelemetry {
            memory_used_bytes,
            memory_total_bytes,
            disk_used_bytes,
            disk_total_bytes,
            cpu_usage_percent,
        },
        cpu_sample,
    )
}

fn disk_usage_at(path: &Path) -> (Option<u64>, Option<u64>) {
    let Some(path) = existing_stat_path(path) else {
        return (None, None);
    };
    let Ok(stat) = fs2::statvfs(path) else {
        return (None, None);
    };
    let total = stat.total_space();
    let available = stat.available_space();
    (Some(total.saturating_sub(available)), Some(total))
}

fn existing_stat_path(path: &Path) -> Option<&Path> {
    let mut current = Some(path);
    while let Some(path) = current {
        if path.exists() {
            return Some(path);
        }
        current = path.parent();
    }
    None
}

fn cpu_usage_between(previous: CpuSample, current: CpuSample) -> Option<f32> {
    let total = current.total.checked_sub(previous.total)? as f32;
    let idle = current.idle.checked_sub(previous.idle)? as f32;
    if total <= 0.0 {
        return None;
    }
    Some(((total - idle) / total * 100.0).clamp(0.0, 100.0))
}

#[cfg(target_os = "linux")]
fn read_memory_usage() -> (Option<u64>, Option<u64>) {
    let Ok(contents) = std::fs::read_to_string("/proc/meminfo") else {
        return (None, None);
    };
    let mut total_kb = None;
    let mut available_kb = None;
    for line in contents.lines() {
        if let Some(value) = line.strip_prefix("MemTotal:") {
            total_kb = parse_meminfo_kb(value);
        } else if let Some(value) = line.strip_prefix("MemAvailable:") {
            available_kb = parse_meminfo_kb(value);
        }
    }
    let total = total_kb.map(|value| value * 1024);
    let used = total_kb
        .zip(available_kb)
        .map(|(total, available)| total.saturating_sub(available) * 1024);
    (used, total)
}

#[cfg(not(target_os = "linux"))]
fn read_memory_usage() -> (Option<u64>, Option<u64>) {
    (None, None)
}

#[cfg(target_os = "linux")]
fn parse_meminfo_kb(value: &str) -> Option<u64> {
    value.split_whitespace().next()?.parse::<u64>().ok()
}

#[cfg(target_os = "linux")]
fn read_cpu_sample() -> Option<CpuSample> {
    let contents = std::fs::read_to_string("/proc/stat").ok()?;
    let line = contents.lines().find(|line| line.starts_with("cpu "))?;
    let values = line
        .split_whitespace()
        .skip(1)
        .filter_map(|value| value.parse::<u64>().ok())
        .collect::<Vec<_>>();
    if values.len() < 4 {
        return None;
    }
    let idle = values
        .get(3)
        .copied()
        .unwrap_or_default()
        .saturating_add(values.get(4).copied().unwrap_or_default());
    let total = values.into_iter().sum();
    Some(CpuSample { idle, total })
}

#[cfg(not(target_os = "linux"))]
fn read_cpu_sample() -> Option<CpuSample> {
    None
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

pub async fn pick_folder(title: &'static str) -> Option<PathBuf> {
    tokio::task::spawn_blocking(move || rfd::FileDialog::new().set_title(title).pick_folder())
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
