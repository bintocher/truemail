-- Явная связь операции с письмом нужна для undo, дедупликации и безопасной
-- очистки очереди. Полный remote locator дополнительно остаётся в payload.
ALTER TABLE outbox_ops ADD COLUMN message_id INTEGER REFERENCES messages(id) ON DELETE CASCADE;
CREATE INDEX IF NOT EXISTS idx_outbox_message ON outbox_ops(message_id, status);
