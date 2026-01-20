use crate::error::Result;
use knotter_core::domain::{Contact, ContactId, TagName};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize)]
pub struct ImportReport {
    pub created: usize,
    pub updated: usize,
    pub skipped: usize,
    pub warnings: Vec<String>,
    pub dry_run: bool,
}

#[derive(Debug, Clone)]
pub struct VcfContact {
    pub display_name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub tags: Vec<TagName>,
    pub next_touchpoint_at: Option<i64>,
    pub cadence_days: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct ParsedVcf {
    pub contacts: Vec<VcfContact>,
    pub warnings: Vec<String>,
    pub skipped: usize,
}

pub fn parse_vcf(data: &str) -> Result<ParsedVcf> {
    let mut warnings = Vec::new();
    let mut contacts = Vec::new();
    let mut skipped = 0;

    let mut current: Option<RawCard> = None;
    for line in unfold_lines(data) {
        let trimmed = line.trim();
        if trimmed.eq_ignore_ascii_case("BEGIN:VCARD") {
            if current.is_some() {
                warnings.push("nested BEGIN:VCARD encountered".to_string());
            }
            current = Some(RawCard::default());
            continue;
        }

        if trimmed.eq_ignore_ascii_case("END:VCARD") {
            if let Some(card) = current.take() {
                if let Some(contact) = card.into_contact(&mut warnings, &mut skipped) {
                    contacts.push(contact);
                }
            } else {
                warnings.push("END:VCARD without matching BEGIN:VCARD".to_string());
            }
            continue;
        }

        let Some(card) = current.as_mut() else {
            continue;
        };

        let Some((key, raw_value)) = split_property(trimmed) else {
            continue;
        };

        match key.as_str() {
            "FN" => {
                let value = unescape_vcard_value(&raw_value);
                if card.fn_name.is_none() && !value.trim().is_empty() {
                    card.fn_name = Some(value.trim().to_string());
                }
            }
            "EMAIL" => {
                let value = unescape_vcard_value(&raw_value);
                if card.email.is_none() && !value.trim().is_empty() {
                    card.email = Some(value.trim().to_string());
                }
            }
            "TEL" => {
                let value = unescape_vcard_value(&raw_value);
                if card.phone.is_none() && !value.trim().is_empty() {
                    card.phone = Some(value.trim().to_string());
                }
            }
            "CATEGORIES" => {
                let raw = raw_value.trim();
                if !raw.is_empty() {
                    for item in split_escaped_commas(raw) {
                        let item = unescape_vcard_value(&item);
                        let item = item.trim();
                        if !item.is_empty() {
                            card.categories.push(item.to_string());
                        }
                    }
                }
            }
            "X-KNOTTER-NEXT-TOUCHPOINT" => {
                let value = unescape_vcard_value(&raw_value);
                if card.next_touchpoint_at.is_none() && !value.trim().is_empty() {
                    card.next_touchpoint_at = Some(value.trim().to_string());
                }
            }
            "X-KNOTTER-CADENCE-DAYS" => {
                let value = unescape_vcard_value(&raw_value);
                if card.cadence_days.is_none() && !value.trim().is_empty() {
                    card.cadence_days = Some(value.trim().to_string());
                }
            }
            _ => {}
        }
    }

    if current.is_some() {
        warnings.push("missing END:VCARD at end of file".to_string());
        if let Some(card) = current.take() {
            if let Some(contact) = card.into_contact(&mut warnings, &mut skipped) {
                contacts.push(contact);
            }
        }
    }

    Ok(ParsedVcf {
        contacts,
        warnings,
        skipped,
    })
}

pub fn export_vcf(contacts: &[Contact], tags: &HashMap<ContactId, Vec<String>>) -> Result<String> {
    let mut entries: Vec<&Contact> = contacts.iter().collect();
    entries.sort_by_key(|contact| contact.display_name.to_ascii_lowercase());

    let mut out = String::new();
    for contact in entries {
        out.push_str("BEGIN:VCARD\r\n");
        out.push_str("VERSION:3.0\r\n");
        out.push_str(&format!(
            "FN:{}\r\n",
            escape_vcard_value(&contact.display_name)
        ));

        if let Some(email) = &contact.email {
            out.push_str(&format!("EMAIL:{}\r\n", escape_vcard_value(email)));
        }
        if let Some(phone) = &contact.phone {
            out.push_str(&format!("TEL:{}\r\n", escape_vcard_value(phone)));
        }
        if let Some(names) = tags.get(&contact.id) {
            if !names.is_empty() {
                let mut sorted = names.clone();
                sorted.sort_by_key(|name| name.to_ascii_lowercase());
                let joined = sorted
                    .iter()
                    .map(|name| escape_vcard_value(name))
                    .collect::<Vec<_>>()
                    .join(",");
                out.push_str(&format!("CATEGORIES:{}\r\n", joined));
            }
        }
        if let Some(next_touchpoint_at) = contact.next_touchpoint_at {
            out.push_str(&format!(
                "X-KNOTTER-NEXT-TOUCHPOINT:{}\r\n",
                next_touchpoint_at
            ));
        }
        if let Some(cadence_days) = contact.cadence_days {
            out.push_str(&format!("X-KNOTTER-CADENCE-DAYS:{}\r\n", cadence_days));
        }

        out.push_str("END:VCARD\r\n");
    }

    Ok(out)
}

#[derive(Default)]
struct RawCard {
    fn_name: Option<String>,
    email: Option<String>,
    phone: Option<String>,
    categories: Vec<String>,
    next_touchpoint_at: Option<String>,
    cadence_days: Option<String>,
}

impl RawCard {
    fn into_contact(self, warnings: &mut Vec<String>, skipped: &mut usize) -> Option<VcfContact> {
        let display_name = match self.fn_name {
            Some(value) if !value.trim().is_empty() => value,
            _ => {
                warnings.push("missing FN; skipping vCard".to_string());
                *skipped += 1;
                return None;
            }
        };

        let mut tag_set: HashSet<TagName> = HashSet::new();
        for raw in self.categories {
            match TagName::new(&raw) {
                Ok(tag) => {
                    tag_set.insert(tag);
                }
                Err(_) => warnings.push(format!("invalid tag category: {raw}")),
            }
        }
        let mut tags: Vec<TagName> = tag_set.into_iter().collect();
        tags.sort_by(|a, b| a.as_str().cmp(b.as_str()));

        let cadence_days = match self.cadence_days {
            Some(raw) => match raw.parse::<i32>() {
                Ok(value) if value > 0 => Some(value),
                Ok(value) => {
                    warnings.push(format!("invalid cadence_days: {value}"));
                    None
                }
                Err(_) => {
                    warnings.push(format!("invalid cadence_days: {raw}"));
                    None
                }
            },
            None => None,
        };

        let next_touchpoint_at = match self.next_touchpoint_at {
            Some(raw) => match raw.parse::<i64>() {
                Ok(value) if value >= 0 => Some(value),
                Ok(value) => {
                    warnings.push(format!("invalid next_touchpoint_at: {value}"));
                    None
                }
                Err(_) => {
                    warnings.push(format!("invalid next_touchpoint_at: {raw}"));
                    None
                }
            },
            None => None,
        };

        Some(VcfContact {
            display_name,
            email: self.email,
            phone: self.phone,
            tags,
            next_touchpoint_at,
            cadence_days,
        })
    }
}

fn unfold_lines(input: &str) -> Vec<String> {
    let input = normalize_line_endings(input);
    let mut lines: Vec<String> = Vec::new();
    for raw in input.lines() {
        let line = raw;
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some(last) = lines.last_mut() {
                last.push_str(&line[1..]);
            } else {
                lines.push(line[1..].to_string());
            }
        } else {
            lines.push(line.to_string());
        }
    }
    lines
}

