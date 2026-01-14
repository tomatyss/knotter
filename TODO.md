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
- [ ] Create Cargo workspace root
- [ ] Create crates under `crates/`:
  - [ ] `knotter-core`
  - [ ] `knotter-store`
  - [ ] `knotter-sync`
  - [ ] `knotter-cli`
  - [ ] `knotter-tui`
  - [ ] (optional) `knotter` umbrella binary crate
- [ ] Ensure dependency direction matches architecture:
  - [ ] `knotter-core` has no DB, no IO, no terminal deps
  - [ ] `knotter-store` depends on core
  - [ ] `knotter-sync` depends on core (optionally store for upserts)
  - [ ] CLI/TUI depend on core/store/sync, never the reverse

### A2. Decide baseline crates and features (commit early)
- [ ] Pick datetime crate strategy (and stick to it):
  - [ ] timestamps stored as `i64` unix seconds UTC in DB
  - [ ] conversions at edges
- [ ] Pick DB crate strategy:
  - [ ] recommended MVP: `rusqlite` with bundled sqlite option enabled
- [ ] Pick CLI argument parsing crate (recommended: `clap`)
- [ ] Pick error crates:
  - [ ] `thiserror` in core/store/sync
  - [ ] `anyhow` at CLI/TUI top level (optional but convenient)
- [ ] Pick serialization crate for JSON output:
  - [ ] `serde` + `serde_json`
- [ ] Pick TUI crates:
  - [ ] Ratatui + Crossterm
- [ ] Create feature flags:
  - [ ] `desktop-notify` (enables desktop notification backend)
  - [ ] `dav-sync` (future)

### A3. Tooling and CI
- [ ] Add `rustfmt` config (or use default) + enforce `cargo fmt --check`
- [ ] Enable clippy in CI with warnings denied
- [ ] Add CI workflow:
  - [ ] build
  - [ ] fmt check
  - [ ] clippy
  - [ ] test
- [ ] Add `.editorconfig`
- [ ] Add `LICENSE`
- [ ] Add `README.md` (minimal)
  - [ ] what knotter is
  - [ ] quickstart
  - [ ] where data is stored (XDG)
- [ ] Add `docs/` directory and commit `docs/ARCHITECTURE.md` (already produced)

**DoD (Milestone A)**  
Workspace builds and tests run in CI; docs folder exists; dependency direction is clean.

---

## 2) Milestone B — Domain model + business rules (knotter-core)

### B1. Module layout in `knotter-core`
- [ ] `src/lib.rs` exports the public API
- [ ] `src/domain/`:
  - [ ] `contact.rs`
  - [ ] `interaction.rs`
  - [ ] `tag.rs`
  - [ ] `ids.rs` (newtypes)
- [ ] `src/rules/`:
  - [ ] `due.rs` (due state computation)
  - [ ] `cadence.rs` (schedule helpers)
- [ ] `src/filter/`:
  - [ ] `ast.rs`
  - [ ] `parser.rs`
  - [ ] `mod.rs`
- [ ] `src/error.rs`

### B2. IDs + invariants
- [ ] Implement ID newtypes:
  - [ ] `ContactId`
  - [ ] `InteractionId`
  - [ ] `TagId`
- [ ] Ensure IDs serialize cleanly to/from strings (for DB + export)
- [ ] Add constructors/helpers to prevent “raw uuid string everywhere”

### B3. Define domain structs
- [ ] Implement `Contact`:
  - [ ] required: `display_name`
  - [ ] optional: email, phone, handle, timezone
  - [ ] scheduling: `next_touchpoint_at: Option<i64>`, `cadence_days: Option<i32>`
  - [ ] timestamps: `created_at`, `updated_at`
  - [ ] optional `archived_at` (can be later; decide now if included in schema)
- [ ] Implement `Interaction`:
  - [ ] `occurred_at`, `created_at`
  - [ ] `InteractionKind` enum (`Call`, `Text`, `Hangout`, `Email`, `Other(String)`)
  - [ ] `follow_up_at: Option<i64>`
- [ ] Implement `Tag`:
  - [ ] `name` normalization rules (single shared function)

### B4. Core rules
- [ ] Implement due logic:
  - [ ] `DueState::{Unscheduled, Overdue, Today, Soon, Scheduled}`
  - [ ] compute from `(now_utc, next_touchpoint_at, soon_days, local_timezone)`
