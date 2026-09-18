//! Сценарные проверки очереди отправки (specs/undo-send.md).
//!
//! Каждая проверка проходит сценарий целиком по настоящему пути: настоящая
//! база с применёнными миграциями, настоящее хранилище больших объектов,
//! настоящая очередь операций и настоящая передача письма. Сервера в проверке
//! быть не может, поэтому его место занимает поддельный серверный модуль: он
//! отвечает так, как в проверяемом случае ответил бы настоящий, а проверяемый
//! код о подмене не знает.

use super::Db;
use super::repo::test_storage::{TestDb, open_test_db};
use crate::backend::{
    DiscoveredFolder, FolderSyncCursor, ImapDiscovery, MailBackend, OutgoingAttachment,
    OutgoingMessage, SendFailure, SendOutcome, SendResult,
};
use crate::model::*;
use std::collections::HashMap;
use std::sync::Mutex;

async fn seed_account(db: &Db, email: &str) -> i64 {
    sqlx::query_as::<_, (i64,)>(
        "INSERT INTO accounts(uuid, email, provider, backend_kind, auth_kind)
         VALUES(?, ?, 'generic', 'imap', 'password') RETURNING id",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(email)
    .fetch_one(&db.write_pool)
    .await
    .expect("создать ящик")
    .0
}

fn letter(from: &str) -> OutgoingMessage {
    OutgoingMessage {
        from: from.to_owned(),
        to: vec!["boss@partner.test".into()],
        cc: Vec::new(),
        bcc: vec!["secret@partner.test".into()],
        subject: "Договор".into(),
        body_text: "Текст письма".into(),
        body_html: Some("<p>Текст письма</p>".into()),
        attachments: vec![OutgoingAttachment {
            filename: "договор.pdf".into(),
            mime_type: "application/pdf".into(),
            data: vec![7_u8; 2048],
        }],
        ..Default::default()
    }
}

async fn status_of(db: &Db, operation_id: i64) -> String {
    sqlx::query_as::<_, (String,)>("SELECT status FROM outbox_ops WHERE id=?")
        .bind(operation_id)
        .fetch_one(&db.pool)
        .await
        .expect("прочитать состояние операции")
        .0
}

async fn kind_of(db: &Db, operation_id: i64) -> String {
    sqlx::query_as::<_, (String,)>("SELECT op_kind FROM outbox_ops WHERE id=?")
        .bind(operation_id)
        .fetch_one(&db.pool)
        .await
        .expect("прочитать вид операции")
        .0
}

async fn attempts_of(db: &Db, operation_id: i64) -> i64 {
    sqlx::query_as::<_, (i64,)>("SELECT attempts FROM outbox_ops WHERE id=?")
        .bind(operation_id)
        .fetch_one(&db.pool)
        .await
        .expect("прочитать число попыток")
        .0
}

/// Что ответит поддельный серверный модуль на передачу письма.
#[derive(Clone, Copy)]
enum Answer {
    /// Отказ до передачи письма: соединение не установилось.
    FailBeforeHandoff,
    /// Отказ после передачи: письмо ушло, ответа на него нет.
    FailAfterHandoff,
    /// Сервер принял письмо и сам сохранил копию.
    SavedOnServer,
    /// Сервер принял письмо, копию в папку кладёт сама программа.
    NeedsSentAppend,
}

/// Поддельный серверный модуль. Настоящий требует сервера, поэтому только так
/// проверяется весь путь передачи: чтение данных операции, обращение к модулю,
/// разбор его отказа и судьба операции после него.
struct FakeBackend {
    answer: Answer,
    /// Дозапись копии в папку с ролью `sent` отказывает.
    append_fails: bool,
    /// Вид операции, каким его видит дозапись копии: по нему проверяется, что
    /// операция стала дозаписью до обращения к серверу за копией.
    kind_seen_by_append: Mutex<Option<String>>,
    db: Db,
    sends: Mutex<usize>,
}

impl FakeBackend {
    fn new(db: &Db, answer: Answer) -> Self {
        Self {
            answer,
            append_fails: false,
            kind_seen_by_append: Mutex::new(None),
            db: db.clone(),
            sends: Mutex::new(0),
        }
    }

    fn failing_append(db: &Db) -> Self {
        Self {
            append_fails: true,
            ..Self::new(db, Answer::NeedsSentAppend)
        }
    }

    fn sends(&self) -> usize {
        *self.sends.lock().expect("счётчик передач")
    }
}

#[async_trait::async_trait]
impl MailBackend for FakeBackend {
    fn provider_id(&self) -> &'static str {
        "fake"
    }

    async fn validate(&self, _email: &str, _credential: &str) -> crate::Result<()> {
        unimplemented!("проверка учётных данных в сценарии отправки не участвует")
    }

    async fn discover(
        &self,
        _email: &str,
        _credential: &str,
        _cursors: &HashMap<String, FolderSyncCursor>,
        _retention_days: i64,
    ) -> crate::Result<ImapDiscovery> {
        unimplemented!("синхронизация в сценарии отправки не участвует")
    }

    async fn discover_folders(
        &self,
        _email: &str,
        _credential: &str,
    ) -> crate::Result<Vec<DiscoveredFolder>> {
        unimplemented!("перечень папок в сценарии отправки не участвует")
    }

    async fn discover_inbox(
        &self,
        _email: &str,
        _credential: &str,
        _cursors: &HashMap<String, FolderSyncCursor>,
    ) -> crate::Result<ImapDiscovery> {
        unimplemented!("входящие в сценарии отправки не участвуют")
    }

    async fn apply_operation(
        &self,
        _email: &str,
        _credential: &str,
        _operation: &str,
        _payload: &str,
    ) -> crate::Result<()> {
        unimplemented!("операции увода в сценарии отправки не участвуют")
    }

    async fn create_folder(
        &self,
        _email: &str,
        _credential: &str,
        _parent_path: Option<&str>,
        _name: &str,
    ) -> crate::Result<String> {
        unimplemented!("создание папки в сценарии отправки не участвует")
    }

    async fn rename_folder(
        &self,
        _email: &str,
        _credential: &str,
        _remote_path: &str,
        _new_name: &str,
    ) -> crate::Result<String> {
        unimplemented!("переименование папки в сценарии отправки не участвует")
    }

    async fn delete_folder(
        &self,
        _email: &str,
        _credential: &str,
        _remote_path: &str,
    ) -> crate::Result<()> {
        unimplemented!("удаление папки в сценарии отправки не участвует")
    }

    async fn wait_for_change(&self, _email: &str, _credential: &str) -> crate::Result<()> {
        unimplemented!("ожидание изменений в сценарии отправки не участвует")
    }

    async fn send(&self, _message: OutgoingMessage, _credential: &str) -> SendResult {
        *self.sends.lock().expect("счётчик передач") += 1;
        match self.answer {
            Answer::FailBeforeHandoff => Err(SendFailure::before_handoff(
                crate::Error::classified_backend(
                    "fake",
                    crate::ErrorKind::NetworkUnavailable,
                    "соединение не установлено",
                ),
            )),
            Answer::FailAfterHandoff => Err(SendFailure::after_handoff(
                crate::Error::classified_backend(
                    "fake",
                    crate::ErrorKind::NetworkUnavailable,
                    "соединение оборвалось после передачи письма",
                ),
            )),
            Answer::SavedOnServer => Ok(SendOutcome::SavedOnServer),
            Answer::NeedsSentAppend => Ok(SendOutcome::NeedsSentAppend(b"MIME-BYTES".to_vec())),
        }
    }

    async fn append_sent(&self, _email: &str, _credential: &str, _raw: &[u8]) -> crate::Result<()> {
        let kind: Option<(String,)> = sqlx::query_as(
            "SELECT op_kind FROM outbox_ops WHERE op_kind IN ('send','append_sent') LIMIT 1",
        )
        .fetch_optional(&self.db.pool)
        .await
        .expect("прочитать вид операции");
        *self.kind_seen_by_append.lock().expect("вид операции") = kind.map(|(value,)| value);
        if self.append_fails {
            return Err(crate::Error::Other("APPEND отклонён сервером".into()));
        }
        Ok(())
    }

    async fn fetch_message_raw(
        &self,
        _email: &str,
        _credential: &str,
        _folder_path: &str,
        _uid: u32,
        _remote_id: Option<&str>,
    ) -> crate::Result<Vec<u8>> {
        unimplemented!("докачка письма в сценарии отправки не участвует")
    }
}

