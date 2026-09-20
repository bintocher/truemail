//! Сценарная проверка счётчиков писем в боковой панели после работы правил и
//! очереди операций.
//!
//! Счётчик умной папки и её список - два разных прохода по письмам: список
//! читает страницами с сортировкой, счётчик идёт потоком без предела. Разойдись
//! их условия хоть в одном месте, пользователь видит на папке число, которого
//! в ней нет: счётчик обещает непрочитанные письма, а список пуст.
//!
//! Проверка ведёт письма настоящим путём: правило помечает часть писем
//! прочитанными, другое правило уводит письма в корзину, очередь выполняется
//! работником. Сверяются три независимых счёта: число от счётчика, длина
//! списка и прямой пересчёт писем по правилу их жизни.

use super::Db;
use super::repo::test_storage::{TestDb, open_test_db};
use crate::backend::DiscoveredMessage;
use crate::model::*;

const UNREAD_FOLDER: &str = "all-unread";
const INBOX_FOLDER: &str = "all-inbox";

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

fn discovered(uid: u32, from: &str) -> DiscoveredMessage {
    let raw = format!(
        "From: Отправитель <{from}>\r\nTo: me@example.test\r\nSubject: Письмо\r\n\
         Message-ID: <counts-{uid}@example.test>\r\nDate: Mon, 14 Sep 2026 10:00:00 +0000\r\n\r\n\
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

fn rule(id: &str, sender: &str, action: &str) -> MailRuleInput {
    MailRuleInput {
        id: id.to_owned(),
        name: format!("Правило {id}"),
        account_id: None,
        enabled: true,
        groups: vec![MailRuleGroup {
            logic: "all".into(),
            conditions: vec![MailRuleCondition {
                field: "sender_address".into(),
                op: "equals".into(),
                value: sender.to_owned(),
                unit: None,
                value2: None,
            }],
        }],
        exceptions: Vec::new(),
        actions: vec![MailRuleAction {
            kind: action.to_owned(),
            folder_id: None,
            folder_role: None,
            label_id: None,
        }],
        confirm_key: None,
    }
}

/// Прямой пересчёт непрочитанных писем по правилу жизни письма: письмо живо,
/// пока не отложено и пока по нему нет незавершённого увода. Счёт записан
/// здесь своим запросом намеренно - копия запроса счётчика подтверждала бы
/// саму себя.
async fn unread_by_hand(db: &Db) -> i64 {
    sqlx::query_as::<_, (i64,)>(
        "SELECT count(*) FROM messages m
          WHERE m.seen=0
            AND (m.snoozed_until IS NULL OR m.snoozed_until <= datetime('now'))
            AND NOT EXISTS (
              SELECT 1 FROM outbox_ops o WHERE o.message_id=m.id
                AND o.op_kind IN ('move','delete') AND o.status IN ('pending','processing','retry')
            )",
    )
    .fetch_one(&db.pool)
    .await
    .expect("пересчитать непрочитанные письма")
    .0
}

/// Счётчик умной папки и её список: возвращает число от счётчика, число
/// непрочитанных от счётчика и длину списка.
async fn counts_and_list(db: &Db, stable_id: &str) -> (i64, i64, i64) {
    let counts = db
        .count_smart_folder_messages(&[stable_id.to_owned()])
        .await
        .expect("счётчики умных папок");
    let count = counts
        .iter()
        .find(|item| item.id == stable_id)
        .unwrap_or_else(|| panic!("счётчик папки {stable_id} не вернулся"));
    let listed = db
        .list_smart_folder_messages(stable_id, db.limit_count(LIMIT_SMART_MESSAGE_PAGE))
        .await
        .expect("список умной папки")
        .len() as i64;
    (count.total, count.unread, listed)
}

/// Выполнить всю очередь ящика настоящим путём: работник забирает операции и
/// закрывает их успехом.
async fn drain_queue(db: &Db, account_id: i64) {
    // Срок первой попытки у поставленной операции отложен на окно отмены:
    // проверке нужна наступившая очередь, а не выжданное окно.
    sqlx::query("UPDATE outbox_ops SET next_attempt_at=datetime('now','-1 second')")
        .execute(&db.write_pool)
        .await
        .expect("сдвинуть сроки попыток");
    loop {
        let operations = db
            .claim_outbox_operations(account_id, db.limit(LIMIT_STAGE_BATCH))
            .await
            .expect("забрать операции");
        if operations.is_empty() {
            break;
        }
        for operation in &operations {
            db.complete_outbox_operation(operation)
                .await
                .expect("закрыть операцию успехом");
        }
    }
}

/// После прогона правил и выполнения очереди счётчик непрочитанных писем
/// согласован со списком и с прямым пересчётом - и в тот момент, когда письма
/// уже стоят в очереди на увод, и после того, как очередь их унесла.
#[tokio::test]
async fn unread_counter_matches_the_list_after_rules_and_queue() {
    let db: TestDb = open_test_db("folder-counts").await;
    let account = seed_account(&db, "me@example.test").await;
    seed_folder(&db, account, "INBOX", "inbox").await;
    seed_folder(&db, account, "Trash", "trash").await;

    db.save_mail_rule(&rule("rule-read", "reports@example.test", "mark_read"), false, None)
        .await
        .expect("правило отметки прочитанным");
    db.save_mail_rule(&rule("rule-trash", "list@example.test", "trash"), false, None)
        .await
        .expect("правило уборки в корзину");

    // Три письма пометит правило, два уведёт правило, два останутся нетронутыми.
    let mut letters = Vec::new();
    letters.extend((1..=3).map(|uid| discovered(uid, "reports@example.test")));
    letters.extend((4..=5).map(|uid| discovered(uid, "list@example.test")));
    letters.extend((6..=7).map(|uid| discovered(uid, "boss@example.test")));
    let total_letters = letters.len() as i64;
    db.save_discovered_messages(account, &letters, false)
        .await
        .expect("синхронизация принесла письма");

    let (total, unread, listed) = counts_and_list(&db, UNREAD_FOLDER).await;
    assert_eq!(
        (total, listed, unread_by_hand(&db).await),
        (total_letters, total_letters, total_letters),
        "до правил счётчик непрочитанных разошёлся со списком или с пересчётом"
    );
    assert_eq!(unread, total, "в папке непрочитанных нет прочитанных писем");

    db.process_sync_batch_stages()
        .await
        .expect("прогон стадий обработки");

    // Письма, поставленные в очередь на увод, уже не показываются: счётчик
    // обязан это учитывать наравне со списком, иначе число на папке обещает
    // письма, которых в ней нет.
    let queued: (i64,) = sqlx::query_as(
        "SELECT count(*) FROM outbox_ops
          WHERE op_kind IN ('move','delete') AND status IN ('pending','processing','retry')",
    )
    .fetch_one(&db.pool)
    .await
    .expect("прочитать очередь");
    assert_eq!(queued.0, 2, "правило уборки поставило не две операции увода");

    let (_, unread, listed) = counts_and_list(&db, UNREAD_FOLDER).await;
    let by_hand = unread_by_hand(&db).await;
    assert_eq!(
        (unread, listed, by_hand),
        (2, 2, 2),
        "пока увод стоит в очереди, счётчик, список и пересчёт разошлись"
    );

    drain_queue(&db, account).await;

    let (total, unread, listed) = counts_and_list(&db, UNREAD_FOLDER).await;
    let by_hand = unread_by_hand(&db).await;
    assert_eq!(
        (unread, listed, by_hand),
        (2, 2, 2),
        "после выполнения очереди счётчик, список и пересчёт разошлись"
    );
    assert_eq!(total, unread, "в папке непрочитанных оказались прочитанные письма");

    // Общий список входящих считается тем же проходом: уведённые письма ушли,
    // помеченные прочитанными остались.
    let (inbox_total, inbox_unread, inbox_listed) = counts_and_list(&db, INBOX_FOLDER).await;
    assert_eq!(
        (inbox_total, inbox_listed),
        (total_letters - 2, total_letters - 2),
        "счётчик входящих разошёлся со списком после уборки"
    );
    assert_eq!(
        inbox_unread, 2,
        "во входящих непрочитанных больше, чем осталось нетронутых писем"
    );
    db.close().await;
}
