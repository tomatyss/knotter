# Knotter Architecture

knotter is a personal CRM / friendship-tracking tool designed for terminal-first workflows.
It is built as an offline-first Rust application with a CLI and a TUI, backed by a portable SQLite database, with standards-based import/export.

This document describes:
- the workspace layout and module boundaries
- the core domain model and invariants
- storage (SQLite) schema and migrations
- error handling conventions
- filtering/query semantics
- import/export design
- reminders/notifications architecture
- testing and operational expectations

---

## 1. Design goals

### Primary goals
- **System-agnostic**: knotter should run on most Linux/Unix machines with minimal assumptions.
- **Offline-first**: local database is the source of truth; the app remains useful with no network.
- **Terminal-first UX**: fully usable via CLI; TUI provides fast browsing/editing.
- **Personal CRM features**:
  - contacts (people)
  - interactions/notes (relationship history)
  - scheduling next touchpoint (future intent)
- **Tags + filtering**: quickly list contacts by category and due state.
- **Interchange**:
  - export/import contacts via **vCard (.vcf)**
  - export touchpoints via **iCalendar (.ics)**
- **Optional syncing with macOS apps** (post-MVP): via standards-based CardDAV/CalDAV, not by poking at Apple local databases.

### Non-goals (MVP)
- Full bidirectional sync with iCloud “local” stores.
- Running a mandatory background daemon.
- AI features or “relationship scoring” heuristics.
- Complex recurring schedules (beyond basic cadence + next date).

---

## 2. High-level architecture

knotter is split into layers to keep UI and persistence separate from business logic.

### Layer overview
- **knotter-core**
  - Pure domain types and business rules.
  - No SQLite, no terminal, no file I/O.
- **knotter-config**
  - Loads config from XDG paths or `--config`.
  - Validates values and enforces strict permissions (where supported).
- **knotter-store**
  - SQLite persistence, migrations, and repositories.
  - Converts between DB rows and core domain types.
- **knotter-sync**
  - Import/export adapters:
    - vCard (.vcf) for contacts
    - iCalendar (.ics) for touchpoints
    - Telegram 1:1 sync (snippets only)
  - Future: CardDAV/CalDAV sync (optional).
- **knotter-cli**
  - CLI frontend (commands, argument parsing).
  - Calls core/store/sync; prints output; exit codes.
- **knotter-tui**
  - TUI frontend using Ratatui + Crossterm.
  - Maintains application state and renders views.
- **knotter (bin)**
  - Small binary crate that wires everything together.
  - Can expose both CLI and `knotter tui`.

### Dependency direction (must not be violated)
- knotter-core: depends on (almost) nothing; ideally only `uuid`, `serde` (optional), and a time crate.
- knotter-config: depends on core for validation helpers.
- knotter-store: depends on core + SQLite libs.
- knotter-sync: depends on core + parsing/generation libs; may depend on store when import wants to upsert.
- knotter-cli / knotter-tui: depend on core/config/store/sync; never the other way around.

---

## 3. Workspace layout

Recommended structure:

- `Cargo.toml` (workspace root)
- `crates/`
  - `knotter-core/`
    - `src/`
      - `lib.rs`
      - `domain/` (contact.rs, interaction.rs, tag.rs)
      - `rules/` (due.rs, cadence.rs)
      - `filter/` (parser.rs, ast.rs)
      - `error.rs`
  - `knotter-config/`
    - `src/`
      - `lib.rs` (XDG config lookup + TOML parsing)
  - `knotter-store/`
    - `src/`
      - `lib.rs`
      - `db.rs` (connection/open, pragmas)
      - `migrate.rs`
      - `repo/` (contacts.rs, tags.rs, interactions.rs)
      - `error.rs`
    - `migrations/`
      - `001_init.sql`
      - `002_...sql`
  - `knotter-sync/`
    - `src/`
      - `lib.rs`
      - `vcf/` (import.rs, export.rs, mapping.rs)
      - `ics/` (export.rs, mapping.rs, uid.rs)
      - `error.rs`
  - `knotter-cli/`
    - `src/`
      - `main.rs` (or lib+bin style)
      - `commands/` (add_contact.rs, list.rs, remind.rs, import.rs, export.rs, ...)
      - `output/` (human.rs, json.rs)
  - `knotter-tui/`
    - `src/`
      - `main.rs` (or lib+bin style)
      - `app.rs` (App state)
      - `ui/` (render.rs, widgets.rs)
      - `modes/` (list.rs, detail.rs, modals/)
      - `events.rs` (input mapping)
      - `error.rs`
  - `knotter/` (optional “umbrella” bin)
    - `src/main.rs`

