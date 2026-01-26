# Database Schema

## Overview

knotter stores all data locally in a single **SQLite** database file. The database is the **source of truth** (offline-first). This document is the **authoritative schema reference** for the project. The schema is designed to be:

- portable across Linux/Unix machines
- easy to back up (single DB file + possible WAL side files)
- efficient for the most common queries (due touchpoints, name search, tag filtering, interaction history)

knotter intentionally keeps the schema small and stable. Most “behavior” lives in `knotter-core` business rules, not in triggers.

For the broader design context, see [Architecture](ARCHITECTURE.md).

---

## Storage location

By default, knotter uses XDG base directories:

- **Data dir**: `$XDG_DATA_HOME/knotter/`
  - fallback: `~/.local/share/knotter/`
- **DB file**: `knotter.sqlite3`

So the full default path is typically:

- `~/.local/share/knotter/knotter.sqlite3`

Notes:
- With `PRAGMA journal_mode=WAL`, SQLite will also create:
  - `knotter.sqlite3-wal`
  - `knotter.sqlite3-shm`
  These are normal; backups should consider the entire set (or use SQLite’s backup API via code).

---

## Backups

knotter’s `backup` command creates a consistent SQLite snapshot using the
SQLite online backup API. This is safe with WAL enabled and does not require
closing the database.

---

## Connection pragmas (recommended)

knotter-store sets pragmatic defaults for local-app usage:

- `PRAGMA foreign_keys = ON;`
  - ensures cascading deletes work as intended
- `PRAGMA journal_mode = WAL;`
  - better responsiveness for reads while writing
- `PRAGMA synchronous = NORMAL;`
  - good balance for local apps
- `PRAGMA busy_timeout = 2000;`
  - reduces “database is locked” errors when the app briefly contends with itself (e.g., two processes)

These are not strictly part of “schema,” but they matter for behavior.

---

## Migration model

knotter uses **numbered SQL migrations** in:

`crates/knotter-store/migrations/`

Example:
- `001_init.sql`
- `002_add_whatever.sql`
- `003_more_changes.sql`

### Schema version tracking

A simple schema version table is used:

- `knotter_schema(version INTEGER NOT NULL)`

The migration runner is responsible for:
- creating `knotter_schema` if missing
- inserting an initial version row if needed
- applying migrations in numeric order inside a transaction
- updating `knotter_schema.version` after each applied migration

### Migration rules (knotter conventions)

- Prefer **additive** changes (new columns/tables) over destructive ones.
- Avoid “rewrite everything” migrations.
- Keep data transformations explicit and testable.
- Always add indexes if a new query path is introduced.
- When changing semantics, update [Architecture](ARCHITECTURE.md) and this doc.

---

## Migration: 001_init.sql

This is the MVP schema. It includes:
- contacts
- tags
- contact↔tag links
- interactions (notes/history)
- schema version table

