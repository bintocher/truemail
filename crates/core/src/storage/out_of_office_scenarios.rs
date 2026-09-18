//! Сценарные проверки автоответа "нет на месте" (specs/out-of-office.md).
//!
//! Проверки идут по настоящему пути: настоящая база, настоящий конвейер стадий
//! разбора нового письма и настоящая очередь отправки. Ответ рассылке или
//! чужому автоответу запускает бесконечную переписку двух программ, поэтому
//! правила молчания проверяются на живом конвейере, а не в обход него.

use super::Db;
use super::repo::test_storage::{TestDb, open_test_db};
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

async fn seed_folder(db: &Db, account_id: i64, path: &str, role: Option<&str>) -> i64 {
    sqlx::query_as::<_, (i64,)>(
        "INSERT INTO folders(account_id, remote_path, display_name, role)
         VALUES(?, ?, ?, ?) RETURNING id",
    )
    .bind(account_id)
    .bind(path)
    .bind(path)
    .bind(role)
    .fetch_one(&db.write_pool)
    .await
    .expect("создать папку")
    .0
}

/// Входящее письмо в том виде, в каком его сохраняет синхронизация.
#[derive(Default)]
struct Incoming<'a> {
    from: &'a str,
    subject: &'a str,
    message_id: &'a str,
    newsletter: bool,
    auto_submitted: Option<&'a str>,
    precedence: Option<&'a str>,
    reply_to: Option<&'a str>,
    to: Option<&'a str>,
    backfilled: bool,
    headers_known: bool,
    /// Время получения письма. Пусто - письмо только что пришло.
    date: Option<&'a str>,
}

