pub fn normalize_phone_for_match(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut out = String::new();
    let mut saw_digit = false;

    if trimmed.starts_with('+') {
        out.push('+');
    }

    for ch in trimmed.chars() {
        if ch.is_ascii_digit() {
            out.push(ch);
            saw_digit = true;
            continue;
        }

        if matches!(ch, 'x' | 'X' | '#' | ';' | ',') {
            if !saw_digit {
                return None;
            }
            break;
        }
    }

    if !saw_digit {
        return None;
    }

    Some(out)
}

#[cfg(test)]
mod tests {
    use super::normalize_phone_for_match;

    #[test]
    fn normalize_phone_trims_and_strips_formatting() {
        let value = normalize_phone_for_match("  (415) 555-1212  ").unwrap();
        assert_eq!(value, "4155551212");
    }

    #[test]
    fn normalize_phone_preserves_leading_plus() {
        let value = normalize_phone_for_match("+1 (415) 555-1212").unwrap();
        assert_eq!(value, "+14155551212");
    }

    #[test]
    fn normalize_phone_ignores_extensions() {
        let value = normalize_phone_for_match("415-555-1212 x89").unwrap();
        assert_eq!(value, "4155551212");
    }

    #[test]
    fn normalize_phone_rejects_extension_only_values() {
        assert!(normalize_phone_for_match("ext 123").is_none());
        assert!(normalize_phone_for_match("x123").is_none());
    }

    #[test]
    fn normalize_phone_rejects_empty() {
        assert!(normalize_phone_for_match("   ").is_none());
    }
}
