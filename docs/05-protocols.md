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
`historyId` для Gmail, `queryState`/`Email state` для JMAP, `sync-token`/ETag
для DAV и `SyncState` для EWS. Новый курсор фиксируется только после успешного
сохранения соответствующей порции данных. Ядро не допускает две одновременные
синхронизации одного аккаунта даже для разных типов данных, поэтому почта,
календари и контакты не создают конкурирующие транзакции.

IMAP предпочитает QRESYNC (`VANISHED` и изменения флагов), затем CONDSTORE и
UID-диапазон для новых писем. Полный список UID запрашивается только при первом
запуске, смене UIDVALIDITY и суточной сверке удалений. Если расширения
недоступны, остаётся UID-fallback.

Gmail в desktop-сборке использует лёгкий опрос ID входящих раз в 25 секунд и
дельта-синхронизацию по `historyId`. Список и дельта загружают `metadata`
(заголовки, flags и preview) без тел вложений; полный raw MIME загружается
лениво при открытии письма. Серверный `Retry-After` сохраняется по аккаунту в
зашифрованной БД и соблюдается после перезапуска приложения. `users.watch` не включён: он
требует отдельного Google Cloud Pub/Sub topic/subscription и постоянно
доступного обработчика вне desktop-приложения. Это осознанная граница
local-first версии, а не незавершённый транспорт.

## Календарь и контакты

Яндекс использует CalDAV/CardDAV `sync-collection`; изменившиеся ресурсы
загружаются через multiget, удаления приходят tombstone-ответами. При отсутствии
RFC 6578 выполняется сравнение сохранённых ETag, а полный scoped-проход нужен
только для первого запуска или недействительного токена. Google Calendar и
People используют штатные sync-токены. Google Tasks не имеет sync-токена,
поэтому использует `updatedMin` с пятиминутным перекрытием, дедупликацией и
суточной сверкой списка. EWS использует `SyncFolderHierarchy` и
`SyncFolderItems` с отдельным `SyncState` каждой папки/коллекции.

Raw MIME, vCard и iCalendar остаются каноническими данными; нормализованные
таблицы предназначены для быстрого UI и поиска.

Спецификация Microsoft OAuth для IMAP/SMTP:
<https://learn.microsoft.com/en-us/exchange/client-developer/legacy-protocols/how-to-authenticate-an-imap-pop-smtp-application-by-using-oauth>.
JMAP discovery и transport реализованы по
<https://www.rfc-editor.org/rfc/rfc8620> и <https://www.rfc-editor.org/rfc/rfc8621>.