async fn seed_message(
    db: &Db,
    account_id: i64,
    folder_id: i64,
    uid: i64,
    seed: Incoming<'_>,
) -> i64 {
    let to = seed
        .to
        .map(str::to_owned)
        .unwrap_or_else(|| r#"[{"name":null,"email":"me@example.test"}]"#.to_owned());
    let reply_to = seed
        .reply_to
        .map(|email| format!(r#"[{{"name":null,"email":"{email}"}}]"#));
    sqlx::query_as::<_, (i64,)>(
        "INSERT INTO messages(account_id, folder_id, uid, from_name, from_addr, subject, preview,
                              date, rfc822_message_id, to_addrs, reply_to_addrs, backfilled,
                              is_newsletter, auto_submitted, precedence, silence_headers_known,
                              remote_id, size)
         VALUES(?, ?, ?, 'Отправитель', ?, ?, 'предпросмотр',
                coalesce(?, datetime('now')), ?, ?, ?, ?, ?, ?, ?, ?, ?, 2048)
         RETURNING id",
    )
    .bind(account_id)
    .bind(folder_id)
    .bind(uid)
    .bind(seed.from)
    .bind(seed.subject)
    .bind(seed.date)
    .bind(seed.message_id)
    .bind(to)
    .bind(reply_to)
    .bind(seed.backfilled as i64)
    .bind(seed.newsletter as i64)
    .bind(seed.auto_submitted)
    .bind(seed.precedence)
    .bind(seed.headers_known as i64)
    .bind(format!("remote-{folder_id}-{uid}"))
    .fetch_one(&db.write_pool)
    .await
    .expect("сохранить письмо")
    .0
}

/// Включить локальный автоответ на период вокруг текущего времени.
async fn enable_local_absence(db: &Db, account_id: i64) {
    enable_local_absence_since(db, account_id, 1).await
}

/// То же, но с началом периода на заданное число суток назад: письмо старше
/// суток должно попадать внутрь периода, иначе его отсекает не возраст, а
/// граница периода.
async fn enable_local_absence_since(db: &Db, account_id: i64, days_back: i64) {
    let now = chrono::Utc::now();
    db.save_local_out_of_office(&OutOfOfficeInput {
        account_id,
        enabled: true,
        starts_at: (now - chrono::Duration::days(days_back)).to_rfc3339(),
        ends_at: (now + chrono::Duration::days(1)).to_rfc3339(),
        internal_text: "Я в отпуске, коллеги в курсе".into(),
        external_text: "Я в отпуске, отвечу позже".into(),
        internal_domains: vec!["example.test".into()],
    })
    .await
    .expect("включить автоответ");
}

/// Автоответы, поставленные в очередь отправки этого ящика.
async fn queued_replies(db: &Db, account_id: i64) -> Vec<(String, String, String)> {
    sqlx::query_as(
        "SELECT json_extract(payload,'$.to[0]'), json_extract(payload,'$.subject'), send_origin
           FROM outbox_ops
          WHERE account_id=? AND op_kind='send' AND send_origin='automatic'
          ORDER BY id",
    )
    .bind(account_id)
    .fetch_all(&db.pool)
    .await
    .expect("прочитать очередь автоответов")
}

/// Период отсутствия идёт, приходит обычное письмо и письмо рассылки.
/// Рассылка остаётся без ответа, обычное письмо получает ровно один автоответ с
/// нулевым окном отмены, а второе письмо того же адресата ответа не добавляет
/// (S-030, S-038, S-048, S-054, S-055).
#[tokio::test]
async fn newsletter_stays_silent_while_a_colleague_gets_exactly_one_reply() {
    let db: TestDb = open_test_db("oof-basic").await;
    let account = seed_account(&db, "me@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    enable_local_absence(&db, account).await;

    seed_message(
        &db,
        account,
        inbox,
        1,
        Incoming {
            from: "shop@partner.test",
            subject: "Скидки недели",
            message_id: "<news-1@partner.test>",
            newsletter: true,
            headers_known: true,
            ..Default::default()
        },
    )
    .await;
    seed_message(
        &db,
        account,
        inbox,
        2,
        Incoming {
            from: "boss@example.test",
            subject: "Отчёт",
            message_id: "<letter-1@example.test>",
            headers_known: true,
            ..Default::default()
        },
    )
    .await;
    db.process_sync_batch_stages().await.expect("стадии");

    let replies = queued_replies(&db, account).await;
    assert_eq!(replies.len(), 1, "рассылка ответа не получает");
    assert_eq!(replies[0].0, "boss@example.test");
    assert_eq!(replies[0].1, "Re: Отчёт");
    assert_eq!(replies[0].2, SEND_ORIGIN_AUTOMATIC);
    let (cancel_until, next_attempt): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT cancel_until, next_attempt_at FROM outbox_ops WHERE send_origin='automatic'",
    )
    .fetch_one(&db.pool)
    .await
    .expect("прочитать сроки автоответа");
    assert_eq!(
        cancel_until, next_attempt,
        "служебное письмо окна отмены не получает"
    );
    // S-061: в ответ уходит только сохранённый текст отсутствия.
    let (payload,): (String,) =
        sqlx::query_as("SELECT payload FROM outbox_ops WHERE send_origin='automatic'")
            .fetch_one(&db.pool)
            .await
            .expect("данные автоответа");
    let payload: SendPayload = serde_json::from_str(&payload).expect("разбор данных");
    let body: SendBody =
        serde_json::from_slice(&db.blobs.get(&payload.body_ref).expect("тело автоответа"))
            .expect("разбор тела");
    assert_eq!(body.body_text, "Я в отпуске, коллеги в курсе");
    assert!(body.body_html.is_none(), "оформления в автоответе нет");
    assert!(payload.attachments.is_empty());
    // S-056 - S-058: письмо помечено автоматическим и связано с исходным.
    assert!(
        payload
            .headers
            .iter()
            .any(|(name, value)| name == "Auto-Submitted" && value == "auto-replied")
    );
    assert!(
        payload
            .headers
            .iter()
            .any(|(name, value)| name == "X-Auto-Response-Suppress" && value == "All")
    );
    assert!(
        payload
            .headers
            .iter()
            .any(|(name, value)| name == "In-Reply-To" && value == "<letter-1@example.test>")
    );

    // Второе письмо того же адресата внутри окна молчания ответа не добавляет.
    seed_message(
        &db,
        account,
        inbox,
        3,
        Incoming {
            from: "boss@example.test",
            subject: "Отчёт, дополнение",
            message_id: "<letter-2@example.test>",
            headers_known: true,
            ..Default::default()
        },
    )
    .await;
    db.process_sync_batch_stages().await.expect("стадии");
    assert_eq!(
        queued_replies(&db, account).await.len(),
        1,
        "окно молчания в семь суток держит одного адресата на одном ответе"
    );
    db.close().await;
}

/// Повторный разбор того же письма второй отправки не создаёт, а письмо,
/// закрытое предшествующей стадией, ответа не получает вовсе (S-031, S-053).
#[tokio::test]
async fn blocked_sender_and_reprocessing_never_produce_a_second_reply() {
    let db: TestDb = open_test_db("oof-stages").await;
    let account = seed_account(&db, "me@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    seed_folder(&db, account, "Trash", Some("trash")).await;
    enable_local_absence(&db, account).await;
    db.save_sender_policy("address", "spam@partner.test", "blocked", false)
        .await
        .expect("заблокировать отправителя");

    seed_message(
        &db,
        account,
        inbox,
        1,
        Incoming {
            from: "spam@partner.test",
            subject: "Предложение",
            message_id: "<spam-1@partner.test>",
            headers_known: true,
            ..Default::default()
        },
    )
    .await;
    let letter = seed_message(
        &db,
        account,
        inbox,
        2,
        Incoming {
            from: "client@partner.test",
            subject: "Вопрос",
            message_id: "<client-1@partner.test>",
            headers_known: true,
            ..Default::default()
        },
    )
    .await;
    db.process_sync_batch_stages().await.expect("стадии");
    let replies = queued_replies(&db, account).await;
    assert_eq!(
        replies.len(),
        1,
        "заблокированный отправитель ответа не получает"
    );
    assert_eq!(replies[0].0, "client@partner.test");

    // Повторный разбор того же письма: курсор стадии отматываем назад, как это
    // сделала бы повторная синхронизация той же папки.
    sqlx::query("DELETE FROM stage_progress WHERE stage=?")
        .bind(OUT_OF_OFFICE_STAGE_NAME)
        .execute(&db.write_pool)
        .await
        .expect("сбросить курсор стадии");
    db.process_sync_batch_stages()
        .await
        .expect("повторные стадии");
    assert_eq!(
        queued_replies(&db, account).await.len(),
        1,
        "запись ответа по тому же письму второй отправки не создаёт"
    );
    assert!(letter > 0);
    db.close().await;
}

/// Письмо, которого нет ни в поле "Кому", ни в поле "Копия", догруженное
/// прокруткой письмо и письмо с несколькими адресами для ответа остаются без
/// автоответа (S-033, S-037, S-046).
#[tokio::test]
async fn hidden_copy_backfill_and_multiple_reply_to_stay_silent() {
    let db: TestDb = open_test_db("oof-silence").await;
    let account = seed_account(&db, "me@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    enable_local_absence(&db, account).await;

    seed_message(
        &db,
        account,
        inbox,
        1,
        Incoming {
            from: "list@partner.test",
            subject: "Переписка коллег",
            message_id: "<bcc-1@partner.test>",
            to: Some(r#"[{"name":null,"email":"other@partner.test"}]"#),
            headers_known: true,
            ..Default::default()
        },
    )
    .await;
    seed_message(
        &db,
        account,
        inbox,
        2,
        Incoming {
            from: "old@partner.test",
            subject: "Старое письмо",
            message_id: "<old-1@partner.test>",
            backfilled: true,
            headers_known: true,
            ..Default::default()
        },
    )
    .await;
    let many_reply_to = sqlx::query_as::<_, (i64,)>(
        "INSERT INTO messages(account_id, folder_id, uid, from_addr, subject, preview, date,
                              rfc822_message_id, to_addrs, reply_to_addrs, silence_headers_known,
                              remote_id, size)
         VALUES(?, ?, 3, 'team@partner.test', 'Совещание', '', datetime('now'),
                '<many-1@partner.test>',
                '[{\"name\":null,\"email\":\"me@example.test\"}]',
                '[{\"name\":null,\"email\":\"one@partner.test\"},{\"name\":null,\"email\":\"two@partner.test\"}]',
                1, 'remote-many', 100)
         RETURNING id",
    )
    .bind(account)
    .bind(inbox)
    .fetch_one(&db.write_pool)
    .await
    .expect("письмо с несколькими адресами для ответа")
    .0;
    assert!(many_reply_to > 0);

    db.process_sync_batch_stages().await.expect("стадии");
    assert!(
        queued_replies(&db, account).await.is_empty(),
        "ни одно из этих писем автоответа не получает"
    );
    db.close().await;
}

/// Новое включение автоответа стирает память о прежних ответах, а отключение
/// отменяет ещё не начатые автоответы этого ящика (S-051, S-067).
#[tokio::test]
async fn switching_the_absence_off_and_on_clears_memory_and_cancels_pending_replies() {
    let db: TestDb = open_test_db("oof-toggle").await;
    let account = seed_account(&db, "me@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    enable_local_absence(&db, account).await;
    seed_message(
        &db,
        account,
        inbox,
        1,
        Incoming {
            from: "client@partner.test",
            subject: "Вопрос",
            message_id: "<client-1@partner.test>",
            headers_known: true,
            ..Default::default()
        },
    )
    .await;
    db.process_sync_batch_stages().await.expect("стадии");
    assert_eq!(queued_replies(&db, account).await.len(), 1);

    let settings = db
        .out_of_office_settings(account)
        .await
        .expect("прочитать настройку");
    db.save_local_out_of_office(&OutOfOfficeInput {
        account_id: account,
        enabled: false,
        starts_at: settings.starts_at.clone().unwrap_or_default(),
        ends_at: settings.ends_at.clone().unwrap_or_default(),
        internal_text: settings.internal_text.clone(),
        external_text: settings.external_text.clone(),
        internal_domains: settings.internal_domains.clone(),
    })
    .await
    .expect("отключить автоответ");
    let (status,): (String,) =
        sqlx::query_as("SELECT status FROM outbox_ops WHERE send_origin='automatic'")
            .fetch_one(&db.pool)
            .await
            .expect("прочитать состояние автоответа");
    assert_eq!(
        status, SEND_STATUS_CANCELLED,
        "ещё не начатый автоответ отменяется вместе с настройкой"
    );

    enable_local_absence(&db, account).await;
    let (memory,): (i64,) = sqlx::query_as("SELECT count(*) FROM out_of_office_replies")
        .fetch_one(&db.pool)
        .await
        .expect("прочитать память об ответах");
    assert_eq!(
        memory, 0,
        "новый период отсутствия начинается без унаследованного молчания"
    );
    db.close().await;
}

/// Внешний отправитель получает внешний текст, а отправитель внутреннего
/// домена и его поддомена - внутренний (S-028, S-029).
#[tokio::test]
async fn internal_and_external_senders_get_their_own_texts() {
    let db: TestDb = open_test_db("oof-texts").await;
    let account = seed_account(&db, "me@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    enable_local_absence(&db, account).await;
    for (uid, from, message_id) in [
        (1_i64, "colleague@sub.example.test", "<in-1@example.test>"),
        (2, "client@notexample.test", "<out-1@notexample.test>"),
    ] {
        seed_message(
            &db,
            account,
            inbox,
            uid,
            Incoming {
                from,
                subject: "Вопрос",
                message_id,
                headers_known: true,
                ..Default::default()
            },
        )
        .await;
    }
    db.process_sync_batch_stages().await.expect("стадии");
    let payloads: Vec<(String,)> =
        sqlx::query_as("SELECT payload FROM outbox_ops WHERE send_origin='automatic' ORDER BY id")
            .fetch_all(&db.pool)
            .await
            .expect("данные автоответов");
    assert_eq!(payloads.len(), 2);
    let mut texts = Vec::new();
    for (payload,) in payloads {
        let payload: SendPayload = serde_json::from_str(&payload).expect("разбор данных");
        let body: SendBody =
            serde_json::from_slice(&db.blobs.get(&payload.body_ref).expect("тело"))
                .expect("разбор тела");
        texts.push(body.body_text);
    }
    assert_eq!(
        texts[0], "Я в отпуске, коллеги в курсе",
        "поддомен считается своим"
    );
    assert_eq!(
        texts[1], "Я в отпуске, отвечу позже",
        "похожий домен своим не становится"
    );
    db.close().await;
}

/// Окончательно отказавший автоответ адресата не закрывает: следующее его письмо
/// снова получает ответ, потому что ни одного ответа он так и не получил
/// (S-048, S-068).
#[tokio::test]
async fn a_finally_failed_reply_does_not_silence_the_sender() {
    let db: TestDb = open_test_db("oof-failed").await;
    let account = seed_account(&db, "me@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    enable_local_absence(&db, account).await;
    seed_message(
        &db,
        account,
        inbox,
        1,
        Incoming {
            from: "colleague@example.test",
            subject: "Вопрос",
            message_id: "<first@partner.test>",
            headers_known: true,
            ..Default::default()
        },
    )
    .await;
    db.process_out_of_office_stage()
        .await
        .expect("стадия автоответа");
    let (operation_id,): (i64,) = sqlx::query_as(
        "SELECT operation_id FROM out_of_office_replies WHERE account_id=? ORDER BY id LIMIT 1",
    )
    .bind(account)
    .fetch_one(&db.pool)
    .await
    .expect("запись ответа");

    // Попытки исчерпаны: ответ окончательно отказал и адресата не получил.
    sqlx::query("UPDATE outbox_ops SET attempts=8 WHERE id=?")
        .bind(operation_id)
        .execute(&db.write_pool)
        .await
        .expect("исчерпать попытки");
    db.fail_outbox_operation(operation_id, "сервер отверг письмо")
        .await
        .expect("окончательный отказ");
    let (state,): (String,) =
        sqlx::query_as("SELECT state FROM out_of_office_replies WHERE operation_id=?")
            .bind(operation_id)
            .fetch_one(&db.pool)
            .await
            .expect("состояние записи ответа");
    assert_eq!(state, "failed", "отказ записан в самой записи ответа");

    seed_message(
        &db,
        account,
        inbox,
        2,
        Incoming {
            from: "colleague@example.test",
            subject: "Ещё вопрос",
            message_id: "<second@partner.test>",
            headers_known: true,
            ..Default::default()
        },
    )
    .await;
    db.process_out_of_office_stage()
        .await
        .expect("вторая стадия автоответа");
    assert_eq!(
        queued_replies(&db, account).await.len(),
        2,
        "после окончательного отказа адресат получает ответ на следующее письмо"
    );
    db.close().await;
}

/// Число писем, оставшихся без ответа по возрасту, считает только те письма,
/// которые ответ получили бы: рассылка, служебный адрес и чужое письмо в него не
/// попадают (S-035, S-065).
#[tokio::test]
async fn the_unanswered_counter_holds_only_messages_that_would_have_been_answered() {
    let db: TestDb = open_test_db("oof-old").await;
    let account = seed_account(&db, "me@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    enable_local_absence_since(&db, account, 3).await;
    let long_ago = (chrono::Utc::now() - chrono::Duration::hours(30))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let older = [
        Incoming {
            from: "shop@partner.test",
            subject: "Скидки",
            message_id: "<bulk@partner.test>",
            newsletter: true,
            headers_known: true,
            date: Some(&long_ago),
            ..Default::default()
        },
        Incoming {
            from: "mailer-daemon@partner.test",
            subject: "Отчёт",
            message_id: "<daemon@partner.test>",
            headers_known: true,
            date: Some(&long_ago),
            ..Default::default()
        },
        Incoming {
            from: "stranger@partner.test",
            subject: "Рассылка отдела",
            message_id: "<hidden@partner.test>",
            headers_known: true,
            to: Some(r#"[{"name":null,"email":"team@partner.test"}]"#),
            date: Some(&long_ago),
            ..Default::default()
        },
    ];
    let mut uid = 1;
    for seed in older {
        seed_message(&db, account, inbox, uid, seed).await;
        uid += 1;
    }
    db.process_out_of_office_stage()
        .await
        .expect("стадия автоответа");
    let (skipped,): (i64,) =
        sqlx::query_as("SELECT skipped_old FROM out_of_office_settings WHERE account_id=?")
            .bind(account)
            .fetch_one(&db.pool)
            .await
            .expect("счётчик писем без ответа");
    assert_eq!(
        skipped, 0,
        "письма, которым ответа не было бы в любом случае, в счётчик не идут"
    );

    // Обычное старое письмо коллеги - тот самый случай, ради которого счётчик и
    // заведён: ответ был бы, но программа не работала.
    seed_message(
        &db,
        account,
        inbox,
        uid,
        Incoming {
            from: "colleague@example.test",
            subject: "Вопрос",
            message_id: "<old-letter@partner.test>",
            headers_known: true,
            date: Some(&long_ago),
            ..Default::default()
        },
    )
    .await;
    db.process_out_of_office_stage()
        .await
        .expect("вторая стадия автоответа");
    let (skipped,): (i64,) =
        sqlx::query_as("SELECT skipped_old FROM out_of_office_settings WHERE account_id=?")
            .bind(account)
            .fetch_one(&db.pool)
            .await
            .expect("счётчик писем без ответа");
    assert_eq!(skipped, 1, "старое письмо коллеги названо прямо");
    assert!(
        queued_replies(&db, account).await.is_empty(),
        "запоздалых ответов программа не рассылает"
    );
    db.close().await;
}
