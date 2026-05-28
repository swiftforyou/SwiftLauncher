use std::time::{Duration, SystemTime, UNIX_EPOCH};

use lighty_auth::microsoft::MicrosoftAuth;
use lighty_auth::{AuthProvider as LightyAuthProvider, Authenticator, ExposeSecret, SecretString};
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

fn map_microsoft_error(error: impl std::fmt::Display) -> AppError {
    let message = error.to_string();
    if message.contains("AADSTS700016") || message.contains("unauthorized_client") {
        AppError::Auth(
            "Microsoft rejected the client id. Use the Azure Application (client) ID, not object/tenant ID, and set Supported account types to personal Microsoft accounts or multitenant + personal accounts. Then enable public client/native flows.".into(),
        )
    } else {
        AppError::Auth(message)
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
