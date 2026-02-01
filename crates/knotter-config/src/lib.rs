use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use knotter_core::domain::TagName;
use knotter_core::rules::cadence::MAX_CADENCE_DAYS;
use knotter_core::rules::{validate_soon_days, LoopPolicy, LoopRule, LoopStrategy};
use serde::Deserialize;
use thiserror::Error;

const APP_DIR: &str = "knotter";
const CONFIG_FILENAME: &str = "config.toml";

pub const DEFAULT_SOON_DAYS: i64 = 7;
pub const DEFAULT_TELEGRAM_SNIPPET_LEN: usize = 160;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub due_soon_days: i64,
    pub default_cadence_days: Option<i32>,
    pub notifications: NotificationsConfig,
    pub interactions: InteractionsConfig,
    pub loops: LoopConfig,
    pub contacts: ContactsConfig,
}

#[derive(Debug, Clone)]
pub struct NotificationsConfig {
    pub enabled: bool,
    pub backend: NotificationBackend,
    pub email: Option<NotificationsEmailConfig>,
}

#[derive(Debug, Clone, Default)]
pub struct InteractionsConfig {
    pub auto_reschedule: bool,
}

#[derive(Debug, Clone)]
pub struct NotificationsEmailConfig {
    pub from: String,
    pub to: Vec<String>,
    pub subject_prefix: String,
    pub smtp_host: String,
    pub smtp_port: Option<u16>,
    pub username: Option<String>,
    pub password_env: Option<String>,
    pub tls: EmailTls,
    pub timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct LoopConfig {
    pub policy: LoopPolicy,
    pub apply_on_tag_change: bool,
    pub schedule_missing: bool,
    pub anchor: LoopAnchor,
    pub override_existing: bool,
}

impl Default for LoopConfig {
    fn default() -> Self {
        Self {
            policy: LoopPolicy::default(),
            apply_on_tag_change: false,
            schedule_missing: false,
            anchor: LoopAnchor::Now,
            override_existing: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LoopAnchor {
    Now,
    CreatedAt,
    LastInteraction,
}

#[derive(Debug, Clone, Default)]
pub struct ContactsConfig {
    pub sources: Vec<ContactSourceConfig>,
    pub email_accounts: Vec<EmailAccountConfig>,
    pub telegram_accounts: Vec<TelegramAccountConfig>,
}

impl ContactsConfig {
    pub fn source(&self, name: &str) -> Option<&ContactSourceConfig> {
        let needle = normalize_source_name(name).ok()?;
        self.sources.iter().find(|source| source.name == needle)
    }

    pub fn email_account(&self, name: &str) -> Option<&EmailAccountConfig> {
        let needle = normalize_source_name(name).ok()?;
        self.email_accounts
            .iter()
            .find(|account| account.name == needle)
    }

    pub fn telegram_account(&self, name: &str) -> Option<&TelegramAccountConfig> {
        let needle = normalize_source_name(name).ok()?;
        self.telegram_accounts
            .iter()
            .find(|account| account.name == needle)
    }
}

#[derive(Debug, Clone)]
pub struct ContactSourceConfig {
    pub name: String,
    pub kind: ContactSourceKind,
}

#[derive(Debug, Clone)]
pub enum ContactSourceKind {
    Carddav(CardDavSourceConfig),
    Macos(MacosSourceConfig),
}

#[derive(Debug, Clone)]
pub struct CardDavSourceConfig {
    pub url: String,
    pub username: Option<String>,
    pub password_env: Option<String>,
    pub tag: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MacosSourceConfig {
    pub group: Option<String>,
    pub tag: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EmailMergePolicy {
    EmailOnly,
    NameOrEmail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EmailAccountTls {
    Tls,
    StartTls,
    None,
}

#[derive(Debug, Clone)]
pub struct EmailAccountConfig {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password_env: String,
    pub mailboxes: Vec<String>,
    pub identities: Vec<String>,
    pub tag: Option<String>,
    pub merge_policy: EmailMergePolicy,
    pub tls: EmailAccountTls,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TelegramMergePolicy {
    #[default]
    NameOrUsername,
    UsernameOnly,
}

#[derive(Debug, Clone)]
pub struct TelegramAccountConfig {
    pub name: String,
    pub api_id: i32,
    pub api_hash_env: String,
    pub phone: String,
    pub session_path: Option<PathBuf>,
    pub tag: Option<String>,
    pub merge_policy: TelegramMergePolicy,
    pub allowlist_user_ids: Vec<i64>,
    pub snippet_len: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotificationBackend {
    Stdout,
    Desktop,
    Email,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum EmailTls {
    None,
    #[default]
    StartTls,
    Tls,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            due_soon_days: DEFAULT_SOON_DAYS,
            default_cadence_days: None,
            notifications: NotificationsConfig {
                enabled: false,
                backend: NotificationBackend::Desktop,
                email: None,
            },
            interactions: InteractionsConfig::default(),
            loops: LoopConfig::default(),
            contacts: ContactsConfig::default(),
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
    #[error("invalid loops.default_cadence_days value: {0}")]
    InvalidLoopDefaultCadence(i32),
    #[error("invalid loops rule cadence_days value: {0}")]
    InvalidLoopCadenceDays(i32),
    #[error("invalid loops rule tag: {0}")]
    InvalidLoopTag(String),
    #[error("duplicate loops rule tag: {0}")]
    DuplicateLoopTag(String),
    #[error("invalid contact source name: {0}")]
    InvalidContactSourceName(String),
    #[error("duplicate contact source name: {0}")]
    DuplicateContactSourceName(String),
    #[error("invalid contact source {source_name} field: {field}")]
    InvalidContactSourceField { source_name: String, field: String },
    #[error("invalid email account name: {0}")]
    InvalidEmailAccountName(String),
    #[error("duplicate email account name: {0}")]
    DuplicateEmailAccountName(String),
    #[error("invalid email account {account_name} field: {field}")]
    InvalidEmailAccountField { account_name: String, field: String },
    #[error("invalid telegram account name: {0}")]
    InvalidTelegramAccountName(String),
    #[error("duplicate telegram account name: {0}")]
    DuplicateTelegramAccountName(String),
    #[error("invalid telegram account {account_name} field: {field}")]
    InvalidTelegramAccountField { account_name: String, field: String },
    #[error("invalid notifications email field: {field}")]
    InvalidNotificationsEmailField { field: String },
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
    interactions: Option<InteractionsFile>,
    loops: Option<LoopConfigFile>,
    contacts: Option<ContactsFile>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NotificationsFile {
    enabled: Option<bool>,
    backend: Option<NotificationBackend>,
    email: Option<NotificationsEmailFile>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NotificationsEmailFile {
    from: Option<String>,
    to: Option<Vec<String>>,
    subject_prefix: Option<String>,
    smtp_host: Option<String>,
    smtp_port: Option<u16>,
    username: Option<String>,
    password_env: Option<String>,
    tls: Option<EmailTls>,
    timeout_seconds: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct InteractionsFile {
    auto_reschedule: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LoopConfigFile {
    default_cadence_days: Option<i32>,
    strategy: Option<LoopStrategy>,
    apply_on_tag_change: Option<bool>,
    schedule_missing: Option<bool>,
    anchor: Option<LoopAnchor>,
    override_existing: Option<bool>,
    tags: Option<Vec<LoopRuleFile>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LoopRuleFile {
    tag: String,
    cadence_days: i32,
    priority: Option<i32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ContactsFile {
    sources: Option<Vec<ContactSourceFile>>,
    email_accounts: Option<Vec<EmailAccountFile>>,
    telegram_accounts: Option<Vec<TelegramAccountFile>>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum ContactSourceFile {
    Carddav {
        name: String,
        url: String,
        username: Option<String>,
        password_env: Option<String>,
        tag: Option<String>,
    },
    Macos {
        name: String,
        group: Option<String>,
        tag: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EmailAccountFile {
    name: String,
    host: String,
    port: Option<u16>,
    username: String,
    password_env: String,
    mailboxes: Option<Vec<String>>,
    identities: Option<Vec<String>>,
    tag: Option<String>,
    merge_policy: Option<EmailMergePolicy>,
    tls: Option<EmailAccountTls>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TelegramAccountFile {
    name: String,
    api_id: i32,
    api_hash_env: String,
    phone: String,
    session_path: Option<String>,
    tag: Option<String>,
    merge_policy: Option<TelegramMergePolicy>,
    allowlist_user_ids: Option<Vec<i64>>,
    snippet_len: Option<usize>,
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
        if let Some(email) = notifications.email {
            config.notifications.email = Some(merge_notifications_email(email)?);
        }
    }

    if let Some(interactions) = parsed.interactions {
        if let Some(auto_reschedule) = interactions.auto_reschedule {
            config.interactions.auto_reschedule = auto_reschedule;
        }
    }

    if config.notifications.enabled
        && config.notifications.backend == NotificationBackend::Email
        && config.notifications.email.is_none()
    {
        return Err(ConfigError::InvalidNotificationsEmailField {
            field: "notifications.email".to_string(),
        });
    }

    if let Some(loops) = parsed.loops {
        if let Some(default_cadence) = loops.default_cadence_days {
            if default_cadence <= 0 || default_cadence > MAX_CADENCE_DAYS {
                return Err(ConfigError::InvalidLoopDefaultCadence(default_cadence));
            }
            config.loops.policy.default_cadence_days = Some(default_cadence);
        }

        if let Some(strategy) = loops.strategy {
            config.loops.policy.strategy = strategy;
        }

        if let Some(enabled) = loops.apply_on_tag_change {
            config.loops.apply_on_tag_change = enabled;
        }

        if let Some(schedule_missing) = loops.schedule_missing {
            config.loops.schedule_missing = schedule_missing;
        }

        if let Some(anchor) = loops.anchor {
            config.loops.anchor = anchor;
        }

        if let Some(override_existing) = loops.override_existing {
            config.loops.override_existing = override_existing;
        }

        if let Some(rules) = loops.tags {
            let mut seen: HashSet<String> = HashSet::new();
            for rule in rules {
                let tag = TagName::new(&rule.tag)
                    .map_err(|_| ConfigError::InvalidLoopTag(rule.tag.clone()))?;
                let normalized = tag.as_str().to_string();
                if !seen.insert(normalized.clone()) {
                    return Err(ConfigError::DuplicateLoopTag(normalized));
                }

                let priority = rule.priority.unwrap_or(0);
                let loop_rule = LoopRule::new(tag, rule.cadence_days, priority)
                    .map_err(|_| ConfigError::InvalidLoopCadenceDays(rule.cadence_days))?;
                config.loops.policy.rules.push(loop_rule);
            }
        }
    }

    if let Some(contacts) = parsed.contacts {
        if let Some(sources) = contacts.sources {
            let mut seen: HashSet<String> = HashSet::new();
            for source in sources {
                let (name, kind) = match source {
                    ContactSourceFile::Carddav {
                        name,
                        url,
                        username,
                        password_env,
                        tag,
                    } => {
                        let name = normalize_source_name(&name)?;
                        let url = normalize_required_string(url, &name, "url")?;
                        let username = normalize_optional_string(username).ok_or_else(|| {
                            ConfigError::InvalidContactSourceField {
                                source_name: name.clone(),
                                field: "username".to_string(),
                            }
                        })?;
                        let password_env = normalize_optional_string(password_env);
                        let tag = normalize_optional_tag(tag, &name)?;
                        (
                            name,
                            ContactSourceKind::Carddav(CardDavSourceConfig {
                                url,
                                username: Some(username),
                                password_env,
                                tag,
                            }),
                        )
                    }
                    ContactSourceFile::Macos { name, group, tag } => {
                        let name = normalize_source_name(&name)?;
                        let group = normalize_optional_string(group);
                        let tag = normalize_optional_tag(tag, &name)?;
                        (
                            name,
                            ContactSourceKind::Macos(MacosSourceConfig { group, tag }),
                        )
                    }
                };

                if !seen.insert(name.clone()) {
                    return Err(ConfigError::DuplicateContactSourceName(name));
                }

                config
                    .contacts
                    .sources
                    .push(ContactSourceConfig { name, kind });
            }
        }
        if let Some(accounts) = contacts.email_accounts {
            let mut seen: HashSet<String> = HashSet::new();
            for account in accounts {
                let name = normalize_email_account_name(&account.name)?;
                if !seen.insert(name.clone()) {
                    return Err(ConfigError::DuplicateEmailAccountName(name));
                }
                let host = normalize_email_account_field(account.host, &name, "host")?;
                let port = account.port.unwrap_or(993);
                if port == 0 {
                    return Err(ConfigError::InvalidEmailAccountField {
                        account_name: name.clone(),
                        field: "port".to_string(),
                    });
                }
                let username = normalize_email_account_field(account.username, &name, "username")?;
                let password_env =
                    normalize_email_account_field(account.password_env, &name, "password_env")?;
                let mailboxes = normalize_mailboxes(account.mailboxes, &name)?;
                let identities = normalize_identities(account.identities, &username);
                let tag = normalize_optional_tag_for_email_account(account.tag, &name)?;
                let merge_policy = account
                    .merge_policy
                    .unwrap_or(EmailMergePolicy::NameOrEmail);
                let tls = account.tls.unwrap_or(EmailAccountTls::Tls);

                config.contacts.email_accounts.push(EmailAccountConfig {
                    name,
                    host,
                    port,
                    username,
                    password_env,
                    mailboxes,
                    identities,
                    tag,
                    merge_policy,
                    tls,
                });
            }
        }
        if let Some(accounts) = contacts.telegram_accounts {
            let mut seen: HashSet<String> = HashSet::new();
            for account in accounts {
                let name = normalize_telegram_account_name(&account.name)?;
                if !seen.insert(name.clone()) {
                    return Err(ConfigError::DuplicateTelegramAccountName(name));
                }
                if account.api_id <= 0 {
                    return Err(ConfigError::InvalidTelegramAccountField {
                        account_name: name.clone(),
                        field: "api_id".to_string(),
                    });
                }
                let api_hash_env =
                    normalize_telegram_account_field(account.api_hash_env, &name, "api_hash_env")?;
                let phone = normalize_telegram_account_field(account.phone, &name, "phone")?;
                let session_path =
                    normalize_optional_string(account.session_path).map(PathBuf::from);
                let tag = normalize_optional_tag_for_telegram_account(account.tag, &name)?;
                let merge_policy = account.merge_policy.unwrap_or_default();
                let allowlist_user_ids =
                    normalize_allowlist_user_ids(account.allowlist_user_ids, &name)?;
                let snippet_len = match account.snippet_len {
                    Some(0) => {
                        return Err(ConfigError::InvalidTelegramAccountField {
                            account_name: name.clone(),
                            field: "snippet_len".to_string(),
                        })
                    }
                    Some(value) => value,
                    None => DEFAULT_TELEGRAM_SNIPPET_LEN,
                };

                config
                    .contacts
                    .telegram_accounts
                    .push(TelegramAccountConfig {
                        name,
                        api_id: account.api_id,
                        api_hash_env,
                        phone,
                        session_path,
                        tag,
                        merge_policy,
                        allowlist_user_ids,
                        snippet_len,
                    });
            }
        }
    }

    Ok(config)
}

fn merge_notifications_email(file: NotificationsEmailFile) -> Result<NotificationsEmailConfig> {
    let from = normalize_required_email_field(file.from, "notifications.email.from")?;
    validate_email_address(&from, "notifications.email.from")?;
    let to_values = file
        .to
        .ok_or_else(|| ConfigError::InvalidNotificationsEmailField {
            field: "notifications.email.to".to_string(),
        })?;
    let mut to = Vec::new();
    for value in to_values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(ConfigError::InvalidNotificationsEmailField {
                field: "notifications.email.to".to_string(),
            });
        }
        validate_email_address(trimmed, "notifications.email.to")?;
        to.push(trimmed.to_string());
    }
    if to.is_empty() {
        return Err(ConfigError::InvalidNotificationsEmailField {
            field: "notifications.email.to".to_string(),
        });
    }

    let smtp_host =
        normalize_required_email_field(file.smtp_host, "notifications.email.smtp_host")?;
    let smtp_port = match file.smtp_port {
        Some(0) => {
            return Err(ConfigError::InvalidNotificationsEmailField {
                field: "notifications.email.smtp_port".to_string(),
            })
        }
        Some(port) => Some(port),
        None => None,
    };
    let subject_prefix = normalize_optional_string(file.subject_prefix)
        .unwrap_or_else(|| "knotter reminders".to_string());
    let username = normalize_optional_string(file.username);
    let password_env = normalize_optional_string(file.password_env);
    if username.is_some() != password_env.is_some() {
        return Err(ConfigError::InvalidNotificationsEmailField {
            field: "notifications.email.username/password_env".to_string(),
        });
    }
    let tls = file.tls.unwrap_or_default();
    let timeout_seconds = match file.timeout_seconds {
        Some(0) => {
            return Err(ConfigError::InvalidNotificationsEmailField {
                field: "notifications.email.timeout_seconds".to_string(),
            })
        }
        Some(value) => Some(value),
        None => None,
    };

    Ok(NotificationsEmailConfig {
        from,
        to,
        subject_prefix,
        smtp_host,
        smtp_port,
        username,
        password_env,
        tls,
        timeout_seconds,
    })
}

fn normalize_source_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(ConfigError::InvalidContactSourceName(name.to_string()));
    }
    Ok(trimmed.to_ascii_lowercase())
}

fn normalize_email_account_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(ConfigError::InvalidEmailAccountName(name.to_string()));
    }
    Ok(trimmed.to_ascii_lowercase())
}

fn normalize_email_account_field(value: String, account: &str, field: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ConfigError::InvalidEmailAccountField {
            account_name: account.to_string(),
            field: field.to_string(),
        });
    }
    Ok(trimmed.to_string())
}

fn normalize_telegram_account_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(ConfigError::InvalidTelegramAccountName(name.to_string()));
    }
    let lowered = trimmed.to_ascii_lowercase();
    let mut components = std::path::Path::new(&lowered).components();
    match components.next() {
        Some(std::path::Component::Normal(_)) if components.next().is_none() => Ok(lowered),
        _ => Err(ConfigError::InvalidTelegramAccountName(name.to_string())),
    }
}

fn normalize_telegram_account_field(value: String, account: &str, field: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ConfigError::InvalidTelegramAccountField {
            account_name: account.to_string(),
            field: field.to_string(),
        });
    }
    Ok(trimmed.to_string())
}

fn normalize_mailboxes(value: Option<Vec<String>>, account: &str) -> Result<Vec<String>> {
    let list = value.unwrap_or_else(|| vec!["INBOX".to_string()]);
    let mut out = Vec::new();
    for raw in list {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(ConfigError::InvalidEmailAccountField {
                account_name: account.to_string(),
                field: "mailboxes".to_string(),
            });
        }
        if !out
            .iter()
            .any(|existing: &String| existing.eq_ignore_ascii_case(trimmed))
        {
            out.push(trimmed.to_string());
        }
    }
    if out.is_empty() {
        return Err(ConfigError::InvalidEmailAccountField {
            account_name: account.to_string(),
            field: "mailboxes".to_string(),
        });
    }
    Ok(out)
}

fn normalize_identities(value: Option<Vec<String>>, username: &str) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(values) = value {
        for raw in values {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }
            if !out
                .iter()
                .any(|existing: &String| existing.eq_ignore_ascii_case(trimmed))
            {
                out.push(trimmed.to_string());
            }
        }
    }
    if out.is_empty() && username.contains('@') {
        out.push(username.to_string());
    }
    out
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn normalize_required_email_field(value: Option<String>, field: &str) -> Result<String> {
    let value = value.ok_or_else(|| ConfigError::InvalidNotificationsEmailField {
        field: field.to_string(),
    })?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ConfigError::InvalidNotificationsEmailField {
            field: field.to_string(),
        });
    }
    Ok(trimmed.to_string())
}

fn validate_email_address(value: &str, field: &str) -> Result<()> {
    if value.parse::<lettre::message::Mailbox>().is_err() {
        return Err(ConfigError::InvalidNotificationsEmailField {
            field: field.to_string(),
        });
    }
    Ok(())
}

fn normalize_optional_tag(value: Option<String>, source_name: &str) -> Result<Option<String>> {
    match value {
        Some(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Err(ConfigError::InvalidContactSourceField {
                    source_name: source_name.to_string(),
                    field: "tag".to_string(),
                });
            }
            let tag = knotter_core::domain::TagName::new(trimmed).map_err(|_| {
                ConfigError::InvalidContactSourceField {
                    source_name: source_name.to_string(),
                    field: "tag".to_string(),
                }
            })?;
            Ok(Some(tag.as_str().to_string()))
        }
        None => Ok(None),
    }
}

fn normalize_optional_tag_for_email_account(
    value: Option<String>,
    account_name: &str,
) -> Result<Option<String>> {
    match value {
        Some(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Err(ConfigError::InvalidEmailAccountField {
                    account_name: account_name.to_string(),
                    field: "tag".to_string(),
                });
            }
            let tag = knotter_core::domain::TagName::new(trimmed).map_err(|_| {
                ConfigError::InvalidEmailAccountField {
                    account_name: account_name.to_string(),
                    field: "tag".to_string(),
                }
            })?;
            Ok(Some(tag.as_str().to_string()))
        }
        None => Ok(None),
    }
}

fn normalize_optional_tag_for_telegram_account(
    value: Option<String>,
    account_name: &str,
) -> Result<Option<String>> {
    match value {
        Some(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Err(ConfigError::InvalidTelegramAccountField {
                    account_name: account_name.to_string(),
                    field: "tag".to_string(),
                });
            }
            let tag = knotter_core::domain::TagName::new(trimmed).map_err(|_| {
                ConfigError::InvalidTelegramAccountField {
                    account_name: account_name.to_string(),
                    field: "tag".to_string(),
                }
            })?;
            Ok(Some(tag.as_str().to_string()))
        }
        None => Ok(None),
    }
}

