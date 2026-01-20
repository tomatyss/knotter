# TODO: knotter (personal CRM / friendship-tracking TUI in Rust)

This is the **refined, updated, very detailed** implementation plan for **knotter**.
It aligns with `docs/ARCHITECTURE.md` (layering, DB schema, filter language, and portability rules).

---

## 0) Project definition (pin scope so it ships)

### MVP outcome
knotter is “MVP done” when all are true:

- CLI can:
  - create/edit/show/list contacts
  - tag contacts and filter by tags
  - add interactions/notes
  - set/clear next touchpoint + optional cadence
  - compute reminders (overdue / today / soon)
  - import/export contacts as `.vcf`
  - export touchpoints as `.ics`
- TUI can:
  - list + filter contacts
  - view contact detail (tags + recent interactions)
  - add interactions
  - edit tags
  - schedule next touchpoint
- Data is stored under XDG data dir (portable) in a single SQLite DB file.
- Reminders work on any Unix machine:
  - at minimum, printing to stdout (`knotter remind`)
  - optional desktop notification behind a feature flag

### Non-goals (MVP)
- Background daemon required for functionality
- Full CardDAV/CalDAV sync (post-MVP)
- ICS import (post-MVP)
- Complex recurrence rules (keep “cadence_days + next_touchpoint_at” only)
- AI features

---

## 1) Milestone A — Workspace scaffolding + engineering baseline

### A1. Create workspace layout
- [x] Create Cargo workspace root
- [x] Create crates under `crates/`:
  - [x] `knotter-core`
  - [x] `knotter-store`
  - [x] `knotter-sync`
  - [x] `knotter-cli`
  - [x] `knotter-tui`
  - [ ] (optional) `knotter` umbrella binary crate
- [x] Ensure dependency direction matches architecture:
  - [x] `knotter-core` has no DB, no IO, no terminal deps
  - [x] `knotter-store` depends on core
  - [x] `knotter-sync` depends on core (optionally store for upserts)
  - [x] CLI/TUI depend on core/store/sync, never the reverse

### A2. Decide baseline crates and features (commit early)
- [x] Pick datetime crate strategy (and stick to it):
  - [x] timestamps stored as `i64` unix seconds UTC in DB
  - [x] conversions at edges
- [x] Pick DB crate strategy:
  - [x] recommended MVP: `rusqlite` with bundled sqlite option enabled
- [x] Pick CLI argument parsing crate (recommended: `clap`)
- [x] Pick error crates:
  - [x] `thiserror` in core/store/sync
  - [x] `anyhow` at CLI/TUI top level (optional but convenient)
- [x] Pick serialization crate for JSON output:
  - [x] `serde` + `serde_json`
- [x] Pick TUI crates:
  - [x] Ratatui + Crossterm
- [ ] Create feature flags:
  - [x] `desktop-notify` (enables desktop notification backend)
  - [ ] `dav-sync` (future)

### A3. Tooling and CI
- [x] Add `rustfmt` config (or use default) + enforce `cargo fmt --check`
- [x] Enable clippy in CI with warnings denied
- [x] Add CI workflow:
  - [x] build
  - [x] fmt check
  - [x] clippy
  - [x] test
- [x] Add `.editorconfig`
- [x] Add `LICENSE`
- [x] Add `README.md` (minimal)
  - [x] what knotter is
  - [x] quickstart
  - [x] where data is stored (XDG)
- [x] Add `docs/` directory and commit `docs/ARCHITECTURE.md`

**DoD (Milestone A)**  
Workspace builds and tests run in CI; docs folder exists; dependency direction is clean.

---

## 2) Milestone B — Domain model + business rules (knotter-core)

### B1. Module layout in `knotter-core`
- [x] `src/lib.rs` exports the public API
- [x] `src/domain/`:
  - [x] `contact.rs`
  - [x] `interaction.rs`
  - [x] `tag.rs`
  - [x] `ids.rs` (newtypes)
- [x] `src/rules/`:
  - [x] `due.rs` (due state computation)
  - [x] `cadence.rs` (schedule helpers)
