use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CoreError {
    #[error("display name is required")]
    EmptyDisplayName,
    #[error("invalid cadence days: {0}")]
    InvalidCadenceDays(i32),
    #[error("invalid soon days: {0}")]
    InvalidSoonDays(i64),
    #[error("invalid tag name")]
    InvalidTagName,
    #[error("invalid interaction kind label")]
    InvalidInteractionKindLabel,
    #[error("invalid timestamp")]
    InvalidTimestamp,
    #[error("timestamp must be now or later")]
    TimestampInPast,
}
