use crate::domain::ids::TagId;
use crate::error::CoreError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TagName(String);

impl TagName {
    pub fn new(raw: &str) -> Result<Self, CoreError> {
        let normalized = normalize_tag_name(raw)?;
        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tag {
    pub id: TagId,
    pub name: TagName,
}

pub fn normalize_tag_name(raw: &str) -> Result<String, CoreError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(CoreError::InvalidTagName);
    }

    let mut out = String::with_capacity(trimmed.len());
    let mut prev_dash = false;
    for ch in trimmed.chars() {
        let mut mapped = ch;
        if ch.is_whitespace() {
            mapped = '-';
        }

        if mapped == '-' {
            if prev_dash {
                continue;
            }
            prev_dash = true;
            out.push('-');
        } else {
            prev_dash = false;
            out.push(mapped.to_ascii_lowercase());
        }
    }

    if out.is_empty() {
        return Err(CoreError::InvalidTagName);
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::normalize_tag_name;

    #[test]
    fn normalize_tag_basic() {
        let value = normalize_tag_name(" Friends ").unwrap();
        assert_eq!(value, "friends");
    }

    #[test]
    fn normalize_tag_spaces() {
        let value = normalize_tag_name("design team").unwrap();
        assert_eq!(value, "design-team");
    }

    #[test]
    fn normalize_tag_collapse_dashes() {
        let value = normalize_tag_name("design   team").unwrap();
        assert_eq!(value, "design-team");
    }

    #[test]
    fn normalize_tag_empty() {
        assert!(normalize_tag_name("   ").is_err());
    }
}