The key is: **core is reusable** and UI crates are replaceable.

---

## 4. Core domain model (knotter-core)

### 4.1 Identity and time

#### IDs
Use UUIDs for portability and for stable export identifiers.
Define newtypes for IDs to prevent mixing them up:
- `ContactId(Uuid)`
- `InteractionId(Uuid)`
- `TagId(Uuid)`
- `ContactDateId(Uuid)`

#### Time representation
Use UTC timestamps in storage and business logic.
Recommended:
- store timestamps as `i64` unix seconds (UTC) in SQLite
- convert to/from a Rust datetime type at the edges

Define in core:
- `Timestamp` wrapper or use a time crate type
- always treat DB timestamps as UTC
- “today/due soon” computations use the **local machine timezone** (MVP) unless contact timezone is explicitly supported later

### 4.2 Entities

#### Contact
A contact represents a person you want to keep in touch with.

Core fields:
- `id: ContactId`
- `display_name: String` (required, non-empty)
- `email: Option<String>` (primary email; additional emails live in `contact_emails`)
- `phone: Option<String>`
- `handle: Option<String>` (free text: Discord/IG/etc.)
- `timezone: Option<String>` (IANA TZ string; optional MVP)
- `created_at: i64` (unix seconds UTC)
- `updated_at: i64` (unix seconds UTC)
- `next_touchpoint_at: Option<i64>` (unix seconds UTC)
- `cadence_days: Option<i32>` (e.g. 7, 14, 30)
- `archived_at: Option<i64>` (optional; included in schema but UI support can be post-MVP)

Invariants:
- `display_name.trim()` must not be empty.
- `cadence_days`, if set, must be > 0 and within a reasonable range (e.g. <= 3650).
- `next_touchpoint_at`, if set, should be a valid timestamp (>= 0 recommended).
- Contact emails are normalized lowercase; exactly one may be marked primary.
- `contact_emails.source` tracks provenance (e.g., cli/tui/vcf or email account name).

#### Interaction
An interaction is a timestamped note/history entry for a contact.

Core fields:
- `id: InteractionId`
- `contact_id: ContactId`
- `occurred_at: i64` (when the interaction happened; default “now”)
- `created_at: i64` (when it was logged; default “now”)
- `kind: InteractionKind`
- `note: String` (can be empty, but usually non-empty is better)
- `follow_up_at: Option<i64>` (optional per-interaction follow-up date)

`InteractionKind`:
- `Call`
- `Text`
- `Hangout`
- `Email`
- `Telegram`
- `Other(String)` (must be normalized/trimmed)

Invariants:
- `Other(s)` should be stored as trimmed; reject empty.
- `occurred_at` should not be wildly in the future (soft validation; warning not hard error).

#### Tag
Tags are categories, like “designer”, “family”, “school”, “soccer”, etc.

Core fields:
- `id: TagId`
- `name: String` (normalized)

Normalization rules (must be identical everywhere):
- trim
- lowercase
- replace spaces with `-`
- collapse repeated `-`
- reject empty after normalization

#### ContactDate
Contact dates capture birthdays and other annual milestones.

Core fields:
- `id: ContactDateId`
- `contact_id: ContactId`
- `kind: ContactDateKind` (`birthday`, `name_day`, `custom`)
- `label: Option<String>` (required for `custom`)
- `month: u8` (1-12)
- `day: u8` (1-31, validated against month)
- `year: Option<i32>` (optional, used for birthdays/notes)
- `created_at: i64`
- `updated_at: i64`
- `source: Option<String>` (cli/tui/vcf/etc)

