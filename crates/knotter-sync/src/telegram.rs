use crate::error::{Result, SyncError};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct TelegramAccount {
    pub name: String,
    pub api_id: i32,
    pub api_hash: String,
    pub phone: String,
    pub session_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct TelegramUser {
    pub id: i64,
    pub username: Option<String>,
    pub phone: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub is_bot: bool,
}

impl TelegramUser {
    pub fn display_name(&self) -> String {
        let mut parts = Vec::new();
        if let Some(first) = &self.first_name {
            if !first.trim().is_empty() {
                parts.push(first.trim().to_string());
            }
        }
        if let Some(last) = &self.last_name {
            if !last.trim().is_empty() {
                parts.push(last.trim().to_string());
            }
        }
        if !parts.is_empty() {
            return parts.join(" ");
        }
        if let Some(username) = &self.username {
            if !username.trim().is_empty() {
                return username.trim().to_string();
            }
        }
        if let Some(phone) = &self.phone {
            if !phone.trim().is_empty() {
                return phone.trim().to_string();
            }
        }
        format!("telegram-user-{}", self.id)
    }
}

#[derive(Debug, Clone)]
pub struct TelegramMessage {
    pub id: i64,
    pub peer_id: i64,
    pub sender_id: Option<i64>,
    pub occurred_at: i64,
    pub outgoing: bool,
    pub text: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TelegramMessageBatch {
    pub messages: Vec<TelegramMessage>,
    pub complete: bool,
}

pub trait TelegramClient {
    fn account_name(&self) -> &str;
    fn list_users(&mut self) -> Result<Vec<TelegramUser>>;
    fn fetch_messages(
        &mut self,
        peer_id: i64,
        since_message_id: i64,
        limit: Option<usize>,
    ) -> Result<TelegramMessageBatch>;
    fn ensure_authorized(&mut self) -> Result<()>;
}

#[cfg(feature = "telegram-sync")]
mod imp {
    use super::{
        Result, SyncError, TelegramAccount, TelegramClient, TelegramMessage, TelegramMessageBatch,
        TelegramUser,
    };
    use grammers_client::types::{Chat, PackedChat};
    use grammers_client::{Client, Config, SignInError};
    use grammers_session::Session;
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
    use tokio::runtime::Runtime;

    pub struct GrammersTelegramClient {
        account_name: String,
        client: Client,
        runtime: Runtime,
        session_path: PathBuf,
        phone: String,
        peers: HashMap<i64, PackedChat>,
    }

    impl TelegramClient for GrammersTelegramClient {
        fn account_name(&self) -> &str {
            &self.account_name
        }

        fn list_users(&mut self) -> Result<Vec<TelegramUser>> {
            let (users, peers) = self.runtime.block_on(async {
                let mut dialogs = self.client.iter_dialogs();
                let mut out = Vec::new();
                let mut peers = HashMap::new();
                while let Some(dialog) = dialogs
                    .next()
                    .await
                    .map_err(|err| SyncError::Command(err.to_string()))?
                {
                    let chat = dialog.chat();
                    let Chat::User(user) = chat else {
                        continue;
                    };
                    if user.is_self() {
                        continue;
                    }
                    let first_name = user.first_name().trim();
                    let first_name = if first_name.is_empty() {
                        None
                    } else {
                        Some(first_name.to_string())
                    };
                    let last_name = user
                        .last_name()
                        .map(|value| value.trim())
                        .filter(|value| !value.is_empty())
                        .map(|value| value.to_string());
                    peers.insert(user.id(), chat.pack());
                    out.push(TelegramUser {
                        id: user.id(),
                        username: user.username().map(|value| value.to_string()),
                        phone: user.phone().map(|value| value.to_string()),
                        first_name,
                        last_name,
                        is_bot: user.is_bot(),
                    });
                }
                Ok::<(Vec<TelegramUser>, HashMap<i64, PackedChat>), SyncError>((out, peers))
            })?;
            self.peers = peers;
            Ok(users)
        }

        fn fetch_messages(
            &mut self,
            peer_id: i64,
            since_message_id: i64,
            limit: Option<usize>,
        ) -> Result<TelegramMessageBatch> {
            let peer = if let Some(peer) = self.peers.get(&peer_id).cloned() {
                peer
            } else {
                let maybe_peer = self.runtime.block_on(async {
                    let mut dialogs = self.client.iter_dialogs();
                    while let Some(dialog) = dialogs
                        .next()
                        .await
                        .map_err(|err| SyncError::Command(err.to_string()))?
                    {
                        let chat = dialog.chat();
                        if let Chat::User(user) = chat {
                            if user.id() == peer_id {
                                return Ok::<Option<PackedChat>, SyncError>(Some(chat.pack()));
                            }
                        }
                    }
                    Ok::<Option<PackedChat>, SyncError>(None)
                })?;
                match maybe_peer {
                    Some(peer) => {
                        self.peers.insert(peer_id, peer);
                        peer
                    }
                    None => {
                        return Err(SyncError::Command(format!(
                            "telegram peer {peer_id} not found"
                        )));
                    }
                }
            };
            self.runtime.block_on(async {
                let mut iter = self.client.iter_messages(peer);
                let fetch_limit = limit.map(|value| value.saturating_add(1));
                if let Some(limit) = fetch_limit {
                    iter = iter.limit(limit);
                }
                let mut out = Vec::new();
                let mut reached_cursor = false;
                while let Some(message) = iter
                    .next()
                    .await
                    .map_err(|err| SyncError::Command(err.to_string()))?
                {
                    if since_message_id > 0 && message.id() as i64 <= since_message_id {
                        reached_cursor = true;
                        break;
                    }
                    let occurred_at = message.date().timestamp();
                    let sender_id = message.sender().and_then(|sender| match sender {
                        Chat::User(user) => Some(user.id()),
                        _ => None,
                    });
                    let text = {
                        let value = message.text().trim();
                        if value.is_empty() {
                            None
                        } else {
                            Some(value.to_string())
                        }
                    };
                    out.push(TelegramMessage {
                        id: message.id() as i64,
                        peer_id,
                        sender_id,
                        occurred_at,
                        outgoing: message.outgoing(),
                        text,
                    });
                }
                let over_limit = limit.is_some_and(|limit| out.len() > limit);
                if let Some(limit) = limit {
                    if out.len() > limit {
                        out.truncate(limit);
                    }
                }
                Ok(TelegramMessageBatch {
                    messages: out,
                    complete: reached_cursor || !over_limit,
                })
            })
        }

        fn ensure_authorized(&mut self) -> Result<()> {
            self.runtime.block_on(async {
                if self
                    .client
                    .is_authorized()
                    .await
                    .map_err(|err| SyncError::Command(err.to_string()))?
                {
                    return Ok(());
                }

                let token = self
                    .client
                    .request_login_code(&self.phone)
                    .await
                    .map_err(|err| SyncError::Command(err.to_string()))?;
                let code = read_env_or_prompt("KNOTTER_TELEGRAM_CODE", "Telegram login code")?;
                match self.client.sign_in(&token, &code).await {
                    Ok(_) => {}
                    Err(SignInError::PasswordRequired(password_token)) => {
                        let password = read_env_or_prompt(
                            "KNOTTER_TELEGRAM_PASSWORD",
                            "Telegram 2FA password",
                        )?;
                        self.client
                            .check_password(password_token, password.as_bytes())
                            .await
                            .map_err(|err| SyncError::Command(err.to_string()))?;
                    }
                    Err(err) => {
                        return Err(SyncError::Command(err.to_string()));
                    }
                }

                if let Err(err) = self.client.session().save_to_file(&self.session_path) {
                    return Err(SyncError::Io(err));
                }
                Ok(())
            })
        }
    }

    fn load_session(path: &std::path::Path) -> Result<Session> {
        if path.exists() {
            Session::load_file(path).map_err(|err| SyncError::Command(err.to_string()))
        } else {
            Ok(Session::new())
        }
    }

    fn ensure_parent_dir(path: &std::path::Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(SyncError::Io)?;
            }
        }
        Ok(())
    }

    fn read_env_or_prompt(env_key: &str, prompt: &str) -> Result<String> {
        if let Ok(value) = std::env::var(env_key) {
            let trimmed = value.trim().to_string();
            if !trimmed.is_empty() {
                return Ok(trimmed);
            }
        }
        eprint!("{prompt}: ");
        let mut buffer = String::new();
        std::io::stdin()
            .read_line(&mut buffer)
            .map_err(SyncError::Io)?;
        let trimmed = buffer.trim().to_string();
        if trimmed.is_empty() {
            return Err(SyncError::Command(format!(
                "{env_key} is not set and no input was provided"
            )));
        }
        Ok(trimmed)
    }

    pub fn connect(account: TelegramAccount) -> Result<Box<dyn TelegramClient>> {
        ensure_parent_dir(account.session_path.as_path())?;
        let session = load_session(account.session_path.as_path())?;
        let runtime = Runtime::new().map_err(|err| SyncError::Command(err.to_string()))?;
        let client = runtime.block_on(async {
            Client::connect(Config {
                session,
                api_id: account.api_id,
                api_hash: account.api_hash.clone(),
                params: Default::default(),
            })
            .await
            .map_err(|err| SyncError::Command(err.to_string()))
        })?;
        Ok(Box::new(GrammersTelegramClient {
            account_name: account.name,
            client,
            runtime,
            session_path: account.session_path,
            phone: account.phone,
            peers: HashMap::new(),
        }))
    }
}

#[cfg(feature = "telegram-sync")]
pub use imp::connect;

#[cfg(not(feature = "telegram-sync"))]
pub fn connect(_account: TelegramAccount) -> Result<Box<dyn TelegramClient>> {
    Err(SyncError::Unavailable(
        "telegram sync requires the telegram-sync feature".to_string(),
    ))
}
