-- История получателей по каждому ящику (specs/recipient-history.md). Прежний
-- сбор корреспондентов заводил контакт на каждый встреченный адрес, поэтому в
-- адресной книге пользователя оседали адреса, которых он туда не добавлял.

CREATE TABLE recipient_history (
    id              INTEGER PRIMARY KEY,
    account_id      INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    -- Ключ адреса общий со списками отправителей и автоответом (S-021).
    address_key     TEXT    NOT NULL,
    display_address TEXT    NOT NULL,
    display_name    TEXT    NOT NULL DEFAULT '',
    -- Имя, изменённое пользователем, не перебивается именем из новой отправки.
    name_edited     INTEGER NOT NULL DEFAULT 0,
    last_used_at    TEXT,
    -- Скрытие пользователем и вытеснение пределом различаются намеренно:
    -- вытесненная запись возвращается любым новым обращением (S-051, S-052).
    hidden_by_user  INTEGER NOT NULL DEFAULT 0,
    hidden_at       TEXT,
    evicted         INTEGER NOT NULL DEFAULT 0,
    evicted_at      TEXT,
    created_at      TEXT    NOT NULL DEFAULT (datetime('now')),
    UNIQUE(account_id, address_key)
);

CREATE INDEX idx_recipient_history_visible
    ON recipient_history(account_id, hidden_by_user, evicted, last_used_at);

-- Отметки обращений: на запись хранится не больше 50 последних (S-027).
CREATE TABLE recipient_history_touches (
    id          INTEGER PRIMARY KEY,
    history_id  INTEGER NOT NULL REFERENCES recipient_history(id) ON DELETE CASCADE,
    -- Ключ письма - Message-ID, а при его отсутствии локальный номер письма.
    -- Он и защищает от повторного счёта: пересборка папки меняет номера писем,
    -- а курсор пополнения остаётся только ускорением прохода (S-017, S-018).
    message_key TEXT    NOT NULL,
    used_at     TEXT    NOT NULL,
    UNIQUE(history_id, message_key)
);

CREATE INDEX idx_recipient_touches_age ON recipient_history_touches(history_id, used_at);

-- Отметка собственной отправки: по ней своя копия, появившаяся в папке с ролью
-- sent, узнаётся и второго обращения не добавляет (S-010, S-011).
CREATE TABLE recipient_own_sends (
    id               INTEGER PRIMARY KEY,
    account_id       INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    fixed_message_id TEXT    NOT NULL,
    sent_at          TEXT    NOT NULL DEFAULT (datetime('now')),
    UNIQUE(account_id, fixed_message_id)
);

-- Состояние истории ящика: граница очистки, курсор пополнения и отметка
-- завершённого первичного заполнения (S-008, S-047).
CREATE TABLE recipient_history_state (
    account_id        INTEGER PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
    cleared_at        TEXT,
    cursor_message_id INTEGER NOT NULL DEFAULT 0,
    initial_done      INTEGER NOT NULL DEFAULT 0,
    updated_at        TEXT    NOT NULL DEFAULT (datetime('now'))
);
