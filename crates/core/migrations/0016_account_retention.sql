-- Глубина локального кэша писем на аккаунт: сколько дней держать письма
-- полностью (raw в blob). 0 = без ограничений (кэшировать всё).
-- По умолчанию 7 дней (неделя) для всех аккаунтов.
ALTER TABLE accounts ADD COLUMN retention_days INTEGER NOT NULL DEFAULT 7;
