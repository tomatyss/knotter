use crate::domain::ids::ContactId;
use crate::error::CoreError;
use crate::rules::cadence::MAX_CADENCE_DAYS;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contact {
    pub id: ContactId,
    pub display_name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub handle: Option<String>,
    pub timezone: Option<String>,
    pub next_touchpoint_at: Option<i64>,
    pub cadence_days: Option<i32>,
    pub created_at: i64,
    pub updated_at: i64,
    pub archived_at: Option<i64>,
}

impl Contact {
    pub fn validate(&self) -> Result<(), CoreError> {
        if self.display_name.trim().is_empty() {
            return Err(CoreError::EmptyDisplayName);
        }

        if let Some(cadence) = self.cadence_days {
            if cadence <= 0 || cadence > MAX_CADENCE_DAYS {
                return Err(CoreError::InvalidCadenceDays(cadence));
            }
        }

        Ok(())
    }
}
