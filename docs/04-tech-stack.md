# Технологический стек

- Rust workspace: ядро, протоколы, хранение и Tauri-команды.
- Tauri 2 + системный WebView: desktop-интерфейс и интеграция с ОС.
- SQLCipher 4.17.0 (SQLite 3.53.3) через `sqlx`: зашифрованная локальная база,
  WAL и миграции. Версии закреплены в `vendor/libsqlite3-sys` и проверяются при
  сборке и открытии базы.
- Отдельный зашифрованный blob-store: raw MIME, vCard и iCalendar.
- IMAP/SMTP, JMAP, Gmail API, CalDAV/CardDAV и EWS: транспорты провайдеров.
- SQLite FTS5 за трейтом `SearchIndex`: локальный полнотекстовый поиск.
- Axum: loopback-only REST/MCP API.
- Общие JSON-каталоги RU/EN: единая локализация WebView и core.

Секреты аккаунтов и API-клиентов хранятся в системном keychain. Ключи базы и
blob-store не записываются в открытую на диск.
