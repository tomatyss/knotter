use crate::error::{Result, StoreError};
use knotter_core::domain::ContactId;
use rusqlite::{params, Connection, OptionalExtension};
use std::str::FromStr;

// ASCII-only normalization to keep SQLite lower() and Rust matching consistent.
fn normalize_external_id_key(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

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

#[derive(Debug, Clone)]
pub struct ContactSourceMatch {
    pub contact_id: ContactId,
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
        Ok(self
            .find_contact(source, external_id)?
            .map(|matched| matched.contact_id))
    }

    pub fn find_contact(
        &self,
        source: &str,
        external_id: &str,
    ) -> Result<Option<ContactSourceMatch>> {
        let normalized = normalize_external_id_key(external_id);
        let value: Option<(String, String)> = self
            .conn
            .query_row(
                "SELECT contact_id, external_id
                 FROM contact_sources
                 WHERE source = ?1 AND external_id_norm = ?2
                 LIMIT 1;",
                params![source, normalized],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;

        value
            .map(|(contact_id, external_id)| {
                let contact_id = ContactId::from_str(&contact_id)
                    .map_err(|_| StoreError::InvalidId(contact_id.clone()))?;
                Ok(ContactSourceMatch {
                    contact_id,
                    external_id,
                })
            })
            .transpose()
    }

    pub fn find_case_insensitive_matches(
        &self,
        source: &str,
        external_id: &str,
    ) -> Result<Vec<ContactSource>> {
        let normalized = normalize_external_id_key(external_id);
        let mut stmt = self.conn.prepare(
            "SELECT contact_id, source, external_id, created_at, updated_at
             FROM contact_sources
             WHERE source = ?1
               AND (external_id_norm = ?2 OR lower(trim(external_id)) = ?2)
             ORDER BY updated_at DESC, external_id ASC;",
        )?;
        let rows = stmt.query_map(params![source, normalized], |row| {
            let contact_id: String = row.get(0)?;
            let source: String = row.get(1)?;
            let external_id: String = row.get(2)?;
            let created_at: i64 = row.get(3)?;
            let updated_at: i64 = row.get(4)?;
            Ok((contact_id, source, external_id, created_at, updated_at))
        })?;

        let mut matches = Vec::new();
        for row in rows {
            let (contact_id, source, external_id, created_at, updated_at) = row?;
            let contact_id = ContactId::from_str(&contact_id)
                .map_err(|_| StoreError::InvalidId(contact_id.clone()))?;
            matches.push(ContactSource {
                contact_id,
                source,
                external_id,
                created_at,
                updated_at,
            });
        }
        Ok(matches)
    }

    pub fn collapse_case_insensitive_duplicates(
        &self,
        now_utc: i64,
        source: &str,
        contact_id: ContactId,
        keep_external_id: &str,
    ) -> Result<usize> {
        let normalized = normalize_external_id_key(keep_external_id);
        let removed = self.conn.execute(
            "DELETE FROM contact_sources
             WHERE source = ?1
               AND contact_id = ?2
               AND (external_id_norm = ?3 OR lower(trim(external_id)) = ?3)
               AND external_id <> ?4;",
            params![source, contact_id.to_string(), normalized, keep_external_id],
        )?;
        self.conn.execute(
            "UPDATE contact_sources
             SET external_id_norm = ?1,
                 updated_at = ?2
             WHERE source = ?3
               AND external_id = ?4;",
            params![normalized, now_utc, source, keep_external_id],
        )?;
        Ok(removed)
    }

    pub fn upsert(&self, now_utc: i64, mapping: ContactSourceNew) -> Result<()> {
        let normalized = normalize_external_id_key(&mapping.external_id);
        let matches = self.find_case_insensitive_matches(&mapping.source, &mapping.external_id)?;
        if matches.is_empty() {
            self.conn.execute(
                "INSERT INTO contact_sources
                 (contact_id, source, external_id, external_id_norm, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6);",
                params![
                    mapping.contact_id.to_string(),
                    mapping.source,
                    mapping.external_id,
                    normalized,
                    now_utc,
                    now_utc
                ],
            )?;
            return Ok(());
        }

        if matches
            .iter()
            .any(|existing| existing.contact_id != mapping.contact_id)
        {
            return Err(StoreError::DuplicateContactSource(
                mapping.source,
                mapping.external_id,
            ));
        }

        let existing = matches
            .iter()
            .find(|existing| existing.external_id == mapping.external_id)
            .unwrap_or(&matches[0]);
        self.conn.execute(
            "UPDATE contact_sources
             SET updated_at = ?1,
                 external_id_norm = ?2
             WHERE source = ?3 AND external_id = ?4;",
            params![now_utc, normalized, mapping.source, existing.external_id],
        )?;
        // Keep mapping.external_id (and case) in place; do not rewrite the primary key.
        Ok(())
    }

    pub fn list_contact_ids_for_source(&self, source: &str) -> Result<Vec<ContactId>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT contact_id
             FROM contact_sources
             WHERE source = ?1
             ORDER BY contact_id ASC;",
        )?;
        let rows = stmt.query_map(params![source], |row| {
            let contact_id: String = row.get(0)?;
            Ok(contact_id)
        })?;

        let mut ids = Vec::new();
        for row in rows {
            let value = row?;
            let id = ContactId::from_str(&value).map_err(|_| StoreError::InvalidId(value))?;
            ids.push(id);
        }
        Ok(ids)
    }
}
