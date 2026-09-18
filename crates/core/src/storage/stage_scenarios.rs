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

fn days_ago(days: i64) -> String {
    (chrono::Utc::now() - chrono::Duration::days(days))
        .format("%Y-%m-%dT%H:%M:%S+00:00")
        .to_string()
}

/// Блокировка отправителя от начала до конца: запись, приход письма, порядок
/// стадий, постановка переноса в корзину, исключение письма из уведомления о
/// новой почте и защита письма, уже лежащего в корзине
/// (blocked-senders.md S-001 - S-005, S-021, S-023 - S-025).
#[tokio::test]
async fn blocked_sender_is_queued_to_trash_and_closes_the_message() {
    let db = test_db("stage-blocked").await;
    let account = seed_account(&db, "me@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    let trash = seed_folder(&db, account, "Trash", Some("trash")).await;
    // Правило, которое пометило бы письмо, если бы оно дошло до стадии правил.
    let rule = MailRuleInput {
        id: "rule-after-block".into(),
        name: "После блокировки".into(),
        account_id: None,
        enabled: true,
        groups: vec![MailRuleGroup {
            logic: "all".into(),
            conditions: vec![MailRuleCondition {
                field: "subject".into(),
                op: "contains".into(),
                value: "рассылка".into(),
                unit: None,
                value2: None,
            }],
        }],
        exceptions: Vec::new(),
        actions: vec![MailRuleAction {
            kind: "mark_read".into(),
            folder_id: None,
            folder_role: None,
            label_id: None,
        }],
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
        1,
        Seed {
            from: "spam@example.test",
            subject: "рассылка",
            ..Seed::default()
        },
    )
    .await;
    let wanted = seed_message(
        &db,
        account,
        inbox,
        2,
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
        3,
        Seed {
            from: "spam@example.test",
            subject: "старое",
            ..Seed::default()
        },
    )
    .await;

    db.process_sync_batch_stages().await.expect("стадии");

    let queued = takeaways(&db, blocked).await;
    assert_eq!(queued.len(), 1, "по письму ровно одна операция увода");
    assert_eq!(queued[0].0, "move", "письмо переносится, а не удаляется");
    assert_eq!(queued[0].2, Some(trash));
    assert_eq!(
        closed_by(&db, blocked).await.as_deref(),
        Some(SENDER_POLICY_STAGE_NAME),
        "письмо закрыто стадией списков и дальше не идёт"
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
            &[format!("remote-{inbox}-1"), format!("remote-{inbox}-2")],
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
    assert_eq!(
        takeaways(&db, after_snapshot).await.len(),
        1,
        "письмо после снимка уводит стадия списков"
    );

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
    assert_eq!(trash, trash);
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

    // Сервер выполнил перенос: письма оказались в корзине, локальная копия
    // это увидела. Возврат ищет их там по приметам (S-037).
    sqlx::query("UPDATE messages SET folder_id=? WHERE id IN (?, ?, ?)")
        .bind(trash)
        .bind(root)
        .bind(branch)
        .bind(future)
        .execute(&db.write_pool)
        .await
        .expect("письма в корзине");
    sqlx::query("UPDATE outbox_ops SET status='done' WHERE status='pending'")
        .execute(&db.write_pool)
        .await
        .expect("операции выполнены");

    let report = db
        .disable_ignore_conversation(record.id, true)
        .await
        .expect("прекращение с возвратом");
    assert_eq!(report.queued, 3, "опознанные письма возвращаются");
    assert!(
        report.skipped >= 1,
        "письмо, не найденное в корзине, честно считается пропущенным"
    );
    for message in [root, branch, future] {
        let ops = takeaways(&db, message).await;
        let back = ops.last().expect("операция возврата");
        assert_eq!(back.2, Some(inbox), "письмо возвращается в свою папку");
    }
    let after = db
        .ignored_conversation(record.id)
        .await
        .expect("запись после возврата");
    assert_eq!(after.returned, 3);
    assert_eq!(after.skipped, 1);
    db.close().await;
}

/// Собственное письмо в папке отправленных снимает игнорирование переписки
/// (S-030), а предел числа идентификаторов переводит запись в частичное
/// покрытие (S-028).
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
    db.start_sender_sweep(new_now, "").await.expect("правило");
    let rules = db.list_mail_rules().await.expect("список правил");
    let created = rules
        .iter()
        .find(|rule| rule.name.contains("ads@example.test"))
        .expect("правило режима \"новые сразу\"");
    assert_eq!(created.groups[0].conditions[0].field, "sender_address");
    assert_eq!(created.groups[0].conditions[0].op, "equals");
    assert!(created.actions.iter().any(|action| action.kind == "trash"));
    assert!(created.actions.iter().any(|action| action.kind == "stop"));
    db.close().await;
}

/// Порядок стадий и одна операция увода на письмо: письмо подходит и под
/// список отправителей, и под игнорируемую переписку, и под правило - его
/// закрывает первая стадия, а второй операции по нему не появляется
/// (blocked-senders.md S-001, S-002, S-004).
#[tokio::test]
async fn first_stage_closes_the_message_and_no_second_takeaway_appears() {
    let db = test_db("stage-order").await;
    let account = seed_account(&db, "me@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    let trash = seed_folder(&db, account, "Trash", Some("trash")).await;
    let spam = seed_folder(&db, account, "Spam", Some("spam")).await;

    let root = seed_message(
        &db,
        account,
        inbox,
        1,
        Seed {
            from: "sender@example.test",
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
    db.save_sender_policy(
        POLICY_KIND_ADDRESS,
        "sender@example.test",
        POLICY_DECISION_BLOCKED,
        false,
    )
    .await
    .expect("блокировка");
    let rule = MailRuleInput {
        id: "rule-spam".into(),
        name: "В спам".into(),
        account_id: None,
        enabled: true,
        groups: vec![MailRuleGroup {
            logic: "all".into(),
            conditions: vec![MailRuleCondition {
                field: "sender_address".into(),
                op: "equals".into(),
                value: "sender@example.test".into(),
                unit: None,
                value2: None,
            }],
        }],
        exceptions: Vec::new(),
        actions: vec![MailRuleAction {
            kind: "spam".into(),
            folder_id: None,
            folder_role: None,
            label_id: None,
        }],
        confirm_key: None,
    };
    db.save_mail_rule(&rule, false, None)
        .await
        .expect("правило");

    let next = seed_message(
        &db,
        account,
        inbox,
        2,
        Seed {
            from: "sender@example.test",
            subject: "Re: Переписка",
            message_id: Some("<next@example.test>"),
            in_reply_to: Some("<root@example.test>"),
            ..Seed::default()
        },
    )
    .await;
    db.process_sync_batch_stages().await.expect("стадии");
    let queued = takeaways(&db, next).await;
    assert_eq!(
        queued.len(),
        1,
        "одна незавершённая операция увода на письмо"
    );
    assert_eq!(
        queued[0].2,
        Some(trash),
        "письмо закрыто первой стадией, а не правилом"
    );
    assert_ne!(queued[0].2, Some(spam));
    assert_eq!(
        closed_by(&db, next).await.as_deref(),
        Some(SENDER_POLICY_STAGE_NAME)
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
    assert_eq!(
        takeaways(&db, busy).await.len(),
        1,
        "второй операции по занятому письму не появилось"
    );
    assert_eq!(takeaways(&db, free).await.len(), 1);
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
