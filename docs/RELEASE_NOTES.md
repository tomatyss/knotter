# Release Notes

## v0.5.0 (2026-02-09)

### Reminders
- `knotter remind` now falls back to a random contact when the reminders list is empty.

### vCard Import/Merge
- Improved vCard contact-source matching (harden/normalize external ID matching).
- Track vCard UID as a contact source mapping to improve dedupe/round-trips.
- Fix safe auto-merge classification for a vCard phone-name edge case.

### Maintenance
- Dependency updates: clap, notify-rust, anyhow, bytes, time.
- Release housekeeping: bump crate versions to 0.5.0.

## v0.4.6 (2026-02-03)

- Release housekeeping: bump crate versions to 0.4.6.

## v0.4.5 (2026-02-02)

- Release housekeeping: bump crate versions to 0.4.5.

## v0.4.4 (2026-02-02)

### Maintenance
- Release housekeeping: bump crate versions to 0.4.4.

## v0.4.3 (2026-02-02)

### Maintenance
- Release housekeeping: bump crate versions to 0.4.3.

## v0.4.2 (2026-02-02)

### Maintenance
- Release housekeeping: bump crate versions to 0.4.2.

## v0.4.1 (2026-02-01)

### Maintenance
- Release housekeeping: bump crate versions to 0.4.1.

## v0.4.0 (2026-02-01)

### Telegram Sync (1:1)
- Added Telegram sync backend (feature-gated) with CLI import/sync wiring.
- Improved Telegram matching (handle/username), messages-only behavior, and limit safety.
- Hardened Telegram account naming and username normalization.

### Contact Dates & vCard
- Added per-contact dates with CLI commands and storage schema updates.
- vCard import/export now handles labeled birthdays and preserves yearful entries.
- Fixed BDAY merge selection and added custom label DB constraints.

### Merge & TUI
- Added a TUI merge picker workflow for resolving merge candidates.

## v0.3.0 (2026-01-27)

### Contact Dates
- Added per-contact dates (birthday/name day/custom) with CLI commands and JSON export.
- `knotter remind` now includes a `dates_today` bucket.
- vCard import/export supports `BDAY` and `X-KNOTTER-DATE` fields.

### Merge Candidates
- New merge candidate workflow with storage, CLI `knotter merge` commands, and TUI merge list/actions.
- Import/sync now stages ambiguous matches (duplicate emails or name collisions) as merge candidates instead of skipping.

### Sync & Import
- `knotter sync` now runs best-effort across sources/accounts, reporting warnings while continuing.
- Email import can match archived staged contacts when an open merge candidate exists, preserving touches.

## v0.2.2 (2026-01-26)

### Packaging & Defaults
- Fixes `Cargo.lock` entries so default-feature builds resolve cleanly again.

## v0.2.1 (2026-01-26)

### Packaging & Defaults
- Default builds now include `dav-sync` and `email-sync`, so CardDAV and IMAP imports work out of the box.
- Use `--no-default-features` to build a minimal binary and re-enable sync features explicitly.

## v0.2.0 (2026-01-25)

### Highlights
- Email import/sync via IMAP (`knotter import email`) with Message-ID dedupe, retry/force flags, and JSON reporting.
- Interaction rescheduling support (`knotter touch` and `add-note --reschedule`) plus config-driven auto-reschedule.
- Multi-arch Linux release artifacts (gnu + musl) produced by the release workflow.

### Import/Export & Sync
- Added email account sync profiles in config (`contacts.email_accounts`) and the `email-sync` feature gate.
- Email sync records header-only touches and creates/merges contacts based on sender matching.
- New `--retry-skipped` and `--force-uidvalidity-resync` controls for IMAP edge cases.
- vCard import now maps all EMAIL values to contact emails (primary + secondary).

### CLI/TUI Behavior
- `knotter touch` records an interaction and optionally reschedules the next touchpoint.
- `add-note` supports `--reschedule`; `interactions.auto_reschedule` config enables default rescheduling.
- Schedule inputs now validate “now or later”; date-only values store end-of-day timestamps.
- TUI adds `Ctrl+N` to set date/time fields to “now”.

### Data Model & Output
- Contact JSON output now includes an `emails` array alongside the primary `email`.
- Email sync adds new storage tables/migrations for message tracking and dedupe.

### Packaging & Maintenance
- License updated to Apache-2.0.
- Dependency updates: `toml` 0.9, `uuid` 1.20, `thiserror` 2, `dirs` 6.
