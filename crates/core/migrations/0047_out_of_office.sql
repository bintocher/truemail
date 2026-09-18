-- Автоответ "нет на месте" (specs/out-of-office.md). Настройка относится к
-- одному ящику и удаляется вместе с ним, поэтому общей таблице настроек она не
-- подходит: та хранит пары ключа и значения без владельца.

CREATE TABLE out_of_office_settings (
    account_id      INTEGER PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
    -- Режим выбирает программа по виду серверного модуля и по сборке (S-002).
    mode            TEXT    NOT NULL CHECK(mode IN ('server', 'local')),
    enabled         INTEGER NOT NULL DEFAULT 0,
    starts_at       TEXT,
    ends_at         TEXT,
    internal_text   TEXT    NOT NULL DEFAULT '',
    external_text   TEXT    NOT NULL DEFAULT '',
    -- Перечень внутренних доменов: не больше 20 канонических значений (S-026).
    internal_domains TEXT   NOT NULL DEFAULT '[]',
    -- Номер версии настройки: по нему узнаётся изменение периода и текстов.
    version         INTEGER NOT NULL DEFAULT 1,
    -- Для серверного режима хранится только последнее подтверждённое сервером
    -- состояние вместе со временем его чтения: показывается всегда оно (S-018).
    server_checked_at TEXT,
    last_error      TEXT,
    -- Письма периода отсутствия старше суток остаются без ответа, и их число
    -- показывается отдельным сообщением в разделе автоответа (S-064, S-065).
    skipped_old     INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT    NOT NULL DEFAULT (datetime('now'))
);

-- Записи ответов: память о том, кому этот ящик уже ответил (S-048).
CREATE TABLE out_of_office_replies (
    id                 INTEGER PRIMARY KEY,
    account_id         INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    -- Ключ адресата - адрес назначения ответа в канонической форме (S-036).
    recipient_key      TEXT    NOT NULL,
    -- Ключ исходного письма: пара ящика и письма - единственный уникальный ключ.
    source_message_key TEXT    NOT NULL,
    operation_id       INTEGER,
    state              TEXT    NOT NULL DEFAULT 'queued'
                           CHECK(state IN ('queued', 'sent', 'failed')),
    replied_at         TEXT    NOT NULL DEFAULT (datetime('now')),
    UNIQUE(account_id, source_message_key)
);

-- Окно молчания в семь суток проверяется запросом внутри той же неделимой
-- операции, что и вставка записи: ограничение схемы не может зависеть от
-- текущего времени (S-052).
CREATE INDEX idx_out_of_office_replies_recipient
    ON out_of_office_replies(account_id, recipient_key, replied_at);

-- Заголовки правил молчания: без них стадия автоответа загружала бы тело
-- письма из сети на каждое новое письмо (S-062, S-063).
ALTER TABLE messages ADD COLUMN is_newsletter INTEGER NOT NULL DEFAULT 0;
ALTER TABLE messages ADD COLUMN auto_submitted TEXT;
ALTER TABLE messages ADD COLUMN precedence TEXT;
ALTER TABLE messages ADD COLUMN return_path_empty INTEGER NOT NULL DEFAULT 0;
ALTER TABLE messages ADD COLUMN auto_response_suppress TEXT;
-- Адрес назначения ответа берётся из заголовка Reply-To, поэтому он хранится
-- рядом с остальными адресами письма в том же виде - перечнем адресов (S-036).
ALTER TABLE messages ADD COLUMN reply_to_addrs TEXT;
-- Признак того, что заголовки правил молчания у этого письма вообще читались:
-- у письма, сохранённого до обновления, и у модуля, который их не отдаёт,
-- соответствующие правила молчания не применяются (S-039).
ALTER TABLE messages ADD COLUMN silence_headers_known INTEGER NOT NULL DEFAULT 0;
