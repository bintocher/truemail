# Почтовые протоколы и синхронизация

## Почта

Обычные и Яндекс-аккаунты используют IMAP/SMTP, Gmail — Gmail API для почты,
Outlook/Microsoft 365 — IMAP/SMTP с Microsoft OAuth, self-hosted Exchange — EWS.
Стандартные JMAP-серверы используют JMAP Core, Mail и Submission. Автоконфигурация сначала проверяет известного
провайдера, затем стандартные autoconfig/ISPDB/SRV-механизмы.

Desktop OAuth использует PKCE и loopback callback. Для Яндекса в настройках
OAuth-приложения должен быть зарегистрирован точный адрес
`http://127.0.0.1:34982/oauth/yandex/callback`; его можно заменить при сборке
через `TRUEMAIL_YANDEX_REDIRECT_URI`. Google и Microsoft принимают случайный
loopback-порт. Microsoft Entra приложение должно разрешать public client flow и
делегированные scope `IMAP.AccessAsUser.All`, `SMTP.Send`, `offline_access`.

Синхронизация хранит серверные курсоры: UIDVALIDITY/HIGHESTMODSEQ для IMAP,
`historyId` для Gmail, `queryState`/`Email state` для JMAP, `syncToken`/`ctag`
для DAV. Новый курсор фиксируется только
после успешного сохранения соответствующей порции данных. Ядро не допускает две
одновременные синхронизации одного аккаунта и одного типа данных.

Gmail в desktop-сборке использует лёгкий опрос ID входящих раз в 25 секунд и
дельта-синхронизацию по `historyId`. Серверный `users.watch` не включён: он
требует отдельного Google Cloud Pub/Sub topic/subscription и постоянно
доступного обработчика вне desktop-приложения. Это осознанная граница
local-first версии, а не незавершённый транспорт.

## Календарь и контакты

Яндекс использует CalDAV/CardDAV, Google — Calendar/People/Tasks API. ETag,
sync-token и удалённые идентификаторы сохраняются локально для дельта-обновлений.

Raw MIME, vCard и iCalendar остаются каноническими данными; нормализованные
таблицы предназначены для быстрого UI и поиска.

Спецификация Microsoft OAuth для IMAP/SMTP:
<https://learn.microsoft.com/en-us/exchange/client-developer/legacy-protocols/how-to-authenticate-an-imap-pop-smtp-application-by-using-oauth>.
JMAP discovery и transport реализованы по
<https://www.rfc-editor.org/rfc/rfc8620> и <https://www.rfc-editor.org/rfc/rfc8621>.
