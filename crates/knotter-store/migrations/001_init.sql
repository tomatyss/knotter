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

  email TEXT,
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
