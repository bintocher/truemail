//! Сценарные проверки хранилища по настоящему пути: настоящая база с
//! применёнными миграциями, настоящие запросы репозитория, настоящее
//! хранилище больших объектов, настоящий поиск и настоящий локальный сервер
//! программного доступа.
//!
//! Предметы разведены по отдельным проверкам намеренно. Пока они лежали одной
//! проверкой на пятьсот строк, падение в середине не называло предмет, а до
//! хвоста проверка не доходила вовсе: первая же поломка скрывала все
//! остальные.

use super::Db;
use super::repo::test_storage::{TestDb, open_test_db};
use crate::backend::{DiscoveredFolder, DiscoveredMessage};
use crate::model::*;
use std::sync::Arc;

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

async fn seed_message(db: &Db, account_id: i64, folder_id: i64, uid: i64, subject: &str) -> i64 {
    sqlx::query_as::<_, (i64,)>(
        "INSERT INTO messages(account_id, folder_id, uid, from_addr, subject, preview, date, size)
         VALUES(?, ?, ?, 'boss@example.test', ?, 'предпросмотр', datetime('now'), 100)
         RETURNING id",
    )
    .bind(account_id)
    .bind(folder_id)
    .bind(uid)
    .bind(subject)
    .fetch_one(&db.write_pool)
    .await
    .expect("сохранить письмо")
    .0
}

/// Письмо с вложением-приглашением: разбор MIME, вложение и тело для поиска
/// берутся проверками из него.
const CALENDAR_LETTER: &[u8] = b"From: calendar@example.test\r\nTo: repo@example.test\r\nSubject: Meeting\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=calendar\r\n\r\n--calendar\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nInvitation\r\n--calendar\r\nContent-Type: text/calendar; charset=utf-8; name=invite.ics\r\nContent-Disposition: attachment; filename=invite.ics\r\n\r\nBEGIN:VCALENDAR\r\nVERSION:2.0\r\nEND:VCALENDAR\r\n--calendar--\r\n";

/// Смена сочетания клавиш заменяет прежнее, а не заводит рядом второе: два
/// сочетания на одно действие означали бы, что список клавиш показывает одно,
/// а работает другое.
#[tokio::test]
async fn changing_a_keybinding_replaces_the_previous_combo() {
    let db: TestDb = open_test_db("repo-keys").await;
    let seeded = db.list_keybindings().await.expect("перечень сочетаний");
    assert!(
        seeded.iter().any(|binding| binding.action == "compose"),
        "написание письма засеяно сочетанием по умолчанию"
    );

    db.set_keybinding("compose", "N")
        .await
        .expect("сменить сочетание");

    let after = db
        .list_keybindings()
        .await
        .expect("перечень сочетаний после смены");
    assert_eq!(
        after.len(),
        seeded.len(),
        "смена сочетания не заводит второй строки на то же действие"
    );
    let combos: Vec<String> = after
        .iter()
        .filter(|binding| binding.action == "compose")
        .map(|binding| binding.combo.clone())
        .collect();
    assert_eq!(combos, vec!["N".to_owned()]);
    db.close().await;
}

/// Разрешение показывать картинки помнится по отправителю без оглядки на
/// регистр: адрес приходит то "News@Example.Test", то "news@example.test", и
/// пользователь не должен разрешать одно и то же дважды.
#[tokio::test]
async fn image_trust_is_remembered_regardless_of_address_case() {
    let db: TestDb = open_test_db("repo-image-trust").await;
    assert!(
        !db.image_sender_trusted("news@example.test")
            .await
            .expect("прочитать отсутствующее разрешение"),
        "без решения пользователя картинки не показываются"
    );

    db.set_image_sender_trusted("News@Example.Test", true)
        .await
        .expect("разрешить картинки");

    assert!(
        db.image_sender_trusted("news@example.test")
            .await
            .expect("прочитать разрешение другим регистром"),
        "разрешение найдено по тому же адресу в другом регистре"
    );
    assert!(
        !db.image_sender_trusted("other@example.test")
            .await
            .expect("прочитать чужое разрешение"),
        "разрешение дано одному отправителю, а не всем"
    );
    db.close().await;
}

