use std::time::{Duration, SystemTime, UNIX_EPOCH};

use lighty_auth::microsoft::MicrosoftAuth;
use lighty_auth::{AuthProvider as LightyAuthProvider, Authenticator, ExposeSecret, SecretString};
use serde::Deserialize;
use tokio::sync::mpsc;

use crate::auth::{AuthProvider, Session};
use crate::error::AppError;

pub fn far_future_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() + 60 * 60 * 24 * 30)
        .unwrap_or(60 * 60 * 24 * 30)
}

pub fn minecraft_token_expiry_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() + 60 * 60 * 23)
        .unwrap_or(60 * 60 * 23)
}

pub async fn begin_device_flow() -> Result<(String, String), AppError> {
    Err(AppError::Auth(
        "Microsoft auth now starts from the Sign in button and requires SWIFT_LAUNCHER_MS_CLIENT_ID".into(),
    ))
}

pub async fn authenticate_device_stub() -> Result<Session, AppError> {
    authenticate_device(None).await
}

pub async fn authenticate_device(
    device_tx: Option<mpsc::UnboundedSender<(String, String)>>,
) -> Result<Session, AppError> {
    let client_id = microsoft_client_id()?;
    let mut auth = MicrosoftAuth::new(client_id);
    auth.set_timeout(Duration::from_secs(300));
    auth.set_poll_interval(Duration::from_secs(5));
    if let Some(tx) = device_tx {
        auth.set_device_code_callback(move |code, url| {
            let _ = tx.send((code.to_string(), url.to_string()));
        });
    }

    let profile = auth.authenticate().await.map_err(map_microsoft_error)?;
    session_from_profile(profile)
}

pub async fn refresh_session(session: &Session) -> Result<Session, AppError> {
    let client_id = microsoft_client_id()?;
    let Some(refresh_token) = &session.refresh_token else {
        return Err(AppError::Auth("Microsoft refresh token missing".into()));
    };

    let mut auth = MicrosoftAuth::new(client_id);
    let secret = SecretString::from(refresh_token.clone());
    let profile = auth
        .authenticate_with_refresh_token(&secret)
        .await
        .map_err(map_microsoft_error)?;
    session_from_profile(profile)
}

pub async fn validate(session: &Session) -> Result<(), AppError> {
    validate_with_profile_url(session, &minecraft_profile_url()).await
}

async fn validate_with_profile_url(session: &Session, profile_url: &str) -> Result<(), AppError> {
    if session.access_token == "offline" {
        return Ok(());
    }

    let response = reqwest::Client::new()
        .get(profile_url)
        .bearer_auth(&session.access_token)
        .send()
        .await?;
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(minecraft_services_error(status.as_u16(), &body));
    }

    let profile: MinecraftProfile = serde_json::from_str(&body)?;
    if normalize_uuid(&profile.id) != normalize_uuid(&session.uuid) {
        return Err(AppError::Auth(format!(
            "Microsoft token profile mismatch: expected {}, got {}",
            session.uuid, profile.id
        )));
    }
    Ok(())
}

pub async fn offline_dev_session() -> Session {
    Session {
        provider: AuthProvider::Microsoft,
        uuid: "00000000000000000000000000000000".into(),
        username: "Player".into(),
        access_token: "offline".into(),
        refresh_token: Some("offline".into()),
        expires_at_unix: far_future_unix(),
        avatar_url: None,
    }
}

fn microsoft_client_id() -> Result<String, AppError> {
    const DEFAULT_CLIENT_ID: &str = "328faca9-e866-47dc-bf41-de106cd7f1a5";
    std::env::var("SWIFT_LAUNCHER_MS_CLIENT_ID")
        .map(|value| value.trim().to_string())
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| Some(DEFAULT_CLIENT_ID.to_string()))
        .ok_or_else(|| AppError::Auth("Microsoft client id missing".into()))
}

