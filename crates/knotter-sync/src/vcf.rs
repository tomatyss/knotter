use crate::error::Result;
use knotter_core::domain::{
    normalize_contact_date_label, Contact, ContactDate, ContactDateKind, ContactId, TagName,
};
use knotter_core::time::parse_date_parts;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;

#[derive(Debug, Clone, Serialize)]
pub struct ImportReport {
    pub created: usize,
    pub updated: usize,
    pub skipped: usize,
    pub merge_candidates_created: usize,
    pub warnings: Vec<String>,
    pub dry_run: bool,
}

#[derive(Debug, Clone)]
pub struct VcfContact {
    pub display_name: String,
    pub emails: Vec<String>,
    pub phone: Option<String>,
    pub tags: Vec<TagName>,
    pub next_touchpoint_at: Option<i64>,
    pub cadence_days: Option<i32>,
    pub dates: Vec<ContactDateInput>,
    pub external_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ContactDateInput {
    pub kind: ContactDateKind,
    pub label: Option<String>,
    pub month: u8,
    pub day: u8,
    pub year: Option<i32>,
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
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    card.emails.push(trimmed.to_string());
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
            "BDAY" => {
                let value = unescape_vcard_value(&raw_value);
                if card.birthday.is_none() && !value.trim().is_empty() {
                    card.birthday = Some(value.trim().to_string());
                }
            }
            "X-KNOTTER-DATE" => {
                let value = unescape_vcard_value(&raw_value);
                if !value.trim().is_empty() {
                    card.date_fields.push(value.trim().to_string());
                }
            }
            "UID" => {
                let value = unescape_vcard_value(&raw_value);
                if card.uid.is_none() && !value.trim().is_empty() {
                    card.uid = Some(value.trim().to_string());
                }
            }
            "X-ABUID" => {
                let value = unescape_vcard_value(&raw_value);
                if card.ab_uid.is_none() && !value.trim().is_empty() {
                    card.ab_uid = Some(value.trim().to_string());
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

pub fn export_vcf(
    contacts: &[Contact],
    tags: &HashMap<ContactId, Vec<String>>,
    emails: &HashMap<ContactId, Vec<String>>,
    dates: &HashMap<ContactId, Vec<ContactDate>>,
) -> Result<String> {
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

        let mut email_list = emails.get(&contact.id).cloned().unwrap_or_default();
        if email_list.is_empty() {
            if let Some(email) = &contact.email {
                email_list.push(email.clone());
            }
        }
        for email in email_list {
            out.push_str(&format!("EMAIL:{}\r\n", escape_vcard_value(&email)));
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
        if let Some(contact_dates) = dates.get(&contact.id) {
            let primary_birthday = contact_dates
                .iter()
                .filter(|date| date.kind == ContactDateKind::Birthday)
                .max_by_key(|date| {
                    let has_year = date.year.is_some();
                    let label_empty = normalize_contact_date_label(date.label.clone()).is_none();
                    (has_year, label_empty)
                });

            let primary_birthday_id = primary_birthday.map(|date| date.id);
            if let Some(birthday) = primary_birthday {
                let date = format_vcard_date(birthday.month, birthday.day, birthday.year);
                out.push_str(&format!("BDAY:{}\r\n", date));
            }

            for date in contact_dates {
                let label = normalize_contact_date_label(date.label.clone()).unwrap_or_default();
                let is_primary_birthday = date.kind == ContactDateKind::Birthday
                    && primary_birthday_id.is_some_and(|id| id == date.id);
                if is_primary_birthday && label.is_empty() {
                    continue;
                }
                let date_value = format_vcard_date(date.month, date.day, date.year);
                let raw = if label.is_empty() {
                    format!("{}|{}", date.kind.as_str(), date_value)
                } else {
                    format!("{}|{}|{}", date.kind.as_str(), date_value, label)
                };
                out.push_str(&format!("X-KNOTTER-DATE:{}\r\n", escape_vcard_value(&raw)));
            }
        }

        out.push_str("END:VCARD\r\n");
    }

    Ok(out)
}

#[derive(Default)]
struct RawCard {
    fn_name: Option<String>,
    emails: Vec<String>,
    phone: Option<String>,
    categories: Vec<String>,
    next_touchpoint_at: Option<String>,
    cadence_days: Option<String>,
    birthday: Option<String>,
    date_fields: Vec<String>,
    uid: Option<String>,
    ab_uid: Option<String>,
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

        let mut emails = Vec::new();
        for raw in self.emails {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }
            if !emails
                .iter()
                .any(|value: &String| value.as_str().eq_ignore_ascii_case(trimmed))
            {
                emails.push(trimmed.to_string());
            }
        }

        let mut dates: Vec<ContactDateInput> = Vec::new();
        let mut date_index: HashMap<String, usize> = HashMap::new();

        let mut birthday_date: Option<ContactDateInput> = None;
        if let Some(raw) = self.birthday {
            match parse_vcard_date(&raw) {
                Ok((month, day, year)) => {
                    let year = normalize_date_year(year, warnings, "BDAY");
                    birthday_date = Some(ContactDateInput {
                        kind: ContactDateKind::Birthday,
                        label: None,
                        month,
                        day,
                        year,
                    });
                }
                Err(message) => warnings.push(format!("invalid BDAY: {}", message)),
            }
        }

        for raw in self.date_fields {
            match parse_knotter_date_field(&raw) {
                Ok(mut date) => {
                    date.year = normalize_date_year(date.year, warnings, "X-KNOTTER-DATE");
                    push_contact_date(&mut dates, &mut date_index, date, warnings);
                }
                Err(message) => warnings.push(format!("invalid X-KNOTTER-DATE: {}", message)),
            }
        }

        if let Some(birthday) = birthday_date {
            let birthday_year = birthday.year;
            let target_index = dates
                .iter()
                .enumerate()
                .filter(|(_, date)| {
                    date.kind == ContactDateKind::Birthday
                        && date.month == birthday.month
                        && date.day == birthday.day
                })
                .max_by_key(|(_, date)| {
                    let year_match = birthday_year.is_some() && date.year == birthday_year;
                    let has_year = date.year.is_some();
                    let label_empty = normalize_contact_date_label(date.label.clone()).is_none();
                    (year_match, label_empty, has_year)
                })
                .map(|(index, _)| index);

            if let Some(index) = target_index {
                let existing = &mut dates[index];
                if existing.year.is_none() && birthday.year.is_some() {
                    existing.year = birthday.year;
                }
            } else {
                push_contact_date(&mut dates, &mut date_index, birthday, warnings);
            }
        }

        Some(VcfContact {
            display_name,
            emails,
            phone: self.phone,
            tags,
            next_touchpoint_at,
            cadence_days,
            dates,
            external_id: normalize_external_id(self.uid.as_deref(), self.ab_uid.as_deref()),
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

fn normalize_external_id(uid: Option<&str>, ab_uid: Option<&str>) -> Option<String> {
    let raw = uid.or(ab_uid)?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lowered = trimmed.to_ascii_lowercase();
    let normalized = lowered.strip_prefix("urn:uuid:").unwrap_or(&lowered);
    Some(normalized.to_string())
}

fn format_vcard_date(month: u8, day: u8, year: Option<i32>) -> String {
    match year {
        Some(year) => format!("{year:04}-{month:02}-{day:02}"),
        None => format!("--{month:02}{day:02}"),
    }
}

fn parse_vcard_date(raw: &str) -> std::result::Result<(u8, u8, Option<i32>), String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("empty date".to_string());
    }
    let date_part = trimmed
        .split(['T', ' '])
        .next()
        .unwrap_or(trimmed)
        .trim_end_matches('Z');
    parse_date_parts(date_part).map_err(|err| err.to_string())
}

fn parse_knotter_date_field(raw: &str) -> std::result::Result<ContactDateInput, String> {
    let parts: Vec<&str> = raw.split('|').collect();
    if parts.len() < 2 {
        return Err("expected kind|date|label".to_string());
    }
    let kind = ContactDateKind::from_str(parts[0]).map_err(|_| "invalid kind".to_string())?;
    let (month, day, year) = parse_vcard_date(parts[1])?;
    let label = if parts.len() > 2 {
        let joined = parts[2..].join("|");
        normalize_contact_date_label(Some(joined))
    } else {
        None
    };
    if kind == ContactDateKind::Custom && label.is_none() {
        return Err("custom dates require a label".to_string());
    }
    Ok(ContactDateInput {
        kind,
        label,
        month,
        day,
        year,
    })
}

fn normalize_date_year(
    year: Option<i32>,
    warnings: &mut Vec<String>,
    context: &str,
) -> Option<i32> {
    match year {
        Some(value) if !(1..=9999).contains(&value) => {
            warnings.push(format!("invalid {context} year: {value}; dropping year"));
            None
        }
        other => other,
    }
}

fn push_contact_date(
    dates: &mut Vec<ContactDateInput>,
    date_index: &mut HashMap<String, usize>,
    date: ContactDateInput,
    warnings: &mut Vec<String>,
) {
    let label = date.label.clone().unwrap_or_default();
    let key = format!(
        "{}|{}|{}|{}",
        date.kind.as_str(),
        label,
        date.month,
        date.day
    );
    if let Some(&idx) = date_index.get(&key) {
        let existing = &mut dates[idx];
        match (existing.year, date.year) {
            (None, Some(_)) => {
                *existing = date;
            }
            (Some(existing_year), Some(new_year)) if existing_year != new_year => {
                let label_prefix = if label.is_empty() {
                    String::new()
                } else {
                    format!("{label} ")
                };
                warnings.push(format!(
                    "conflicting {} year for {}{:02}-{:02}; keeping {}",
                    existing.kind.as_str(),
                    label_prefix,
                    existing.month,
                    existing.day,
                    existing_year
                ));
            }
            _ => {}
        }
    } else {
        date_index.insert(key, dates.len());
        dates.push(date);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use knotter_core::domain::ContactDateId;
    use std::str::FromStr;

    #[test]
    fn parse_vcf_basic() {
        let data = "BEGIN:VCARD\nVERSION:3.0\nFN:Jane Doe\nEMAIL:jane@example.com\nTEL:555-1234\nCATEGORIES:Friends,Work\nEND:VCARD\n";
        let parsed = parse_vcf(data).expect("parse");
        assert_eq!(parsed.contacts.len(), 1);
        let contact = &parsed.contacts[0];
        assert_eq!(contact.display_name, "Jane Doe");
        assert_eq!(
            contact.emails.first().map(String::as_str),
            Some("jane@example.com")
        );
        assert_eq!(contact.phone.as_deref(), Some("555-1234"));
        assert_eq!(contact.tags.len(), 2);
    }

    #[test]
    fn parse_vcf_reads_uid_and_abuid() {
        let data = "BEGIN:VCARD\nVERSION:3.0\nUID:urn:uuid:ABC-123\nFN:Jane Doe\nEND:VCARD\n";
        let parsed = parse_vcf(data).expect("parse");
        assert_eq!(parsed.contacts.len(), 1);
        assert_eq!(parsed.contacts[0].external_id.as_deref(), Some("abc-123"));

        let data = "BEGIN:VCARD\nVERSION:3.0\nX-ABUID:XYZ\nFN:Jane Doe\nEND:VCARD\n";
        let parsed = parse_vcf(data).expect("parse");
        assert_eq!(parsed.contacts.len(), 1);
        assert_eq!(parsed.contacts[0].external_id.as_deref(), Some("xyz"));
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
    fn parse_vcf_drops_invalid_year() {
        let data = "BEGIN:VCARD\nVERSION:3.0\nFN:Ada\nBDAY:0000-01-01\nEND:VCARD\n";
        let parsed = parse_vcf(data).expect("parse");
        assert_eq!(parsed.contacts.len(), 1);
        assert!(parsed
            .warnings
            .iter()
            .any(|warning| warning.contains("invalid BDAY year")));
        assert_eq!(parsed.contacts[0].dates.len(), 1);
        assert_eq!(parsed.contacts[0].dates[0].year, None);
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
        assert_eq!(
            contact.emails.first().map(String::as_str),
            Some("jane@example.com")
        );
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
        let mut email_map = HashMap::new();
        email_map.insert(contact.id, vec!["ada@example.com".to_string()]);
        let date_map: HashMap<ContactId, Vec<ContactDate>> = HashMap::new();
        let output = export_vcf(&[contact], &tag_map, &email_map, &date_map).expect("export");
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
        let mut email_map = HashMap::new();
        email_map.insert(contact.id, vec!["grace@example.com".to_string()]);

        let date_map: HashMap<ContactId, Vec<ContactDate>> = HashMap::new();
        let output = export_vcf(&[contact], &tag_map, &email_map, &date_map).expect("export");
        let parsed = parse_vcf(&output).expect("parse");
        assert_eq!(parsed.contacts.len(), 1);
        let round = &parsed.contacts[0];
        assert_eq!(round.display_name, "Grace Hopper");
        assert_eq!(
            round.emails.first().map(String::as_str),
            Some("grace@example.com")
        );
        assert_eq!(round.next_touchpoint_at, Some(1_700_123_456));
        assert_eq!(round.cadence_days, Some(14));
        assert_eq!(round.tags.len(), 1);
        assert_eq!(round.tags[0].as_str(), "pioneers");
    }

    #[test]
    fn vcf_export_roundtrip_preserves_dates() {
        let contact = Contact {
            id: ContactId::from_str("4b8b83e0-1b7c-4f28-9e1a-1a2d5b1e5e2d").unwrap(),
            display_name: "Ada Lovelace".to_string(),
            email: Some("ada@example.com".to_string()),
            phone: None,
            handle: None,
            timezone: None,
            next_touchpoint_at: None,
            cadence_days: None,
            created_at: 0,
            updated_at: 0,
            archived_at: None,
        };
        let mut tag_map = HashMap::new();
        tag_map.insert(contact.id, vec!["friends".to_string()]);
        let mut email_map = HashMap::new();
        email_map.insert(contact.id, vec!["ada@example.com".to_string()]);
        let birthday = ContactDate {
            id: ContactDateId::new(),
            contact_id: contact.id,
            kind: ContactDateKind::Birthday,
            label: None,
            month: 2,
            day: 14,
            year: Some(1990),
            created_at: 0,
            updated_at: 0,
            source: None,
        };
        let extra_birthday = ContactDate {
            id: ContactDateId::new(),
            contact_id: contact.id,
            kind: ContactDateKind::Birthday,
            label: None,
            month: 3,
            day: 1,
            year: None,
            created_at: 0,
            updated_at: 0,
            source: None,
        };
        let custom = ContactDate {
            id: ContactDateId::new(),
            contact_id: contact.id,
            kind: ContactDateKind::Custom,
            label: Some("Wife birthday".to_string()),
            month: 2,
            day: 14,
            year: None,
            created_at: 0,
            updated_at: 0,
            source: None,
        };
        let mut date_map = HashMap::new();
        date_map.insert(
            contact.id,
            vec![birthday.clone(), extra_birthday.clone(), custom.clone()],
        );

        let output = export_vcf(&[contact], &tag_map, &email_map, &date_map).expect("export");
        assert!(output.contains("BDAY:1990-02-14"));
        assert!(output.contains("X-KNOTTER-DATE:birthday|--0301"));
        assert!(output.contains("X-KNOTTER-DATE:custom|--0214|Wife birthday"));

        let parsed = parse_vcf(&output).expect("parse");
        let round = &parsed.contacts[0];
        let mut kinds = round
            .dates
            .iter()
            .map(|date| {
                (
                    date.kind,
                    date.label.clone(),
                    date.month,
                    date.day,
                    date.year,
                )
            })
            .collect::<Vec<_>>();
        kinds.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));

        assert!(kinds.iter().any(|item| {
            item.0 == ContactDateKind::Birthday
                && item.2 == 2
                && item.3 == 14
                && item.4 == Some(1990)
        }));
        assert!(kinds.iter().any(|item| {
            item.0 == ContactDateKind::Birthday && item.2 == 3 && item.3 == 1 && item.4.is_none()
        }));
        assert!(kinds.iter().any(|item| {
            item.0 == ContactDateKind::Custom
                && item.1.as_deref() == Some("Wife birthday")
                && item.2 == 2
                && item.3 == 14
                && item.4.is_none()
        }));
    }

    #[test]
    fn vcf_export_roundtrip_preserves_labeled_birthday() {
        let contact = Contact {
            id: ContactId::from_str("0b8b83e0-1b7c-4f28-9e1a-1a2d5b1e5e2d").unwrap(),
            display_name: "Grace Hopper".to_string(),
            email: Some("grace@example.com".to_string()),
            phone: None,
            handle: None,
            timezone: None,
            next_touchpoint_at: None,
            cadence_days: None,
            created_at: 0,
            updated_at: 0,
            archived_at: None,
        };
        let mut tag_map = HashMap::new();
        tag_map.insert(contact.id, vec!["friends".to_string()]);
        let mut email_map = HashMap::new();
        email_map.insert(contact.id, vec!["grace@example.com".to_string()]);
        let birthday = ContactDate {
            id: ContactDateId::new(),
            contact_id: contact.id,
            kind: ContactDateKind::Birthday,
            label: Some("Legal".to_string()),
            month: 7,
            day: 4,
            year: Some(1906),
            created_at: 0,
            updated_at: 0,
            source: None,
        };
        let mut date_map = HashMap::new();
        date_map.insert(contact.id, vec![birthday.clone()]);

        let output = export_vcf(&[contact], &tag_map, &email_map, &date_map).expect("export");
        assert!(output.contains("BDAY:1906-07-04"));
        assert!(output.contains("X-KNOTTER-DATE:birthday|1906-07-04|Legal"));

        let parsed = parse_vcf(&output).expect("parse");
        let round = &parsed.contacts[0];
        let birthdays: Vec<_> = round
            .dates
            .iter()
            .filter(|date| date.kind == ContactDateKind::Birthday)
            .collect();
        assert_eq!(birthdays.len(), 1);
        assert_eq!(birthdays[0].label.as_deref(), Some("Legal"));
        assert_eq!(birthdays[0].year, Some(1906));
    }

    #[test]
    fn vcf_export_roundtrip_preserves_unlabeled_when_labeled_has_year() {
        let contact = Contact {
            id: ContactId::from_str("1b8b83e0-1b7c-4f28-9e1a-1a2d5b1e5e2d").unwrap(),
            display_name: "Grace Hopper".to_string(),
            email: Some("grace@example.com".to_string()),
            phone: None,
            handle: None,
            timezone: None,
            next_touchpoint_at: None,
            cadence_days: None,
            created_at: 0,
            updated_at: 0,
            archived_at: None,
        };
        let mut tag_map = HashMap::new();
        tag_map.insert(contact.id, vec!["friends".to_string()]);
        let mut email_map = HashMap::new();
        email_map.insert(contact.id, vec!["grace@example.com".to_string()]);
        let unlabeled = ContactDate {
            id: ContactDateId::new(),
            contact_id: contact.id,
            kind: ContactDateKind::Birthday,
            label: None,
            month: 7,
            day: 4,
            year: None,
            created_at: 0,
            updated_at: 0,
            source: None,
        };
        let labeled = ContactDate {
            id: ContactDateId::new(),
            contact_id: contact.id,
            kind: ContactDateKind::Birthday,
            label: Some("Legal".to_string()),
            month: 7,
            day: 4,
            year: Some(1906),
            created_at: 0,
            updated_at: 0,
            source: None,
        };
        let mut date_map = HashMap::new();
        date_map.insert(contact.id, vec![unlabeled.clone(), labeled.clone()]);

        let output = export_vcf(&[contact], &tag_map, &email_map, &date_map).expect("export");
        assert!(output.contains("BDAY:1906-07-04"));
        assert!(output.contains("X-KNOTTER-DATE:birthday|--0704"));
        assert!(output.contains("X-KNOTTER-DATE:birthday|1906-07-04|Legal"));

        let parsed = parse_vcf(&output).expect("parse");
        let round = &parsed.contacts[0];
        let birthdays: Vec<_> = round
            .dates
            .iter()
            .filter(|date| date.kind == ContactDateKind::Birthday)
            .collect();
        assert_eq!(birthdays.len(), 2);
        let unlabeled_round = birthdays
            .iter()
            .find(|date| date.label.is_none())
            .expect("unlabeled");
        assert_eq!(unlabeled_round.year, None);
        let labeled_round = birthdays
            .iter()
            .find(|date| date.label.as_deref() == Some("Legal"))
            .expect("labeled");
        assert_eq!(labeled_round.year, Some(1906));
    }

    #[test]
    fn parse_vcf_prefers_unlabeled_when_year_mismatch() {
        let data = concat!(
            "BEGIN:VCARD\n",
            "VERSION:3.0\n",
            "FN:Grace Hopper\n",
            "BDAY:1906-07-04\n",
            "X-KNOTTER-DATE:birthday|--0704\n",
            "X-KNOTTER-DATE:birthday|1907-07-04|Legal\n",
            "END:VCARD\n",
        );
        let parsed = parse_vcf(data).expect("parse");
        assert_eq!(parsed.contacts.len(), 1);
        let birthdays: Vec<_> = parsed.contacts[0]
            .dates
            .iter()
            .filter(|date| date.kind == ContactDateKind::Birthday)
            .collect();
        assert_eq!(birthdays.len(), 2);
        let unlabeled = birthdays
            .iter()
            .find(|date| date.label.is_none())
            .expect("unlabeled");
        assert_eq!(unlabeled.year, Some(1906));
        let labeled = birthdays
            .iter()
            .find(|date| date.label.as_deref() == Some("Legal"))
            .expect("labeled");
        assert_eq!(labeled.year, Some(1907));
    }
}
