-- 004_email_message_dedupe_indexes.sql
-- Enforce Message-ID dedupe across mailboxes; fall back to account+mailbox+uidvalidity+uid when missing.

-- Remove duplicate Message-ID rows per account (keep the earliest row).
DELETE FROM email_messages
WHERE message_id IS NOT NULL
  AND rowid NOT IN (
    SELECT MIN(rowid)
    FROM email_messages
    WHERE message_id IS NOT NULL
    GROUP BY account, message_id
  );

-- Remove duplicate rows with missing Message-ID per account+mailbox+uid.
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
