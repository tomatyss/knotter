-- 002_email_sync.sql
-- Add multi-email support and email sync metadata.

CREATE TABLE IF NOT EXISTS contact_emails (
  contact_id TEXT NOT NULL,
  email TEXT NOT NULL,
  is_primary INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL,
  source TEXT,

  PRIMARY KEY (contact_id, email),
  UNIQUE (email),
  FOREIGN KEY(contact_id) REFERENCES contacts(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_contact_emails_contact_id
  ON contact_emails(contact_id);

CREATE INDEX IF NOT EXISTS idx_contact_emails_email
  ON contact_emails(email);

UPDATE contacts
SET email = LOWER(TRIM(email))
WHERE email IS NOT NULL;

INSERT OR IGNORE INTO contact_emails (contact_id, email, is_primary, created_at, source)
SELECT id, email, 1, created_at, 'legacy'
FROM contacts
WHERE email IS NOT NULL
ORDER BY (archived_at IS NOT NULL) ASC, updated_at DESC;

CREATE TABLE IF NOT EXISTS email_sync_state (
  account TEXT NOT NULL,
  mailbox TEXT NOT NULL,
  uidvalidity INTEGER,
  last_uid INTEGER NOT NULL DEFAULT 0,
  last_seen_at INTEGER,
  PRIMARY KEY (account, mailbox)
);

CREATE TABLE IF NOT EXISTS email_messages (
  account TEXT NOT NULL,
  mailbox TEXT NOT NULL,
  uid INTEGER NOT NULL,
  message_id TEXT,
  contact_id TEXT NOT NULL,
  occurred_at INTEGER NOT NULL,
  direction TEXT NOT NULL,
  subject TEXT,
  created_at INTEGER NOT NULL,
  PRIMARY KEY (account, mailbox, uid),
  FOREIGN KEY(contact_id) REFERENCES contacts(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_email_messages_contact_occurred
  ON email_messages(contact_id, occurred_at DESC);

CREATE INDEX IF NOT EXISTS idx_email_messages_message_id
  ON email_messages(message_id);
