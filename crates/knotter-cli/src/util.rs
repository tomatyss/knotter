use anyhow::{anyhow, Result};
use chrono::{
    DateTime, FixedOffset, Local, NaiveDate, NaiveDateTime, NaiveTime, Offset, TimeZone, Utc,
};
use knotter_core::domain::{ContactId, InteractionKind};
use knotter_core::rules::DueState;
use std::str::FromStr;

const DATETIME_FORMATS: [&str; 4] = [
    "%Y-%m-%d %H:%M",
    "%Y-%m-%dT%H:%M",
    "%Y-%m-%d %H:%M:%S",
    "%Y-%m-%dT%H:%M:%S",
];

pub fn now_utc() -> i64 {
    Utc::now().timestamp()
}

pub fn local_offset() -> FixedOffset {
    Local::now().offset().fix()
}

pub fn parse_local_timestamp(input: &str) -> Result<i64> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("timestamp cannot be empty"));
    }

    if let Ok(date) = NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
        let naive = date
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| anyhow!("invalid date"))?;
        return local_to_utc_timestamp(naive);
    }

    for fmt in DATETIME_FORMATS {
        if let Ok(dt) = NaiveDateTime::parse_from_str(trimmed, fmt) {
            return local_to_utc_timestamp(dt);
        }
    }

    Err(anyhow!(
        "invalid datetime format: expected YYYY-MM-DD or YYYY-MM-DD HH:MM"
    ))
}

pub fn parse_local_date_time(date: &str, time: Option<&str>) -> Result<i64> {
    let date = NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d")
        .map_err(|_| anyhow!("invalid date format: expected YYYY-MM-DD"))?;
    let time = match time {
        Some(raw) => NaiveTime::parse_from_str(raw.trim(), "%H:%M")
            .map_err(|_| anyhow!("invalid time format: expected HH:MM"))?,
        None => NaiveTime::from_hms_opt(0, 0, 0).ok_or_else(|| anyhow!("invalid time"))?,
    };

    let naive = date.and_time(time);
    local_to_utc_timestamp(naive)
}

pub fn format_timestamp_date(ts: i64) -> String {
    let dt = DateTime::<Utc>::from_timestamp(ts, 0)
        .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap())
        .with_timezone(&Local);
    dt.format("%Y-%m-%d").to_string()
}

pub fn format_timestamp_datetime(ts: i64) -> String {
    let dt = DateTime::<Utc>::from_timestamp(ts, 0)
        .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap())
        .with_timezone(&Local);
    dt.format("%Y-%m-%d %H:%M").to_string()
}

pub fn parse_interaction_kind(raw: &str) -> Result<InteractionKind> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("interaction kind cannot be empty"));
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
            Err(anyhow!(
                "invalid interaction kind: expected call|text|hangout|email|other:<label>"
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
        return Err(anyhow!("contact id cannot be empty"));
    }
    ContactId::from_str(trimmed).map_err(|_| anyhow!("invalid contact id"))
}

fn local_to_utc_timestamp(naive: NaiveDateTime) -> Result<i64> {
    let local = Local
        .from_local_datetime(&naive)
        .single()
        .ok_or_else(|| anyhow!("ambiguous local time: {}", naive))?;
    Ok(local.with_timezone(&Utc).timestamp())
}
