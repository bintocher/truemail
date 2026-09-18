-- Правила обработки почты с группами условий, группами исключений и цепочкой
-- действий (specs/mail-rules-conditions-and-actions.md).
-- Прежние столбцы mail_rules не удаляются и не переписываются: их ограничения
-- CHECK продолжают действовать, а перенос старых правил в группы и действия
-- выполняет прикладной шаг Db::migrate (S-074 - S-078).

-- Версия правила растёт при каждом изменении его условий, действий или области:
-- по ней ручной прогон узнаёт, что правило изменилось после начала задания.
ALTER TABLE mail_rules ADD COLUMN rule_version INTEGER NOT NULL DEFAULT 1;

-- Название стадии, закрывшей письмо (S-008, S-009). Пустое значение означает,
-- что письмо прошло все стадии и доступно следующей. Таблица писем только
-- получает столбец, строки при этом не переписываются.
ALTER TABLE messages ADD COLUMN closed_by_stage TEXT;

-- Состав полей повторяет smart_conditions: тот же разбор значения, единицы и
-- второй границы, но словарь полей у правил свой (S-020, S-021).
CREATE TABLE mail_rule_conditions (
    id             INTEGER PRIMARY KEY,
    rule_id        TEXT    NOT NULL REFERENCES mail_rules(id) ON DELETE CASCADE,
    -- 0 - обычная группа, 1 - группа исключений (S-015, S-017).
    is_exception   INTEGER NOT NULL DEFAULT 0,
    group_index    INTEGER NOT NULL DEFAULT 0,
    group_logic    TEXT    NOT NULL DEFAULT 'all' CHECK(group_logic IN ('all', 'any')),
    position       INTEGER NOT NULL DEFAULT 0,
    field          TEXT    NOT NULL,
    op             TEXT    NOT NULL,
    value          TEXT    NOT NULL DEFAULT '',
    unit           TEXT,
    value2         TEXT
);

CREATE INDEX idx_mail_rule_conditions_rule
    ON mail_rule_conditions(rule_id, is_exception, group_index, position);

-- Действия правила в порядке выполнения (S-036). Ссылки на папку и метку
-- обнуляются вместе с ними, как это уже сделано в 0033: правило остаётся и
-- переходит в состояние needs_attention (S-054).
CREATE TABLE mail_rule_actions (
    id          INTEGER PRIMARY KEY,
    rule_id     TEXT    NOT NULL REFERENCES mail_rules(id) ON DELETE CASCADE,
    position    INTEGER NOT NULL DEFAULT 0,
    kind        TEXT    NOT NULL,
    folder_id   INTEGER REFERENCES folders(id) ON DELETE SET NULL,
    -- Роль папки вместо её номера у правила для всех ящиков (S-045).
    folder_role TEXT,
    label_id    INTEGER REFERENCES labels(id) ON DELETE SET NULL
);

CREATE INDEX idx_mail_rule_actions_rule ON mail_rule_actions(rule_id, position);

-- Долговечное задание ручного прогона (S-066 - S-073): выбранные папки,
-- снимок версий правил, граница набора писем, состояние, курсор и счётчики.
CREATE TABLE mail_rule_runs (
    id                INTEGER PRIMARY KEY,
    folder_ids        TEXT    NOT NULL,
    rule_versions     TEXT    NOT NULL,
    max_message_id    INTEGER NOT NULL DEFAULT 0,
    state             TEXT    NOT NULL DEFAULT 'pending'
                          CHECK(state IN ('pending', 'running', 'done', 'failed')),
    cursor_message_id INTEGER NOT NULL DEFAULT 0,
    scanned           INTEGER NOT NULL DEFAULT 0,
    applied           INTEGER NOT NULL DEFAULT 0,
    queued            INTEGER NOT NULL DEFAULT 0,
    skipped           INTEGER NOT NULL DEFAULT 0,
    remaining         INTEGER NOT NULL DEFAULT 0,
    last_error        TEXT,
    created_at        TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at        TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_mail_rule_runs_state ON mail_rule_runs(state, id);

-- S-004: связь операции с письмом восстанавливается из её собственных данных
-- там, где столбец message_id появился позже самой операции (0012).
UPDATE outbox_ops
   SET message_id = CAST(json_extract(payload, '$.message_id') AS INTEGER)
 WHERE message_id IS NULL
   AND op_kind IN ('move', 'delete')
   AND status IN ('pending', 'processing', 'retry')
   AND json_valid(payload)
   AND json_extract(payload, '$.message_id') IS NOT NULL
   AND EXISTS (
        SELECT 1 FROM messages m
         WHERE m.id = CAST(json_extract(outbox_ops.payload, '$.message_id') AS INTEGER)
   );

-- S-004: дубли удаляются до создания ограничения, иначе миграция не применится
-- у пользователя, которому прежние версии успели поставить два увода на письмо.
-- Остаётся запись с наименьшим номером: она попала в очередь первой.
DELETE FROM outbox_ops
 WHERE op_kind IN ('move', 'delete')
   AND status IN ('pending', 'processing', 'retry')
   AND message_id IS NOT NULL
   AND id NOT IN (
        SELECT min(id) FROM outbox_ops
         WHERE op_kind IN ('move', 'delete')
           AND status IN ('pending', 'processing', 'retry')
           AND message_id IS NOT NULL
         GROUP BY message_id
   );

-- S-003: одна незавершённая операция увода на письмо. Ограничение схемы, а не
-- проверка в коде: стадии, правила и действия пользователя ставят операции из
-- разных мест, и договорённость в коде их не связывает.
CREATE UNIQUE INDEX idx_outbox_single_takeaway ON outbox_ops(message_id)
 WHERE message_id IS NOT NULL
   AND op_kind IN ('move', 'delete')
   AND status IN ('pending', 'processing', 'retry');