/// Курсор синхронизации папки записывается только отдельным подтверждением
/// после разбора писем. Сохранись он вместе с описанием папки - прерванный на
/// середине проход считался бы выполненным, и пропущенные письма никогда бы не
/// пришли.
#[tokio::test]
async fn a_folder_sync_cursor_is_committed_only_after_the_pass() {
    let db: TestDb = open_test_db("repo-sync-cursor").await;
    let account = seed_account(&db, "repo@example.test").await;
    let folder = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    let discovered = DiscoveredFolder {
        remote_path: "INBOX".into(),
        display_name: "Inbox".into(),
        role: Some(FolderRole::Inbox),
        parent_remote_path: None,
        unread_count: 0,
        total_count: 2,
        uidvalidity: None,
        uidnext: None,
        highestmodseq: None,
        sync_token: Some("history-123".into()),
    };

    db.save_discovered_folders(account, std::slice::from_ref(&discovered))
        .await
        .expect("сохранить описание папки");
    let (pending,): (Option<String>,) = sqlx::query_as("SELECT sync_token FROM folders WHERE id=?")
        .bind(folder)
        .fetch_one(&db.pool)
        .await
        .expect("прочитать курсор до подтверждения");
    assert_eq!(
        pending, None,
        "описание папки курсор не двигает: письма ещё не разобраны"
    );

    db.save_folder_sync_tokens(account, std::slice::from_ref(&discovered))
        .await
        .expect("подтвердить курсор");
    let (committed,): (Option<String>,) =
        sqlx::query_as("SELECT sync_token FROM folders WHERE id=?")
            .bind(folder)
            .fetch_one(&db.pool)
            .await
            .expect("прочитать подтверждённый курсор");
    assert_eq!(committed.as_deref(), Some("history-123"));
    db.close().await;
}

