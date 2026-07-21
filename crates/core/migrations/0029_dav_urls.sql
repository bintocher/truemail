-- Базовые адреса CalDAV/CardDAV. Задаются вручную либо находятся по RFC 6764
-- (.well-known/caldav, .well-known/carddav). Храним их здесь, чтобы поиск
-- выполнялся один раз на аккаунт, а не при каждой синхронизации.
ALTER TABLE accounts ADD COLUMN caldav_url TEXT;
ALTER TABLE accounts ADD COLUMN carddav_url TEXT;