- [ ] Implement cadence helper:
  - [ ] `schedule_next(now_utc, cadence_days) -> i64`
- [ ] Implement “touch” helper logic (core-level behavior only):
  - [ ] if reschedule requested and cadence set => update next_touchpoint_at to now + cadence
  - [ ] otherwise leave next_touchpoint_at unchanged

### B5. Filter language spec + parser
- [ ] Write `docs/filter-language.md` (or keep in architecture; either is fine)
- [ ] Implement AST:
  - [ ] text tokens
  - [ ] tags `#tag`
  - [ ] due tokens `due:overdue|today|soon|any|none`
  - [ ] AND semantics across tokens
- [ ] Parser requirements:
  - [ ] tokenize by whitespace
  - [ ] invalid `due:` value => parse error
  - [ ] empty tag `#` => parse error
  - [ ] normalize tags via shared function
- [ ] Add unit tests:
  - [ ] happy paths (single tag, multiple tags, due filters)
  - [ ] invalid tokens
  - [ ] normalization behavior

### B6. Core JSON types (for CLI output)
- [ ] Define lightweight, stable JSON structs (DTOs), separate from domain if needed
- [ ] Ensure you can output:
  - [ ] `ContactListItemDto`
  - [ ] `ContactDetailDto`
  - [ ] `ReminderOutputDto`

**DoD (Milestone B)**  
Core compiles standalone; due logic + parser are tested; invariants enforced consistently.

---

## 3) Milestone C — SQLite store + migrations (knotter-store)

### C1. XDG paths and DB opening
- [ ] Implement `paths` module:
  - [ ] XDG data dir resolution
  - [ ] create `.../knotter/` directory if missing
  - [ ] DB path: `knotter.sqlite3`
- [ ] Create DB open function:
  - [ ] open connection
  - [ ] set pragmas:
    - [ ] `foreign_keys = ON`
    - [ ] `journal_mode = WAL`
    - [ ] `synchronous = NORMAL`
    - [ ] `busy_timeout = 2000` (or config)
- [ ] Ensure file permissions are user-restricted where possible (document OS limitations)

### C2. Migrations framework
- [ ] Create `migrations/` directory in `knotter-store`
- [ ] Implement migration runner:
  - [ ] schema version table `knotter_schema(version)`
  - [ ] apply migrations in order inside a transaction
  - [ ] set version
  - [ ] handle “fresh DB” and “already migrated” cases
- [ ] Create `001_init.sql` matching architecture doc schema

### C3. Repositories and API surface
Implement repositories (traits or concrete structs). Keep SQL internal.

#### Contacts
- [ ] `create_contact`:
  - [ ] validate name non-empty
  - [ ] set created/updated timestamps
- [ ] `update_contact`:
  - [ ] update `updated_at`
- [ ] `get_contact`
- [ ] `delete_contact` (hard delete MVP)
- [ ] `archive_contact` (optional; if included, add CLI/TUI support later)
- [ ] `list_contacts(query)`:
  - [ ] supports text filters (name/email/phone/handle)
  - [ ] supports tag filters via EXISTS
  - [ ] supports due filters by comparing `next_touchpoint_at` to computed boundaries
  - [ ] stable ordering:
    - [ ] due first (overdue, today, soon), then scheduled, then unscheduled
    - [ ] within same bucket sort by display_name

#### Tags
- [ ] `upsert_tag`:
  - [ ] normalize name
  - [ ] unique constraint ensures dedupe
- [ ] `list_tags_with_counts`
- [ ] `list_tags_for_contact`
- [ ] `add_tag_to_contact`
- [ ] `remove_tag_from_contact`
- [ ] `set_contact_tags` (replace entire tag set; simplifies TUI tag editor)

#### Interactions
- [ ] `add_interaction`
- [ ] `list_interactions(contact_id, limit, offset)`
- [ ] `delete_interaction` (optional MVP)
- [ ] `touch_contact` helper:
  - [ ] inserts a minimal interaction
  - [ ] optionally reschedules next_touchpoint_at (when requested + cadence exists)

### C4. SQL query compilation from filter AST
- [ ] Implement `ContactQuery` compilation:
  - [ ] convert filter tokens into WHERE clauses + bound params
  - [ ] all params must be bound, never interpolated
- [ ] For due filters:
  - [ ] compute boundaries in Rust:
    - [ ] start of today local (UTC timestamp)
    - [ ] start of tomorrow local (UTC timestamp)
    - [ ] soon window end timestamp
  - [ ] translate to range queries on `next_touchpoint_at`