fn normalize_allowlist_user_ids(values: Option<Vec<i64>>, account_name: &str) -> Result<Vec<i64>> {
    let Some(values) = values else {
        return Ok(Vec::new());
    };
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for value in values {
        if value <= 0 {
            return Err(ConfigError::InvalidTelegramAccountField {
                account_name: account_name.to_string(),
                field: "allowlist_user_ids".to_string(),
            });
        }
        if seen.insert(value) {
            out.push(value);
        }
    }
    Ok(out)
}

fn normalize_required_string(value: String, source: &str, field: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ConfigError::InvalidContactSourceField {
            source_name: source.to_string(),
            field: field.to_string(),
        });
    }
    Ok(trimmed.to_string())
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
    use super::{
        load_at_path, merge_config, CardDavSourceConfig, ConfigFile, ContactSourceFile,
        ContactSourceKind, ContactsFile, EmailAccountFile, EmailAccountTls, EmailMergePolicy,
        EmailTls, LoopAnchor, LoopConfigFile, LoopRuleFile, LoopStrategy, MacosSourceConfig,
        NotificationBackend, NotificationsEmailFile, NotificationsFile, TelegramAccountFile,
        TelegramMergePolicy, DEFAULT_TELEGRAM_SNIPPET_LEN,
    };
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
                email: None,
            }),
            interactions: None,
            loops: None,
            contacts: None,
        };
        let merged = merge_config(parsed).expect("merge");
        assert_eq!(merged.due_soon_days, 3);
        assert_eq!(merged.default_cadence_days, Some(14));
        assert!(merged.notifications.enabled);
        assert_eq!(merged.notifications.backend, NotificationBackend::Desktop);
    }

    #[test]
    fn merge_config_parses_email_notifications() {
        let parsed = ConfigFile {
            due_soon_days: None,
            default_cadence_days: None,
            notifications: Some(NotificationsFile {
                enabled: Some(true),
                backend: Some(NotificationBackend::Email),
                email: Some(NotificationsEmailFile {
                    from: Some("Knotter <knotter@example.com>".to_string()),
                    to: Some(vec![
                        "one@example.com".to_string(),
                        " two@example.com ".to_string(),
                    ]),
                    subject_prefix: Some("Reminders".to_string()),
                    smtp_host: Some("smtp.example.com".to_string()),
                    smtp_port: Some(587),
                    username: Some("user@example.com".to_string()),
                    password_env: Some("KNOTTER_SMTP_PASSWORD".to_string()),
                    tls: Some(EmailTls::StartTls),
                    timeout_seconds: Some(20),
                }),
            }),
            interactions: None,
            loops: None,
            contacts: None,
        };

        let merged = merge_config(parsed).expect("merge");
        assert_eq!(merged.notifications.backend, NotificationBackend::Email);
        let email = merged.notifications.email.expect("email config");
        assert_eq!(email.from, "Knotter <knotter@example.com>");
        assert_eq!(email.to.len(), 2);
        assert_eq!(email.to[1], "two@example.com");
        assert_eq!(email.subject_prefix, "Reminders");
        assert_eq!(email.smtp_host, "smtp.example.com");
        assert_eq!(email.smtp_port, Some(587));
        assert_eq!(email.username.as_deref(), Some("user@example.com"));
        assert_eq!(email.password_env.as_deref(), Some("KNOTTER_SMTP_PASSWORD"));
        assert_eq!(email.tls, EmailTls::StartTls);
        assert_eq!(email.timeout_seconds, Some(20));
    }

    #[test]
    fn merge_config_rejects_email_backend_without_email_config() {
        let parsed = ConfigFile {
            due_soon_days: None,
            default_cadence_days: None,
            notifications: Some(NotificationsFile {
                enabled: Some(true),
                backend: Some(NotificationBackend::Email),
                email: None,
            }),
            interactions: None,
            loops: None,
            contacts: None,
        };

        let err = merge_config(parsed).unwrap_err();
        assert!(err.to_string().contains("notifications.email"));
    }

    #[test]
    fn merge_config_allows_email_backend_when_disabled_without_email_config() {
        let parsed = ConfigFile {
            due_soon_days: None,
            default_cadence_days: None,
            notifications: Some(NotificationsFile {
                enabled: Some(false),
                backend: Some(NotificationBackend::Email),
                email: None,
            }),
            interactions: None,
            loops: None,
            contacts: None,
        };

        let merged = merge_config(parsed).expect("merge");
        assert!(!merged.notifications.enabled);
        assert_eq!(merged.notifications.backend, NotificationBackend::Email);
        assert!(merged.notifications.email.is_none());
    }

    #[test]
    fn merge_config_rejects_email_missing_password_env() {
        let parsed = ConfigFile {
            due_soon_days: None,
            default_cadence_days: None,
            notifications: Some(NotificationsFile {
                enabled: Some(true),
                backend: Some(NotificationBackend::Email),
                email: Some(NotificationsEmailFile {
                    from: Some("knotter@example.com".to_string()),
                    to: Some(vec!["one@example.com".to_string()]),
                    subject_prefix: None,
                    smtp_host: Some("smtp.example.com".to_string()),
                    smtp_port: Some(587),
                    username: Some("user@example.com".to_string()),
                    password_env: None,
                    tls: None,
                    timeout_seconds: None,
                }),
            }),
            interactions: None,
            loops: None,
            contacts: None,
        };

        let err = merge_config(parsed).unwrap_err();
        assert!(err.to_string().contains("username/password_env"));
    }

    #[test]
    fn merge_config_rejects_invalid_email_addresses() {
        let parsed = ConfigFile {
            due_soon_days: None,
            default_cadence_days: None,
            notifications: Some(NotificationsFile {
                enabled: Some(true),
                backend: Some(NotificationBackend::Email),
                email: Some(NotificationsEmailFile {
                    from: Some("not-an-email".to_string()),
                    to: Some(vec!["also-bad".to_string()]),
                    subject_prefix: None,
                    smtp_host: Some("smtp.example.com".to_string()),
                    smtp_port: Some(587),
                    username: None,
                    password_env: None,
                    tls: None,
                    timeout_seconds: None,
                }),
            }),
            interactions: None,
            loops: None,
            contacts: None,
        };

        let err = merge_config(parsed).unwrap_err();
        assert!(err.to_string().contains("notifications.email"));
    }

    #[test]
    fn merge_config_parses_contact_sources() {
        let parsed = ConfigFile {
            due_soon_days: None,
            default_cadence_days: None,
            notifications: None,
            interactions: None,
            loops: None,
            contacts: Some(ContactsFile {
                sources: Some(vec![
                    ContactSourceFile::Carddav {
                        name: "Gmail".to_string(),
                        url: "https://example.test/carddav/".to_string(),
                        username: Some("user@example.com".to_string()),
                        password_env: Some("KNOTTER_GMAIL_PASSWORD".to_string()),
                        tag: Some("gmail".to_string()),
                    },
                    ContactSourceFile::Macos {
                        name: "Local".to_string(),
                        group: Some("Friends".to_string()),
                        tag: None,
                    },
                ]),
                email_accounts: None,
                telegram_accounts: None,
            }),
        };

        let merged = merge_config(parsed).expect("merge");
        assert_eq!(merged.contacts.sources.len(), 2);
        let gmail = &merged.contacts.sources[0];
        assert_eq!(gmail.name, "gmail");
        match &gmail.kind {
            ContactSourceKind::Carddav(CardDavSourceConfig { url, username, .. }) => {
                assert_eq!(url, "https://example.test/carddav/");
                assert_eq!(username.as_deref(), Some("user@example.com"));
            }
            _ => panic!("expected carddav"),
        }
        let local = &merged.contacts.sources[1];
        match &local.kind {
            ContactSourceKind::Macos(MacosSourceConfig { group, .. }) => {
                assert_eq!(group.as_deref(), Some("Friends"));
            }
            _ => panic!("expected macos"),
        }
    }

    #[test]
    fn merge_config_parses_email_accounts() {
        let parsed = ConfigFile {
            due_soon_days: None,
            default_cadence_days: None,
            notifications: None,
            interactions: None,
            loops: None,
            contacts: Some(ContactsFile {
                sources: None,
                email_accounts: Some(vec![EmailAccountFile {
                    name: "Gmail".to_string(),
                    host: "imap.example.com".to_string(),
                    port: None,
                    username: "user@example.com".to_string(),
                    password_env: "KNOTTER_GMAIL_PASSWORD".to_string(),
                    mailboxes: Some(vec!["INBOX".to_string(), "Sent".to_string()]),
                    identities: Some(vec!["user@example.com".to_string()]),
                    tag: Some("friends".to_string()),
                    merge_policy: Some(EmailMergePolicy::NameOrEmail),
                    tls: Some(EmailAccountTls::Tls),
                }]),
                telegram_accounts: None,
            }),
        };

        let merged = merge_config(parsed).expect("merge");
        assert_eq!(merged.contacts.email_accounts.len(), 1);
        let account = &merged.contacts.email_accounts[0];
        assert_eq!(account.name, "gmail");
        assert_eq!(account.host, "imap.example.com");
        assert_eq!(account.port, 993);
        assert_eq!(account.mailboxes, vec!["INBOX", "Sent"]);
        assert_eq!(account.identities, vec!["user@example.com"]);
        assert_eq!(account.tag.as_deref(), Some("friends"));
        assert_eq!(account.merge_policy, EmailMergePolicy::NameOrEmail);
        assert_eq!(account.tls, EmailAccountTls::Tls);
    }

    #[test]
    fn merge_config_parses_telegram_accounts() {
        let parsed = ConfigFile {
            due_soon_days: None,
            default_cadence_days: None,
            notifications: None,
            interactions: None,
            loops: None,
            contacts: Some(ContactsFile {
                sources: None,
                email_accounts: None,
                telegram_accounts: Some(vec![TelegramAccountFile {
                    name: "Primary".to_string(),
                    api_id: 123,
                    api_hash_env: "KNOTTER_TELEGRAM_HASH".to_string(),
                    phone: "+15551234567".to_string(),
                    session_path: Some("/tmp/knotter-telegram.session".to_string()),
                    tag: Some("friends".to_string()),
                    merge_policy: Some(TelegramMergePolicy::NameOrUsername),
                    allowlist_user_ids: Some(vec![42, 7, 42]),
                    snippet_len: None,
                }]),
            }),
        };

        let merged = merge_config(parsed).expect("merge");
        assert_eq!(merged.contacts.telegram_accounts.len(), 1);
        let account = &merged.contacts.telegram_accounts[0];
        assert_eq!(account.name, "primary");
        assert_eq!(account.api_id, 123);
        assert_eq!(account.api_hash_env, "KNOTTER_TELEGRAM_HASH");
        assert_eq!(account.phone, "+15551234567");
        assert_eq!(
            account
                .session_path
                .as_ref()
                .map(|path| path.display().to_string()),
            Some("/tmp/knotter-telegram.session".to_string())
        );
        assert_eq!(account.tag.as_deref(), Some("friends"));
        assert_eq!(account.merge_policy, TelegramMergePolicy::NameOrUsername);
        assert_eq!(account.allowlist_user_ids, vec![42, 7]);
        assert_eq!(account.snippet_len, DEFAULT_TELEGRAM_SNIPPET_LEN);
    }

    #[test]
    fn merge_config_rejects_invalid_telegram_account_name() {
        let parsed = ConfigFile {
            due_soon_days: None,
            default_cadence_days: None,
            notifications: None,
            interactions: None,
            loops: None,
            contacts: Some(ContactsFile {
                sources: None,
                email_accounts: None,
                telegram_accounts: Some(vec![TelegramAccountFile {
                    name: "../Primary".to_string(),
                    api_id: 123,
                    api_hash_env: "KNOTTER_TELEGRAM_HASH".to_string(),
                    phone: "+15551234567".to_string(),
                    session_path: None,
                    tag: None,
                    merge_policy: None,
                    allowlist_user_ids: None,
                    snippet_len: None,
                }]),
            }),
        };

        let err = merge_config(parsed).expect_err("expected invalid name");
        assert!(matches!(
            err,
            crate::ConfigError::InvalidTelegramAccountName(_)
        ));
    }

    #[test]
    fn merge_config_rejects_duplicate_sources() {
        let parsed = ConfigFile {
            due_soon_days: None,
            default_cadence_days: None,
            notifications: None,
            interactions: None,
            loops: None,
            contacts: Some(ContactsFile {
                sources: Some(vec![
                    ContactSourceFile::Macos {
                        name: "Primary".to_string(),
                        group: None,
                        tag: None,
                    },
                    ContactSourceFile::Macos {
                        name: "primary".to_string(),
                        group: None,
                        tag: None,
                    },
                ]),
                email_accounts: None,
                telegram_accounts: None,
            }),
        };

        let err = merge_config(parsed).unwrap_err();
        assert!(err.to_string().contains("duplicate contact source name"));
    }

    #[test]
    fn merge_config_rejects_empty_carddav_url() {
        let parsed = ConfigFile {
            due_soon_days: None,
            default_cadence_days: None,
            notifications: None,
            interactions: None,
            loops: None,
            contacts: Some(ContactsFile {
                sources: Some(vec![ContactSourceFile::Carddav {
                    name: "Gmail".to_string(),
                    url: "   ".to_string(),
                    username: Some("user@example.com".to_string()),
                    password_env: Some("KNOTTER_GMAIL_PASSWORD".to_string()),
                    tag: None,
                }]),
                email_accounts: None,
                telegram_accounts: None,
            }),
        };

        let err = merge_config(parsed).unwrap_err();
        assert!(err.to_string().contains("invalid contact source"));
    }

    #[test]
    fn merge_config_trims_optional_contact_fields() {
        let parsed = ConfigFile {
            due_soon_days: None,
            default_cadence_days: None,
            notifications: None,
            interactions: None,
            loops: None,
            contacts: Some(ContactsFile {
                sources: Some(vec![ContactSourceFile::Carddav {
                    name: "Gmail".to_string(),
                    url: "https://example.test/carddav/".to_string(),
                    username: Some("user@example.com".to_string()),
                    password_env: Some("".to_string()),
                    tag: Some("friends".to_string()),
                }]),
                email_accounts: None,
                telegram_accounts: None,
            }),
        };

        let merged = merge_config(parsed).expect("merge");
        let source = merged.contacts.sources.first().expect("source");
        match &source.kind {
            ContactSourceKind::Carddav(CardDavSourceConfig {
                password_env, tag, ..
            }) => {
                assert!(password_env.is_none());
                assert_eq!(tag.as_deref(), Some("friends"));
            }
            _ => panic!("expected carddav"),
        }
    }

    #[test]
    fn merge_config_parses_loops() {
        let parsed = ConfigFile {
            due_soon_days: None,
            default_cadence_days: None,
            notifications: None,
            interactions: None,
            loops: Some(LoopConfigFile {
                default_cadence_days: Some(180),
                strategy: Some(LoopStrategy::Priority),
                apply_on_tag_change: Some(true),
                schedule_missing: Some(true),
                anchor: Some(LoopAnchor::LastInteraction),
                override_existing: Some(true),
                tags: Some(vec![
                    LoopRuleFile {
                        tag: "friend".to_string(),
                        cadence_days: 90,
                        priority: Some(10),
                    },
                    LoopRuleFile {
                        tag: "family".to_string(),
                        cadence_days: 30,
                        priority: None,
                    },
                ]),
            }),
            contacts: None,
        };

        let merged = merge_config(parsed).expect("merge");
        assert_eq!(merged.loops.policy.default_cadence_days, Some(180));
        assert_eq!(merged.loops.policy.strategy, LoopStrategy::Priority);
        assert!(merged.loops.apply_on_tag_change);
        assert!(merged.loops.schedule_missing);
        assert_eq!(merged.loops.anchor, LoopAnchor::LastInteraction);
        assert!(merged.loops.override_existing);
        assert_eq!(merged.loops.policy.rules.len(), 2);
        assert_eq!(merged.loops.policy.rules[0].tag.as_str(), "friend");
        assert_eq!(merged.loops.policy.rules[0].cadence_days, 90);
        assert_eq!(merged.loops.policy.rules[0].priority, 10);
    }

    #[test]
    fn merge_config_rejects_duplicate_loop_tags() {
        let parsed = ConfigFile {
            due_soon_days: None,
            default_cadence_days: None,
            notifications: None,
            interactions: None,
            loops: Some(LoopConfigFile {
                default_cadence_days: None,
                strategy: None,
                apply_on_tag_change: None,
                schedule_missing: None,
                anchor: None,
                override_existing: None,
                tags: Some(vec![
                    LoopRuleFile {
                        tag: "Friend".to_string(),
                        cadence_days: 90,
                        priority: None,
                    },
                    LoopRuleFile {
                        tag: "friend".to_string(),
                        cadence_days: 30,
                        priority: None,
                    },
                ]),
            }),
            contacts: None,
        };

        let err = merge_config(parsed).unwrap_err();
        assert!(err.to_string().contains("duplicate loops rule tag"));
    }

    #[test]
    fn merge_config_rejects_invalid_loop_tag() {
        let parsed = ConfigFile {
            due_soon_days: None,
            default_cadence_days: None,
            notifications: None,
            interactions: None,
            loops: Some(LoopConfigFile {
                default_cadence_days: None,
                strategy: None,
                apply_on_tag_change: None,
                schedule_missing: None,
                anchor: None,
                override_existing: None,
                tags: Some(vec![LoopRuleFile {
                    tag: "   ".to_string(),
                    cadence_days: 30,
                    priority: None,
                }]),
            }),
            contacts: None,
        };

        let err = merge_config(parsed).unwrap_err();
        assert!(err.to_string().contains("invalid loops rule tag"));
    }

    #[test]
    fn merge_config_rejects_missing_carddav_username() {
        let parsed = ConfigFile {
            due_soon_days: None,
            default_cadence_days: None,
            notifications: None,
            interactions: None,
            loops: None,
            contacts: Some(ContactsFile {
                sources: Some(vec![ContactSourceFile::Carddav {
                    name: "Gmail".to_string(),
                    url: "https://example.test/carddav/".to_string(),
                    username: Some("   ".to_string()),
                    password_env: None,
                    tag: None,
                }]),
                email_accounts: None,
                telegram_accounts: None,
            }),
        };

        let err = merge_config(parsed).unwrap_err();
        assert!(err.to_string().contains("username"));
    }

    #[test]
    fn merge_config_rejects_empty_contact_tag() {
        let parsed = ConfigFile {
            due_soon_days: None,
            default_cadence_days: None,
            notifications: None,
            interactions: None,
            loops: None,
            contacts: Some(ContactsFile {
                sources: Some(vec![ContactSourceFile::Macos {
                    name: "Local".to_string(),
                    group: None,
                    tag: Some("   ".to_string()),
                }]),
                email_accounts: None,
                telegram_accounts: None,
            }),
        };

        let err = merge_config(parsed).unwrap_err();
        assert!(err.to_string().contains("tag"));
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
