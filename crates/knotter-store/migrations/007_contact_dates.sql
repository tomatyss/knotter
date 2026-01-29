-- 007_contact_dates.sql

CREATE TABLE IF NOT EXISTS contact_dates (
  id TEXT PRIMARY KEY,                         -- UUID string
  contact_id TEXT NOT NULL,
  kind TEXT NOT NULL,                          -- birthday|name_day|custom
  label TEXT NOT NULL DEFAULT '',
  month INTEGER NOT NULL,
  day INTEGER NOT NULL,
  year INTEGER,                                -- optional year
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  source TEXT,
  FOREIGN KEY(contact_id) REFERENCES contacts(id) ON DELETE CASCADE,
  UNIQUE (contact_id, kind, label, month, day),
  CHECK (month >= 1 AND month <= 12),
  CHECK (day >= 1 AND day <= 31)
);

CREATE INDEX IF NOT EXISTS idx_contact_dates_contact_id
  ON contact_dates(contact_id);

CREATE INDEX IF NOT EXISTS idx_contact_dates_month_day
  ON contact_dates(month, day);
