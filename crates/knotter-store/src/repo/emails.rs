use crate::error::{Result, StoreError};
use crate::temp_table::TempContactIdTable;
use knotter_core::domain::{normalize_email, ContactId};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;
use std::str::FromStr;

#[derive(Debug, Clone)]
pub struct ContactEmail {
    pub contact_id: ContactId,
    pub email: String,
    pub is_primary: bool,
    pub created_at: i64,
    pub source: Option<String>,
}

pub struct EmailsRepo<'a> {
    conn: &'a Connection,
}

impl<'a> EmailsRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn list_for_contact(&self, contact_id: &ContactId) -> Result<Vec<ContactEmail>> {
        let mut stmt = self.conn.prepare(
            "SELECT contact_id, email, is_primary, created_at, source
             FROM contact_emails
             WHERE contact_id = ?1
             ORDER BY is_primary DESC, email COLLATE NOCASE ASC;",
        )?;
        let mut rows = stmt.query([contact_id.to_string()])?;
        let mut emails = Vec::new();
        while let Some(row) = rows.next()? {
            let id_str: String = row.get(0)?;
            let id =
                ContactId::from_str(&id_str).map_err(|_| StoreError::InvalidId(id_str.clone()))?;
            emails.push(ContactEmail {
                contact_id: id,
                email: row.get(1)?,
                is_primary: row.get::<_, i64>(2)? != 0,
                created_at: row.get(3)?,
                source: row.get(4)?,
            });
        }
        Ok(emails)
    }

    pub fn list_emails_for_contact(&self, contact_id: &ContactId) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT email
             FROM contact_emails
             WHERE contact_id = ?1
             ORDER BY is_primary DESC, email COLLATE NOCASE ASC;",
        )?;
        let mut rows = stmt.query([contact_id.to_string()])?;
        let mut emails = Vec::new();
        while let Some(row) = rows.next()? {
            emails.push(row.get(0)?);
        }
        Ok(emails)
    }

    pub fn list_emails_for_contacts(
        &self,
        contact_ids: &[ContactId],
    ) -> Result<HashMap<ContactId, Vec<String>>> {
        if contact_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let temp_table = TempContactIdTable::create(self.conn, contact_ids)?;
        let mut stmt = self.conn.prepare(&format!(
            "SELECT ce.contact_id, ce.email
             FROM contact_emails ce
             INNER JOIN {} ids ON ids.id = ce.contact_id
             ORDER BY ce.is_primary DESC, ce.email COLLATE NOCASE ASC;",
            temp_table.name()
        ))?;

        let mut rows = stmt.query([])?;
        let mut map: HashMap<ContactId, Vec<String>> = HashMap::new();
        while let Some(row) = rows.next()? {
            let id_str: String = row.get(0)?;
            let id =
                ContactId::from_str(&id_str).map_err(|_| StoreError::InvalidId(id_str.clone()))?;
            let email: String = row.get(1)?;
            map.entry(id).or_default().push(email);
        }
        Ok(map)
    }

    pub fn find_contact_id_by_email(&self, email: &str) -> Result<Option<ContactId>> {
        let Some(email) = normalize_email(email) else {
            return Ok(None);
        };
        let id_str: Option<String> = self
            .conn
            .query_row(
                "SELECT contact_id FROM contact_emails WHERE email = ?1;",
                [email],
                |row| row.get(0),
            )
            .optional()?;
        let Some(id_str) = id_str else {
            return Ok(None);
        };
        let id = ContactId::from_str(&id_str).map_err(|_| StoreError::InvalidId(id_str))?;
        Ok(Some(id))
    }

    pub fn add_email(
        &self,
        now_utc: i64,
        contact_id: &ContactId,
        email: &str,
        source: Option<&str>,
        make_primary_if_missing: bool,
    ) -> Result<bool> {
        let Some(email) = normalize_email(email) else {
            return Ok(false);
        };
        if let Some(existing_id) = self.find_contact_id_by_email(&email)? {
            if existing_id != *contact_id {
                return Err(StoreError::DuplicateEmail(email));
            }
        }

        let inserted = self.conn.execute(
            "INSERT OR IGNORE INTO contact_emails (contact_id, email, is_primary, created_at, source)
             VALUES (?1, ?2, 0, ?3, ?4);",
            params![
                contact_id.to_string(),
                email,
                now_utc,
                source.map(str::to_string)
            ],
        )?;

        if inserted == 0 {
            if let Some(source) = source {
                self.conn.execute(
                    "UPDATE contact_emails
                     SET source = ?3
                     WHERE contact_id = ?1
                       AND email = ?2
                       AND (source IS NULL OR source = 'primary');",
                    params![contact_id.to_string(), email, source],
                )?;
            }
            if make_primary_if_missing {
                let has_primary: i64 = self.conn.query_row(
                    "SELECT COUNT(1) FROM contact_emails WHERE contact_id = ?1 AND is_primary = 1;",
                    [contact_id.to_string()],
                    |row| row.get(0),
                )?;
                if has_primary == 0 {
                    self.set_primary(contact_id, Some(&email))?;
                }
            }
            return Ok(false);
        }

        if make_primary_if_missing {
            let has_primary: i64 = self.conn.query_row(
                "SELECT COUNT(1) FROM contact_emails WHERE contact_id = ?1 AND is_primary = 1;",
                [contact_id.to_string()],
                |row| row.get(0),
            )?;
            if has_primary == 0 {
                self.set_primary(contact_id, Some(&email))?;
            }
        }

        Ok(true)
    }

    pub fn set_primary(&self, contact_id: &ContactId, email: Option<&str>) -> Result<()> {
        let normalized = email.and_then(normalize_email);
        if let Some(primary) = normalized.as_deref() {
            if let Some(existing_id) = self.find_contact_id_by_email(primary)? {
                if existing_id != *contact_id {
                    return Err(StoreError::DuplicateEmail(primary.to_string()));
                }
            }
        }

        self.conn.execute(
            "UPDATE contact_emails SET is_primary = 0 WHERE contact_id = ?1;",
            [contact_id.to_string()],
        )?;

        if let Some(primary) = normalized.as_deref() {
            let updated = self.conn.execute(
                "UPDATE contact_emails
                 SET is_primary = 1
                 WHERE contact_id = ?1 AND email = ?2;",
                params![contact_id.to_string(), primary],
            )?;
            if updated == 0 {
                self.conn.execute(
                    "INSERT INTO contact_emails (contact_id, email, is_primary, created_at)
                     VALUES (?1, ?2, 1, strftime('%s','now'));",
                    params![contact_id.to_string(), primary],
                )?;
            }
            self.conn.execute(
                "UPDATE contacts SET email = ?2 WHERE id = ?1;",
                params![contact_id.to_string(), primary],
            )?;
        } else {
            self.conn.execute(
                "UPDATE contacts SET email = NULL WHERE id = ?1;",
                [contact_id.to_string()],
            )?;
        }
        Ok(())
    }

    pub fn remove_email(&self, contact_id: &ContactId, email: &str) -> Result<bool> {
        let Some(email) = normalize_email(email) else {
            return Ok(false);
        };
        let removed = self.conn.execute(
            "DELETE FROM contact_emails WHERE contact_id = ?1 AND email = ?2;",
            params![contact_id.to_string(), email],
        )?;

        if removed > 0 {
            let primary: Option<String> = self
                .conn
                .query_row(
                    "SELECT email FROM contact_emails
                     WHERE contact_id = ?1 AND is_primary = 1
                     LIMIT 1;",
                    [contact_id.to_string()],
                    |row| row.get(0),
                )
                .optional()?;
            if primary.is_none() {
                let fallback: Option<String> = self
                    .conn
                    .query_row(
                        "SELECT email FROM contact_emails
                         WHERE contact_id = ?1
                         ORDER BY email COLLATE NOCASE ASC
                         LIMIT 1;",
                        [contact_id.to_string()],
                        |row| row.get(0),
                    )
                    .optional()?;
                self.set_primary(contact_id, fallback.as_deref())?;
            }
        }
        Ok(removed > 0)
    }

    pub fn clear_emails(&self, contact_id: &ContactId) -> Result<()> {
        self.conn.execute(
            "DELETE FROM contact_emails WHERE contact_id = ?1;",
            [contact_id.to_string()],
        )?;
        self.conn.execute(
            "UPDATE contacts SET email = NULL WHERE id = ?1;",
            [contact_id.to_string()],
        )?;
        Ok(())
    }

    pub fn replace_emails(
        &self,
        now_utc: i64,
        contact_id: &ContactId,
        emails: Vec<String>,
        primary: Option<String>,
        source: Option<&str>,
    ) -> Result<Vec<String>> {
        let tx = self.conn.unchecked_transaction()?;
        let repo = EmailsRepo::new(&tx);
        let result = repo.replace_emails_in_tx(now_utc, contact_id, emails, primary, source)?;
        tx.commit()?;
        Ok(result)
    }

    pub(crate) fn replace_emails_in_tx(
        &self,
        now_utc: i64,
        contact_id: &ContactId,
        emails: Vec<String>,
        primary: Option<String>,
        source: Option<&str>,
    ) -> Result<Vec<String>> {
        let mut normalized: Vec<String> = Vec::new();
        for email in emails {
            if let Some(email) = normalize_email(&email) {
                if !normalized.contains(&email) {
                    normalized.push(email);
                }
            }
        }

        let normalized_primary = primary.as_deref().and_then(normalize_email);
        if let Some(primary) = normalized_primary.as_deref() {
            if !normalized.iter().any(|value| value == primary) {
                normalized.push(primary.to_string());
            }
        }
        let primary = normalized_primary.or_else(|| normalized.first().cloned());

        for email in &normalized {
            if let Some(existing_id) = self.find_contact_id_by_email(email)? {
                if existing_id != *contact_id {
                    return Err(StoreError::DuplicateEmail(email.to_string()));
                }
            }
        }

        let mut stmt = self.conn.prepare(
            "SELECT email, created_at, source FROM contact_emails WHERE contact_id = ?1;",
        )?;
        let mut rows = stmt.query([contact_id.to_string()])?;
        let mut existing: HashMap<String, (i64, Option<String>)> = HashMap::new();
        while let Some(row) = rows.next()? {
            let email: String = row.get(0)?;
            let created_at: i64 = row.get(1)?;
            let source: Option<String> = row.get(2)?;
            existing.insert(email, (created_at, source));
        }

        let mut normalized_set: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        for email in &normalized {
            normalized_set.insert(email.clone());
        }

        for email in existing.keys() {
            if !normalized_set.contains(email) {
                self.conn.execute(
                    "DELETE FROM contact_emails WHERE contact_id = ?1 AND email = ?2;",
                    params![contact_id.to_string(), email],
                )?;
            }
        }

        for email in &normalized {
            let is_primary = primary.as_deref() == Some(email.as_str());
            if let Some((_created_at, existing_source)) = existing.get(email) {
                self.conn.execute(
                    "UPDATE contact_emails
                     SET is_primary = ?3
                     WHERE contact_id = ?1 AND email = ?2;",
                    params![
                        contact_id.to_string(),
                        email,
                        if is_primary { 1 } else { 0 }
                    ],
                )?;
                if let Some(source) = source {
                    if existing_source.as_deref().is_none()
                        || existing_source.as_deref() == Some("primary")
                    {
                        self.conn.execute(
                            "UPDATE contact_emails
                             SET source = ?3
                             WHERE contact_id = ?1 AND email = ?2;",
                            params![contact_id.to_string(), email, source],
                        )?;
                    }
                }
            } else {
                self.conn.execute(
                    "INSERT INTO contact_emails (contact_id, email, is_primary, created_at, source)
                     VALUES (?1, ?2, ?3, ?4, ?5);",
                    params![
                        contact_id.to_string(),
                        email,
                        if is_primary { 1 } else { 0 },
                        now_utc,
                        source.map(str::to_string)
                    ],
                )?;
            }
        }

        self.conn.execute(
            "UPDATE contacts SET email = ?2 WHERE id = ?1;",
            params![contact_id.to_string(), primary],
        )?;

        Ok(normalized)
    }
}
