use crate::error::AppError;

#[derive(Debug, Clone)]
pub struct Activity {
    pub details: String,
    pub state: String,
}

pub fn configured_client_id() -> Option<String> {
    option_env!("SWIFT_LAUNCHER_DISCORD_CLIENT_ID")
        .map(str::to_owned)
        .or_else(|| std::env::var("SWIFT_LAUNCHER_DISCORD_CLIENT_ID").ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub async fn publish_activity(details: String, state: String) -> Result<String, AppError> {
    let client_id = configured_client_id().ok_or_else(|| {
        AppError::Process(
            "Discord Rich Presence needs SWIFT_LAUNCHER_DISCORD_CLIENT_ID at build/runtime".into(),
        )
    })?;
    tokio::task::spawn_blocking(move || {
        send_activity(Some(Activity { details, state }), &client_id)
    })
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

pub fn clear_activity_blocking() -> Result<String, AppError> {
    let client_id = configured_client_id().ok_or_else(|| {
        AppError::Process(
            "Discord Rich Presence needs SWIFT_LAUNCHER_DISCORD_CLIENT_ID at build/runtime".into(),
        )
    })?;
    send_activity(None, &client_id)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn send_activity(activity: Option<Activity>, client_id: &str) -> Result<String, AppError> {
    use serde_json::json;
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    let path = discord_socket_path()?;
    let mut stream = UnixStream::connect(&path)
        .map_err(|error| AppError::Process(format!("Discord IPC unavailable: {error}")))?;
    stream
        .set_write_timeout(Some(Duration::from_millis(700)))
        .map_err(|error| AppError::Process(error.to_string()))?;
    stream
        .set_read_timeout(Some(Duration::from_millis(1_200)))
        .map_err(|error| AppError::Process(error.to_string()))?;

    write_frame(&mut stream, 0, &json!({ "v": 1, "client_id": client_id }))?;
    let ready = read_frame(&mut stream)?;
    validate_response(&ready, "DISPATCH")?;
    let payload = match activity {
        Some(activity) => json!({
            "cmd": "SET_ACTIVITY",
            "args": {
                "pid": std::process::id(),
                "activity": {
                    "details": activity.details,
                    "state": activity.state,
                    "timestamps": { "start": unix_timestamp() },
                    "assets": {
                        "large_text": "Swift Launcher"
                    }
                }
            },
            "nonce": nonce("swift")
        }),
        None => json!({
            "cmd": "SET_ACTIVITY",
            "args": {
                "pid": std::process::id(),
                "activity": null
            },
            "nonce": nonce("swift-clear")
        }),
    };
    write_frame(&mut stream, 1, &payload)?;
    let response = read_frame(&mut stream)?;
    validate_response(&response, "SET_ACTIVITY")?;
    Ok("Discord Rich Presence updated".into())
}

#[cfg(target_os = "windows")]
fn send_activity(activity: Option<Activity>, client_id: &str) -> Result<String, AppError> {
    use serde_json::json;
    use std::fs::OpenOptions;

    let path = discord_pipe_path()?;
    let mut pipe = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|error| AppError::Process(format!("Discord IPC unavailable: {error}")))?;

    write_frame(&mut pipe, 0, &json!({ "v": 1, "client_id": client_id }))?;
    let ready = read_frame(&mut pipe)?;
    validate_response(&ready, "DISPATCH")?;
    let payload = match activity {
        Some(activity) => json!({
            "cmd": "SET_ACTIVITY",
            "args": {
                "pid": std::process::id(),
                "activity": {
                    "details": activity.details,
                    "state": activity.state,
                    "timestamps": { "start": unix_timestamp() },
                    "assets": {
                        "large_text": "Swift Launcher"
                    }
                }
            },
            "nonce": nonce("swift")
        }),
        None => json!({
            "cmd": "SET_ACTIVITY",
            "args": {
                "pid": std::process::id(),
                "activity": null
            },
            "nonce": nonce("swift-clear")
        }),
    };
    write_frame(&mut pipe, 1, &payload)?;
    let response = read_frame(&mut pipe)?;
    validate_response(&response, "SET_ACTIVITY")?;
    Ok("Discord Rich Presence updated".into())
}

#[cfg(not(any(all(unix, not(target_os = "macos")), target_os = "windows")))]
fn send_activity(_: Option<Activity>, _: &str) -> Result<String, AppError> {
    Err(AppError::Process(
        "Discord Rich Presence IPC is implemented on Linux and Windows only for now".into(),
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

#[cfg(target_os = "windows")]
fn discord_pipe_path() -> Result<String, AppError> {
    for index in 0..10 {
        let path = format!(r"\\?\pipe\discord-ipc-{index}");
        if std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .is_ok()
        {
            return Ok(path);
        }
    }
    Err(AppError::Process("Discord IPC pipe not found".into()))
}

#[cfg(any(all(unix, not(target_os = "macos")), target_os = "windows"))]
fn write_frame<W: std::io::Write>(
    stream: &mut W,
    opcode: u32,
    value: &serde_json::Value,
) -> Result<(), AppError> {
    let bytes = serde_json::to_vec(value)?;
    stream.write_all(&opcode.to_le_bytes())?;
    stream.write_all(&(bytes.len() as u32).to_le_bytes())?;
    stream.write_all(&bytes)?;
    stream.flush()?;
    Ok(())
}

#[cfg(any(all(unix, not(target_os = "macos")), target_os = "windows"))]
fn read_frame<R: std::io::Read>(stream: &mut R) -> Result<serde_json::Value, AppError> {
    let mut header = [0_u8; 8];
    stream
        .read_exact(&mut header)
        .map_err(|error| AppError::Process(format!("Discord IPC read failed: {error}")))?;
    let _opcode = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
    let len = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
    if len > 64 * 1024 {
        return Err(AppError::Process(format!(
            "Discord IPC response too large: {len} bytes"
        )));
    }
    let mut bytes = vec![0_u8; len];
    stream
        .read_exact(&mut bytes)
        .map_err(|error| AppError::Process(format!("Discord IPC body read failed: {error}")))?;
    serde_json::from_slice(&bytes).map_err(AppError::from)
}

#[cfg(any(all(unix, not(target_os = "macos")), target_os = "windows"))]
fn validate_response(value: &serde_json::Value, expected_cmd: &str) -> Result<(), AppError> {
    let cmd = value
        .get("cmd")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if cmd == "DISPATCH" && expected_cmd == "DISPATCH" {
        return Ok(());
    }
    if cmd == expected_cmd {
        return Ok(());
    }
    if cmd == "ERROR" {
        let message = value
            .get("data")
            .and_then(|data| data.get("message"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown Discord RPC error");
        return Err(AppError::Process(format!(
            "Discord RPC rejected {expected_cmd}: {message}"
        )));
    }
    Err(AppError::Process(format!(
        "Discord RPC expected {expected_cmd}, got {cmd}: {value}"
    )))
}

#[cfg(any(all(unix, not(target_os = "macos")), target_os = "windows"))]
fn unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or_default()
}

#[cfg(any(all(unix, not(target_os = "macos")), target_os = "windows"))]
fn nonce(prefix: &str) -> String {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("{prefix}-{now}-{id}")
}
