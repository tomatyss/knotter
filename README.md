# knotter

knotter is a terminal-first personal CRM and friendship tracker. It is an offline-first Rust app with a CLI and TUI, backed by a portable SQLite database, with vCard/iCalendar import/export.

Status: CLI MVP and TUI MVP are available from this repo. Tag-driven releases publish macOS/Linux tarballs and Linux .deb packages.

## Install (macOS via Homebrew)

This repo acts as its own tap. Install from this repo with:

```
brew tap tomatyss/knotter https://github.com/tomatyss/knotter
brew install tomatyss/knotter/knotter
```

The Homebrew install should provide both `knotter` (CLI) and `knotter-tui` (TUI).

If you already tapped, run `brew update` to pull the latest formula changes.

## Install (Linux / Debian / Ubuntu)

Tag-driven releases publish `.deb` artifacts (x86_64). For generic Linux installs
(including musl builds), see `docs/packaging.md`.

## Quickstart (dev)

Build and run from source:

```
# build all crates
cargo build

# run the CLI
cargo run -p knotter-cli -- --help

# run the TUI
cargo run -p knotter-tui -- --help
```

Common dev commands:

- Build: `cargo build`
- Test: `cargo test`
- Format: `cargo fmt`
- Lint: `cargo clippy --all-targets --all-features -D warnings`
- Precommit checks: `just precommit`

## CLI basics

Create a contact and list it:

```
knotter add-contact --name "Ada Lovelace" --email ada@example.com --tag friend
knotter list
knotter list --filter "#friend due:soon"
```

Schedule a touchpoint and see reminders:

```
knotter schedule <id> --at "2026-02-01" --time "09:00"
knotter remind --soon-days 14
```

Add an interaction:

```
knotter add-note <id> --kind call --note "Caught up after the conference"
```

Add important dates:

```
knotter date add <id> --kind birthday --on 1990-02-14
knotter date add <id> --kind custom --label "wife birthday" --on 02-14
knotter date ls <id>
```

Record a touch and reschedule in one step:

```
knotter touch <id> --kind call --note "Caught up after the conference" --reschedule
```

Add an interaction and reschedule the next touchpoint:

```
knotter add-note <id> --kind call --note "Caught up after the conference" --reschedule
```

Archive or unarchive a contact:

```
knotter archive-contact <id>
knotter unarchive-contact <id>
```

Apply keep-in-touch loops (tag-based cadences):

```
knotter loops apply
```

Apply loops immediately after tagging:

```
knotter tag add <id> friend --apply-loop
```

JSON output is available for automation (see `docs/cli-output.md`).

## Shell completions

Generate and install shell completions:

```
knotter completions bash > ~/.local/share/bash-completion/completions/knotter
```

See `docs/completions.md` for the full list of supported shells and install steps.

## TUI basics

Launch:

```
knotter tui

# or run directly
knotter-tui
```

Common keys (full list in `docs/KEYBINDINGS.md`):

- `Enter` open detail
- `/` edit filter
- `a` add contact
- `n` add note
- `t` edit tags
- `s` schedule
- `q` quit

## Import/export

- Import vCard: `knotter import vcf <file>`
- Import macOS Contacts: `knotter import macos`
- Import CardDAV (Gmail/iCloud/etc.): `knotter import carddav --url <addressbook-url> --username <user> --password-env <ENV>`
- Import email accounts (IMAP): `knotter import email --account <name> [--limit N] [--retry-skipped] [--force-uidvalidity-resync]`
- Import Telegram (1:1 snippets): `knotter import telegram --account <name> [--limit N] [--contacts-only|--messages-only]`
- Sync all configured sources + email + telegram, then apply loops and remind: `knotter sync` (use `--no-telegram` to skip Telegram)
- Export vCard: `knotter export vcf --out <file>`
- Export touchpoints (ICS): `knotter export ics --out <file>`
- Export full JSON snapshot: `knotter export json --out <file>` (add `--exclude-archived` to omit archived)

CardDAV and email import are enabled in default builds (v0.2.1+). Telegram sync is opt-in. To slim down a build, use `--no-default-features` and re-enable sync with `--features dav-sync,email-sync,telegram-sync`. See `docs/import-export.md` for mapping details.

## Reminders

Use your system scheduler to run reminders (cron/systemd examples in `docs/scheduling.md`):

```
/path/to/knotter remind --notify
```

Email notifications require building with the `email-notify` feature and configuring
SMTP settings (see below).

## Configuration

