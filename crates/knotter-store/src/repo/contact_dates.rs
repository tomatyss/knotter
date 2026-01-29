use crate::error::{Result, StoreError};
use crate::temp_table::TempContactIdTable;
use chrono::{Datelike, FixedOffset};
use knotter_core::domain::{
    normalize_contact_date_label, ContactDate, ContactDateId, ContactDateKind, ContactId,
};
use knotter_core::rules::{is_leap_year, local_today};
use rusqlite::{params, Connection, Row};
use std::collections::HashMap;
use std::str::FromStr;

#[derive(Debug, Clone)]
pub struct ContactDateNew {
    pub contact_id: ContactId,
    pub kind: ContactDateKind,
    pub label: Option<String>,
    pub month: u8,
    pub day: u8,
    pub year: Option<i32>,
    pub source: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ContactDateOccurrence {
    pub contact_id: ContactId,
    pub display_name: String,
    pub kind: ContactDateKind,
    pub label: Option<String>,
    pub month: u8,
    pub day: u8,
    pub year: Option<i32>,
}

#[derive(Debug, Clone, Copy)]
pub enum ContactDateUpsertPolicy {
    OverwriteYear,
    PreserveYear,
}

pub struct ContactDatesRepo<'a> {
    conn: &'a Connection,
}

impl<'a> ContactDatesRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn upsert(&self, now_utc: i64, input: ContactDateNew) -> Result<ContactDate> {
        self.upsert_with_policy(now_utc, input, ContactDateUpsertPolicy::OverwriteYear)
    }

    pub fn upsert_preserve_year(&self, now_utc: i64, input: ContactDateNew) -> Result<ContactDate> {
        self.upsert_with_policy(now_utc, input, ContactDateUpsertPolicy::PreserveYear)
    }

    fn upsert_with_policy(
        &self,
        now_utc: i64,
        input: ContactDateNew,
        policy: ContactDateUpsertPolicy,
    ) -> Result<ContactDate> {
        let normalized_label = normalize_contact_date_label(input.label);
        let label_db = normalized_label.clone().unwrap_or_default();
        let contact_date = ContactDate {
            id: ContactDateId::new(),
            contact_id: input.contact_id,
            kind: input.kind,
            label: normalized_label,
            month: input.month,
            day: input.day,
            year: input.year,
            created_at: now_utc,
            updated_at: now_utc,
            source: input.source.clone(),
        };
        contact_date.validate()?;

        let sql = match policy {
            ContactDateUpsertPolicy::OverwriteYear => {
                "INSERT INTO contact_dates
                 (id, contact_id, kind, label, month, day, year, created_at, updated_at, source)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT(contact_id, kind, label, month, day) DO UPDATE SET
                    year = excluded.year,
                    updated_at = excluded.updated_at,
                    source = COALESCE(excluded.source, contact_dates.source);"
            }
            ContactDateUpsertPolicy::PreserveYear => {
                "INSERT INTO contact_dates
                 (id, contact_id, kind, label, month, day, year, created_at, updated_at, source)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT(contact_id, kind, label, month, day) DO UPDATE SET
                    year = COALESCE(excluded.year, contact_dates.year),
                    updated_at = excluded.updated_at,
                    source = COALESCE(excluded.source, contact_dates.source);"
            }
        };

        self.conn.execute(
            sql,
            params![
                contact_date.id.to_string(),
                contact_date.contact_id.to_string(),
                contact_date.kind.as_str(),
                label_db,
                contact_date.month,
                contact_date.day,
                contact_date.year,
                contact_date.created_at,
                contact_date.updated_at,
                contact_date.source,
            ],
        )?;

        self.get_by_key(
            contact_date.contact_id,
            contact_date.kind,
            &label_db,
            contact_date.month,
            contact_date.day,
        )?
        .ok_or_else(|| StoreError::NotFound("contact date not found".to_string()))
    }

    pub fn list_for_contact(&self, contact_id: ContactId) -> Result<Vec<ContactDate>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, contact_id, kind, label, month, day, year, created_at, updated_at, source
             FROM contact_dates
             WHERE contact_id = ?1
             ORDER BY kind ASC, month ASC, day ASC, label ASC;",
        )?;
        let mut rows = stmt.query([contact_id.to_string()])?;
        let mut dates = Vec::new();
        while let Some(row) = rows.next()? {
            dates.push(contact_date_from_row(row)?);
        }
        Ok(dates)
    }

    pub fn list_for_contacts(
        &self,
        contact_ids: &[ContactId],
    ) -> Result<HashMap<ContactId, Vec<ContactDate>>> {
        let mut map: HashMap<ContactId, Vec<ContactDate>> = HashMap::new();
        if contact_ids.is_empty() {
            return Ok(map);
        }

        let temp_table = TempContactIdTable::create(self.conn, contact_ids)?;
        let temp_table_name = temp_table.name();

        let mut stmt = self.conn.prepare(&format!(
            "SELECT d.id,
                    d.contact_id,
                    d.kind,
                    d.label,
                    d.month,
                    d.day,
                    d.year,
                    d.created_at,
                    d.updated_at,
                    d.source
             FROM contact_dates d
             INNER JOIN {temp_table_name} tmp ON tmp.id = d.contact_id
             ORDER BY d.contact_id ASC,
                      d.kind ASC,
                      d.month ASC,
                      d.day ASC,
                      d.label ASC;"
        ))?;

        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let date = contact_date_from_row(row)?;
            map.entry(date.contact_id).or_default().push(date);
        }

        Ok(map)
    }

    pub fn list_today(
        &self,
        now_utc: i64,
        local_offset: FixedOffset,
    ) -> Result<Vec<ContactDateOccurrence>> {
        let today = local_today(now_utc, local_offset)?;
        let month = today.month() as u8;
        let day = today.day() as u8;
        let include_feb_29 = month == 2 && day == 28 && !is_leap_year(today.year());

        let sql = if include_feb_29 {
            "SELECT d.contact_id, c.display_name, d.kind, d.label, d.month, d.day, d.year
             FROM contact_dates d
             JOIN contacts c ON c.id = d.contact_id
             WHERE c.archived_at IS NULL
               AND ((d.month = ?1 AND d.day = ?2) OR (d.month = 2 AND d.day = 29))
             ORDER BY c.display_name COLLATE NOCASE ASC;"
        } else {
            "SELECT d.contact_id, c.display_name, d.kind, d.label, d.month, d.day, d.year
             FROM contact_dates d
             JOIN contacts c ON c.id = d.contact_id
             WHERE c.archived_at IS NULL
               AND d.month = ?1
               AND d.day = ?2
             ORDER BY c.display_name COLLATE NOCASE ASC;"
        };

        let mut stmt = self.conn.prepare(sql)?;
        let mut rows = stmt.query(params![month, day])?;
        let mut items = Vec::new();
        while let Some(row) = rows.next()? {
            items.push(contact_date_occurrence_from_row(row)?);
        }
        Ok(items)
    }

    pub fn delete(&self, id: ContactDateId) -> Result<()> {
        let updated = self
            .conn
            .execute("DELETE FROM contact_dates WHERE id = ?1;", [id.to_string()])?;
        if updated == 0 {
            return Err(StoreError::NotFound(id.to_string()));
        }
        Ok(())
    }

    fn get_by_key(
        &self,
        contact_id: ContactId,
        kind: ContactDateKind,
        label: &str,
        month: u8,
        day: u8,
    ) -> Result<Option<ContactDate>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, contact_id, kind, label, month, day, year, created_at, updated_at, source
             FROM contact_dates
             WHERE contact_id = ?1 AND kind = ?2 AND label = ?3 AND month = ?4 AND day = ?5
             LIMIT 1;",
        )?;
        let mut rows = stmt.query(params![
            contact_id.to_string(),
            kind.as_str(),
            label,
            month,
            day
        ])?;
        if let Some(row) = rows.next()? {
            Ok(Some(contact_date_from_row(row)?))
        } else {
            Ok(None)
        }
    }
}

