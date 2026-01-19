# Import and Export

This document describes knotter's vCard (.vcf) and iCalendar (.ics) mappings.
For stable CLI output expectations, see [Knotter CLI Output](cli-output.md).

## vCard import

Command:

```
knotter import vcf <file>
```

### Mapping rules

- `FN` → `display_name` (required)
- `EMAIL` (first) → `email`
- `TEL` (first) → `phone`
- `CATEGORIES` → tags (normalized; comma-separated)
- `X-KNOTTER-NEXT-TOUCHPOINT` → `next_touchpoint_at` (unix seconds UTC)
- `X-KNOTTER-CADENCE-DAYS` → `cadence_days`

### Dedupe policy

- If `EMAIL` is present and matches exactly one active contact (case-insensitive), update that contact.
- If `EMAIL` is missing, always create a new contact.
- If multiple contacts share the same email, the import skips the entry and emits a warning.
- If the only match is archived, the import skips the entry and emits a warning.
- Imported tags are merged with existing tags when updating.

### Warnings

Import reports include warnings for:
- missing `FN`
- invalid tag values
- invalid `X-KNOTTER-*` values

## vCard export

Command:

```
knotter export vcf [--out <file>]
```

### Output

- Version: vCard 3.0
- Fields: `FN`, `EMAIL`, `TEL`, `CATEGORIES`
- Optional metadata:
  - `X-KNOTTER-NEXT-TOUCHPOINT` (unix seconds UTC)
  - `X-KNOTTER-CADENCE-DAYS`

Archived contacts are excluded from exports.

### Round-trip notes

- Only `FN`, `EMAIL`, `TEL`, and `CATEGORIES` are exported; other vCard fields are ignored.
- `X-KNOTTER-*` fields are specific to knotter and may be dropped by other apps.

## JSON export (full snapshot)

Command:

```
knotter export json [--out <file>] [--exclude-archived]
```

### Output

- JSON snapshot containing metadata and all contacts.
- Includes tags and full interaction history per contact.
- Interactions are ordered by most recent first.

### Notes

- Archived contacts are included by default; `--exclude-archived` omits them.
- `metadata.format_version` can be used to handle future schema changes.

## iCalendar export (touchpoints)

Command:

```
knotter export ics [--out <file>] [--window-days N]
```

### Output

- One event per contact with `next_touchpoint_at`
- `UID` is stable and derived from the contact UUID
- `SUMMARY`: `Reach out to {name}`
- `DTSTART`: UTC timestamp from `next_touchpoint_at`
- `DESCRIPTION`: tags if present

### Window filtering

When `--window-days` is provided, only events between now and now + N days
are exported (overdue items are skipped). If `--window-days` is omitted,
all contacts with a `next_touchpoint_at` are exported.

Archived contacts are excluded from exports.

### Round-trip notes

- Exported events are one-way snapshots; editing them in a calendar does not update knotter.