knotter reads an optional TOML config file from:

- `$XDG_CONFIG_HOME/knotter/config.toml`
- Fallback: `~/.config/knotter/config.toml`

Use `--config /path/to/config.toml` to override the location.

Full example (all sections + optional fields):

```toml
due_soon_days = 7
default_cadence_days = 30

[notifications]
enabled = false
backend = "stdout" # stdout | desktop | email

[notifications.email]
from = "Knotter <knotter@example.com>"
to = ["you@example.com"]
subject_prefix = "knotter reminders"
smtp_host = "smtp.example.com"
smtp_port = 587
username = "user@example.com"
password_env = "KNOTTER_SMTP_PASSWORD"
tls = "start-tls" # start-tls | tls | none
timeout_seconds = 20

[interactions]
auto_reschedule = false

[loops]
default_cadence_days = 180
strategy = "shortest" # shortest | priority
schedule_missing = true
anchor = "created-at" # now | created-at | last-interaction
apply_on_tag_change = false
override_existing = false

[[loops.tags]]
tag = "friend"
cadence_days = 90

[[loops.tags]]
tag = "family"
cadence_days = 30
priority = 10

[contacts]
[[contacts.sources]]
name = "gmail"
type = "carddav"
url = "https://example.test/carddav/addressbook/"
username = "user@example.com"
password_env = "KNOTTER_GMAIL_PASSWORD"
tag = "gmail"

[[contacts.sources]]
name = "macos"
type = "macos"
group = "Friends"
tag = "personal"

[[contacts.email_accounts]]
name = "gmail"
host = "imap.gmail.com"
port = 993
username = "user@gmail.com"
password_env = "KNOTTER_GMAIL_PASSWORD"
mailboxes = ["INBOX", "[Gmail]/Sent Mail"]
identities = ["user@gmail.com"]
merge_policy = "name-or-email" # name-or-email | email-only
tls = "tls" # tls | start-tls | none
tag = "gmail"

[[contacts.telegram_accounts]]
name = "primary"
api_id = 123456
api_hash_env = "KNOTTER_TELEGRAM_API_HASH"
phone = "+15551234567"
session_path = "/home/user/.local/share/knotter/telegram/primary.session"
merge_policy = "name-or-username" # name-or-username | username-only
allowlist_user_ids = [123456789]
snippet_len = 160
tag = "telegram"
```

Notes:

- On Unix, the config file must be user-readable only (`chmod 600`).
- When `notifications.enabled = true`, `notifications.backend = "email"` requires
  the `email-notify` feature and a `[notifications.email]` block.
- When `notifications.enabled = true`, `notifications.backend = "desktop"` requires
  the `desktop-notify` feature.
- CardDAV sources require `url` and `username`; `password_env` can be omitted if
  you pass `--password-env` or `--password-stdin` at runtime.
- Email accounts default to `port = 993`, `mailboxes = ["INBOX"]`, and
  `identities = [username]` when the username is an email address.
- Telegram accounts require `api_id`, `api_hash_env`, and `phone`. `session_path`
  is optional; by default sessions are stored under
  `$XDG_DATA_HOME/knotter/telegram/<name>.session` (or `~/.local/share/knotter/telegram/<name>.session`).
  `snippet_len` defaults to 160; `allowlist_user_ids` limits sync to specific Telegram user ids.
- Telegram sync prompts for a login code on first use. Set `KNOTTER_TELEGRAM_CODE` and
  (if you have 2FA) `KNOTTER_TELEGRAM_PASSWORD` to run non-interactively.
- Telegram account names must be a single path segment (no slashes), since they are used
  to construct default session filenames.

## Data location

By default, knotter stores data under the XDG data directory:

- `$XDG_DATA_HOME/knotter/knotter.sqlite3`
- Fallback: `~/.local/share/knotter/knotter.sqlite3`

You can override the database path with `--db-path`.

## Backup

Create a consistent SQLite snapshot (safe with WAL):

```
knotter backup
```

Or write to an explicit path:

```
knotter backup --out /path/to/backup.sqlite3
```

## More docs

- `docs/ARCHITECTURE.md` for system design and filtering semantics
- `docs/DB_SCHEMA.md` for the authoritative schema
- `docs/cli-output.md` for stable JSON output
- `docs/KEYBINDINGS.md` for TUI keys
- `docs/import-export.md` for vCard/ICS/JSON behavior
- `docs/scheduling.md` for reminder scheduling
- `docs/packaging.md` for package build notes
