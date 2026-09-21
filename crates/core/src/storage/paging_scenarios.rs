//! Сценарные проверки выдачи списка писем страницами по курсору.
//!
//! Проверка идёт по настоящему пути: настоящая база с применёнными
//! миграциями и те же вызовы, которыми список писем пользуется в работе -
//! первая страница и страницы после курсора. Размер страницы берётся из
//! настройки предела, а не из числа в проверке.
//!
//! Главный случай здесь - письма с одинаковой датой. Во всех прежних
//! проверках даты писем были разными, и потеря вторичного порядка по номеру
//! письма проходила мимо них: при равных датах страница отдавала бы письма в
//! произвольном порядке, курсор двигался бы наугад, и пользователь видел бы
//! одни письма дважды, а другие не видел вовсе.

use super::Db;
use super::repo::test_storage::{TestDb, open_test_db};
use crate::model::*;
use std::collections::HashSet;

/// Одна и та же дата у всех писем пачки: так приходит выгрузка, сделанная
/// одним заходом, и так выглядят письма рассылки, отправленные пачкой.
const SAME_DATE: &str = "2026-09-14T10:00:00+00:00";

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

async fn seed_folder(db: &Db, account_id: i64) -> i64 {
    sqlx::query_as::<_, (i64,)>(
        "INSERT INTO folders(account_id, remote_path, display_name, role)
         VALUES(?, 'INBOX', 'Входящие', 'inbox') RETURNING id",
    )
    .bind(account_id)
    .fetch_one(&db.write_pool)
    .await
    .expect("создать папку")
    .0
}

/// Письма одной пачкой в одной неделимой операции: тысяча отдельных записей в
/// зашифрованную базу заняла бы заметное время, а проверяется не запись.
async fn seed_messages_with_one_date(db: &Db, account_id: i64, folder_id: i64, count: i64) {
    let mut tx = db.begin_write().await.expect("начать запись");
    for index in 0..count {
        sqlx::query(
            "INSERT INTO messages(account_id, folder_id, uid, from_addr, subject, preview, date,
                                  rfc822_message_id, remote_id, size)
             VALUES(?, ?, ?, 'list@example.test', 'Рассылка', '', ?, ?, ?, 1024)",
        )
        .bind(account_id)
        .bind(folder_id)
        .bind(index + 1)
        .bind(SAME_DATE)
        .bind(format!("<same-{index}@example.test>"))
        .bind(format!("remote-{index}"))
        .execute(&mut *tx)
        .await
        .expect("сохранить письмо");
    }
    tx.commit().await.expect("записать пачку писем");
}

