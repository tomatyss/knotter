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
--match-phone-name # match existing contacts by display name + phone when no email match is found
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
- If `EMAIL` is missing, create a new contact unless `--match-phone-name` finds a display-name + phone match.
- When `--match-phone-name` is set, knotter normalizes phone numbers (digits-only, leading `+` preserved) and matches by display name + phone.
- If multiple contacts share the same email, knotter stages an archived contact and creates merge candidates.
- If multiple contacts match by display name + phone, knotter creates merge candidates between existing contacts.
- Staged contacts only include emails that are not already assigned to other contacts (to satisfy uniqueness).
- If the only match is archived, the import skips the entry and emits a warning.
- Imported tags are merged with existing tags when updating.
Resolve merge candidates via `knotter merge` or the TUI merge list.
Duplicate-email and vcf-ambiguous-phone-name candidates are marked auto-merge safe and can be bulk-applied via `knotter merge apply-all`.

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
- If `--group` is set, the group must already exist in Contacts; omit it to import all contacts.
- The import uses the same vCard mapping rules and dedupe policy as `import vcf`, with phone+name matching enabled by default.

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
- CardDAV import is enabled by default (v0.2.1+). Disable with `--no-default-features` or re-enable with `--features dav-sync`.

## Email account sync (IMAP)

Sync email headers from configured IMAP accounts and record email touches:

```
knotter import email --account gmail
```

Notes:
- Email sync is enabled by default (v0.2.1+). Disable with `--no-default-features` or re-enable with `--features email-sync`.
- Sync reads headers only (From/To/Date/Subject/Message-ID) and does not store bodies.
- If the sender email matches an existing contact, it attaches the email and records an email touch.
- If no match exists, a new contact is created.
- If multiple name matches exist, knotter stages an archived contact and creates merge candidates.
- `--retry-skipped` stops the import run when a header is skipped so you can retry after fixing config or un-archiving contacts.
- If UIDVALIDITY changes and the mailbox contains messages without Message-ID, import will skip the resync (and not update state) to avoid duplicate touches. Use `--force-uidvalidity-resync` to override.

## Telegram sync (1:1, snippets only)

Sync Telegram 1:1 chats and store short snippets:

```
knotter import telegram --account primary
```

Optional flags:

```
--dry-run
--limit <N>        # max messages per user (after last synced)
--contacts-only
--messages-only
--retry-skipped
--tag <tag>
```

Notes:
- Telegram sync is included in default builds. For a no-sync build from source, use
  `--no-default-features`. To enable Telegram in a minimal build, add
  `--features telegram-sync` (plus any other sync features you want).
- Only 1:1 chats are imported; group chats are ignored.
- Snippets are stored (collapsed to a single line); full message bodies are not stored.
- On first sync, knotter will request a login code. Set `KNOTTER_TELEGRAM_CODE` and (if you
  use 2FA) `KNOTTER_TELEGRAM_PASSWORD` to run non-interactively.
- If a Telegram user id is already linked, knotter updates metadata and records touches.
- If no link exists, knotter matches by username (including matching contact handles), then display name;
  ambiguous matches create merge candidates unless `--messages-only` is used.
- `allowlist_user_ids` in config limits sync to specific Telegram user ids.
- `--messages-only` never creates or stages contacts; it only attaches messages to unambiguous matches,
  otherwise it skips the user with a warning.

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
  - `BDAY` (birthday, `YYYY-MM-DD`, `YYYYMMDD`, `--MMDD`, or `--MM-DD`)
  - `X-KNOTTER-DATE` (`kind|date|label` for name-day/custom dates and extra/labeled birthdays)

Archived contacts are excluded from exports.

### Round-trip notes

- Only `FN`, `EMAIL`, `TEL`, `CATEGORIES`, `BDAY`, and `X-KNOTTER-DATE` are exported; other vCard fields are ignored.
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
