//! Сценарные проверки закрепления письма в списке (specs/pin-message.md).
//!
//! Проверки идут по настоящему пути: настоящая база с применёнными миграциями,
//! настоящие запросы страниц списка папки, метки и умной папки, настоящие
//! счётчики, настоящая очередь операций и настоящая очистка кэша.

use super::Db;
use super::repo::test_storage::{TestDb, open_test_db};
use crate::backend::DiscoveredMessage;
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
    pin(&db, old).await;

    let first = db
        .list_folder_first_page(inbox, 100)
        .await
        .expect("первая страница папки");
    let ids = first.iter().map(|message| message.id).collect::<Vec<_>>();
    assert_eq!(
        ids,
        vec![fresh, middle],
        "закреплённое письмо ушло из обычной части, порядок остальных прежний"
    );

    // Внешний интерфейс приложений страницами списка не пользуется и обязан
    // видеть всю почту папки: исключение сделано ради окна программы (S-012).
    let external = db
        .list_messages(inbox, 100)
        .await
        .expect("письма папки для внешнего интерфейса");
    assert!(
        external.iter().any(|message| message.id == old),
        "закреплённое письмо пропало из внешнего интерфейса приложений"
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
    pin(&db, message).await;
    let before = pinned_at(&db, message).await.expect("время закрепления");

    db.queue_message_action(&[message], "trash")
        .await
        .expect("поставить перенос в корзину");

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

/// S-051, S-058: закрепление переживает и обычную синхронизацию. Строку письма
/// удаляют не только своя операция переноса и извещение об исчезновении: то же
/// делают сверка снимка папки и смена признака её действительности. Письмо,
/// разложенное по папкам на другом устройстве, иначе молча теряло бы
/// закрепление и уезжало вниз списка.
#[tokio::test]
async fn a_snapshot_reconcile_keeps_the_pin_of_a_message_moved_elsewhere() {
    let db: TestDb = open_test_db("pin-snapshot").await;
    let account = seed_account(&db, "snapshot@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    let archive = seed_folder(&db, account, "Archive", Some("archive")).await;
    db.save_discovered_messages(
        account,
        &[discovered("INBOX", 31, "<snapshot@example.test>")],
        false,
    )
    .await
    .expect("письмо пришло во входящие");
    let message = message_id_by_uid(&db, inbox, 31).await;
    db.set_messages_pinned(&[message], true)
        .await
        .expect("закрепить письмо");
    let before = pinned_at(&db, message).await.expect("время закрепления");

    // Снимок папки письма больше не называет: его унесли на другом устройстве.
    db.reconcile_imap_snapshot(account, &[("INBOX".to_owned(), Vec::new())], &[])
        .await
        .expect("сверка снимка папки");
    db.save_discovered_messages(
        account,
        &[discovered("Archive", 931, "<snapshot@example.test>")],
        false,
    )
    .await
    .expect("письмо нашлось в архиве");

    let moved = message_id_by_uid(&db, archive, 931).await;
    assert_eq!(
        pinned_at(&db, moved).await.as_deref(),
        Some(before.as_str()),
        "обычная синхронизация унесла закрепление вместе со строкой письма"
    );
    db.close().await;
}

/// S-007, S-008, S-059: предел в 20 закреплённых писем на ящик проверяется в
/// ядре и считается по всем закреплённым письмам, включая отложенные и
/// уводимые. Второй ящик при этом свой предел держит сам.
#[tokio::test]
async fn the_pin_limit_is_counted_in_the_core_per_mailbox() {
    let db: TestDb = open_test_db("pin-limit").await;
    let first = seed_account(&db, "limit-one@example.test").await;
    let second = seed_account(&db, "limit-two@example.test").await;
    let first_inbox = seed_folder(&db, first, "INBOX", Some("inbox")).await;
    let second_inbox = seed_folder(&db, second, "INBOX", Some("inbox")).await;
    let mut ids = Vec::new();
    for uid in 1..=21 {
        ids.push(
            seed_message(
                &db,
                first,
                first_inbox,
                uid,
                &format!("<limit-{uid}@example.test>"),
                1,
            )
            .await,
        );
    }
    // Одно письмо отложено, другое уводится: в списках их сейчас нет, но они
    // вернутся сами и место в пределе занимают.
    sqlx::query("UPDATE messages SET snoozed_until=datetime('now','+1 day') WHERE id=?")
        .bind(ids[0])
        .execute(&db.write_pool)
        .await
        .expect("отложить письмо");
    db.queue_message_action(&[ids[1]], "trash")
        .await
        .expect("поставить перенос в корзину");

    let result = db
        .set_messages_pinned(&ids[..20], true)
        .await
        .expect("закрепить двадцать писем");
    assert_eq!(result.changed, 20, "двадцать писем закрепляются");
    assert_eq!(result.rejected_limit, 0, "предел ещё не достигнут");

    let over = db
        .set_messages_pinned(&ids[20..], true)
        .await
        .expect("двадцать первое письмо");
    assert_eq!(over.changed, 0, "двадцать первое письмо не закрепляется");
    assert_eq!(
        over.rejected_limit, 1,
        "отказ назван пределом, а не ошибкой"
    );
    assert!(
        pinned_at(&db, ids[20]).await.is_none(),
        "письмо сверх предела всё-таки закреплено"
    );

    let other = seed_message(&db, second, second_inbox, 1, "<other@example.test>", 1).await;
    let neighbour = db
        .set_messages_pinned(&[other], true)
        .await
        .expect("закрепить письмо второго ящика");
    assert_eq!(
        neighbour.changed, 1,
        "предел одного ящика не мешает второму ящику"
    );
    db.close().await;
}

/// S-009, S-038 - S-041: перечень закреплённых писем читается у ядра с видом
/// открытого представления и его приметой. Видов четыре, и каждый отбирает
/// своё: папка - письма этой папки, метка - письма с этой меткой, умная папка -
/// письма, прошедшие её условия, объединённое представление - письма всех
/// ящиков. Без отдельного перечня закреплённое письмо пропадает из списка:
/// страницы списка закреплённые письма исключают.
#[tokio::test]
async fn the_pinned_list_answers_each_of_the_four_views() {
    let db: TestDb = open_test_db("pin-views").await;
    let first = seed_account(&db, "views-one@example.test").await;
    let second = seed_account(&db, "views-two@example.test").await;
    let inbox = seed_folder(&db, first, "INBOX", Some("inbox")).await;
    let archive = seed_folder(&db, first, "Archive", Some("archive")).await;
    let other_inbox = seed_folder(&db, second, "INBOX", Some("inbox")).await;

    // Все три письма закреплены и лежат далеко в прошлом: в загруженные
    // страницы они не попали бы вовсе.
    let in_inbox = seed_message(&db, first, inbox, 1, "<inbox@example.test>", 400).await;
    let in_archive = seed_message(&db, first, archive, 2, "<archive@example.test>", 500).await;
    let in_other = seed_message(&db, second, other_inbox, 3, "<other@example.test>", 600).await;
    let label = db
        .create_label("Работа", "#ff0000")
        .await
        .expect("создать метку");
    add_label(&db, in_archive, label).await;
    for message in [in_inbox, in_archive, in_other] {
        pin(&db, message).await;
    }
    // Умная папка "Все важные": признак важности стоит у писем посева.
    let smart = seed_flagged_smart_folder(&db).await;
    sqlx::query("UPDATE messages SET flagged=0 WHERE id=?")
        .bind(in_other)
        .execute(&db.write_pool)
        .await
        .expect("снять признак важности у письма второго ящика");

    let ids = |list: &PinnedMessageList| {
        let mut ids = list
            .messages
            .iter()
            .map(|message| message.id)
            .collect::<Vec<_>>();
        ids.sort_unstable();
        ids
    };

    let folder_view = db
        .list_pinned_messages("folder", Some(&inbox.to_string()))
        .await
        .expect("закреплённые письма папки");
    assert_eq!(
        ids(&folder_view),
        vec![in_inbox],
        "перечень папки отдал чужие закреплённые письма"
    );

    let label_view = db
        .list_pinned_messages("label", Some("Работа"))
        .await
        .expect("закреплённые письма метки");
    assert_eq!(
        ids(&label_view),
        vec![in_archive],
        "перечень метки собран не по метке"
    );

    let smart_view = db
        .list_pinned_messages("smart", Some(&smart))
        .await
        .expect("закреплённые письма умной папки");
    assert_eq!(
        ids(&smart_view),
        vec![in_inbox, in_archive],
        "перечень умной папки не спросил вхождение у того же отбора, что и страницы"
    );

    let unified_view = db
        .list_pinned_messages("unified", None)
        .await
        .expect("закреплённые письма объединённого представления");
    assert_eq!(
        ids(&unified_view),
        vec![in_inbox, in_archive, in_other],
        "объединённое представление потеряло закреплённые письма одного из ящиков"
    );

    // Вид неизвестен - ошибка, а не молчаливый пустой перечень.
    assert!(
        db.list_pinned_messages("inbox", None).await.is_err(),
        "неизвестный вид представления принят как свой"
    );
    db.close().await;
}

/// S-011, S-072: вторая страница списка берётся по курсору из пары даты и
/// номера письма, и закреплённые письма в неё тоже не попадают - ни в списке
/// метки, ни в умной папке. Иначе письмо показалось бы дважды при первой же
/// прокрутке вниз.
#[tokio::test]
async fn second_pages_by_cursor_leave_pinned_messages_out() {
    let db: TestDb = open_test_db("pin-second-page").await;
    let account = seed_account(&db, "cursor@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    let label = db
        .create_label("Работа", "#00ff00")
        .await
        .expect("создать метку");
    let smart = seed_flagged_smart_folder(&db).await;

    // Четыре письма по убыванию даты: первая страница отдаёт два, вторая -
    // остальные. Закреплено третье по счёту, и оно обязано выпасть из обеих.
    let mut ids = Vec::new();
    for (index, days) in [1i64, 2, 3, 4].into_iter().enumerate() {
        let message = seed_message(
            &db,
            account,
            inbox,
            index as i64 + 1,
            &format!("<cursor-{index}@example.test>"),
            days,
        )
        .await;
        add_label(&db, message, label).await;
        ids.push(message);
    }
    pin(&db, ids[2]).await;

    let first = db
        .list_label_messages_page("Работа", None, None, 2)
        .await
        .expect("первая страница метки");
    let last = first.last().expect("первая страница не пуста");
    let second = db
        .list_label_messages_page("Работа", last.date.as_deref(), Some(last.id), 2)
        .await
        .expect("вторая страница метки");
    assert!(
        !second.iter().any(|message| message.id == ids[2]),
        "вторая страница метки отдала закреплённое письмо во второй раз"
    );
    assert_eq!(
        second.iter().map(|message| message.id).collect::<Vec<_>>(),
        vec![ids[3]],
        "вторая страница метки собрана не по курсору"
    );

    let first_smart = db
        .list_smart_folder_messages_page(&smart, None, None, 2)
        .await
        .expect("первая страница умной папки");
    let last_smart = first_smart.last().expect("первая страница не пуста");
    let second_smart = db
        .list_smart_folder_messages_page(&smart, last_smart.date.as_deref(), Some(last_smart.id), 2)
        .await
        .expect("вторая страница умной папки");
    assert!(
        !second_smart.iter().any(|message| message.id == ids[2]),
        "вторая страница умной папки отдала закреплённое письмо во второй раз"
    );
    assert_eq!(
        second_smart
            .iter()
            .map(|message| message.id)
            .collect::<Vec<_>>(),
        vec![ids[3]],
        "вторая страница умной папки собрана не по курсору"
    );
    db.close().await;
}
