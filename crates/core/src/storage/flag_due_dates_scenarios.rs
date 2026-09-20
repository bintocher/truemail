//! Сценарные проверки сроков у флажка письма и списка дел
//! (specs/flag-due-dates.md).
//!
//! Проверки идут по настоящему пути: настоящая база с применёнными миграциями,
//! настоящая очередь операций, настоящие пути синхронизации признаков и
//! настоящая очистка кэша. Подмены слоёв нет: там, где в программе пишет
//! хранилище, здесь пишет то же хранилище.

use super::Db;
use super::repo::test_storage::{TestDb, open_test_db};
use crate::backend::{DiscoveredFlagUpdate, DiscoveredMessage};
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

/// Письмо в базе. Дата задаётся сдвигом от текущего времени, чтобы проверки
/// очистки кэша не зависели от календаря.
async fn seed_message(
    db: &Db,
    account_id: i64,
    folder_id: i64,
    uid: i64,
    message_id: &str,
    days_ago: i64,
    flagged: bool,
) -> i64 {
    sqlx::query_as::<_, (i64,)>(
        "INSERT INTO messages(account_id, folder_id, uid, from_addr, from_name, subject, preview,
                              date, rfc822_message_id, remote_id, size, flagged)
         VALUES(?, ?, ?, 'boss@example.test', 'Начальник', 'Договор', '', datetime('now', ?), ?, ?, 100, ?)
         RETURNING id",
    )
    .bind(account_id)
    .bind(folder_id)
    .bind(uid)
    .bind(format!("-{days_ago} days"))
    .bind(message_id)
    .bind(format!("remote-{folder_id}-{uid}"))
    .bind(flagged as i64)
    .fetch_one(&db.write_pool)
    .await
    .expect("сохранить письмо")
    .0
}

/// Строка дела: сроки и состояние по письму.
async fn seed_task(db: &Db, message_id: i64, due_at: Option<&str>, state: &str) {
    sqlx::query("INSERT INTO message_tasks(message_id, due_at, state) VALUES(?, ?, ?)")
        .bind(message_id)
        .bind(due_at)
        .bind(state)
        .execute(&db.write_pool)
        .await
        .expect("завести дело письма");
}

async fn task_state(db: &Db, message_id: i64) -> Option<(String, Option<String>, Option<String>)> {
    sqlx::query_as::<_, (String, Option<String>, Option<String>)>(
        "SELECT state, due_at, completed_at FROM message_tasks WHERE message_id=?",
    )
    .bind(message_id)
    .fetch_optional(&db.pool)
    .await
    .expect("прочитать дело")
}

async fn flagged(db: &Db, message_id: i64) -> bool {
    sqlx::query_as::<_, (i64,)>("SELECT flagged FROM messages WHERE id=?")
        .bind(message_id)
        .fetch_one(&db.pool)
        .await
        .expect("прочитать признак важности")
        .0
        != 0
}

/// Письмо в виде, в котором его приносит синхронизация: заголовки настоящие,
/// поэтому `Message-ID` попадёт в базу тем же разбором, что и в работе.
fn discovered(folder: &str, uid: u32, message_id: &str, flagged: bool) -> DiscoveredMessage {
    let raw = format!(
        "From: Начальник <boss@example.test>\r\nTo: me@example.test\r\nSubject: Договор\r\nMessage-ID: {message_id}\r\nDate: Mon, 14 Sep 2026 10:00:00 +0000\r\n\r\nТекст письма\r\n"
    );
    DiscoveredMessage {
        folder_path: folder.to_owned(),
        uid,
        remote_id: Some(format!("remote-{folder}-{uid}")),
        size: Some(raw.len() as u32),
        seen: false,
        flagged,
        answered: false,
        draft: false,
        raw: raw.into_bytes(),
        body_fetched: true,
        has_attachments: None,
    }
}

async fn message_id_by_uid(db: &Db, folder_id: i64, uid: i64) -> i64 {
    sqlx::query_as::<_, (i64,)>("SELECT id FROM messages WHERE folder_id=? AND uid=?")
        .bind(folder_id)
        .bind(uid)
        .fetch_one(&db.pool)
        .await
        .expect("найти письмо по номеру на сервере")
        .0
}

/// Прогнать все три пути, которыми значение признака приходит с сервера, и
/// убедиться, что ни один не перебил обещание пользователя.
///
/// Путей ровно три: изменение признаков IMAP, полный обход папки и запись
/// признака по причине синхронизации. Каждый ломается сам по себе, поэтому
/// сломанный называется в отказе вместе с состоянием очереди.
async fn every_sync_path_keeps_the_users_flag(
    db: &Db,
    account: i64,
    uid: u32,
    message_id: &str,
    message: i64,
    queue: &str,
) {
    db.apply_imap_flag_updates(
        account,
        &[DiscoveredFlagUpdate {
            folder_path: "INBOX".into(),
            uid,
            seen: false,
            flagged: false,
            answered: false,
            draft: false,
        }],
    )
    .await
    .expect("применить изменения признаков IMAP");
    assert!(
        flagged(db, message).await,
        "{queue}: ответ сервера снял только что поставленный пользователем флажок"
    );

    db.save_discovered_messages(
        account,
        &[discovered("INBOX", uid, message_id, false)],
        false,
    )
    .await
    .expect("полный обход папки");
    assert!(
        flagged(db, message).await,
        "{queue}: полный обход папки затёр флажок, который ещё не ушёл на сервер"
    );

    db.mark_flagged_many(&[message], false, FlagChangeReason::Sync)
        .await
        .expect("запись признака по причине синхронизации");
    assert!(
        flagged(db, message).await,
        "{queue}: запись признака по причине синхронизации не отступила перед очередью"
    );
    assert_eq!(
        task_state(db, message)
            .await
            .expect("дело письма на месте")
            .0,
        "active",
        "{queue}: дело отсоединено из-за собственной же неотправленной операции признака"
    );
}

