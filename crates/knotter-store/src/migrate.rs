use crate::error::{Result, StoreError};
use rusqlite::{Connection, OptionalExtension, Transaction};

const MIGRATIONS: &[(&str, &str)] = &[(
    "001_init.sql",
    include_str!("../migrations/001_init.sql"),
)];

pub fn run_migrations(conn: &Connection) -> Result<()> {
    let tx = conn.transaction()?;
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

fn ensure_schema_table(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS knotter_schema (version INTEGER NOT NULL);",
    )?;

    let existing: Option<i64> = tx
        .query_row(
            "SELECT version FROM knotter_schema LIMIT 1;",
            [],
            |row| row.get(0),
        )
        .optional()?;

    if existing.is_none() {
        tx.execute("INSERT INTO knotter_schema (version) VALUES (0);", [])?;
    }

    Ok(())
}

fn current_version(tx: &Transaction<'_>) -> Result<i64> {
    let version: i64 = tx.query_row(
        "SELECT version FROM knotter_schema LIMIT 1;",
        [],
        |row| row.get(0),
    )?;
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
