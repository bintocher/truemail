-- Закрепление хранится на строке письма и не передаётся серверу
-- (specs/pin-message.md).
ALTER TABLE messages ADD COLUMN pinned_at TEXT;

CREATE INDEX idx_messages_pinned
    ON messages(account_id, pinned_at)
    WHERE pinned_at IS NOT NULL;