- [x] `src/filter/`:
  - [x] `ast.rs`
  - [x] `parser.rs`
  - [x] `mod.rs`
- [x] `src/error.rs`

### B2. IDs + invariants
- [x] Implement ID newtypes:
  - [x] `ContactId`
  - [x] `InteractionId`
  - [x] `TagId`
- [x] Ensure IDs serialize cleanly to/from strings (for DB + export)
- [x] Add constructors/helpers to prevent “raw uuid string everywhere”

### B3. Define domain structs
- [x] Implement `Contact`:
  - [x] required: `display_name`
  - [x] optional: email, phone, handle, timezone
  - [x] scheduling: `next_touchpoint_at: Option<i64>`, `cadence_days: Option<i32>`
  - [x] timestamps: `created_at`, `updated_at`
  - [x] optional `archived_at` (included in schema; UI support can be post-MVP)
- [x] Implement `Interaction`:
  - [x] `occurred_at`, `created_at`
  - [x] `InteractionKind` enum (`Call`, `Text`, `Hangout`, `Email`, `Other(String)`)
  - [x] `follow_up_at: Option<i64>`
- [x] Implement `Tag`:
  - [x] `name` normalization rules (single shared function; spaces -> `-`)

### B4. Core rules
- [x] Implement due logic:
  - [x] `DueState::{Unscheduled, Overdue, Today, Soon, Scheduled}`
  - [x] compute from `(now_utc, next_touchpoint_at, soon_days, local_timezone)`
- [x] Implement cadence helper:
  - [x] `schedule_next(now_utc, cadence_days) -> i64`
- [x] Implement “touch” helper logic (core-level behavior only):
  - [x] if reschedule requested and cadence set => update next_touchpoint_at to now + cadence
  - [x] otherwise leave next_touchpoint_at unchanged

### B5. Filter language spec + parser
- [x] Keep filter language spec in `docs/ARCHITECTURE.md` (no separate doc for MVP)
- [x] Implement AST:
  - [x] text tokens
  - [x] tags `#tag`
  - [x] due tokens `due:overdue|today|soon|any|none`
  - [x] AND semantics across tokens
- [x] Parser requirements:
  - [x] tokenize by whitespace
  - [x] invalid `due:` value => parse error
  - [x] empty tag `#` => parse error
  - [x] normalize tags via shared function
- [x] Add unit tests:
  - [x] happy paths (single tag, multiple tags, due filters)
  - [x] invalid tokens
  - [x] normalization behavior

### B6. Core JSON types (for CLI output)
- [x] Define lightweight, stable JSON structs (DTOs), separate from domain if needed
- [x] Ensure you can output:
  - [x] `ContactListItemDto`
  - [x] `ContactDetailDto`
  - [x] `ReminderOutputDto`

**DoD (Milestone B)**  
Core compiles standalone; due logic + parser are tested; invariants enforced consistently.

---

## 3) Milestone C — SQLite store + migrations (knotter-store)

### C1. XDG paths and DB opening
- [x] Implement `paths` module:
  - [x] XDG data dir resolution
  - [x] create `.../knotter/` directory if missing
  - [x] DB path: `knotter.sqlite3`
- [x] Create DB open function:
  - [x] open connection
  - [x] set pragmas:
    - [x] `foreign_keys = ON`
    - [x] `journal_mode = WAL`
    - [x] `synchronous = NORMAL`
    - [x] `busy_timeout = 2000` (or config)
- [x] Ensure file permissions are user-restricted where possible (document OS limitations)

### C2. Migrations framework
- [x] Create `migrations/` directory in `knotter-store`
- [x] Treat `docs/DB_SCHEMA.md` as the authoritative schema reference (keep architecture summary in sync)
- [x] Implement migration runner:
  - [x] schema version table `knotter_schema(version)`
  - [x] apply migrations in order inside a transaction
  - [x] set version
  - [x] handle “fresh DB” and “already migrated” cases
- [x] Create `001_init.sql` matching architecture doc schema

### C3. Repositories and API surface
Implement repositories (traits or concrete structs). Keep SQL internal.

