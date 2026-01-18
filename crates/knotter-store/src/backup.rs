use crate::db;
use crate::error::{Result, StoreError};
use crate::paths;
use rusqlite::backup::Backup;
use rusqlite::Connection;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

const PAGES_PER_STEP: i32 = 200;
const PAUSE_BETWEEN_STEPS: Duration = Duration::from_millis(25);

pub fn backup_to(conn: &Connection, path: &Path) -> Result<()> {
    paths::ensure_parent_dir(path)?;
    let target = canonicalize_path(path)?;
    if let Some(main_path) = main_db_path(conn)? {
        let main_target = canonicalize_path(&main_path)?;
        if main_target == target {
            return Err(StoreError::InvalidBackupPath(path.to_path_buf()));
        }
        if is_sidecar_path(&target, &main_target) {
            return Err(StoreError::InvalidBackupPath(path.to_path_buf()));
        }
        if is_same_file_identity(&target, &main_target)? {
            return Err(StoreError::InvalidBackupPath(path.to_path_buf()));
        }
    }
    let mut dest = Connection::open(&target)?;
    let backup = Backup::new(conn, &mut dest)?;
    backup.run_to_completion(PAGES_PER_STEP, PAUSE_BETWEEN_STEPS, None)?;
    db::restrict_db_permissions(&target)?;
    Ok(())
}

fn canonicalize_path(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        return Ok(fs::canonicalize(path)?);
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let parent = fs::canonicalize(parent)?;
    let file_name = path
        .file_name()
        .ok_or_else(|| StoreError::InvalidBackupPath(path.to_path_buf()))?;
    Ok(parent.join(file_name))
}

fn main_db_path(conn: &Connection) -> Result<Option<PathBuf>> {
    let mut stmt = conn.prepare("PRAGMA database_list;")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        let file: String = row.get(2)?;
        if name == "main" && !file.is_empty() {
            return Ok(Some(PathBuf::from(file)));
        }
    }
    Ok(None)
}

fn is_sidecar_path(target: &Path, main: &Path) -> bool {
    let mut wal = main.as_os_str().to_owned();
    wal.push("-wal");
    let mut shm = main.as_os_str().to_owned();
    shm.push("-shm");
    target == Path::new(&wal) || target == Path::new(&shm)
}

#[cfg(unix)]
fn is_same_file_identity(target: &Path, main: &Path) -> Result<bool> {
    use std::os::unix::fs::MetadataExt;
    if !target.exists() || !main.exists() {
        return Ok(false);
    }
    let target_meta = fs::metadata(target)?;
    let main_meta = fs::metadata(main)?;
    Ok(target_meta.dev() == main_meta.dev() && target_meta.ino() == main_meta.ino())
}

#[cfg(not(unix))]
fn is_same_file_identity(_target: &Path, _main: &Path) -> Result<bool> {
    Ok(false)
}
