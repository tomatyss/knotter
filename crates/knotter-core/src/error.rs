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
    #[error("invalid contact date kind: {0}")]
    InvalidContactDateKind(String),
    #[error("invalid contact date month: {0}")]
    InvalidContactDateMonth(u8),
    #[error("invalid contact date day: {month}-{day}")]
    InvalidContactDateDay { month: u8, day: u8 },
    #[error("invalid contact date year: {0}")]
    InvalidContactDateYear(i32),
    #[error("contact date label is required for custom dates")]
    MissingContactDateLabel,
    #[error("invalid contact date label")]
    InvalidContactDateLabel,
    #[error("invalid timestamp")]
    InvalidTimestamp,
    #[error("timestamp must be now or later")]
    TimestampInPast,
}
