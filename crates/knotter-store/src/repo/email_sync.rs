use crate::error::Result;
use knotter_core::domain::ContactId;
use rusqlite::{params, Connection, OptionalExtension};

#[derive(Debug, Clone)]
pub struct EmailSyncState {
    pub account: String,
    pub mailbox: String,
    pub uidvalidity: Option<i64>,
    pub last_uid: i64,
    pub last_seen_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct EmailMessageRecord {
    pub account: String,
    pub mailbox: String,
    pub uidvalidity: i64,
    pub uid: i64,
    pub message_id: Option<String>,
    pub contact_id: ContactId,
    pub occurred_at: i64,
    pub direction: String,
    pub subject: Option<String>,
    pub created_at: i64,
}

type EmailSyncStateRow = (String, String, Option<i64>, i64, Option<i64>);

pub struct EmailSyncRepo<'a> {
    conn: &'a Connection,
}

impl<'a> EmailSyncRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn load_state(&self, account: &str, mailbox: &str) -> Result<Option<EmailSyncState>> {
        let row: Option<EmailSyncStateRow> = self
            .conn
            .query_row(
                "SELECT account, mailbox, uidvalidity, last_uid, last_seen_at
                 FROM email_sync_state
                 WHERE account = ?1 AND mailbox = ?2;",
                params![account, mailbox],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .optional()?;

        Ok(row.map(
            |(account, mailbox, uidvalidity, last_uid, last_seen_at)| EmailSyncState {
                account,
                mailbox,
                uidvalidity,
                last_uid,
                last_seen_at,
            },
        ))
    }

    pub fn upsert_state(&self, state: &EmailSyncState) -> Result<()> {
        self.conn.execute(
            "INSERT INTO email_sync_state (account, mailbox, uidvalidity, last_uid, last_seen_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(account, mailbox) DO UPDATE SET
               uidvalidity = excluded.uidvalidity,
               last_uid = excluded.last_uid,
               last_seen_at = excluded.last_seen_at;",
            params![
                state.account,
                state.mailbox,
                state.uidvalidity,
                state.last_uid,
                state.last_seen_at
            ],
        )?;
        Ok(())
    }

    pub fn record_message(&self, record: &EmailMessageRecord) -> Result<bool> {
        let inserted = self.conn.execute(
            "INSERT OR IGNORE INTO email_messages
             (account, mailbox, uidvalidity, uid, message_id, contact_id, occurred_at, direction, subject, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10);",
            params![
                record.account,
                record.mailbox,
                record.uidvalidity,
                record.uid,
                record.message_id,
                record.contact_id.to_string(),
                record.occurred_at,
                record.direction,
                record.subject,
                record.created_at
            ],
        )?;
        Ok(inserted > 0)
    }

    pub fn has_null_message_id(&self, account: &str, mailbox: &str) -> Result<bool> {
        let exists: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM email_messages
                 WHERE account = ?1 AND mailbox = ?2 AND message_id IS NULL
                 LIMIT 1;",
                params![account, mailbox],
                |row| row.get(0),
            )
            .optional()?;
        Ok(exists.is_some())
    }

    pub fn clear_mailbox_messages(&self, account: &str, mailbox: &str) -> Result<usize> {
        let removed = self.conn.execute(
            "DELETE FROM email_messages WHERE account = ?1 AND mailbox = ?2;",
            params![account, mailbox],
        )?;
        Ok(removed)
    }

    pub fn latest_email_touch_for_contact(&self, contact_id: &ContactId) -> Result<Option<i64>> {
        let ts: Option<i64> = self
            .conn
            .query_row(
                "SELECT occurred_at FROM email_messages
                 WHERE contact_id = ?1
                 ORDER BY occurred_at DESC
                 LIMIT 1;",
                [contact_id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        Ok(ts)
    }
}
