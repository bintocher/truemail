//! Сценарные проверки очереди отправки (specs/undo-send.md).
//!
//! Каждая проверка проходит сценарий целиком по настоящему пути: настоящая
//! база с применёнными миграциями, настоящее хранилище больших объектов и
//! настоящая очередь операций. Сервера в проверке быть не может, поэтому его
//! итог передаётся тем же вызовом, каким его сообщает работник очереди.

use super::Db;
use super::repo::test_storage::{TestDb, open_test_db};
use crate::backend::{OutgoingAttachment, OutgoingMessage};
use crate::model::*;

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

async fn attempts_of(db: &Db, operation_id: i64) -> i64 {
    sqlx::query_as::<_, (i64,)>("SELECT attempts FROM outbox_ops WHERE id=?")
        .bind(operation_id)
        .fetch_one(&db.pool)
        .await
        .expect("прочитать число попыток")
        .0
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

    // S-051: пользователь решает сам, и закреплённый идентификатор письма при
    // этом сохраняется.
    let (fixed_before,): (Option<String>,) =
        sqlx::query_as("SELECT fixed_message_id FROM outbox_ops WHERE id=?")
            .bind(queued.operation_id)
            .fetch_one(&db.pool)
            .await
            .expect("прочитать идентификатор письма");
    db.retry_send_operation(account, queued.operation_id)
        .await
        .expect("ручной повтор");
    let (fixed_after,): (Option<String>,) =
        sqlx::query_as("SELECT fixed_message_id FROM outbox_ops WHERE id=?")
            .bind(queued.operation_id)
            .fetch_one(&db.pool)
            .await
            .expect("прочитать идентификатор письма после повтора");
    assert_eq!(fixed_before, fixed_after);
    assert!(
        db.claim_send_operation(account)
            .await
            .expect("захват после решения пользователя")
            .is_some()
    );
    db.close().await;
}

/// Сеть недоступна сутки: попытки не сгорают, и письмо уходит при появлении
/// связи (S-028, S-048). Прежнее поведение сжигало восемь попыток примерно за
/// полтора часа, и обещание "письмо уйдёт при следующем подключении" не
/// выполнялось.
#[tokio::test]
async fn a_day_without_network_does_not_burn_the_attempts() {
    let db: TestDb = open_test_db("send-offline").await;
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
    for _ in 0..12 {
        let claimed = db
            .claim_send_operation(account)
            .await
            .expect("захват операции")
            .expect("письмо готово к передаче");
        db.defer_send_operation(claimed.id, "сеть недоступна")
            .await
            .expect("отложить без расхода попытки");
        // Отсрочка отсчитывается от текущего времени, поэтому для следующего
        // прохода её сдвигаем назад, как это сделало бы само время.
        sqlx::query("UPDATE outbox_ops SET next_attempt_at=datetime('now','-1 second') WHERE id=?")
            .bind(claimed.id)
            .execute(&db.write_pool)
            .await
            .expect("приблизить срок следующей попытки");
    }
    assert_eq!(
        attempts_of(&db, queued.operation_id).await,
        0,
        "недоступность сети попыток не расходует"
    );
    assert_eq!(status_of(&db, queued.operation_id).await, SEND_STATUS_RETRY);

    // Связь появилась: письмо уходит и очередь его отпускает вместе с большими
    // объектами (S-010).
    let claimed = db
        .claim_send_operation(account)
        .await
        .expect("захват операции")
        .expect("письмо готово к передаче");
    db.complete_outbox_operation(&claimed)
        .await
        .expect("успешная передача");
    let (left,): (i64,) = sqlx::query_as("SELECT count(*) FROM outbox_ops WHERE id=?")
        .bind(queued.operation_id)
        .fetch_one(&db.pool)
        .await
        .expect("прочитать очередь");
    assert_eq!(left, 0);
    assert!(
        db.blobs.references().expect("перечень объектов").is_empty(),
        "после успеха большие объекты письма освобождаются"
    );
    db.close().await;
}

