pub mod avatar;
pub mod elyby;
pub mod littleskin;
pub mod microsoft;

use serde::{Deserialize, Serialize};

use crate::error::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthProvider {
    Microsoft,
    ElyBy,
    LittleSkin,
}

impl AuthProvider {
    pub fn as_key(self) -> &'static str {
        match self {
            Self::Microsoft => "microsoft",
            Self::ElyBy => "elyby",
            Self::LittleSkin => "littleskin",
        }
    }
}

impl std::fmt::Display for AuthProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Microsoft => f.write_str("Microsoft"),
            Self::ElyBy => f.write_str("Ely.by"),
            Self::LittleSkin => f.write_str("LittleSkin"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub provider: AuthProvider,
    pub uuid: String,
    pub username: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at_unix: u64,
    pub avatar_url: Option<String>,
}

impl Session {
    pub fn expired_or_stale(&self, now_unix: u64) -> bool {
        self.expires_at_unix <= now_unix + 300
    }
}

pub async fn authenticate(
    provider: AuthProvider,
    username: String,
    password: String,
) -> Result<Session, AppError> {
    match provider {
        AuthProvider::Microsoft => microsoft::authenticate_device_stub().await,
        AuthProvider::ElyBy => elyby::authenticate(username, password).await,
        AuthProvider::LittleSkin => littleskin::authenticate(username, password).await,
    }
}

pub async fn refresh(session: &Session) -> Result<Session, AppError> {
    match session.provider {
        AuthProvider::Microsoft => microsoft::refresh_session(session).await,
        AuthProvider::ElyBy => elyby::refresh(session).await,
        AuthProvider::LittleSkin => littleskin::refresh(session).await,
    }
}

pub async fn validate(session: &Session) -> Result<(), AppError> {
    match session.provider {
        AuthProvider::Microsoft => Ok(()),
        AuthProvider::ElyBy => elyby::validate(session).await,
        AuthProvider::LittleSkin => littleskin::validate(session).await,
    }
}

pub async fn invalidate(session: &Session) -> Result<(), AppError> {
    match session.provider {
        AuthProvider::Microsoft => Ok(()),
        AuthProvider::ElyBy => elyby::invalidate(session).await,
        AuthProvider::LittleSkin => littleskin::invalidate(session).await,
    }
}
