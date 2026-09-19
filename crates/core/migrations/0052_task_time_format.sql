-- Сроки дела, записанные выпуском 0.3.0 в виде интерфейса (ISO 8601 с T и Z),
-- приводятся к виду базы: сравнения идут строками, и до этой правки дело не
-- считалось просроченным в день своего срока, а напоминание ждало полуночи
-- (specs/flag-due-dates.md, S-017).
UPDATE message_tasks
   SET start_at    = COALESCE(strftime('%Y-%m-%d %H:%M:%S', start_at), start_at),
       due_at      = COALESCE(strftime('%Y-%m-%d %H:%M:%S', due_at), due_at),
       reminder_at = COALESCE(strftime('%Y-%m-%d %H:%M:%S', reminder_at), reminder_at)
 WHERE start_at LIKE '%T%' OR due_at LIKE '%T%' OR reminder_at LIKE '%T%';
