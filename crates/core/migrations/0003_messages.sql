-- Письма, треды, вложения, метки, outbox-очередь операций.
-- Тела/оригиналы RFC5322 хранятся в зашифрованном blob-store (raw_blob_ref),
-- в БД - только метаданные для UI и поиска. См. docs/03-architecture.md.

CREATE TABLE threads (
    id             INTEGER PRIMARY KEY,
    account_id     INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    root_message_id TEXT,
    subject_norm   TEXT,
    last_date      TEXT,
    message_count  INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE messages (
    id                 INTEGER PRIMARY KEY,
    account_id         INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    folder_id          INTEGER NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
    thread_id          INTEGER REFERENCES threads(id) ON DELETE SET NULL,

    uid                INTEGER NOT NULL,            -- IMAP UID
    modseq             INTEGER,                     -- CONDSTORE MODSEQ
    rfc822_message_id  TEXT,                        -- заголовок Message-ID
    in_reply_to        TEXT,
    references_ids     TEXT,                        -- заголовок References

    from_name          TEXT,
    from_addr          TEXT,
    to_addrs           TEXT,                        -- JSON [{name,addr}]
    cc_addrs           TEXT,
    subject            TEXT    NOT NULL DEFAULT '',
    preview            TEXT    NOT NULL DEFAULT '',
    date               TEXT,
    size               INTEGER,

    seen               INTEGER NOT NULL DEFAULT 0,
    flagged            INTEGER NOT NULL DEFAULT 0,
    answered           INTEGER NOT NULL DEFAULT 0,
    draft              INTEGER NOT NULL DEFAULT 0,
    has_attachments    INTEGER NOT NULL DEFAULT 0,

    dkim_pass          INTEGER,                     -- NULL=неизвестно, 0/1
    spf_pass           INTEGER,
    dmarc_pass         INTEGER,

    raw_blob_ref       TEXT,                        -- ссылка на зашифрованный оригинал
    body_fetched       INTEGER NOT NULL DEFAULT 0,
    UNIQUE(folder_id, uid)
);

CREATE INDEX idx_messages_thread ON messages(thread_id);
CREATE INDEX idx_messages_date   ON messages(date);
CREATE INDEX idx_messages_folder ON messages(folder_id);
CREATE INDEX idx_messages_from   ON messages(from_addr);

CREATE TABLE attachments (
    id          INTEGER PRIMARY KEY,
    message_id  INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    filename    TEXT    NOT NULL,
    mime_type   TEXT,
    size        INTEGER,
    content_id  TEXT,
    is_inline   INTEGER NOT NULL DEFAULT 0,
    blob_ref    TEXT,                               -- зашифрованный blob
    fetched     INTEGER NOT NULL DEFAULT 0
);

-- Метки (модель тегов поверх папок, как notmuch)
CREATE TABLE labels (
    id    INTEGER PRIMARY KEY,
    name  TEXT NOT NULL UNIQUE,
    color TEXT
);

CREATE TABLE message_labels (
    message_id INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    label_id   INTEGER NOT NULL REFERENCES labels(id) ON DELETE CASCADE,
    PRIMARY KEY(message_id, label_id)
);

-- Outbox: локальные операции применяются к серверу асинхронно
CREATE TABLE outbox_ops (
    id          INTEGER PRIMARY KEY,
    account_id  INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    op_kind     TEXT    NOT NULL,                   -- flag | move | delete | append | send | unsubscribe
    payload     TEXT    NOT NULL,                   -- JSON
    created_at  TEXT    NOT NULL DEFAULT (datetime('now')),
    attempts    INTEGER NOT NULL DEFAULT 0,
    last_error  TEXT
);
