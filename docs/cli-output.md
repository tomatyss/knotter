# knotter CLI output

This document defines the **stable output surface** for the CLI.

## General rules

- IDs are UUID strings (lowercase hex with dashes).
- Timestamps are unix seconds (UTC) in JSON output.
- Human output is intended for terminals and may evolve; JSON output is the stable interface.

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

### JSON for mutating commands

For `add-contact`, `edit-contact`, `schedule`, `clear-schedule`, `add-note`, and `touch`,
JSON output includes the created/updated entity:

- Contact mutations return a serialized `Contact` object.
- Interaction mutations return a serialized `InteractionDto` object.

Note: This output shape may be expanded in the future, but existing fields are stable.
