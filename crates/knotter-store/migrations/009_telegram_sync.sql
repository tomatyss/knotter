-- 009_telegram_sync.sql

-- Telegram identities linked to contacts (1:1 user chats)
CREATE TABLE IF NOT EXISTS contact_telegram_accounts (
  contact_id TEXT NOT NULL,
  telegram_user_id INTEGER NOT NULL,
  username TEXT,
  phone TEXT,
  first_name TEXT,
  last_name TEXT,
  source TEXT,
  created_at INTEGER NOT NULL,
  PRIMARY KEY (contact_id, telegram_user_id),
  UNIQUE (telegram_user_id),
  FOREIGN KEY(contact_id) REFERENCES contacts(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_contact_telegram_accounts_contact_id
  ON contact_telegram_accounts(contact_id);

CREATE INDEX IF NOT EXISTS idx_contact_telegram_accounts_username
  ON contact_telegram_accounts(username);

CREATE INDEX IF NOT EXISTS idx_contact_telegram_accounts_phone
  ON contact_telegram_accounts(phone);

-- Per-account sync state for Telegram 1:1 chats
CREATE TABLE IF NOT EXISTS telegram_sync_state (
  account TEXT NOT NULL,
  peer_id INTEGER NOT NULL,
  last_message_id INTEGER NOT NULL DEFAULT 0,
  last_seen_at INTEGER,
  PRIMARY KEY (account, peer_id)
);

-- Telegram message history (snippet-only) for dedupe + interaction history
CREATE TABLE IF NOT EXISTS telegram_messages (
  account TEXT NOT NULL,
  peer_id INTEGER NOT NULL,
  message_id INTEGER NOT NULL,
  contact_id TEXT NOT NULL,
  occurred_at INTEGER NOT NULL,
  direction TEXT NOT NULL,
  snippet TEXT,
  created_at INTEGER NOT NULL,
  PRIMARY KEY (account, peer_id, message_id),
  FOREIGN KEY(contact_id) REFERENCES contacts(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_telegram_messages_contact_occurred
  ON telegram_messages(contact_id, occurred_at DESC);
