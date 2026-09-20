//! Сценарные проверки одновременной работы: два действия приходят к одной
//! строке базы в одно время, и у каждого из них есть правильный единственный
//! исход.
//!
//! Проверки идут на рантайме из нескольких рабочих потоков и запускают
//! соперников отдельными задачами: одной задачей футуры чередуются только на
//! точках ожидания, а здесь проверяется именно одновременность. Остальные
//! проверки ядра однопоточные, и гонка в них не воспроизводится вовсе.
//!
//! Каждый предмет - отдельный класс потерь для пользователя: двойная уборка
//! писем по одному подтверждению, письмо, отправленное дважды, и вторая
//! операция увода по письму, которое уже уносит первая.

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
        "From: Отправитель <{from}>\r\nTo: me@example.test\r\nSubject: Рассылка\r\n\
         Message-ID: <race-{uid}@example.test>\r\nDate: Mon, 14 Sep 2026 10:00:00 +0000\r\n\r\n\
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

/// Сколько незавершённых операций увода стоит по письмам ящика.
async fn takeaway_count(db: &Db) -> i64 {
    sqlx::query_as::<_, (i64,)>(
        "SELECT count(*) FROM outbox_ops
          WHERE op_kind IN ('move','delete') AND status IN ('pending','processing','retry')",
    )
    .fetch_one(&db.pool)
    .await
    .expect("прочитать очередь")
    .0
}

/// Одно подтверждение уборки, пришедшее дважды, заводит ровно одно задание.
///
/// Так бывает при двойном нажатии и при повторе запроса: ключ снимка
/// расходуется неделимо, поэтому второе подтверждение обязано получить отказ,
/// а не завести вторую уборку тех же писем. Вторая уборка означала бы вторую
/// пачку операций по тем же письмам и повторный обход сервера.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_confirmations_of_one_snapshot_start_a_single_sweep() {
    let db: TestDb = open_test_db("race-sweep-confirm").await;
    let account = seed_account(&db, "me@example.test").await;
    seed_folder(&db, account, "INBOX", "inbox").await;
    seed_folder(&db, account, "Trash", "trash").await;
    let letters: Vec<DiscoveredMessage> = (1..=3).map(|uid| discovered(uid, "list@example.test")).collect();
    db.save_discovered_messages(account, &letters, false)
        .await
        .expect("синхронизация принесла письма");

    let input = SenderSweepInput {
        address: "list@example.test".into(),
        mode: SWEEP_MODE_ONCE.into(),
        account_id: None,
        days: None,
        sweep_archive: false,
    };
    let preview = db
        .preview_sender_sweep(input.clone())
        .await
        .expect("предпросмотр уборки");
    assert_eq!(
        preview.total,
        letters.len() as i64,
        "предпросмотр насчитал не все письма отправителя"
    );

    // Два подтверждения одного и того же снимка в одно время.
    let (first_db, second_db) = ((*db).clone(), (*db).clone());
    let (first_input, second_input) = (input.clone(), input.clone());
    let (first_key, second_key) = (preview.snapshot_key.clone(), preview.snapshot_key.clone());
    let first = tokio::spawn(async move { first_db.start_sender_sweep(first_input, &first_key).await });
    let second =
        tokio::spawn(async move { second_db.start_sender_sweep(second_input, &second_key).await });
    let (first, second) = tokio::join!(first, second);
    let results = [first.expect("первая задача"), second.expect("вторая задача")];
    let accepted = results.iter().filter(|result| result.is_ok()).count();
    assert_eq!(
        accepted,
        1,
        "подтверждение сработало {accepted} раз: {:?}",
        results
            .iter()
            .map(|result| match result {
                Ok(report) => format!("принято, состояние {}", report.state),
                Err(error) => format!("отказ: {error}"),
            })
            .collect::<Vec<_>>()
    );

    let jobs: (i64,) = sqlx::query_as("SELECT count(*) FROM sender_sweep_jobs")
        .fetch_one(&db.pool)
        .await
        .expect("прочитать задания уборки");
    assert_eq!(jobs.0, 1, "одно подтверждение завело {} заданий уборки", jobs.0);
    assert_eq!(
        takeaway_count(&db).await,
        letters.len() as i64,
        "по письмам встало не по одной операции увода"
    );
    db.close().await;
}