Invariants:
- `custom` dates require a non-empty label.
- Month/day must be a valid calendar day; `Feb 29` is allowed without a year.
- Date occurrences are evaluated in the local machine timezone (MVP).
- On non-leap years, `Feb 29` occurrences are surfaced on `Feb 28`.

### 4.3 Business rules

#### Due state
Given `now` (UTC) and local timezone rules for “today”:
- Overdue: `next_touchpoint_at < now`
- Due today: same local date as now
- Due soon: within N days (configurable, e.g. 7)
- Scheduled: anything later
- Unscheduled: `next_touchpoint_at == None`

This logic lives in core so both CLI and TUI behave identically.

#### Cadence helper
If a contact has `cadence_days`:
- after a “touch” action (or interaction with rescheduling enabled), set
  `next_touchpoint_at = max(now, occurred_at) + cadence_days` (never in the past)

Optional rule (MVP decision):
- If `next_touchpoint_at` already exists and is later than `now`, only reschedule if user
  explicitly requests (e.g., via CLI flags or `interactions.auto_reschedule`).

Scheduling guard:
- User-provided `next_touchpoint_at` inputs must be `now` or later.
- Date-only inputs are interpreted as end-of-day local time (so "today" remains scheduled).

---

## 5. Filter/query language (knotter-core::filter)

Filtering is used by both CLI and TUI.
knotter defines a minimal filter string that compiles into a query AST.

### 5.1 Supported syntax (MVP)

- Plain text token:
  - matches `display_name`, `email`, `phone`, `handle`
  - optionally matches recent interaction notes (post-MVP, because it’s heavier)
- Tag tokens:
  - `#designer` (require tag “designer”)
- Due tokens:
  - `due:overdue`
  - `due:today`
  - `due:soon`
  - `due:any` (any scheduled, including overdue/today/soon/later)
  - `due:none` (unscheduled)
- Archived tokens:
  - `archived:true` (only archived contacts)
  - `archived:false` (only active contacts)

Combining:
- Default combination is AND across tokens.
- Multiple tags are AND by default:
  - `#designer #founder` means must have both tags.
- (Optional later) OR groups:
  - `#designer,#engineer` means either tag

Default UI behavior:
- CLI/TUI list views exclude archived contacts unless explicitly included via flags or `archived:true`.

### 5.2 AST types

- `FilterExpr`
  - `Text(String)`
  - `Tag(String)` (normalized)
  - `Due(DueSelector)`
  - `Archived(ArchivedSelector)`
  - `And(Vec<FilterExpr>)`
  - (Later) `Or(Vec<FilterExpr>)`

### 5.3 Parser behavior

- Tokenize on whitespace.
- Tokens starting with `#` become Tag filters.
- Tokens starting with `due:` become Due filters.
- Tokens starting with `archived:` become Archived filters.
- Everything else becomes Text filters.
- Invalid tokens:
  - unknown `due:` value -> return parse error
  - unknown `archived:` value -> return parse error
  - empty tag after `#` -> parse error

The parser returns:
- `Result<ContactFilter, FilterParseError>`

---

## 6. Storage architecture (knotter-store)

knotter-store is the only layer that touches SQLite.
It provides repositories that operate on core types and filter ASTs.

### 6.1 SQLite connection + pragmas

Open DB at XDG data path:
- `$XDG_DATA_HOME/knotter/knotter.sqlite3`
- fallback to `~/.local/share/knotter/knotter.sqlite3`

Recommended pragmas (document + test):
- `PRAGMA foreign_keys = ON;`
- `PRAGMA journal_mode = WAL;` (improves concurrency; safe default for a local app)
- `PRAGMA synchronous = NORMAL;` (balance performance/safety)
- `PRAGMA busy_timeout = 2000;` (avoid “database is locked” on short contention)

### 6.2 Migrations

knotter uses numbered SQL migrations stored in `crates/knotter-store/migrations/`.

Migration requirements:
- A schema version table:
  - `knotter_schema(version INTEGER NOT NULL)`
- On startup:
  - open DB
  - apply migrations in order inside a transaction
  - update schema version
- Migrations must be idempotent in the sense that:
  - they only run once each
  - schema version ensures ordering

### 6.3 Schema (MVP)

