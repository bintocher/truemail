-- Игнорирование переписки (specs/ignore-conversation.md). Опознание идёт по
-- идентификаторам писем, а не по теме: тема совпадает у несвязанных писем, и
-- одно действие уносило бы в корзину чужую почту (S-009).
-- Расчёт thread_id не изменяется: игнорирование опирается на собственный набор
-- идентификаторов (S-012).

CREATE TABLE ignored_conversations (
    id           INTEGER PRIMARY KEY,
    account_id   INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    -- Снимок темы и участников: список остаётся читаемым после того, как
    -- успешное перемещение удалит локальные строки писем (S-047).
    subject      TEXT    NOT NULL DEFAULT '',
    participants TEXT    NOT NULL DEFAULT '',
    state        TEXT    NOT NULL DEFAULT 'enabled'
                     CHECK(state IN ('enabled', 'disabling', 'returning', 'disabled', 'return_failed')),
    -- Набор достиг предела идентификаторов: новые ветви переписки могут снова
    -- появляться во входящих (S-028).
    partial      INTEGER NOT NULL DEFAULT 0,
    last_error   TEXT,
    created_at   TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at   TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_ignored_conversations_account ON ignored_conversations(account_id, state);

CREATE TABLE ignored_conversation_ids (
    id              INTEGER PRIMARY KEY,
    conversation_id INTEGER NOT NULL REFERENCES ignored_conversations(id) ON DELETE CASCADE,
    account_id      INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    message_id      TEXT    NOT NULL
);

-- S-010, S-045: один идентификатор принадлежит одной записи в пределах ящика,
-- а спорная коллизия разбирается подтверждённой цепочкой ссылок.
CREATE UNIQUE INDEX idx_ignored_conversation_ids_value
    ON ignored_conversation_ids(account_id, message_id);
CREATE INDEX idx_ignored_conversation_ids_record
    ON ignored_conversation_ids(conversation_id);

-- Приметы убранного письма: по ним оно ищется в корзине при возврате, потому
-- что серверный модуль нового расположения письма не сообщает (S-020, S-037).
CREATE TABLE ignored_conversation_moves (
    id                  INTEGER PRIMARY KEY,
    conversation_id     INTEGER NOT NULL REFERENCES ignored_conversations(id) ON DELETE CASCADE,
    -- Номер локальной строки письма до перемещения: после успеха он
    -- освобождается и достаётся другому письму, поэтому опознанию не служит.
    message_id          INTEGER,
    source_folder_id    INTEGER,
    from_addr           TEXT,
    message_date        TEXT,
    -- Нормализованный заголовок Message-ID: без него возврат обещать нечем
    -- (S-021).
    header_id           TEXT,
    operation_id        INTEGER,
    move_state          TEXT    NOT NULL DEFAULT 'queued',
    return_state        TEXT    NOT NULL DEFAULT 'pending'
                            CHECK(return_state IN ('pending', 'waiting_sync', 'completed', 'skipped', 'failed')),
    -- Момент запроса возврата: от него отсчитываются 7 суток ожидания (S-042).
    return_requested_at TEXT,
    created_at          TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at          TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_ignored_conversation_moves_record
    ON ignored_conversation_moves(conversation_id, return_state);
-- Для одной записи и одного набора примет письма существует одна запись
-- перемещения: повторная синхронизация того же письма второй не создаёт.
CREATE UNIQUE INDEX idx_ignored_conversation_moves_unique
    ON ignored_conversation_moves(conversation_id, header_id, from_addr, message_date)
    WHERE header_id IS NOT NULL;

-- Долговечное задание уборки или возврата с ключом снимка, курсором и
-- счётчиками (S-022, S-048).
CREATE TABLE ignored_conversation_jobs (
    id                INTEGER PRIMARY KEY,
    conversation_id   INTEGER NOT NULL REFERENCES ignored_conversations(id) ON DELETE CASCADE,
    kind              TEXT    NOT NULL CHECK(kind IN ('sweep', 'return')),
    snapshot_key      TEXT    NOT NULL DEFAULT '',
    max_message_id    INTEGER NOT NULL DEFAULT 0,
    state             TEXT    NOT NULL DEFAULT 'pending'
                          CHECK(state IN ('pending', 'running', 'completed', 'cancelled', 'failed')),
    cursor_message_id INTEGER NOT NULL DEFAULT 0,
    queued            INTEGER NOT NULL DEFAULT 0,
    skipped           INTEGER NOT NULL DEFAULT 0,
    failed            INTEGER NOT NULL DEFAULT 0,
    remaining         INTEGER NOT NULL DEFAULT 0,
    last_error        TEXT,
    created_at        TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at        TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_ignored_conversation_jobs_state ON ignored_conversation_jobs(state, id);
