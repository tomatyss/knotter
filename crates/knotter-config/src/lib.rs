use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use knotter_core::rules::cadence::MAX_CADENCE_DAYS;
use knotter_core::rules::validate_soon_days;
use serde::Deserialize;
use thiserror::Error;

const APP_DIR: &str = "knotter";
const CONFIG_FILENAME: &str = "config.toml";

pub const DEFAULT_SOON_DAYS: i64 = 7;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub due_soon_days: i64,
    pub default_cadence_days: Option<i32>,
    pub notifications: NotificationsConfig,
}

#[derive(Debug, Clone)]
pub struct NotificationsConfig {
    pub enabled: bool,
    pub backend: NotificationBackend,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotificationBackend {
    Stdout,
    Desktop,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            due_soon_days: DEFAULT_SOON_DAYS,
            default_cadence_days: None,
            notifications: NotificationsConfig {
                enabled: false,
                backend: NotificationBackend::Desktop,
            },
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing home directory")]
    MissingHomeDir,
    #[error("invalid config path: {0}")]
    InvalidConfigPath(PathBuf),
    #[error("config file not found: {0}")]
    MissingConfigFile(PathBuf),
    #[error("config file permissions too permissive: {0}")]
    InsecurePermissions(PathBuf),
    #[error("invalid due_soon_days value: {0}")]
    InvalidSoonDays(i64),
    #[error("invalid default_cadence_days value: {0}")]
    InvalidCadenceDays(i32),
    #[error("failed to read config file {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse config file {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
}

pub type Result<T> = std::result::Result<T, ConfigError>;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    due_soon_days: Option<i64>,
    default_cadence_days: Option<i32>,
    notifications: Option<NotificationsFile>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NotificationsFile {
    enabled: Option<bool>,
    backend: Option<NotificationBackend>,
}

pub fn load(config_path: Option<PathBuf>) -> Result<AppConfig> {
    let required = config_path.is_some();
    let path = match resolve_config_path(config_path.clone()) {
        Ok(path) => path,
        Err(ConfigError::MissingHomeDir) if !required => return Ok(AppConfig::default()),
        Err(ConfigError::InvalidConfigPath(_)) if !required => return Ok(AppConfig::default()),
        Err(err) => return Err(err),
    };
    match load_at_path(&path, required)? {
        Some(config) => Ok(config),
        None => Ok(AppConfig::default()),
    }
}

pub fn resolve_config_path(custom: Option<PathBuf>) -> Result<PathBuf> {
    match custom {
        Some(path) => {
            if path.as_os_str().is_empty() {
                return Err(ConfigError::InvalidConfigPath(path));
            }
            Ok(path)
        }
        None => {
            let base = if let Some(dir) = env::var_os("XDG_CONFIG_HOME") {
                let path = PathBuf::from(dir);
                if path.as_os_str().is_empty() {
                    return Err(ConfigError::InvalidConfigPath(path));
                }
                path
            } else {
                let home = dirs::home_dir().ok_or(ConfigError::MissingHomeDir)?;
                home.join(".config")
            };
            Ok(base.join(APP_DIR).join(CONFIG_FILENAME))
        }
    }
}

fn load_at_path(path: &Path, required: bool) -> Result<Option<AppConfig>> {
    if !path.exists() {
        if required {
            return Err(ConfigError::MissingConfigFile(path.to_path_buf()));
        }
        return Ok(None);
    }

    ensure_permissions(path)?;
    let contents = fs::read_to_string(path).map_err(|source| ConfigError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let parsed: ConfigFile = toml::from_str(&contents).map_err(|source| ConfigError::Parse {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(Some(merge_config(parsed)?))
}

fn merge_config(parsed: ConfigFile) -> Result<AppConfig> {
    let mut config = AppConfig::default();

    if let Some(soon_days) = parsed.due_soon_days {
        let soon_days =
            validate_soon_days(soon_days).map_err(|_| ConfigError::InvalidSoonDays(soon_days))?;
        config.due_soon_days = soon_days;
    }

    if let Some(cadence) = parsed.default_cadence_days {
        if cadence <= 0 || cadence > MAX_CADENCE_DAYS {
            return Err(ConfigError::InvalidCadenceDays(cadence));
        }
        config.default_cadence_days = Some(cadence);
    }

    if let Some(notifications) = parsed.notifications {
        if let Some(enabled) = notifications.enabled {
            config.notifications.enabled = enabled;
        }
        if let Some(backend) = notifications.backend {
            config.notifications.backend = backend;
        }
    }

    Ok(config)
}

#[cfg(unix)]
fn ensure_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = fs::metadata(path).map_err(|source| ConfigError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let mode = metadata.permissions().mode();
    if mode & 0o077 != 0 {
        return Err(ConfigError::InsecurePermissions(path.to_path_buf()));
    }
    Ok(())
}

#[cfg(not(unix))]
fn ensure_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{load_at_path, merge_config, ConfigFile, NotificationBackend, NotificationsFile};
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    fn restrict_permissions(path: &Path) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(path).expect("metadata").permissions();
            perms.set_mode(0o600);
            fs::set_permissions(path, perms).expect("chmod");
        }
    }

    #[test]
    fn merge_config_applies_values() {
        let parsed = ConfigFile {
            due_soon_days: Some(3),
            default_cadence_days: Some(14),
            notifications: Some(NotificationsFile {
                enabled: Some(true),
                backend: Some(NotificationBackend::Desktop),
            }),
        };
        let merged = merge_config(parsed).expect("merge");
        assert_eq!(merged.due_soon_days, 3);
        assert_eq!(merged.default_cadence_days, Some(14));
        assert!(merged.notifications.enabled);
        assert_eq!(merged.notifications.backend, NotificationBackend::Desktop);
    }

    #[test]
    fn load_at_path_requires_file_when_requested() {
        let temp = TempDir::new().expect("tempdir");
        let missing = temp.path().join("config.toml");
        let err = load_at_path(&missing, true).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("config file not found"));
    }

    #[test]
    fn load_at_path_parses_toml() {
        let temp = TempDir::new().expect("tempdir");
        let path = temp.path().join("config.toml");
        fs::write(
            &path,
            "due_soon_days = 5\n[notifications]\nenabled = true\nbackend = \"stdout\"\n",
        )
        .expect("write config");
        restrict_permissions(&path);

        let config = load_at_path(&path, true).expect("load").expect("config");
        assert_eq!(config.due_soon_days, 5);
        assert!(config.notifications.enabled);
    }
}