```sql
-- 001_init.sql
-- knotter database schema (initial)

-- Schema version table
CREATE TABLE IF NOT EXISTS knotter_schema (
  version INTEGER NOT NULL
);

-- Contacts
CREATE TABLE IF NOT EXISTS contacts (
  id TEXT PRIMARY KEY,                         -- UUID string
  display_name TEXT NOT NULL,

  email TEXT,                                  -- primary email (optional)
  phone TEXT,
  handle TEXT,
  timezone TEXT,                               -- IANA TZ string (optional)

  next_touchpoint_at INTEGER,                  -- unix seconds UTC
  cadence_days INTEGER,                        -- integer days (optional)

  created_at INTEGER NOT NULL,                 -- unix seconds UTC
  updated_at INTEGER NOT NULL,                 -- unix seconds UTC
  archived_at INTEGER                          -- unix seconds UTC (optional; may be unused in MVP)
);

CREATE INDEX IF NOT EXISTS idx_contacts_display_name
  ON contacts(display_name);

CREATE INDEX IF NOT EXISTS idx_contacts_next_touchpoint_at
  ON contacts(next_touchpoint_at);

CREATE INDEX IF NOT EXISTS idx_contacts_archived_at
  ON contacts(archived_at);

-- Tags (normalized)
CREATE TABLE IF NOT EXISTS tags (
  id TEXT PRIMARY KEY,                         -- UUID string
  name TEXT NOT NULL UNIQUE                    -- normalized lowercase
);

CREATE INDEX IF NOT EXISTS idx_tags_name
  ON tags(name);

-- Contact <-> Tag join
CREATE TABLE IF NOT EXISTS contact_tags (
  contact_id TEXT NOT NULL,
  tag_id TEXT NOT NULL,

  PRIMARY KEY (contact_id, tag_id),
  FOREIGN KEY(contact_id) REFERENCES contacts(id) ON DELETE CASCADE,
  FOREIGN KEY(tag_id) REFERENCES tags(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_contact_tags_contact_id
  ON contact_tags(contact_id);

CREATE INDEX IF NOT EXISTS idx_contact_tags_tag_id
  ON contact_tags(tag_id);

-- Interactions (relationship history)
CREATE TABLE IF NOT EXISTS interactions (
  id TEXT PRIMARY KEY,                         -- UUID string
  contact_id TEXT NOT NULL,

  occurred_at INTEGER NOT NULL,                -- unix seconds UTC
  created_at INTEGER NOT NULL,                 -- unix seconds UTC

  kind TEXT NOT NULL,                          -- "call"|"text"|"hangout"|"email"|"other:<label>"
  note TEXT NOT NULL,
  follow_up_at INTEGER,                        -- unix seconds UTC (optional)

  FOREIGN KEY(contact_id) REFERENCES contacts(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_interactions_contact_occurred
  ON interactions(contact_id, occurred_at DESC);
```

## Migration: 002_email_sync.sql

Adds multi-email support and email sync metadata.

```sql
-- 002_email_sync.sql

-- Contact emails (normalized lowercase)
CREATE TABLE IF NOT EXISTS contact_emails (
  contact_id TEXT NOT NULL,
  email TEXT NOT NULL,
  is_primary INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL,
  source TEXT,                              -- provenance (cli/tui/vcf/<account>/primary)

  PRIMARY KEY (contact_id, email),
  UNIQUE (email),
  FOREIGN KEY(contact_id) REFERENCES contacts(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_contact_emails_contact_id
  ON contact_emails(contact_id);

CREATE INDEX IF NOT EXISTS idx_contact_emails_email
  ON contact_emails(email);

-- Email sync state (per account/mailbox)
CREATE TABLE IF NOT EXISTS email_sync_state (
  account TEXT NOT NULL,
  mailbox TEXT NOT NULL,
  uidvalidity INTEGER,
  last_uid INTEGER NOT NULL DEFAULT 0,
  last_seen_at INTEGER,
  PRIMARY KEY (account, mailbox)
);

-- Email messages (dedupe + touch history)
CREATE TABLE IF NOT EXISTS email_messages (
  account TEXT NOT NULL,
  mailbox TEXT NOT NULL,
  uidvalidity INTEGER NOT NULL DEFAULT 0,
  uid INTEGER NOT NULL,
  message_id TEXT,
  contact_id TEXT NOT NULL,
  occurred_at INTEGER NOT NULL,
  direction TEXT NOT NULL,
  subject TEXT,
  created_at INTEGER NOT NULL,
  PRIMARY KEY (account, mailbox, uidvalidity, uid),
  FOREIGN KEY(contact_id) REFERENCES contacts(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_email_messages_contact_occurred
  ON email_messages(contact_id, occurred_at DESC);

CREATE UNIQUE INDEX IF NOT EXISTS idx_email_messages_account_message_id
  ON email_messages(account, message_id)
  WHERE message_id IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_email_messages_account_mailbox_uidvalidity_uid_null_message_id
  ON email_messages(account, mailbox, uidvalidity, uid)
  WHERE message_id IS NULL;
```

## Migration: 003_email_sync_uidvalidity.sql

Adds uidvalidity to the email message dedupe key and reconciles legacy duplicate emails.

