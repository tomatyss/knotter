use knotter_core::CoreError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SyncError {
    #[error("{0}")]
    Unavailable(String),
    #[error("command failed: {0}")]
    Command(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("core error: {0}")]
    Core(#[from] CoreError),
    #[cfg(feature = "dav-sync")]
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[cfg(feature = "dav-sync")]
    #[error("url error: {0}")]
    Url(#[from] url::ParseError),
    #[error("parse error: {0}")]
    Parse(String),
}

pub type Result<T> = std::result::Result<T, SyncError>;
