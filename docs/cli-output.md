# Knotter CLI Output

This document defines the **stable output surface** for the CLI.

## General rules

- IDs are UUID strings (lowercase hex with dashes).
- Timestamps are unix seconds (UTC) in JSON output.
- Human output is intended for terminals and may evolve; JSON output is the stable interface.
- Diagnostics are written to stderr; `--verbose` enables debug logs. Sensitive fields should not be logged.

Related docs:
- [Scheduling](scheduling.md) for reminder automation.
- [Import/Export](import-export.md) for vCard/ICS/JSON commands.

## JSON output

Enable JSON output with the global flag `--json`.

### `knotter list --json`

Output: JSON array of contact list items.

Each item matches `ContactListItemDto`:

- `id` (string UUID)
- `display_name` (string)
- `due_state` (string enum: `unscheduled`, `overdue`, `today`, `soon`, `scheduled`)
- `next_touchpoint_at` (number|null, unix seconds UTC)
- `archived_at` (number|null, unix seconds UTC)
- `tags` (array of strings)

Archived contacts are excluded by default. Use `--include-archived` or `--only-archived`
to change this behavior (or filter with `archived:true|false`).

### `knotter remind --json`

Output: JSON object matching `ReminderOutputDto`:

- `overdue` (array of `ContactListItemDto`)
- `today` (array of `ContactListItemDto`)
- `soon` (array of `ContactListItemDto`)

Note: `due_state` and reminder buckets depend on the current `due_soon_days`
setting (CLI flag or config default). In JSON mode, notifications only run when
`--notify` is provided explicitly. When `notifications.backend = "stdout"`,
`--notify --json` returns a non-zero exit code because stdout notifications
cannot run without corrupting JSON output.
When `notifications.backend = "email"`, `--notify` sends email and failures
return a non-zero exit code.

Reminder items include the `archived_at` field from `ContactListItemDto`, but it
will always be null because archived contacts are excluded from reminders.

### `knotter show <id> --json`

Output: JSON object matching `ContactDetailDto`:

- `id`, `display_name`, `email` (primary), `emails` (array), `phone`, `handle`, `timezone`
- `next_touchpoint_at`, `cadence_days`, `created_at`, `updated_at`, `archived_at`
- `tags` (array of strings)
- `recent_interactions` (array of `InteractionDto`)

`InteractionDto` fields:
- `id` (string UUID)
- `occurred_at` (number)
- `kind` (string, one of `call`, `text`, `hangout`, `email`, or `other:<label>`)
- `note` (string)
- `follow_up_at` (number|null)

### `knotter tag ls --json`

Output: JSON array of tag counts:

- `name` (string, normalized)
- `count` (number)

### `knotter tag add/rm --json`

Output: JSON object containing:

- `id` (string UUID)
- `tag` (string, normalized)

### `knotter loops apply --json`

Output: JSON object containing:

- `matched` (number of contacts that matched a loop rule or default)
- `updated` (number of contacts updated)
- `scheduled` (number of contacts scheduled from a missing touchpoint)
- `skipped` (number of contacts skipped)
- `dry_run` (boolean)
- `changes` (array of objects):
  - `id` (string UUID)
  - `display_name` (string)
  - `cadence_before` (number|null)
  - `cadence_after` (number|null)
  - `next_touchpoint_before` (number|null)
  - `next_touchpoint_after` (number|null)
  - `scheduled` (boolean)

### `knotter sync`

`knotter sync` runs all configured contact sources and email accounts, then
applies loops and runs reminders. It does not support `--json`; use individual
commands (`import`, `loops apply`, `remind`) if you need machine-readable output.
Sync is best-effort: it continues after failures, prints warnings to stderr, and
returns a non-zero exit code if any step fails.

### JSON for mutating commands

For `add-contact`, `edit-contact`, `archive-contact`, `unarchive-contact`, `schedule`,
`clear-schedule`, `add-note`, and `touch`,
JSON output includes the created/updated entity:

- Contact mutations return a serialized `Contact` object.
- Interaction mutations return a serialized `InteractionDto` object.

Note: This output shape may be expanded in the future, but existing fields are stable.

When `default_cadence_days` is set in config, `add-contact` uses it if
`--cadence-days` is omitted. If loop rules are configured, they take precedence
over the default cadence when `--cadence-days` is omitted.

Note: `add-note` and `touch` only reschedule the next touchpoint when
`--reschedule` is used or `interactions.auto_reschedule = true` is set in
config.

Note: `next_touchpoint_at` values provided via `add-contact`, `edit-contact`,
or `schedule` must be `now` or later. Date-only inputs are treated as
day-precision (today or later) and are saved as the end of that day.

### `knotter import vcf --json`

Output: JSON object matching `ImportReport`:

- `created` (number)
- `updated` (number)
- `skipped` (number)
- `warnings` (array of strings)
- `dry_run` (boolean)

The same output shape is used for `import macos`, `import carddav`, and `import source`.

### `knotter import email --json`

Output: JSON object matching `EmailImportReport`:

- `accounts`, `mailboxes`
- `messages_seen`, `messages_imported`
- `contacts_created`, `contacts_merged`, `contacts_matched`
- `touches_recorded`
- `warnings` (array of strings)
- `dry_run` (boolean)

### `knotter export vcf/ics --json`

Note: `--json` requires `--out` to avoid mixing JSON with exported data.

Output: JSON object:

- `format` (string: `vcf` or `ics`)
- `count` (number of exported entries)
- `output` (string path)

### `knotter export json`

If `--out` is omitted, the snapshot JSON is written to stdout (regardless of `--json`).
If `--out` is provided, stdout contains a human message by default, or a JSON report
when `--json` is set (same shape as other export commands).

Snapshot JSON output:

- `metadata` object:
  - `exported_at` (number, unix seconds UTC)
  - `app_version` (string)
  - `schema_version` (number)
  - `format_version` (number)
- `contacts` array of objects:
  - contact fields: `id`, `display_name`, `email` (primary), `emails` (array), `phone`, `handle`, `timezone`,
    `next_touchpoint_at`, `cadence_days`, `created_at`, `updated_at`, `archived_at`
  - `tags` (array of strings)
  - `interactions` (array of objects):
    - `id`, `occurred_at`, `created_at`, `kind`, `note`, `follow_up_at`
    - ordered by `occurred_at` descending

Archived contacts are included by default. Use `--exclude-archived` to omit them.

### `knotter backup --json`

If `--out` is omitted, the backup is written to the XDG data dir using a
timestamped filename.

Output: JSON object:

- `output` (string path)
- `size_bytes` (number)

## Exit codes (selected)

- `1` for general failures (I/O, database, unexpected errors).
- `2` for missing resources (e.g., contact not found, missing TUI binary).
- `3` for invalid input (e.g., invalid filter syntax like `due:later`, invalid dates, invalid flags).