fn normalize_line_endings(input: &str) -> std::borrow::Cow<'_, str> {
    if !input.contains('\r') {
        return std::borrow::Cow::Borrowed(input);
    }

    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' {
            if matches!(chars.peek(), Some('\n')) {
                chars.next();
            }
            out.push('\n');
        } else {
            out.push(ch);
        }
    }
    std::borrow::Cow::Owned(out)
}

fn split_property(line: &str) -> Option<(String, String)> {
    let mut parts = line.splitn(2, ':');
    let left = parts.next()?;
    let value = parts.next()?.to_string();
    let mut name = left.split(';').next()?.trim();
    if let Some((_, group)) = name.rsplit_once('.') {
        name = group;
    }
    if name.is_empty() {
        return None;
    }
    Some((name.to_ascii_uppercase(), value))
}

fn split_escaped_commas(value: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut current = String::new();
    let mut escape = false;

    for ch in value.chars() {
        if escape {
            current.push(ch);
            escape = false;
            continue;
        }

        if ch == '\\' {
            current.push(ch);
            escape = true;
            continue;
        }

        if ch == ',' {
            items.push(current);
            current = String::new();
        } else {
            current.push(ch);
        }
    }

    items.push(current);
    items
}

fn escape_vcard_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\n"),
            ';' => out.push_str("\\;"),
            ',' => out.push_str("\\,"),
            _ => out.push(ch),
        }
    }
    out
}