#### Contacts
- [x] `create_contact`:
  - [x] validate name non-empty
  - [x] set created/updated timestamps
- [x] `update_contact`:
  - [x] update `updated_at`
- [x] `get_contact`
- [x] `delete_contact` (hard delete MVP)
- [ ] `archive_contact` (optional; if included, add CLI/TUI support later)
- [x] `list_contacts(query)`:
  - [x] supports text filters (name/email/phone/handle)
  - [x] supports tag filters via EXISTS
  - [x] supports due filters by comparing `next_touchpoint_at` to computed boundaries
  - [x] stable ordering:
    - [x] due first (overdue, today, soon), then scheduled, then unscheduled
    - [x] within same bucket sort by display_name

#### Tags
- [x] `upsert_tag`:
  - [x] normalize name
  - [x] unique constraint ensures dedupe
- [x] `list_tags_with_counts`
- [x] `list_tags_for_contact`
- [x] bulk tag lookup for list views avoids SQLite parameter limits (temp table strategy)
- [x] `add_tag_to_contact`
- [x] `remove_tag_from_contact`
- [x] `set_contact_tags` (replace entire tag set; simplifies TUI tag editor)

#### Interactions
- [x] `add_interaction`
- [x] `list_interactions(contact_id, limit, offset)`
- [ ] `delete_interaction` (optional MVP)
- [x] `touch_contact` helper:
  - [x] inserts a minimal interaction
  - [x] optionally reschedules next_touchpoint_at (when requested + cadence exists)

### C4. SQL query compilation from filter AST
- [x] Implement `ContactQuery` compilation:
  - [x] convert filter tokens into WHERE clauses + bound params
  - [x] all params must be bound, never interpolated
- [x] For due filters:
  - [x] compute boundaries in Rust:
    - [x] start of today local (UTC timestamp)
    - [x] start of tomorrow local (UTC timestamp)
    - [x] soon window end timestamp
  - [x] translate to range queries on `next_touchpoint_at`

### C5. Store tests (must-have)
- [x] Migration tests:
  - [x] new DB applies all migrations cleanly
  - [x] re-running doesn’t break
- [x] CRUD tests:
  - [x] create/update/get/delete contact
- [x] Tag tests:
  - [x] normalization is applied
  - [x] attach/detach
  - [x] counts correct
- [x] Filter tests:
  - [x] tag AND logic correct
  - [x] due filters correct (overdue/today/soon/none)
- [x] Interaction tests:
  - [x] add/list order by occurred_at desc
  - [x] touch creates interaction and optional reschedule behavior works

**DoD (Milestone C)**  
DB opens via XDG path, migrations run, repos work, and tests cover the core behaviors.

---

## 4) Milestone D — CLI MVP (knotter-cli)

### D1. CLI skeleton
- [x] Implement `knotter` binary entry:
  - [x] parse args
  - [x] open DB + migrate
  - [x] run command
  - [x] map errors to exit codes
  - [x] invalid filter syntax returns exit code 3
- [x] Add global flags:
  - [x] `--db-path` (optional override, for testing)
  - [x] `--config` (optional)
  - [x] `--json` (for commands that support it)
  - [x] `--verbose` (optional)

### D2. Core commands (MVP)
Contacts:
- [x] `knotter add-contact`
  - [x] `--name`
  - [x] `--email?`
  - [x] `--phone?`
  - [x] `--handle?`
  - [x] `--cadence-days?`
  - [x] `--next-touchpoint-at?` (date input)
- [x] `knotter edit-contact <id>` (flags optional; only update provided fields)
- [x] `knotter show <id>`
- [x] `knotter list [--filter "…"] [--json]`
  - [x] bulk tag fetch to avoid N+1 queries
- [x] `knotter delete <id>` (optional but useful)

Tags:
- [x] `knotter tag add <id> <tag>`
- [x] `knotter tag rm <id> <tag>`
- [x] `knotter tag ls [--json]`