The authoritative SQL schema lives in [Database Schema](DB_SCHEMA.md). Keep this document aligned with it.

Summary of MVP tables/indexes:
* `knotter_schema(version)` for migration tracking.
* `contacts` with `archived_at` included for future archiving (unused in MVP UI).
* `tags` (normalized), `contact_tags` join table.
* `interactions` with `kind` stored as a normalized string.
* Indexes on `contacts.display_name`, `contacts.next_touchpoint_at`, `contacts.archived_at`,
  `tags.name`, `contact_tags` foreign keys, and `interactions(contact_id, occurred_at DESC)`.

Notes:
* IDs are stored as TEXT UUIDs.
* Timestamps are INTEGER unix seconds UTC.

### 6.4 Repository boundaries

Expose repositories as traits in knotter-store (or as concrete structs with a stable API).
Avoid leaking SQL details to callers.

#### ContactsRepository

* `create_contact(...) -> Contact`
* `update_contact(...) -> Contact`
* `get_contact(id) -> Option<Contact>`
* `delete_contact(id) -> ()` (hard delete MVP)
* `archive_contact(id) -> Contact`
* `unarchive_contact(id) -> Contact`
* `list_contacts(query: ContactQuery) -> Vec<ContactListItem>`

`ContactListItem` is a lightweight projection for list views:

* id
* display_name
* next_touchpoint_at
* due_state (computed in core, not stored)
* tags (either eager-loaded or loaded separately; choose based on performance)

#### TagsRepository

* `upsert_tag(name) -> Tag` (normalize before upsert)
* `list_tags_with_counts() -> Vec<(Tag, count)>`
* `set_contact_tags(contact_id, tags: Vec<TagName>)` (replace set)
* `add_tag(contact_id, tag)`
* `remove_tag(contact_id, tag)`
* `list_tags_for_contact(contact_id) -> Vec<Tag>`
* `list_names_for_contacts(contact_ids: &[ContactId]) -> Map<ContactId, Vec<String>>` (bulk tag lookup for list views; uses per-call temp table to avoid collisions)

#### InteractionsRepository

* `add_interaction(...) -> Interaction`
* `list_interactions(contact_id, limit, offset) -> Vec<Interaction>`
* `delete_interaction(interaction_id)` (optional MVP)
* `touch(contact_id, occurred_at, kind, note, reschedule: bool)` (convenience)

### 6.5 Query compilation strategy

knotter-core provides a parsed filter AST.
knotter-store translates AST -> SQL WHERE + bind parameters.

Rules:

* Always use bound parameters, never string interpolation (avoid SQL injection even in local tools).
* For tag filters, use EXISTS subqueries:

  * require all tags -> multiple EXISTS clauses
* For due filters:

  * compare `next_touchpoint_at` to now and to “today boundaries” computed in Rust

Implementation note:

* Because “today” boundaries depend on local timezone, compute:

  * start_of_today_local -> convert to UTC timestamp
  * start_of_tomorrow_local -> convert to UTC timestamp
    Then query ranges in UTC.

---

## 7. Import/export architecture (knotter-sync)

knotter-sync contains adapters that map between external formats and core types.

### 7.1 vCard (.vcf)

#### Import strategy (MVP)

* Parse each card into an intermediate `VCardContact` structure.
* Map into knotter `ContactCreate` + tags:

  * FN -> display_name
  * EMAIL (all) -> contact_emails (first becomes primary)
  * first TEL -> phone
  * CATEGORIES -> tags (normalized)
* Deduplication:

  * If email matches an existing contact, update that contact.
  * When phone+name matching is enabled, normalize the phone and match by display name + phone.
  * Ambiguous matches create merge candidates for manual resolution.
  * Archived-only matches are skipped with a warning.

Manual merge candidates are created when imports/sync encounter ambiguous matches
(e.g., multiple name matches or duplicate emails). Candidates are resolved via
`knotter merge` or the TUI merge list.
Applying a merge marks the chosen candidate as merged and dismisses any other
open candidates that referenced the removed contact.
Some candidate reasons are marked auto-merge safe (currently duplicate-email and
vcf-ambiguous-phone-name), which enables bulk apply workflows.

