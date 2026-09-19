-- Списки заблокированных и доверенных отправителей (specs/blocked-senders.md).
-- Списки общие для всех ящиков и account_id не содержат, как это уже сделано
-- для image_trust: отправитель, нежелательный в одном ящике, нежелателен и в
-- остальных. По ящикам разделены только задания уборки.
-- Таблица image_trust остаётся отдельной и своего назначения не меняет (S-046).

CREATE TABLE sender_policies (
    id         INTEGER PRIMARY KEY,
    -- Вид записи: адрес целиком или домен (S-006).
    kind       TEXT    NOT NULL CHECK(kind IN ('address', 'domain')),
    -- Каноническое значение: домен в нижнем регистре и в общей форме
    -- международного домена, написание локальной части адреса сохранено
    -- (S-007 - S-011).
    value      TEXT    NOT NULL,
    decision   TEXT    NOT NULL CHECK(decision IN ('blocked', 'trusted')),
    -- Сколько писем уже убрано этой записью: и стадией, и уборкой (S-045).
    swept      INTEGER NOT NULL DEFAULT 0,
    -- Причина отказа стадии или уборки, показанная в разделе отправителей
    -- (S-003, S-048).
    last_error TEXT,
    created_at TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT    NOT NULL DEFAULT (datetime('now'))
);

-- S-043: повторное добавление того же вида и значения возвращает существующую
-- запись, второй такой же не появляется. Сравнение без учёта регистра: регистр
-- локальной части хранится, но различать записи не должен.
CREATE UNIQUE INDEX idx_sender_policies_value ON sender_policies(kind, lower(value));

-- Долговечное задание уборки уже полученных писем (S-031, S-037, S-038).
CREATE TABLE sender_policy_jobs (
    id                INTEGER PRIMARY KEY,
    policy_id         INTEGER NOT NULL REFERENCES sender_policies(id) ON DELETE CASCADE,
    -- Задания разделены по ящикам и удаляются вместе с ящиком (S-049).
    account_id        INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    snapshot_key      TEXT    NOT NULL,
    -- Граница снимка: письма с большим номером пришли после подтверждения и
    -- достаются стадии списков как новые (S-031).
    max_message_id    INTEGER NOT NULL DEFAULT 0,
    state             TEXT    NOT NULL DEFAULT 'pending'
                          CHECK(state IN ('pending', 'running', 'completed', 'cancelled', 'failed')),
    -- Аренда задания: задание с истёкшим временем считается брошенным (S-037).
    lease_expires_at  TEXT,
    cursor_message_id INTEGER NOT NULL DEFAULT 0,
    queued            INTEGER NOT NULL DEFAULT 0,
    skipped           INTEGER NOT NULL DEFAULT 0,
    failed            INTEGER NOT NULL DEFAULT 0,
    remaining         INTEGER NOT NULL DEFAULT 0,
    last_error        TEXT,
    created_at        TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at        TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_sender_policy_jobs_state ON sender_policy_jobs(state, id);
CREATE INDEX idx_sender_policy_jobs_policy ON sender_policy_jobs(policy_id);

-- Курсор стадии разбора писем: стадия берёт только письма с номером больше
-- сохранённого, как это уже делает прогресс правила. Таблица общая для всех
-- стадий первой волны, поэтому заводится один раз здесь.
CREATE TABLE stage_progress (
    stage      TEXT    PRIMARY KEY,
    message_id INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT    NOT NULL DEFAULT (datetime('now'))
);

-- Снимок кандидатов уборки: набор писем, показанный пользователю до
-- подтверждения, и его ключ (blocked-senders.md S-031, ignore-conversation.md
-- S-014, sweep-by-sender.md S-011). Снимок хранится в базе, а не в памяти:
-- подтверждение может прийти после перезапуска программы.
CREATE TABLE stage_snapshots (
    key            TEXT    PRIMARY KEY,
    kind           TEXT    NOT NULL,
    payload        TEXT    NOT NULL,
    max_message_id INTEGER NOT NULL DEFAULT 0,
    total          INTEGER NOT NULL DEFAULT 0,
    created_at     TEXT    NOT NULL DEFAULT (datetime('now'))
);