/// S-039, S-040: пока по письму лежит незавершённая операция вида `flag`,
/// значение признака с сервера к нему не применяется ни одним из трёх путей.
/// Иначе устаревший ответ сервера снял бы только что поставленный пользователем
/// флажок и увёл бы дело в состояние `detached`.
///
/// Пути проверяются вместе, одним обещанием пользователя: порознь они
/// повторяли бы одну и ту же подготовку, а ломается в них одно и то же -
/// забытый взгляд на очередь. Операция, исчерпавшая попытки, придерживает
/// значение наравне с ожидающей: отказ отправки не делает обещание
/// пользователя отменённым, и первый же ответ сервера иначе воскресил бы
/// выполненное дело. Сколько попыток до отказа - настройка
/// LIMIT_OPERATION_ATTEMPTS, и проверка берёт число из неё.
#[tokio::test]
async fn a_pending_flag_operation_holds_off_the_server_value_on_every_path() {
    let db: TestDb = open_test_db("flag-sync-race").await;
    let account = seed_account(&db, "race@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    db.save_discovered_messages(
        account,
        &[discovered("INBOX", 11, "<race@example.test>", false)],
        false,
    )
    .await
    .expect("первый обход папки");
    let message = message_id_by_uid(&db, inbox, 11).await;

    // Пользователь поставил флажок и задал срок: в очереди лежит операция вида
    // flag, а у письма - действующее дело.
    db.mark_flagged(message, true)
        .await
        .expect("поставить флажок");
    seed_task(&db, message, Some("2026-10-08 09:00:00"), "active").await;
    let operation: (i64,) = sqlx::query_as(
        "SELECT id FROM outbox_ops WHERE message_id=? AND op_kind='flag'
          AND status IN ('pending','retry')",
    )
    .bind(message)
    .fetch_one(&db.pool)
    .await
    .expect("флажок ставится прежним путём, через очередь");

    every_sync_path_keeps_the_users_flag(
        &db,
        account,
        11,
        "<race@example.test>",
        message,
        "операция ждёт отправки",
    )
    .await;

    // Попытки исчерпаны: операция уходит в состояние отказа и повтору не
    // подлежит, но незавершённой быть не перестаёт.
    for _ in 0..db.limit(LIMIT_OPERATION_ATTEMPTS) {
        db.fail_outbox_operation(operation.0, "сервер недоступен")
            .await
            .expect("попытка отправки не удалась");
    }
    let status: (String,) = sqlx::query_as("SELECT status FROM outbox_ops WHERE id=?")
        .bind(operation.0)
        .fetch_one(&db.pool)
        .await
        .expect("прочитать состояние операции");
    assert_eq!(
        status.0, "failed",
        "операция признака не дошла до отказа за назначенное настройкой число попыток"
    );

    every_sync_path_keeps_the_users_flag(
        &db,
        account,
        11,
        "<race@example.test>",
        message,
        "операция исчерпала попытки",
    )
    .await;
    db.close().await;
}