Import should return a report:

* created_count
* updated_count
* skipped_count
* warnings (invalid tags, missing FN, etc.)

#### Contact sources (macOS + CardDAV)

Additional sources should convert their data into vCard payloads and reuse the
existing vCard import pipeline. This keeps dedupe logic and mapping consistent.

* macOS Contacts: fetch vCards via the Contacts app (AppleScript / Contacts framework); import enables phone+name matching by default to reduce duplicates when emails are missing.
* CardDAV providers (Gmail, iCloud, etc.): fetch addressbook vCards via CardDAV REPORT.

#### Export strategy (MVP)

* For each contact:

  * emit FN
  * emit EMAIL/TEL if present
  * emit CATEGORIES from tags
* Optional: include custom `X-` fields for knotter metadata:

  * `X-KNOTTER-NEXT-TOUCHPOINT: <unix or iso datetime>`
  * `X-KNOTTER-CADENCE-DAYS: <int>`
  * `BDAY: <YYYY-MM-DD, YYYYMMDD, --MMDD, or --MM-DD>` (birthday when available)
  * `X-KNOTTER-DATE: <kind>|<date>|<label>` (name-day/custom dates and extra/labeled birthdays)

Round-trip expectations must be documented:

* Other apps may ignore X- fields (fine).
* knotter should preserve its own X- fields when re-importing its own export.

### 7.3 Email account sync (IMAP, post-MVP)

Email sync ingests headers from configured IMAP inboxes and maps them into
contact emails + interaction history:

* If an email address matches an existing contact, attach it (and record an email touch).
* If it matches none, create a new contact.
* If it matches a unique display name, merge by adding the email to that contact.
* If it matches multiple display names, stage an archived contact and create merge candidates.
* Duplicate-email conflicts create merge candidates for manual resolution.
* Each new message creates an `InteractionKind::Email` entry.
* Sync is incremental using `email_sync_state` (account/mailbox, UIDVALIDITY, last UID).

### 7.4 Telegram 1:1 sync (snippets-only)

Telegram sync ingests 1:1 user chats (no groups) and stores **snippets only**:

* Each Telegram user maps to a contact via `contact_telegram_accounts` (telegram user id, username, phone, names).
* If a telegram user id is already linked, update metadata + record interactions.
* If no link exists:
  * match by username when available (including contact handles)
  * otherwise (and only when enabled) match by display name
  * ambiguous matches create merge candidates; a staged archived contact holds the telegram id
  * `--messages-only` skips staging and only attaches to unambiguous matches
* Each imported message inserts:
  * `telegram_messages` row for dedupe
  * `InteractionKind::Telegram` with a snippet note
* Sync state is tracked per account + peer via `telegram_sync_state` (last_message_id).
* First-time authentication requires a login code; non-interactive runs can provide
  `KNOTTER_TELEGRAM_CODE` and (for 2FA) `KNOTTER_TELEGRAM_PASSWORD`.

### 7.2 iCalendar (.ics) for touchpoints

knotter uses calendar events as an export mechanism for scheduled touchpoints.

#### Event generation rules

* One event per contact that has `next_touchpoint_at`.
* Summary:

  * `Reach out to {display_name}`
* DTSTART:

  * `next_touchpoint_at` as UTC or local-floating time (choose one; UTC recommended for simplicity)
* Description:

  * tags and/or a short “last interaction” snippet (optional)
* UID:

  * stable and deterministic so repeated exports update the same event:

    * `knotter-{contact_uuid}@local` (or similar)

Export options:

* export window (e.g. next 60 days)
* export due-only

### 7.3 JSON snapshot export

knotter also supports a JSON snapshot export for portability and backups.

Snapshot rules:

* include metadata (export timestamp, app version, schema version, format version)
* include all contacts with tags and full interaction history
* interactions are ordered with most recent first
* archived contacts are included by default with an `--exclude-archived` escape hatch

Import of ICS back into knotter is post-MVP.

---

## 8. Reminders and notifications

knotter supports reminders without requiring a daemon.

### 8.1 Reminder computation