fn minecraft_profile_url() -> String {
    std::env::var("SWIFT_LAUNCHER_MS_PROFILE_URL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "https://api.minecraftservices.com/minecraft/profile".into())
}

#[derive(Debug, Deserialize)]
struct MinecraftProfile {
    id: String,
}

fn minecraft_services_error(status: u16, body: &str) -> AppError {
    #[derive(Debug, Deserialize)]
    struct ErrorBody {
        #[serde(default)]
        error: String,
        #[serde(default, rename = "errorMessage")]
        error_message: String,
        #[serde(default)]
        path: String,
    }

    let message = serde_json::from_str::<ErrorBody>(body)
        .ok()
        .and_then(|error| {
            [error.error_message, error.error, error.path]
                .into_iter()
                .find(|value| !value.trim().is_empty())
        })
        .unwrap_or_else(|| format!("HTTP {status}"));
    AppError::Auth(format!("Microsoft session invalid: {message}"))
}

fn normalize_uuid(value: &str) -> String {
    value
        .chars()
        .filter(|ch| *ch != '-')
        .flat_map(char::to_lowercase)
        .collect()
}

fn map_microsoft_error(error: impl std::fmt::Display) -> AppError {
    let message = error.to_string();
    if message.contains("AADSTS700016") || message.contains("unauthorized_client") {
        AppError::Auth(
            "Microsoft rejected the client id. Use the Azure Application (client) ID, not object/tenant ID, and set Supported account types to personal Microsoft accounts or multitenant + personal accounts. Then enable public client/native flows.".into(),
        )
    } else if message.contains("Invalid app registration")
        || message.contains("/authentication/login_with_xbox")
        || message.contains("aka.ms/AppRegInfo")
    {
        AppError::Auth(
            "Minecraft rejected this launcher app registration. Azure sign-in worked, but Minecraft Services requires a valid/approved Minecraft app registration. Register/approve the app at https://aka.ms/AppRegInfo, then build Swift Launcher with that Application (client) ID.".into(),
        )
    } else {
        AppError::Auth(message)
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;
    use tokio::time::{timeout, Duration};

    async fn spawn_profile_server(
        status: u16,
        response_body: &'static str,
    ) -> (
        String,
        oneshot::Receiver<(String, Option<String>)>,
        tokio::task::JoinHandle<()>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = oneshot::channel();
        let response_body = Arc::new(response_body.as_bytes().to_vec());
        let handle = tokio::spawn(async move {
            let (mut socket, _) = match listener.accept().await {
                Ok(value) => value,
                Err(_) => return,
            };
            let mut data = Vec::new();
            loop {
                let mut buf = [0u8; 1024];
                let Ok(Ok(n)) = timeout(Duration::from_secs(2), socket.read(&mut buf)).await else {
                    break;
                };
                if n == 0 {
                    break;
                }
                data.extend_from_slice(&buf[..n]);
                if data.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            let headers = String::from_utf8_lossy(&data);
            let path = headers
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/")
                .to_string();
            let authorization = headers.lines().find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("authorization")
                    .then(|| value.trim().to_string())
            });
            let _ = tx.send((path, authorization));
            let status_text = if status == 200 { "OK" } else { "Unauthorized" };
            let response = format!(
                "HTTP/1.1 {status} {status_text}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
                response_body.len()
            );
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.write_all(&response_body).await;
        });
        (format!("http://{}", addr), rx, handle)
    }

    fn test_session() -> Session {
        Session {
            provider: AuthProvider::Microsoft,
            uuid: "ffc8fdc9-5824-509e-8a57-c99b940fb996".into(),
            username: "Steve".into(),
            access_token: "token".into(),
            refresh_token: Some("refresh".into()),
            expires_at_unix: far_future_unix(),
            avatar_url: None,
        }
    }

    #[tokio::test]
    async fn microsoft_validate_calls_profile_endpoint() {
        let (url, rx, handle) = spawn_profile_server(
            200,
            r#"{"id":"ffc8fdc95824509e8a57c99b940fb996","name":"Steve"}"#,
        )
        .await;

        validate_with_profile_url(&test_session(), &url)
            .await
            .unwrap();
        let (path, authorization) = rx.await.unwrap();

        handle.abort();

        assert_eq!(path, "/");
        assert_eq!(authorization.as_deref(), Some("Bearer token"));
    }

    #[tokio::test]
    async fn microsoft_validate_rejects_unauthorized() {
        let (url, _rx, handle) = spawn_profile_server(
            401,
            r#"{"error":"Unauthorized","errorMessage":"Invalid token"}"#,
        )
        .await;

        let error = validate_with_profile_url(&test_session(), &url)
            .await
            .unwrap_err()
            .to_string();

        handle.abort();

        assert!(error.contains("Microsoft session invalid"));
        assert!(error.contains("Invalid token"));
    }
}

fn session_from_profile(profile: lighty_auth::UserProfile) -> Result<Session, AppError> {
    let access_token = profile
        .access_token
        .as_ref()
        .map(|token| token.expose_secret().to_string())
        .ok_or_else(|| {
            AppError::Auth("Microsoft profile did not include an access token".into())
        })?;
    let refresh_token = match &profile.provider {
        LightyAuthProvider::Microsoft { refresh_token, .. } => refresh_token
            .as_ref()
            .map(|token| token.expose_secret().to_string()),
        _ => None,
    };

    let uuid = profile.uuid.clone();
    let avatar_url = Some(crate::auth::avatar::avatar_url_for_uuid(&uuid));
    Ok(Session {
        provider: AuthProvider::Microsoft,
        uuid,
        username: profile.username,
        access_token,
        refresh_token,
        expires_at_unix: minecraft_token_expiry_unix(),
        avatar_url,
    })
}
