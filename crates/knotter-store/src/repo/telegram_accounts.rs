use crate::error::{Result, StoreError};
use knotter_core::domain::ContactId;
use rusqlite::{params, Connection, OptionalExtension};
use std::str::FromStr;

#[derive(Debug, Clone)]
pub struct TelegramAccount {
    pub contact_id: ContactId,
    pub telegram_user_id: i64,
    pub username: Option<String>,
    pub phone: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub source: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct TelegramAccountNew {
    pub contact_id: ContactId,
    pub telegram_user_id: i64,
    pub username: Option<String>,
    pub phone: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub source: Option<String>,
}

pub struct TelegramAccountsRepo<'a> {
    conn: &'a Connection,
}

impl<'a> TelegramAccountsRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn list_for_contact(&self, contact_id: ContactId) -> Result<Vec<TelegramAccount>> {
        let mut stmt = self.conn.prepare(
            "SELECT contact_id, telegram_user_id, username, phone, first_name, last_name, source, created_at
             FROM contact_telegram_accounts
             WHERE contact_id = ?1
             ORDER BY created_at DESC;",
        )?;
        let mut rows = stmt.query([contact_id.to_string()])?;
        let mut accounts = Vec::new();
        while let Some(row) = rows.next()? {
            accounts.push(account_from_row(row)?);
        }
        Ok(accounts)
    }

    pub fn find_contact_id_by_user_id(&self, telegram_user_id: i64) -> Result<Option<ContactId>> {
        let value: Option<String> = self
            .conn
            .query_row(
                "SELECT contact_id FROM contact_telegram_accounts WHERE telegram_user_id = ?1;",
                [telegram_user_id],
                |row| row.get(0),
            )
            .optional()?;
        value
            .map(|value| {
                ContactId::from_str(&value).map_err(|_| StoreError::InvalidId(value.clone()))
            })
            .transpose()
    }

    pub fn list_contact_ids_by_username(&self, username: &str) -> Result<Vec<ContactId>> {
        let normalized = normalize_username(Some(username.to_string()));
        let Some(normalized) = normalized else {
            return Ok(Vec::new());
        };
        let with_at = format!("@{normalized}");
        let mut stmt = self.conn.prepare(
            "SELECT contact_id FROM contact_telegram_accounts
             WHERE username = ?1 COLLATE NOCASE OR username = ?2 COLLATE NOCASE;",
        )?;
        let mut rows = stmt.query([normalized.as_str(), with_at.as_str()])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            let value: String = row.get(0)?;
            let id =
                ContactId::from_str(&value).map_err(|_| StoreError::InvalidId(value.clone()))?;
            out.push(id);
        }
        Ok(out)
    }

    pub fn upsert(&self, now_utc: i64, account: TelegramAccountNew) -> Result<()> {
        let username = normalize_username(account.username);
        if let Some(existing) = self.find_contact_id_by_user_id(account.telegram_user_id)? {
            if existing != account.contact_id {
                return Err(StoreError::DuplicateTelegramUser(account.telegram_user_id));
            }
            self.conn.execute(
                "UPDATE contact_telegram_accounts
                 SET username = ?1,
                     phone = ?2,
                     first_name = ?3,
                     last_name = ?4,
                     source = ?5
                 WHERE contact_id = ?6 AND telegram_user_id = ?7;",
                params![
                    username,
                    account.phone,
                    account.first_name,
                    account.last_name,
                    account.source,
                    account.contact_id.to_string(),
                    account.telegram_user_id,
                ],
            )?;
            return Ok(());
        }

        self.conn.execute(
            "INSERT INTO contact_telegram_accounts
             (contact_id, telegram_user_id, username, phone, first_name, last_name, source, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8);",
            params![
                account.contact_id.to_string(),
                account.telegram_user_id,
                username,
                account.phone,
                account.first_name,
                account.last_name,
                account.source,
                now_utc,
            ],
        )?;
        Ok(())
    }
}

fn normalize_username(value: Option<String>) -> Option<String> {
    let value = value?;
    let trimmed = value.trim().trim_start_matches('@');
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_ascii_lowercase())
    }
}

fn account_from_row(row: &rusqlite::Row<'_>) -> Result<TelegramAccount> {
    let contact_id_str: String = row.get(0)?;
    let contact_id =
        ContactId::from_str(&contact_id_str).map_err(|_| StoreError::InvalidId(contact_id_str))?;
    Ok(TelegramAccount {
        contact_id,
        telegram_user_id: row.get(1)?,
        username: row.get(2)?,
        phone: row.get(3)?,
        first_name: row.get(4)?,
        last_name: row.get(5)?,
        source: row.get(6)?,
        created_at: row.get(7)?,
    })
}
