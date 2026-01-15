use crate::domain::ids::{ContactId, InteractionId};
use crate::error::CoreError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionKind {
    Call,
    Text,
    Hangout,
    Email,
    Other(String),
}

impl InteractionKind {
    pub fn other(label: &str) -> Result<Self, CoreError> {
        let trimmed = label.trim();
        if trimmed.is_empty() {
            return Err(CoreError::InvalidInteractionKindLabel);
        }
        Ok(Self::Other(trimmed.to_ascii_lowercase()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Interaction {
    pub id: InteractionId,
    pub contact_id: ContactId,
    pub occurred_at: i64,
    pub created_at: i64,
    pub kind: InteractionKind,
    pub note: String,
    pub follow_up_at: Option<i64>,
}