/// Одно письмо Gmail видно из нескольких папок сразу. Сверка с сервером
/// оставляет ровно те копии, которые сервер и показывает: лишняя копия
/// означала бы письмо в папке, где его уже нет.
#[tokio::test]
async fn remote_projections_are_reduced_to_what_the_server_reports() {
    let db: TestDb = open_test_db("repo-projections").await;
    let account = seed_account(&db, "repo@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    let all_mail = seed_folder(&db, account, "ALL", None).await;
    for (folder, uid, remote_id) in [
        (inbox, 2_i64, "remote-1"),
        (all_mail, 2_i64, "remote-1"),
        (all_mail, 3_i64, "remote-deleted"),
    ] {
        sqlx::query("INSERT INTO messages(account_id, folder_id, uid, remote_id) VALUES(?, ?, ?, ?)")
            .bind(account)
            .bind(folder)
            .bind(uid)
            .bind(remote_id)
            .execute(&db.write_pool)
            .await
            .expect("копия письма в папке");
    }

    let removed = db
        .reconcile_remote_projections(
            account,
            &[DiscoveredMessage {
                folder_path: "INBOX".into(),
                uid: 2,
                remote_id: Some("remote-1".into()),
                size: None,
                seen: false,
                flagged: false,
                answered: false,
                draft: false,
                raw: Vec::new(),
                body_fetched: true,
                has_attachments: None,
            }],
            &["remote-1".into(), "remote-deleted".into()],
            None,
        )
        .await
        .expect("сверить копии с сервером");

    assert_eq!(removed, 2, "убраны обе лишние копии");
    let remaining: Vec<(String, String)> = sqlx::query_as(
        "SELECT m.remote_id, f.remote_path FROM messages m
         JOIN folders f ON f.id=m.folder_id WHERE m.remote_id IS NOT NULL",
    )
    .fetch_all(&db.pool)
    .await
    .expect("прочитать оставшиеся копии");
    assert_eq!(remaining, vec![("remote-1".into(), "INBOX".into())]);
    db.close().await;
}

/// Отложенное письмо уходит из списка папки до назначенного срока и
/// возвращается, как только откладывание снято. Оставшееся в списке письмо
/// сделало бы откладывание бессмысленным, а не вернувшееся - потерянным.
#[tokio::test]
async fn a_snoozed_message_leaves_the_list_and_comes_back() {
    let db: TestDb = open_test_db("repo-snooze").await;
    let account = seed_account(&db, "repo@example.test").await;
    let folder = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    let message = seed_message(&db, account, folder, 1, "Договор").await;
    let page = db.limit(LIMIT_MESSAGE_PAGE);
    let tomorrow = (chrono::Utc::now() + chrono::Duration::days(1))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();

    assert_eq!(
        db.set_messages_snoozed(&[message], Some(&tomorrow))
            .await
            .expect("отложить письмо"),
        1
    );
    assert!(
        !db.list_messages(folder, page)
            .await
            .expect("список без отложенного письма")
            .iter()
            .any(|item| item.id == message),
        "отложенное письмо в списке папки не показывается"
    );

    db.set_messages_snoozed(&[message], None)
        .await
        .expect("вернуть письмо");
    assert!(
        db.list_messages(folder, page)
            .await
            .expect("список с вернувшимся письмом")
            .iter()
            .any(|item| item.id == message),
        "снятое откладывание возвращает письмо в список"
    );
    db.close().await;
}

/// Подпись и шаблоны письма хранятся отдельно по ящику и переживают запись,
/// чтение и удаление. Пропавший шаблон пользователь набирает заново.
#[tokio::test]
async fn signatures_and_templates_are_kept_per_account() {
    let db: TestDb = open_test_db("repo-templates").await;
    let account = seed_account(&db, "repo@example.test").await;

    db.upsert_signature(account, "new", "<b>С уважением</b>", true)
        .await
        .expect("сохранить подпись");
    let signatures = db.list_signatures(account).await.expect("перечень подписей");
    assert_eq!(signatures.len(), 1);
    assert_eq!(signatures[0].body_html, "<b>С уважением</b>");

    let template = db
        .save_message_template(None, account, "Отчёт", "Итоги недели", "<p>Готово</p>")
        .await
        .expect("сохранить шаблон");
    let templates = db
        .list_message_templates(account)
        .await
        .expect("перечень шаблонов");
    assert_eq!(templates.len(), 1);
    assert_eq!(templates[0].id, template);
    assert_eq!(templates[0].subject, "Итоги недели");

    let other = seed_account(&db, "other@example.test").await;
    assert!(
        db.list_message_templates(other)
            .await
            .expect("перечень шаблонов чужого ящика")
            .is_empty(),
        "шаблоны принадлежат своему ящику"
    );
    assert!(
        !db.delete_message_template(template, other)
            .await
            .expect("удаление шаблона из чужого ящика"),
        "чужой ящик шаблон не удаляет"
    );
    assert!(
        db.delete_message_template(template, account)
            .await
            .expect("удалить шаблон")
    );
    assert!(
        db.list_message_templates(account)
            .await
            .expect("перечень после удаления")
            .is_empty()
    );
    db.close().await;
}

/// Полученное письмо разбирается один раз: вложение доступно по имени, разбор
/// MIME сохраняется в кэше и дальше берётся оттуда, а тело попадает в поиск.
/// Иначе каждое открытие письма заново разбирало бы весь MIME.
#[tokio::test]
async fn a_saved_letter_keeps_its_attachment_body_and_search_entry() {
    let db: TestDb = open_test_db("repo-letter").await;
    let account = seed_account(&db, "repo@example.test").await;
    let folder = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    db.save_discovered_messages(
        account,
        &[DiscoveredMessage {
            folder_path: "INBOX".into(),
            uid: 5,
            remote_id: Some("calendar-message".into()),
            size: Some(CALENDAR_LETTER.len() as u32),
            seen: false,
            flagged: false,
            answered: false,
            draft: false,
            raw: CALENDAR_LETTER.to_vec(),
            body_fetched: true,
            has_attachments: None,
        }],
        false,
    )
    .await
    .expect("сохранить письмо с приглашением");
    let (message,): (i64,) = sqlx::query_as("SELECT id FROM messages WHERE folder_id=? AND uid=5")
        .bind(folder)
        .fetch_one(&db.pool)
        .await
        .expect("найти сохранённое письмо");

    let parsed = db.get_message(message).await.expect("разобрать письмо");
    assert!(
        parsed.attachments[0].size.unwrap_or_default() > 0,
        "размер вложения посчитан при разборе"
    );
    assert_eq!(
        db.attachment_bytes(message, 0)
            .await
            .expect("прочитать вложение")
            .0,
        "invite.ics"
    );

    let (cached,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM message_content_cache WHERE message_id=?")
            .bind(message)
            .fetch_one(&db.pool)
            .await
            .expect("прочитать кэш разбора");
    assert_eq!(cached, 1, "разбор MIME сохранён в кэше");
    // Подменённое в кэше тело видно при следующем открытии - значит письмо
    // читается из кэша, а не разбирается заново.
    sqlx::query("UPDATE message_content_cache SET body_text='из кэша' WHERE message_id=?")
        .bind(message)
        .execute(&db.write_pool)
        .await
        .expect("подменить кэш разбора");
    assert_eq!(
        db.get_message(message)
            .await
            .expect("прочитать письмо повторно")
            .body_text
            .as_deref(),
        Some("из кэша")
    );

    use crate::search::{Fts5Index, SearchIndex};
    assert!(
        Fts5Index::new(db.clone())
            .search("Invitation", db.limit(LIMIT_MESSAGE_PAGE))
            .await
            .expect("искать по телу письма")
            .contains(&message),
        "тело разобранного письма попало в поиск"
    );
    db.close().await;
}

/// Лёгкая проекция письма (сервер отдал только заголовки) не затирает уже
/// загруженное тело, вложения и отметку полноты. Затерев их, программа
/// показала бы вместо письма обрывок и потеряла бы вложения. Письмо, которое
/// пришло только проекцией, наоборот помечено неполным и догружается позже.
#[tokio::test]
async fn a_metadata_projection_does_not_erase_a_downloaded_body() {
    let db: TestDb = open_test_db("repo-projection-body").await;
    let account = seed_account(&db, "repo@example.test").await;
    let folder = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    db.save_discovered_messages(
        account,
        &[DiscoveredMessage {
            folder_path: "INBOX".into(),
            uid: 5,
            remote_id: Some("calendar-message".into()),
            size: Some(CALENDAR_LETTER.len() as u32),
            seen: false,
            flagged: false,
            answered: false,
            draft: false,
            raw: CALENDAR_LETTER.to_vec(),
            body_fetched: true,
            has_attachments: None,
        }],
        false,
    )
    .await
    .expect("сохранить письмо целиком");
    let (message,): (i64,) = sqlx::query_as("SELECT id FROM messages WHERE folder_id=? AND uid=5")
        .bind(folder)
        .fetch_one(&db.pool)
        .await
        .expect("найти письмо");

    db.save_discovered_messages(
        account,
        &[DiscoveredMessage {
            folder_path: "INBOX".into(),
            uid: 5,
            remote_id: Some("calendar-message".into()),
            size: Some(CALENDAR_LETTER.len() as u32),
            seen: true,
            flagged: false,
            answered: false,
            draft: false,
            raw: "Subject: Meeting\r\n\r\nобрывок".as_bytes().to_vec(),
            body_fetched: false,
            has_attachments: None,
        }],
        false,
    )
    .await
    .expect("получить лёгкую проекцию того же письма");

    assert!(
        db.message_fetch_locator(message)
            .await
            .expect("прочитать признак полноты")
            .expect("письмо на месте")
            .4,
        "письмо осталось полным"
    );
    assert_eq!(
        db.message_raw_bytes(message)
            .await
            .expect("прочитать оригинал"),
        CALENDAR_LETTER,
        "оригинал письма не затёрт обрывком"
    );
    assert_eq!(
        db.get_message(message)
            .await
            .expect("разобрать письмо после проекции")
            .attachments
            .len(),
        1,
        "вложение пережило лёгкую проекцию"
    );

    // Письмо, пришедшее только проекцией, помечено неполным, и догрузка тела
    // снимает пометку - иначе оно навсегда осталось бы обрывком.
    db.save_discovered_messages(
        account,
        &[DiscoveredMessage {
            folder_path: "INBOX".into(),
            uid: 6,
            remote_id: Some("metadata-only".into()),
            size: Some(50_000_000),
            seen: false,
            flagged: false,
            answered: false,
            draft: false,
            raw: "Subject: Large\r\n\r\nобрывок".as_bytes().to_vec(),
            body_fetched: false,
            has_attachments: None,
        }],
        false,
    )
    .await
    .expect("сохранить проекцию без тела");
    let (projection,): (i64,) =
        sqlx::query_as("SELECT id FROM messages WHERE folder_id=? AND uid=6")
            .bind(folder)
            .fetch_one(&db.pool)
            .await
            .expect("найти проекцию");
    assert!(
        !db.message_fetch_locator(projection)
            .await
            .expect("прочитать признак полноты проекции")
            .expect("проекция на месте")
            .4,
        "проекция помечена неполной"
    );
    db.store_fetched_raw(projection, "Subject: Large\r\n\r\nполное тело".as_bytes())
        .await
        .expect("догрузить тело");
    assert!(
        db.message_fetch_locator(projection)
            .await
            .expect("прочитать признак полноты после догрузки")
            .expect("проекция на месте")
            .4,
        "догруженное письмо стало полным"
    );
    db.close().await;
}

/// Умная папка отбирает письма по своим условиям и только из включённых
/// источников: отключённая папка не должна возвращаться в список окольным
/// путём. Опущенный при сохранении набор пользовательских папок удаляется -
/// интерфейс отдаёт весь набор целиком.
#[tokio::test]
async fn a_smart_folder_follows_its_conditions_and_sources() {
    let db: TestDb = open_test_db("repo-smart").await;
    let account = seed_account(&db, "repo@example.test").await;
    let folder = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    let message = seed_message(&db, account, folder, 1, "тайный договор").await;
    seed_message(&db, account, folder, 2, "обычное письмо").await;
    let page = db.limit_count(LIMIT_SMART_MESSAGE_PAGE);

    let smart = SmartFolder {
        id: "test-subject".into(),
        name: "Тайное".into(),
        icon: Some("search".into()),
        is_builtin: false,
        enabled: true,
        sort_order: 99,
        groups: vec![SmartConditionGroup {
            logic: "all".into(),
            conditions: vec![SmartCondition {
                field: "subject".into(),
                op: "contains".into(),
                value: "тайный".into(),
                unit: None,
                value2: None,
            }],
        }],
    };
    db.save_smart_folders(std::slice::from_ref(&smart))
        .await
        .expect("сохранить умную папку");
    assert!(
        db.list_smart_folders()
            .await
            .expect("перечень умных папок")
            .iter()
            .any(|item| item.id == smart.id)
    );

    let selected = db
        .list_smart_folder_messages(&smart.id, page)
        .await
        .expect("выполнить умную папку");
    assert_eq!(
        selected.iter().map(|item| item.id).collect::<Vec<_>>(),
        vec![message],
        "умная папка отобрала письмо по условию темы"
    );

    db.set_unified_source(folder, false)
        .await
        .expect("исключить источник");
    assert!(
        db.list_smart_folder_messages(&smart.id, page)
            .await
            .expect("выполнить умную папку без источника")
            .is_empty(),
        "письма исключённого источника в умную папку не попадают"
    );
    db.set_unified_source(folder, true)
        .await
        .expect("вернуть источник");

    db.save_smart_folders(&[])
        .await
        .expect("сохранить набор без пользовательской папки");
    assert!(
        !db.list_smart_folders()
            .await
            .expect("перечень после удаления")
            .iter()
            .any(|item| item.id == smart.id),
        "опущенная в наборе папка удалена"
    );
    db.close().await;
}

/// Удаление ящика уносит его письма вместе с поисковым индексом. Осиротевшая
/// строка индекса показывала бы в поиске письмо, которого уже нет, а открыть
/// его было бы нечем.
#[tokio::test]
async fn deleting_an_account_clears_its_search_index() {
    let db: TestDb = open_test_db("repo-cascade").await;
    let account = seed_account(&db, "repo@example.test").await;
    let folder = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    let message = seed_message(&db, account, folder, 1, "тайный договор").await;
    let kept = seed_account(&db, "other@example.test").await;
    let kept_folder = seed_folder(&db, kept, "INBOX", Some("inbox")).await;
    seed_message(&db, kept, kept_folder, 1, "тайный договор").await;

    use crate::search::{Fts5Index, SearchIndex};
    let search = Fts5Index::new(db.clone());
    let page = db.limit(LIMIT_MESSAGE_PAGE);
    assert!(
        search
            .search("тайный", page)
            .await
            .expect("искать до удаления")
            .contains(&message),
        "сохранённое письмо попало в поиск"
    );

    sqlx::query("DELETE FROM accounts WHERE id=?")
        .bind(account)
        .execute(&db.write_pool)
        .await
        .expect("удалить ящик");

    let found = search
        .search("тайный", page)
        .await
        .expect("искать после удаления");
    assert!(
        !found.contains(&message),
        "письмо удалённого ящика ушло из поиска"
    );
    assert_eq!(
        found.len(),
        1,
        "письмо оставшегося ящика из поиска не пропало"
    );
    db.close().await;
}

/// Локальный сервер программного доступа пускает только по выданному ключу и
/// только к разрешённым ему средствам, а отказ записывает в журнал. Без этого
/// любая программа на той же машине читала бы чужую почту, и следа бы не
/// осталось.
#[tokio::test]
async fn the_local_api_checks_tokens_and_records_denied_calls() {
    use sha2::Digest as _;

    let db: TestDb = open_test_db("repo-api").await;
    let account = seed_account(&db, "repo@example.test").await;
    let folder = seed_folder(&db, account, "INBOX", Some("inbox")).await;
    let message = seed_message(&db, account, folder, 1, "Договор").await;

    // Ключ доступа хранится отпечатком: в базе его открытого вида нет.
    let token = "tm_integration_test_token";
    let fingerprint = sha2::Sha256::digest(token.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    sqlx::query(
        "INSERT INTO api_clients(name, token_ref, token_hash, caps)
         VALUES('integration', 'not-used-in-test', ?, '[\"read\"]')",
    )
    .bind(fingerprint)
    .execute(&db.write_pool)
    .await
    .expect("выдать ключ доступа");

    let core = Arc::new(crate::Core {
        db: db.clone(),
        search: Arc::new(crate::search::Fts5Index::new(db.clone())),
        crypto: Arc::new(crate::crypto::StorageCrypto::from_key([7_u8; 32])),
        accounts: crate::account::AccountManager::new(db.clone()),
    });
    let journal = core.clone();
    let server = crate::api::start_server(core, 0)
        .await
        .expect("запустить локальный сервер");
    let base = format!("http://127.0.0.1:{}", server.port);
    let http = reqwest::Client::new();

    assert_eq!(
        http.get(format!("{base}/health"))
            .send()
            .await
            .expect("опросить состояние")
            .status(),
        reqwest::StatusCode::OK,
        "состояние сервера видно без ключа"
    );
    assert_eq!(
        http.get(format!("{base}/v1/tools"))
            .send()
            .await
            .expect("запрос без ключа")
            .status(),
        reqwest::StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        http.get(format!("{base}/v1/tools"))
            .bearer_auth("tm_invalid")
            .send()
            .await
            .expect("запрос с чужим ключом")
            .status(),
        reqwest::StatusCode::UNAUTHORIZED
    );

    let tools: serde_json::Value = http
        .get(format!("{base}/v1/tools"))
        .bearer_auth(token)
        .send()
        .await
        .expect("перечень средств по ключу")
        .json()
        .await
        .expect("разобрать перечень средств");
    let tools = tools["tools"].as_array().expect("перечень средств").clone();
    assert!(
        tools.iter().any(|tool| tool["name"] == "list_messages"),
        "разрешённое чтение в перечне есть"
    );
    assert!(
        !tools.iter().any(|tool| tool["name"] == "send"),
        "отправка ключу с одним чтением не показана"
    );

    assert_eq!(
        http.post(format!("{base}/v1/tools/label"))
            .bearer_auth(token)
            .json(&serde_json::json!({"message_id": message, "label_id": 1}))
            .send()
            .await
            .expect("вызов запрещённого средства")
            .status(),
        reqwest::StatusCode::FORBIDDEN
    );
    server.stop();

    assert!(
        crate::api::list_audit(journal.as_ref(), db.limit(LIMIT_MESSAGE_PAGE))
            .await
            .expect("прочитать журнал доступа")
            .iter()
            .any(|entry| entry.action == "tool:label:denied"),
        "отказ записан в журнал доступа"
    );
    db.close().await;
}
