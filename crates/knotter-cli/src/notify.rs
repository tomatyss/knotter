use anyhow::Result;

pub trait Notifier {
    fn send(&self, title: &str, body: &str) -> Result<()>;
}

pub struct StdoutNotifier;

impl Notifier for StdoutNotifier {
    fn send(&self, title: &str, body: &str) -> Result<()> {
        println!("{title}: {body}");
        Ok(())
    }
}

#[cfg(feature = "email-notify")]
pub struct EmailNotifier {
    from: lettre::message::Mailbox,
    to: Vec<lettre::message::Mailbox>,
    transport: lettre::SmtpTransport,
}

#[cfg(feature = "email-notify")]
impl EmailNotifier {
    pub fn new(config: &knotter_config::NotificationsEmailConfig) -> Result<Self> {
        use crate::error::invalid_input;
        use lettre::transport::smtp::authentication::Credentials;
        use std::env;
        use std::time::Duration;

        let from = config
            .from
            .parse()
            .map_err(|_| invalid_input("notifications.email.from must be a valid email address"))?;
        let mut to = Vec::with_capacity(config.to.len());
        for raw in &config.to {
            let mailbox = raw.parse().map_err(|_| {
                invalid_input("notifications.email.to must contain valid email addresses")
            })?;
            to.push(mailbox);
        }

        let mut builder = match config.tls {
            knotter_config::EmailTls::Tls => lettre::SmtpTransport::relay(&config.smtp_host)
                .map_err(|_| invalid_input("invalid notifications.email.smtp_host"))?,
            knotter_config::EmailTls::StartTls => {
                lettre::SmtpTransport::starttls_relay(&config.smtp_host)
                    .map_err(|_| invalid_input("invalid notifications.email.smtp_host"))?
            }
            knotter_config::EmailTls::None => {
                lettre::SmtpTransport::builder_dangerous(&config.smtp_host)
            }
        };

        if let Some(port) = config.smtp_port {
            builder = builder.port(port);
        }

        if let Some(seconds) = config.timeout_seconds {
            builder = builder.timeout(Some(Duration::from_secs(seconds)));
        }

        if let (Some(username), Some(password_env)) =
            (config.username.as_deref(), config.password_env.as_deref())
        {
            let password = env::var(password_env)
                .map_err(|_| invalid_input(format!("missing env var {password_env}")))?;
            let password = password.trim();
            if password.is_empty() {
                return Err(invalid_input(format!("env var {password_env} is empty")));
            }
            let credentials = Credentials::new(username.to_string(), password.to_string());
            builder = builder.credentials(credentials);
        }

        Ok(Self {
            from,
            to,
            transport: builder.build(),
        })
    }
}

#[cfg(feature = "email-notify")]
impl Notifier for EmailNotifier {
    fn send(&self, title: &str, body: &str) -> Result<()> {
        use lettre::message::header::ContentType;
        use lettre::Message;
        use lettre::Transport as _;

        let mut builder = Message::builder()
            .from(self.from.clone())
            .subject(title)
            .header(ContentType::TEXT_PLAIN);
        for mailbox in &self.to {
            builder = builder.to(mailbox.clone());
        }

        let message = builder.body(body.to_string())?;
        self.transport.send(&message)?;
        Ok(())
    }
}

#[cfg(feature = "desktop-notify")]
pub struct DesktopNotifier;

#[cfg(feature = "desktop-notify")]
impl Notifier for DesktopNotifier {
    fn send(&self, title: &str, body: &str) -> Result<()> {
        notify_rust::Notification::new()
            .summary(title)
            .body(body)
            .show()?;
        Ok(())
    }
}

#[cfg(all(test, feature = "email-notify"))]
mod tests {
    use super::EmailNotifier;
    use knotter_config::{EmailTls, NotificationsEmailConfig};

    fn base_config() -> NotificationsEmailConfig {
        NotificationsEmailConfig {
            from: "Knotter <knotter@example.com>".to_string(),
            to: vec!["Ada Lovelace <ada@example.com>".to_string()],
            subject_prefix: "knotter reminders".to_string(),
            smtp_host: "smtp.example.com".to_string(),
            smtp_port: Some(587),
            username: None,
            password_env: None,
            tls: EmailTls::StartTls,
            timeout_seconds: Some(5),
        }
    }

    #[test]
    fn email_notifier_new_fails_when_password_env_missing() {
        let mut config = base_config();
        config.username = Some("user@example.com".to_string());
        config.password_env = Some("KNOTTER_TEST_SMTP_PASSWORD".to_string());
        std::env::remove_var("KNOTTER_TEST_SMTP_PASSWORD");
        match EmailNotifier::new(&config) {
            Ok(_) => panic!("expected error"),
            Err(err) => {
                assert!(err.to_string().contains("missing env var"));
            }
        }
    }

    #[test]
    fn email_notifier_new_fails_when_password_env_empty() {
        let mut config = base_config();
        config.username = Some("user@example.com".to_string());
        config.password_env = Some("KNOTTER_TEST_SMTP_PASSWORD_EMPTY".to_string());
        std::env::set_var("KNOTTER_TEST_SMTP_PASSWORD_EMPTY", "   ");
        match EmailNotifier::new(&config) {
            Ok(_) => panic!("expected error"),
            Err(err) => {
                assert!(err
                    .to_string()
                    .contains("env var KNOTTER_TEST_SMTP_PASSWORD_EMPTY is empty"));
            }
        }
        std::env::remove_var("KNOTTER_TEST_SMTP_PASSWORD_EMPTY");
    }

    #[test]
    fn email_notifier_new_supports_tls_modes() {
        let mut config = base_config();
        for tls in [EmailTls::None, EmailTls::StartTls, EmailTls::Tls] {
            config.tls = tls;
            let result = EmailNotifier::new(&config);
            assert!(result.is_ok());
        }
    }
}