### C5. Store tests (must-have)
- [ ] Migration tests:
  - [ ] new DB applies all migrations cleanly
  - [ ] re-running doesn’t break
- [ ] CRUD tests:
  - [ ] create/update/get/delete contact
- [ ] Tag tests:
  - [ ] normalization is applied
  - [ ] attach/detach
  - [ ] counts correct
- [ ] Filter tests:
  - [ ] tag AND logic correct
  - [ ] due filters correct (overdue/today/soon/none)
- [ ] Interaction tests:
  - [ ] add/list order by occurred_at desc
  - [ ] touch creates interaction and optional reschedule behavior works

**DoD (Milestone C)**  
DB opens via XDG path, migrations run, repos work, and tests cover the core behaviors.

---

## 4) Milestone D — CLI MVP (knotter-cli)

### D1. CLI skeleton
- [ ] Implement `knotter` binary entry:
  - [ ] parse args
  - [ ] open DB + migrate
  - [ ] run command
  - [ ] map errors to exit codes
- [ ] Add global flags:
  - [ ] `--db-path` (optional override, for testing)
  - [ ] `--config` (optional)
  - [ ] `--json` (for commands that support it)
  - [ ] `--verbose` (optional)

### D2. Core commands (MVP)
Contacts:
- [ ] `knotter add-contact`
  - [ ] `--name`
  - [ ] `--email?`
  - [ ] `--phone?`
  - [ ] `--handle?`
  - [ ] `--cadence-days?`
  - [ ] `--next-touchpoint-at?` (date input)
- [ ] `knotter edit-contact <id>` (flags optional; only update provided fields)
- [ ] `knotter show <id>`
- [ ] `knotter list [--filter "…"] [--json]`
- [ ] `knotter delete <id>` (optional but useful)

Tags:
- [ ] `knotter tag add <id> <tag>`
- [ ] `knotter tag rm <id> <tag>`
- [ ] `knotter tag ls [--json]`

Interactions:
- [ ] `knotter add-note <id>`
  - [ ] `--kind call|text|hangout|email|other:<label>`
  - [ ] `--when` (optional, default now)
  - [ ] `--note` (optional; if absent, read stdin for note)
  - [ ] `--follow-up-at` (optional)
- [ ] `knotter touch <id>`
  - [ ] creates a small interaction at “now”
  - [ ] `--reschedule` (if cadence set, update next touchpoint)

Touchpoints:
- [ ] `knotter schedule <id> --at "YYYY-MM-DD" [--time "HH:MM"]`
- [ ] `knotter clear-schedule <id>`

Reminders:
- [ ] `knotter remind [--soon-days N] [--notify] [--json]`
  - [ ] groups overdue/today/soon
  - [ ] stdout output stable for cron/systemd usage
  - [ ] if `--notify` and feature `desktop-notify` enabled, trigger desktop notification backend; otherwise fall back to stdout

TUI launcher:
- [ ] `knotter tui`

Import/export:
- [ ] `knotter import vcf <file>`
- [ ] `knotter export vcf [--out <file>]`
- [ ] `knotter export ics [--out <file>] [--window-days N]`

### D3. Output format spec
- [ ] Write `docs/cli-output.md` (short but explicit):
  - [ ] how IDs are printed
  - [ ] how due states are shown
  - [ ] JSON schema notes (fields + stability expectations)

### D4. CLI integration tests
- [ ] Add a small harness:
  - [ ] create temp DB
  - [ ] run binary commands
  - [ ] assert outputs
- [ ] Test flows:
  - [ ] add contact → list includes it
  - [ ] tag add → filter `#tag` finds it
  - [ ] schedule → remind includes it in correct bucket

**DoD (Milestone D)**  
CLI is fully usable without TUI; reminders and export are operational.

---

## 5) Milestone E — TUI MVP (knotter-tui)

### E1. TUI foundation
- [ ] Terminal init + restore guaranteed:
  - [ ] normal exit
  - [ ] panic hook restore
  - [ ] ctrl-c handling
- [ ] Event loop:
  - [ ] input events
  - [ ] tick events (optional)
  - [ ] resize events

### E2. App state + mode machine
- [ ] Implement `App`:
  - [ ] mode enum
  - [ ] filter input string
  - [ ] parsed filter + parse errors
  - [ ] contact list cache
  - [ ] selection cursor
  - [ ] detail cache (selected contact + tags + recent interactions)
  - [ ] status line
  - [ ] error line
