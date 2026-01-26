use crate::domain::{ContactId, InteractionId};
use crate::rules::DueState;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContactListItemDto {
    pub id: ContactId,
    pub display_name: String,
    pub due_state: DueState,
    pub next_touchpoint_at: Option<i64>,
    pub archived_at: Option<i64>,
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
    pub emails: Vec<String>,
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
pub struct ExportMetadataDto {
    pub exported_at: i64,
    pub app_version: String,
    pub schema_version: i64,
    pub format_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportInteractionDto {
    pub id: InteractionId,
    pub occurred_at: i64,
    pub created_at: i64,
    pub kind: String,
    pub note: String,
    pub follow_up_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportContactDto {
    pub id: ContactId,
    pub display_name: String,
    pub email: Option<String>,
    pub emails: Vec<String>,
    pub phone: Option<String>,
    pub handle: Option<String>,
    pub timezone: Option<String>,
    pub next_touchpoint_at: Option<i64>,
    pub cadence_days: Option<i32>,
    pub created_at: i64,
    pub updated_at: i64,
    pub archived_at: Option<i64>,
    pub tags: Vec<String>,
    pub interactions: Vec<ExportInteractionDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportSnapshotDto {
    pub metadata: ExportMetadataDto,
    pub contacts: Vec<ExportContactDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReminderOutputDto {
    pub overdue: Vec<ContactListItemDto>,
    pub today: Vec<ContactListItemDto>,
    pub soon: Vec<ContactListItemDto>,
}

impl ReminderOutputDto {
    pub fn from_items(items: Vec<ContactListItemDto>) -> Self {
        let mut output = Self {
            overdue: Vec::new(),
            today: Vec::new(),
            soon: Vec::new(),
        };

        for item in items {
            match item.due_state {
                DueState::Overdue => output.overdue.push(item),
                DueState::Today => output.today.push(item),
                DueState::Soon => output.soon.push(item),
                DueState::Scheduled | DueState::Unscheduled => {}
            }
        }

        output
    }

    pub fn is_empty(&self) -> bool {
        self.overdue.is_empty() && self.today.is_empty() && self.soon.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::{ContactListItemDto, ReminderOutputDto};
    use crate::domain::ContactId;
    use crate::rules::DueState;

    #[test]
    fn reminder_output_groups_only_due_buckets() {
        let items = vec![
            ContactListItemDto {
                id: ContactId::new(),
                display_name: "Ada".to_string(),
                due_state: DueState::Overdue,
                next_touchpoint_at: Some(1),
                archived_at: None,
                tags: vec!["friends".to_string()],
            },
            ContactListItemDto {
                id: ContactId::new(),
                display_name: "Grace".to_string(),
                due_state: DueState::Today,
                next_touchpoint_at: Some(2),
                archived_at: None,
                tags: Vec::new(),
            },
            ContactListItemDto {
                id: ContactId::new(),
                display_name: "Tim".to_string(),
                due_state: DueState::Soon,
                next_touchpoint_at: Some(3),
                archived_at: None,
                tags: Vec::new(),
            },
            ContactListItemDto {
                id: ContactId::new(),
                display_name: "Linus".to_string(),
                due_state: DueState::Scheduled,
                next_touchpoint_at: Some(4),
                archived_at: None,
                tags: Vec::new(),
            },
            ContactListItemDto {
                id: ContactId::new(),
                display_name: "Ken".to_string(),
                due_state: DueState::Unscheduled,
                next_touchpoint_at: None,
                archived_at: None,
                tags: Vec::new(),
            },
        ];

        let output = ReminderOutputDto::from_items(items);

        assert_eq!(output.overdue.len(), 1);
        assert_eq!(output.today.len(), 1);
        assert_eq!(output.soon.len(), 1);
        assert_eq!(output.overdue[0].display_name, "Ada");
        assert_eq!(output.today[0].display_name, "Grace");
        assert_eq!(output.soon[0].display_name, "Tim");
    }

    #[test]
    fn reminder_output_empty() {
        let output = ReminderOutputDto::from_items(Vec::new());
        assert!(output.is_empty());
    }
}