Interactions:
- [x] `knotter add-note <id>`
  - [x] `--kind call|text|hangout|email|other:<label>`
  - [x] `--when` (optional, default now)
  - [x] `--note` (optional; if absent, read stdin for note)
  - [x] `--follow-up-at` (optional)
- [x] `knotter touch <id>`
  - [x] creates a small interaction at “now”
  - [x] `--reschedule` (if cadence set, update next touchpoint)

Touchpoints:
- [x] `knotter schedule <id> --at "YYYY-MM-DD" [--time "HH:MM"]`
- [x] `knotter clear-schedule <id>`

Reminders:
- [x] `knotter remind [--soon-days N] [--notify] [--json]`
  - [x] groups overdue/today/soon
  - [x] stdout output stable for cron/systemd usage
  - [x] if `--notify` and feature `desktop-notify` enabled, trigger desktop notification backend; otherwise fall back to stdout

TUI launcher:
- [x] `knotter tui`

Import/export:
- [x] `knotter import vcf <file>`
- [x] `knotter export vcf [--out <file>]`
- [x] `knotter export ics [--out <file>] [--window-days N]`

### D3. Output format spec
- [x] Write `docs/cli-output.md` (short but explicit):
  - [x] how IDs are printed
  - [x] how due states are shown
  - [x] JSON schema notes (fields + stability expectations)

### D4. CLI integration tests
- [x] Add a small harness:
  - [x] create temp DB
  - [x] run binary commands
  - [x] assert outputs
- [x] Test flows:
  - [x] add contact → list includes it
  - [x] tag add → filter `#tag` finds it
  - [x] schedule → remind includes it in correct bucket

**DoD (Milestone D)**  
CLI is fully usable without TUI; reminders and export are operational.

---

## 5) Milestone E — TUI MVP (knotter-tui)

### E1. TUI foundation
- [x] Terminal init + restore guaranteed:
  - [x] normal exit
  - [x] panic hook restore
  - [x] ctrl-c handling
- [x] Event loop:
  - [x] input events
  - [x] tick events (optional)
  - [x] resize events

### E2. App state + mode machine
- [x] Implement `App`:
  - [x] mode enum
  - [x] filter input string
  - [x] parsed filter + parse errors
  - [x] contact list cache
  - [x] selection cursor
  - [x] detail cache (selected contact + tags + recent interactions)
  - [x] status line
  - [x] error line
- [x] Implement modes:
  - [x] List
  - [x] Detail(contact_id)
  - [x] FilterEditing
  - [x] ModalAddContact
  - [x] ModalEditContact(contact_id)
  - [x] ModalAddNote(contact_id)
  - [x] ModalEditTags(contact_id)
  - [x] ModalSchedule(contact_id)

### E3. Action/side-effect pattern
- [x] Define UI actions (examples):
  - [x] `LoadList(filter)`
  - [x] `LoadDetail(contact_id)`
  - [x] `CreateContact(...)`
  - [x] `UpdateContact(...)`
  - [x] `AddInteraction(...)`
  - [x] `SetTags(contact_id, tags)`
  - [x] `Schedule(contact_id, at)`
- [x] Executor runs actions and returns results to update state
- [x] Ensure no blocking DB calls inside render functions

### E4. Screens and workflows
List screen:
- [x] contact list shows:
  - [x] name
  - [x] due state indicator
  - [x] next touchpoint date (if any)
  - [ ] top tags (truncate smartly)
  - [x] tags displayed (no truncation yet)
- [x] keybinds:
  - [x] arrows/j/k navigation
  - [x] enter opens detail
  - [x] `/` edit filter
  - [x] `a` add contact
  - [x] `e` edit selected contact
  - [x] `t` edit tags
  - [x] `n` add note
  - [x] `s` schedule
  - [x] `x` clear schedule
  - [x] `q` quit

Detail screen:
- [x] show contact fields
- [x] show tags
- [x] show next touchpoint + cadence
- [x] show recent interactions (scroll)
- [x] allow quick add note from detail
- [x] allow tag editing and scheduling from detail

Modals:
- [x] Add/Edit contact modal:
  - [x] validations (name required, cadence positive, etc.)
  - [x] consistent date parsing with CLI (reuse parsing utility)
