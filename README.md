# knotter

knotter is a terminal-first personal CRM and friendship tracker. It is an offline-first Rust app with a CLI and TUI, backed by a portable SQLite database, with vCard/iCalendar import/export.

Status: CLI MVP and TUI MVP are available from this repo (no packaged release yet).

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

## CLI basics

Create a contact and list it:

```
knotter add-contact --name "Ada Lovelace" --email ada@example.com
knotter list
knotter list --filter "#friends due:soon"
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

JSON output is available for automation (see `docs/cli-output.md`).

## TUI basics

Launch:

```
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
- Export vCard: `knotter export vcf --out <file>`
- Export touchpoints (ICS): `knotter export ics --out <file>`

See `docs/import-export.md` for mapping details.

## Reminders

Use your system scheduler to run reminders (cron/systemd examples in `docs/scheduling.md`):

```
/path/to/knotter remind --notify
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
- `docs/import-export.md` for vCard/ICS behavior
- `docs/scheduling.md` for reminder scheduling
