-- Скрытый локальный контакт остаётся в базе, чтобы автоматический сбор
-- корреспондентов из писем не создавал его заново после удаления.

ALTER TABLE contacts ADD COLUMN hidden INTEGER NOT NULL DEFAULT 0;
