use crate::error::AppError;

pub fn configured_client_id() -> Option<String> {
    option_env!("SWIFT_LAUNCHER_DISCORD_CLIENT_ID")
        .map(str::to_owned)
        .or_else(|| std::env::var("SWIFT_LAUNCHER_DISCORD_CLIENT_ID").ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub async fn publish_activity(instance_name: String, version: String) -> Result<String, AppError> {
    let client_id = configured_client_id().ok_or_else(|| {
        AppError::Process(
            "Discord Rich Presence needs SWIFT_LAUNCHER_DISCORD_CLIENT_ID at build/runtime".into(),
        )
    })?;
    tokio::task::spawn_blocking(move || send_activity(Some((instance_name, version)), &client_id))
        .await
        .map_err(|error| AppError::Process(format!("Discord RPC task failed: {error}")))?
}

pub async fn clear_activity() -> Result<String, AppError> {
    let client_id = configured_client_id().ok_or_else(|| {
        AppError::Process(
            "Discord Rich Presence needs SWIFT_LAUNCHER_DISCORD_CLIENT_ID at build/runtime".into(),
        )
    })?;
    tokio::task::spawn_blocking(move || send_activity(None, &client_id))
        .await
        .map_err(|error| AppError::Process(format!("Discord RPC task failed: {error}")))?
}

#[cfg(all(unix, not(target_os = "macos")))]
fn send_activity(activity: Option<(String, String)>, client_id: &str) -> Result<String, AppError> {
    use serde_json::json;
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    let path = discord_socket_path()?;
    let mut stream = UnixStream::connect(&path)
        .map_err(|error| AppError::Process(format!("Discord IPC unavailable: {error}")))?;
    stream
        .set_write_timeout(Some(Duration::from_millis(700)))
        .map_err(|error| AppError::Process(error.to_string()))?;

    write_frame(&mut stream, 0, &json!({ "v": 1, "client_id": client_id }))?;
    let payload = match activity {
        Some((instance_name, version)) => json!({
            "cmd": "SET_ACTIVITY",
            "args": {
                "pid": std::process::id(),
                "activity": {
                    "details": format!("Playing {instance_name}"),
                    "state": format!("Minecraft {version}"),
                    "timestamps": { "start": unix_timestamp() },
                    "assets": {
                        "large_text": "Swift Launcher"
                    }
                }
            },
            "nonce": format!("swift-{}", unix_timestamp())
        }),
        None => json!({
            "cmd": "SET_ACTIVITY",
            "args": {
                "pid": std::process::id(),
                "activity": null
            },
            "nonce": format!("swift-clear-{}", unix_timestamp())
        }),
    };
    write_frame(&mut stream, 1, &payload)?;
    Ok("Discord Rich Presence updated".into())
}

#[cfg(not(all(unix, not(target_os = "macos"))))]
fn send_activity(_: Option<(String, String)>, _: &str) -> Result<String, AppError> {
    Err(AppError::Process(
        "Discord Rich Presence IPC is implemented on Linux only for now".into(),
    ))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn discord_socket_path() -> Result<std::path::PathBuf, AppError> {
    let mut roots = Vec::new();
    if let Ok(path) = std::env::var("XDG_RUNTIME_DIR") {
        roots.push(std::path::PathBuf::from(path));
    }
    roots.push(std::env::temp_dir());
    for root in roots {
        for index in 0..10 {
            let path = root.join(format!("discord-ipc-{index}"));
            if path.exists() {
                return Ok(path);
            }
        }
    }
    Err(AppError::Process("Discord IPC socket not found".into()))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn write_frame(
    stream: &mut std::os::unix::net::UnixStream,
    opcode: u32,
    value: &serde_json::Value,
) -> Result<(), AppError> {
    use std::io::Write;

    let bytes = serde_json::to_vec(value)?;
    stream.write_all(&opcode.to_le_bytes())?;
    stream.write_all(&(bytes.len() as u32).to_le_bytes())?;
    stream.write_all(&bytes)?;
    stream.flush()?;
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or_default()
}
