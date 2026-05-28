use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::AppError;
use crate::instances::Instance;

pub async fn write_launch_crash_report(
    instance: &Instance,
    status: &str,
    runtime_seconds: u64,
    lines: &[String],
) -> Result<String, AppError> {
    let dir = instance.path.join("crash-reports");
    tokio::fs::create_dir_all(&dir).await?;

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| AppError::Process(error.to_string()))?
        .as_secs();
    let file_name = format!(
        "swift-launcher-{}-{timestamp}.log",
        sanitize_file_part(&instance.name)
    );
    let path = dir.join(file_name);

    let mut body = String::new();
    body.push_str("Swift Launcher crash report\n");
    body.push_str("===========================\n\n");
    body.push_str(&format!("Instance: {}\n", instance.name));
    body.push_str(&format!("Instance id: {}\n", instance.id));
    body.push_str(&format!("Minecraft: {}\n", instance.minecraft_version));
    body.push_str(&format!("Loader: {}\n", instance.loader));
    if let Some(loader_version) = &instance.loader_version {
        body.push_str(&format!("Loader version: {loader_version}\n"));
    }
    body.push_str(&format!("Exit status: {status}\n"));
    body.push_str(&format!("Runtime seconds: {runtime_seconds}\n"));
    body.push_str(&format!("RAM MB: {}\n", instance.ram_mb));
    body.push_str(&format!("Java path: {}\n", instance.java_path));
    body.push_str(&format!("JVM args: {}\n", instance.jvm_args));
    body.push_str("\nProcess output\n");
    body.push_str("--------------\n");
    if lines.is_empty() {
        body.push_str("No output captured.\n");
    } else {
        for line in lines {
            body.push_str(line);
            body.push('\n');
        }
    }

    tokio::fs::write(&path, body).await?;
    Ok(path.display().to_string())
}

fn sanitize_file_part(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
            out.push(ch);
        } else if ch.is_whitespace() {
            out.push('-');
        }
    }
    if out.is_empty() {
        "instance".into()
    } else {
        out
    }
}
