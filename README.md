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
- Export vCard: `knotter export vcf --out <file>`
- Export touchpoints (ICS): `knotter export ics --out <file>`
- Export full JSON snapshot: `knotter export json --out <file>` (add `--exclude-archived` to omit archived)

CardDAV import requires building with the `dav-sync` feature. See `docs/import-export.md` for mapping details.

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

Example:

```
due_soon_days = 7
default_cadence_days = 30

[notifications]
enabled = false
backend = "stdout" # or "desktop" or "email"

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
# Auto-reschedule after adding interactions (requires cadence_days on the contact)
auto_reschedule = false

[loops]
# Optional default when no tag matches (e.g., ~6 months)
default_cadence_days = 180
strategy = "shortest" # or "priority"
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
```

On Unix, the config file must be user-readable only (`chmod 600`).

Contact source profiles can also live in config (see `docs/ARCHITECTURE.md`):

```
[contacts]
[[contacts.sources]]
name = "gmail"
type = "carddav"
url = "https://example.test/carddav/addressbook/"
username = "user@example.com"
password_env = "KNOTTER_GMAIL_PASSWORD"
tag = "gmail"
```

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
