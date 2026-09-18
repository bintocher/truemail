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

/// Таблица дел: номер письма первичным ключом, сроки, состояние и отметки
/// времени. Имя взято из миграции `0049_flag_due_dates.sql`.
const TASKS: &str = "message_tasks";

async fn table_exists(db: &Db, table: &str) -> bool {
    sqlx::query_as::<_, (i64,)>("SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?")
        .bind(table)
        .fetch_one(&db.pool)
        .await
        .expect("прочитать схему")
        .0
        > 0
}

async fn column_exists(db: &Db, table: &str, column: &str) -> bool {
    sqlx::query_as::<_, (i64,)>("SELECT count(*) FROM pragma_table_info(?) WHERE name=?")
        .bind(table)
        .bind(column)
        .fetch_one(&db.pool)
        .await
        .expect("прочитать столбцы")
        .0
        > 0
}

/// Без таблицы дел ни один сценарий этого файла не имеет смысла: сказать об
/// этом словами честнее, чем уронить проверку невнятной ошибкой запроса.
async fn require_tasks(db: &Db, what_breaks: &str) {
    assert!(
        table_exists(db, TASKS).await,
        "таблицы дел {TASKS} нет: {what_breaks}"
    );
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

/// S-010, S-017: сроки дела хранятся в локальной базе отдельной таблицей,
/// связанной с письмом. Если её нет или связь не каскадная, пользователь либо
/// вовсе не может задать письму срок, либо получает дела писем, которых в
/// программе уже нет.
#[tokio::test]
async fn migration_0049_adds_task_storage() {
    let db: TestDb = open_test_db("flag-tasks-schema").await;
    let applied: Vec<(i64,)> =
        sqlx::query_as("SELECT version FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&db.pool)
            .await
            .expect("прочитать применённые миграции");
    let versions = applied.iter().map(|row| row.0).collect::<Vec<_>>();
    assert!(
        versions.contains(&48),
        "миграция 0048 должна остаться на месте: {versions:?}"
    );
    assert!(
        versions.contains(&49),
        "миграции 0049 нет: сроки дела хранить негде, и раздел \"Дела\" будет пуст"
    );
    require_tasks(&db, "задать письму срок нечем").await;
    for column in [
        "message_id",
        "start_at",
        "due_at",
        "reminder_at",
        "state",
        "completed_at",
        "reminder_shown_at",
    ] {
        assert!(
            column_exists(&db, TASKS, column).await,
            "в таблице дел нет столбца {column}: часть сроков сохранить будет негде"
        );
    }

    let account = seed_account(&db, "schema@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    let message = seed_message(&db, account, inbox, 1, "<schema@example.test>", 1, true).await;
    seed_task(&db, message, Some("2026-09-20 09:00:00"), "active").await;

    // Состояние ограничено проверкой схемы: чужое значение состояния означало
    // бы дело, которое ни один список показать не умеет.
    let bogus = sqlx::query("UPDATE message_tasks SET state='postponed' WHERE message_id=?")
        .bind(message)
        .execute(&db.write_pool)
        .await;
    assert!(
        bogus.is_err(),
        "схема обязана допускать только active, done и detached"
    );

    // Письмо удалено - дело уходит вместе с ним (S-088, S-090).
    sqlx::query("DELETE FROM messages WHERE id=?")
        .bind(message)
        .execute(&db.write_pool)
        .await
        .expect("удалить письмо");
    assert!(
        task_state(&db, message).await.is_none(),
        "дело удалённого письма осталось в базе: список дел показал бы письмо, которого нет"
    );
    db.close().await;
}

/// S-039, S-040: пока по письму лежит незавершённая операция вида `flag`,
/// значение признака с сервера к нему не применяется. Иначе устаревший ответ
/// сервера снял бы только что поставленный пользователем флажок и увёл бы дело
/// в состояние `detached`.
#[tokio::test]
async fn imap_flag_delta_yields_to_a_pending_flag_operation() {
    let db: TestDb = open_test_db("flag-sync-race").await;
    let account = seed_account(&db, "race@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    let message = seed_message(&db, account, inbox, 11, "<race@example.test>", 1, false).await;

    // Пользователь поставил флажок: в очереди лежит операция вида flag.
    db.mark_flagged(message, true)
        .await
        .expect("поставить флажок");
    let pending: (i64,) = sqlx::query_as(
        "SELECT count(*) FROM outbox_ops WHERE message_id=? AND op_kind='flag' AND status IN ('pending','retry')",
    )
    .bind(message)
    .fetch_one(&db.pool)
    .await
    .expect("прочитать очередь");
    assert_eq!(pending.0, 1, "флажок ставится прежним путём, через очередь");

    // Сервер отвечает прежним состоянием: письмо ещё не помечено там.
    db.apply_imap_flag_updates(
        account,
        &[DiscoveredFlagUpdate {
            folder_path: "INBOX".into(),
            uid: 11,
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
        "ответ сервера снял только что поставленный пользователем флажок"
    );
    require_tasks(&db, "проверить состояние дела нечем").await;
    assert!(
        !matches!(task_state(&db, message).await, Some((state, _, _)) if state == "detached"),
        "дело отсоединено из-за собственной же неотправленной операции признака"
    );
    db.close().await;
}

/// S-039: тот же запрет на втором пути записи признака - в полной
/// синхронизации писем. Путей два, и пропущенный означает, что флажок
/// пользователя исчезает при первом же обходе папки.
#[tokio::test]
async fn full_sync_yields_to_a_pending_flag_operation() {
    let db: TestDb = open_test_db("flag-sync-full").await;
    let account = seed_account(&db, "full@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    db.save_discovered_messages(
        account,
        &[discovered("INBOX", 21, "<full@example.test>", false)],
        false,
    )
    .await
    .expect("первый обход папки");
    let message = message_id_by_uid(&db, inbox, 21).await;

    db.mark_flagged(message, true)
        .await
        .expect("поставить флажок");
    db.save_discovered_messages(
        account,
        &[discovered("INBOX", 21, "<full@example.test>", false)],
        false,
    )
    .await
    .expect("повторный обход папки");

    assert!(
        flagged(&db, message).await,
        "полная синхронизация затёрла флажок, который ещё не ушёл на сервер"
    );
    db.close().await;
}

/// S-092, S-093: флажок, снятый на другом устройстве, переводит дело в
/// состояние `detached` и сохраняет его сроки. Без этого сроки, введённые
/// руками, пропали бы без ведома пользователя.
#[tokio::test]
async fn sync_detaches_the_task_and_keeps_its_dates() {
    let db: TestDb = open_test_db("flag-detach").await;
    let account = seed_account(&db, "detach@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    let message = seed_message(&db, account, inbox, 31, "<detach@example.test>", 1, true).await;
    require_tasks(&db, "снятый на телефоне флажок некуда отложить").await;
    seed_task(&db, message, Some("2026-09-25 09:00:00"), "active").await;

    db.apply_imap_flag_updates(
        account,
        &[DiscoveredFlagUpdate {
            folder_path: "INBOX".into(),
            uid: 31,
            seen: false,
            flagged: false,
            answered: false,
            draft: false,
        }],
    )
    .await
    .expect("синхронизация сняла флажок");

    let task = task_state(&db, message)
        .await
        .expect("дело осталось в базе");
    assert_eq!(
        task.0, "detached",
        "дело должно отсоединиться, а не исчезнуть вместе с флажком"
    );
    assert_eq!(
        task.1.as_deref(),
        Some("2026-09-25 09:00:00"),
        "сроки отсоединённого дела сохраняются"
    );
    db.close().await;
}

/// S-094: признак важности, появившийся снова, возвращает отсоединённое дело в
/// работу с прежними сроками. Иначе пользователь, вернувший флажок на телефоне,
/// увидел бы дело без срока.
#[tokio::test]
async fn returning_flag_revives_a_detached_task() {
    let db: TestDb = open_test_db("flag-revive").await;
    let account = seed_account(&db, "revive@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    let message = seed_message(&db, account, inbox, 41, "<revive@example.test>", 1, false).await;
    require_tasks(&db, "вернуть отсоединённое дело в работу нечему").await;
    seed_task(&db, message, Some("2026-09-26 09:00:00"), "detached").await;

    db.apply_imap_flag_updates(
        account,
        &[DiscoveredFlagUpdate {
            folder_path: "INBOX".into(),
            uid: 41,
            seen: false,
            flagged: true,
            answered: false,
            draft: false,
        }],
    )
    .await
    .expect("синхронизация вернула флажок");

    let task = task_state(&db, message).await.expect("дело на месте");
    assert_eq!(task.0, "active", "дело вернулось в работу");
    assert_eq!(
        task.1.as_deref(),
        Some("2026-09-26 09:00:00"),
        "прежние сроки остались при деле"
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
    require_tasks(&db, "правило не сможет вернуть выполненное дело в работу").await;
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
    assert_eq!(task.1, None, "правило сроков не задаёт");
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
    require_tasks(&db, "очистка кэша не отличит дело от обычного письма").await;
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
    require_tasks(&db, "переносить вместе с письмом нечего").await;
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
    require_tasks(&db, "вернуть дело после пересборки папки нечему").await;
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
