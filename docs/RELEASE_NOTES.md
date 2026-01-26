# Release Notes

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
