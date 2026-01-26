-- 005_email_message_id_normalize.sql
-- Normalize message_id values and rebuild message dedupe indexes.

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
