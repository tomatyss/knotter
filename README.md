# knotter

knotter is a terminal-first personal CRM and friendship tracker. It is an offline-first Rust app with a CLI and TUI, backed by a portable SQLite database, plus vCard/iCalendar import/export.

Status: CLI MVP in progress (import/export done; TUI ongoing).

## Quickstart (dev)

- Build: `cargo build`
- Test: `cargo test`
- Format: `cargo fmt`
- Lint: `cargo clippy --all-targets --all-features -D warnings`

## Import/export (CLI)

- Import vCard: `knotter import vcf <file>`
- Export vCard: `knotter export vcf --out <file>`
- Export touchpoints (ICS): `knotter export ics --out <file>`

## Data location

By default, knotter stores data under the XDG data directory:

- `$XDG_DATA_HOME/knotter/knotter.sqlite3`
- Fallback: `~/.local/share/knotter/knotter.sqlite3`
