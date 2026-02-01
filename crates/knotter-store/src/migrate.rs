use crate::error::{Result, StoreError};
use rusqlite::{Connection, OptionalExtension, Transaction};

const MIGRATIONS: &[(&str, &str)] = &[
    ("001_init.sql", include_str!("../migrations/001_init.sql")),
    (
        "002_email_sync.sql",
        include_str!("../migrations/002_email_sync.sql"),
    ),
    (
        "003_email_sync_uidvalidity.sql",
        include_str!("../migrations/003_email_sync_uidvalidity.sql"),
    ),
    (
        "004_email_message_dedupe_indexes.sql",
        include_str!("../migrations/004_email_message_dedupe_indexes.sql"),
    ),
    (
        "005_email_message_id_normalize.sql",
        include_str!("../migrations/005_email_message_id_normalize.sql"),
    ),
    (
        "006_contact_merge_candidates.sql",
        include_str!("../migrations/006_contact_merge_candidates.sql"),
    ),
    (
        "007_contact_dates.sql",
        include_str!("../migrations/007_contact_dates.sql"),
    ),
    (
        "008_contact_dates_custom_label.sql",
        include_str!("../migrations/008_contact_dates_custom_label.sql"),
    ),
    (
        "009_telegram_sync.sql",
        include_str!("../migrations/009_telegram_sync.sql"),
    ),
];

pub fn run_migrations(conn: &Connection) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    ensure_schema_table(&tx)?;
    let current = current_version(&tx)?;

    if current > MIGRATIONS.len() as i64 {
        return Err(StoreError::Migration(format!(
            "db version {} newer than available migrations {}",
            current,
            MIGRATIONS.len()
        )));
    }

    for (index, (_name, sql)) in MIGRATIONS.iter().enumerate() {
        let version = (index + 1) as i64;
        if current >= version {
            continue;
        }
        tx.execute_batch(sql)?;
        set_version(&tx, version)?;
    }

    tx.commit()?;
    Ok(())
}

pub fn schema_version(conn: &Connection) -> Result<i64> {
    let version: i64 =
        conn.query_row("SELECT version FROM knotter_schema LIMIT 1;", [], |row| {
            row.get(0)
        })?;
    Ok(version)
}

fn ensure_schema_table(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS knotter_schema (version INTEGER NOT NULL);")?;

    let existing: Option<i64> = tx
        .query_row("SELECT version FROM knotter_schema LIMIT 1;", [], |row| {
            row.get(0)
        })
        .optional()?;

    if existing.is_none() {
        tx.execute("INSERT INTO knotter_schema (version) VALUES (0);", [])?;
    }

    Ok(())
}

fn current_version(tx: &Transaction<'_>) -> Result<i64> {
    let version: i64 = tx.query_row("SELECT version FROM knotter_schema LIMIT 1;", [], |row| {
        row.get(0)
    })?;
    Ok(version)
}

fn set_version(tx: &Transaction<'_>, version: i64) -> Result<()> {
    let updated = tx.execute("UPDATE knotter_schema SET version = ?1;", [version])?;
    if updated != 1 {
        return Err(StoreError::Migration(format!(
            "expected single schema row, updated {}",
            updated
        )));
    }
    Ok(())
}
