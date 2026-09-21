//! Сценарные проверки возврата отложенного письма.
//!
//! Проверка идёт по настоящему пути: настоящая база, настоящая команда
//! откладывания и те же запросы списка и счётчика умной папки, которыми
//! пользуется боковая панель. Время не выжидается, а сдвигается в базе:
//! проверке нужен наступивший срок, а не прошедшая минута.
//!
//! Слабое место здесь - знак сравнения со сроком. Он записан в списке папки,
//! в списке умной папки и в её счётчике, и перевёрнутый знак означает, что
//! отложенное письмо не возвращается никогда, а письмо, отложенное на неделю
//! вперёд, висит в списке. Прежние проверки откладывания смотрели только на
//! письмо с будущим сроком, поэтому переворот знака проходил мимо них.

use super::Db;
use super::repo::test_storage::{TestDb, open_test_db};
use crate::model::*;

/// Умная папка всех непрочитанных писем: её счётчик показывает боковая панель.
const UNREAD_FOLDER: &str = "all-unread";

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

async fn seed_message(db: &Db, account_id: i64, folder_id: i64, uid: i64, subject: &str) -> i64 {
    sqlx::query_as::<_, (i64,)>(
        "INSERT INTO messages(account_id, folder_id, uid, from_addr, subject, preview, date,
                              rfc822_message_id, remote_id, size, seen)
         VALUES(?, ?, ?, 'boss@example.test', ?, '', datetime('now'), ?, ?, 2048, 0)
         RETURNING id",
    )
    .bind(account_id)
    .bind(folder_id)
    .bind(uid)
    .bind(subject)
    .bind(format!("<snooze-{uid}@example.test>"))
    .bind(format!("remote-{uid}"))
    .fetch_one(&db.write_pool)
    .await
    .expect("сохранить письмо")
    .0
}

/// Письма, которые список папки сейчас показывает.
async fn listed(db: &Db, folder_id: i64) -> Vec<i64> {
    db.list_folder_first_page(folder_id, db.limit(LIMIT_MESSAGE_FIRST_PAGE))
        .await
        .expect("список папки")
        .into_iter()
        .map(|message| message.id)
        .collect()
}

/// Письма умной папки непрочитанных и её счётчик - оба пути отбора.
async fn unread_view(db: &Db) -> (Vec<i64>, i64) {
    let messages = db
        .list_smart_folder_messages(UNREAD_FOLDER, db.limit_count(LIMIT_SMART_MESSAGE_PAGE))
        .await
        .expect("список умной папки")
        .into_iter()
        .map(|message| message.id)
        .collect();
    let counts = db
        .count_smart_folder_messages(&[UNREAD_FOLDER.to_owned()])
        .await
        .expect("счётчик умной папки");
    let total = counts
        .iter()
        .find(|count| count.id == UNREAD_FOLDER)
        .expect("счётчик непрочитанных")
        .total;
    (messages, total)
}

/// Сдвинуть срок письма в прошлое. Настоящее ожидание здесь было бы ожиданием
/// часов, а проверяется наступление срока, а не работа таймера.
async fn expire_snooze(db: &Db, message_id: i64) {
    sqlx::query("UPDATE messages SET snoozed_until=datetime('now','-1 minute') WHERE id=?")
        .bind(message_id)
        .execute(&db.write_pool)
        .await
        .expect("сдвинуть срок отложенного письма");
}

/// Отложенное письмо уходит из списка и из счётчика, а по наступлении срока
/// возвращается в оба.
///
/// Второе письмо остаётся отложенным на будущее и в проверке участвует
/// постоянно: без него перевёрнутый знак сравнения выглядел бы как рабочее
/// поведение - возвращалось бы всё подряд, включая то, что пользователь убрал
/// с глаз.
#[tokio::test]
async fn a_due_snoozed_message_returns_to_the_list_and_to_the_counter() {
    let db: TestDb = open_test_db("snooze-return").await;
    let account = seed_account(&db, "me@example.test").await;
    let inbox = seed_folder(&db, account).await;
    let due = seed_message(&db, account, inbox, 1, "Вернуть сегодня").await;
    let later = seed_message(&db, account, inbox, 2, "Вернуть через неделю").await;
    let plain = seed_message(&db, account, inbox, 3, "Обычное письмо").await;

    let (_, before) = unread_view(&db).await;
    assert_eq!(before, 3, "до откладывания счётчик видит все три письма");

    // Настоящий путь откладывания: та же команда, что и в списке писем.
    db.set_messages_snoozed(&[due], Some("2026-12-31T00:00:00+00:00"))
        .await
        .expect("отложить первое письмо");
    db.set_messages_snoozed(&[later], Some("2027-12-31T00:00:00+00:00"))
        .await
        .expect("отложить второе письмо");

    let visible = listed(&db, inbox).await;
    assert_eq!(
        visible,
        vec![plain],
        "отложенные письма обязаны уйти из списка папки"
    );
    let (unread, total) = unread_view(&db).await;
    assert_eq!(
        unread,
        vec![plain],
        "отложенные письма обязаны уйти из умной папки"
    );
    assert_eq!(total, 1, "счётчик обязан считать только показанное письмо");

    // Срок первого письма наступил.
    expire_snooze(&db, due).await;

    let visible = listed(&db, inbox).await;
    assert!(
        visible.contains(&due),
        "письмо с наступившим сроком не вернулось в список папки"
    );
    assert!(
        !visible.contains(&later),
        "письмо, отложенное на будущее, показано раньше срока"
    );
    let (unread, total) = unread_view(&db).await;
    assert!(
        unread.contains(&due),
        "письмо с наступившим сроком не вернулось в умную папку"
    );
    assert!(
        !unread.contains(&later),
        "умная папка показала письмо, отложенное на будущее"
    );
    assert_eq!(
        total,
        visible.len() as i64,
        "счётчик умной папки разошёлся со списком после возврата письма"
    );

    // Фоновый проход снимает наступивший срок, и только его: письмо с будущим
    // сроком обязано остаться отложенным.
    let released = db.release_due_snoozes().await.expect("снять сроки");
    assert_eq!(released, 1, "снят не ровно один наступивший срок");
    let remaining: Vec<(i64, Option<String>)> = sqlx::query_as(
        "SELECT id, snoozed_until FROM messages WHERE snoozed_until IS NOT NULL ORDER BY id",
    )
    .fetch_all(&db.pool)
    .await
    .expect("прочитать сроки");
    assert_eq!(
        remaining.iter().map(|row| row.0).collect::<Vec<_>>(),
        vec![later],
        "после прохода отложенным обязано остаться только письмо с будущим сроком"
    );

    let visible = listed(&db, inbox).await;
    assert_eq!(
        visible.len(),
        2,
        "после снятия срока список обязан показывать вернувшееся и обычное письмо"
    );
    db.close().await;
}