- [x] Add note modal:
  - [x] kind selector
  - [x] timestamp default now
  - [x] multi-line note editing
- [x] Tag editor modal:
  - [x] show list of tags + counts
  - [x] type-to-filter tags
  - [x] create new tag on enter
  - [x] toggle attach/detach
  - [x] apply set_tags (replace) on save
- [x] Schedule modal:
  - [x] date input
  - [x] optional time input
  - [ ] quick options (today+7, today+30) (optional)

### E5. TUI docs + smoke checks
- [x] Write `docs/KEYBINDINGS.md`
- [x] Manual smoke check checklist doc (`docs/tui-smoke.md`):
  - [x] open TUI
  - [x] add contact
  - [x] add tag
  - [x] add note
  - [x] schedule touchpoint
  - [x] filter by `#tag` and `due:soon`

**DoD (Milestone E)**  
TUI provides the same core workflows as CLI (at least add note, tags, schedule, filter).

---

## 6) Milestone F — Import/export adapters (knotter-sync)

### F1. vCard (.vcf) import
- [x] Implement parser integration with chosen crate
- [x] Mapping rules:
  - [x] FN → display_name (required)
  - [x] EMAIL (first) → email
  - [x] TEL (first) → phone
  - [x] CATEGORIES → tags
- [x] Decide dedupe policy (document + implement):
  - [x] MVP recommended: if email matches existing contact, update; else create new
  - [x] if missing email, do not dedupe (create new)
  - [x] skip when multiple contacts share the same email or the only match is archived
- [x] Import report:
  - [x] created
  - [x] updated
  - [x] skipped
  - [x] warnings (missing FN, invalid tags, etc.)
- [x] macOS Contacts import (Contacts app vCard export)
- [x] CardDAV import (feature `dav-sync`)

### F2. vCard export
- [x] Export all contacts as vCards:
  - [x] include FN, EMAIL, TEL
  - [x] include CATEGORIES from tags
- [x] Optional knotter metadata via X- properties (document clearly):
  - [x] `X-KNOTTER-NEXT-TOUCHPOINT`
  - [x] `X-KNOTTER-CADENCE-DAYS`
- [x] Ensure exported file is parseable by common apps (keep it conservative)

### F3. iCalendar (.ics) export for touchpoints
- [x] Export one event per contact with `next_touchpoint_at`
- [x] Stable UID generation:
  - [x] deterministic from contact UUID (so repeated exports update rather than duplicate)
- [x] Event fields:
  - [x] SUMMARY: `Reach out to {name}`
  - [x] DTSTART: from next_touchpoint_at (choose UTC for simplicity)
  - [x] DESCRIPTION: tags and optional last-interaction snippet
- [x] Export options:
  - [x] `--window-days` limits events
  - [x] due-only mode optional

### F4. Sync tests
- [x] vCard parse tests:
  - [x] FN missing -> warning + skip
  - [x] categories -> tags normalized
- [x] vCard export tests:
  - [x] exported file parses back and contains expected fields
- [x] ICS export tests:
  - [x] UID stable
  - [x] DTSTART correct for known timestamps

### F5. Docs
- [x] Write `docs/import-export.md`:
  - [x] what fields knotter imports/exports
  - [x] what may be lost when round-tripping via other apps
  - [x] how UID stability works for ICS

**DoD (Milestone F)**  
Import/export works reliably with predictable mappings and test coverage.

---

## 7) Milestone G — Reminders + notification backends

### G1. Reminder computation (core + store integration)
- [x] Implement reminder grouping:
  - [x] overdue
  - [x] today
  - [x] soon (N days)
- [x] Ensure the same logic powers CLI and TUI badges

### G2. Notifier abstraction
- [x] Define a small notifier interface in a non-core crate (or CLI/TUI module):
  - [x] `send(title, body) -> Result<()>`
- [x] Implement stdout backend (always available)
- [x] Implement desktop backend behind `desktop-notify` feature:
  - [x] if it fails, fallback to stdout
