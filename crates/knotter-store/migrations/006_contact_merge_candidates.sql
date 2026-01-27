CREATE TABLE IF NOT EXISTS contact_merge_candidates (
  id TEXT PRIMARY KEY,                         -- UUID string
  created_at INTEGER NOT NULL,                 -- unix seconds UTC
  status TEXT NOT NULL,                        -- open|merged|dismissed
  reason TEXT NOT NULL,                        -- import/email/etc
  source TEXT,                                 -- optional source label
  contact_a_id TEXT NOT NULL,
  contact_b_id TEXT NOT NULL,
  preferred_contact_id TEXT,                   -- optional suggestion
  resolved_at INTEGER,                         -- unix seconds UTC (optional)
  CHECK (contact_a_id <> contact_b_id)
);

CREATE INDEX IF NOT EXISTS idx_contact_merge_candidates_status
  ON contact_merge_candidates(status, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_contact_merge_candidates_contact_a
  ON contact_merge_candidates(contact_a_id);

CREATE INDEX IF NOT EXISTS idx_contact_merge_candidates_contact_b
  ON contact_merge_candidates(contact_b_id);

CREATE INDEX IF NOT EXISTS idx_contact_merge_candidates_preferred
  ON contact_merge_candidates(preferred_contact_id);

CREATE UNIQUE INDEX IF NOT EXISTS idx_contact_merge_candidates_pair_open
  ON contact_merge_candidates(contact_a_id, contact_b_id)
  WHERE status = 'open';
