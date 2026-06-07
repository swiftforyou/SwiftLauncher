use std::io::Read;
use std::path::{Path, PathBuf};

use ring::digest;
use sha1::{Digest, Sha1};
use tokio::task;

use crate::error::AppError;

#[derive(Debug, Clone)]
pub struct AssetCheck {
    pub path: PathBuf,
    pub expected_sha1: String,
}

pub async fn verify_assets(assets: Vec<AssetCheck>) -> Result<(usize, usize), AppError> {
    let total = assets.len();
    let mut ok = 0;
    for asset in assets {
        if sha1_file(&asset.path).await.unwrap_or_default() == asset.expected_sha1 {
            ok += 1;
        }
    }
    Ok((ok, total))
}

pub async fn sha1_file(path: &Path) -> Result<String, AppError> {
    let path = path.to_path_buf();
    task::spawn_blocking(move || {
        let mut file = std::fs::File::open(path).map_err(|e| AppError::Download(e.to_string()))?;
        let mut context = digest::Context::new(&digest::SHA1_FOR_LEGACY_USE_ONLY);
        let mut hasher = Sha1::new();
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = file
                .read(&mut buffer)
                .map_err(|e| AppError::Download(e.to_string()))?;
            if read == 0 {
                break;
            }
            context.update(&buffer[..read]);
            hasher.update(&buffer[..read]);
        }
        let _ring_digest = context.finish();
        let hash = hasher.finalize();
        let mut hex = String::with_capacity(hash.len() * 2);
        for byte in hash {
            use std::fmt::Write;
            let _ = write!(&mut hex, "{byte:02x}");
        }
        Ok(hex)
    })
    .await
    .map_err(|e| AppError::Download(e.to_string()))?
}
