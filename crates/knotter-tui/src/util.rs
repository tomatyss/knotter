use anyhow::{anyhow, Result};
use knotter_core::domain::InteractionKind;

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
