use crate::error::Result;
use knotter_core::domain::ContactId;
use rusqlite::Connection;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEMP_TABLE_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) struct TempContactIdTable<'a> {
    conn: &'a Connection,
    name: String,
}

impl<'a> TempContactIdTable<'a> {
    pub(crate) fn create(conn: &'a Connection, contact_ids: &[ContactId]) -> Result<Self> {
        let table_name = generate_temp_table_name();
        debug_assert!(table_name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_'));
        let full_name = format!("temp.{}", table_name);

        conn.execute_batch(&format!(
            "DROP TABLE IF EXISTS {full_name};
             CREATE TEMP TABLE {full_name} (id TEXT PRIMARY KEY);"
        ))?;

        let guard = Self {
            conn,
            name: full_name,
        };

        {
            let mut stmt = guard.conn.prepare(&format!(
                "INSERT OR IGNORE INTO {} (id) VALUES (?1);",
                guard.name
            ))?;
            for id in contact_ids {
                stmt.execute([id.to_string()])?;
            }
        }

        Ok(guard)
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }
}

impl Drop for TempContactIdTable<'_> {
    fn drop(&mut self) {
        let _ = self
            .conn
            .execute(&format!("DROP TABLE IF EXISTS {};", self.name), []);
    }
}

fn generate_temp_table_name() -> String {
    let micros = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros();
    let counter = TEMP_TABLE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("temp_contact_ids_{}_{}", micros, counter)
}

#[cfg(test)]
mod tests {
    use super::generate_temp_table_name;

    #[test]
    fn temp_table_names_are_unique_and_safe() {
        let first = generate_temp_table_name();
        let second = generate_temp_table_name();
        assert_ne!(first, second);
        assert!(first
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_'));
    }
}
