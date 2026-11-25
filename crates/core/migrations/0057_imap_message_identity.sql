-- Признак действительности, с которым сервер выдал именно эту строку письма.
-- Старые строки его не знали: перед обращением по UID проверяется Message-ID.
ALTER TABLE messages ADD COLUMN imap_uidvalidity INTEGER;
