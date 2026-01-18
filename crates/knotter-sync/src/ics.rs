use crate::error::{Result, SyncError};
use chrono::{DateTime, Utc};
use knotter_core::domain::{Contact, ContactId};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy)]
pub struct IcsExportOptions {
    pub now_utc: i64,
    pub window_days: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct IcsExport {
    pub data: String,
    pub count: usize,
}

pub fn export_ics(
    contacts: &[Contact],
    tags: &HashMap<ContactId, Vec<String>>,
    options: IcsExportOptions,
) -> Result<IcsExport> {
    let mut events: Vec<&Contact> = contacts
        .iter()
        .filter(|contact| contact.next_touchpoint_at.is_some())
        .collect();
    events.sort_by_key(|contact| {
        (
            contact.next_touchpoint_at.unwrap_or(i64::MAX),
            contact.display_name.to_ascii_lowercase(),
        )
    });

    let window_end = if let Some(days) = options.window_days {
        let seconds = days
            .checked_mul(86_400)
            .ok_or_else(|| SyncError::Parse("window_days overflow".to_string()))?;
        Some(
            options
                .now_utc
                .checked_add(seconds)
                .ok_or_else(|| SyncError::Parse("window_days overflow".to_string()))?,
        )
    } else {
        None
    };

    let mut out = String::new();
    let mut count = 0usize;
    out.push_str("BEGIN:VCALENDAR\r\n");
    out.push_str("VERSION:2.0\r\n");
    out.push_str("PRODID:-//knotter//EN\r\n");
    out.push_str("CALSCALE:GREGORIAN\r\n");

    let dtstamp = format_ics_timestamp(options.now_utc)?;

    for contact in events {
        let Some(next_touchpoint_at) = contact.next_touchpoint_at else {
            continue;
        };

        if let Some(end) = window_end {
            if next_touchpoint_at < options.now_utc || next_touchpoint_at > end {
                continue;
            }
        }

        let dtstart = format_ics_timestamp(next_touchpoint_at)?;
        out.push_str("BEGIN:VEVENT\r\n");
        out.push_str(&format!("UID:{}\r\n", uid_for_contact(&contact.id)));
        out.push_str(&format!("DTSTAMP:{}\r\n", dtstamp));
        out.push_str(&format!("DTSTART:{}\r\n", dtstart));
        out.push_str(&format!(
            "SUMMARY:{}\r\n",
            escape_ics_value(&format!("Reach out to {}", contact.display_name))
        ));

        let description = build_description(contact, tags);
        if !description.is_empty() {
            out.push_str(&format!(
                "DESCRIPTION:{}\r\n",
                escape_ics_value(&description)
            ));
        }

        out.push_str("END:VEVENT\r\n");
        count += 1;
    }

    out.push_str("END:VCALENDAR\r\n");
    Ok(IcsExport { data: out, count })
}

fn build_description(contact: &Contact, tags: &HashMap<ContactId, Vec<String>>) -> String {
    let mut lines = Vec::new();
    if let Some(names) = tags.get(&contact.id) {
        if !names.is_empty() {
            let mut sorted = names.clone();
            sorted.sort_by_key(|name| name.to_ascii_lowercase());
            lines.push(format!("Tags: {}", sorted.join(", ")));
        }
    }
    lines.join("\\n")
}

fn format_ics_timestamp(ts: i64) -> Result<String> {
    let dt = DateTime::<Utc>::from_timestamp(ts, 0)
        .ok_or_else(|| SyncError::Parse(format!("invalid timestamp: {ts}")))?;
    Ok(dt.format("%Y%m%dT%H%M%SZ").to_string())
}

fn uid_for_contact(id: &ContactId) -> String {
    format!("knotter-{}@knotter.local", id)
}

fn escape_ics_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            ';' => out.push_str("\\;"),
            ',' => out.push_str("\\,"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use knotter_core::domain::{Contact, ContactId};
    use std::str::FromStr;

    fn contact_with_id(id: &str, name: &str, next_touchpoint_at: i64) -> Contact {
        Contact {
            id: ContactId::from_str(id).expect("id"),
            display_name: name.to_string(),
            email: None,
            phone: None,
            handle: None,
            timezone: None,
            next_touchpoint_at: Some(next_touchpoint_at),
            cadence_days: None,
            created_at: 0,
            updated_at: 0,
            archived_at: None,
        }
    }

    #[test]
    fn uid_is_stable_for_contact_id() {
        let id = ContactId::from_str("2d8b83e0-1b7c-4f28-9e1a-1a2d5b1e5e2d").unwrap();
        assert_eq!(
            uid_for_contact(&id),
            "knotter-2d8b83e0-1b7c-4f28-9e1a-1a2d5b1e5e2d@knotter.local"
        );
    }

    #[test]
    fn export_ics_includes_contact_event() {
        let contact = contact_with_id("2d8b83e0-1b7c-4f28-9e1a-1a2d5b1e5e2d", "Ada", 1_700_000_000);
        let export = export_ics(
            &[contact],
            &HashMap::new(),
            IcsExportOptions {
                now_utc: 1_699_000_000,
                window_days: Some(365),
            },
        )
        .expect("export");
        assert_eq!(export.count, 1);
        assert!(export.data.contains("BEGIN:VEVENT"));
        assert!(export.data.contains("SUMMARY:Reach out to Ada"));
        assert!(export
            .data
            .contains("UID:knotter-2d8b83e0-1b7c-4f28-9e1a-1a2d5b1e5e2d@knotter.local"));
    }
}