/// Письмо из очереди отправки достаётся ровно одному работнику.
///
/// Два работника ящика приходят за одной готовой операцией. Достанься она
/// обоим - письмо уйдёт адресату дважды, и вернуть его будет нечем.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_send_operation_is_claimed_by_exactly_one_worker() {
    let db: TestDb = open_test_db("race-send-claim").await;
    let account = seed_account(&db, "me@example.test").await;
    seed_folder(&db, account, "INBOX", "inbox").await;

    let letter = OutgoingMessage {
        from: "me@example.test".into(),
        to: vec!["boss@partner.test".into()],
        subject: "Договор".into(),
        body_text: "Текст письма".into(),
        ..Default::default()
    };
    // Происхождение без окна отмены: письмо готово к передаче сразу, и оба
    // работника приходят за ним одновременно.
    let queued = db
        .queue_outgoing_send(account, letter, SEND_ORIGIN_AUTOMATIC, None, 0)
        .await
        .expect("принять письмо в очередь");

    let (first_db, second_db) = ((*db).clone(), (*db).clone());
    let first = tokio::spawn(async move { first_db.claim_send_operation(account).await });
    let second = tokio::spawn(async move { second_db.claim_send_operation(account).await });
    let (first, second) = tokio::join!(first, second);
    let claimed: Vec<i64> = [first.expect("первый работник"), second.expect("второй работник")]
        .into_iter()
        .flat_map(|result| result.expect("захват операции"))
        .map(|operation| operation.id)
        .collect();
    assert_eq!(
        claimed,
        vec![queued.operation_id],
        "письмо досталось не одному работнику: {claimed:?}"
    );

    let status: (String,) = sqlx::query_as("SELECT status FROM outbox_ops WHERE id=?")
        .bind(queued.operation_id)
        .fetch_one(&db.pool)
        .await
        .expect("прочитать состояние операции");
    assert_eq!(status.0, "processing", "захваченная операция обязана быть в передаче");
    db.close().await;
}

/// Два одновременных прохода стадий по одному письму уводят его один раз.
///
/// Проходы запускает и синхронизация, и действия пользователя, и они
/// накладываются. Второй проход обязан увидеть работу первого: иначе правило
/// применяется к письму дважды, а по письму появляется вторая операция увода.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_stage_passes_take_one_message_away_once() {
    let db: TestDb = open_test_db("race-stage-pass").await;
    let account = seed_account(&db, "me@example.test").await;
    seed_folder(&db, account, "INBOX", "inbox").await;
    seed_folder(&db, account, "Trash", "trash").await;

    // Правило заводится до письма: прогресс правила стоит на текущем конце
    // списка писем, и новое письмо достаётся именно проходу стадий.
    let rule = MailRuleInput {
        id: "rule-race".into(),
        name: "Рассылку в корзину".into(),
        account_id: None,
        enabled: true,
        groups: vec![MailRuleGroup {
            logic: "all".into(),
            conditions: vec![MailRuleCondition {
                field: "sender_address".into(),
                op: "equals".into(),
                value: "list@example.test".into(),
                unit: None,
                value2: None,
            }],
        }],
        exceptions: Vec::new(),
        actions: vec![MailRuleAction {
            kind: "trash".into(),
            folder_id: None,
            folder_role: None,
            label_id: None,
        }],
        confirm_key: None,
    };
    db.save_mail_rule(&rule, false, None)
        .await
        .expect("сохранить правило");
    db.save_discovered_messages(account, &[discovered(1, "list@example.test")], false)
        .await
        .expect("синхронизация принесла письмо");

    let (first_db, second_db) = ((*db).clone(), (*db).clone());
    let first = tokio::spawn(async move { first_db.process_sync_batch_stages().await });
    let second = tokio::spawn(async move { second_db.process_sync_batch_stages().await });
    let (first, second) = tokio::join!(first, second);
    let applied = [first.expect("первый проход"), second.expect("второй проход")]
        .into_iter()
        .map(|result| result.expect("проход стадий"))
        .sum::<usize>();
    assert_eq!(
        applied, 1,
        "правило применилось к одному письму {applied} раз"
    );
    assert_eq!(
        takeaway_count(&db).await,
        1,
        "по письму поставлено больше одной операции увода"
    );
    db.close().await;
}
