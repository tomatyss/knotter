use crate::error::{Result, StoreError};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

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

pub fn db_path_in(dir: &Path) -> PathBuf {
    dir.join(DB_FILENAME)
}

#[cfg(unix)]
fn restrict_dir_permissions(dir: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let perms = fs::Permissions::from_mode(0o700);
    fs::set_permissions(dir, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict_dir_permissions(_dir: &Path) -> Result<()> {
    Ok(())
}
