# Import and Export

This document describes knotter's vCard (.vcf) and iCalendar (.ics) mappings.
For stable CLI output expectations, see [Knotter CLI Output](cli-output.md).

## vCard import

Command:

```
knotter import vcf <file>
```

Optional flags:

```
--dry-run          # parse + dedupe, but do not write to the DB
--limit <N>        # only process the first N contacts
--tag <tag>        # add an extra tag to all imported contacts (repeatable)
```

### Mapping rules

- `FN` → `display_name` (required)
- `EMAIL` (all) → contact emails (first becomes primary)
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

## macOS Contacts import

Command:

```
knotter import macos
```

Optional flags:

```
--group <name>     # only import contacts from a specific Contacts.app group
--dry-run
--limit <N>
--tag <tag>
```

Notes:

- The first run will prompt for Contacts access on macOS.
- The import uses the same vCard mapping rules and dedupe policy as `import vcf`.

## CardDAV import (Gmail, iCloud, and other providers)

Command:

```
knotter import carddav --url <addressbook-url> --username <user> --password-env <ENV>
```

Alternative password input:

```
echo "app-password" | knotter import carddav --url <addressbook-url> --username <user> --password-stdin
```

Optional flags:

```
--dry-run
--limit <N>
--force-uidvalidity-resync
--retry-skipped
--tag <tag>
```

Notes:

- Use the provider’s CardDAV addressbook URL (often listed in their settings docs).
- Some providers require an app-specific password when 2FA is enabled.
- CardDAV import requires the `dav-sync` feature at build time.

## Email account sync (IMAP)

Sync email headers from configured IMAP accounts and record email touches:

```
knotter import email --account gmail
```

Notes:
- Email sync requires the `email-sync` feature at build time.
- Sync reads headers only (From/To/Date/Subject/Message-ID) and does not store bodies.
- If the sender email matches an existing contact, it attaches the email and records an email touch.
- If no match exists, a new contact is created.
- `--retry-skipped` stops the import run when a header is skipped so you can retry after fixing config or un-archiving contacts.
- If UIDVALIDITY changes and the mailbox contains messages without Message-ID, import will skip the resync (and not update state) to avoid duplicate touches. Use `--force-uidvalidity-resync` to override.

## Import sources from config

When you configure contact sources in `config.toml`, you can run:

```
knotter import source <name>
```

The config source can be `carddav` or `macos`, and may include a default `tag`.
See the configuration section in `docs/ARCHITECTURE.md` for the schema.

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
