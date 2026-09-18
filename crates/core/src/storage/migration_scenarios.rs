//! Сценарная проверка обновления непустой базы прежней версии миграциями
//! второй волны (specs/undo-send.md, specs/out-of-office.md,
//! specs/recipient-history.md).
//!
//! Проверка идёт по настоящему пути: база наполняется в схеме до второй волны,
//! а дальше применяется тот же мигратор, который работает при запуске
//! программы, вместе со всеми его прикладными шагами.

use super::repo::test_storage::{TestDb, open_test_db_upto};
use crate::model::*;
use crate::storage::recipient_history::{RecipientTouch, TouchOrigin};

/// Последняя миграция первой волны: после неё в базе ещё нет ни срока отмены,
/// ни автоответа, ни истории получателей.
const BEFORE_SECOND_WAVE: i64 = 45;

/// Непустая база прежней версии переживает три новые миграции подряд: письма,
/// контакты и уже стоящая в очереди отправка остаются на месте, а новые разделы
/// начинают работать.
#[tokio::test]
async fn a_filled_database_survives_the_second_wave_migrations() {
    let db: TestDb = open_test_db_upto("migrate-wave2", BEFORE_SECOND_WAVE).await;
    let account = sqlx::query_as::<_, (i64,)>(
        "INSERT INTO accounts(uuid, email, provider, backend_kind, auth_kind)
         VALUES(?, 'me@example.test', 'generic', 'imap', 'password') RETURNING id",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .fetch_one(&db.write_pool)
    .await
    .expect("создать ящик")
    .0;
    let folder = sqlx::query_as::<_, (i64,)>(
        "INSERT INTO folders(account_id, remote_path, display_name, role)
         VALUES(?, 'Sent', 'Отправленные', 'sent') RETURNING id",
    )
    .bind(account)
    .fetch_one(&db.write_pool)
    .await
    .expect("создать папку")
    .0;
    sqlx::query(
        "INSERT INTO messages(account_id, folder_id, uid, from_addr, subject, preview, date,
                              rfc822_message_id, to_addrs, remote_id, size)
         VALUES(?, ?, 1, 'me@example.test', 'письмо', '', datetime('now'),
                '<old@example.test>', ?, 'remote-1', 100)",
    )
    .bind(account)
    .bind(folder)
    .bind(r#"[{"name":"Клиент","email":"client@partner.test"}]"#)
    .execute(&db.write_pool)
    .await
    .expect("сохранить отправленное письмо");

    // Почтовый контакт прежнего сбора: вторая волна переносит его в историю.
    let contact = sqlx::query_as::<_, (i64,)>(
        "INSERT INTO contacts(account_id, uid, display_name)
         VALUES(?, 'mail:client@partner.test', 'Клиент') RETURNING id",
    )
    .bind(account)
    .fetch_one(&db.write_pool)
    .await
    .expect("почтовый контакт")
    .0;
    sqlx::query("INSERT INTO contact_emails(contact_id, email, kind) VALUES(?, ?, 'work')")
        .bind(contact)
        .bind("client@partner.test")
        .execute(&db.write_pool)
        .await
        .expect("адрес контакта");

    // Отложенная отправка прежней версии: письмо лежит в данных операции
    // строкой, а полей второй волны у строки ещё нет вовсе.
    let legacy = serde_json::to_string(&crate::backend::OutgoingMessage {
        from: "me@example.test".into(),
        to: vec!["boss@partner.test".into()],
        subject: "Договор".into(),
        body_text: "Текст письма".into(),
        ..Default::default()
    })
    .expect("данные старого формата");
    let (operation_id,): (i64,) = sqlx::query_as(
        "INSERT INTO outbox_ops(account_id, op_kind, payload, status, next_attempt_at)
         VALUES(?, 'send', ?, 'pending', datetime('now')) RETURNING id",
    )
    .bind(account)
    .bind(&legacy)
    .fetch_one(&db.write_pool)
    .await
    .expect("отложенная отправка прежней версии");

    db.migrate().await.expect("обновление базы второй волной");

    // Данные на месте: письмо, ящик и уже стоящая отправка никуда не делись.
    let (messages,): (i64,) = sqlx::query_as("SELECT count(*) FROM messages")
        .fetch_one(&db.pool)
        .await
        .expect("прочитать письма");
    assert_eq!(messages, 1, "письма пережили обновление");
    let outbox = db
        .list_outbox_sends(Some(account), 100, 0)
        .await
        .expect("раздел исходящих");
    assert_eq!(
        outbox.len(),
        1,
        "отправка прежней версии осталась в очереди"
    );
    assert_eq!(outbox[0].id, operation_id);
    assert_eq!(
        outbox[0].origin, SEND_ORIGIN_SCHEDULED,
        "прежняя очередь состояла из отложенных отправок, и окна отмены у них нет"
    );
    assert!(
        db.claim_send_operation(account)
            .await
            .expect("захват перенесённой отправки")
            .is_some(),
        "перенесённая отправка осталась готовой к передаче"
    );

    // Новые разделы работают на этой же базе.
    db.save_local_out_of_office(&OutOfOfficeInput {
        account_id: account,
        enabled: true,
        starts_at: (chrono::Utc::now() - chrono::Duration::days(1)).to_rfc3339(),
        ends_at: (chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339(),
        internal_text: "Я в отпуске".into(),
        external_text: "Я в отпуске".into(),
        internal_domains: vec!["example.test".into()],
    })
    .await
    .expect("включить автоответ на обновлённой базе");
    db.record_recipient_touches(
        account,
        &[RecipientTouch {
            email: "boss@partner.test".into(),
            name: "Борис".into(),
            message_key: "<after-migration@example.test>".into(),
            used_at: chrono::Utc::now()
                .format("%Y-%m-%dT%H:%M:%S+00:00")
                .to_string(),
        }],
        TouchOrigin::OwnSend,
    )
    .await
    .expect("записать обращение на обновлённой базе");

    // Прикладной шаг обновления перенёс почтовый контакт в историю: адресная
    // книга больше не наполняется письмами (recipient-history.md S-002).
    let addresses: Vec<(String,)> = sqlx::query_as(
        "SELECT display_address FROM recipient_history WHERE account_id=? ORDER BY display_address",
    )
    .bind(account)
    .fetch_all(&db.pool)
    .await
    .expect("прочитать историю");
    let addresses: Vec<String> = addresses.into_iter().map(|(value,)| value).collect();
    assert!(
        addresses.contains(&"client@partner.test".to_owned()),
        "перенесённый контакт стал записью истории: {addresses:?}"
    );
    assert!(addresses.contains(&"boss@partner.test".to_owned()));
    let (contacts,): (i64,) = sqlx::query_as("SELECT count(*) FROM contacts")
        .fetch_one(&db.pool)
        .await
        .expect("прочитать адресную книгу");
    assert_eq!(contacts, 0, "почтовый контакт в адресной книге не остался");

    // Повторный запуск программы на уже обновлённой базе ничего не ломает.
    db.migrate().await.expect("повторное обновление");
    db.close().await;
}