- [ ] Implement modes:
  - [ ] List
  - [ ] Detail(contact_id)
  - [ ] FilterEditing
  - [ ] ModalAddContact
  - [ ] ModalEditContact(contact_id)
  - [ ] ModalAddNote(contact_id)
  - [ ] ModalEditTags(contact_id)
  - [ ] ModalSchedule(contact_id)

### E3. Action/side-effect pattern
- [ ] Define UI actions (examples):
  - [ ] `LoadList(filter)`
  - [ ] `LoadDetail(contact_id)`
  - [ ] `CreateContact(...)`
  - [ ] `UpdateContact(...)`
  - [ ] `AddInteraction(...)`
  - [ ] `SetTags(contact_id, tags)`
  - [ ] `Schedule(contact_id, at)`
- [ ] Executor runs actions and returns results to update state
- [ ] Ensure no blocking DB calls inside render functions

### E4. Screens and workflows
List screen:
- [ ] contact list shows:
  - [ ] name
  - [ ] due state indicator
  - [ ] next touchpoint date (if any)
  - [ ] top tags (truncate smartly)
- [ ] keybinds:
  - [ ] arrows/j/k navigation
  - [ ] enter opens detail
  - [ ] `/` edit filter
  - [ ] `a` add contact
  - [ ] `e` edit selected contact
  - [ ] `t` edit tags
  - [ ] `n` add note
  - [ ] `s` schedule
  - [ ] `x` clear schedule
  - [ ] `q` quit

Detail screen:
- [ ] show contact fields
- [ ] show tags
- [ ] show next touchpoint + cadence
- [ ] show recent interactions (scroll)
- [ ] allow quick add note from detail
- [ ] allow tag editing and scheduling from detail

Modals:
- [ ] Add/Edit contact modal:
  - [ ] validations (name required, cadence positive, etc.)
  - [ ] consistent date parsing with CLI (reuse parsing utility)
- [ ] Add note modal:
  - [ ] kind selector
  - [ ] timestamp default now
  - [ ] multi-line note editing
- [ ] Tag editor modal:
  - [ ] show list of tags + counts
  - [ ] type-to-filter tags
  - [ ] create new tag on enter
  - [ ] toggle attach/detach
  - [ ] apply set_tags (replace) on save
- [ ] Schedule modal:
  - [ ] date input
  - [ ] optional time input
  - [ ] quick options (today+7, today+30) (optional)

### E5. TUI docs + smoke checks
- [ ] Write `docs/KEYBINDINGS.md`
- [ ] Manual smoke check checklist doc:
  - [ ] open TUI
  - [ ] add contact
  - [ ] add tag
  - [ ] add note
  - [ ] schedule touchpoint
  - [ ] filter by `#tag` and `due:soon`

**DoD (Milestone E)**  
TUI provides the same core workflows as CLI (at least add note, tags, schedule, filter).

---

## 6) Milestone F — Import/export adapters (knotter-sync)

### F1. vCard (.vcf) import
- [ ] Implement parser integration with chosen crate
- [ ] Mapping rules:
  - [ ] FN → display_name (required)
  - [ ] EMAIL (first) → email
  - [ ] TEL (first) → phone
  - [ ] CATEGORIES → tags
- [ ] Decide dedupe policy (document + implement):
  - [ ] MVP recommended: if email matches existing contact, update; else create new
  - [ ] if missing email, do not dedupe (create new)
- [ ] Import report:
  - [ ] created
  - [ ] updated
  - [ ] skipped
  - [ ] warnings (missing FN, invalid tags, etc.)

### F2. vCard export
- [ ] Export all contacts as vCards:
  - [ ] include FN, EMAIL, TEL
  - [ ] include CATEGORIES from tags
- [ ] Optional knotter metadata via X- properties (document clearly):
  - [ ] `X-KNOTTER-NEXT-TOUCHPOINT`
  - [ ] `X-KNOTTER-CADENCE-DAYS`
- [ ] Ensure exported file is parseable by common apps (keep it conservative)

### F3. iCalendar (.ics) export for touchpoints
- [ ] Export one event per contact with `next_touchpoint_at`
- [ ] Stable UID generation:
  - [ ] deterministic from contact UUID (so repeated exports update rather than duplicate)
