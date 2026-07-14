-- Значения настроек шифруются на уровне приложения (ChaCha20-Poly1305).
-- После этой миграции Db::migrate преобразует старые UTF-8 значения в
-- версионированные зашифрованные BLOB до запуска пользовательского интерфейса.

ALTER TABLE settings RENAME TO settings_unencrypted;

CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value BLOB NOT NULL
);

INSERT INTO settings (key, value)
SELECT key, CAST(value AS BLOB)
FROM settings_unencrypted;

DROP TABLE settings_unencrypted;
