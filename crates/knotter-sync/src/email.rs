#[derive(Debug, Clone)]
pub struct EmailAccount {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub tls: EmailTls,
    pub mailboxes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmailTls {
    Tls,
    StartTls,
    None,
}

#[derive(Debug, Clone)]
pub struct EmailAddress {
    pub name: Option<String>,
    pub email: String,
}

#[derive(Debug, Clone)]
pub struct EmailHeader {
    pub mailbox: String,
    pub uid: u32,
    pub message_id: Option<String>,
    pub occurred_at: i64,
    pub from: Vec<EmailAddress>,
    pub to: Vec<EmailAddress>,
    pub subject: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MailboxSyncResult {
    pub mailbox: String,
    pub uidvalidity: Option<i64>,
    pub last_uid: i64,
    pub headers: Vec<EmailHeader>,
}

#[cfg(feature = "email-sync")]
mod imp {
    use super::{EmailAccount, EmailAddress, EmailHeader, EmailTls, MailboxSyncResult};
    use crate::error::{Result, SyncError};
    use mailparse::{addrparse, dateparse, MailHeaderMap};

    pub fn fetch_mailbox_headers(
        account: &EmailAccount,
        mailbox: &str,
        last_uid: i64,
        limit: Option<usize>,
    ) -> Result<MailboxSyncResult> {
        let mut session = connect(account)?;
        let mailbox_info = session
            .select(mailbox)
            .map_err(|err| SyncError::Command(err.to_string()))?;
        let uidvalidity = mailbox_info.uid_validity.map(|value| value as i64);
        let search = format!("UID {}:*", last_uid.saturating_add(1));
        let uids = session
            .uid_search(search)
            .map_err(|err| SyncError::Command(err.to_string()))?;
        let mut uids: Vec<u32> = uids.into_iter().collect();
        if !uids.is_empty() {
            uids.sort_unstable();
            if let Some(limit) = limit {
                if uids.len() > limit {
                    uids.truncate(limit);
                }
            }
        }
        let mut headers = Vec::new();
        let mut max_uid = last_uid;

        if !uids.is_empty() {
            let sequence = uids
                .iter()
                .map(|uid| uid.to_string())
                .collect::<Vec<_>>()
                .join(",");
            let fetches = session
                .uid_fetch(
                    sequence,
                    "BODY.PEEK[HEADER.FIELDS (DATE FROM TO CC SUBJECT MESSAGE-ID)]",
                )
                .map_err(|err| SyncError::Command(err.to_string()))?;
            for fetch in fetches.iter() {
                let uid = fetch.uid.unwrap_or_default();
                max_uid = max_uid.max(uid as i64);
                let Some(header_bytes) = fetch.header() else {
                    continue;
                };
                let (parsed_headers, _) = mailparse::parse_headers(header_bytes)
                    .map_err(|err| SyncError::Parse(format!("mail header parse: {err}")))?;

                let message_id = normalize_message_id(parsed_headers.get_first_value("Message-ID"));
                let subject = parsed_headers.get_first_value("Subject");
                let from = parse_addresses(parsed_headers.get_first_value("From").as_deref());
                let mut to = parse_addresses(parsed_headers.get_first_value("To").as_deref());
                let cc = parse_addresses(parsed_headers.get_first_value("Cc").as_deref());
                if !cc.is_empty() {
                    to.extend(cc);
                }
                let occurred_at = parsed_headers
                    .get_first_value("Date")
                    .as_deref()
                    .and_then(|value| dateparse(value).ok())
                    .unwrap_or_else(|| chrono::Utc::now().timestamp());

                headers.push(EmailHeader {
                    mailbox: mailbox.to_string(),
                    uid,
                    message_id,
                    occurred_at,
                    from,
                    to,
                    subject,
                });
            }
        }

        session
            .logout()
            .map_err(|err| SyncError::Command(err.to_string()))?;

        Ok(MailboxSyncResult {
            mailbox: mailbox.to_string(),
            uidvalidity,
            last_uid: max_uid,
            headers,
        })
    }

    fn connect(account: &EmailAccount) -> Result<imap::Session<imap::Connection>> {
        let mode = match account.tls {
            EmailTls::Tls => imap::ConnectionMode::Tls,
            EmailTls::StartTls => imap::ConnectionMode::StartTls,
            EmailTls::None => imap::ConnectionMode::Plaintext,
        };
        let client = imap::ClientBuilder::new(account.host.as_str(), account.port)
            .mode(mode)
            .connect()
            .map_err(|err| SyncError::Command(err.to_string()))?;
        let session = client
            .login(&account.username, &account.password)
            .map_err(|err| SyncError::Command(err.0.to_string()))?;
        Ok(session)
    }

    fn parse_addresses(value: Option<&str>) -> Vec<EmailAddress> {
        let Some(raw) = value else {
            return Vec::new();
        };
        let Ok(list) = addrparse(raw) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for addr in list.iter() {
            match addr {
                mailparse::MailAddr::Single(single) => {
                    push_single(&mut out, single);
                }
                mailparse::MailAddr::Group(group) => {
                    for member in group.addrs.iter() {
                        push_single(&mut out, member);
                    }
                }
            }
        }
        out
    }

    fn push_single(out: &mut Vec<EmailAddress>, single: &mailparse::SingleInfo) {
        out.push(EmailAddress {
            name: single
                .display_name
                .as_ref()
                .map(|name| name.trim().to_string()),
            email: single.addr.clone(),
        });
    }

    fn normalize_message_id(value: Option<String>) -> Option<String> {
        value
            .map(|value| {
                value
                    .trim()
                    .trim_matches(&['<', '>'][..])
                    .trim()
                    .to_string()
            })
            .filter(|value| !value.is_empty())
            .map(|value| value.to_ascii_lowercase())
    }
}

#[cfg(feature = "email-sync")]
pub use imp::fetch_mailbox_headers;

#[cfg(not(feature = "email-sync"))]
pub fn fetch_mailbox_headers(
    _account: &EmailAccount,
    _mailbox: &str,
    _last_uid: i64,
    _limit: Option<usize>,
) -> crate::error::Result<MailboxSyncResult> {
    Err(crate::error::SyncError::Unavailable(
        "email sync requires the email-sync feature".to_string(),
    ))
}