- [ ] Event fields:
  - [ ] SUMMARY: `Reach out to {name}`
  - [ ] DTSTART: from next_touchpoint_at (choose UTC for simplicity)
  - [ ] DESCRIPTION: tags and optional last-interaction snippet
- [ ] Export options:
  - [ ] `--window-days` limits events
  - [ ] due-only mode optional

### F4. Sync tests
- [ ] vCard parse tests:
  - [ ] FN missing -> warning + skip
  - [ ] categories -> tags normalized
- [ ] vCard export tests:
  - [ ] exported file parses back and contains expected fields
- [ ] ICS export tests:
  - [ ] UID stable
  - [ ] DTSTART correct for known timestamps

### F5. Docs
- [ ] Write `docs/import-export.md`:
  - [ ] what fields knotter imports/exports
  - [ ] what may be lost when round-tripping via other apps
  - [ ] how UID stability works for ICS

**DoD (Milestone F)**  
Import/export works reliably with predictable mappings and test coverage.

---

## 7) Milestone G — Reminders + notification backends

### G1. Reminder computation (core + store integration)
- [ ] Implement reminder grouping:
  - [ ] overdue
  - [ ] today
  - [ ] soon (N days)
- [ ] Ensure the same logic powers CLI and TUI badges

### G2. Notifier abstraction
- [ ] Define a small notifier interface in a non-core crate (or CLI/TUI module):
  - [ ] `send(title, body) -> Result<()>`
- [ ] Implement stdout backend (always available)
- [ ] Implement desktop backend behind `desktop-notify` feature:
  - [ ] if it fails, fallback to stdout
- [ ] Wire `knotter remind --notify` to notifier selection
- [ ] Add config support (optional MVP):
  - [ ] default notify on/off
  - [ ] default soon window days

### G3. Scheduling documentation
- [ ] Write `docs/scheduling.md`:
  - [ ] cron example (runs `knotter remind --notify`)
  - [ ] systemd user timer example
  - [ ] note about running without desktop notifications (stdout mode)

**DoD (Milestone G)**  
Daily reminders can be scheduled externally; notifications are optional and safe.

---

## 8) Milestone H — Documentation, polish, and release readiness

### H1. Documentation completeness
- [ ] README final:
  - [ ] quickstart
  - [ ] CLI examples
  - [ ] TUI basics + keybinds
  - [ ] data location (XDG)
  - [ ] reminders scheduling
  - [ ] import/export usage
- [ ] Keep architecture doc up to date with any deviations:
  - [ ] schema changes
  - [ ] filter changes
  - [ ] feature flag changes

### H2. Backup and portability
- [ ] Implement `knotter backup`:
  - [ ] copies SQLite DB to timestamped file in data dir (or user-specified path)
- [ ] Implement `knotter export json` (optional but very useful):
  - [ ] full snapshot for portability

### H3. Shell completions (optional MVP)
- [ ] Generate completion scripts via CLI framework tooling
- [ ] Document how to install completions

### H4. Robustness and ergonomics
- [ ] Ensure all CLI commands have consistent error messages
- [ ] Ensure exit codes are correct (0 success, non-zero failure)
- [ ] Add logging policy:
  - [ ] quiet by default
  - [ ] verbose flag prints debug info (never secrets)
- [ ] TUI never corrupts terminal state even on panic

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
- [ ] add contact with name only
- [ ] add email/phone/handle
- [ ] tag add + list by `--filter "#tag"`
- [ ] schedule next touchpoint
- [ ] remind shows it in the expected bucket
- [ ] add note, then show detail includes note
- [ ] export vcf produces file importable elsewhere
- [ ] export ics produces events with stable UIDs (re-export doesn’t duplicate)

TUI:
- [ ] open list view, filter by `#tag`
- [ ] open detail
- [ ] add note via modal
- [ ] edit tags via modal
- [ ] schedule touchpoint via modal
- [ ] quit restores terminal correctly

Portability:
- [ ] remove XDG env vars and verify fallback path works
- [ ] DB file is created in expected location
- [ ] reminders work without desktop environment (stdout)

---

## Notes to self (implementation guardrails)

- Keep knotter-core pure: no filesystem, no DB, no terminal.
- Keep normalization rules single-sourced: tag normalization must never diverge.
- Timestamps in SQLite are UTC unix seconds.
- Use bound parameters for all SQL.
- Stable ICS UID is mandatory for non-annoying calendar behavior.
- Prefer shipping MVP over expanding scope into “full sync” too early.

