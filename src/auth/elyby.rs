use serde::{Deserialize, Serialize};

use crate::auth::{AuthProvider, Session};
use crate::error::AppError;
use crate::storage;

fn elyby_auth_base() -> String {
    std::env::var("SWIFT_LAUNCHER_ELYBY_BASE")
        .ok()
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "https://authserver.ely.by".to_string())
}

fn elyby_auth_endpoint() -> String {
    format!("{}/auth", elyby_auth_base())
}

#[derive(Debug, Serialize)]
struct Agent {
    name: &'static str,
    version: u8,
}

#[derive(Debug, Serialize)]
struct AuthRequest {
    agent: Agent,
    username: String,
    password: String,
    #[serde(rename = "clientToken")]
    client_token: String,
    #[serde(rename = "requestUser")]
    request_user: bool,
}

#[derive(Debug, Deserialize)]
struct Profile {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct AuthResponse {
    #[serde(rename = "accessToken")]
    access_token: String,
    #[serde(rename = "clientToken")]
    client_token: String,
    #[serde(rename = "selectedProfile")]
    selected_profile: Profile,
}

pub async fn authenticate(username: String, password: String) -> Result<Session, AppError> {
    let client_token = storage::yggdrasil_client_token()?;
    let payload = AuthRequest {
        agent: Agent {
            name: "Minecraft",
            version: 1,
        },
        username,
        password,
        client_token,
        request_user: true,
    };
    let base = elyby_auth_base();
    let response = reqwest::Client::new()
        .post(format!("{base}/auth/authenticate"))
        .json(&payload)
        .send()
        .await?;
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(yggdrasil_error(status.as_u16(), &body, AuthProvider::ElyBy));
    }
    let response: AuthResponse = serde_json::from_str(&body)?;

    let uuid = response.selected_profile.id.clone();
    Ok(Session {
        provider: AuthProvider::ElyBy,
        uuid: uuid.clone(),
        username: response.selected_profile.name.clone(),
        access_token: response.access_token,
        refresh_token: Some(response.client_token),
        expires_at_unix: crate::auth::microsoft::far_future_unix(),
        avatar_url: Some(crate::auth::avatar::avatar_url_for_username(
            &response.selected_profile.name,
        )),
    })
}

pub async fn refresh(session: &Session) -> Result<Session, AppError> {
    let endpoint = elyby_auth_endpoint();
    refresh_with_base(AuthProvider::ElyBy, &endpoint, session).await
}

pub async fn validate(session: &Session) -> Result<(), AppError> {
    let endpoint = elyby_auth_endpoint();
    validate_with_base(AuthProvider::ElyBy, &endpoint, session).await
}

pub async fn invalidate(session: &Session) -> Result<(), AppError> {
    let endpoint = elyby_auth_endpoint();
    invalidate_with_base(AuthProvider::ElyBy, &endpoint, session).await
}

#[derive(Debug, Serialize)]
struct RefreshRequest {
    #[serde(rename = "accessToken")]
    access_token: String,
    #[serde(rename = "clientToken")]
    client_token: String,
    #[serde(rename = "requestUser")]
    request_user: bool,
}

#[derive(Debug, Serialize)]
struct ValidateRequest {
    #[serde(rename = "accessToken")]
    access_token: String,
}

#[derive(Debug, Serialize)]
struct InvalidateRequest {
    #[serde(rename = "accessToken")]
    access_token: String,
    #[serde(rename = "clientToken")]
    client_token: String,
}

pub(crate) async fn refresh_with_base(
    provider: AuthProvider,
    auth_endpoint: &str,
    session: &Session,
) -> Result<Session, AppError> {
    let client_token = session
        .refresh_token
        .clone()
        .unwrap_or(storage::yggdrasil_client_token()?);
    let payload = RefreshRequest {
        access_token: session.access_token.clone(),
        client_token,
        request_user: true,
    };
    let response = reqwest::Client::new()
        .post(format!("{auth_endpoint}/refresh"))
        .json(&payload)
        .send()
        .await?;
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(yggdrasil_error(status.as_u16(), &body, provider));
    }
    let response: AuthResponse = serde_json::from_str(&body)?;
    let uuid = response.selected_profile.id.clone();
    Ok(Session {
        provider,
        uuid: uuid.clone(),
        username: response.selected_profile.name.clone(),
        access_token: response.access_token,
        refresh_token: Some(response.client_token),
        expires_at_unix: crate::auth::microsoft::far_future_unix(),
        avatar_url: Some(crate::auth::avatar::avatar_url_for_username(
            &response.selected_profile.name,
        )),
    })
}

pub(crate) async fn validate_with_base(
    provider: AuthProvider,
    auth_endpoint: &str,
    session: &Session,
) -> Result<(), AppError> {
    let payload = ValidateRequest {
        access_token: session.access_token.clone(),
    };
    let response = reqwest::Client::new()
        .post(format!("{auth_endpoint}/validate"))
        .json(&payload)
        .send()
        .await?;
    if response.status().is_success() {
        return Ok(());
    }
    let status = response.status();
    let body = response.text().await?;
    Err(yggdrasil_error(status.as_u16(), &body, provider))
}

pub(crate) async fn invalidate_with_base(
    provider: AuthProvider,
    auth_endpoint: &str,
    session: &Session,
) -> Result<(), AppError> {
    let client_token = session
        .refresh_token
        .clone()
        .unwrap_or(storage::yggdrasil_client_token()?);
    let payload = InvalidateRequest {
        access_token: session.access_token.clone(),
        client_token,
    };
    let response = reqwest::Client::new()
        .post(format!("{auth_endpoint}/invalidate"))
        .json(&payload)
        .send()
        .await?;
    if response.status().is_success() {
        return Ok(());
    }
    let status = response.status();
    let body = response.text().await?;
    Err(yggdrasil_error(status.as_u16(), &body, provider))
}

