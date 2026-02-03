-- 010_contact_sources.sql
-- Map external source IDs (e.g., vCard UID) to contacts for idempotent imports.

CREATE TABLE IF NOT EXISTS contact_sources (
  contact_id TEXT NOT NULL,
  source TEXT NOT NULL,
  external_id TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,

  PRIMARY KEY (source, external_id),
  FOREIGN KEY(contact_id) REFERENCES contacts(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_contact_sources_contact_id
  ON contact_sources(contact_id);

CREATE INDEX IF NOT EXISTS idx_contact_sources_source
  ON contact_sources(source);
