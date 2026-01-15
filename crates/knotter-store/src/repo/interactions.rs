use crate::error::{Result, StoreError};
use knotter_core::domain::{ContactId, Interaction, InteractionId, InteractionKind};
use rusqlite::{params, Connection};
use std::str::FromStr;

#[derive(Debug, Clone)]
pub struct InteractionNew {
    pub contact_id: ContactId,
    pub occurred_at: i64,
    pub created_at: i64,
    pub kind: InteractionKind,
    pub note: String,
    pub follow_up_at: Option<i64>,
}

pub struct InteractionsRepo<'a> {
    conn: &'a Connection,
}

impl<'a> InteractionsRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn add(&self, input: InteractionNew) -> Result<Interaction> {
        let id = InteractionId::new();
        let kind = serialize_kind(&input.kind)?;

        self.conn.execute(
            "INSERT INTO interactions (id, contact_id, occurred_at, created_at, kind, note, follow_up_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7);",
            params![
                id.to_string(),
                input.contact_id.to_string(),
                input.occurred_at,
                input.created_at,
                kind,
                input.note,
                input.follow_up_at,
            ],
        )?;

        Ok(Interaction {
            id,
            contact_id: input.contact_id,
            occurred_at: input.occurred_at,
            created_at: input.created_at,
            kind: input.kind,
            note: input.note,
            follow_up_at: input.follow_up_at,
        })
    }

    pub fn list_for_contact(&self, contact_id: ContactId, limit: i64, offset: i64) -> Result<Vec<Interaction>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, contact_id, occurred_at, created_at, kind, note, follow_up_at
             FROM interactions
             WHERE contact_id = ?1
             ORDER BY occurred_at DESC
             LIMIT ?2 OFFSET ?3;",
        )?;
        let mut rows = stmt.query(params![contact_id.to_string(), limit, offset])?;
        let mut items = Vec::new();
        while let Some(row) = rows.next()? {
            items.push(interaction_from_row(row)?);
        }
        Ok(items)
    }
}

fn serialize_kind(kind: &InteractionKind) -> Result<String> {
    match kind {
        InteractionKind::Call => Ok("call".to_string()),
        InteractionKind::Text => Ok("text".to_string()),
        InteractionKind::Hangout => Ok("hangout".to_string()),
        InteractionKind::Email => Ok("email".to_string()),
        InteractionKind::Other(label) => {
            let trimmed = label.trim();
            if trimmed.is_empty() {
                return Err(StoreError::InvalidInteractionKind(label.clone()));
            }
            Ok(format!("other:{}", trimmed.to_ascii_lowercase()))
        }
    }
}

fn parse_kind(raw: &str) -> Result<InteractionKind> {
    match raw {
        "call" => Ok(InteractionKind::Call),
        "text" => Ok(InteractionKind::Text),
        "hangout" => Ok(InteractionKind::Hangout),
        "email" => Ok(InteractionKind::Email),
        _ => {
            if let Some(rest) = raw.strip_prefix("other:") {
                if rest.trim().is_empty() {
                    return Err(StoreError::InvalidInteractionKind(raw.to_string()));
                }
                return Ok(InteractionKind::Other(rest.trim().to_ascii_lowercase()));
            }
            Err(StoreError::InvalidInteractionKind(raw.to_string()))
        }
    }
}

fn interaction_from_row(row: &rusqlite::Row<'_>) -> Result<Interaction> {
    let id_str: String = row.get(0)?;
    let id = InteractionId::from_str(&id_str).map_err(|_| StoreError::InvalidId(id_str.clone()))?;
    let contact_id_str: String = row.get(1)?;
    let contact_id = ContactId::from_str(&contact_id_str)
        .map_err(|_| StoreError::InvalidId(contact_id_str.clone()))?;
    let kind_raw: String = row.get(4)?;
    let kind = parse_kind(&kind_raw)?;
    Ok(Interaction {
        id,
        contact_id,
        occurred_at: row.get(2)?,
        created_at: row.get(3)?,
        kind,
        note: row.get(5)?,
        follow_up_at: row.get(6)?,
    })
}
