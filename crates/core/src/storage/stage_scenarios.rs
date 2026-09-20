//! Сценарные проверки стадий разбора письма: списки отправителей,
//! игнорируемые переписки и автоочистка по отправителю
//! (specs/blocked-senders.md, specs/ignore-conversation.md,
//! specs/sweep-by-sender.md).
//!
//! Каждая проверка проходит сценарий целиком по настоящему пути: настоящая
//! база с применёнными миграциями, настоящая очередь операций и настоящий
//! конвейер стадий. Подмены слоёв здесь нет намеренно: в прошлой задаче
//! проверки, читавшие модули в обход настоящего пути, пропустили блокирующий
//! дефект.

use super::Db;
use super::repo::test_storage::{TestDb, open_test_db};
use crate::model::*;

async fn test_db(prefix: &str) -> TestDb {
    open_test_db(prefix).await
}

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

/// Письмо в том виде, в каком его сохраняет синхронизация: адрес отправителя,
/// дата, заголовки опознания переписки и признак догрузки прокруткой.
#[derive(Default)]
struct Seed<'a> {
    from: &'a str,
    subject: &'a str,
    date: Option<&'a str>,
    message_id: Option<&'a str>,
    in_reply_to: Option<&'a str>,
    references: Option<&'a str>,
    backfilled: bool,
}

async fn seed_message(db: &Db, account_id: i64, folder_id: i64, uid: i64, seed: Seed<'_>) -> i64 {
    sqlx::query_as::<_, (i64,)>(
        "INSERT INTO messages(account_id, folder_id, uid, from_name, from_addr, subject, preview,
                              date, rfc822_message_id, in_reply_to, references_ids, backfilled,
                              remote_id, size)
         VALUES(?, ?, ?, 'Отправитель', ?, ?, 'предпросмотр', ?, ?, ?, ?, ?, ?, 2048)
         RETURNING id",
    )
    .bind(account_id)
    .bind(folder_id)
    .bind(uid)
    .bind(seed.from)
    .bind(seed.subject)
    .bind(seed.date)
    .bind(seed.message_id)
    .bind(seed.in_reply_to)
    .bind(seed.references)
    .bind(seed.backfilled as i64)
    .bind(format!("remote-{folder_id}-{uid}"))
    .fetch_one(&db.write_pool)
    .await
    .expect("сохранить письмо")
    .0
}

/// Операции увода по письму: вид, состояние и папка назначения.
async fn takeaways(db: &Db, message_id: i64) -> Vec<(String, String, Option<i64>)> {
    sqlx::query_as(
        "SELECT op_kind, status, CAST(json_extract(payload,'$.target_folder_id') AS INTEGER)
           FROM outbox_ops WHERE message_id=? AND op_kind IN ('move','delete') ORDER BY id",
    )
    .bind(message_id)
    .fetch_all(&db.pool)
    .await
    .expect("прочитать очередь")
}

async fn closed_by(db: &Db, message_id: i64) -> Option<String> {
    sqlx::query_as::<_, (Option<String>,)>("SELECT closed_by_stage FROM messages WHERE id=?")
        .bind(message_id)
        .fetch_one(&db.pool)
        .await
        .expect("прочитать признак стадии")
        .0
}

/// Приметы письма, унесённого очередью: по ним возврат ищет его в корзине.
struct MovedHeaders {
    from: String,
    date: String,
    message_id: String,
}

/// Выполнить всю очередь ящика настоящим путём: операция забирается работником
/// и закрывается успехом, а успешное перемещение удаляет и операцию, и
/// локальную строку письма. Возвращает приметы перемещённых писем.
async fn complete_queue(db: &Db, account_id: i64) -> Vec<MovedHeaders> {
    let operations = db
        .claim_outbox_operations(account_id, 500)
        .await
        .expect("забрать операции");
    let mut moved = Vec::new();
    for operation in &operations {
        if let Some(message_id) = operation.message_id {
            let row: Option<(Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
                "SELECT from_addr, date, rfc822_message_id FROM messages WHERE id=?",
            )
            .bind(message_id)
            .fetch_optional(&db.pool)
            .await
            .expect("приметы письма");
            if let Some((from, date, header)) = row
                && let (Some(from), Some(date), Some(header)) = (from, date, header)
            {
                moved.push(MovedHeaders {
                    from,
                    date,
                    message_id: header,
                });
            }
        }
        db.complete_outbox_operation(operation)
            .await
            .expect("операция выполнена");
    }
    moved
}

fn days_ago(days: i64) -> String {
    (chrono::Utc::now() - chrono::Duration::days(days))
        .format("%Y-%m-%dT%H:%M:%S+00:00")
        .to_string()
}

