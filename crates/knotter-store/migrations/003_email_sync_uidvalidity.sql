-- 003_email_sync_uidvalidity.sql
-- Add uidvalidity to email message dedupe key and reconcile legacy email rows.

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

-- Reconcile contacts.email with contact_emails for legacy duplicates.
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
