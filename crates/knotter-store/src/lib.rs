pub mod backup;
pub mod db;
pub mod error;
pub mod migrate;
pub mod paths;
pub mod query;
pub mod repo;
pub(crate) mod temp_table;

use crate::error::Result;
use rusqlite::Connection;
use std::path::Path;

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = db::open(path)?;
        Ok(Self { conn })
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = db::open_in_memory()?;
        Ok(Self { conn })
    }

    pub fn migrate(&self) -> Result<()> {
        migrate::run_migrations(&self.conn)
    }

    pub fn schema_version(&self) -> Result<i64> {
        migrate::schema_version(&self.conn)
    }

    pub fn backup_to(&self, path: &Path) -> Result<()> {
        backup::backup_to(&self.conn, path)
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub fn contacts(&self) -> repo::ContactsRepo<'_> {
        repo::ContactsRepo::new(&self.conn)
    }

    pub fn emails(&self) -> repo::EmailsRepo<'_> {
        repo::EmailsRepo::new(&self.conn)
    }

    pub fn email_sync(&self) -> repo::EmailSyncRepo<'_> {
        repo::EmailSyncRepo::new(&self.conn)
    }

    pub fn tags(&self) -> repo::TagsRepo<'_> {
        repo::TagsRepo::new(&self.conn)
    }

    pub fn interactions(&self) -> repo::InteractionsRepo<'_> {
        repo::InteractionsRepo::new(&self.conn)
    }

    pub fn contact_dates(&self) -> repo::ContactDatesRepo<'_> {
        repo::ContactDatesRepo::new(&self.conn)
    }

    pub fn merge_candidates(&self) -> repo::MergeCandidatesRepo<'_> {
        repo::MergeCandidatesRepo::new(&self.conn)
    }
}
