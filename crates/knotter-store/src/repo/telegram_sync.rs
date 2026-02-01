use crate::error::Result;
use knotter_core::domain::ContactId;
use rusqlite::{params, Connection, OptionalExtension};

#[derive(Debug, Clone)]
pub struct TelegramSyncState {
    pub account: String,
    pub peer_id: i64,
    pub last_message_id: i64,
    pub last_seen_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct TelegramMessageRecord {
    pub account: String,
    pub peer_id: i64,
    pub message_id: i64,
    pub contact_id: ContactId,
    pub occurred_at: i64,
    pub direction: String,
    pub snippet: Option<String>,
    pub created_at: i64,
}

type TelegramSyncStateRow = (String, i64, i64, Option<i64>);

pub struct TelegramSyncRepo<'a> {
    conn: &'a Connection,
}

impl<'a> TelegramSyncRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn load_state(&self, account: &str, peer_id: i64) -> Result<Option<TelegramSyncState>> {
        let row: Option<TelegramSyncStateRow> = self
            .conn
            .query_row(
                "SELECT account, peer_id, last_message_id, last_seen_at
                 FROM telegram_sync_state
                 WHERE account = ?1 AND peer_id = ?2;",
                params![account, peer_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        Ok(row.map(
            |(account, peer_id, last_message_id, last_seen_at)| TelegramSyncState {
                account,
                peer_id,
                last_message_id,
                last_seen_at,
            },
        ))
    }

    pub fn upsert_state(&self, state: &TelegramSyncState) -> Result<()> {
        self.conn.execute(
            "INSERT INTO telegram_sync_state (account, peer_id, last_message_id, last_seen_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(account, peer_id) DO UPDATE SET
               last_message_id = excluded.last_message_id,
               last_seen_at = excluded.last_seen_at;",
            params![
                state.account,
                state.peer_id,
                state.last_message_id,
                state.last_seen_at
            ],
        )?;
        Ok(())
    }

    pub fn record_message(&self, record: &TelegramMessageRecord) -> Result<bool> {
        let inserted = self.conn.execute(
            "INSERT OR IGNORE INTO telegram_messages
             (account, peer_id, message_id, contact_id, occurred_at, direction, snippet, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8);",
            params![
                record.account,
                record.peer_id,
                record.message_id,
                record.contact_id.to_string(),
                record.occurred_at,
                record.direction,
                record.snippet,
                record.created_at
            ],
        )?;
        Ok(inserted > 0)
    }
}
