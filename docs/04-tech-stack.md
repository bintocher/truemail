# Технологический стек

- Rust workspace: ядро, протоколы, хранение и Tauri-команды.
- Tauri 2 + системный WebView: desktop-интерфейс и интеграция с ОС.
- SQLCipher через `sqlx`: зашифрованная SQLite-база, WAL и миграции.
- Отдельный зашифрованный blob-store: raw MIME, vCard и iCalendar.
- IMAP/SMTP, Gmail API, CalDAV/CardDAV и EWS: транспорты провайдеров.
- SQLite FTS5 за трейтом `SearchIndex`: локальный полнотекстовый поиск.
- Axum: loopback-only REST/MCP API.
- Fluent и UI-словарь: текущий переходный слой локализации.

Секреты аккаунтов и API-клиентов хранятся в системном keychain. Ключи базы и
blob-store не записываются в открытую на диск.
