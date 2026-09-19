-- Отмена отправки письма (specs/undo-send.md). Обычная отправка переходит с
-- прямого вызова серверного модуля на очередь операций, поэтому очередь
-- получает срок отмены, происхождение отправки, закреплённый идентификатор
-- письма и ключ запроса.

-- Ключ запроса отправки: повторное нажатие "Отправить" узнаётся по нему и
-- второй операции не создаёт (S-053).
ALTER TABLE outbox_ops ADD COLUMN request_key TEXT;
-- Закреплённый Message-ID: один и тот же при всех попытках передачи одного
-- запроса, иначе сервер получателя не распознал бы повторную доставку (S-045).
ALTER TABLE outbox_ops ADD COLUMN fixed_message_id TEXT;
-- Происхождение отправки: ordinary, scheduled, automatic или external (S-092).
ALTER TABLE outbox_ops ADD COLUMN send_origin TEXT;
-- Срок отмены во всемирном времени: до него работник операцию не берёт (S-015).
ALTER TABLE outbox_ops ADD COLUMN cancel_until TEXT;

-- Один ключ запроса - одна операция отправки. Частичный индекс не мешает
-- операциям остальных видов: у них ключа запроса нет вовсе.
CREATE UNIQUE INDEX idx_outbox_request_key
    ON outbox_ops(request_key)
    WHERE request_key IS NOT NULL AND op_kind = 'send';

-- Отбор готовых операций отправки идёт по ящику, состоянию и сроку отмены.
CREATE INDEX idx_outbox_send_ready
    ON outbox_ops(account_id, status, cancel_until)
    WHERE op_kind = 'send';

-- S-030: состояния отмены и неопределённого итога допускаются только у
-- операций отправки и выросшей из них дозаписи копии. Иначе частичное
-- ограничение очереди первой волны перестало бы закрывать все состояния
-- операций увода: незавершённая операция в новом состоянии не попала бы в него.
CREATE TRIGGER outbox_new_states_insert
BEFORE INSERT ON outbox_ops
WHEN NEW.status IN ('cancelled', 'uncertain')
     AND NEW.op_kind NOT IN ('send', 'append_sent')
BEGIN
    SELECT RAISE(ABORT, 'состояния отмены и неопределённого итога только у отправки');
END;

CREATE TRIGGER outbox_new_states_update
BEFORE UPDATE OF status ON outbox_ops
WHEN NEW.status IN ('cancelled', 'uncertain')
     AND NEW.op_kind NOT IN ('send', 'append_sent')
BEGIN
    SELECT RAISE(ABORT, 'состояния отмены и неопределённого итога только у отправки');
END;

-- Уже лежащие в очереди операции отправки созданы отложенной отправкой по
-- времени: их срок первой попытки выбрал пользователь, и окна отмены у них нет.
UPDATE outbox_ops
   SET send_origin = 'scheduled',
       cancel_until = coalesce(next_attempt_at, created_at)
 WHERE op_kind = 'send' AND send_origin IS NULL;

UPDATE outbox_ops
   SET send_origin = 'ordinary'
 WHERE op_kind = 'append_sent' AND send_origin IS NULL;

-- Ключи запросов отправки: итог запроса хранится вместе с ключом, поэтому
-- повторное нажатие получает прежний ответ, а не вторую отправку (S-053, S-054).
CREATE TABLE send_request_keys (
    request_key  TEXT    PRIMARY KEY,
    account_id   INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    operation_id INTEGER NOT NULL,
    outcome      TEXT    NOT NULL DEFAULT 'queued',
    cancel_until TEXT,
    created_at   TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_send_request_keys_created ON send_request_keys(created_at);
