# Knotter CLI Output

This document defines the **stable output surface** for the CLI.

## General rules

- IDs are UUID strings (lowercase hex with dashes).
- Timestamps are unix seconds (UTC) in JSON output.
- Human output is intended for terminals and may evolve; JSON output is the stable interface.

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
- `tags` (array of strings)

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

### `knotter show <id> --json`

Output: JSON object matching `ContactDetailDto`:

- `id`, `display_name`, `email`, `phone`, `handle`, `timezone`
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

### JSON for mutating commands

For `add-contact`, `edit-contact`, `schedule`, `clear-schedule`, `add-note`, and `touch`,
JSON output includes the created/updated entity:

- Contact mutations return a serialized `Contact` object.
- Interaction mutations return a serialized `InteractionDto` object.

Note: This output shape may be expanded in the future, but existing fields are stable.

When `default_cadence_days` is set in config, `add-contact` uses it if
`--cadence-days` is omitted.

### `knotter import vcf --json`

Output: JSON object matching `ImportReport`:

- `created` (number)
- `updated` (number)
- `skipped` (number)
- `warnings` (array of strings)

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
  - contact fields: `id`, `display_name`, `email`, `phone`, `handle`, `timezone`,
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

- `3` for invalid filter syntax (e.g., `due:later`).