/// Отмена и захват работником спорят за одну строку. Проигравшая отмена
/// получает точный отказ, а не молчаливое исчезновение действия (S-038).
#[tokio::test]
async fn cancel_that_lost_the_race_says_the_sending_has_started() {
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

/// Двойное нажатие "Отправить" не создаёт второй отправки, а письмо очереди
/// продолжает передаваться по одной операции за раз (S-025, S-053).
#[tokio::test]
async fn one_request_key_gives_one_send_and_the_worker_takes_them_one_by_one() {
    let db: TestDb = open_test_db("send-idempotent").await;
    let account = seed_account(&db, "me@example.test").await;
    let first = db
        .queue_outgoing_send(
            account,
            letter("me@example.test"),
            SEND_ORIGIN_ORDINARY,
            Some("same-key".into()),
            0,
        )
        .await
        .expect("первое нажатие");
    let second = db
        .queue_outgoing_send(
            account,
            letter("me@example.test"),
            SEND_ORIGIN_ORDINARY,
            Some("same-key".into()),
            0,
        )
        .await
        .expect("повторное нажатие");
    assert!(second.duplicate);
    assert_eq!(first.operation_id, second.operation_id);
    let (count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM outbox_ops WHERE op_kind='send' AND account_id=?")
            .bind(account)
            .fetch_one(&db.pool)
            .await
            .expect("прочитать очередь");
    assert_eq!(count, 1, "второй отправки не появилось");

    // Второе письмо ставится отдельно и ждёт своей очереди: внутри одного
    // прохода работника письма передаются по одному, и второе остаётся
    // ожидающим и доступным для отмены.
    let other = db
        .queue_outgoing_send(
            account,
            letter("me@example.test"),
            SEND_ORIGIN_ORDINARY,
            Some("second-key".into()),
            0,
        )
        .await
        .expect("второе письмо");
    let claimed = db
        .claim_send_operation(account)
        .await
        .expect("захват")
        .expect("первое письмо");
    assert_eq!(claimed.id, first.operation_id);
    assert_eq!(
        status_of(&db, other.operation_id).await,
        SEND_STATUS_PENDING
    );
    assert_eq!(
        db.cancel_send_operation(account, other.operation_id)
            .await
            .expect("отмена ещё не начатой отправки"),
        CancelSendOutcome::Cancelled
    );
    db.close().await;
}

/// Отложенная отправка по времени окна отмены поверх выбранного времени не
/// получает, но отменить её до начала передачи можно (S-055, S-056). Новые
/// состояния очереди при этом остаются только у отправки: операции увода их не
/// принимают, иначе частичное ограничение очереди перестало бы закрывать все их
/// состояния (S-030).
#[tokio::test]
async fn scheduled_send_keeps_its_time_and_new_states_belong_to_sending_only() {
    let db: TestDb = open_test_db("send-scheduled").await;
    let account = seed_account(&db, "me@example.test").await;
    let queued = db
        .queue_scheduled_send(account, letter("me@example.test"), "2099-01-01 00:00:00")
        .await
        .expect("отложенная отправка");
    assert_eq!(queued.cancel_until, "2099-01-01 00:00:00");
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

/// Сервер принял письмо, а сохранение копии в папку с ролью `sent` не удалось:
/// повторяется только дозапись копии, и её байты лежат в хранилище больших
/// объектов, а не строкой в данных операции (S-009, S-052).
#[tokio::test]
async fn failed_sent_copy_retries_by_reference_and_never_sends_the_message_again() {
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
    db.claim_send_operation(account)
        .await
        .expect("захват")
        .expect("письмо готово к передаче");
    db.convert_send_to_sent_append(queued.operation_id, b"MIME-BYTES", "APPEND отклонён")
        .await
        .expect("превратить в дозапись копии");

    let (kind, payload): (String, String) =
        sqlx::query_as("SELECT op_kind, payload FROM outbox_ops WHERE id=?")
            .bind(queued.operation_id)
            .fetch_one(&db.pool)
            .await
            .expect("прочитать операцию");
    assert_eq!(kind, "append_sent");
    assert!(
        !payload.contains("MIME-BYTES"),
        "байты письма в данные операции не переносятся"
    );
    let parsed: SendPayload = serde_json::from_str(&payload).expect("данные дозаписи копии");
    assert_eq!(
        db.sent_append_bytes(&parsed).expect("байты письма"),
        b"MIME-BYTES".to_vec()
    );
    // S-025: дозапись копии повторной отправки получателю не создаёт, поэтому
    // работник берёт её обычной пачкой, а не как отправку.
    assert!(
        db.claim_send_operation(account)
            .await
            .expect("захват отправки")
            .is_none()
    );
    // Дозапись копии ждёт свои пять секунд перед повтором: приближаем этот срок,
    // как это сделало бы само время.
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
    db.close().await;
}