* `knotter remind` queries scheduled contacts and groups by:

  * overdue
  * due today
  * due soon (configurable days)
  * dates today (birthdays/custom dates that occur on the local date)
* “Due soon” threshold is config-driven (default 7).

### 8.2 Notification interface

Define a small trait in a shared place (either core or a small `knotter-notify` module, but keep core free of OS calls):

* `Notifier::send(title: &str, body: &str) -> Result<()>`

Backends:

* Stdout (always available)
* Desktop notification (optional feature flag)
* Email (optional feature flag, SMTP via config/env)

Behavior:

* If desktop notification fails, fall back to stdout (do not crash).
* CLI decides whether to notify (`--notify`) or just print.

### 8.3 System scheduling

knotter intentionally relies on external schedulers:

* cron
* systemd user timers
* (optional) macOS launchd for reminders on macOS

knotter provides stable, script-friendly outputs:

* `--json` mode for automation
* exit codes that reflect success/failure

---

## 9. CLI architecture (knotter-cli)

knotter-cli is a thin coordinator.

Responsibilities:

* parse args into command structs
* open DB + run migrations
* call repositories and core functions
* format output (human or JSON)
* set exit codes

Conventions:

* Human output is readable and stable enough for casual scripting.
* JSON output is versioned or at least documented to avoid breaking users.

Error handling:

* Validate obvious bad inputs at the CLI layer (e.g., invalid date format).
* Let store/core return typed errors; convert to friendly messages.

---

## 10. TUI architecture (knotter-tui)

The TUI is a state machine with explicit modes.

### 10.1 Application state model

`App` holds:

* `mode: Mode`
* `filter_input: String`
* `parsed_filter: Option<ContactFilter>`
* `list: Vec<ContactListItem>`
* `selected_index: usize`
* `detail: Option<ContactDetail>` (selected contact, tags, recent interactions)
* `status_message: Option<String>`
* `error_message: Option<String>`
* config values (soon window, etc.)

### 10.2 Modes

* `List`
* `Detail(contact_id)`
* `FilterEditing`
* `ModalAddContact`
* `ModalEditContact(contact_id)`
* `ModalAddNote(contact_id)`
* `ModalEditTags(contact_id)`
* `ModalSchedule(contact_id)`

Each mode defines:

* allowed keybindings
* how input is interpreted
* which components are rendered
* what side effects occur (DB writes)

### 10.3 Event loop + side effects

Key rules:

* Never block the render loop for “long” operations.
* Use a simple command queue pattern:

  * UI produces `Action`s
  * An executor runs actions (DB calls) and returns results
  * App state updates with results

For MVP, DB ops are usually fast; still, structure code so you can move DB work to a worker thread if needed.

### 10.4 Terminal safety

Always restore terminal state:

* on normal exit
* on panic (install panic hook)
* on ctrl-c

---

## 11. Error handling conventions

### 11.1 Typed errors in libraries

Use `thiserror` in:

* knotter-core
* knotter-store
* knotter-sync

Examples:

* `FilterParseError`
* `DomainError` (invalid tag, invalid name)
* `StoreError` (sqlite error, migration error, not found)
* `SyncError` (parse failure, unsupported fields)

### 11.2 Contextual errors at the edges

In knotter-cli and knotter-tui:

* use `anyhow` (or equivalent) for top-level error aggregation and context
* convert typed errors to user-friendly messages

### 11.3 Error message policy

* core/store/sync errors should be actionable but not overly technical
* include debug details only when verbose logging is enabled

---

## 12. Configuration and paths

### 12.1 XDG paths (Linux/Unix)

* Data:

  * `$XDG_DATA_HOME/knotter/`
  * DB: `knotter.sqlite3`
* Config:

  * `$XDG_CONFIG_HOME/knotter/config.toml`
* Cache:

  * `$XDG_CACHE_HOME/knotter/`

Fallbacks:

* if XDG env vars are missing, use standard defaults under `~/.local/share`, `~/.config`, `~/.cache`.

### 12.2 Config file (TOML)

Config keys (MVP):

