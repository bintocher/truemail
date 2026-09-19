-- Точный набор писем-кандидатов уборки, повтор отложенных писем и результат
-- возврата (specs/blocked-senders.md S-031, specs/ignore-conversation.md S-014,
-- S-037 - S-043, specs/sweep-by-sender.md S-011, S-019 - S-021, S-035, S-036).

-- Строки снимка: по подтверждению убираются ровно те письма, которые были
-- показаны пользователю. Прежней границы по наибольшему номеру письма для
-- этого мало: под неё попадали и письма, в подсчёте не участвовавшие.
CREATE TABLE stage_snapshot_messages (
    key        TEXT    NOT NULL,
    message_id INTEGER NOT NULL,
    PRIMARY KEY(key, message_id)
) WITHOUT ROWID;

-- Отложенные письма прохода: письмо с чужой незавершённой операцией
-- повторяется по своему номеру, поэтому движение общего курсора не выводит
-- его из остатка навсегда.
CREATE TABLE stage_job_deferrals (
    kind       TEXT    NOT NULL,
    job_id     INTEGER NOT NULL,
    message_id INTEGER NOT NULL,
    PRIMARY KEY(kind, job_id, message_id)
) WITHOUT ROWID;

-- Номер операции возврата: возврат завершается по её результату, а не по
-- постановке в очередь, иначе отчёт обещал бы письмо, оставшееся в корзине.
ALTER TABLE ignored_conversation_moves ADD COLUMN return_operation_id INTEGER;

-- Проход, заведённый приходом письма: полным проходом записи он не считается
-- и суточный срок не сбрасывает.
ALTER TABLE sender_sweep_jobs ADD COLUMN triggered_by_message INTEGER NOT NULL DEFAULT 0;