/// Полный обход списка папки страницами по курсору: ни одно письмо не
/// повторяется и ни одно не пропадает.
///
/// Письма намеренно с одной датой. Порядок выдачи обязан доопределяться
/// номером письма: курсор запоминает пару "дата и номер" и следующая страница
/// отбирает строго то, что меньше этой пары. Без вторичного порядка страницы
/// начинают перекрываться, и список показывает одни письма дважды, а другие
/// теряет.
#[tokio::test]
async fn cursor_paging_covers_every_message_when_dates_are_equal() {
    let db: TestDb = open_test_db("paging-same-date").await;
    let account = seed_account(&db, "me@example.test").await;
    let folder = seed_folder(&db, account).await;

    // Размер страницы - настройка. Берём наименьшее допустимое значение:
    // страниц получается много, и перекрытие соседних страниц проявляется.
    let page = limit_spec(LIMIT_MESSAGE_PAGE)
        .expect("описание предела страницы")
        .min;
    db.set_limit(LIMIT_MESSAGE_PAGE, page)
        .await
        .expect("записать предел страницы");
    let page = db.limit(LIMIT_MESSAGE_PAGE);
    // Сто полных страниц и ещё неполная: так проверяется и стык страниц, и
    // остановка обхода на неполной странице.
    let total = page * 100 + page / 2;
    seed_messages_with_one_date(&db, account, folder, total).await;

    let expected: Vec<i64> =
        sqlx::query_as::<_, (i64,)>("SELECT id FROM messages WHERE folder_id=? ORDER BY id DESC")
            .bind(folder)
            .fetch_all(&db.pool)
            .await
            .expect("прочитать номера писем")
            .into_iter()
            .map(|row| row.0)
            .collect();
    assert_eq!(
        expected.len() as i64,
        total,
        "проверка засеяла не все письма"
    );

    let mut walked: Vec<i64> = Vec::new();
    let mut cursor: Option<(String, i64)> = None;
    let mut pages = 0_i64;
    loop {
        let batch = db
            .list_messages_page(
                folder,
                cursor.as_ref().map(|(date, _)| date.as_str()),
                cursor.as_ref().map(|(_, id)| *id),
                page,
            )
            .await
            .expect("страница списка");
        if batch.is_empty() {
            break;
        }
        pages += 1;
        // Обход не должен уметь длиться вечно: при перекрытии страниц курсор
        // может встать на месте, и проверка обязана закончиться отчётом, а не
        // зависанием.
        assert!(
            pages <= total,
            "обход не заканчивается: страниц уже {pages} при {total} письмах"
        );
        let last = batch.last().expect("непустая страница");
        cursor = Some((last.date.clone().unwrap_or_default(), last.id));
        walked.extend(batch.iter().map(|message| message.id));
        if (batch.len() as i64) < page {
            break;
        }
    }

    let unique: HashSet<i64> = walked.iter().copied().collect();
    let repeated: Vec<i64> = {
        let mut seen = HashSet::new();
        walked
            .iter()
            .copied()
            .filter(|id| !seen.insert(*id))
            .collect()
    };
    assert!(
        repeated.is_empty(),
        "обход показал письма повторно: {} штук, например {:?}",
        repeated.len(),
        repeated.iter().take(5).collect::<Vec<_>>()
    );
    let missing: Vec<i64> = expected
        .iter()
        .copied()
        .filter(|id| !unique.contains(id))
        .collect();
    assert!(
        missing.is_empty(),
        "обход потерял письма: {} штук, например {:?}",
        missing.len(),
        missing.iter().take(5).collect::<Vec<_>>()
    );
    assert_eq!(
        walked, expected,
        "при равных датах порядок выдачи обязан идти по убыванию номера письма"
    );
    db.close().await;
}

/// Первая страница списка и первая страница обхода по курсору - один и тот же
/// порядок. Первая страница читается другим запросом, и разойдись они, второй
/// запрос продолжал бы список не с того места.
#[tokio::test]
async fn the_first_page_and_the_cursor_walk_agree_on_order() {
    let db: TestDb = open_test_db("paging-first-page").await;
    let account = seed_account(&db, "me@example.test").await;
    let folder = seed_folder(&db, account).await;

    let page = limit_spec(LIMIT_MESSAGE_FIRST_PAGE)
        .expect("описание предела первой страницы")
        .min;
    db.set_limit(LIMIT_MESSAGE_FIRST_PAGE, page)
        .await
        .expect("записать предел первой страницы");
    let page = db.limit(LIMIT_MESSAGE_FIRST_PAGE);
    seed_messages_with_one_date(&db, account, folder, page * 3).await;

    let first = db
        .list_folder_first_page(folder, page)
        .await
        .expect("первая страница");
    let walked = db
        .list_messages_page(folder, None, None, page)
        .await
        .expect("первая страница обхода");
    assert_eq!(
        first.iter().map(|message| message.id).collect::<Vec<_>>(),
        walked.iter().map(|message| message.id).collect::<Vec<_>>(),
        "два пути первой страницы отдают разный порядок писем"
    );

    // Продолжение обхода не должно возвращать письма первой страницы.
    let last = first.last().expect("первая страница не пуста");
    let next = db
        .list_messages_page(folder, last.date.as_deref(), Some(last.id), page)
        .await
        .expect("вторая страница");
    let first_ids: HashSet<i64> = first.iter().map(|message| message.id).collect();
    let overlap: Vec<i64> = next
        .iter()
        .map(|message| message.id)
        .filter(|id| first_ids.contains(id))
        .collect();
    assert!(
        overlap.is_empty(),
        "вторая страница повторила письма первой: {overlap:?}"
    );
    db.close().await;
}