/// Блокировка отправителя от начала до конца: запись правила, приход письма,
/// порядок стадий, постановка переноса в корзину, исключение письма из
/// уведомления о новой почте и защита письма, уже лежащего в корзине
/// (blocked-senders.md S-001 - S-005, S-021, S-023 - S-025).
///
/// Письмо нарочно подходит сразу под три стадии - список отправителей,
/// игнорируемую переписку и правило: его закрывает первая стадия, второй
/// операции увода по нему не появляется, и правило к закрытому письму не
/// применяется. Два увода по одному письму - это перенос в корзину и следом
/// перенос из корзины в спам, то есть письмо, потерянное на сервере.
#[tokio::test]
async fn blocked_sender_is_queued_to_trash_and_closes_the_message() {
    let db = test_db("stage-blocked").await;
    let account = seed_account(&db, "me@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    let trash = seed_folder(&db, account, "Trash", Some("trash")).await;
    let spam = seed_folder(&db, account, "Spam", Some("spam")).await;

    // Первое письмо переписки: по нему человек включает игнорирование, и та же
    // переписка попадает под блокировку отправителя.
    let root = seed_message(
        &db,
        account,
        inbox,
        1,
        Seed {
            from: "spam@example.test",
            subject: "Переписка",
            message_id: Some("<root@example.test>"),
            ..Seed::default()
        },
    )
    .await;
    let ignore_preview = db
        .preview_ignore_conversation(root)
        .await
        .expect("предпросмотр игнорирования");
    db.enable_ignore_conversation(root, &ignore_preview.snapshot_key, true)
        .await
        .expect("игнорирование переписки");

    // Правило, которое увело бы письмо в спам и пометило прочитанным, если бы
    // оно дошло до стадии правил.
    let rule = MailRuleInput {
        id: "rule-after-block".into(),
        name: "После блокировки".into(),
        account_id: None,
        enabled: true,
        groups: vec![MailRuleGroup {
            logic: "all".into(),
            conditions: vec![MailRuleCondition {
                field: "sender_address".into(),
                op: "equals".into(),
                value: "spam@example.test".into(),
                unit: None,
                value2: None,
            }],
        }],
        exceptions: Vec::new(),
        actions: vec![
            // Пометка раньше увода: после уводящего действия хвост правила
            // не выполняется, и правило с таким порядком просто не сохранить.
            MailRuleAction {
                kind: "mark_read".into(),
                folder_id: None,
                folder_role: None,
                label_id: None,
            },
            MailRuleAction {
                kind: "spam".into(),
                folder_id: None,
                folder_role: None,
                label_id: None,
            },
        ],
        confirm_key: None,
    };
    db.save_mail_rule(&rule, false, None)
        .await
        .expect("правило");

    let preview = db
        .preview_sender_policy(POLICY_KIND_ADDRESS, " Spam@Example.TEST ")
        .await
        .expect("предпросмотр");
    assert_eq!(preview.value, "Spam@example.test");
    assert!(!preview.own_address);
    db.save_sender_policy(
        POLICY_KIND_ADDRESS,
        &preview.value,
        POLICY_DECISION_BLOCKED,
        false,
    )
    .await
    .expect("запись блокировки");

    let blocked = seed_message(
        &db,
        account,
        inbox,
        2,
        Seed {
            from: "spam@example.test",
            subject: "Re: Переписка",
            message_id: Some("<next@example.test>"),
            in_reply_to: Some("<root@example.test>"),
            ..Seed::default()
        },
    )
    .await;
    let wanted = seed_message(
        &db,
        account,
        inbox,
        3,
        Seed {
            from: "friend@example.test",
            subject: "рассылка",
            ..Seed::default()
        },
    )
    .await;
    // Письмо того же отправителя, уже лежащее в корзине: перенос корзина в
    // корзину превратился бы в безвозвратное удаление (S-005).
    let in_trash = seed_message(
        &db,
        account,
        trash,
        4,
        Seed {
            from: "spam@example.test",
            subject: "старое",
            ..Seed::default()
        },
    )
    .await;

    db.process_sync_batch_stages().await.expect("стадии");

    let queued = takeaways(&db, blocked).await;
    assert_eq!(
        queued.len(),
        1,
        "по письму больше одной операции увода: следующие стадии разобрали закрытое письмо"
    );
    assert_eq!(queued[0].0, "move", "письмо переносится, а не удаляется");
    assert_eq!(
        queued[0].2,
        Some(trash),
        "письмо увело не первой стадией, а правилом"
    );
    assert_ne!(queued[0].2, Some(spam));
    assert_eq!(
        closed_by(&db, blocked).await.as_deref(),
        Some(SENDER_POLICY_STAGE_NAME),
        "письмо закрыто не стадией списков отправителей и дальше идёт"
    );
    let read: (i64,) = sqlx::query_as("SELECT seen FROM messages WHERE id=?")
        .bind(blocked)
        .fetch_one(&db.pool)
        .await
        .expect("признак прочтения");
    assert_eq!(read.0, 0, "правила к закрытому письму не применяются");

    assert!(takeaways(&db, wanted).await.is_empty());
    assert!(closed_by(&db, wanted).await.is_none());
    assert!(
        takeaways(&db, in_trash).await.is_empty(),
        "письмо из корзины в корзину не переносится"
    );

    // S-010 общей спецификации правил: уведённое письмо в уведомление о новой
    // почте не попадает, а обычное - попадает.
    let notify = db
        .inbox_message_ids_by_remote_ids(
            account,
            &[format!("remote-{inbox}-2"), format!("remote-{inbox}-3")],
            None,
            None,
        )
        .await
        .expect("письма для уведомления");
    assert_eq!(notify, vec![wanted]);
    db.close().await;
}

/// Доверие, собственный адрес и собственный домен: точная запись адреса важнее
/// записи домена, собственный адрес заблокировать нельзя, а блокировка своего
/// домена требует отдельного подтверждения и собственную почту не трогает
/// (S-014 - S-020).
#[tokio::test]
async fn own_address_survives_blocking_of_its_domain() {
    let db = test_db("stage-own").await;
    let account = seed_account(&db, "me@company.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    let trash = seed_folder(&db, account, "Trash", Some("trash")).await;

    let refused = db
        .save_sender_policy(
            POLICY_KIND_ADDRESS,
            "me@company.test",
            POLICY_DECISION_BLOCKED,
            false,
        )
        .await;
    assert!(refused.is_err(), "собственный адрес заблокировать нельзя");

    let without_confirmation = db
        .save_sender_policy(
            POLICY_KIND_DOMAIN,
            "company.test",
            POLICY_DECISION_BLOCKED,
            false,
        )
        .await;
    assert!(
        without_confirmation.is_err(),
        "блокировка своего домена требует подтверждения"
    );
    db.save_sender_policy(
        POLICY_KIND_DOMAIN,
        "company.test",
        POLICY_DECISION_BLOCKED,
        true,
    )
    .await
    .expect("подтверждённая блокировка домена");
    db.save_sender_policy(
        POLICY_KIND_ADDRESS,
        "colleague@company.test",
        POLICY_DECISION_TRUSTED,
        false,
    )
    .await
    .expect("доверенный адрес внутри домена");

    let own = seed_message(
        &db,
        account,
        inbox,
        1,
        Seed {
            from: "me@company.test",
            subject: "своё письмо",
            ..Seed::default()
        },
    )
    .await;
    let colleague = seed_message(
        &db,
        account,
        inbox,
        2,
        Seed {
            from: "colleague@company.test",
            subject: "от коллеги",
            ..Seed::default()
        },
    )
    .await;
    let stranger = seed_message(
        &db,
        account,
        inbox,
        3,
        Seed {
            from: "stranger@sub.company.test",
            subject: "от чужого",
            ..Seed::default()
        },
    )
    .await;

    db.process_sync_batch_stages().await.expect("стадии");
    assert!(
        takeaways(&db, own).await.is_empty(),
        "своё письмо доверенное"
    );
    assert!(
        takeaways(&db, colleague).await.is_empty(),
        "доверенный адрес важнее заблокированного домена"
    );
    let queued = takeaways(&db, stranger).await;
    assert_eq!(queued.len(), 1, "поддомен совпадает по границе точки");
    assert_eq!(queued[0].2, Some(trash));
    db.close().await;
}

/// Уборка уже полученных писем: снимок кандидатов, пачка в 500 писем, остаток,
/// продолжение следующим проходом и письмо, пришедшее после снимка
/// (S-030 - S-036), затем снятие блокировки с отменой неисполненных
/// перемещений и отчётом о неотменимых (S-039 - S-042).
#[tokio::test]
async fn sweep_follows_the_snapshot_and_release_cancels_pending_moves() {
    let db = test_db("stage-sweep").await;
    let account = seed_account(&db, "me@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    let trash = seed_folder(&db, account, "Trash", Some("trash")).await;
    let archive = seed_folder(&db, account, "Archive", Some("archive")).await;
    let sent = seed_folder(&db, account, "Sent", Some("sent")).await;
    for uid in 0..520 {
        seed_message(
            &db,
            account,
            inbox,
            uid,
            Seed {
                from: "news@example.test",
                subject: "выпуск",
                ..Seed::default()
            },
        )
        .await;
    }
    // Архив и отправленные уборка списков не берёт ни при каком согласии
    // (S-034).
    let archived = seed_message(
        &db,
        account,
        archive,
        900,
        Seed {
            from: "news@example.test",
            subject: "сохранённое",
            ..Seed::default()
        },
    )
    .await;
    let sent_copy = seed_message(
        &db,
        account,
        sent,
        901,
        Seed {
            from: "news@example.test",
            subject: "отправленное",
            ..Seed::default()
        },
    )
    .await;

    let preview = db
        .preview_sender_policy(POLICY_KIND_ADDRESS, "news@example.test")
        .await
        .expect("предпросмотр");
    assert_eq!(preview.total, 520, "считаются только рабочие папки");
    assert_eq!(preview.per_account.len(), 1);
    let policy = db
        .save_sender_policy(
            POLICY_KIND_ADDRESS,
            &preview.value,
            POLICY_DECISION_BLOCKED,
            false,
        )
        .await
        .expect("запись блокировки");

    // Письмо, пришедшее после снимка: уборка его не берёт, им занимается
    // стадия списков (S-031).
    let after_snapshot = seed_message(
        &db,
        account,
        inbox,
        902,
        Seed {
            from: "news@example.test",
            subject: "пришло позже",
            ..Seed::default()
        },
    )
    .await;

    let refused = db
        .start_sender_policy_sweep(policy.id, &preview.snapshot_key, false)
        .await;
    assert!(refused.is_err(), "без согласия уборка не начинается");

    let reports = db
        .start_sender_policy_sweep(policy.id, &preview.snapshot_key, true)
        .await
        .expect("уборка");
    let first = reports.first().expect("отчёт прохода");
    assert_eq!(first.queued, 500, "за проход не больше 500 перемещений");
    assert_eq!(first.remaining, 20, "остаток снимка виден пользователю");
    assert_eq!(first.state, "pending");

    let second = db
        .continue_sender_policy_sweep(first.id)
        .await
        .expect("второй проход");
    assert_eq!(second.queued, 520);
    assert_eq!(second.remaining, 0);
    assert_eq!(second.state, "completed");

    assert!(
        takeaways(&db, archived).await.is_empty(),
        "архив уборка списков не трогает"
    );
    assert!(
        takeaways(&db, sent_copy).await.is_empty(),
        "отправленные уборка не трогает"
    );
    assert!(
        takeaways(&db, after_snapshot).await.is_empty(),
        "письмо после снимка уборкой не берётся"
    );
    db.process_sync_batch_stages().await.expect("стадии");
    let late = takeaways(&db, after_snapshot).await;
    assert_eq!(late.len(), 1, "письмо после снимка уводит стадия списков");
    assert_eq!(late[0].2, Some(trash), "письмо уходит в корзину");

    // Одна операция уже ушла на сервер: отменить её нельзя, и её число
    // называется пользователю (S-042).
    sqlx::query(
        "UPDATE outbox_ops SET status='processing' WHERE id=(SELECT min(id) FROM outbox_ops)",
    )
    .execute(&db.write_pool)
    .await
    .expect("операция в работе");
    let release = db
        .delete_sender_policy(policy.id)
        .await
        .expect("снятие блокировки");
    assert_eq!(release.irreversible, 1);
    assert!(
        release.cancelled >= 520,
        "неисполненные перемещения отменены"
    );
    assert!(
        release.kept_in_trash > 0,
        "уже убранные письма остаются в корзине"
    );
    let left: (i64,) =
        sqlx::query_as("SELECT count(*) FROM outbox_ops WHERE status IN ('pending','retry')")
            .fetch_one(&db.pool)
            .await
            .expect("остаток очереди");
    assert_eq!(left.0, 0);
    // Отмена вернула письма стадиям: признак закрывшей стадии снят, иначе
    // письмо осталось бы закрытым навсегда и выпало бы из разбора и из
    // уведомления о новой почте.
    let still_closed: (i64,) = sqlx::query_as(
        "SELECT count(*) FROM messages WHERE folder_id=? AND closed_by_stage IS NOT NULL",
    )
    .bind(inbox)
    .fetch_one(&db.pool)
    .await
    .expect("закрытые письма");
    assert_eq!(
        still_closed.0, 1,
        "закрытым осталось только письмо, перемещение которого уже выполняется"
    );
    db.close().await;
}

/// Игнорирование переписки целиком: обход связей по трём заголовкам, уборка
/// текущих писем с запоминанием примет, новое письмо той же переписки,
/// догруженное прокруткой письмо и возврат писем из корзины по приметам
/// (ignore-conversation.md S-006 - S-008, S-016, S-020, S-025, S-026,
/// S-033 - S-039).
#[tokio::test]
async fn ignored_conversation_sweeps_current_and_future_messages_then_returns_them() {
    let db = test_db("stage-ignore").await;
    let account = seed_account(&db, "me@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    let trash = seed_folder(&db, account, "Trash", Some("trash")).await;
    let root = seed_message(
        &db,
        account,
        inbox,
        1,
        Seed {
            from: "one@example.test",
            subject: "Обсуждение",
            date: Some("2026-09-01T10:00:00+00:00"),
            message_id: Some("<root@example.test>"),
            ..Seed::default()
        },
    )
    .await;
    // Ветвь через References без In-Reply-To: по одному лишь thread_id она в
    // переписку не попала бы.
    let branch = seed_message(
        &db,
        account,
        inbox,
        2,
        Seed {
            from: "two@example.test",
            subject: "Re: Обсуждение",
            date: Some("2026-09-02T10:00:00+00:00"),
            message_id: Some("<branch@example.test>"),
            references: Some("<root@example.test>"),
            ..Seed::default()
        },
    )
    .await;
    let foreign = seed_message(
        &db,
        account,
        inbox,
        3,
        Seed {
            from: "three@example.test",
            subject: "Обсуждение",
            message_id: Some("<foreign@example.test>"),
            ..Seed::default()
        },
    )
    .await;

    let preview = db
        .preview_ignore_conversation(root)
        .await
        .expect("предпросмотр");
    assert_eq!(preview.total, 2, "тема чужое письмо в переписку не тянет");
    assert!(!preview.partial);
    let record = db
        .enable_ignore_conversation(root, &preview.snapshot_key, true)
        .await
        .expect("включение игнорирования");
    assert_eq!(record.state, IGNORE_STATE_ENABLED);

    for message in [root, branch] {
        let queued = takeaways(&db, message).await;
        assert_eq!(queued.len(), 1, "письмо переписки уходит в корзину");
        assert_eq!(queued[0].2, Some(trash));
    }
    assert!(takeaways(&db, foreign).await.is_empty());

    // Новое письмо переписки: его уводит стадия игнорируемых переписок.
    let future = seed_message(
        &db,
        account,
        inbox,
        4,
        Seed {
            from: "two@example.test",
            subject: "Re: Обсуждение",
            date: Some("2026-09-03T10:00:00+00:00"),
            message_id: Some("<future@example.test>"),
            in_reply_to: Some("<branch@example.test>"),
            ..Seed::default()
        },
    )
    .await;
    db.process_sync_batch_stages().await.expect("стадии");
    assert_eq!(takeaways(&db, future).await.len(), 1);
    assert_eq!(
        closed_by(&db, future).await.as_deref(),
        Some(IGNORED_CONVERSATION_STAGE_NAME)
    );

    // Догруженное прокруткой письмо: путь догрузки выполняет стадию
    // игнорируемых переписок (S-026).
    let backfilled = seed_message(
        &db,
        account,
        inbox,
        5,
        Seed {
            from: "one@example.test",
            subject: "Re: Обсуждение",
            date: Some("2026-08-20T10:00:00+00:00"),
            message_id: Some("<old@example.test>"),
            references: Some("<root@example.test>"),
            backfilled: true,
            ..Seed::default()
        },
    )
    .await;
    db.process_backfill_stages().await.expect("стадии догрузки");
    assert_eq!(takeaways(&db, backfilled).await.len(), 1);

    // Приметы письма сохранены до перемещения (S-020).
    let moves: Vec<(Option<i64>, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT source_folder_id, from_addr, header_id FROM ignored_conversation_moves
          WHERE conversation_id=? ORDER BY id",
    )
    .bind(record.id)
    .fetch_all(&db.pool)
    .await
    .expect("приметы писем");
    assert_eq!(moves.len(), 4);
    assert!(
        moves
            .iter()
            .all(|(folder, _, header)| { *folder == Some(inbox) && header.is_some() })
    );

    // Очередь выполнила перенос по-настоящему: успешная операция удаляет и
    // саму себя, и локальную строку письма (S-037).
    let moved_headers = complete_queue(&db, account).await;
    let gone: (i64,) = sqlx::query_as("SELECT count(*) FROM messages WHERE id IN (?, ?, ?)")
        .bind(root)
        .bind(branch)
        .bind(future)
        .fetch_one(&db.pool)
        .await
        .expect("локальные строки писем");
    assert_eq!(gone.0, 0, "успешное перемещение удалило локальные строки");

    // Следующая синхронизация принесла письма уже в корзине: возврат ищет их
    // там по приметам, а не по прежнему номеру строки. Одно письмо в корзине
    // так и не появилось - его возврат остаётся ждать синхронизации (S-041).
    for (index, header) in moved_headers
        .iter()
        .filter(|header| !header.message_id.contains("old@example.test"))
        .enumerate()
    {
        seed_message(
            &db,
            account,
            trash,
            100 + index as i64,
            Seed {
                from: &header.from,
                subject: "в корзине",
                date: Some(&header.date),
                message_id: Some(&header.message_id),
                ..Seed::default()
            },
        )
        .await;
    }

    let report = db
        .disable_ignore_conversation(record.id, true)
        .await
        .expect("прекращение с возвратом");
    assert_eq!(
        report.queued, 0,
        "возврат не считается выполненным до ответа очереди"
    );
    assert!(report.remaining >= 3, "письма ждут выполнения возврата");
    // S-035: повторная команда во время возврата второго задания не заводит.
    assert!(
        db.disable_ignore_conversation(record.id, true)
            .await
            .is_err(),
        "повторное прекращение во время возврата отклоняется"
    );
    let jobs: (i64,) = sqlx::query_as(
        "SELECT count(*) FROM ignored_conversation_jobs WHERE conversation_id=? AND kind='return'",
    )
    .bind(record.id)
    .fetch_one(&db.pool)
    .await
    .expect("задания возврата");
    assert_eq!(jobs.0, 1, "второго задания возврата не появилось");

    // Очередь выполнила возврат: только теперь письма считаются возвращёнными.
    complete_queue(&db, account).await;
    db.process_sync_batch_stages().await.expect("стадии");
    let after = db
        .ignored_conversation(record.id)
        .await
        .expect("запись после возврата");
    assert_eq!(after.returned, 3, "возврат подтверждён результатом очереди");
    assert_eq!(
        after.state, IGNORE_STATE_RETURNING,
        "ненайденное письмо держит запись в состоянии возврата"
    );

    // S-042, S-043: за семь суток письмо в корзине так и не появилось - его
    // возврат переходит в отказ, а запись в состояние неполного возврата.
    sqlx::query(
        "UPDATE ignored_conversation_moves
            SET return_requested_at=datetime('now', '-8 days')
          WHERE conversation_id=? AND return_state='waiting_sync'",
    )
    .bind(record.id)
    .execute(&db.write_pool)
    .await
    .expect("истёкший срок ожидания");
    db.process_sync_batch_stages().await.expect("стадии");
    let expired = db
        .ignored_conversation(record.id)
        .await
        .expect("запись после срока");
    assert_eq!(expired.failed, 1, "непрошедшее возврат письмо названо");
    assert_eq!(expired.state, IGNORE_STATE_RETURN_FAILED);
    db.close().await;
}

/// Собственное письмо в папке отправленных снимает игнорирование переписки
/// (S-030).
#[tokio::test]
async fn sent_message_disables_ignoring() {
    let db = test_db("stage-ignore-sent").await;
    let account = seed_account(&db, "me@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    seed_folder(&db, account, "Trash", Some("trash")).await;
    let sent = seed_folder(&db, account, "Sent", Some("sent")).await;
    let root = seed_message(
        &db,
        account,
        inbox,
        1,
        Seed {
            from: "one@example.test",
            subject: "Обсуждение",
            message_id: Some("<root@example.test>"),
            ..Seed::default()
        },
    )
    .await;
    let preview = db
        .preview_ignore_conversation(root)
        .await
        .expect("предпросмотр");
    let record = db
        .enable_ignore_conversation(root, &preview.snapshot_key, true)
        .await
        .expect("включение");

    let answer = seed_message(
        &db,
        account,
        sent,
        2,
        Seed {
            from: "me@example.test",
            subject: "Re: Обсуждение",
            message_id: Some("<answer@example.test>"),
            in_reply_to: Some("<root@example.test>"),
            ..Seed::default()
        },
    )
    .await;
    db.process_sync_batch_stages().await.expect("стадии");
    assert!(
        takeaways(&db, answer).await.is_empty(),
        "отправленное письмо в корзину не уходит"
    );
    let after = db
        .ignored_conversation(record.id)
        .await
        .expect("запись после ответа");
    assert_eq!(after.state, IGNORE_STATE_DISABLED);
    db.close().await;
}

/// Автоочистка по отправителю: режим "только последнее" оставляет одно письмо,
/// режим "старше N дней" берёт только письма с разобранной датой за границей,
/// архив трогается лишь по отдельному согласию, а режим "новые сразу"
/// превращается в обычное правило (sweep-by-sender.md S-008, S-015, S-016,
/// S-022 - S-025, S-029, S-033, S-034).
#[tokio::test]
async fn sweep_modes_keep_the_last_message_and_respect_dates() {
    let db = test_db("stage-sweep-modes").await;
    let account = seed_account(&db, "me@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    let trash = seed_folder(&db, account, "Trash", Some("trash")).await;
    let archive = seed_folder(&db, account, "Archive", Some("archive")).await;

    let old = seed_message(
        &db,
        account,
        inbox,
        1,
        Seed {
            from: "shop@example.test",
            subject: "старое",
            date: Some(&days_ago(40)),
            ..Seed::default()
        },
    )
    .await;
    let fresh = seed_message(
        &db,
        account,
        inbox,
        2,
        Seed {
            from: "shop@example.test",
            subject: "свежее",
            date: Some(&days_ago(2)),
            ..Seed::default()
        },
    )
    .await;
    let undated = seed_message(
        &db,
        account,
        inbox,
        3,
        Seed {
            from: "shop@example.test",
            subject: "без даты",
            ..Seed::default()
        },
    )
    .await;
    let archived = seed_message(
        &db,
        account,
        archive,
        4,
        Seed {
            from: "shop@example.test",
            subject: "в архиве",
            date: Some(&days_ago(50)),
            ..Seed::default()
        },
    )
    .await;

    // Режим "старше N дней" без согласия на архив.
    let input = SenderSweepInput {
        address: "shop@example.test".into(),
        mode: SWEEP_MODE_OLDER_THAN.into(),
        account_id: Some(account),
        days: Some(30),
        sweep_archive: false,
    };
    let preview = db
        .preview_sender_sweep(input.clone())
        .await
        .expect("предпросмотр");
    assert_eq!(
        preview.total, 1,
        "берётся только письмо с датой старше границы"
    );
    let report = db
        .start_sender_sweep(input, &preview.snapshot_key)
        .await
        .expect("уборка");
    assert_eq!(report.queued, 1);
    assert_eq!(takeaways(&db, old).await.len(), 1);
    assert_eq!(takeaways(&db, old).await[0].2, Some(trash));
    assert!(
        takeaways(&db, undated).await.is_empty(),
        "письмо без даты не убирается"
    );
    assert!(takeaways(&db, fresh).await.is_empty());
    assert!(
        takeaways(&db, archived).await.is_empty(),
        "архив без согласия не трогается"
    );

    // Вторая включённая запись для того же адреса и области не создаётся.
    let again = SenderSweepInput {
        address: "shop@example.test".into(),
        mode: SWEEP_MODE_ONLY_LAST.into(),
        account_id: Some(account),
        days: None,
        sweep_archive: true,
    };
    let second_preview = db
        .preview_sender_sweep(again.clone())
        .await
        .expect("предпросмотр второй записи");
    let refused = db
        .start_sender_sweep(again, &second_preview.snapshot_key)
        .await;
    assert!(
        refused.is_err(),
        "второй включённой записи адреса не бывает"
    );

    // Режим "только последнее" для другого отправителя вместе с архивом.
    for uid in 10..13 {
        seed_message(
            &db,
            account,
            inbox,
            uid,
            Seed {
                from: "news@example.test",
                subject: "выпуск",
                date: Some(&days_ago(10 - uid)),
                ..Seed::default()
            },
        )
        .await;
    }
    let only_last = SenderSweepInput {
        address: "news@example.test".into(),
        mode: SWEEP_MODE_ONLY_LAST.into(),
        account_id: Some(account),
        days: None,
        sweep_archive: false,
    };
    let preview = db
        .preview_sender_sweep(only_last.clone())
        .await
        .expect("предпросмотр");
    assert_eq!(preview.total, 2, "последнее письмо остаётся");
    let report = db
        .start_sender_sweep(only_last, &preview.snapshot_key)
        .await
        .expect("уборка");
    assert_eq!(report.queued, 2);
    let kept: (i64,) = sqlx::query_as(
        "SELECT count(*) FROM messages m
          WHERE lower(m.from_addr)='news@example.test'
            AND NOT EXISTS (SELECT 1 FROM outbox_ops o WHERE o.message_id=m.id)",
    )
    .fetch_one(&db.pool)
    .await
    .expect("оставшиеся письма");
    assert_eq!(kept.0, 1);

    // Режим "новые сразу" записи автоочистки не создаёт: он становится обычным
    // правилом с точным условием по адресу отправителя.
    let new_now = SenderSweepInput {
        address: "ads@example.test".into(),
        mode: SWEEP_MODE_NEW_NOW.into(),
        account_id: None,
        days: None,
        sweep_archive: false,
    };
    // S-010, S-011: режим "новые сразу" идёт общим путём подтверждения, и ключ
    // снимка расходуется неделимо.
    let new_now_preview = db
        .preview_sender_sweep(new_now.clone())
        .await
        .expect("предпросмотр режима \"новые сразу\"");
    assert!(
        db.start_sender_sweep(new_now.clone(), "").await.is_err(),
        "без ключа снимка правило не создаётся"
    );
    db.start_sender_sweep(new_now.clone(), &new_now_preview.snapshot_key)
        .await
        .expect("правило");
    assert!(
        db.start_sender_sweep(new_now, &new_now_preview.snapshot_key)
            .await
            .is_err(),
        "тот же ключ снимка второй раз не принимается"
    );
    let rules = db.list_mail_rules().await.expect("список правил");
    let created = rules
        .iter()
        .find(|rule| rule.name.contains("ads@example.test"))
        .expect("правило режима \"новые сразу\"");
    assert_eq!(created.groups[0].conditions[0].field, "sender_address");
    assert_eq!(created.groups[0].conditions[0].op, "equals");
    assert!(created.actions.iter().any(|action| action.kind == "trash"));
    assert!(created.actions.iter().any(|action| action.kind == "stop"));

    // S-036: приход письма проверяет запись, но полным проходом не считается.
    // Отметка прохода старится намеренно: событийное задание не должно её
    // обновить, иначе суточный полный проход не наступит никогда.
    sqlx::query("UPDATE sender_sweep_rules SET last_full_pass_at=datetime('now', '-12 hours')")
        .execute(&db.write_pool)
        .await
        .expect("прежний полный проход");
    let aged = db.list_sender_sweep_rules().await.expect("записи")[0]
        .last_full_pass_at
        .clone();
    let newest = seed_message(
        &db,
        account,
        inbox,
        20,
        Seed {
            from: "news@example.test",
            subject: "самое новое",
            // Дата заведомо позже дат прежних писем этого отправителя.
            date: Some("2030-01-01T00:00:00+00:00"),
            ..Seed::default()
        },
    )
    .await;
    db.process_sync_batch_stages().await.expect("стадии");
    assert!(
        takeaways(&db, newest).await.is_empty(),
        "новое письмо остаётся последним"
    );
    assert_eq!(
        db.list_sender_sweep_rules().await.expect("записи")[0].last_full_pass_at,
        aged,
        "приход письма за полный проход не засчитывается"
    );

    db.close().await;
}

/// Полный проход включённой записи автоочистки выполняется при запуске
/// программы, а не только по суточному сроку (sweep-by-sender.md S-035): у
/// пользователя без синхронизации запись иначе не работала бы вовсе.
#[tokio::test]
async fn full_sweep_pass_runs_on_start() {
    let db = test_db("stage-sweep-start").await;
    let account = seed_account(&db, "me@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    let trash = seed_folder(&db, account, "Trash", Some("trash")).await;
    let old = seed_message(
        &db,
        account,
        inbox,
        1,
        Seed {
            from: "news@example.test",
            subject: "прежнее",
            date: Some(&days_ago(10)),
            ..Seed::default()
        },
    )
    .await;
    let last = seed_message(
        &db,
        account,
        inbox,
        2,
        Seed {
            from: "news@example.test",
            subject: "последнее",
            date: Some(&days_ago(1)),
            ..Seed::default()
        },
    )
    .await;
    // Запись заводится прямым запросом: она уже была создана в прошлом запуске,
    // а свежий проход по ней с тех пор не выполнялся.
    sqlx::query(
        "INSERT INTO sender_sweep_rules(address, account_id, mode, last_full_pass_at)
         VALUES('news@example.test', ?, 'only_last', datetime('now'))",
    )
    .bind(account)
    .execute(&db.write_pool)
    .await
    .expect("запись автоочистки");

    db.resume_stage_jobs().await.expect("запуск программы");
    assert_eq!(
        takeaways(&db, old).await.len(),
        1,
        "полный проход при запуске убрал прежнее письмо"
    );
    assert_eq!(takeaways(&db, old).await[0].2, Some(trash));
    assert!(
        takeaways(&db, last).await.is_empty(),
        "последнее письмо остаётся"
    );
    db.close().await;
}

/// Конфликт ограничения очереди пропускает письмо со счётчиком, а не роняет
/// проход: письмо с чужой незавершённой операцией переводит проход автоочистки
/// в состояние ожидания (sweep-by-sender.md S-004, S-019 - S-021).
#[tokio::test]
async fn busy_message_only_postpones_the_pass() {
    let db = test_db("stage-busy").await;
    let account = seed_account(&db, "me@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    let archive = seed_folder(&db, account, "Archive", Some("archive")).await;
    seed_folder(&db, account, "Trash", Some("trash")).await;
    let busy = seed_message(
        &db,
        account,
        inbox,
        1,
        Seed {
            from: "shop@example.test",
            subject: "занятое",
            date: Some(&days_ago(40)),
            ..Seed::default()
        },
    )
    .await;
    let free = seed_message(
        &db,
        account,
        inbox,
        2,
        Seed {
            from: "shop@example.test",
            subject: "свободное",
            date: Some(&days_ago(41)),
            ..Seed::default()
        },
    )
    .await;
    // Пользователь уже убрал письмо в архив вручную.
    sqlx::query(
        "INSERT INTO outbox_ops(account_id, message_id, op_kind, payload, status, next_attempt_at)
         VALUES(?, ?, 'move', ?, 'pending', datetime('now'))",
    )
    .bind(account)
    .bind(busy)
    .bind(serde_json::json!({"message_id": busy, "target_folder_id": archive}).to_string())
    .execute(&db.write_pool)
    .await
    .expect("чужая операция");

    let input = SenderSweepInput {
        address: "shop@example.test".into(),
        mode: SWEEP_MODE_OLDER_THAN.into(),
        account_id: Some(account),
        days: Some(30),
        sweep_archive: false,
    };
    let preview = db
        .preview_sender_sweep(input.clone())
        .await
        .expect("предпросмотр");
    let report = db
        .start_sender_sweep(input, &preview.snapshot_key)
        .await
        .expect("уборка");
    assert_eq!(report.queued, 1, "свободное письмо убрано");
    assert_eq!(report.skipped, 1, "занятое письмо пропущено со счётчиком");
    assert_eq!(report.state, SWEEP_JOB_WAITING);
    assert_eq!(report.remaining, 1, "занятое письмо осталось в остатке");
    assert_eq!(
        takeaways(&db, busy).await.len(),
        1,
        "второй операции по занятому письму не появилось"
    );
    assert_eq!(takeaways(&db, free).await.len(), 1);

    // Проход повторяется, пока письмо занято: остаток не тает сам собой.
    let again = db
        .continue_sender_sweep_job(report.id)
        .await
        .expect("повторный проход");
    assert_eq!(again.state, SWEEP_JOB_WAITING);
    assert_eq!(again.remaining, 1, "занятое письмо из остатка не выпало");

    // Пользователь отменил своё перемещение: письмо освободилось, и проход
    // возвращается именно к нему, хотя курсор давно ушёл вперёд.
    sqlx::query("DELETE FROM outbox_ops WHERE message_id=?")
        .bind(busy)
        .execute(&db.write_pool)
        .await
        .expect("чужая операция отменена");
    let finished = db
        .continue_sender_sweep_job(report.id)
        .await
        .expect("проход после освобождения письма");
    assert_eq!(
        takeaways(&db, busy).await.len(),
        1,
        "освободившееся письмо убрано следующим проходом"
    );
    assert_eq!(finished.remaining, 0, "остаток сошёлся с настоящей работой");
    assert_eq!(finished.state, SWEEP_JOB_COMPLETED);

    // Пользователь передумал: отмена уборки удаляет неисполненные перемещения
    // и возвращает письма стадиям - иначе они остались бы закрытыми навсегда.
    db.cancel_sender_sweep_job(report.id)
        .await
        .expect("отмена уборки");
    assert!(takeaways(&db, busy).await.is_empty());
    assert!(takeaways(&db, free).await.is_empty());
    for message in [busy, free] {
        assert!(
            closed_by(&db, message).await.is_none(),
            "признак закрывшей стадии снят вместе с отменой перемещения"
        );
    }
    db.close().await;
}

/// Обновление на непустой базе: запись блокировки, заведённая до первой
/// синхронизации, не уводит в корзину всю прежнюю почту отправителя
/// (blocked-senders.md S-021, S-030 - S-031). Стадия берёт только письма,
/// сохранённые после обновления.
#[tokio::test]
async fn policy_added_before_first_sync_keeps_old_messages() {
    let db = test_db("stage-cursor-seed").await;
    let account = seed_account(&db, "me@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    seed_folder(&db, account, "Trash", Some("trash")).await;
    let mut old = Vec::new();
    for uid in 0..5 {
        old.push(
            seed_message(
                &db,
                account,
                inbox,
                uid,
                Seed {
                    from: "news@example.test",
                    subject: "прежний выпуск",
                    ..Seed::default()
                },
            )
            .await,
        );
    }
    // База прежней версии стадий не знала и строк курсора не содержит:
    // обновление должно завести их само.
    sqlx::query("DELETE FROM stage_progress")
        .execute(&db.write_pool)
        .await
        .expect("база до стадий");
    db.migrate().await.expect("обновление");

    db.save_sender_policy(
        POLICY_KIND_ADDRESS,
        "news@example.test",
        POLICY_DECISION_BLOCKED,
        false,
    )
    .await
    .expect("запись блокировки до первой синхронизации");
    db.process_sync_batch_stages().await.expect("стадии");
    for message in &old {
        assert!(
            takeaways(&db, *message).await.is_empty(),
            "прежняя почта остаётся на месте: согласия на её уборку не было"
        );
    }

    // Новое письмо того же отправителя стадия уводит как обычно.
    let fresh = seed_message(
        &db,
        account,
        inbox,
        10,
        Seed {
            from: "news@example.test",
            subject: "новый выпуск",
            ..Seed::default()
        },
    )
    .await;
    db.process_sync_batch_stages().await.expect("стадии");
    assert_eq!(takeaways(&db, fresh).await.len(), 1);
    db.close().await;
}

/// Подозрительная коллизия идентификатора не объединяет переписки, а связь,
/// подтверждённая письмом одной из записей, объединяет
/// (ignore-conversation.md S-045, S-046).
#[tokio::test]
async fn forged_identifier_does_not_merge_conversations() {
    let db = test_db("stage-ignore-merge").await;
    let account = seed_account(&db, "me@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    seed_folder(&db, account, "Trash", Some("trash")).await;
    let first_root = seed_message(
        &db,
        account,
        inbox,
        1,
        Seed {
            from: "one@example.test",
            subject: "Первая",
            message_id: Some("<first@example.test>"),
            ..Seed::default()
        },
    )
    .await;
    let second_root = seed_message(
        &db,
        account,
        inbox,
        2,
        Seed {
            from: "two@example.test",
            subject: "Вторая",
            message_id: Some("<second@example.test>"),
            ..Seed::default()
        },
    )
    .await;
    for message in [first_root, second_root] {
        let preview = db
            .preview_ignore_conversation(message)
            .await
            .expect("предпросмотр");
        db.enable_ignore_conversation(message, &preview.snapshot_key, true)
            .await
            .expect("включение игнорирования");
    }
    assert_eq!(
        db.list_ignored_conversations().await.expect("список").len(),
        2
    );

    // Чужое письмо со ссылками сразу на обе переписки: своим его не признаёт
    // ни одна запись, поэтому наборы объединять нельзя.
    let forged = seed_message(
        &db,
        account,
        inbox,
        3,
        Seed {
            from: "stranger@example.test",
            subject: "Подделка",
            message_id: Some("<forged@example.test>"),
            references: Some("<first@example.test> <second@example.test>"),
            ..Seed::default()
        },
    )
    .await;
    let preview = db
        .preview_ignore_conversation(forged)
        .await
        .expect("предпросмотр подделки");
    let refused = db
        .enable_ignore_conversation(forged, &preview.snapshot_key, true)
        .await;
    assert!(refused.is_err(), "подделанные ссылки наборы не объединяют");
    let records = db.list_ignored_conversations().await.expect("список");
    assert_eq!(records.len(), 2, "записи остались раздельными");
    assert!(
        records.iter().any(|record| record.last_error.is_some()),
        "коллизия записана как отказ и видна пользователю"
    );

    // Настоящая связь: письмо первой переписки отвечает письму второй, поэтому
    // первая запись признаёт его своим и цепочка подтверждена.
    let bridge = seed_message(
        &db,
        account,
        inbox,
        4,
        Seed {
            from: "one@example.test",
            subject: "Re: Первая",
            message_id: Some("<bridge@example.test>"),
            in_reply_to: Some("<first@example.test>"),
            references: Some("<second@example.test>"),
            ..Seed::default()
        },
    )
    .await;
    db.process_sync_batch_stages().await.expect("стадии");
    let preview = db
        .preview_ignore_conversation(bridge)
        .await
        .expect("предпросмотр связи");
    db.enable_ignore_conversation(bridge, &preview.snapshot_key, true)
        .await
        .expect("слияние по подтверждённой цепочке");
    assert_eq!(
        db.list_ignored_conversations().await.expect("список").len(),
        1,
        "подтверждённая цепочка объединила записи"
    );
    db.close().await;
}

/// Пределы набора и числа записей: набор, упёршийся в предел идентификаторов,
/// переходит в частичное покрытие, а тысяча первая переписка не включается
/// (ignore-conversation.md S-028, S-029).
#[tokio::test]
async fn conversation_limits_are_enforced() {
    let db = test_db("stage-ignore-limits").await;
    let account = seed_account(&db, "me@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    seed_folder(&db, account, "Trash", Some("trash")).await;
    let limits = LimitSet::defaults();
    let max_ids = limits.count(LIMIT_CONVERSATION_IDS);
    let references = (0..max_ids + 200)
        .map(|index| format!("<ref{index}@example.test>"))
        .collect::<Vec<_>>()
        .join(" ");
    let root = seed_message(
        &db,
        account,
        inbox,
        1,
        Seed {
            from: "one@example.test",
            subject: "Длинная переписка",
            message_id: Some("<huge@example.test>"),
            references: Some(&references),
            ..Seed::default()
        },
    )
    .await;
    let preview = db
        .preview_ignore_conversation(root)
        .await
        .expect("предпросмотр");
    assert!(preview.partial, "набор упёрся в предел идентификаторов");
    let record = db
        .enable_ignore_conversation(root, &preview.snapshot_key, true)
        .await
        .expect("включение");
    assert!(record.partial);
    assert!(
        record.ids_count <= max_ids as i64,
        "в наборе не больше предела идентификаторов"
    );

    // Предел числа записей: остальные заводятся прямым запросом, потому что
    // тысяча настоящих переписок в проверке не нужна.
    for index in 0..limits.get(LIMIT_IGNORED_CONVERSATIONS) - 1 {
        sqlx::query("INSERT INTO ignored_conversations(account_id, subject) VALUES(?, ?)")
            .bind(account)
            .bind(format!("переписка {index}"))
            .execute(&db.write_pool)
            .await
            .expect("запись");
    }
    let extra = seed_message(
        &db,
        account,
        inbox,
        2,
        Seed {
            from: "two@example.test",
            subject: "Ещё одна",
            message_id: Some("<extra@example.test>"),
            ..Seed::default()
        },
    )
    .await;
    let preview = db
        .preview_ignore_conversation(extra)
        .await
        .expect("предпросмотр");
    let refused = db
        .enable_ignore_conversation(extra, &preview.snapshot_key, true)
        .await;
    assert!(refused.is_err(), "тысяча первая переписка не включается");
    db.close().await;
}

/// Удаление ящика: задания, записи игнорирования и записи автоочистки этого
/// ящика уходят каскадом, общие списки и записи всех ящиков остаются
/// (blocked-senders.md S-049, ignore-conversation.md S-050,
/// sweep-by-sender.md S-045).
#[tokio::test]
async fn removing_account_keeps_shared_lists() {
    let db = test_db("stage-account-delete").await;
    let first = seed_account(&db, "first@example.test").await;
    let second = seed_account(&db, "second@example.test").await;
    let inbox = seed_folder(&db, first, "INBOX", Some("inbox")).await;
    seed_folder(&db, first, "Trash", Some("trash")).await;
    seed_folder(&db, second, "INBOX", Some("inbox")).await;
    let root = seed_message(
        &db,
        first,
        inbox,
        1,
        Seed {
            from: "news@example.test",
            subject: "Переписка",
            message_id: Some("<root@example.test>"),
            ..Seed::default()
        },
    )
    .await;
    let preview = db
        .preview_ignore_conversation(root)
        .await
        .expect("предпросмотр");
    db.enable_ignore_conversation(root, &preview.snapshot_key, true)
        .await
        .expect("игнорирование");
    let policy_preview = db
        .preview_sender_policy(POLICY_KIND_ADDRESS, "news@example.test")
        .await
        .expect("предпросмотр списка");
    let policy = db
        .save_sender_policy(
            POLICY_KIND_ADDRESS,
            &policy_preview.value,
            POLICY_DECISION_BLOCKED,
            false,
        )
        .await
        .expect("запись списка");
    db.start_sender_policy_sweep(policy.id, &policy_preview.snapshot_key, true)
        .await
        .expect("уборка");
    let sweep = SenderSweepInput {
        address: "news@example.test".into(),
        mode: SWEEP_MODE_ONLY_LAST.into(),
        account_id: Some(first),
        days: None,
        sweep_archive: false,
    };
    let sweep_preview = db
        .preview_sender_sweep(sweep.clone())
        .await
        .expect("предпросмотр автоочистки");
    db.start_sender_sweep(sweep, &sweep_preview.snapshot_key)
        .await
        .expect("запись автоочистки");

    // Ящик удаляется строкой таблицы: всё остальное уносит каскад схемы.
    sqlx::query("DELETE FROM accounts WHERE id=?")
        .bind(first)
        .execute(&db.write_pool)
        .await
        .expect("удаление ящика");

    let conversations = db.list_ignored_conversations().await.expect("переписки");
    assert!(
        conversations.is_empty(),
        "записи игнорирования ушли вместе с ящиком"
    );
    let jobs: (i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE account_id=?), count(*) FILTER (WHERE account_id=?)
           FROM sender_policy_jobs",
    )
    .bind(first)
    .bind(second)
    .fetch_one(&db.pool)
    .await
    .expect("задания уборки");
    assert_eq!(jobs.0, 0, "задания уборки удалённого ящика ушли каскадом");
    assert_eq!(jobs.1, 1, "задание уборки оставшегося ящика сохранилось");
    assert!(
        db.list_sender_sweep_rules()
            .await
            .expect("автоочистка")
            .is_empty(),
        "запись автоочистки области ящика удалена"
    );
    let policies = db.list_sender_policies().await.expect("списки");
    assert_eq!(
        policies.len(),
        1,
        "общие списки отправителей переживают удаление ящика"
    );
    db.close().await;
}

/// Задания, брошенные закрытием программы, возвращаются в очередь при
/// следующем запуске (blocked-senders.md S-038, ignore-conversation.md S-048,
/// sweep-by-sender.md S-043).
#[tokio::test]
async fn abandoned_jobs_return_to_pending_on_start() {
    let db = test_db("stage-restore").await;
    let account = seed_account(&db, "me@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    seed_folder(&db, account, "Trash", Some("trash")).await;
    seed_message(
        &db,
        account,
        inbox,
        1,
        Seed {
            from: "news@example.test",
            subject: "выпуск",
            ..Seed::default()
        },
    )
    .await;
    let preview = db
        .preview_sender_policy(POLICY_KIND_ADDRESS, "news@example.test")
        .await
        .expect("предпросмотр");
    let policy = db
        .save_sender_policy(
            POLICY_KIND_ADDRESS,
            &preview.value,
            POLICY_DECISION_BLOCKED,
            false,
        )
        .await
        .expect("запись");
    db.start_sender_policy_sweep(policy.id, &preview.snapshot_key, true)
        .await
        .expect("уборка");
    sqlx::query("UPDATE sender_policy_jobs SET state='running', lease_expires_at=datetime('now')")
        .execute(&db.write_pool)
        .await
        .expect("брошенное задание");
    db.migrate().await.expect("повторный запуск");
    let state: (String,) = sqlx::query_as("SELECT state FROM sender_policy_jobs LIMIT 1")
        .fetch_one(&db.pool)
        .await
        .expect("состояние задания");
    assert_eq!(state.0, "pending");

    // S-035: запуск программы продвигает задания сам, без действий
    // пользователя и без синхронизации.
    db.resume_stage_jobs().await.expect("запуск заданий");
    let finished: (String,) = sqlx::query_as("SELECT state FROM sender_policy_jobs LIMIT 1")
        .fetch_one(&db.pool)
        .await
        .expect("состояние задания после запуска");
    assert_eq!(
        finished.0, "completed",
        "брошенная уборка доведена до конца"
    );
    db.close().await;
}

/// Автоочистка на пути догрузки и после нового письма: режим "только
/// последнее" оставляет самое новое письмо и убирает прежнее последнее, а
/// письмо, поднятое прокруткой, уводит стадия автоочистки - списки
/// отправителей и обычные правила этот путь не выполняет (S-030, S-031,
/// S-036).
#[tokio::test]
async fn sweep_stage_handles_new_and_backfilled_messages() {
    let db = test_db("stage-sweep-stage").await;
    let account = seed_account(&db, "me@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    let trash = seed_folder(&db, account, "Trash", Some("trash")).await;
    let first = seed_message(
        &db,
        account,
        inbox,
        1,
        Seed {
            from: "news@example.test",
            subject: "первое",
            date: Some(&days_ago(5)),
            ..Seed::default()
        },
    )
    .await;
    let input = SenderSweepInput {
        address: "news@example.test".into(),
        mode: SWEEP_MODE_ONLY_LAST.into(),
        account_id: Some(account),
        days: None,
        sweep_archive: false,
    };
    let preview = db
        .preview_sender_sweep(input.clone())
        .await
        .expect("предпросмотр");
    assert_eq!(preview.total, 0, "единственное письмо и есть последнее");
    db.start_sender_sweep(input, &preview.snapshot_key)
        .await
        .expect("запись автоочистки");
    assert!(takeaways(&db, first).await.is_empty());

    // Пришло новое письмо: последним становится оно, прежнее уходит в корзину.
    let second = seed_message(
        &db,
        account,
        inbox,
        2,
        Seed {
            from: "news@example.test",
            subject: "второе",
            date: Some(&days_ago(1)),
            ..Seed::default()
        },
    )
    .await;
    db.process_sync_batch_stages().await.expect("стадии");
    assert_eq!(
        takeaways(&db, first).await.len(),
        1,
        "прежнее последнее письмо убрано"
    );
    assert_eq!(takeaways(&db, first).await[0].2, Some(trash));
    assert!(
        takeaways(&db, second).await.is_empty(),
        "новое письмо остаётся последним"
    );

    // Письмо, поднятое прокруткой: списки отправителей его не берут, а
    // автоочистка берёт.
    db.save_sender_policy(
        POLICY_KIND_ADDRESS,
        "news@example.test",
        POLICY_DECISION_BLOCKED,
        false,
    )
    .await
    .expect("блокировка");
    let backfilled = seed_message(
        &db,
        account,
        inbox,
        3,
        Seed {
            from: "news@example.test",
            subject: "поднято прокруткой",
            date: Some(&days_ago(30)),
            backfilled: true,
            ..Seed::default()
        },
    )
    .await;
    db.process_backfill_stages().await.expect("стадии догрузки");
    let queued = takeaways(&db, backfilled).await;
    assert_eq!(queued.len(), 1, "старое письмо убирает автоочистка");
    assert_eq!(
        closed_by(&db, backfilled).await.as_deref(),
        Some(SENDER_SWEEP_STAGE_NAME),
        "на пути догрузки письмо закрывает автоочистка, а не списки отправителей"
    );
    db.close().await;
}

/// Ящик без корзины: списки отправителей закрывают письмо и дальше его не
/// пускают, а автоочистка оставляет письмо правилам обработки
/// (blocked-senders.md S-003, sweep-by-sender.md S-003).
#[tokio::test]
async fn mailbox_without_trash_closes_lists_but_lets_rules_run() {
    let db = test_db("stage-no-trash").await;
    let account = seed_account(&db, "me@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    let archive = seed_folder(&db, account, "Archive", Some("archive")).await;
    let rule = MailRuleInput {
        id: "rule-archive".into(),
        name: "В архив".into(),
        account_id: None,
        enabled: true,
        groups: vec![MailRuleGroup {
            logic: "all".into(),
            conditions: vec![MailRuleCondition {
                field: "subject".into(),
                op: "contains".into(),
                value: "счёт".into(),
                unit: None,
                value2: None,
            }],
        }],
        exceptions: Vec::new(),
        actions: vec![MailRuleAction {
            kind: "archive".into(),
            folder_id: None,
            folder_role: None,
            label_id: None,
        }],
        confirm_key: None,
    };
    db.save_mail_rule(&rule, false, None)
        .await
        .expect("правило");
    db.save_sender_policy(
        POLICY_KIND_ADDRESS,
        "blocked@example.test",
        POLICY_DECISION_BLOCKED,
        false,
    )
    .await
    .expect("блокировка");
    // Запись автоочистки заводится в обход подтверждения: снимок здесь не
    // проверяется, нужна только сама запись со стадией.
    sqlx::query(
        "INSERT INTO sender_sweep_rules(address, account_id, mode, days)
         VALUES('shop@example.test', ?, 'older_than', 10)",
    )
    .bind(account)
    .execute(&db.write_pool)
    .await
    .expect("запись автоочистки");

    let blocked = seed_message(
        &db,
        account,
        inbox,
        1,
        Seed {
            from: "blocked@example.test",
            subject: "счёт от заблокированного",
            ..Seed::default()
        },
    )
    .await;
    let swept = seed_message(
        &db,
        account,
        inbox,
        2,
        Seed {
            from: "shop@example.test",
            subject: "счёт из магазина",
            date: Some(&days_ago(40)),
            ..Seed::default()
        },
    )
    .await;

    db.process_sync_batch_stages().await.expect("стадии");
    assert!(
        takeaways(&db, blocked).await.is_empty(),
        "без корзины письмо остаётся на месте"
    );
    assert_eq!(
        closed_by(&db, blocked).await.as_deref(),
        Some(SENDER_POLICY_STAGE_NAME),
        "заблокированное письмо закрыто и правилам не передаётся"
    );
    let notify = db
        .inbox_message_ids_by_remote_ids(account, &[format!("remote-{inbox}-1")], None, None)
        .await
        .expect("письма для уведомления");
    assert!(
        notify.is_empty(),
        "закрытое письмо в уведомление не попадает"
    );

    let rule_move = takeaways(&db, swept).await;
    assert_eq!(
        rule_move.len(),
        1,
        "письмо автоочистки без корзины доходит до правил"
    );
    assert_eq!(rule_move[0].2, Some(archive));
    db.close().await;
}

/// Состарить снимок кандидатов: подтверждение открывают и возвращаются к нему
/// спустя часы, а ждать их в проверке нельзя.
async fn age_snapshot(db: &Db, key: &str, hours: i64) {
    sqlx::query("UPDATE stage_snapshots SET created_at=datetime('now', ?) WHERE key=?")
        .bind(format!("-{hours} hours"))
        .bind(key)
        .execute(&db.write_pool)
        .await
        .expect("состарить снимок");
}

/// Число заведённых заданий всех трёх стадий.
async fn job_counts(db: &Db) -> (i64, i64, i64) {
    let count = |sql: &'static str| async move {
        sqlx::query_as::<_, (i64,)>(sql)
            .fetch_one(&db.pool)
            .await
            .expect("посчитать задания")
            .0
    };
    (
        count("SELECT count(*) FROM sender_policy_jobs").await,
        count("SELECT count(*) FROM ignored_conversation_jobs").await,
        count("SELECT count(*) FROM sender_sweep_jobs").await,
    )
}

/// Список писем-кандидатов живёт ровно столько, сколько говорит настройка, и
/// это проверяется на всех трёх путях подтверждения: списками отправителей,
/// игнорированием переписки и автоочисткой по отправителю (blocked-senders.md
/// S-030, ignore-conversation.md S-016, sweep-by-sender.md S-011,
/// configurable-limits.md S-018).
///
/// Срок жизни снимка спрашивала только уборка - при запуске программы и перед
/// новым снимком. Между ними снимок жил сколько угодно, и подтверждение
/// недельной давности запускало массовое перемещение писем по списку, который
/// пользователь уже не видел. Само число сроком в сутки было вписано в оба
/// запроса уборки: кто открывал подтверждение вечером и возвращался к нему
/// через день, начинал сначала, а кому суток много, тот не мог их сократить.
/// Поэтому один и тот же возраст снимка проверяется дважды - при сроке короче
/// него и при сроке длиннее.
#[tokio::test]
async fn a_list_of_messages_expires_by_the_setting_on_every_confirmation_path() {
    let db = test_db("stage-stale-snapshot").await;
    let account = seed_account(&db, "me@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    let trash = seed_folder(&db, account, "Trash", Some("trash")).await;
    let blocked = seed_message(
        &db,
        account,
        inbox,
        1,
        Seed {
            from: "news@example.test",
            subject: "выпуск",
            ..Seed::default()
        },
    )
    .await;
    let talk = seed_message(
        &db,
        account,
        inbox,
        2,
        Seed {
            from: "one@example.test",
            subject: "Обсуждение",
            date: Some(&days_ago(3)),
            message_id: Some("<talk@example.test>"),
            ..Seed::default()
        },
    )
    .await;
    let swept = seed_message(
        &db,
        account,
        inbox,
        3,
        Seed {
            from: "shop@example.test",
            subject: "счёт",
            date: Some(&days_ago(40)),
            ..Seed::default()
        },
    )
    .await;
    let sweep_input = SenderSweepInput {
        address: "shop@example.test".into(),
        mode: SWEEP_MODE_OLDER_THAN.into(),
        account_id: Some(account),
        days: Some(30),
        sweep_archive: false,
    };

    let policy_preview = db
        .preview_sender_policy(POLICY_KIND_ADDRESS, "news@example.test")
        .await
        .expect("предпросмотр списка отправителей");
    let policy = db
        .save_sender_policy(
            POLICY_KIND_ADDRESS,
            &policy_preview.value,
            POLICY_DECISION_BLOCKED,
            false,
        )
        .await
        .expect("запись блокировки");
    let ignore_preview = db
        .preview_ignore_conversation(talk)
        .await
        .expect("предпросмотр игнорирования");
    let sweep_preview = db
        .preview_sender_sweep(sweep_input.clone())
        .await
        .expect("предпросмотр автоочистки");

    // Все три списка собраны тридцать часов назад, а срок им отведён часовой.
    db.set_limit(LIMIT_STAGE_SNAPSHOT_HOURS, 1)
        .await
        .expect("записать срок жизни снимка");
    for key in [
        &policy_preview.snapshot_key,
        &ignore_preview.snapshot_key,
        &sweep_preview.snapshot_key,
    ] {
        age_snapshot(&db, key, 30).await;
    }
    db.purge_stale_stage_snapshots()
        .await
        .expect("уборка снимков");

    let refusals = [
        db.start_sender_policy_sweep(policy.id, &policy_preview.snapshot_key, true)
            .await
            .err()
            .map(|error| error.to_string()),
        db.enable_ignore_conversation(talk, &ignore_preview.snapshot_key, true)
            .await
            .err()
            .map(|error| error.to_string()),
        db.start_sender_sweep(sweep_input.clone(), &sweep_preview.snapshot_key)
            .await
            .err()
            .map(|error| error.to_string()),
    ];
    for refusal in &refusals {
        let message = refusal
            .as_deref()
            .expect("просроченный список писем подтверждён: возраст снимка не проверяется");
        assert!(
            message.contains("устарел"),
            "отказ не объясняет, что список устарел: {message}"
        );
    }
    for message in [blocked, talk, swept] {
        assert!(
            takeaways(&db, message).await.is_empty(),
            "по просроченному списку письмо всё-таки уводится"
        );
    }
    assert_eq!(
        job_counts(&db).await,
        (0, 0, 0),
        "отказ оставил после себя задание уборки"
    );
    assert!(
        db.list_ignored_conversations()
            .await
            .expect("записи игнорирования")
            .is_empty(),
        "переписка записана в игнорируемые по просроченному списку"
    );

    // Тот же возраст снимка при сроке в двое суток доходит до уборки: отказ
    // выше вызван настройкой, а не поломкой подтверждения и не числом,
    // вписанным в запрос.
    db.set_limit(LIMIT_STAGE_SNAPSHOT_HOURS, 48)
        .await
        .expect("поднять срок жизни снимка");
    let policy_preview = db
        .preview_sender_policy(POLICY_KIND_ADDRESS, "news@example.test")
        .await
        .expect("второй предпросмотр списка отправителей");
    let ignore_preview = db
        .preview_ignore_conversation(talk)
        .await
        .expect("второй предпросмотр игнорирования");
    let sweep_preview = db
        .preview_sender_sweep(sweep_input.clone())
        .await
        .expect("второй предпросмотр автоочистки");
    for key in [
        &policy_preview.snapshot_key,
        &ignore_preview.snapshot_key,
        &sweep_preview.snapshot_key,
    ] {
        age_snapshot(&db, key, 30).await;
    }
    // Уборка идёт по тому же сроку: снимок, который ещё жив, она забрать не
    // вправе - подтверждение после неё отказа не получает.
    db.purge_stale_stage_snapshots()
        .await
        .expect("уборка снимков");

    db.start_sender_policy_sweep(policy.id, &policy_preview.snapshot_key, true)
        .await
        .expect("уборка по списку отправителей отказана живому снимку");
    db.enable_ignore_conversation(talk, &ignore_preview.snapshot_key, true)
        .await
        .expect("включение игнорирования отказано живому снимку");
    db.start_sender_sweep(sweep_input, &sweep_preview.snapshot_key)
        .await
        .expect("автоочистка по отправителю отказана живому снимку");
    for message in [blocked, talk, swept] {
        let queued = takeaways(&db, message).await;
        assert_eq!(queued.len(), 1, "живой список письмо не убрал");
        assert_eq!(queued[0].2, Some(trash));
    }
    db.close().await;
}
