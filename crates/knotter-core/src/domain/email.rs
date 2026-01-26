pub fn normalize_email(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::normalize_email;

    #[test]
    fn normalize_email_trims_and_lowercases() {
        let value = normalize_email("  Ada@Example.com ");
        assert_eq!(value.as_deref(), Some("ada@example.com"));
    }
}
