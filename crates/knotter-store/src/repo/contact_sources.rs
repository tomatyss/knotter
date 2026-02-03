use crate::error::{Result, StoreError};
use knotter_core::domain::ContactId;
use rusqlite::{params, Connection, OptionalExtension};
use std::str::FromStr;

#[derive(Debug, Clone)]
pub struct ContactSource {
    pub contact_id: ContactId,
    pub source: String,
    pub external_id: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct ContactSourceNew {
    pub contact_id: ContactId,
    pub source: String,
    pub external_id: String,
}

pub struct ContactSourcesRepo<'a> {
    conn: &'a Connection,
}

impl<'a> ContactSourcesRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn find_contact_id(&self, source: &str, external_id: &str) -> Result<Option<ContactId>> {
        let value: Option<String> = self
            .conn
            .query_row(
                "SELECT contact_id FROM contact_sources WHERE source = ?1 AND external_id = ?2;",
                params![source, external_id],
                |row| row.get(0),
            )
            .optional()?;
        value
            .map(|value| {
                ContactId::from_str(&value).map_err(|_| StoreError::InvalidId(value.clone()))
            })
            .transpose()
    }

    pub fn upsert(&self, now_utc: i64, mapping: ContactSourceNew) -> Result<()> {
        if let Some(existing) = self.find_contact_id(&mapping.source, &mapping.external_id)? {
            if existing != mapping.contact_id {
                return Err(StoreError::DuplicateContactSource(
                    mapping.source,
                    mapping.external_id,
                ));
            }
            self.conn.execute(
                "UPDATE contact_sources
                 SET updated_at = ?1
                 WHERE source = ?2 AND external_id = ?3;",
                params![now_utc, mapping.source, mapping.external_id],
            )?;
            return Ok(());
        }

        self.conn.execute(
            "INSERT INTO contact_sources
             (contact_id, source, external_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5);",
            params![
                mapping.contact_id.to_string(),
                mapping.source,
                mapping.external_id,
                now_utc,
                now_utc
            ],
        )?;
        Ok(())
    }
}
