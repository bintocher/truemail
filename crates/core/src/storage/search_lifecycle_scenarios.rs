//! Сценарная проверка поиска на обычном пути жизни письма: письмо пришло
//! синхронизацией, нашлось поиском, было удалено навсегда - и из поискового
//! индекса ушло вместе со строкой письма.
//!
//! Путь настоящий во всех звеньях: письмо сохраняется тем же вызовом, каким
//! его сохраняет синхронизация (с разбором настоящего MIME и записью тела в
//! индекс), поиск идёт тем же запросом и тем же построителем запроса, что и
//! поиск в программе, а удаление проходит через очередь операций, как у
//! пользователя.
//!
//! Проверка ловит два разных дефекта. Первый - письмо сохранено, но в индекс
//! не попало: список его показывает, а поиск не находит, и пользователь
//! считает, что письма нет. Второй - письмо удалено, а строка индекса
//! осталась: поиск отдаёт номер несуществующего письма, и открыть его нельзя.

use super::Db;
use super::repo::test_storage::{TestDb, open_test_db};
use crate::backend::DiscoveredMessage;
use crate::model::*;
use crate::search::{Fts5Index, SearchIndex, prefix_query};

/// Слово темы и слово тела: тема попадает в индекс триггером базы, тело -
/// отдельной записью после разбора письма. Дефект обычно задевает только одно
/// из двух, поэтому проверяются оба.
const SUBJECT_WORD: &str = "квартальный";
const BODY_WORD: &str = "отгрузки";

fn discovered(uid: u32) -> DiscoveredMessage {
    let raw = format!(
        "From: Начальник <boss@example.test>\r\nTo: me@example.test\r\n\
         Subject: Отчёт {SUBJECT_WORD}\r\nMessage-ID: <search-{uid}@example.test>\r\n\
         Date: Mon, 14 Sep 2026 10:00:00 +0000\r\n\r\nСроки {BODY_WORD} уточнены\r\n"
    );
    DiscoveredMessage {
        folder_path: "INBOX".to_owned(),
        uid,
        remote_id: Some(format!("remote-{uid}")),
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

async fn seed_folder(db: &Db, account_id: i64, path: &str, role: &str) -> i64 {
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

/// Поиск тем же путём, каким ищет программа: запрос строится тем же
/// построителем, предел выдачи берётся из настройки.
async fn found_by(db: &Db, word: &str) -> Vec<i64> {
    let index = Fts5Index::new((*db).clone());
    let query = prefix_query(word).expect("поисковый запрос из слова");
    index
        .search(&query, db.limit(LIMIT_MESSAGE_FIRST_PAGE))
        .await
        .expect("поиск")
}

/// Письмо сохранено синхронизацией и находится поиском; после удаления
/// навсегда оно уходит и из списка, и из поискового индекса.
#[tokio::test]
async fn a_message_is_searchable_while_it_lives_and_gone_after_deletion() {
    let db: TestDb = open_test_db("search-lifecycle").await;
    let account = seed_account(&db, "me@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", "inbox").await;
    // Папка корзины нужна соседнему письму, которое остаётся на месте: без
    // него проверка "поиск больше ничего не находит" была бы проверкой пустой
    // базы, а не проверкой ухода одного письма.
    seed_folder(&db, account, "Trash", "trash").await;
    db.save_discovered_messages(account, &[discovered(1), discovered(2)], false)
        .await
        .expect("синхронизация принесла письма");

    let ids: Vec<i64> =
        sqlx::query_as::<_, (i64,)>("SELECT id FROM messages WHERE folder_id=? ORDER BY uid")
            .bind(inbox)
            .fetch_all(&db.pool)
            .await
            .expect("найти письма")
            .into_iter()
            .map(|row| row.0)
            .collect();
    assert_eq!(ids.len(), 2, "синхронизация сохранила не два письма");
    let (doomed, kept) = (ids[0], ids[1]);

    let by_subject = found_by(&db, SUBJECT_WORD).await;
    assert!(
        by_subject.contains(&doomed) && by_subject.contains(&kept),
        "сохранённые письма не находятся по слову из темы: {by_subject:?}"
    );
    let by_body = found_by(&db, BODY_WORD).await;
    assert!(
        by_body.contains(&doomed) && by_body.contains(&kept),
        "сохранённые письма не находятся по слову из тела: {by_body:?}"
    );
    // Поиск отдаёт номера писем, и по ним программа читает список: номер, по
    // которому письма не прочитать, для пользователя означает пустую строку.
    let listed = db
        .list_messages_by_ids(&by_subject)
        .await
        .expect("прочитать найденные письма");
    assert_eq!(
        listed.len(),
        by_subject.len(),
        "поиск вернул номера, по которым писем не прочитать"
    );

    // Удаление навсегда настоящим путём: команда пользователя, очередь,
    // работник и успешное завершение операции.
    let queued = db
        .queue_message_action(&[doomed], "delete")
        .await
        .expect("поставить удаление в очередь");
    assert_eq!(
        queued.operation_ids.len(),
        1,
        "удаление не встало в очередь"
    );
    sqlx::query("UPDATE outbox_ops SET next_attempt_at=datetime('now','-1 second') WHERE id=?")
        .bind(queued.operation_ids[0])
        .execute(&db.write_pool)
        .await
        .expect("сдвинуть срок попытки");
    let claimed = db
        .claim_outbox_operations(account, 1)
        .await
        .expect("забрать операцию");
    assert_eq!(claimed.len(), 1, "операция удаления не досталась работнику");
    db.complete_outbox_operation(&claimed[0])
        .await
        .expect("сервер удалил письмо");

    let rows: (i64,) = sqlx::query_as("SELECT count(*) FROM messages WHERE id=?")
        .bind(doomed)
        .fetch_one(&db.pool)
        .await
        .expect("прочитать письма");
    assert_eq!(rows.0, 0, "удалённое письмо осталось в базе");

    let by_subject = found_by(&db, SUBJECT_WORD).await;
    assert!(
        !by_subject.contains(&doomed),
        "поиск по теме всё ещё отдаёт удалённое письмо: {by_subject:?}"
    );
    assert!(
        by_subject.contains(&kept),
        "поиск потерял оставшееся письмо: {by_subject:?}"
    );
    let by_body = found_by(&db, BODY_WORD).await;
    assert!(
        !by_body.contains(&doomed),
        "поиск по телу всё ещё отдаёт удалённое письмо: {by_body:?}"
    );
    assert!(
        by_body.contains(&kept),
        "поиск по телу потерял оставшееся письмо: {by_body:?}"
    );

    // Строка индекса уходит вместе с письмом: осиротевшая строка молча
    // распухала бы базу и отдавала бы поиску несуществующие номера.
    let orphans: (i64,) = sqlx::query_as("SELECT count(*) FROM messages_fts WHERE rowid=?")
        .bind(doomed)
        .fetch_one(&db.pool)
        .await
        .expect("прочитать поисковый индекс");
    assert_eq!(
        orphans.0, 0,
        "в поисковом индексе осталась строка удалённого письма"
    );
    db.close().await;
}