/// S-092 - S-094: флажок, снятый на другом устройстве, отсоединяет дело и
/// сохраняет его сроки, а вернувшийся флажок поднимает то же дело обратно в
/// работу с теми же сроками.
///
/// Это один путь пользователя, а не два: он трогает флажок на телефоне туда и
/// обратно. Порознь снятие и возврат проверялись на разных делах, и сроки,
/// потерянные при отсоединении, возврату было уже неоткуда взять - проверка
/// возврата заводила их заново. Здесь сроки задаются один раз и обязаны
/// пережить оба перехода.
#[tokio::test]
async fn a_flag_taken_and_returned_by_sync_keeps_the_dates_of_one_task() {
    let db: TestDb = open_test_db("flag-detach-revive").await;
    let account = seed_account(&db, "detach@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    let message = seed_message(&db, account, inbox, 31, "<detach@example.test>", 1, true).await;
    let due = "2026-09-25 09:00:00";
    seed_task(&db, message, Some(due), "active").await;

    let server_flag = async |db: &Db, flagged: bool| {
        db.apply_imap_flag_updates(
            account,
            &[DiscoveredFlagUpdate {
                folder_path: "INBOX".into(),
                uid: 31,
                seen: false,
                flagged,
                answered: false,
                draft: false,
            }],
        )
        .await
        .expect("синхронизация принесла значение признака");
    };

    server_flag(&db, false).await;
    let detached = task_state(&db, message)
        .await
        .expect("дело исчезло вместе с флажком");
    assert_eq!(
        detached.0, "detached",
        "дело должно отсоединиться, а не исчезнуть вместе с чужим снятием флажка"
    );
    assert_eq!(
        detached.1.as_deref(),
        Some(due),
        "сроки отсоединённого дела потеряны"
    );
    assert!(
        !flagged(&db, message).await,
        "признак важности с сервера не записан"
    );

    server_flag(&db, true).await;
    let revived = task_state(&db, message).await.expect("дело исчезло");
    assert_eq!(
        revived.0, "active",
        "вернувшийся флажок не поднял дело обратно в работу"
    );
    assert_eq!(
        revived.1.as_deref(),
        Some(due),
        "прежние сроки не пережили возврат флажка: пользователь увидит дело без срока"
    );
    db.close().await;
}

/// S-038, S-096, S-097, S-099: правило обработки почты ставит флажок письму с
/// завершённым делом. Дело возвращается в работу с очищенным временем
/// выполнения, но сроков правило не задаёт. Иначе выполненное дело осталось бы
/// выполненным при поднятом флажке, и список дел спорил бы с фильтром
/// "Важные".
#[tokio::test]
async fn a_rule_flag_revives_a_completed_task_without_dates() {
    let db: TestDb = open_test_db("flag-rule-revive").await;
    let account = seed_account(&db, "rule@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    let message = seed_message(&db, account, inbox, 51, "<rule@example.test>", 1, false).await;
    sqlx::query(
        "INSERT INTO message_tasks(message_id, state, completed_at)
         VALUES(?, 'done', datetime('now','-1 days'))",
    )
    .bind(message)
    .execute(&db.write_pool)
    .await
    .expect("выполненное дело");

    let rule = MailRuleInput {
        id: "flag-rule".into(),
        name: "Пометить важным".into(),
        account_id: Some(account),
        enabled: true,
        groups: vec![MailRuleGroup {
            logic: "all".into(),
            conditions: vec![MailRuleCondition {
                field: "subject".into(),
                op: "contains".into(),
                value: "Договор".into(),
                unit: None,
                value2: None,
            }],
        }],
        exceptions: Vec::new(),
        actions: vec![MailRuleAction {
            kind: "mark_flagged".into(),
            folder_id: None,
            folder_role: None,
            label_id: None,
        }],
        confirm_key: None,
    };
    db.save_mail_rule(&rule, true, None)
        .await
        .expect("сохранить правило");
    db.process_mail_rules().await.expect("прогон правил");

    assert!(flagged(&db, message).await, "правило поставило флажок");
    let task = task_state(&db, message).await.expect("дело на месте");
    assert_eq!(task.0, "active", "выполненное дело вернулось в работу");
    assert_eq!(task.2, None, "время выполнения очищено");
    db.close().await;
}

/// S-084, S-085: очистка кэша по глубине локального хранения не трогает писем
/// с незавершённым делом, но письма выполненных дел чистит наравне с
/// остальными. Иначе через неделю работы программы дело со сроком исчезло бы
/// само, а письмо осталось бы лежать на сервере.
#[tokio::test]
async fn cache_pruning_keeps_messages_with_unfinished_tasks() {
    let db: TestDb = open_test_db("flag-prune").await;
    let account = seed_account(&db, "prune@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    let active = seed_message(&db, account, inbox, 61, "<active@example.test>", 40, true).await;
    let detached = seed_message(
        &db,
        account,
        inbox,
        62,
        "<detached@example.test>",
        40,
        false,
    )
    .await;
    let done = seed_message(&db, account, inbox, 63, "<done@example.test>", 40, false).await;
    let plain = seed_message(&db, account, inbox, 64, "<plain@example.test>", 40, false).await;
    seed_task(&db, active, Some("2026-09-30 09:00:00"), "active").await;
    seed_task(&db, detached, Some("2026-09-30 09:00:00"), "detached").await;
    seed_task(&db, done, None, "done").await;

    db.prune_cached_messages(account, 30)
        .await
        .expect("очистка кэша");

    let left: Vec<(i64,)> = sqlx::query_as("SELECT id FROM messages ORDER BY id")
        .fetch_all(&db.pool)
        .await
        .expect("прочитать оставшиеся письма");
    let left = left.into_iter().map(|row| row.0).collect::<Vec<_>>();
    assert!(
        left.contains(&active),
        "письмо с незавершённым делом вычистили вместе со сроком"
    );
    assert!(
        left.contains(&detached),
        "отсоединённое дело тоже незавершённое и чистке не подлежит"
    );
    assert!(!left.contains(&done), "выполненное дело чистке не мешает");
    assert!(!left.contains(&plain), "обычное старое письмо чистится");
    db.close().await;
}

/// S-081, S-086, S-087: сквозная проверка судьбы дела при переносе письма.
/// Пользователь ставит флажок, задаёт срок и уводит письмо в другую папку;
/// очередь завершает перенос и удаляет прежнюю строку, а следующая
/// синхронизация приносит то же письмо на новом месте. Дело обязано приехать
/// вместе с ним: иначе разобранное по папкам письмо теряет срок.
#[tokio::test]
async fn a_moved_message_carries_its_task_to_the_new_row() {
    let db: TestDb = open_test_db("flag-move").await;
    let account = seed_account(&db, "move@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    let archive = seed_folder(&db, account, "Archive", Some("archive")).await;
    db.save_discovered_messages(
        account,
        &[discovered("INBOX", 71, "<move@example.test>", false)],
        false,
    )
    .await
    .expect("письмо пришло во входящие");
    let message = message_id_by_uid(&db, inbox, 71).await;

    db.mark_flagged(message, true)
        .await
        .expect("поставить флажок");
    seed_task(&db, message, Some("2026-10-01 09:00:00"), "active").await;

    let queued = db
        .queue_message_move(&[message], archive)
        .await
        .expect("поставить перенос");
    assert_eq!(queued.operation_ids.len(), 1, "перенос поставлен в очередь");

    // Окно отмены переноса - 10 секунд; ждать их незачем, время сдвигается.
    sqlx::query(
        "UPDATE outbox_ops SET next_attempt_at=datetime('now','-1 minutes') WHERE op_kind='move'",
    )
    .execute(&db.write_pool)
    .await
    .expect("окно отмены истекло");
    let operations = db
        .claim_outbox_operations(account, 10)
        .await
        .expect("забрать операции");
    for operation in operations.iter().filter(|op| op.op_kind == "move") {
        db.complete_outbox_operation(operation)
            .await
            .expect("перенос выполнен на сервере");
    }
    assert!(
        sqlx::query_as::<_, (i64,)>("SELECT count(*) FROM messages WHERE id=?")
            .bind(message)
            .fetch_one(&db.pool)
            .await
            .expect("прежняя строка письма")
            .0
            == 0,
        "завершённый перенос удаляет прежнюю строку письма"
    );

    // Следующий обход папки приносит то же письмо новым номером на сервере.
    db.save_discovered_messages(
        account,
        &[discovered("Archive", 903, "<move@example.test>", true)],
        false,
    )
    .await
    .expect("письмо нашлось на новом месте");
    let moved = message_id_by_uid(&db, archive, 903).await;

    let task = task_state(&db, moved)
        .await
        .expect("дело вернулось на новую строку письма");
    assert_eq!(task.0, "active", "перенос в другую папку дела не закрывает");
    assert_eq!(
        task.1.as_deref(),
        Some("2026-10-01 09:00:00"),
        "срок исполнения пережил перенос"
    );
    db.close().await;
}

/// S-091: пересборка папки удаляет строку письма другим путём - по сообщению
/// сервера об исчезнувших номерах. Дело обязано вернуться и здесь, иначе
/// обычная переиндексация ящика молча стирает сроки.
#[tokio::test]
async fn a_folder_rebuild_returns_the_task_to_the_new_row() {
    let db: TestDb = open_test_db("flag-rebuild").await;
    let account = seed_account(&db, "rebuild@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    db.save_discovered_messages(
        account,
        &[discovered("INBOX", 81, "<rebuild@example.test>", true)],
        false,
    )
    .await
    .expect("письмо пришло");
    let message = message_id_by_uid(&db, inbox, 81).await;
    seed_task(&db, message, Some("2026-10-02 09:00:00"), "active").await;

    db.apply_imap_vanished(account, &[("INBOX".to_owned(), vec![81])])
        .await
        .expect("сервер сообщил об исчезнувшем номере");
    db.save_discovered_messages(
        account,
        &[discovered("INBOX", 82, "<rebuild@example.test>", true)],
        false,
    )
    .await
    .expect("то же письмо пришло новым номером");

    let rebuilt = message_id_by_uid(&db, inbox, 82).await;
    let task = task_state(&db, rebuilt)
        .await
        .expect("дело вернулось на новую строку");
    assert_eq!(task.0, "active");
    assert_eq!(task.1.as_deref(), Some("2026-10-02 09:00:00"));
    db.close().await;
}

/// S-081, S-091: обычная синхронизация тоже удаляет строку письма - сверкой
/// снимка папки и сменой признака действительности папки. Письмо, разложенное
/// по папкам на другом устройстве, и любая переиндексация ящика проходят
/// именно этими путями, и без сохранения примет сроки дела уходят молча и без
/// возврата.
#[tokio::test]
async fn a_snapshot_reconcile_keeps_the_task_of_a_message_moved_elsewhere() {
    let db: TestDb = open_test_db("flag-snapshot").await;
    let account = seed_account(&db, "snapshot@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    let archive = seed_folder(&db, account, "Archive", Some("archive")).await;
    db.save_discovered_messages(
        account,
        &[discovered("INBOX", 91, "<snapshot@example.test>", true)],
        false,
    )
    .await
    .expect("письмо пришло во входящие");
    let message = message_id_by_uid(&db, inbox, 91).await;
    seed_task(&db, message, Some("2026-10-03 09:00:00"), "active").await;

    // На другом устройстве письмо унесли из входящих: снимок папки его больше
    // не называет.
    db.reconcile_imap_snapshot(account, &[("INBOX".to_owned(), Vec::new())], &[])
        .await
        .expect("сверка снимка папки");
    db.save_discovered_messages(
        account,
        &[discovered("Archive", 905, "<snapshot@example.test>", true)],
        false,
    )
    .await
    .expect("письмо нашлось в архиве");

    let moved = message_id_by_uid(&db, archive, 905).await;
    let task = task_state(&db, moved)
        .await
        .expect("дело не вернулось после сверки снимка");
    assert_eq!(task.0, "active", "сверка снимка дела не закрывает");
    assert_eq!(
        task.1.as_deref(),
        Some("2026-10-03 09:00:00"),
        "срок исполнения потерян обычной синхронизацией"
    );
    db.close().await;
}

/// S-091: смена признака действительности папки перестраивает её целиком -
/// все строки писем удаляются и приходят заново. Дело обязано вернуться на
/// новую строку того же письма.
#[tokio::test]
async fn a_uidvalidity_reset_keeps_the_task_of_the_rebuilt_folder() {
    let db: TestDb = open_test_db("flag-uidvalidity").await;
    let account = seed_account(&db, "uidvalidity@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    db.save_discovered_messages(
        account,
        &[discovered("INBOX", 101, "<uidvalidity@example.test>", true)],
        false,
    )
    .await
    .expect("письмо пришло");
    let message = message_id_by_uid(&db, inbox, 101).await;
    seed_task(&db, message, Some("2026-10-04 09:00:00"), "active").await;

    db.reconcile_imap_snapshot(
        account,
        &[("INBOX".to_owned(), vec![101])],
        &["INBOX".to_owned()],
    )
    .await
    .expect("признак действительности папки сменился");
    db.save_discovered_messages(
        account,
        &[discovered("INBOX", 1, "<uidvalidity@example.test>", true)],
        false,
    )
    .await
    .expect("папка пришла заново новыми номерами");

    let rebuilt = message_id_by_uid(&db, inbox, 1).await;
    let task = task_state(&db, rebuilt)
        .await
        .expect("дело не вернулось после смены признака действительности папки");
    assert_eq!(task.1.as_deref(), Some("2026-10-04 09:00:00"));
    db.close().await;
}

/// S-081 - S-083: три ветки переноса примет на одном сценарии. Копия письма в
/// новой папке приходит раньше извещения об исчезновении из старой - это самый
/// частый порядок событий, и приметы обязаны переехать сразу на неё. Когда
/// копии ещё нет, приметы откладываются до её появления. Когда копий несколько,
/// выбирать не из чего, и дело не наследуется чужим письмом.
#[tokio::test]
async fn the_three_branches_of_carrying_the_traits_over() {
    let db: TestDb = open_test_db("flag-traits-branches").await;
    let account = seed_account(&db, "branches@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    let archive = seed_folder(&db, account, "Archive", Some("archive")).await;
    let work = seed_folder(&db, account, "Work", None).await;

    // Ветка первая: копия уже пришла в архив, извещение об исчезновении из
    // входящих приходит после неё.
    db.save_discovered_messages(
        account,
        &[discovered("INBOX", 111, "<early@example.test>", true)],
        false,
    )
    .await
    .expect("письмо во входящих");
    let early = message_id_by_uid(&db, inbox, 111).await;
    seed_task(&db, early, Some("2026-10-05 09:00:00"), "active").await;
    db.save_discovered_messages(
        account,
        &[discovered("Archive", 911, "<early@example.test>", true)],
        false,
    )
    .await
    .expect("копия письма пришла в архив раньше извещения");
    db.apply_imap_vanished(account, &[("INBOX".to_owned(), vec![111])])
        .await
        .expect("сервер сообщил об исчезнувшем номере");
    let carried = message_id_by_uid(&db, archive, 911).await;
    let task = task_state(&db, carried)
        .await
        .expect("дело не переехало на пришедшую раньше копию письма");
    assert_eq!(
        task.1.as_deref(),
        Some("2026-10-05 09:00:00"),
        "срок исполнения потерян при самом частом порядке событий"
    );

    // Ветка вторая: копии ещё нет, приметы ждут её появления.
    db.save_discovered_messages(
        account,
        &[discovered("INBOX", 112, "<late@example.test>", true)],
        false,
    )
    .await
    .expect("второе письмо во входящих");
    let late = message_id_by_uid(&db, inbox, 112).await;
    seed_task(&db, late, Some("2026-10-06 09:00:00"), "active").await;
    db.apply_imap_vanished(account, &[("INBOX".to_owned(), vec![112])])
        .await
        .expect("извещение об исчезновении пришло раньше копии");
    db.save_discovered_messages(
        account,
        &[discovered("Work", 912, "<late@example.test>", true)],
        false,
    )
    .await
    .expect("копия письма пришла позже");
    let restored = message_id_by_uid(&db, work, 912).await;
    assert_eq!(
        task_state(&db, restored)
            .await
            .expect("отложенные приметы не вернулись")
            .1
            .as_deref(),
        Some("2026-10-06 09:00:00")
    );

    // Ветка третья: одинаковый заголовок у двух писем, единственного совпадения
    // нет, и чужое письмо дела не наследует.
    db.save_discovered_messages(
        account,
        &[
            discovered("INBOX", 113, "<twin@example.test>", true),
            discovered("Archive", 913, "<twin@example.test>", true),
            discovered("Work", 914, "<twin@example.test>", true),
        ],
        false,
    )
    .await
    .expect("три письма с одним заголовком");
    let twin = message_id_by_uid(&db, inbox, 113).await;
    seed_task(&db, twin, Some("2026-10-07 09:00:00"), "active").await;
    db.apply_imap_vanished(account, &[("INBOX".to_owned(), vec![113])])
        .await
        .expect("исходное письмо исчезло");
    for (folder, uid) in [(archive, 913), (work, 914)] {
        let other = message_id_by_uid(&db, folder, uid).await;
        assert!(
            task_state(&db, other).await.is_none(),
            "дело унаследовало письмо с тем же заголовком, хотя совпадение не единственное"
        );
    }
    db.close().await;
}

/// S-039, S-040: операция признака после восьми попыток уходит в состояние
/// отказа и повтору не подлежит, но незавершённой быть не перестаёт. Иначе
/// первый же ответ сервера вернул бы прежнее значение признака: выполненное
/// дело воскресло бы, а действующее ушло бы в отсоединённое состояние.
#[tokio::test]
async fn a_failed_flag_operation_still_blocks_the_server_value() {
    let db: TestDb = open_test_db("flag-failed-op").await;
    let account = seed_account(&db, "failed@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    db.save_discovered_messages(
        account,
        &[discovered("INBOX", 121, "<failed@example.test>", false)],
        false,
    )
    .await
    .expect("письмо пришло");
    let message = message_id_by_uid(&db, inbox, 121).await;

    db.mark_flagged(message, true)
        .await
        .expect("поставить флажок");
    seed_task(&db, message, Some("2026-10-08 09:00:00"), "active").await;
    let operation: (i64,) =
        sqlx::query_as("SELECT id FROM outbox_ops WHERE message_id=? AND op_kind='flag'")
            .bind(message)
            .fetch_one(&db.pool)
            .await
            .expect("операция признака в очереди");
    // Восемь неудачных попыток - и операция переходит в состояние отказа.
    for _ in 0..8 {
        db.fail_outbox_operation(operation.0, "сервер недоступен")
            .await
            .expect("попытка отправки не удалась");
    }
    let status: (String,) = sqlx::query_as("SELECT status FROM outbox_ops WHERE id=?")
        .bind(operation.0)
        .fetch_one(&db.pool)
        .await
        .expect("прочитать состояние операции");
    assert_eq!(status.0, "failed", "операция признака дошла до отказа");

    // Сервер отвечает прежним состоянием: признака там ещё нет.
    db.apply_imap_flag_updates(
        account,
        &[DiscoveredFlagUpdate {
            folder_path: "INBOX".into(),
            uid: 121,
            seen: false,
            flagged: false,
            answered: false,
            draft: false,
        }],
    )
    .await
    .expect("применить изменения признаков IMAP");
    assert!(
        flagged(&db, message).await,
        "отказавшая операция признака пропустила старое значение сервера"
    );
    assert_eq!(
        task_state(&db, message)
            .await
            .expect("дело письма на месте")
            .0,
        "active",
        "дело отсоединено из-за собственной же неотправленной операции признака"
    );

    // Тот же ответ приходит и полной синхронизацией, и записью признака по
    // причине синхронизации: все три пути обязаны отступить.
    db.save_discovered_messages(
        account,
        &[discovered("INBOX", 121, "<failed@example.test>", false)],
        false,
    )
    .await
    .expect("полная синхронизация принесла прежнее значение");
    assert!(
        flagged(&db, message).await,
        "полная синхронизация сняла флажок при отказавшей операции признака"
    );
    db.mark_flagged_many(&[message], false, FlagChangeReason::Sync)
        .await
        .expect("синхронизация снимает признак");
    assert!(
        flagged(&db, message).await,
        "запись признака по причине синхронизации не отступила перед отказавшей операцией"
    );
    db.close().await;
}

/// S-017, S-054, S-061: сроки приходят из окна программы в своём виде времени,
/// а ядро сравнивает их строками с временем базы. Без приведения к одному виду
/// просроченных дел не бывает вовсе, а напоминание ждёт полуночи. Сроки здесь
/// задаются ровно так, как их шлёт окно программы.
#[tokio::test]
async fn interface_times_are_stored_in_the_format_the_core_compares() {
    use chrono::SecondsFormat;

    let db: TestDb = open_test_db("flag-time-format").await;
    let account = seed_account(&db, "format@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    let overdue = seed_message(&db, account, inbox, 131, "<overdue@example.test>", 1, false).await;
    let reminded = seed_message(&db, account, inbox, 132, "<remind@example.test>", 1, false).await;

    // Срок исполнения истёк час назад: дело просрочено и сегодня, а не с
    // завтрашнего дня.
    let past = (chrono::Utc::now() - chrono::Duration::hours(1))
        .to_rfc3339_opts(SecondsFormat::Millis, true);
    db.save_message_task(&MessageTaskInput {
        message_id: overdue,
        start_at: None,
        due_at: Some(past),
        reminder_at: None,
    })
    .await
    .expect("сохранить срок дела");

    assert_eq!(
        db.overdue_message_task_count()
            .await
            .expect("счётчик просроченных"),
        1,
        "счётчик просроченных не увидел дела, срок которого истёк час назад"
    );
    let page = db.list_message_tasks(100, None).await.expect("список дел");
    let shown = page
        .items
        .iter()
        .find(|item| item.task.message_id == overdue)
        .expect("дело в списке");
    assert!(
        shown
            .task
            .due_at
            .as_deref()
            .is_some_and(|due| !due.contains('T')),
        "срок сохранён в чужом виде времени: {:?}",
        shown.task.due_at
    );

    // Напоминание, назначенное на ближайший час, ещё не наступило.
    let later = (chrono::Utc::now() + chrono::Duration::hours(1))
        .to_rfc3339_opts(SecondsFormat::Millis, true);
    db.save_message_task(&MessageTaskInput {
        message_id: reminded,
        start_at: None,
        due_at: None,
        reminder_at: Some(later),
    })
    .await
    .expect("сохранить время напоминания");
    assert!(
        db.due_task_reminders(50)
            .await
            .expect("наступившие напоминания")
            .is_empty(),
        "напоминание показано раньше своего времени"
    );

    // Время напоминания наступает через секунду: проход цикла обязан его
    // забрать, а не отложить до следующих суток. Проверка "ещё рано" сделана
    // выше на часовом сроке, а не на этом: между записью и чтением проходит
    // неизвестное время, и под нагрузкой секунда успевала истечь, превращая
    // проверку в случайную.
    let soon = (chrono::Utc::now() + chrono::Duration::seconds(1))
        .to_rfc3339_opts(SecondsFormat::Millis, true);
    db.save_message_task(&MessageTaskInput {
        message_id: reminded,
        start_at: None,
        due_at: None,
        reminder_at: Some(soon),
    })
    .await
    .expect("сохранить наступающее время напоминания");
    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
    let reminders = db
        .due_task_reminders(50)
        .await
        .expect("наступившие напоминания");
    assert!(
        reminders.iter().any(|item| item.message_id == reminded),
        "наступившее напоминание не забрано проходом цикла: оно придёт только после полуночи"
    );
    db.close().await;
}

/// S-045 - S-047, S-050, S-052: список дел разбит на группы, выстроен полным
/// порядком и читается страницами по курсору. Курсор ведётся по той же
/// четвёрке - группа, срок, дата письма, номер письма, - иначе страницы
/// повторяют или теряют строки, а уводимое письмо показывается наравне с
/// остальными.
#[tokio::test]
async fn the_task_list_is_grouped_and_paged_by_its_cursor() {
    let db: TestDb = open_test_db("flag-task-paging").await;
    let account = seed_account(&db, "paging@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    let archive = seed_folder(&db, account, "Archive", Some("archive")).await;

    // Сроки заданы сдвигом от текущего времени, чтобы проверка не зависела ни
    // от календаря, ни от часового пояса компьютера.
    let overdue = seed_message(&db, account, inbox, 1, "<overdue@example.test>", 3, true).await;
    let this_week = seed_message(&db, account, inbox, 2, "<week@example.test>", 4, true).await;
    let later = seed_message(&db, account, inbox, 3, "<later@example.test>", 5, true).await;
    let no_due = seed_message(&db, account, inbox, 4, "<nodue@example.test>", 6, true).await;
    let done = seed_message(&db, account, inbox, 5, "<done@example.test>", 7, false).await;
    let takeaway = seed_message(&db, account, inbox, 6, "<takeaway@example.test>", 8, true).await;

    let shift = |offset: &str| -> String {
        (chrono::Utc::now() + chrono::Duration::days(offset.parse::<i64>().unwrap()))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
    };
    seed_task(&db, overdue, Some(&shift("-1")), "active").await;
    seed_task(&db, this_week, Some(&shift("3")), "active").await;
    seed_task(&db, later, Some(&shift("30")), "active").await;
    seed_task(&db, done, None, "done").await;
    seed_task(&db, takeaway, Some(&shift("2")), "active").await;
    // Письмо no_due попадает в список одним признаком важности, без строки
    // дела: письмо с флажком - это дело в состоянии active (S-014, S-015).

    // По уводимому письму стоит незавершённая операция переноса: в списке дел
    // его быть не должно, как и в списках писем.
    db.queue_message_move(&[takeaway], archive)
        .await
        .expect("поставить перенос");

    // Страницами по две записи: курсор ведёт по тому же порядку, что и список.
    let mut seen = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let page = db
            .list_message_tasks(2, cursor.as_deref())
            .await
            .expect("страница списка дел");
        assert!(
            page.items.len() <= 2,
            "страница больше запрошенного предела"
        );
        seen.extend(page.items.iter().map(|item| item.task.message_id));
        match page.next_cursor {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }

    assert_eq!(
        seen,
        vec![overdue, this_week, later, no_due, done],
        "порядок списка дел по группам нарушен или страницы потеряли строки"
    );
    let unique = seen.iter().collect::<std::collections::HashSet<_>>();
    assert_eq!(unique.len(), seen.len(), "страницы повторили одну строку");
    assert!(
        !seen.contains(&takeaway),
        "уводимое письмо показано в списке дел"
    );
    db.close().await;
}

/// S-024, S-034 - S-038, S-092 - S-094: групповая запись признака важности
/// ведёт каждое письмо пачки по своей паре таблицы переходов. Снятие флажка
/// пользователем удаляет дело вместе со сроками, возврат флажка поднимает
/// выполненное и отсоединённое дело обратно в работу, а письмо без строки дела
/// остаётся делом в состоянии active.
#[tokio::test]
async fn a_group_flag_change_follows_the_transition_table_for_every_message() {
    let db: TestDb = open_test_db("flag-group-transitions").await;
    let account = seed_account(&db, "group@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    let active = seed_message(&db, account, inbox, 1, "<active@example.test>", 1, true).await;
    let detached = seed_message(&db, account, inbox, 2, "<detached@example.test>", 1, true).await;
    let plain = seed_message(&db, account, inbox, 3, "<plain@example.test>", 1, true).await;
    let done = seed_message(&db, account, inbox, 4, "<done@example.test>", 1, false).await;
    seed_task(&db, active, Some("2026-10-10 09:00:00"), "active").await;
    seed_task(&db, detached, Some("2026-10-11 09:00:00"), "detached").await;
    seed_task(&db, done, Some("2026-10-12 09:00:00"), "done").await;

    // Пользователь снимает флажок сразу с трёх выделенных писем.
    let changed = db
        .mark_flagged_many(&[active, detached, plain], false, FlagChangeReason::User)
        .await
        .expect("снять флажок по выделению");
    assert_eq!(changed, 3, "групповое снятие обработало не все письма");
    for message in [active, detached, plain] {
        assert!(!flagged(&db, message).await, "флажок остался стоять");
        assert!(
            task_state(&db, message).await.is_none(),
            "снятие флажка пользователем обязано удалить дело вместе со сроками"
        );
    }

    // Возврат флажка той же пачкой: выполненное дело возвращается в работу с
    // очищенным временем выполнения, а письмо без строки дела её не заводит.
    let back = db
        .mark_flagged_many(&[done, plain], true, FlagChangeReason::User)
        .await
        .expect("вернуть флажок по выделению");
    assert_eq!(back, 2, "групповая установка обработала не все письма");
    let revived = task_state(&db, done)
        .await
        .expect("дело выполненного письма");
    assert_eq!(
        revived.0, "active",
        "выполненное дело не вернулось в работу"
    );
    assert_eq!(
        revived.1.as_deref(),
        Some("2026-10-12 09:00:00"),
        "прежние сроки дела потеряны при возврате в работу"
    );
    assert_eq!(revived.2, None, "время выполнения не очищено");
    assert!(
        task_state(&db, plain).await.is_none(),
        "письму с одним лишь флажком заведена лишняя строка дела"
    );
    assert!(
        flagged(&db, done).await && flagged(&db, plain).await,
        "признак важности не записан"
    );
    db.close().await;
}

/// S-039: значение важности с сервера придерживает только та операция очереди,
/// которая важность и меняет. Операция вида `flag` несёт оба признака сразу и
/// ставится в том числе отметкой о прочтении, поэтому неотправленная отметка о
/// прочтении иначе запрещала бы принять важность, выставленную на другом
/// устройстве.
#[tokio::test]
async fn an_unsent_seen_mark_does_not_block_the_server_flag() {
    let db: TestDb = open_test_db("flag-seen-op").await;
    let account = seed_account(&db, "seen@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    db.save_discovered_messages(
        account,
        &[discovered("INBOX", 141, "<seen@example.test>", false)],
        false,
    )
    .await
    .expect("письмо пришло");
    let message = message_id_by_uid(&db, inbox, 141).await;

    // Пользователь прочитал письмо: в очереди лежит операция признаков, но
    // важности она не касается.
    db.mark_seen(message, true)
        .await
        .expect("отметить прочитанным");
    let queued: (i64,) = sqlx::query_as(
        "SELECT count(*) FROM outbox_ops WHERE message_id=? AND op_kind='flag' AND status='pending'",
    )
    .bind(message)
    .fetch_one(&db.pool)
    .await
    .expect("прочитать очередь");
    assert_eq!(queued.0, 1, "отметка о прочтении идёт прежним путём");

    // На другом устройстве письмо пометили важным: значение обязано приехать.
    db.apply_imap_flag_updates(
        account,
        &[DiscoveredFlagUpdate {
            folder_path: "INBOX".into(),
            uid: 141,
            seen: true,
            flagged: true,
            answered: false,
            draft: false,
        }],
    )
    .await
    .expect("применить изменения признаков IMAP");
    assert!(
        flagged(&db, message).await,
        "неотправленная отметка о прочтении запретила принять важность с сервера"
    );

    // А вот своя неотправленная важность значение сервера придерживает.
    db.mark_flagged(message, false).await.expect("снять флажок");
    db.apply_imap_flag_updates(
        account,
        &[DiscoveredFlagUpdate {
            folder_path: "INBOX".into(),
            uid: 141,
            seen: true,
            flagged: true,
            answered: false,
            draft: false,
        }],
    )
    .await
    .expect("применить изменения признаков IMAP");
    assert!(
        !flagged(&db, message).await,
        "ответ сервера отменил только что снятый пользователем флажок"
    );

    // Отметка о прочтении, поставленная после снятия флажка, пересобирает ту
    // же операцию и обещание о важности не теряет.
    db.mark_seen(message, false)
        .await
        .expect("отметить непрочитанным");
    db.apply_imap_flag_updates(
        account,
        &[DiscoveredFlagUpdate {
            folder_path: "INBOX".into(),
            uid: 141,
            seen: true,
            flagged: true,
            answered: false,
            draft: false,
        }],
    )
    .await
    .expect("применить изменения признаков IMAP");
    assert!(
        !flagged(&db, message).await,
        "пересобранная операция потеряла обещание об изменении важности"
    );
    db.close().await;
}
