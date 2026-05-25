use thiserror::Error;

#[derive(Debug, Clone, Error)]
pub enum AppError {
    #[error("storage error: {0}")]
    Storage(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("authentication error: {0}")]
    Auth(String),
    #[error("instance error: {0}")]
    Instance(String),
    #[error("download error: {0}")]
    Download(String),
    #[error("process error: {0}")]
    Process(String),
    #[error("serialization error: {0}")]
    Serialization(String),
}

impl From<sled::Error> for AppError {
    fn from(value: sled::Error) -> Self {
        Self::Storage(value.to_string())
    }
}

impl From<reqwest::Error> for AppError {
    fn from(value: reqwest::Error) -> Self {
        Self::Network(value.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(value: std::io::Error) -> Self {
        Self::Process(value.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(value: serde_json::Error) -> Self {
        Self::Serialization(value.to_string())
    }
}
