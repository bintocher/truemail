[English](README.md) · **Русский**

<p align="center">
  <img src="assets/brand/truemail-logo.svg" alt="truemail" width="380">
</p>

<p align="center">
  Быстрый, красивый, кроссплатформенный почтовый клиент с открытым исходным кодом на Rust.
</p>

---

Автономная десктоп-программа на IMAP/SMTP/MIME, iCalendar, vCard,
CalDAV/CardDAV. Почта Яндекса и Gmail подключается через OAuth; календари и
контакты сейчас синхронизируются для Яндекса. Локальные данные зашифрованы.

## Запуск разработки

```sh
make setup     # установить tauri-cli и sqlx-cli (один раз)
make dev       # запустить десктоп-приложение (Tauri v2)
```

Миграции SQLCipher-базы применяются автоматически при запуске. На Windows
сборка SQLCipher один раз скачает в `temp/` проверенную portable-сборку
Strawberry Perl, если полноценного Perl нет в `PATH`.

После остановки `make dev` cargo-sweep удаляет только build-артефакты, которыми
не пользовались 30 дней. Актуальный кэш сборки сохраняется. Предварительный
список можно посмотреть командой `make sweep-preview`.

### OAuth Яндекса

Создайте OAuth-приложение Яндекса с типом `Веб-сервисы`, callback URL
`https://oauth.yandex.ru/verification_code` и правами `mail:imap_full`,
`mail:smtp`, `calendar:all`, `directory:read_external_contacts`,
`directory:write_external_contacts`. Публичный OAuth `client_id` задаётся при
сборке или запуске development-версии:

```powershell
$env:TRUEMAIL_YANDEX_CLIENT_ID="идентификатор_приложения"
make dev
```

Либо скопируйте `.env.example` в `.env`, вставьте публичный `client_id` и
запустите `make dev`. Makefile загрузит `.env` перед сборкой Tauri. Файл `.env`
не попадает в Git. `client_secret` desktop-приложению не нужен: OAuth использует
Authorization Code + PKCE.

Секрет приложения в desktop-клиент не добавляется: авторизация использует
Authorization Code с PKCE. OAuth-токены хранятся в системном keychain, а при
первом подключении сразу проверяются IMAP, CalDAV и CardDAV.

### OAuth Gmail

1. В [Google Cloud Console](https://console.cloud.google.com/) создайте проект.
2. В `APIs & Services` → `Library` включите `Gmail API`.
3. В `Google Auth Platform` заполните `Branding` и `Audience`. Для разработки
   оставьте режим `Testing` и добавьте свои Gmail-адреса в `Test users`.
4. В `Data Access` добавьте scope `https://mail.google.com/`. Он необходим для
   IMAP и SMTP XOAUTH2.
5. В `Clients` создайте OAuth client с типом `Desktop app`. Redirect URI вручную
   не добавляется: truemail поднимает временный callback на случайном порту
   `http://127.0.0.1:<port>` согласно installed-app flow Google.
6. Скопируйте только `Client ID` в `.env`:

```dotenv
TRUEMAIL_GOOGLE_CLIENT_ID=123456789-example.apps.googleusercontent.com
```

`Client secret` в репозиторий и desktop-приложение добавлять не нужно. Для
публичного выпуска scope `https://mail.google.com/` требует Google OAuth
restricted-scope verification и, как правило, ежегодной security assessment.
До верификации режим Testing ограничен добавленными тестовыми пользователями,
а refresh token внешнего приложения действует 7 дней.

### Локальное хранилище

В первом визарде пользователь выбирает язык, папку данных и создаёт ключи,
двигая мышью. Постоянные ключи SQLCipher и blob-store выводятся из этого ввода
в сочетании с OS CSPRNG через HKDF и хранятся в keychain. SQLCipher шифрует всю
SQLite-базу, включая метаданные, FTS и WAL; ChaCha20-Poly1305 отдельно шифрует
блобы.

## Структура

```
crates/core/            ядро: модели RFC, транспорт, хранилище, поиск, крипто, API
  migrations/           схема БД (миграции sqlx)
  src/model/              каноническая модель (message, event, contact, account, folder)
  src/backend/             трейт MailBackend + адаптеры Яндекс/Gmail IMAP/SMTP
  src/storage/             SQLCipher + зашифрованный blob-store
  src/crypto/              шифрование хранилища (ключи в keychain)
  src/search/               FTS5-поиск + раскладко-независимое сопоставление
  src/account/              менеджер аккаунтов + автоконфигурация
  src/api/                  модель прав для будущего внешнего API
  src/i18n/                  локализация (Fluent)
apps/desktop/            десктоп-приложение (Tauri v2)
  src-tauri/                бэкенд приложения (команды -> ядро)
  ui/                       фронтенд (index.html + styles.css + app.js), согласно мокапам
locales/                 переводы ru.ftl / en.ftl
```

## Ключевое

- Автономность: локальное хранение, шифрование всего на диске, секреты в keychain.
- Мгновенная доставка писем Яндекса и Gmail через IMAP IDLE с инкрементальной дозагрузкой.
- Календари и контакты Яндекса через CalDAV/CardDAV.
- Простой / Эксперт режим; локализация RU+EN; тёмная и светлая темы на лету.
- Реальная SMTP-отправка, зашифрованные черновики, вложения и отложенная отправка.
- Нейтральный трейт `MailBackend`; сейчас реализованы Яндекс и Gmail.

## Лицензия

Двойное лицензирование: [AGPL-3.0](LICENSE) (открытая) + коммерческая лицензия для
тех, кто не хочет открывать свой код. Подробности — в [LICENSING.md](LICENSING.md).
По коммерческим вопросам: bintocher@yandex.ru.

## Участие в разработке

См. [CONTRIBUTING.md](CONTRIBUTING.md). Вклад принимается на условиях
[CLA.md](CLA.md).

## Безопасность

О том, как сообщить об уязвимости, см. [SECURITY.md](SECURITY.md).

## Поддержать

Проект бесплатный и открытый. [Поддержать разработку](DONATE.md).
