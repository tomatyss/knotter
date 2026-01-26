use knotter_core::CoreError;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("sqlite error: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("core error: {0}")]
    Core(#[from] CoreError),
    #[error("missing home directory")]
    MissingHomeDir,
    #[error("invalid id string: {0}")]
    InvalidId(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("migration error: {0}")]
    Migration(String),
    #[error("invalid data path: {0}")]
    InvalidDataPath(PathBuf),
    #[error("invalid backup path (matches database): {0}")]
    InvalidBackupPath(PathBuf),
    #[error("unsupported interaction kind: {0}")]
    InvalidInteractionKind(String),
    #[error("invalid filter: {0}")]
    InvalidFilter(String),
    #[error("duplicate email: {0}")]
    DuplicateEmail(String),
}

pub type Result<T> = std::result::Result<T, StoreError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreErrorKind {
    Io,
    Sql,
    Core,
    MissingHomeDir,
    InvalidId,
    NotFound,
    Migration,
    InvalidDataPath,
    InvalidBackupPath,
    InvalidInteractionKind,
    InvalidFilter,
    DuplicateEmail,
}

impl StoreError {
    pub fn kind(&self) -> StoreErrorKind {
        match self {
            StoreError::Io(_) => StoreErrorKind::Io,
            StoreError::Sql(_) => StoreErrorKind::Sql,
            StoreError::Core(_) => StoreErrorKind::Core,
            StoreError::MissingHomeDir => StoreErrorKind::MissingHomeDir,
            StoreError::InvalidId(_) => StoreErrorKind::InvalidId,
            StoreError::NotFound(_) => StoreErrorKind::NotFound,
            StoreError::Migration(_) => StoreErrorKind::Migration,
            StoreError::InvalidDataPath(_) => StoreErrorKind::InvalidDataPath,
            StoreError::InvalidBackupPath(_) => StoreErrorKind::InvalidBackupPath,
            StoreError::InvalidInteractionKind(_) => StoreErrorKind::InvalidInteractionKind,
            StoreError::InvalidFilter(_) => StoreErrorKind::InvalidFilter,
            StoreError::DuplicateEmail(_) => StoreErrorKind::DuplicateEmail,
        }
    }
}
