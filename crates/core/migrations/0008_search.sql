-- Полнотекстовый поиск (SQLite FTS5) как быстрый старт.
-- При росте ящиков основной индекс переезжает на Tantivy (см. docs/03-architecture.md),
-- за трейтом SearchIndex - таблица остаётся для лёгких инсталляций.

CREATE VIRTUAL TABLE messages_fts USING fts5(
    subject,
    from_text,
    to_text,
    body,
    message_id UNINDEXED,
    tokenize = 'unicode61 remove_diacritics 2'
);

-- Триггеры синхронизации метаданных письма в FTS (тело добавляется отдельно при загрузке).
CREATE TRIGGER messages_fts_ai AFTER INSERT ON messages BEGIN
    INSERT INTO messages_fts(rowid, subject, from_text, to_text, body, message_id)
    VALUES (new.id, new.subject, coalesce(new.from_name,'') || ' ' || coalesce(new.from_addr,''),
            coalesce(new.to_addrs,''), '', new.id);
END;

CREATE TRIGGER messages_fts_ad AFTER DELETE ON messages BEGIN
    DELETE FROM messages_fts WHERE rowid = old.id;
END;

CREATE TRIGGER messages_fts_au AFTER UPDATE ON messages BEGIN
    UPDATE messages_fts
       SET subject = new.subject,
           from_text = coalesce(new.from_name,'') || ' ' || coalesce(new.from_addr,''),
           to_text = coalesce(new.to_addrs,'')
     WHERE rowid = new.id;
END;
