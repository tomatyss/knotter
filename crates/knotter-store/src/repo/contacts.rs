use crate::error::{Result, StoreError};
use crate::query::{due_bounds, ContactQuery};
use chrono::FixedOffset;
use knotter_core::domain::{Contact, ContactId};
use knotter_core::rules::validate_soon_days;
use rusqlite::{params, params_from_iter, Connection};
use std::str::FromStr;

#[derive(Debug, Clone)]
pub struct ContactNew {
    pub display_name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub handle: Option<String>,
    pub timezone: Option<String>,
    pub next_touchpoint_at: Option<i64>,
    pub cadence_days: Option<i32>,
    pub archived_at: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct ContactUpdate {
    pub display_name: Option<String>,
    pub email: Option<Option<String>>,
    pub phone: Option<Option<String>>,
    pub handle: Option<Option<String>>,
    pub timezone: Option<Option<String>>,
    pub next_touchpoint_at: Option<Option<i64>>,
    pub cadence_days: Option<Option<i32>>,
    pub archived_at: Option<Option<i64>>,
}

pub struct ContactsRepo<'a> {
    conn: &'a Connection,
}

impl<'a> ContactsRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn create(&self, now_utc: i64, input: ContactNew) -> Result<Contact> {
        let contact = Contact {
            id: ContactId::new(),
            display_name: input.display_name,
            email: input.email,
            phone: input.phone,
            handle: input.handle,
            timezone: input.timezone,
            next_touchpoint_at: input.next_touchpoint_at,
            cadence_days: input.cadence_days,
            created_at: now_utc,
            updated_at: now_utc,
            archived_at: input.archived_at,
        };

        contact.validate()?;

        self.conn.execute(
            "INSERT INTO contacts (id, display_name, email, phone, handle, timezone, next_touchpoint_at, cadence_days, created_at, updated_at, archived_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11);",
            params![
                contact.id.to_string(),
                contact.display_name,
                contact.email,
                contact.phone,
                contact.handle,
                contact.timezone,
                contact.next_touchpoint_at,
                contact.cadence_days,
                contact.created_at,
                contact.updated_at,
                contact.archived_at,
            ],
        )?;

        Ok(contact)
    }

    pub fn get(&self, id: ContactId) -> Result<Option<Contact>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, display_name, email, phone, handle, timezone, next_touchpoint_at, cadence_days, created_at, updated_at, archived_at
             FROM contacts WHERE id = ?1;",
        )?;
        let mut rows = stmt.query([id.to_string()])?;
        if let Some(row) = rows.next()? {
            Ok(Some(contact_from_row(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn list_by_email(&self, email: &str) -> Result<Vec<Contact>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, display_name, email, phone, handle, timezone, next_touchpoint_at, cadence_days, created_at, updated_at, archived_at
             FROM contacts
             WHERE email IS NOT NULL
               AND email = ?1 COLLATE NOCASE
             ORDER BY (archived_at IS NOT NULL) ASC, updated_at DESC;",
        )?;
        let mut rows = stmt.query([email])?;
        let mut contacts = Vec::new();
        while let Some(row) = rows.next()? {
            contacts.push(contact_from_row(row)?);
        }
        Ok(contacts)
    }

    pub fn update(&self, now_utc: i64, id: ContactId, update: ContactUpdate) -> Result<Contact> {
        let mut contact = self
            .get(id)?
            .ok_or_else(|| StoreError::NotFound(id.to_string()))?;

        if let Some(value) = update.display_name {
            contact.display_name = value;
        }
        if let Some(value) = update.email {
            contact.email = value;
        }
        if let Some(value) = update.phone {
            contact.phone = value;
        }
        if let Some(value) = update.handle {
            contact.handle = value;
        }
        if let Some(value) = update.timezone {
            contact.timezone = value;
        }
        if let Some(value) = update.next_touchpoint_at {
            contact.next_touchpoint_at = value;
        }
        if let Some(value) = update.cadence_days {
            contact.cadence_days = value;
        }
        if let Some(value) = update.archived_at {
            contact.archived_at = value;
        }

        contact.updated_at = now_utc;
        contact.validate()?;

        self.conn.execute(
            "UPDATE contacts SET display_name = ?2, email = ?3, phone = ?4, handle = ?5, timezone = ?6, next_touchpoint_at = ?7, cadence_days = ?8, updated_at = ?9, archived_at = ?10
             WHERE id = ?1;",
            params![
                contact.id.to_string(),
                contact.display_name,
                contact.email,
                contact.phone,
                contact.handle,
                contact.timezone,
                contact.next_touchpoint_at,
                contact.cadence_days,
                contact.updated_at,
                contact.archived_at,
            ],
        )?;

        Ok(contact)
    }

    pub fn delete(&self, id: ContactId) -> Result<()> {
        self.conn
            .execute("DELETE FROM contacts WHERE id = ?1;", [id.to_string()])?;
        Ok(())
    }

    pub fn list_all(&self) -> Result<Vec<Contact>> {
        let query = ContactQuery::default();
        self.list_contacts(&query, 0, 7, FixedOffset::east_opt(0).expect("utc offset"))
    }

    pub fn list_contacts(
        &self,
        query: &ContactQuery,
        now_utc: i64,
        soon_days: i64,
        local_offset: FixedOffset,
    ) -> Result<Vec<Contact>> {
        let compiled = query.to_sql(now_utc, soon_days, local_offset)?;
        let mut stmt = self.conn.prepare(&compiled.sql)?;
        let mut rows = stmt.query(params_from_iter(compiled.params))?;
        let mut contacts = Vec::new();
        while let Some(row) = rows.next()? {
            contacts.push(contact_from_row(row)?);
        }
        Ok(contacts)
    }

    pub fn list_due_contacts(
        &self,
        now_utc: i64,
        soon_days: i64,
        local_offset: FixedOffset,
    ) -> Result<Vec<Contact>> {
        let soon_days = validate_soon_days(soon_days).map_err(StoreError::Core)?;
        let bounds = due_bounds(now_utc, soon_days, local_offset);
        let mut stmt = self.conn.prepare(
            "SELECT id, display_name, email, phone, handle, timezone, next_touchpoint_at, cadence_days, created_at, updated_at, archived_at
             FROM contacts
             WHERE archived_at IS NULL
               AND next_touchpoint_at IS NOT NULL
               AND next_touchpoint_at < ?1
             ORDER BY CASE
                WHEN next_touchpoint_at < ?2 THEN 0
                WHEN next_touchpoint_at >= ?3 AND next_touchpoint_at < ?4 THEN 1
                WHEN next_touchpoint_at >= ?4 AND next_touchpoint_at < ?5 THEN 2
                ELSE 3
             END,
             display_name COLLATE NOCASE ASC;",
        )?;
        let mut rows = stmt.query(params![
            bounds.soon_end,
            now_utc,
            bounds.start_of_today,
            bounds.start_of_tomorrow,
            bounds.soon_end
        ])?;
        let mut contacts = Vec::new();
        while let Some(row) = rows.next()? {
            contacts.push(contact_from_row(row)?);
        }
        Ok(contacts)
    }
}

fn contact_from_row(row: &rusqlite::Row<'_>) -> Result<Contact> {
    let id_str: String = row.get(0)?;
    let id = ContactId::from_str(&id_str).map_err(|_| StoreError::InvalidId(id_str.clone()))?;
    Ok(Contact {
        id,
        display_name: row.get(1)?,
        email: row.get(2)?,
        phone: row.get(3)?,
        handle: row.get(4)?,
        timezone: row.get(5)?,
        next_touchpoint_at: row.get(6)?,
        cadence_days: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        archived_at: row.get(10)?,
    })
}
