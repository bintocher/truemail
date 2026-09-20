//! Сценарная проверка жизненного пути операции очереди увода:
//! ожидание - передача - повтор - отказ (specs/mail-rules-conditions-and-actions.md,
//! S-052, S-053, S-113, S-114).
//!
//! Путь настоящий: письмо приходит синхронизацией, пользователь переносит его
//! в корзину той же командой, что и в списке, операцию забирает работник
//! очереди, отказ сервера считается теми же правилами повтора. Сервера в
//! проверке нет, поэтому его отказ подаётся тем же вызовом, которым его
//! подаёт работник.
//!
//! Пока операция не завершилась отказом, письма нет ни в одном списке и о нём
//! не уведомляют: программа не зовёт читать почту, которой во Входящих через
//! мгновение не будет. После отказа письмо возвращается: сервер его не унёс, и
//! пользователь должен увидеть письмо на прежнем месте.

use super::Db;
use super::repo::test_storage::{TestDb, open_test_db};
use crate::backend::DiscoveredMessage;
use crate::model::*;

/// Письмо в том виде, в каком его приносит синхронизация.
fn discovered(uid: u32) -> DiscoveredMessage {
    let raw = format!(
        "From: Начальник <boss@example.test>\r\nTo: me@example.test\r\nSubject: Договор\r\n\
         Message-ID: <queue-{uid}@example.test>\r\nDate: Mon, 14 Sep 2026 10:00:00 +0000\r\n\r\n\
         Текст письма\r\n"
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

async fn operation_status(db: &Db, operation_id: i64) -> String {
    sqlx::query_as::<_, (String,)>("SELECT status FROM outbox_ops WHERE id=?")
        .bind(operation_id)
        .fetch_one(&db.pool)
        .await
        .expect("прочитать состояние операции")
        .0
}

/// Срок следующей попытки переносится в прошлое: проверке нужна наступившая
/// очередь попытки, а не выжданная задержка повтора.
async fn make_ready(db: &Db, operation_id: i64) {
    sqlx::query("UPDATE outbox_ops SET next_attempt_at=datetime('now','-1 second') WHERE id=?")
        .bind(operation_id)
        .execute(&db.write_pool)
        .await
        .expect("сдвинуть срок попытки");
}

/// Виден ли пользователь письмо: список папки, сводный список последних писем
/// и умная папка всех входящих.
async fn message_is_visible(db: &Db, folder_id: i64, message_id: i64) -> bool {
    let in_folder = db
        .list_folder_first_page(folder_id, db.limit(LIMIT_MESSAGE_FIRST_PAGE))
        .await
        .expect("список папки")
        .iter()
        .any(|message| message.id == message_id);
    let in_recent = db
        .list_recent_messages(db.limit(LIMIT_MESSAGE_FIRST_PAGE))
        .await
        .expect("сводный список")
        .iter()
        .any(|message| message.id == message_id);
    let in_smart = db
        .list_smart_folder_messages("all-inbox", db.limit_count(LIMIT_SMART_MESSAGE_PAGE))
        .await
        .expect("список умной папки")
        .iter()
        .any(|message| message.id == message_id);
    assert_eq!(
        in_folder, in_recent,
        "список папки и сводный список разошлись во мнении о письме"
    );
    assert_eq!(
        in_folder, in_smart,
        "список папки и умная папка разошлись во мнении о письме"
    );
    in_folder
}

/// Позовёт ли программа читать это письмо: отбор уведомления о новой почте и
/// проверка перед самым показом.
async fn message_is_announced(db: &Db, account_id: i64, message_id: i64, remote_id: &str) -> bool {
    let picked = db
        .inbox_message_ids_by_remote_ids(account_id, &[remote_id.to_owned()], None, None)
        .await
        .expect("отбор писем для уведомления")
        .contains(&message_id);
    let notifiable = db
        .message_is_notifiable(message_id)
        .await
        .expect("проверка перед показом уведомления");
    assert_eq!(
        picked, notifiable,
        "отбор уведомления и проверка перед показом разошлись"
    );
    picked
}

/// Письмо скрыто на всём пути операции - от постановки в очередь до отказа, -
/// а после отказа возвращается на прежнее место и новую операцию по нему
/// поставить нельзя до решения пользователя.
#[tokio::test]
async fn a_queued_message_stays_hidden_until_the_operation_fails() {
    let db: TestDb = open_test_db("queue-lifecycle").await;
    let account = seed_account(&db, "me@example.test").await;
    let inbox = seed_folder(&db, account, "INBOX", "inbox").await;
    seed_folder(&db, account, "Trash", "trash").await;
    db.save_discovered_messages(account, &[discovered(1)], false)
        .await
        .expect("синхронизация принесла письмо");
    let message = sqlx::query_as::<_, (i64,)>("SELECT id FROM messages WHERE folder_id=? AND uid=1")
        .bind(inbox)
        .fetch_one(&db.pool)
        .await
        .expect("найти письмо")
        .0;

    assert!(
        message_is_visible(&db, inbox, message).await,
        "только что полученное письмо обязано быть в списке"
    );
    assert!(
        message_is_announced(&db, account, message, "remote-1").await,
        "о новом письме обязано быть уведомление"
    );

    // Пользователь переносит письмо в корзину - та же команда, что в списке.
    let queued = db
        .queue_message_action(&[message], "trash")
        .await
        .expect("поставить перенос в очередь");
    assert_eq!(queued.operation_ids.len(), 1, "перенос не встал в очередь");
    let operation_id = queued.operation_ids[0];
    assert_eq!(operation_status(&db, operation_id).await, "pending");
    assert!(
        !message_is_visible(&db, inbox, message).await,
        "письмо, поставленное в очередь на перенос, осталось в списке"
    );
    assert!(
        !message_is_announced(&db, account, message, "remote-1").await,
        "программа зовёт читать письмо, которое сейчас унесёт очередь"
    );

    // Сколько раз повторять отказавшую операцию - настройка, и проверка идёт
    // ровно до неё: переход в отказ обязан случиться на последней попытке, а
    // не раньше и не позже.
    let attempts_limit = db.limit(LIMIT_OPERATION_ATTEMPTS);
    for attempt in 1..=attempts_limit {
        make_ready(&db, operation_id).await;
        // Работник берёт операции своего ящика; в проверке она одна.
        let claimed = db
            .claim_outbox_operations(account, 1)
            .await
            .expect("забрать операцию");
        assert_eq!(
            claimed.iter().map(|op| op.id).collect::<Vec<_>>(),
            vec![operation_id],
            "работник не получил операцию на попытке {attempt}"
        );
        assert_eq!(
            operation_status(&db, operation_id).await,
            "processing",
            "забранная операция обязана быть в состоянии передачи"
        );
        assert!(
            !message_is_visible(&db, inbox, message).await,
            "письмо показалось в списке во время передачи операции"
        );
        assert!(
            !message_is_announced(&db, account, message, "remote-1").await,
            "о письме уведомили во время передачи операции"
        );

        db.fail_outbox_operation(operation_id, "сервер не ответил")
            .await
            .expect("учесть отказ сервера");
        let status = operation_status(&db, operation_id).await;
        if attempt < attempts_limit {
            assert_eq!(
                status, "retry",
                "до предела попыток операция обязана оставаться в повторах (попытка {attempt})"
            );
            assert!(
                !message_is_visible(&db, inbox, message).await,
                "письмо показалось в списке между повторами"
            );
            assert!(
                !message_is_announced(&db, account, message, "remote-1").await,
                "о письме уведомили между повторами"
            );
        } else {
            assert_eq!(
                status, "failed",
                "на пределе попыток операция обязана перейти в отказ"
            );
        }
    }

    // S-052: отказавшая операция означает, что письмо не уведено. Оно обязано
    // вернуться на прежнее место, иначе пользователь теряет письмо молча.
    assert!(
        message_is_visible(&db, inbox, message).await,
        "письмо не вернулось в список после отказа операции"
    );
    let failed = db
        .failed_takeaway_operations()
        .await
        .expect("список отказавших операций");
    assert_eq!(
        failed.iter().map(|item| item.id).collect::<Vec<_>>(),
        vec![operation_id],
        "отказавшая операция обязана показываться пользователю"
    );

    // S-113, S-114: новая операция по письму не ставится, пока пользователь не
    // решил судьбу отказавшей.
    let again = db
        .queue_message_action(&[message], "trash")
        .await
        .expect("повторный перенос");
    assert!(
        again.operation_ids.is_empty(),
        "после отказа по письму поставлена вторая операция увода"
    );
    assert_eq!(
        again.skipped_failed, 1,
        "повтор обязан вернуться пропуском с причиной отказа"
    );

    // Пользователь выбрал повтор: письмо снова скрыто, операция снова в очереди.
    db.retry_failed_operation(operation_id)
        .await
        .expect("повторить операцию");
    assert_eq!(operation_status(&db, operation_id).await, "retry");
    assert!(
        !message_is_visible(&db, inbox, message).await,
        "после повтора письмо обязано снова скрыться из списка"
    );

    // Успешная попытка завершает путь: письма и операции больше нет.
    make_ready(&db, operation_id).await;
    let claimed = db
        .claim_outbox_operations(account, 1)
        .await
        .expect("забрать операцию после повтора");
    assert_eq!(claimed.len(), 1, "повторённая операция не досталась работнику");
    db.complete_outbox_operation(&claimed[0])
        .await
        .expect("закрыть операцию успехом");
    let left: (i64,) = sqlx::query_as("SELECT count(*) FROM outbox_ops WHERE id=?")
        .bind(operation_id)
        .fetch_one(&db.pool)
        .await
        .expect("прочитать очередь");
    assert_eq!(left.0, 0, "успешная операция обязана уйти из очереди");
    let rows: (i64,) = sqlx::query_as("SELECT count(*) FROM messages WHERE id=?")
        .bind(message)
        .fetch_one(&db.pool)
        .await
        .expect("прочитать письма");
    assert_eq!(rows.0, 0, "перенесённое письмо обязано уйти с прежнего места");
    db.close().await;
}
