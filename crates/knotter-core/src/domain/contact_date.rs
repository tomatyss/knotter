use crate::domain::ids::{ContactDateId, ContactId};
use crate::error::CoreError;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContactDateKind {
    Birthday,
    NameDay,
    Custom,
}

impl ContactDateKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ContactDateKind::Birthday => "birthday",
            ContactDateKind::NameDay => "name_day",
            ContactDateKind::Custom => "custom",
        }
    }
}

impl FromStr for ContactDateKind {
    type Err = CoreError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let value = raw.trim().to_ascii_lowercase();
        match value.as_str() {
            "birthday" => Ok(ContactDateKind::Birthday),
            "nameday" | "name-day" | "name_day" => Ok(ContactDateKind::NameDay),
            "custom" => Ok(ContactDateKind::Custom),
            _ => Err(CoreError::InvalidContactDateKind(raw.to_string())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContactDate {
    pub id: ContactDateId,
    pub contact_id: ContactId,
    pub kind: ContactDateKind,
    pub label: Option<String>,
    pub month: u8,
    pub day: u8,
    pub year: Option<i32>,
    pub created_at: i64,
    pub updated_at: i64,
    pub source: Option<String>,
}

impl ContactDate {
    pub fn validate(&self) -> Result<(), CoreError> {
        let label = self.label.as_deref().map(str::trim);
        if matches!(self.kind, ContactDateKind::Custom)
            && label.is_none_or(|value| value.is_empty())
        {
            return Err(CoreError::MissingContactDateLabel);
        }
        if let Some(value) = label {
            if value.is_empty() {
                return Err(CoreError::InvalidContactDateLabel);
            }
        }

        if let Some(year) = self.year {
            if !(1..=9999).contains(&year) {
                return Err(CoreError::InvalidContactDateYear(year));
            }
        }

        validate_month_day(self.month, self.day, self.year)?;

        Ok(())
    }
}

pub fn normalize_contact_date_label(label: Option<String>) -> Option<String> {
    label.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn validate_month_day(month: u8, day: u8, year: Option<i32>) -> Result<(), CoreError> {
    if !(1..=12).contains(&month) {
        return Err(CoreError::InvalidContactDateMonth(month));
    }
    if day == 0 || day > 31 {
        return Err(CoreError::InvalidContactDateDay { month, day });
    }

    let validation_year = year.unwrap_or(2000);
    if NaiveDate::from_ymd_opt(validation_year, month.into(), day.into()).is_none() {
        return Err(CoreError::InvalidContactDateDay { month, day });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ContactDate, ContactDateKind};
    use crate::domain::{ContactDateId, ContactId};

    #[test]
    fn contact_date_accepts_leap_day_without_year() {
        let date = ContactDate {
            id: ContactDateId::new(),
            contact_id: ContactId::new(),
            kind: ContactDateKind::Birthday,
            label: None,
            month: 2,
            day: 29,
            year: None,
            created_at: 0,
            updated_at: 0,
            source: None,
        };
        assert!(date.validate().is_ok());
    }

    #[test]
    fn contact_date_rejects_empty_custom_label() {
        let date = ContactDate {
            id: ContactDateId::new(),
            contact_id: ContactId::new(),
            kind: ContactDateKind::Custom,
            label: Some(" ".to_string()),
            month: 1,
            day: 1,
            year: None,
            created_at: 0,
            updated_at: 0,
            source: None,
        };
        assert!(date.validate().is_err());
    }
}
