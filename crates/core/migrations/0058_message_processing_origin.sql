-- Перемещение письма меняет строку, но не делает само письмо новым.
ALTER TABLE messages ADD COLUMN processing_origin_id INTEGER;

-- Новые строки должны получать номера выше уже обработанных, даже после
-- удаления последней строки или очистки старой папки.
CREATE TABLE message_id_sequence (
    singleton INTEGER PRIMARY KEY CHECK (singleton=1),
    last_id INTEGER NOT NULL
);
INSERT INTO message_id_sequence(singleton,last_id)
SELECT 1, max(
    coalesce((SELECT max(id) FROM messages),0),
    coalesce((SELECT max(message_id) FROM stage_progress),0),
    coalesce((SELECT max(progress_message_id) FROM mail_rules),0)
);