/// Захватить готовое письмо и передать его поддельным серверным модулем - тем
/// же путём, каким это делает работник очереди.
async fn transmit(db: &Db, account_id: i64, email: &str, backend: &FakeBackend) -> usize {
    let operation = db
        .claim_send_operation(account_id)
        .await
        .expect("захват операции")
        .expect("письмо готово к передаче");
    crate::account::transmit_with_backend(db, email, backend, "token", &operation).await
}

/// Окно отмены от начала до конца: письмо принято в очередь, работник его до
/// срока не берёт, пользователь отменяет, и письмо возвращается целиком - с
/// адресатами, скрытой копией, оформленным телом и вложением
/// (S-001, S-006, S-023, S-037, S-040). Заодно проверяется, что сборка мусора
/// не стирает тело ожидающего письма: иначе первый же запуск отправил бы
/// пустое письмо.
#[tokio::test]
async fn cancelled_message_returns_to_the_composer_with_everything_it_had() {
    let db: TestDb = open_test_db("send-undo").await;
    let account = seed_account(&db, "me@example.test").await;
    let queued = db
        .queue_outgoing_send(
            account,
            letter("me@example.test"),
            SEND_ORIGIN_ORDINARY,
            Some("request-1".into()),
            30,
        )
        .await
        .expect("принять письмо в очередь");
    assert_eq!(queued.status, SEND_STATUS_PENDING);

    // S-023: до срока отмены работник операцию не берёт.
    assert!(
        db.claim_send_operation(account)
            .await
            .expect("захват до срока")
            .is_none(),
        "письмо внутри окна отмены передаче не подлежит"
    );

    // S-007: сборка мусора выполняется при каждом запуске программы. Тело и
    // вложение ожидающего письма должны пережить её.
    let (removed, missing) = db
        .garbage_collect_blobs()
        .await
        .expect("сборка мусора хранилища");
    assert_eq!(removed, 0, "тело и вложение ожидающего письма не удаляются");
    assert!(missing.is_empty(), "ссылки исходящего письма достижимы");

    assert_eq!(
        db.cancel_send_operation(account, queued.operation_id)
            .await
            .expect("отмена отправки"),
        CancelSendOutcome::Cancelled
    );
    // S-039: отменённая операция в захват не попадает.
    assert!(
        db.claim_send_operation(account)
            .await
            .expect("захват после отмены")
            .is_none()
    );

    let restored = db
        .cancelled_send_message(account, queued.operation_id)
        .await
        .expect("вернуть отменённое письмо");
    assert_eq!(restored.to, vec!["boss@partner.test".to_owned()]);
    assert_eq!(restored.bcc, vec!["secret@partner.test".to_owned()]);
    assert_eq!(restored.subject, "Договор");
    assert_eq!(restored.body_html.as_deref(), Some("<p>Текст письма</p>"));
    assert_eq!(restored.attachments.len(), 1);
    assert_eq!(restored.attachments[0].data.len(), 2048);

    // S-043: удаление письма стирает и его большие объекты, иначе содержимое
    // осталось бы в хранилище после явного отказа пользователя.
    db.delete_send_operation(account, queued.operation_id)
        .await
        .expect("удалить отменённое письмо");
    let (removed, _) = db
        .garbage_collect_blobs()
        .await
        .expect("сборка мусора после удаления");
    assert_eq!(removed, 0, "объекты удалённого письма стёрты сразу");
    assert!(
        db.blobs.references().expect("перечень объектов").is_empty(),
        "в хранилище не осталось ни тела, ни вложения"
    );
    db.close().await;
}