fn unescape_vcard_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n') | Some('N') => out.push('\n'),
                Some('r') | Some('R') => out.push('\n'),
                Some('\\') => out.push('\\'),
                Some(';') => out.push(';'),
                Some(',') => out.push(','),
                Some(':') => out.push(':'),
                Some(other) => out.push(other),
                None => break,
            }
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn parse_vcf_basic() {
        let data = "BEGIN:VCARD\nVERSION:3.0\nFN:Jane Doe\nEMAIL:jane@example.com\nTEL:555-1234\nCATEGORIES:Friends,Work\nEND:VCARD\n";
        let parsed = parse_vcf(data).expect("parse");
        assert_eq!(parsed.contacts.len(), 1);
        let contact = &parsed.contacts[0];
        assert_eq!(contact.display_name, "Jane Doe");
        assert_eq!(contact.email.as_deref(), Some("jane@example.com"));
        assert_eq!(contact.phone.as_deref(), Some("555-1234"));
        assert_eq!(contact.tags.len(), 2);
    }

    #[test]
    fn parse_vcf_warns_on_missing_fn() {
        let data = "BEGIN:VCARD\nVERSION:3.0\nEMAIL:foo@example.com\nEND:VCARD\n";
        let parsed = parse_vcf(data).expect("parse");
        assert_eq!(parsed.contacts.len(), 0);
        assert!(parsed
            .warnings
            .iter()
            .any(|warning| warning.contains("missing")));
    }

    #[test]
    fn parse_vcf_categories_with_escaped_commas() {
        let data =
            "BEGIN:VCARD\nVERSION:3.0\nFN:Jane Doe\nCATEGORIES:friends\\,family,work\nEND:VCARD\n";
        let parsed = parse_vcf(data).expect("parse");
        assert_eq!(parsed.contacts.len(), 1);
        let tags: Vec<&str> = parsed.contacts[0]
            .tags
            .iter()
            .map(|tag| tag.as_str())
            .collect();
        assert!(tags.contains(&"friends,family"));
        assert!(tags.contains(&"work"));
    }

    #[test]
    fn parse_vcf_handles_cr_only_line_endings() {
        let data = "BEGIN:VCARD\rVERSION:3.0\rFN:Jane Doe\rEMAIL:jane@example.com\rEND:VCARD\r";
        let parsed = parse_vcf(data).expect("parse");
        assert_eq!(parsed.contacts.len(), 1);
        let contact = &parsed.contacts[0];
        assert_eq!(contact.display_name, "Jane Doe");
        assert_eq!(contact.email.as_deref(), Some("jane@example.com"));
    }

    #[test]
    fn export_vcf_includes_basic_fields() {
        let contact = Contact {
            id: ContactId::from_str("2d8b83e0-1b7c-4f28-9e1a-1a2d5b1e5e2d").unwrap(),
            display_name: "Ada Lovelace".to_string(),
            email: Some("ada@example.com".to_string()),
            phone: Some("555-0101".to_string()),
            handle: None,
            timezone: None,
            next_touchpoint_at: Some(1_700_000_000),
            cadence_days: Some(30),
            created_at: 0,
            updated_at: 0,
            archived_at: None,
        };

        let mut tag_map = HashMap::new();
        tag_map.insert(contact.id, vec!["friends".to_string(), "work".to_string()]);
        let output = export_vcf(&[contact], &tag_map).expect("export");
        assert!(output.contains("BEGIN:VCARD"));
        assert!(output.contains("FN:Ada Lovelace"));
        assert!(output.contains("EMAIL:ada@example.com"));
        assert!(output.contains("TEL:555-0101"));
        assert!(output.contains("CATEGORIES:friends,work"));
        assert!(output.contains("X-KNOTTER-NEXT-TOUCHPOINT:1700000000"));
        assert!(output.contains("X-KNOTTER-CADENCE-DAYS:30"));
    }

    #[test]
    fn vcf_export_roundtrip_parses() {
        let contact = Contact {
            id: ContactId::from_str("3b8b83e0-1b7c-4f28-9e1a-1a2d5b1e5e2d").unwrap(),
            display_name: "Grace Hopper".to_string(),
            email: Some("grace@example.com".to_string()),
            phone: None,
            handle: None,
            timezone: None,
            next_touchpoint_at: Some(1_700_123_456),
            cadence_days: Some(14),
            created_at: 0,
            updated_at: 0,
            archived_at: None,
        };
        let mut tag_map = HashMap::new();
        tag_map.insert(contact.id, vec!["pioneers".to_string()]);

        let output = export_vcf(&[contact], &tag_map).expect("export");
        let parsed = parse_vcf(&output).expect("parse");
        assert_eq!(parsed.contacts.len(), 1);
        let round = &parsed.contacts[0];
        assert_eq!(round.display_name, "Grace Hopper");
        assert_eq!(round.email.as_deref(), Some("grace@example.com"));
        assert_eq!(round.next_touchpoint_at, Some(1_700_123_456));
        assert_eq!(round.cadence_days, Some(14));
        assert_eq!(round.tags.len(), 1);
        assert_eq!(round.tags[0].as_str(), "pioneers");
    }
}
