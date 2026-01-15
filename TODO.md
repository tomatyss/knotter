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
  - [ ] `desktop-notify` (enables desktop notification backend)
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
- [ ] Ensure file permissions are user-restricted where possible (document OS limitations)

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
- [x] `add_tag_to_contact`
- [x] `remove_tag_from_contact`
- [x] `set_contact_tags` (replace entire tag set; simplifies TUI tag editor)

#### Interactions
- [x] `add_interaction`
- [x] `list_interactions(contact_id, limit, offset)`
- [ ] `delete_interaction` (optional MVP)
- [ ] `touch_contact` helper:
  - [ ] inserts a minimal interaction
  - [ ] optionally reschedules next_touchpoint_at (when requested + cadence exists)

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
- [ ] Filter tests:
  - [x] tag AND logic correct
  - [x] due filters correct (overdue/today/soon/none)
- [ ] Interaction tests:
  - [x] add/list order by occurred_at desc
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
- [x] Write `docs/KEYBINDINGS.md`
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