fn contact_date_from_row(row: &Row<'_>) -> Result<ContactDate> {
    let id: String = row.get(0)?;
    let contact_id: String = row.get(1)?;
    let kind: String = row.get(2)?;
    let label: String = row.get(3)?;
    let month: u8 = row.get(4)?;
    let day: u8 = row.get(5)?;
    let year: Option<i32> = row.get(6)?;
    let created_at: i64 = row.get(7)?;
    let updated_at: i64 = row.get(8)?;
    let source: Option<String> = row.get(9)?;

    let id = ContactDateId::from_str(&id).map_err(|_| StoreError::InvalidId(id))?;
    let contact_id =
        ContactId::from_str(&contact_id).map_err(|_| StoreError::InvalidId(contact_id))?;
    let kind = ContactDateKind::from_str(&kind).map_err(StoreError::Core)?;
    let label = if label.trim().is_empty() {
        None
    } else {
        Some(label)
    };

    Ok(ContactDate {
        id,
        contact_id,
        kind,
        label,
        month,
        day,
        year,
        created_at,
        updated_at,
        source,
    })
}

fn contact_date_occurrence_from_row(row: &Row<'_>) -> Result<ContactDateOccurrence> {
    let contact_id: String = row.get(0)?;
    let display_name: String = row.get(1)?;
    let kind: String = row.get(2)?;
    let label: String = row.get(3)?;
    let month: u8 = row.get(4)?;
    let day: u8 = row.get(5)?;
    let year: Option<i32> = row.get(6)?;

    let contact_id =
        ContactId::from_str(&contact_id).map_err(|_| StoreError::InvalidId(contact_id))?;
    let kind = ContactDateKind::from_str(&kind).map_err(StoreError::Core)?;
    let label = if label.trim().is_empty() {
        None
    } else {
        Some(label)
    };

    Ok(ContactDateOccurrence {
        contact_id,
        display_name,
        kind,
        label,
        month,
        day,
        year,
    })
}
