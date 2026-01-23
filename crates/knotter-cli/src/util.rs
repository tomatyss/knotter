use crate::error::invalid_input;
use anyhow::Result;
use knotter_core::domain::{ContactId, InteractionKind};
use knotter_core::rules::DueState;
pub use knotter_core::time::{
    format_timestamp_date, format_timestamp_datetime, local_offset, now_utc,
    parse_local_date_time_with_precision, parse_local_timestamp,
    parse_local_timestamp_with_precision,
};
use std::str::FromStr;

pub fn parse_interaction_kind(raw: &str) -> Result<InteractionKind> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(invalid_input("interaction kind cannot be empty"));
    }
    let lower = trimmed.to_ascii_lowercase();
    match lower.as_str() {
        "call" => Ok(InteractionKind::Call),
        "text" => Ok(InteractionKind::Text),
        "hangout" => Ok(InteractionKind::Hangout),
        "email" => Ok(InteractionKind::Email),
        _ => {
            if lower.starts_with("other:") {
                let rest = &trimmed[6..];
                return Ok(InteractionKind::other(rest)?);
            }
            Err(invalid_input(
                "invalid interaction kind: expected call|text|hangout|email|other:<label>",
            ))
        }
    }
}

pub fn format_interaction_kind(kind: &InteractionKind) -> String {
    match kind {
        InteractionKind::Call => "call".to_string(),
        InteractionKind::Text => "text".to_string(),
        InteractionKind::Hangout => "hangout".to_string(),
        InteractionKind::Email => "email".to_string(),
        InteractionKind::Other(label) => format!("other:{}", label),
    }
}

pub fn due_state_label(state: DueState) -> &'static str {
    match state {
        DueState::Unscheduled => "unscheduled",
        DueState::Overdue => "overdue",
        DueState::Today => "today",
        DueState::Soon => "soon",
        DueState::Scheduled => "scheduled",
    }
}

pub fn parse_contact_id(raw: &str) -> Result<ContactId> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(invalid_input("contact id cannot be empty"));
    }
    ContactId::from_str(trimmed).map_err(|_| invalid_input("invalid contact id"))
}
