ALTER TABLE messages ADD COLUMN remote_id TEXT;
CREATE INDEX IF NOT EXISTS idx_messages_remote_id ON messages(account_id, remote_id);
