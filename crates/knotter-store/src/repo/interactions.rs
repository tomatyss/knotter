use crate::error::{Result, StoreError};
use crate::temp_table::TempContactIdTable;
use knotter_core::domain::{ContactId, Interaction, InteractionId, InteractionKind};
use knotter_core::rules::next_touchpoint_after_touch;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;
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
        add_inner(self.conn, input)
    }

    pub fn add_with_reschedule(
        &self,
        now_utc: i64,
        input: InteractionNew,
        reschedule: bool,
    ) -> Result<Interaction> {
        if !reschedule {
            return self.add(input);
        }

        let tx = self.conn.unchecked_transaction()?;
        let interaction = add_with_reschedule_inner(&tx, now_utc, input, reschedule)?;

        tx.commit()?;
        Ok(interaction)
    }

    pub fn add_with_reschedule_in_tx(
        &self,
        now_utc: i64,
        input: InteractionNew,
        reschedule: bool,
    ) -> Result<Interaction> {
        if !reschedule {
            return self.add(input);
        }
        add_with_reschedule_inner(self.conn, now_utc, input, reschedule)
    }

    pub fn list_for_contact(
        &self,
        contact_id: ContactId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Interaction>> {
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

    pub fn list_for_contacts(
        &self,
        contact_ids: &[ContactId],
    ) -> Result<HashMap<ContactId, Vec<Interaction>>> {
        let mut map: HashMap<ContactId, Vec<Interaction>> = HashMap::new();
        if contact_ids.is_empty() {
            return Ok(map);
        }

        let temp_table = TempContactIdTable::create(self.conn, contact_ids)?;
        let temp_table_name = temp_table.name();

        let mut stmt = self.conn.prepare(&format!(
            "SELECT interactions.id,
                    interactions.contact_id,
                    interactions.occurred_at,
                    interactions.created_at,
                    interactions.kind,
                    interactions.note,
                    interactions.follow_up_at
             FROM interactions
             INNER JOIN {temp_table_name} tmp ON tmp.id = interactions.contact_id
             ORDER BY interactions.contact_id ASC,
                      interactions.occurred_at DESC,
                      interactions.created_at DESC,
                      interactions.id ASC;"
        ))?;

        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let interaction = interaction_from_row(row)?;
            map.entry(interaction.contact_id)
                .or_default()
                .push(interaction);
        }

        Ok(map)
    }

    pub fn latest_occurred_at_for_contacts(
        &self,
        contact_ids: &[ContactId],
    ) -> Result<HashMap<ContactId, i64>> {
        let mut map: HashMap<ContactId, i64> = HashMap::new();
        if contact_ids.is_empty() {
            return Ok(map);
        }

        let temp_table = TempContactIdTable::create(self.conn, contact_ids)?;
        let temp_table_name = temp_table.name();

        let mut stmt = self.conn.prepare(&format!(
            "SELECT interactions.contact_id, MAX(interactions.occurred_at) AS last_at
             FROM interactions
             INNER JOIN {temp_table_name} tmp ON tmp.id = interactions.contact_id
             GROUP BY interactions.contact_id;"
        ))?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let contact_id_raw: String = row.get(0)?;
            let contact_id = ContactId::from_str(&contact_id_raw)
                .map_err(|_| StoreError::InvalidId(contact_id_raw.clone()))?;
            let last_at: i64 = row.get(1)?;
            map.insert(contact_id, last_at);
        }

        Ok(map)
    }

    pub fn touch_contact(
        &self,
        now_utc: i64,
        contact_id: ContactId,
        reschedule: bool,
    ) -> Result<Interaction> {
        let tx = self.conn.unchecked_transaction()?;

        let contact_row: Option<(Option<i32>, Option<i64>)> = tx
            .query_row(
                "SELECT cadence_days, next_touchpoint_at FROM contacts WHERE id = ?1;",
                [contact_id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;

        let (cadence_days, existing_next) = match contact_row {
            Some(values) => values,
            None => return Err(StoreError::NotFound(contact_id.to_string())),
        };

        let next_touchpoint =
            next_touchpoint_after_touch(now_utc, cadence_days, reschedule, existing_next)?;

        if next_touchpoint != existing_next {
            tx.execute(
                "UPDATE contacts SET next_touchpoint_at = ?2, updated_at = ?3 WHERE id = ?1;",
                params![contact_id.to_string(), next_touchpoint, now_utc],
            )?;
        }

        let id = InteractionId::new();
        let kind = InteractionKind::other("touch")?;
        let kind_raw = serialize_kind(&kind)?;

        tx.execute(
            "INSERT INTO interactions (id, contact_id, occurred_at, created_at, kind, note, follow_up_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7);",
            params![
                id.to_string(),
                contact_id.to_string(),
                now_utc,
                now_utc,
                kind_raw,
                "",
                Option::<i64>::None,
            ],
        )?;

        tx.commit()?;

        Ok(Interaction {
            id,
            contact_id,
            occurred_at: now_utc,
            created_at: now_utc,
            kind,
            note: String::new(),
            follow_up_at: None,
        })
    }
}

fn add_with_reschedule_inner(
    conn: &Connection,
    now_utc: i64,
    input: InteractionNew,
    reschedule: bool,
) -> Result<Interaction> {
    let contact_row: Option<(Option<i32>, Option<i64>)> = conn
        .query_row(
            "SELECT cadence_days, next_touchpoint_at FROM contacts WHERE id = ?1;",
            [input.contact_id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;

    let (cadence_days, existing_next) = match contact_row {
        Some(values) => values,
        None => return Err(StoreError::NotFound(input.contact_id.to_string())),
    };

    let anchor = now_utc.max(input.occurred_at);
    let next_touchpoint =
        next_touchpoint_after_touch(anchor, cadence_days, reschedule, existing_next)?;

    if next_touchpoint != existing_next {
        conn.execute(
            "UPDATE contacts SET next_touchpoint_at = ?2, updated_at = ?3 WHERE id = ?1;",
            params![input.contact_id.to_string(), next_touchpoint, now_utc],
        )?;
    }

    add_inner(conn, input)
}

fn add_inner(conn: &Connection, input: InteractionNew) -> Result<Interaction> {
    let id = InteractionId::new();
    let kind = serialize_kind(&input.kind)?;

    conn.execute(
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

fn serialize_kind(kind: &InteractionKind) -> Result<String> {
    match kind {
        InteractionKind::Call => Ok("call".to_string()),
        InteractionKind::Text => Ok("text".to_string()),
        InteractionKind::Hangout => Ok("hangout".to_string()),
        InteractionKind::Email => Ok("email".to_string()),
        InteractionKind::Telegram => Ok("telegram".to_string()),
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
        "telegram" => Ok(InteractionKind::Telegram),
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