```sql
-- 003_email_sync_uidvalidity.sql

CREATE TABLE IF NOT EXISTS email_messages_new (
  account TEXT NOT NULL,
  mailbox TEXT NOT NULL,
  uidvalidity INTEGER NOT NULL DEFAULT 0,
  uid INTEGER NOT NULL,
  message_id TEXT,
  contact_id TEXT NOT NULL,
  occurred_at INTEGER NOT NULL,
  direction TEXT NOT NULL,
  subject TEXT,
  created_at INTEGER NOT NULL,
  PRIMARY KEY (account, mailbox, uidvalidity, uid),
  FOREIGN KEY(contact_id) REFERENCES contacts(id) ON DELETE CASCADE
);

INSERT INTO email_messages_new
  (account, mailbox, uidvalidity, uid, message_id, contact_id, occurred_at, direction, subject, created_at)
SELECT account, mailbox, 0, uid, message_id, contact_id, occurred_at, direction, subject, created_at
FROM email_messages;

DROP TABLE email_messages;
ALTER TABLE email_messages_new RENAME TO email_messages;

CREATE INDEX IF NOT EXISTS idx_email_messages_contact_occurred
  ON email_messages(contact_id, occurred_at DESC);

CREATE INDEX IF NOT EXISTS idx_email_messages_message_id
  ON email_messages(message_id);

UPDATE contacts
SET email = LOWER(TRIM(email))
WHERE email IS NOT NULL;

INSERT OR IGNORE INTO contact_emails (contact_id, email, is_primary, created_at, source)
SELECT id, email, 1, created_at, 'legacy'
FROM contacts
WHERE email IS NOT NULL
ORDER BY (archived_at IS NOT NULL) ASC, updated_at DESC;

UPDATE contacts
SET email = NULL
WHERE email IS NOT NULL
  AND NOT EXISTS (
    SELECT 1 FROM contact_emails ce
    WHERE ce.contact_id = contacts.id AND ce.email = contacts.email
  );
```

## Migration: 004_email_message_dedupe_indexes.sql

Adds unique indexes for message-id dedupe across mailboxes; falls back to account+mailbox+uidvalidity+uid when message-id is missing.

```sql
-- 004_email_message_dedupe_indexes.sql

DELETE FROM email_messages
WHERE message_id IS NOT NULL
  AND rowid NOT IN (
    SELECT MIN(rowid)
    FROM email_messages
    WHERE message_id IS NOT NULL
    GROUP BY account, message_id
  );

DELETE FROM email_messages
WHERE message_id IS NULL
  AND rowid NOT IN (
    SELECT MIN(rowid)
    FROM email_messages
    WHERE message_id IS NULL
    GROUP BY account, mailbox, uidvalidity, uid
  );

DROP INDEX IF EXISTS idx_email_messages_message_id;

CREATE UNIQUE INDEX IF NOT EXISTS idx_email_messages_account_message_id
  ON email_messages(account, message_id)
  WHERE message_id IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_email_messages_account_mailbox_uidvalidity_uid_null_message_id
  ON email_messages(account, mailbox, uidvalidity, uid)
  WHERE message_id IS NULL;
```

## Migration: 005_email_message_id_normalize.sql

Normalizes existing `message_id` values (trim/angle bracket removal/lowercase) and rebuilds dedupe indexes.

```sql
-- 005_email_message_id_normalize.sql

DROP INDEX IF EXISTS idx_email_messages_account_message_id;
DROP INDEX IF EXISTS idx_email_messages_account_mailbox_uidvalidity_uid_null_message_id;

UPDATE email_messages
SET message_id = LOWER(TRIM(TRIM(message_id), '<>'))
WHERE message_id IS NOT NULL;

UPDATE email_messages
SET message_id = NULL
WHERE message_id IS NOT NULL AND message_id = '';

DELETE FROM email_messages
WHERE message_id IS NOT NULL
  AND rowid NOT IN (
    SELECT MIN(rowid)
    FROM email_messages
    WHERE message_id IS NOT NULL
    GROUP BY account, message_id
  );

DELETE FROM email_messages
WHERE message_id IS NULL
  AND rowid NOT IN (
    SELECT MIN(rowid)
    FROM email_messages
    WHERE message_id IS NULL
    GROUP BY account, mailbox, uidvalidity, uid
  );

CREATE UNIQUE INDEX IF NOT EXISTS idx_email_messages_account_message_id
  ON email_messages(account, message_id)
  WHERE message_id IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_email_messages_account_mailbox_uidvalidity_uid_null_message_id
  ON email_messages(account, mailbox, uidvalidity, uid)
  WHERE message_id IS NULL;
```