* `due_soon_days = 7`
* `default_cadence_days = 30` (optional)
* `notifications.enabled = true/false`
* `notifications.backend = "stdout" | "desktop" | "email"` (email requires `email-notify`)
* `notifications.email.from = "Knotter <knotter@example.com>"`
* `notifications.email.to = ["you@example.com"]`
* `notifications.email.smtp_host = "smtp.example.com"`
* `notifications.email.smtp_port = 587` (optional)
* `notifications.email.username = "user@example.com"` (optional)
* `notifications.email.password_env = "KNOTTER_SMTP_PASSWORD"` (required if username set)
* `notifications.email.subject_prefix = "knotter reminders"` (optional)
* `notifications.email.tls = "start-tls" | "tls" | "none"`
* `notifications.email.timeout_seconds = 20` (optional)
* `interactions.auto_reschedule = true/false` (auto-reschedule on interaction add)
* `loops.default_cadence_days = <int>` (optional, fallback cadence when no tag matches)
* `loops.strategy = "shortest" | "priority"` (how to resolve multiple tag matches)
* `loops.schedule_missing = true/false` (schedule when no `next_touchpoint_at`)
* `loops.anchor = "now" | "created-at" | "last-interaction"`
* `loops.apply_on_tag_change = true/false`
* `loops.override_existing = true/false`
* `[[loops.tags]]` with `tag`, `cadence_days`, optional `priority`

Full config example (all sections + optional fields):

```toml
due_soon_days = 7
default_cadence_days = 30

[notifications]
enabled = false
backend = "stdout"

[notifications.email]
from = "Knotter <knotter@example.com>"
to = ["you@example.com"]
subject_prefix = "knotter reminders"
smtp_host = "smtp.example.com"
smtp_port = 587
username = "user@example.com"
password_env = "KNOTTER_SMTP_PASSWORD"
tls = "start-tls"
timeout_seconds = 20

[interactions]
auto_reschedule = false

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
# Optional: import only a named Contacts group (must already exist).
# group = "Friends"
tag = "personal"

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

[[contacts.telegram_accounts]]
name = "primary"
api_id = 123456
api_hash_env = "KNOTTER_TELEGRAM_API_HASH"
phone = "+15551234567"
session_path = "/home/user/.local/share/knotter/telegram/primary.session"
merge_policy = "name-or-username"
allowlist_user_ids = [123456789]
snippet_len = 160
tag = "telegram"
```

Defaults and validation notes:

* When `notifications.enabled = true`, `notifications.backend = "email"` requires a
  `[notifications.email]` block and the `email-notify` feature.
* When `notifications.enabled = true`, `notifications.backend = "desktop"` requires
  the `desktop-notify` feature.
* `notifications.email.username` and `notifications.email.password_env` must be set together.
* CardDAV sources require `url` and `username`; `password_env` and `tag` are optional.
* Email accounts default to `port = 993`, `mailboxes = ["INBOX"]`, and
  `identities = [username]` when `username` is an email address.
* Telegram accounts require `api_id`, `api_hash_env`, and `phone`. `session_path` is optional.
* Telegram `merge_policy` defaults to `name-or-username`; `snippet_len` defaults to `160`.
* Source/account names are normalized to lowercase and must be unique.

Example loop policy:

```
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

Loop precedence:
* Explicit `cadence_days` on a contact takes precedence unless `loops.override_existing = true`.
* When `cadence_days` is unset, tag rules apply first; the loop default applies when no tag matches.
* When `anchor = "last-interaction"`, scheduling occurs only after an interaction exists.
* `loops.schedule_missing = true` only schedules contacts that have no `next_touchpoint_at`.

Contact source config (optional):

```
[contacts]
[[contacts.sources]]
name = "gmail"
type = "carddav"
url = "https://example.test/carddav/addressbook/"
username = "user@example.com"
password_env = "KNOTTER_GMAIL_PASSWORD"
tag = "gmail"