/// Программа аварийно завершилась во время передачи письма. При следующем
/// запуске операция получает неопределённый итог и второй раз сама не уходит
/// (S-026, S-027, S-050). Это тот самый случай, ради которого операция отправки
/// исключена из автоматического захвата по истёкшему сроку.
#[tokio::test]
async fn message_caught_by_a_crash_never_leaves_twice() {
    let db: TestDb = open_test_db("send-crash").await;
    let account = seed_account(&db, "me@example.test").await;
    let queued = db
        .queue_outgoing_send(
            account,
            letter("me@example.test"),
            SEND_ORIGIN_ORDINARY,
            None,
            0,
        )
        .await
        .expect("принять письмо в очередь");
    let claimed = db
        .claim_send_operation(account)
        .await
        .expect("захват операции")
        .expect("письмо готово к передаче");
    assert_eq!(claimed.id, queued.operation_id);
    assert_eq!(
        status_of(&db, queued.operation_id).await,
        SEND_STATUS_PROCESSING
    );

    // Срок захвата истёк, как это выглядит после аварийного завершения.
    sqlx::query("UPDATE outbox_ops SET next_attempt_at=datetime('now','-10 minutes') WHERE id=?")
        .bind(queued.operation_id)
        .execute(&db.write_pool)
        .await
        .expect("состарить срок захвата");
    assert!(
        db.claim_send_operation(account)
            .await
            .expect("захват операции в состоянии передачи")
            .is_none(),
        "операция в состоянии передачи в автоматический захват не попадает"
    );

    assert_eq!(
        db.recover_sending_operations()
            .await
            .expect("восстановление при запуске"),
        1
    );
    assert_eq!(
        status_of(&db, queued.operation_id).await,
        SEND_STATUS_UNCERTAIN
    );
    assert!(
        db.claim_send_operation(account)
            .await
            .expect("захват операции с неопределённым итогом")
            .is_none(),
        "неопределённый итог не повторяется без решения пользователя"
    );

    // S-051: пользователь решает сам, а закреплённый идентификатор письма и
    // ключ его запроса при этом сохраняются.
    let (fixed_before, key_before): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT fixed_message_id, request_key FROM outbox_ops WHERE id=?")
            .bind(queued.operation_id)
            .fetch_one(&db.pool)
            .await
            .expect("прочитать идентификаторы письма");
    db.retry_send_operation(account, queued.operation_id)
        .await
        .expect("ручной повтор");
    let (fixed_after, key_after): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT fixed_message_id, request_key FROM outbox_ops WHERE id=?")
            .bind(queued.operation_id)
            .fetch_one(&db.pool)
            .await
            .expect("прочитать идентификаторы после повтора");
    assert_eq!(fixed_before, fixed_after);
    assert_eq!(key_before, key_after, "ключ запроса повтор не обнуляет");
    assert!(
        db.claim_send_operation(account)
            .await
            .expect("захват после решения пользователя")
            .is_some()
    );
    db.close().await;
}

