# Configuration Examples

This page provides small, setup-focused config snippets. The full reference example
lives in `README.md` and `docs/ARCHITECTURE.md`.

Config file locations:

- `$XDG_CONFIG_HOME/knotter/config.toml`
- fallback: `~/.config/knotter/config.toml`

On Unix, the config file must be user-readable only (e.g., `chmod 600`).

## Minimal defaults

You can omit the file entirely. If you want just a couple defaults:

```toml
due_soon_days = 7
default_cadence_days = 30
```

## Desktop notifications

Requires the `desktop-notify` feature.

```toml
[notifications]
enabled = true
backend = "desktop"
```

## Email notifications (SMTP)

Requires the `email-notify` feature. Provide the password via env var (not in
config).

```toml
[notifications]
enabled = true
backend = "email"

[notifications.email]
from = "Knotter <knotter@example.com>"
to = ["you@example.com"]
subject_prefix = "knotter reminders"
smtp_host = "smtp.example.com"
smtp_port = 587
username = "user@example.com"
password_env = "KNOTTER_SMTP_PASSWORD"
tls = "start-tls"
```

## Random contacts fallback in notifications

If reminders are otherwise empty, you can include N random active contacts in the
notification:

```toml
[notifications]
random_contacts_if_no_reminders = 10
```

Max: 100.

Legacy: `random_contacts_if_no_dates_today` is still accepted (renamed to better match behavior).

## Auto-reschedule on interactions

```toml
[interactions]
auto_reschedule = true
```

## Tag-based loops

```toml
[loops]
default_cadence_days = 180
strategy = "shortest"
schedule_missing = true
anchor = "created-at"
apply_on_tag_change = false
override_existing = false

[[loops.tags]]
tag = "friend"
cadence_days = 90

[[loops.tags]]
tag = "family"
cadence_days = 30
priority = 10
```

## CardDAV contact import

Requires the `dav-sync` feature.

```toml
[contacts]
[[contacts.sources]]
name = "gmail"
type = "carddav"
url = "https://example.test/carddav/addressbook/"
username = "user@example.com"
password_env = "KNOTTER_GMAIL_PASSWORD"
tag = "gmail"
```

## macOS Contacts import

```toml
[contacts]
[[contacts.sources]]
name = "macos"
type = "macos"
# Optional: import only a named Contacts group (must already exist).
# group = "Friends"
tag = "personal"
```

## Email header sync (IMAP)

Requires the `email-sync` feature.

```toml
[contacts]
[[contacts.email_accounts]]
name = "gmail"
host = "imap.gmail.com"
port = 993
username = "user@gmail.com"
password_env = "KNOTTER_GMAIL_PASSWORD"
mailboxes = ["INBOX", "[Gmail]/Sent Mail"]
identities = ["user@gmail.com"]
merge_policy = "name-or-email"
tls = "tls"
tag = "gmail"
```

## Telegram sync

Included in default builds. For a no-sync build from source, use
`--no-default-features`. To enable Telegram in a minimal build, add
`--features telegram-sync`. On first sync, set `KNOTTER_TELEGRAM_CODE`
(and `KNOTTER_TELEGRAM_PASSWORD` if you use 2FA) for non-interactive use.

```toml
[contacts]
[[contacts.telegram_accounts]]
name = "primary"
api_id = 123456
api_hash_env = "KNOTTER_TELEGRAM_API_HASH"
phone = "+15551234567"
merge_policy = "name-or-username"
allowlist_user_ids = [123456789]
snippet_len = 160
tag = "telegram"
```

## Combined setups

If you want a single config that covers all sections at once, use the full
reference example in `README.md` or `docs/ARCHITECTURE.md`.
