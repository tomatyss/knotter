use crate::domain::{ContactId, InteractionId};
use crate::rules::DueState;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContactListItemDto {
    pub id: ContactId,
    pub display_name: String,
    pub due_state: DueState,
    pub next_touchpoint_at: Option<i64>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InteractionDto {
    pub id: InteractionId,
    pub occurred_at: i64,
    pub kind: String,
    pub note: String,
    pub follow_up_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContactDetailDto {
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
    pub tags: Vec<String>,
    pub recent_interactions: Vec<InteractionDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReminderOutputDto {
    pub overdue: Vec<ContactListItemDto>,
    pub today: Vec<ContactListItemDto>,
    pub soon: Vec<ContactListItemDto>,
}
