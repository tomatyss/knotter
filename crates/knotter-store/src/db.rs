use crate::error::Result;
use rusqlite::Connection;
use std::fs;
use std::path::Path;

pub fn open(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    restrict_db_permissions(path)?;
    apply_pragmas(&conn)?;
    Ok(conn)
}

pub fn open_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    apply_pragmas(&conn)?;
    Ok(conn)
}

fn apply_pragmas(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "busy_timeout", 2000)?;
    Ok(())
}

#[cfg(unix)]
fn restrict_db_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if path.exists() {
        let perms = fs::Permissions::from_mode(0o600);
        fs::set_permissions(path, perms)?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn restrict_db_permissions(_path: &Path) -> Result<()> {
    Ok(())
}