pub(crate) fn yggdrasil_error(status: u16, body: &str, provider: AuthProvider) -> AppError {
    #[derive(Debug, Deserialize)]
    struct ErrorBody {
        #[serde(default, rename = "errorMessage")]
        error_message: String,
        #[serde(default)]
        error: String,
    }

    let parsed = serde_json::from_str::<ErrorBody>(body).ok();
    let server_message = parsed
        .as_ref()
        .map(|error| {
            if error.error_message.is_empty() {
                error.error.clone()
            } else {
                error.error_message.clone()
            }
        })
        .filter(|message| !message.is_empty())
        .unwrap_or_else(|| format!("HTTP {status}"));

    if provider == AuthProvider::ElyBy && status == 401 {
        return AppError::Auth(format!(
            "Ely.by rejected the login: {server_message}. Check email/username and password. If 2FA is enabled, enter password:token in the password field."
        ));
    }

    AppError::Auth(format!("{provider} rejected the login: {server_message}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;
    use tokio::time::{timeout, Duration};

    fn find_crlf(input: &[u8]) -> Option<usize> {
        input.windows(2).position(|w| w == b"\r\n")
    }

    fn decode_chunked(mut input: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let Some(line_end) = find_crlf(input) else {
                break;
            };
            let size_str = String::from_utf8_lossy(&input[..line_end]);
            let size = usize::from_str_radix(size_str.trim(), 16).unwrap_or(0);
            let rest = &input[line_end + 2..];
            if rest.len() < size + 2 {
                break;
            }
            if size == 0 {
                break;
            }
            out.extend_from_slice(&rest[..size]);
            input = &rest[size + 2..];
        }
        out
    }

    async fn spawn_capture_server(
        response_body: Vec<u8>,
    ) -> (
        String,
        oneshot::Receiver<(String, String)>,
        tokio::task::JoinHandle<()>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = oneshot::channel();
        let response_body = Arc::new(response_body);
        let handle = tokio::spawn(async move {
            let (mut socket, _) = match listener.accept().await {
                Ok(value) => value,
                Err(_) => return,
            };

            let mut data = Vec::new();
            let mut content_length = None;
            let mut chunked = false;
            let mut header_end = None;
            loop {
                let mut buf = [0u8; 1024];
                let Ok(Ok(n)) = timeout(Duration::from_secs(2), socket.read(&mut buf)).await else {
                    break;
                };
                if n == 0 {
                    break;
                }
                data.extend_from_slice(&buf[..n]);
                if header_end.is_none() {
                    if let Some(pos) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                        header_end = Some(pos);
                        let headers = String::from_utf8_lossy(&data[..pos]);
                        for line in headers.lines() {
                            let lower = line.to_ascii_lowercase();
                            if let Some(value) = lower.strip_prefix("content-length:") {
                                content_length = value.trim().parse::<usize>().ok();
                            }
                            if lower.starts_with("transfer-encoding:") && lower.contains("chunked")
                            {
                                chunked = true;
                            }
                        }
                    }
                }

                if let Some(pos) = header_end {
                    let body_start = pos + 4;
                    let body_len = data.len().saturating_sub(body_start);
                    if let Some(len) = content_length {
                        if body_len >= len {
                            break;
                        }
                    } else if chunked {
                        let body = &data[body_start..];
                        if body.windows(5).any(|w| w == b"0\r\n\r\n")
                            || body.windows(6).any(|w| w == b"\r\n0\r\n\r\n")
                        {
                            break;
                        }
                    }
                }

                if data.len() > 65536 {
                    break;
                }
            }

            let header_end = header_end.unwrap_or_else(|| data.len());
            let headers = String::from_utf8_lossy(&data[..header_end]);
            let path = headers
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/")
                .to_string();
            let raw_body = data.get(header_end + 4..).unwrap_or_default();
            let body_bytes = if chunked {
                decode_chunked(raw_body)
            } else if let Some(len) = content_length {
                raw_body.get(..len).unwrap_or(raw_body).to_vec()
            } else {
                raw_body.to_vec()
            };
            let body = String::from_utf8_lossy(&body_bytes).to_string();

            let _ = tx.send((path, body));

            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                response_body.len()
            );
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.write_all(&response_body).await;
        });
        (format!("http://{}", addr), rx, handle)
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
    async fn elyby_auth_payload_ok() {
        let response = serde_json::json!({
            "accessToken": "token",
            "clientToken": "client",
            "selectedProfile": {"id": "uuid", "name": "Steve"}
        });
        let (base, rx, handle) = spawn_capture_server(response.to_string().into_bytes()).await;

        let temp = temp_dir("elyby");
        std::env::set_var("XDG_DATA_HOME", &temp);
        std::env::set_var("SWIFT_LAUNCHER_ELYBY_BASE", &base);

        let session = authenticate("user".into(), "pass".into()).await.unwrap();

        let (path, body) = rx.await.unwrap();

        std::env::remove_var("SWIFT_LAUNCHER_ELYBY_BASE");
        std::env::remove_var("XDG_DATA_HOME");
        handle.abort();

        assert_eq!(path, "/auth/authenticate");
        let value: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(value["username"], "user");
        assert_eq!(value["password"], "pass");
        assert_eq!(value["agent"]["name"], "Minecraft");
        assert_eq!(value["agent"]["version"], 1);
        assert_eq!(value["requestUser"], true);
        assert!(value["clientToken"].as_str().unwrap_or_default().len() > 0);

        assert_eq!(session.username, "Steve");
        assert_eq!(session.uuid, "uuid");
        assert_eq!(session.access_token, "token");
        assert_eq!(session.refresh_token.as_deref(), Some("client"));
    }
}