- [x] Wire `knotter remind --notify` to notifier selection
- [x] Add config support (optional MVP):
  - [x] default notify on/off
  - [x] default soon window days

### G3. Scheduling documentation
- [x] Write `docs/scheduling.md`:
  - [x] cron example (runs `knotter remind --notify`)
  - [x] systemd user timer example
  - [x] note about running without desktop notifications (stdout mode)

**DoD (Milestone G)**  
Daily reminders can be scheduled externally; notifications are optional and safe.

---

## 8) Milestone H — Documentation, polish, and release readiness

### H1. Documentation completeness
- [x] README final:
  - [x] quickstart
  - [x] CLI examples
  - [x] TUI basics + keybinds
  - [x] data location (XDG)
  - [x] reminders scheduling
  - [x] import/export usage
- [x] Keep architecture doc up to date with any deviations:
  - [x] schema changes
  - [x] filter changes
  - [x] feature flag changes

### H2. Backup and portability
- [x] Implement `knotter backup`:
  - [x] copies SQLite DB to timestamped file in data dir (or user-specified path)
  - [x] reject backup targets that resolve to the live DB or its WAL/SHM sidecars
- [x] Implement `knotter export json` (optional but very useful):
  - [x] full snapshot for portability

### H3. Shell completions (optional MVP)
- [x] Generate completion scripts via CLI framework tooling (`knotter completions`)
- [x] Document how to install completions

### H4. Robustness and ergonomics
- [x] Ensure all CLI commands have consistent error messages
- [x] Ensure exit codes are correct (0 success, non-zero failure)
- [x] Add logging policy:
  - [x] quiet by default
  - [x] verbose flag prints debug info (never secrets)
- [x] TUI never corrupts terminal state even on panic

**DoD (Milestone H)**  
A new user can install, run, and understand knotter using docs only.

---

## 9) Post-MVP backlog (planned, not required to ship)

### I1. CardDAV/CalDAV sync (feature `dav-sync`)
- [ ] Define `Source` traits for contacts and calendar objects
- [ ] Implement DAV pull/push with explicit `knotter sync` command
- [ ] Add conflict policy:
  - [ ] last-updated-wins OR manual conflict list
- [ ] Credential handling:
  - [ ] config with strict permissions (minimum)
  - [ ] keyring integration (better)

### I2. Advanced filtering
- [ ] OR semantics (e.g. `#designer,#engineer`)
- [ ] interaction note search in filter (requires indexing strategy)
- [ ] saved filters (“smart lists”)

### I3. Better scheduling UX
- [ ] snooze workflow in TUI
- [ ] “touch and reschedule” in one keypress
- [ ] per-tag default cadence (e.g. friends 30d, family 14d)

### I4. Privacy/security enhancements
- [ ] optional DB encryption (careful: portability trade-offs)
- [ ] redact sensitive outputs by default in logs

---

## 10) Acceptance test checklist (run before declaring MVP)

CLI:
- [x] add contact with name only
- [x] add email/phone/handle
- [x] tag add + list by `--filter "#tag"`
- [x] schedule next touchpoint
- [x] remind shows it in the expected bucket
- [x] add note, then show detail includes note
- [x] export vcf produces file importable elsewhere
- [x] export ics produces events with stable UIDs (re-export doesn’t duplicate)
- [x] export json produces a full snapshot

TUI:
- [x] open list view, filter by `#tag`
- [x] open detail
- [x] add note via modal
- [x] edit tags via modal
- [x] schedule touchpoint via modal
- [x] quit restores terminal correctly

Portability:
- [x] remove XDG env vars and verify fallback path works
- [x] DB file is created in expected location
- [x] reminders work without desktop environment (stdout)

---

## Notes to self (implementation guardrails)

- Keep knotter-core pure: no filesystem, no DB, no terminal.
- Keep normalization rules single-sourced: tag normalization must never diverge.
- Timestamps in SQLite are UTC unix seconds.
- Use bound parameters for all SQL.
- Stable ICS UID is mandatory for non-annoying calendar behavior.
- Prefer shipping MVP over expanding scope into “full sync” too early.