[[contacts.sources]]
name = "local"
type = "macos"
# Optional: import only a named Contacts group (must already exist).
# group = "Friends"
tag = "personal"
```

Notes:
* `password_env` points to an environment variable so passwords are not stored in plaintext.
* `name` is case-insensitive and must be unique.

Email account sync config (optional):

```
[contacts]
[[contacts.email_accounts]]
name = "gmail"
host = "imap.gmail.com"
port = 993
username = "user@gmail.com"
password_env = "KNOTTER_GMAIL_PASSWORD"
mailboxes = ["INBOX", "[Gmail]/Sent Mail"]
identities = ["user@gmail.com"]
merge_policy = "name-or-email" # or "email-only"
tls = "tls"                    # tls | start-tls | none
tag = "gmail"
```

Telegram account sync config (optional):

```
[contacts]
[[contacts.telegram_accounts]]
name = "primary"
api_id = 123456
api_hash_env = "KNOTTER_TELEGRAM_API_HASH"
phone = "+15551234567"
session_path = "/home/user/.local/share/knotter/telegram/primary.session"
merge_policy = "name-or-username" # or "username-only"
allowlist_user_ids = [123456789]
snippet_len = 160
tag = "telegram"
```

On Unix, config files must be user-readable only (e.g., `chmod 600`).

Config parsing lives outside core (store/cli/tui).

---

## 13. Privacy and security

knotter stores personal notes and contact info.
Minimum expectations:

* DB file should be created with user-only permissions where possible.
* Do not log full notes by default.
* Avoid printing private data in error logs.
* Provide a backup command that uses SQLite's online backup API for a consistent snapshot (safe with WAL).
* If DAV sync is added:

  * never store credentials in plaintext unless explicitly allowed
  * prefer OS keyring integration (post-MVP)

---

## 14. Testing strategy

### 14.1 knotter-core tests

* tag normalization tests
* due-state computation tests (today boundaries)
* filter parser tests (valid and invalid cases)

### 14.2 knotter-store tests

* migration applies from scratch
* CRUD operations
* tag attachment/detachment
* filter query correctness
* due filtering correctness with known timestamps

### 14.3 knotter-sync tests

* vCard parse + map to core structs
* export vCard is parseable and contains expected fields
* ICS export includes stable UIDs and correct timestamps
* Telegram sync mapping (username normalization + snippet formatting)

### 14.4 CLI/TUI smoke tests (optional MVP)

* CLI integration tests for core flows:

  * add contact -> tag -> schedule -> remind output contains it

---

## 15. Feature flags (recommended)

Feature flags keep optional integrations isolated. Default builds enable
CardDAV, email, and Telegram sync, while notification backends remain opt-in:

* `desktop-notify` feature:

  * enables desktop notifications backend
* `email-notify` feature:

  * enables SMTP notifications backend
* `dav-sync` feature:

  * enables CardDAV import code (post-MVP sync)
* `email-sync` feature:

  * enables IMAP email import/sync
* `telegram-sync` feature:

  * enables Telegram 1:1 import/sync

Use `--no-default-features` for a no-sync build and re-enable features explicitly.

---

## 16. Future: CardDAV/CalDAV sync (post-MVP)

knotter’s sync design should fit this pattern:

* CardDAV import exists (one-way) behind `dav-sync`; full bidirectional sync remains post-MVP.

* A `Source` abstraction for contacts/events:

  * `pull()` -> remote items
  * `push()` -> upload local dirty items
* Local DB remains the source of truth.
* Sync is explicit (manual command) before adding any background behavior.
* Conflict handling policy must be deterministic:

  * “last updated wins” (simple) or
  * mark conflicts and require manual resolution (better, later)

---

## 17. Summary of invariants (quick checklist)

* Contact name is non-empty.
* Tags are normalized identically in all layers.
* Timestamps in DB are UTC unix seconds.
* Filter parsing behavior is identical in CLI/TUI.
* Store uses bound parameters only.
* UI never leaves terminal in a broken state.
* Import/export is deterministic and stable (stable ICS UID, consistent VCF mapping).

---

## Appendix: Suggested “kind” string encoding in DB

To avoid schema churn:

* Store kinds as lowercase strings:

  * `call`, `text`, `hangout`, `email`, `telegram`
* For `Other(s)`:

  * store `other:<normalized>` where `<normalized>` is trimmed and lowercased
* When reading:

  * parse known literals into enum variants
  * parse `other:` prefix into `Other(String)`
  * unknown values map to `Other(raw)` as a forward-compat fallback
