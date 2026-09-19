//! Сценарные проверки закрепления письма в списке (specs/pin-message.md).
//!
//! Проверки идут по настоящему пути: настоящая база с применёнными миграциями,
//! настоящие запросы страниц списка папки, метки и умной папки, настоящие
//! счётчики, настоящая очередь операций и настоящая очистка кэша.

use super::Db;
use super::repo::test_storage::{TestDb, open_test_db};
use crate::backend::DiscoveredMessage;
use crate::model::*;

/// Время закрепления письма: пустое значение означает незакреплённое письмо.
const PIN_COLUMN: &str = "pinned_at";

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

/// Без столбца времени закрепления ни один сценарий этого файла не имеет
/// смысла: сказать об этом словами честнее, чем уронить проверку невнятной
/// ошибкой запроса.
async fn require_pin_column(db: &Db, what_breaks: &str) {
    assert!(
        column_exists(db, "messages", PIN_COLUMN).await,
        "у письма нет времени закрепления: {what_breaks}"
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

async fn seed_message(
    db: &Db,
    account_id: i64,
    folder_id: i64,
    uid: i64,
    message_id: &str,
    days_ago: i64,
) -> i64 {
    sqlx::query_as::<_, (i64,)>(
        "INSERT INTO messages(account_id, folder_id, uid, from_addr, from_name, subject, preview,
                              date, rfc822_message_id, remote_id, size, flagged)
         VALUES(?, ?, ?, 'boss@example.test', 'Начальник', 'Договор', '', datetime('now', ?), ?, ?, 100, 1)
         RETURNING id",
    )
    .bind(account_id)
    .bind(folder_id)
    .bind(uid)
    .bind(format!("-{days_ago} days"))
    .bind(message_id)
    .bind(format!("remote-{folder_id}-{uid}"))
    .fetch_one(&db.write_pool)
    .await
    .expect("сохранить письмо")
    .0
}

/// Закрепить письмо прямой записью: команда закрепления - предмет зелёного
/// этапа, а списки обязаны считаться с признаком независимо от того, кто его
/// поставил.
async fn pin(db: &Db, message_id: i64) {
    sqlx::query("UPDATE messages SET pinned_at = datetime('now') WHERE id = ?")
        .bind(message_id)
        .execute(&db.write_pool)
        .await
        .expect("закрепить письмо");
}

async fn pinned_at(db: &Db, message_id: i64) -> Option<String> {
    sqlx::query_as::<_, (Option<String>,)>("SELECT pinned_at FROM messages WHERE id=?")
        .bind(message_id)
        .fetch_one(&db.pool)
        .await
        .expect("прочитать время закрепления")
        .0
}

async fn add_label(db: &Db, message_id: i64, label_id: i64) {
    db.toggle_message_label(message_id, label_id, true)
        .await
        .expect("повесить метку");
}

fn discovered(folder: &str, uid: u32, message_id: &str) -> DiscoveredMessage {
    let raw = format!(
        "From: Начальник <boss@example.test>\r\nTo: me@example.test\r\nSubject: Договор\r\nMessage-ID: {message_id}\r\nDate: Mon, 14 Sep 2026 10:00:00 +0000\r\n\r\nТекст письма\r\n"
    );
    DiscoveredMessage {
        folder_path: folder.to_owned(),
        uid,
        remote_id: Some(format!("remote-{folder}-{uid}")),
        size: Some(raw.len() as u32),
        seen: false,
        flagged: false,
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

/// Умная папка "Все важные" встроенным условием важности: та же запись, что
/// заводит интерфейс при первом запуске.
async fn seed_flagged_smart_folder(db: &Db) -> String {
    let folder = SmartFolder {
        id: "all-flagged".into(),
        name: "Все важные".into(),
        icon: Some("flag".into()),
        is_builtin: true,
        enabled: true,
        sort_order: 0,
        groups: vec![SmartConditionGroup {
            logic: "all".into(),
            conditions: vec![SmartCondition {
                field: "importance".into(),
                op: "is".into(),
                value: "flagged".into(),
                unit: None,
                value2: None,
            }],
        }],
    };
    db.save_smart_folders(&[folder])
        .await
        .expect("сохранить умную папку");
    "all-flagged".to_owned()
}

/// S-001: время закрепления хранится в самой таблице писем и покрыто частичным
/// индексом. Отдельная таблица заставила бы соединяться с ней каждый запрос
/// списка, а без индекса перечень закреплённых читался бы полным просмотром
/// почты.
#[tokio::test]
async fn migration_0050_adds_the_pin_column_and_its_partial_index() {
    let db: TestDb = open_test_db("pin-schema").await;
    let applied: Vec<(i64,)> =
        sqlx::query_as("SELECT version FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&db.pool)
            .await
            .expect("прочитать применённые миграции");
    let versions = applied.iter().map(|row| row.0).collect::<Vec<_>>();
    assert!(
        versions.contains(&50),
        "миграции 0050 нет: закрепить письмо негде, и оно уезжает вниз под новой почтой"
    );
    require_pin_column(&db, "закрепление хранить негде").await;

    let indexes: Vec<(String,)> = sqlx::query_as(
        "SELECT coalesce(sql, '') FROM sqlite_master WHERE type='index' AND tbl_name='messages'",
    )
    .fetch_all(&db.pool)
    .await
    .expect("прочитать индексы писем");
    assert!(
        indexes
            .iter()
            .any(|(sql,)| sql.contains(PIN_COLUMN) && sql.to_uppercase().contains("WHERE")),
        "частичного индекса по времени закрепления нет: перечень закреплённых будет читаться полным просмотром почты"
    );
    db.close().await;
}

/// S-011, S-012, S-019, S-072, S-073: страницы списка папки закреплённых писем
/// не отдают - ни первая, ни страница по курсору, - а порядок остальных писем
/// остаётся прежним. Иначе закреплённое письмо показывалось бы дважды: и
/// вверху, и на своём месте по дате.
#[tokio::test]
async fn folder_pages_leave_pinned_messages_out() {
    let db: TestDb = open_test_db("pin-folder-pages").await;
    let account = seed_account(&db, "folder@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    let fresh = seed_message(&db, account, inbox, 1, "<fresh@example.test>", 0).await;
    let middle = seed_message(&db, account, inbox, 2, "<middle@example.test>", 5).await;
    let old = seed_message(&db, account, inbox, 3, "<old@example.test>", 40).await;
    require_pin_column(&db, "закреплённое письмо задвоится в списке").await;
    pin(&db, old).await;

    let first = db
        .list_messages(inbox, 100)
        .await
        .expect("первая страница папки");
    let ids = first.iter().map(|message| message.id).collect::<Vec<_>>();
    assert_eq!(
        ids,
        vec![fresh, middle],
        "закреплённое письмо ушло из обычной части, порядок остальных прежний"
    );

    // Страница по курсору за самым свежим письмом: закреплённого нет и тут.
    let after = db
        .list_messages_page(inbox, first[0].date.as_deref(), Some(first[0].id), 100)
        .await
        .expect("страница по курсору");
    assert!(
        after.iter().all(|message| message.id != old),
        "страница по курсору вернула закреплённое письмо вторым показом"
    );
    db.close().await;
}

/// S-011, S-013, S-039, S-044, S-045: страница списка метки закреплённое письмо
/// пропускает, а счётчик метки считает его наравне с остальными. Счётчик,
/// уменьшенный на число закреплённых, обещал бы пользователю меньше писем, чем
/// у него есть.
#[tokio::test]
async fn label_page_skips_the_pin_but_the_label_counter_does_not() {
    let db: TestDb = open_test_db("pin-label").await;
    let account = seed_account(&db, "label@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    let plain = seed_message(&db, account, inbox, 1, "<plain@example.test>", 1).await;
    let pinned = seed_message(&db, account, inbox, 2, "<pinned@example.test>", 30).await;
    let label = db
        .create_label("Работа", "#ff0000")
        .await
        .expect("создать метку");
    add_label(&db, plain, label).await;
    add_label(&db, pinned, label).await;
    require_pin_column(&db, "закреплённое письмо с меткой задвоится").await;
    pin(&db, pinned).await;

    let page = db
        .list_label_messages_page("Работа", None, None, 100)
        .await
        .expect("страница метки");
    assert_eq!(
        page.iter().map(|message| message.id).collect::<Vec<_>>(),
        vec![plain],
        "страница метки отдала закреплённое письмо во второй раз"
    );

    let counts = db
        .label_message_counts()
        .await
        .expect("счётчик писем метки");
    let work = counts
        .iter()
        .find(|(name, _)| name == "Работа")
        .map(|(_, count)| *count)
        .unwrap_or_default();
    assert_eq!(
        work, 2,
        "счётчик метки обязан считать закреплённые письма наравне с остальными"
    );
    db.close().await;
}

/// S-011, S-013, S-040, S-044: то же для умной папки. Её счётчик пользуется
/// общим условием живого письма, поэтому исключение закреплённых стоит только
/// в страницах, а не в общем макросе.
#[tokio::test]
async fn smart_folder_page_skips_the_pin_but_its_counter_does_not() {
    let db: TestDb = open_test_db("pin-smart").await;
    let account = seed_account(&db, "smart@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    let plain = seed_message(&db, account, inbox, 1, "<plain@example.test>", 1).await;
    let pinned = seed_message(&db, account, inbox, 2, "<pinned@example.test>", 30).await;
    let stable_id = seed_flagged_smart_folder(&db).await;
    require_pin_column(&db, "закреплённое письмо задвоится в умной папке").await;
    pin(&db, pinned).await;

    let page = db
        .list_smart_folder_messages_page(&stable_id, None, None, 100)
        .await
        .expect("страница умной папки");
    assert_eq!(
        page.iter().map(|message| message.id).collect::<Vec<_>>(),
        vec![plain],
        "страница умной папки отдала закреплённое письмо во второй раз"
    );

    let counts = db
        .count_smart_folder_messages(std::slice::from_ref(&stable_id))
        .await
        .expect("счётчик умной папки");
    assert_eq!(
        counts.first().map(|count| count.total).unwrap_or_default(),
        2,
        "счётчик умной папки уменьшился ровно на число закреплённых писем"
    );
    db.close().await;
}

/// S-054: очистка кэша по глубине локального хранения закреплённых писем не
/// трогает. Иначе через несколько дней письмо, которое пользователь явно
/// закрепил, открепилось бы само вместе со своей строкой.
#[tokio::test]
async fn cache_pruning_keeps_pinned_messages() {
    let db: TestDb = open_test_db("pin-prune").await;
    let account = seed_account(&db, "prune@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    let pinned = seed_message(&db, account, inbox, 1, "<pinned@example.test>", 40).await;
    let plain = seed_message(&db, account, inbox, 2, "<plain@example.test>", 40).await;
    require_pin_column(&db, "очистка кэша не отличит закреплённое письмо").await;
    pin(&db, pinned).await;

    db.prune_cached_messages(account, 30)
        .await
        .expect("очистка кэша");

    let left: Vec<(i64,)> = sqlx::query_as("SELECT id FROM messages ORDER BY id")
        .fetch_all(&db.pool)
        .await
        .expect("прочитать оставшиеся письма");
    let left = left.into_iter().map(|row| row.0).collect::<Vec<_>>();
    assert!(
        left.contains(&pinned),
        "закреплённое письмо вычистили по глубине хранения"
    );
    assert!(!left.contains(&plain), "обычное старое письмо чистится");
    db.close().await;
}

/// S-050, S-051, S-055: сквозная проверка судьбы закрепления при переносе.
/// Письмо закреплено, уведено в корзину, очередь завершила перенос и удалила
/// прежнюю строку, а следующая синхронизация принесла то же письмо на новом
/// месте. Закрепление и время закрепления обязаны приехать вместе с ним.
#[tokio::test]
async fn a_moved_pinned_message_keeps_its_pin() {
    let db: TestDb = open_test_db("pin-move").await;
    let account = seed_account(&db, "move@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    let trash = seed_folder(&db, account, "Trash", Some("trash")).await;
    db.save_discovered_messages(
        account,
        &[discovered("INBOX", 11, "<move@example.test>")],
        false,
    )
    .await
    .expect("письмо пришло во входящие");
    let message = message_id_by_uid(&db, inbox, 11).await;
    require_pin_column(&db, "переносить вместе с письмом нечего").await;
    pin(&db, message).await;
    let before = pinned_at(&db, message).await.expect("время закрепления");

    db.queue_message_action(&[message], "trash")
        .await
        .expect("поставить перенос в корзину");
    // Пока операция не завершена, закрепление сохраняется (S-050).
    assert!(
        pinned_at(&db, message).await.is_some(),
        "незавершённая операция увода закрепление не снимает"
    );

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
    db.save_discovered_messages(
        account,
        &[discovered("Trash", 904, "<move@example.test>")],
        false,
    )
    .await
    .expect("письмо нашлось в корзине");

    let moved = message_id_by_uid(&db, trash, 904).await;
    assert_eq!(
        pinned_at(&db, moved).await.as_deref(),
        Some(before.as_str()),
        "закрепление и время закрепления не вернулись на новую строку письма"
    );
    db.close().await;
}

/// S-058: пересборка папки удаляет строку письма другим путём - по сообщению
/// сервера об исчезнувших номерах. Закрепление обязано вернуться и здесь,
/// иначе обычная переиндексация ящика молча роняет письмо вниз списка.
#[tokio::test]
async fn a_folder_rebuild_returns_the_pin_to_the_new_row() {
    let db: TestDb = open_test_db("pin-rebuild").await;
    let account = seed_account(&db, "rebuild@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    db.save_discovered_messages(
        account,
        &[discovered("INBOX", 21, "<rebuild@example.test>")],
        false,
    )
    .await
    .expect("письмо пришло");
    let message = message_id_by_uid(&db, inbox, 21).await;
    require_pin_column(&db, "вернуть закрепление после пересборки папки нечему").await;
    pin(&db, message).await;

    db.apply_imap_vanished(account, &[("INBOX".to_owned(), vec![21])])
        .await
        .expect("сервер сообщил об исчезнувшем номере");
    db.save_discovered_messages(
        account,
        &[discovered("INBOX", 22, "<rebuild@example.test>")],
        false,
    )
    .await
    .expect("то же письмо пришло новым номером");

    let rebuilt = message_id_by_uid(&db, inbox, 22).await;
    assert!(
        pinned_at(&db, rebuilt).await.is_some(),
        "закрепление не вернулось на новую строку письма"
    );
    db.close().await;
}
