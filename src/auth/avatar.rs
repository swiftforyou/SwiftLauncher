use std::path::PathBuf;

use crate::auth::Session;
use crate::error::AppError;
use crate::storage::data_dir;

pub fn avatar_url_for_uuid(uuid: &str) -> String {
    let clean = uuid.replace('-', "");
    format!("https://mc-heads.net/head/{clean}/48")
}

pub fn avatar_url_for_username(username: &str) -> String {
    format!("https://mc-heads.net/head/{username}/48")
}

pub fn avatar_url_for(session: &Session) -> String {
    match session.provider {
        crate::auth::AuthProvider::Microsoft => avatar_url_for_uuid(&session.uuid),
        _ => avatar_url_for_username(&session.username),
    }
}

pub fn avatar_cache_path(uuid: &str) -> Result<PathBuf, AppError> {
    let clean = uuid.replace('-', "");
    Ok(data_dir()?.join("avatars").join(format!("{clean}.png")))
}

pub async fn cache_avatar(session: &Session) -> Result<Option<PathBuf>, AppError> {
    let url = session
        .avatar_url
        .clone()
        .unwrap_or_else(|| avatar_url_for(session));
    let path = avatar_cache_path(&session.uuid)?;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    if tokio::fs::metadata(&path).await.is_ok() {
        return Ok(Some(path));
    }

    let bytes = reqwest::get(&url)
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    if bytes.len() < 32 {
        return Ok(None);
    }
    tokio::fs::write(&path, &bytes).await?;
    Ok(Some(path))
}
