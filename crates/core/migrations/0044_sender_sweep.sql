-- Автоочистка писем по отправителю (specs/sweep-by-sender.md). Режим
-- "новые сразу" записи здесь не создаёт: он целиком выражается обычным
-- правилом с условием по полю sender_address и живёт в общем списке правил
-- (S-023 - S-025).

CREATE TABLE sender_sweep_rules (
    id                INTEGER PRIMARY KEY,
    -- Канонический адрес: нормализация общая со списками отправителей (S-046).
    address           TEXT    NOT NULL,
    -- Пусто - область всех ящиков; иначе запись живёт и умирает вместе со
    -- своим ящиком (S-026, S-045).
    account_id        INTEGER REFERENCES accounts(id) ON DELETE CASCADE,
    mode              TEXT    NOT NULL CHECK(mode IN ('only_last', 'older_than')),
    -- Число дней обязательно только для режима "старше N дней" и лежит в
    -- границах от 1 до 3650 (S-032).
    days              INTEGER CHECK(days IS NULL OR (days >= 1 AND days <= 3650)),
    sweep_archive     INTEGER NOT NULL DEFAULT 0,
    enabled           INTEGER NOT NULL DEFAULT 1,
    last_full_pass_at TEXT,
    next_check_at     TEXT,
    last_error        TEXT,
    created_at        TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at        TEXT    NOT NULL DEFAULT (datetime('now')),
    CHECK(mode <> 'older_than' OR days IS NOT NULL)
);

-- S-027: две включённые записи для одного адреса и одной области сохранить
-- нельзя - один режим сохранял бы письмо, которое другой сразу уносит.
-- Область всех ящиков записана числом -1: сравнение NULL в уникальном ключе
-- SQLite дублей не ловит.
CREATE UNIQUE INDEX idx_sender_sweep_rules_scope
    ON sender_sweep_rules(coalesce(account_id, -1), lower(address))
    WHERE enabled = 1;

-- Проход уборки: и разовый по согласию пользователя, и продолжающийся проход
-- записи (S-017 - S-021, S-040).
CREATE TABLE sender_sweep_jobs (
    id                INTEGER PRIMARY KEY,
    rule_id           INTEGER REFERENCES sender_sweep_rules(id) ON DELETE CASCADE,
    account_id        INTEGER REFERENCES accounts(id) ON DELETE CASCADE,
    address           TEXT    NOT NULL,
    mode              TEXT    NOT NULL,
    days              INTEGER,
    sweep_archive     INTEGER NOT NULL DEFAULT 0,
    snapshot_key      TEXT    NOT NULL DEFAULT '',
    max_message_id    INTEGER NOT NULL DEFAULT 0,
    state             TEXT    NOT NULL DEFAULT 'pending'
                          CHECK(state IN ('pending', 'running', 'waiting_operation',
                                          'completed', 'cancelled', 'failed')),
    -- Число проверок ожидания чужой операции: их не больше восьми (S-020).
    waits             INTEGER NOT NULL DEFAULT 0,
    next_check_at     TEXT,
    cursor_message_id INTEGER NOT NULL DEFAULT 0,
    found             INTEGER NOT NULL DEFAULT 0,
    queued            INTEGER NOT NULL DEFAULT 0,
    skipped           INTEGER NOT NULL DEFAULT 0,
    failed            INTEGER NOT NULL DEFAULT 0,
    remaining         INTEGER NOT NULL DEFAULT 0,
    last_error        TEXT,
    created_at        TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at        TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_sender_sweep_jobs_state ON sender_sweep_jobs(state, id);
CREATE INDEX idx_sender_sweep_jobs_rule ON sender_sweep_jobs(rule_id);