/// Отказ передачи различается по точке, в которой он случился, а не по виду
/// ошибки: до обращения к серверу письмо ждёт повтора и попытку не расходует, а
/// после обращения итог объявляется неизвестным - сервер мог письмо принять
/// (S-028, S-048, S-049).
#[tokio::test]
async fn failure_before_the_handoff_retries_and_failure_after_it_stays_uncertain() {
    let db: TestDb = open_test_db("send-classify").await;
    let account = seed_account(&db, "me@example.test").await;
    let queued = db
        .queue_outgoing_send(
            account,
            letter("me@example.test"),
            SEND_ORIGIN_ORDINARY,
            None,
            0,
        )
        .await
        .expect("принять письмо в очередь");

    // Сеть недоступна сутки: письмо ждёт связи и попыток не тратит.
    let offline = FakeBackend::new(&db, Answer::FailBeforeHandoff);
    for _ in 0..12 {
        assert_eq!(transmit(&db, account, "me@example.test", &offline).await, 0);
        sqlx::query("UPDATE outbox_ops SET next_attempt_at=datetime('now','-1 second') WHERE id=?")
            .bind(queued.operation_id)
            .execute(&db.write_pool)
            .await
            .expect("приблизить срок следующей попытки");
    }
    assert_eq!(offline.sends(), 12);
    assert_eq!(status_of(&db, queued.operation_id).await, SEND_STATUS_RETRY);
    assert_eq!(
        attempts_of(&db, queued.operation_id).await,
        0,
        "недоступность сети попыток не расходует"
    );

    // Связь появилась, но оборвалась уже после передачи письма: повторять
    // нельзя, у получателя оказался бы второй экземпляр.
    let broken = FakeBackend::new(&db, Answer::FailAfterHandoff);
    assert_eq!(transmit(&db, account, "me@example.test", &broken).await, 0);
    assert_eq!(
        status_of(&db, queued.operation_id).await,
        SEND_STATUS_UNCERTAIN
    );
    assert!(
        db.claim_send_operation(account)
            .await
            .expect("захват после неопределённого итога")
            .is_none(),
        "неопределённый итог сам собой не повторяется"
    );
    db.close().await;
}

/// Сервер принял письмо, а сохранение копии в папку с ролью `sent` не удалось.
/// Операция становится дозаписью копии ещё до обращения за копией, поэтому
/// аварийное завершение между подтверждением и дозаписью письма второй раз не
/// отправит; сами байты лежат в хранилище больших объектов (S-009, S-052).
#[tokio::test]
async fn delivered_message_becomes_a_copy_append_before_the_copy_is_attempted() {
    let db: TestDb = open_test_db("send-append").await;
    let account = seed_account(&db, "me@example.test").await;
    let queued = db
        .queue_outgoing_send(
            account,
            letter("me@example.test"),
            SEND_ORIGIN_ORDINARY,
            None,
            0,
        )
        .await
        .expect("принять письмо в очередь");
    let backend = FakeBackend::failing_append(&db);
    assert_eq!(transmit(&db, account, "me@example.test", &backend).await, 1);

    assert_eq!(
        backend
            .kind_seen_by_append
            .lock()
            .expect("вид операции")
            .as_deref(),
        Some("append_sent"),
        "к моменту дозаписи копии операция уже не отправка"
    );
    assert_eq!(kind_of(&db, queued.operation_id).await, "append_sent");
    let (payload,): (String,) = sqlx::query_as("SELECT payload FROM outbox_ops WHERE id=?")
        .bind(queued.operation_id)
        .fetch_one(&db.pool)
        .await
        .expect("прочитать операцию");
    assert!(
        !payload.contains("MIME-BYTES"),
        "байты письма в данные операции не переносятся"
    );
    let parsed: SendPayload = serde_json::from_str(&payload).expect("данные дозаписи копии");
    assert_eq!(
        db.sent_append_bytes(&parsed).expect("байты письма"),
        b"MIME-BYTES".to_vec()
    );
    // S-025, S-052: дозапись копии повторной отправки получателю не создаёт,
    // поэтому работник берёт её обычной пачкой, а не как отправку.
    assert!(
        db.claim_send_operation(account)
            .await
            .expect("захват отправки")
            .is_none()
    );
    sqlx::query("UPDATE outbox_ops SET next_attempt_at=datetime('now','-1 second') WHERE id=?")
        .bind(queued.operation_id)
        .execute(&db.write_pool)
        .await
        .expect("приблизить срок повтора");
    let batch = db
        .claim_outbox_operations(account, 50)
        .await
        .expect("пачка остальных операций");
    assert_eq!(batch.len(), 1);
    assert_eq!(batch[0].op_kind, "append_sent");
    let (removed, missing) = db
        .garbage_collect_blobs()
        .await
        .expect("сборка мусора хранилища");
    assert_eq!(removed, 0, "байты письма и его вложения достижимы");
    assert!(missing.is_empty());

    // Удачная дозапись копии закрывает операцию и освобождает все её объекты,
    // включая точные байты письма.
    db.complete_send_operation(queued.operation_id)
        .await
        .expect("закрыть дозапись копии");
    assert!(
        db.blobs.references().expect("перечень объектов").is_empty(),
        "после успеха хранилище письма не держит"
    );
    db.close().await;
}

