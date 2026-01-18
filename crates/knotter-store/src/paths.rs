use crate::error::{Result, StoreError};
use chrono::Local;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const APP_DIR: &str = "knotter";
const DB_FILENAME: &str = "knotter.sqlite3";

pub fn data_dir() -> Result<PathBuf> {
    if let Some(dir) = env::var_os("XDG_DATA_HOME") {
        let path = PathBuf::from(dir);
        if path.as_os_str().is_empty() {
            return Err(StoreError::InvalidDataPath(path));
        }
        return Ok(path.join(APP_DIR));
    }

    let home = dirs::home_dir().ok_or(StoreError::MissingHomeDir)?;
    Ok(home.join(".local").join("share").join(APP_DIR))
}

pub fn ensure_data_dir() -> Result<PathBuf> {
    let dir = data_dir()?;
    if !dir.exists() {
        fs::create_dir_all(&dir)?;
    }
    restrict_dir_permissions(&dir)?;
    Ok(dir)
}

pub fn db_path() -> Result<PathBuf> {
    Ok(ensure_data_dir()?.join(DB_FILENAME))
}

pub fn resolve_db_path(custom: Option<PathBuf>) -> Result<PathBuf> {
    match custom {
        Some(path) => {
            ensure_parent_dir(&path)?;
            Ok(path)
        }
        None => db_path(),
    }
}

pub fn db_path_in(dir: &Path) -> PathBuf {
    dir.join(DB_FILENAME)
}

pub fn backup_path() -> Result<PathBuf> {
    let dir = ensure_data_dir()?;
    let stamp = Local::now().format("%Y%m%d-%H%M%S").to_string();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut candidate = dir.join(format!("knotter-backup-{stamp}-{nanos}.sqlite3"));
    if candidate.exists() {
        let mut suffix = 1;
        loop {
            let next = dir.join(format!("knotter-backup-{stamp}-{nanos}-{suffix}.sqlite3"));
            if !next.exists() {
                candidate = next;
                break;
            }
            suffix += 1;
        }
    }
    Ok(candidate)
}

pub(crate) fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            let created = !parent.exists();
            fs::create_dir_all(parent).map_err(|err| {
                StoreError::Io(with_path_context(err, parent, "create db directory"))
            })?;
            if created {
                restrict_dir_permissions(parent)?;
            }
        }
    }
    Ok(())
}

#[cfg(unix)]
fn restrict_dir_permissions(dir: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let perms = fs::Permissions::from_mode(0o700);
    fs::set_permissions(dir, perms)
        .map_err(|err| StoreError::Io(with_path_context(err, dir, "restrict permissions for")))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict_dir_permissions(_dir: &Path) -> Result<()> {
    Ok(())
}

fn with_path_context(err: std::io::Error, path: &Path, action: &str) -> std::io::Error {
    std::io::Error::new(
        err.kind(),
        format!("{} {}: {}", action, path.display(), err),
    )
}