/// Подтверждённая отправка закрывает операцию, освобождает письмо целиком и
/// записывает обращения к его адресатам вместе с их именами
/// (S-010, S-047, recipient-history.md S-009, S-038).
#[tokio::test]
async fn confirmed_send_closes_the_operation_and_records_its_recipients() {
    let db: TestDb = open_test_db("send-done").await;
    let account = seed_account(&db, "me@example.test").await;
    let named = OutgoingMessage {
        to: vec!["Борис Борисов <boss@partner.test>".into()],
        ..letter("me@example.test")
    };
    db.queue_outgoing_send(account, named, SEND_ORIGIN_ORDINARY, None, 0)
        .await
        .expect("принять письмо в очередь");
    let backend = FakeBackend::new(&db, Answer::SavedOnServer);
    assert_eq!(transmit(&db, account, "me@example.test", &backend).await, 1);
    let (left,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM outbox_ops WHERE op_kind IN ('send','append_sent')")
            .fetch_one(&db.pool)
            .await
            .expect("прочитать очередь");
    assert_eq!(left, 0, "успешная отправка очередь не занимает");
    assert!(
        db.blobs.references().expect("перечень объектов").is_empty(),
        "после успеха большие объекты письма освобождаются"
    );

    // recipient-history.md S-038: имя адресата лежит в самой строке адреса, и
    // без разбора история показывала бы один адрес.
    let history = db
        .list_recipient_history(account, 100, 0)
        .await
        .expect("история получателей");
    let boss = history
        .iter()
        .find(|entry| entry.address == "boss@partner.test")
        .expect("запись адресата");
    assert_eq!(boss.name, "Борис Борисов");
    assert!(
        history
            .iter()
            .any(|entry| entry.address == "secret@partner.test"),
        "скрытая копия собственной отправки тоже попадает в историю"
    );
    db.close().await;
}

/// Любой отказ после захвата операции оставляет её решённой: письмо не зависает
/// в состоянии передачи до следующего запуска, где его объявили бы
/// неопределённым итогом, хотя серверу его никто не передавал (S-027, S-048).
#[tokio::test]
async fn a_failure_after_the_claim_never_leaves_the_message_in_transmission() {
    let db: TestDb = open_test_db("send-stuck").await;
    let account = seed_account(&db, "me@example.test").await;
    let queued = db
        .queue_outgoing_send(
            account,
            letter("me@example.test"),
            SEND_ORIGIN_ORDINARY,
            None,
            0,
        )
        .await
        .expect("принять письмо в очередь");
    // Большого объекта тела письма больше нет: собрать письмо нечем.
    for reference in db.blobs.references().expect("перечень объектов") {
        let _ = db.blobs.remove(&reference);
    }
    let backend = FakeBackend::new(&db, Answer::SavedOnServer);
    assert_eq!(transmit(&db, account, "me@example.test", &backend).await, 0);
    assert_eq!(
        backend.sends(),
        0,
        "несобранное письмо серверу не передаётся"
    );
    assert_ne!(
        status_of(&db, queued.operation_id).await,
        SEND_STATUS_PROCESSING,
        "операция не осталась в состоянии передачи"
    );
    assert_eq!(
        db.recover_sending_operations()
            .await
            .expect("восстановление при запуске"),
        0,
        "запуску нечего восстанавливать: операция уже решена"
    );
    db.close().await;
}

/// Отмена и захват работником спорят за одну строку. Проигравшая отмена
/// получает точный отказ, а не молчаливое исчезновение действия, и настоящее
/// состояние операции называется своим именем (S-038, S-064).
#[tokio::test]
async fn cancel_that_lost_the_race_names_the_real_state() {
    let db: TestDb = open_test_db("send-race").await;
    let account = seed_account(&db, "me@example.test").await;
    let queued = db
        .queue_outgoing_send(
            account,
            letter("me@example.test"),
            SEND_ORIGIN_ORDINARY,
            None,
            0,
        )
        .await
        .expect("принять письмо в очередь");
    db.claim_send_operation(account)
        .await
        .expect("захват операции")
        .expect("работник успел первым");
    assert_eq!(
        db.cancel_send_operation(account, queued.operation_id)
            .await
            .expect("отмена после начала передачи"),
        CancelSendOutcome::AlreadySending
    );

    // Итог передачи неизвестен: так пользователю и говорится, а не что письмо
    // отправлено.
    db.mark_send_uncertain(queued.operation_id, "связь оборвалась")
        .await
        .expect("неопределённый итог");
    assert_eq!(
        db.cancel_send_operation(account, queued.operation_id)
            .await
            .expect("отмена письма с неизвестным итогом"),
        CancelSendOutcome::Uncertain
    );

    // Повторное нажатие на уже отменённой операции.
    sqlx::query("UPDATE outbox_ops SET status='cancelled', next_attempt_at=NULL WHERE id=?")
        .bind(queued.operation_id)
        .execute(&db.write_pool)
        .await
        .expect("отменить операцию");
    assert_eq!(
        db.cancel_send_operation(account, queued.operation_id)
            .await
            .expect("повторная отмена"),
        CancelSendOutcome::AlreadyCancelled
    );

    // Письма в очереди уже нет - сервер принял его и операция закрыта.
    sqlx::query("DELETE FROM outbox_ops WHERE id=?")
        .bind(queued.operation_id)
        .execute(&db.write_pool)
        .await
        .expect("закрыть операцию успехом");
    assert_eq!(
        db.cancel_send_operation(account, queued.operation_id)
            .await
            .expect("отмена уже отправленного письма"),
        CancelSendOutcome::AlreadySent
    );
    db.close().await;
}

/// Два одновременных нажатия "Отправить" с одним ключом запроса дают одну
/// операцию: второй вызов узнаёт о первом даже тогда, когда оба дошли до записи
/// (S-053).
#[tokio::test]
async fn two_simultaneous_presses_with_one_request_key_give_one_send() {
    let db: TestDb = open_test_db("send-idempotent").await;
    let account = seed_account(&db, "me@example.test").await;
    let first = db.queue_outgoing_send(
        account,
        letter("me@example.test"),
        SEND_ORIGIN_ORDINARY,
        Some("same-key".into()),
        0,
    );
    let second = db.queue_outgoing_send(
        account,
        letter("me@example.test"),
        SEND_ORIGIN_ORDINARY,
        Some("same-key".into()),
        0,
    );
    let (first, second) = tokio::join!(first, second);
    let first = first.expect("первое нажатие");
    let second = second.expect("второе нажатие");
    assert_eq!(
        first.operation_id, second.operation_id,
        "оба нажатия относятся к одной операции"
    );
    let (count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM outbox_ops WHERE op_kind='send' AND account_id=?")
            .bind(account)
            .fetch_one(&db.pool)
            .await
            .expect("прочитать очередь");
    assert_eq!(count, 1, "второй отправки не появилось");
    // Объекты отвергнутого запроса в хранилище не остаются, а объекты принятого
    // письма остаются достижимыми.
    let (removed, missing) = db
        .garbage_collect_blobs()
        .await
        .expect("сборка мусора хранилища");
    assert_eq!(removed, 0, "мусора от второго нажатия нет");
    assert!(missing.is_empty(), "ссылки принятого письма достижимы");
    db.close().await;
}

/// Выход из программы внутри окна отмены отправляет только те письма, у которых
/// это окно есть. Письмо, назначенное пользователем на будущее, уходит в своё
/// время, а не вечером в пятницу вместе с остальными (S-031, S-036, S-055).
#[tokio::test]
async fn quitting_releases_undo_windows_and_leaves_the_scheduled_message_alone() {
    let db: TestDb = open_test_db("send-quit").await;
    let account = seed_account(&db, "me@example.test").await;
    let ordinary = db
        .queue_outgoing_send(
            account,
            letter("me@example.test"),
            SEND_ORIGIN_ORDINARY,
            None,
            30,
        )
        .await
        .expect("обычное письмо в окне отмены");
    let scheduled = db
        .queue_scheduled_send(account, letter("me@example.test"), "2099-01-01 00:00:00")
        .await
        .expect("письмо, назначенное на будущее");

    // S-036: пользователю называется число писем, ждущих именно окна отмены.
    let state = db
        .startup_send_state()
        .await
        .expect("состояние очереди при запуске");
    assert_eq!(state.pending.len(), 1, "окна отмены ждёт одно письмо");
    assert_eq!(state.pending[0].operation_id, ordinary.operation_id);
    assert_eq!(state.expired, 0, "истёкших окон отмены нет");

    assert_eq!(
        db.release_undo_windows()
            .await
            .expect("отпустить окна отмены при выходе"),
        1
    );
    let claimed = db
        .claim_send_operation(account)
        .await
        .expect("захват после выхода")
        .expect("обычное письмо уходит сейчас");
    assert_eq!(claimed.id, ordinary.operation_id);
    assert!(
        db.claim_send_operation(account)
            .await
            .expect("захват отложенного письма")
            .is_none(),
        "письмо, назначенное на будущее, при выходе не уходит"
    );
    let (cancel_until,): (String,) =
        sqlx::query_as("SELECT cancel_until FROM outbox_ops WHERE id=?")
            .bind(scheduled.operation_id)
            .fetch_one(&db.pool)
            .await
            .expect("прочитать срок отложенного письма");
    assert_eq!(cancel_until, "2099-01-01 00:00:00");
    db.close().await;
}

/// Отложенная отправка получает своё время одной записью и до этого времени
/// работнику недоступна вовсе (S-008, S-055, S-056). Новые состояния очереди
/// при этом остаются только у отправки: операции увода их не принимают, иначе
/// частичное ограничение очереди перестало бы закрывать все их состояния
/// (S-030).
#[tokio::test]
async fn scheduled_send_is_written_with_its_time_at_once() {
    let db: TestDb = open_test_db("send-scheduled").await;
    let account = seed_account(&db, "me@example.test").await;
    let queued = db
        .queue_scheduled_send(account, letter("me@example.test"), "2099-01-01 00:00:00")
        .await
        .expect("отложенная отправка");
    assert_eq!(queued.cancel_until, "2099-01-01 00:00:00");
    // Промежуточного состояния с немедленным сроком у строки не было: оба срока
    // сразу указывают на выбранное пользователем время.
    let (cancel_until, next_attempt): (String, Option<String>) =
        sqlx::query_as("SELECT cancel_until, next_attempt_at FROM outbox_ops WHERE id=?")
            .bind(queued.operation_id)
            .fetch_one(&db.pool)
            .await
            .expect("прочитать сроки операции");
    assert_eq!(cancel_until, "2099-01-01 00:00:00");
    assert_eq!(next_attempt.as_deref(), Some("2099-01-01 00:00:00"));
    assert!(
        db.claim_send_operation(account)
            .await
            .expect("захват до заданного времени")
            .is_none()
    );
    assert_eq!(
        db.cancel_send_operation(account, queued.operation_id)
            .await
            .expect("отмена отложенной отправки"),
        CancelSendOutcome::Cancelled
    );

    let folder = sqlx::query_as::<_, (i64,)>(
        "INSERT INTO folders(account_id, remote_path, display_name, role)
         VALUES(?, 'INBOX', 'INBOX', 'inbox') RETURNING id",
    )
    .bind(account)
    .fetch_one(&db.write_pool)
    .await
    .expect("создать папку")
    .0;
    let message = sqlx::query_as::<_, (i64,)>(
        "INSERT INTO messages(account_id, folder_id, uid, subject, size)
         VALUES(?, ?, 1, 'письмо', 10) RETURNING id",
    )
    .bind(account)
    .bind(folder)
    .fetch_one(&db.write_pool)
    .await
    .expect("создать письмо")
    .0;
    sqlx::query(
        "INSERT INTO outbox_ops(account_id, message_id, op_kind, payload, status)
         VALUES(?, ?, 'move', '{}', 'pending')",
    )
    .bind(account)
    .bind(message)
    .execute(&db.write_pool)
    .await
    .expect("поставить операцию увода");
    let forbidden = sqlx::query(
        "UPDATE outbox_ops SET status='cancelled' WHERE op_kind='move' AND message_id=?",
    )
    .bind(message)
    .execute(&db.write_pool)
    .await;
    assert!(
        forbidden.is_err(),
        "состояние отмены операции увода схема принимать не должна"
    );
    db.close().await;
}

/// Удаление письма из раздела "Исходящие" спорит с захватом работника тем же
/// изменением, которым удаляет строку: захваченное письмо удалить нельзя, и его
/// большие объекты остаются на месте (S-043).
#[tokio::test]
async fn deleting_a_claimed_message_is_refused_and_keeps_its_blobs() {
    let db: TestDb = open_test_db("send-delete").await;
    let account = seed_account(&db, "me@example.test").await;
    let queued = db
        .queue_outgoing_send(
            account,
            letter("me@example.test"),
            SEND_ORIGIN_ORDINARY,
            None,
            0,
        )
        .await
        .expect("принять письмо в очередь");
    db.claim_send_operation(account)
        .await
        .expect("захват операции")
        .expect("работник успел первым");
    assert!(
        db.delete_send_operation(account, queued.operation_id)
            .await
            .is_err(),
        "захваченное письмо удалить нельзя"
    );
    let (removed, missing) = db
        .garbage_collect_blobs()
        .await
        .expect("сборка мусора хранилища");
    assert_eq!(removed, 0, "объекты передаваемого письма на месте");
    assert!(missing.is_empty());
    assert_eq!(
        status_of(&db, queued.operation_id).await,
        SEND_STATUS_PROCESSING
    );

    // Письмо вернулось в очередь - теперь его можно удалить вместе с объектами.
    db.mark_send_uncertain(queued.operation_id, "связь оборвалась")
        .await
        .expect("неопределённый итог");
    db.delete_send_operation(account, queued.operation_id)
        .await
        .expect("удалить письмо");
    assert!(
        db.blobs.references().expect("перечень объектов").is_empty(),
        "объекты удалённого письма стёрты"
    );
    db.close().await;
}

/// Служебное письмо программы показывается в разделе "Исходящие" только тогда,
/// когда с ним что-то не так: ожидание и передача автоответа пользователя не
/// касаются (S-058).
#[tokio::test]
async fn automatic_message_appears_in_the_outbox_only_when_it_fails() {
    let db: TestDb = open_test_db("send-automatic").await;
    let account = seed_account(&db, "me@example.test").await;
    let automatic = db
        .queue_outgoing_send(
            account,
            letter("me@example.test"),
            SEND_ORIGIN_AUTOMATIC,
            None,
            0,
        )
        .await
        .expect("служебное письмо");
    let ordinary = db
        .queue_outgoing_send(
            account,
            letter("me@example.test"),
            SEND_ORIGIN_ORDINARY,
            None,
            0,
        )
        .await
        .expect("письмо пользователя");
    let visible = db
        .list_outbox_sends(Some(account), 100, 0)
        .await
        .expect("раздел исходящих");
    assert_eq!(
        visible.iter().map(|entry| entry.id).collect::<Vec<_>>(),
        vec![ordinary.operation_id],
        "ожидающее служебное письмо в разделе не показывается"
    );

    // Неопределённый итог и окончательный отказ пользователь видеть должен.
    db.mark_send_uncertain(automatic.operation_id, "связь оборвалась")
        .await
        .expect("неопределённый итог служебного письма");
    let visible = db
        .list_outbox_sends(Some(account), 100, 0)
        .await
        .expect("раздел исходящих");
    assert!(
        visible
            .iter()
            .any(|entry| entry.id == automatic.operation_id),
        "отказавшее служебное письмо показывается"
    );
    db.close().await;
}

/// Операция отправки, созданная прежней версией программы, хранит письмо
/// строкой. При первом чтении она переносится в хранилище больших объектов и
/// получает закреплённый идентификатор письма (S-046, а также раздел о
/// совместимости про формат 1).
#[tokio::test]
async fn legacy_scheduled_payload_moves_to_the_blob_store_and_gets_its_message_id() {
    let db: TestDb = open_test_db("send-legacy").await;
    let account = seed_account(&db, "me@example.test").await;
    let legacy = serde_json::to_string(&letter("me@example.test")).expect("данные старого формата");
    let (operation_id,): (i64,) = sqlx::query_as(
        "INSERT INTO outbox_ops(account_id, op_kind, payload, status, next_attempt_at)
         VALUES(?, 'send', ?, 'pending', datetime('now')) RETURNING id",
    )
    .bind(account)
    .bind(&legacy)
    .fetch_one(&db.write_pool)
    .await
    .expect("операция старого формата");

    let payload = db
        .read_send_payload(operation_id, &legacy)
        .await
        .expect("прочитать данные старого формата");
    assert_eq!(payload.version, SEND_PAYLOAD_VERSION);
    assert!(payload.message_id.starts_with('<'));
    assert_eq!(payload.attachments.len(), 1);
    let message = db
        .outgoing_from_payload(&payload)
        .await
        .expect("собрать письмо обратно");
    assert_eq!(message.body_text, "Текст письма");
    assert_eq!(message.attachments[0].data.len(), 2048);
    assert_eq!(
        message.message_id.as_deref(),
        Some(payload.message_id.as_str())
    );

    // Второе чтение той же операции не создаёт новых объектов и сохраняет тот
    // же идентификатор: иначе каждая попытка давала бы новый Message-ID.
    let (stored,): (String,) = sqlx::query_as("SELECT payload FROM outbox_ops WHERE id=?")
        .bind(operation_id)
        .fetch_one(&db.pool)
        .await
        .expect("прочитать перенесённые данные");
    let again = db
        .read_send_payload(operation_id, &stored)
        .await
        .expect("повторное чтение");
    assert_eq!(again.message_id, payload.message_id);
    assert_eq!(again.body_ref, payload.body_ref);
    let (removed, _) = db
        .garbage_collect_blobs()
        .await
        .expect("сборка мусора хранилища");
    assert_eq!(
        removed, 0,
        "перенесённые объекты достижимы по данным операции"
    );
    db.close().await;
}
