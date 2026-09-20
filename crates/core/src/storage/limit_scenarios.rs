//! Сценарные проверки настраиваемых пределов (crates/core/src/model/limits.rs).
//!
//! Проверки идут по настоящему пути: настоящая база с применёнными
//! миграциями, настоящая запись настройки и те же функции, которыми
//! пользуется программа. Проверяется не каждое из трёх десятков значений - это
//! была бы свалка, - а то, что может сломаться: значение читается из настроек,
//! а не из числа в коде; значение вне границ отклоняется с объяснением; ядро и
//! интерфейс берут пределы из одного места; обновление непустой базы не меняет
//! поведения.

use super::Db;
use super::recipient_history::{RecipientTouch, TouchOrigin};
use super::repo::test_storage::{TestDb, open_test_db, open_test_db_upto};
use crate::model::*;

/// Последняя миграция до появления раздела пределов.
const BEFORE_LIMITS: i64 = 52;

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
    seed_named_folder(db, account_id, "INBOX", "inbox").await
}

async fn seed_named_folder(db: &Db, account_id: i64, path: &str, role: &str) -> i64 {
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

/// Письмо от названного отправителя с названной датой: стадии смотрят на них
/// обоих.
async fn seed_message_from(
    db: &Db,
    account_id: i64,
    folder_id: i64,
    uid: i64,
    from: &str,
    date: &str,
) -> i64 {
    sqlx::query_as::<_, (i64,)>(
        "INSERT INTO messages(account_id, folder_id, uid, from_addr, subject, preview, date,
                              rfc822_message_id, remote_id, size)
         VALUES(?, ?, ?, ?, 'письмо', '', ?, ?, ?, 2048)
         RETURNING id",
    )
    .bind(account_id)
    .bind(folder_id)
    .bind(uid)
    .bind(from)
    .bind(date)
    .bind(format!("<m{folder_id}-{uid}@example.test>"))
    .bind(format!("remote-{folder_id}-{uid}"))
    .fetch_one(&db.write_pool)
    .await
    .expect("сохранить письмо")
    .0
}

/// Время на столько дней назад в том же виде, в каком его пишет
/// синхронизация.
fn days_ago(days: i64) -> String {
    (chrono::Utc::now() - chrono::Duration::days(days))
        .format("%Y-%m-%dT%H:%M:%S+00:00")
        .to_string()
}

/// Сколько операций увода стоит в очереди по письму.
async fn takeaways(db: &Db, message_id: i64) -> i64 {
    sqlx::query_as::<_, (i64,)>(
        "SELECT count(*) FROM outbox_ops WHERE message_id=? AND op_kind IN ('move','delete')",
    )
    .bind(message_id)
    .fetch_one(&db.pool)
    .await
    .expect("прочитать очередь")
    .0
}

async fn seed_messages(db: &Db, account_id: i64, folder_id: i64, count: i64) -> Vec<i64> {
    let mut ids = Vec::new();
    for index in 0..count {
        let id = sqlx::query_as::<_, (i64,)>(
            "INSERT INTO messages(account_id, folder_id, uid, from_addr, subject, preview, date,
                                  rfc822_message_id, remote_id, size)
             VALUES(?, ?, ?, 'boss@example.test', 'письмо', '', datetime('now'), ?, ?, 100)
             RETURNING id",
        )
        .bind(account_id)
        .bind(folder_id)
        .bind(index + 1)
        .bind(format!("<m{index}@example.test>"))
        .bind(format!("remote-{index}"))
        .fetch_one(&db.write_pool)
        .await
        .expect("сохранить письмо")
        .0;
        ids.push(id);
    }
    ids
}

/// Предел закреплений читается из настроек, а не из числа в коде.
///
/// Раньше это число стояло в ядре и вторым числом в интерфейсе: список
/// показывал место под пятьдесят писем, а закрепить ядро давало двадцать.
/// Проверка ставит заведомо другое значение и ведёт закрепление настоящим
/// путём - через ту же команду, которой пользуется список.
#[tokio::test]
async fn a_changed_limit_takes_effect_on_the_real_path() {
    let db: TestDb = open_test_db("limits-pinned").await;
    let account = seed_account(&db, "me@example.test").await;
    let folder = seed_folder(&db, account).await;
    let ids = seed_messages(&db, account, folder, 5).await;

    db.set_limit(LIMIT_PINNED_PER_ACCOUNT, 3)
        .await
        .expect("записать предел закреплений");
    let result = db
        .set_messages_pinned(&ids, true)
        .await
        .expect("закрепить письма");
    assert_eq!(
        result.changed, 3,
        "закреплено не столько, сколько разрешено"
    );
    assert_eq!(
        result.rejected_limit, 2,
        "письма сверх предела приняты молча"
    );

    // Предел поднят - те же письма закрепляются дальше, и значение берётся из
    // настройки, а не из прежнего числа.
    db.set_limit(LIMIT_PINNED_PER_ACCOUNT, 5)
        .await
        .expect("поднять предел закреплений");
    let more = db
        .set_messages_pinned(&ids, true)
        .await
        .expect("закрепить остальные письма");
    assert_eq!(more.rejected_limit, 0, "поднятый предел не подействовал");
    let (pinned,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM messages WHERE account_id=? AND pinned_at IS NOT NULL",
    )
    .bind(account)
    .fetch_one(&db.pool)
    .await
    .expect("посчитать закреплённые");
    assert_eq!(pinned, 5);
    db.close().await;
}

/// Срок хранения выполненных дел тоже настройка: прежде он был вписан прямо в
/// текст запроса, и дело исчезало ровно через месяц у всех.
#[tokio::test]
async fn a_completed_task_lives_as_long_as_the_setting_says() {
    let db: TestDb = open_test_db("limits-tasks").await;
    let account = seed_account(&db, "me@example.test").await;
    let folder = seed_folder(&db, account).await;
    let ids = seed_messages(&db, account, folder, 2).await;
    for id in &ids {
        sqlx::query(
            "INSERT INTO message_tasks(message_id, state, completed_at)
             VALUES(?, 'done', datetime('now','-10 days'))",
        )
        .bind(id)
        .execute(&db.write_pool)
        .await
        .expect("завести выполненное дело");
    }

    db.set_limit(LIMIT_DONE_TASK_DAYS, 30)
        .await
        .expect("записать срок хранения");
    assert_eq!(
        db.purge_completed_message_tasks()
            .await
            .expect("уборка дел"),
        0,
        "дело убрано раньше своего срока"
    );

    db.set_limit(LIMIT_DONE_TASK_DAYS, 7)
        .await
        .expect("сократить срок хранения");
    assert_eq!(
        db.purge_completed_message_tasks()
            .await
            .expect("уборка дел"),
        2,
        "сокращённый срок не подействовал: срок читается не из настроек"
    );
    db.close().await;
}

/// Значение вне границ отклоняется с объяснением, а не молча поправляется или
/// записывается. Молча поправленное значение пользователь принял бы за
/// принятое, а записанное сверх границ сломало бы то, что предел охраняет.
#[tokio::test]
async fn a_value_outside_its_bounds_is_rejected_with_an_explanation() {
    let db: TestDb = open_test_db("limits-bounds").await;
    let spec = limit_spec(LIMIT_PINNED_PER_ACCOUNT).expect("описание предела");
    let before = db.limit(LIMIT_PINNED_PER_ACCOUNT);

    let error = db
        .set_limit(LIMIT_PINNED_PER_ACCOUNT, spec.max + 1)
        .await
        .expect_err("значение сверх верхней границы принято");
    let text = error.to_string();
    assert!(
        text.contains(&spec.max.to_string()),
        "отказ не называет границу: {text}"
    );
    assert!(
        text.contains("Сколько писем можно закрепить"),
        "отказ назван именем ключа, а не человеческой подписью: {text}"
    );
    assert!(
        db.set_limit(LIMIT_PINNED_PER_ACCOUNT, spec.min - 1)
            .await
            .is_err(),
        "значение ниже нижней границы принято"
    );
    assert!(
        db.set_limit("limit_несуществующий", 1).await.is_err(),
        "записана настройка, которой нет в перечне"
    );
    assert_eq!(
        db.limit(LIMIT_PINNED_PER_ACCOUNT),
        before,
        "отклонённое значение всё-таки изменило рабочий предел"
    );
    assert_eq!(
        db.setting(LIMIT_PINNED_PER_ACCOUNT)
            .await
            .expect("прочитать настройку"),
        Some(before.to_string()),
        "отклонённое значение записано в базу"
    );

    // Границы - сами настройки, и они тоже проверяются своими границами.
    assert!(
        db.set_limit(LIMIT_UNDO_SEND_MAX, 0).await.is_err(),
        "верхняя граница окна отмены принята нулевой: окно отмены выключилось бы совсем"
    );
    db.close().await;
}

/// Обновление непустой базы не меняет поведения: у того, кто обновился,
/// пределы остаются прежними зашитыми числами, а значение, уже выбранное
/// пользователем, миграция не перетирает.
#[tokio::test]
async fn migrating_a_filled_database_keeps_the_previous_behaviour() {
    let db: TestDb = open_test_db_upto("limits-migrate", BEFORE_LIMITS).await;
    let account = seed_account(&db, "me@example.test").await;
    let folder = seed_folder(&db, account).await;
    seed_messages(&db, account, folder, 3).await;
    // Настройка, выбранная пользователем до обновления. Значения в базе
    // прежней версии лежат открытым текстом - их шифрует шаг миграции.
    sqlx::query("INSERT INTO settings(key, value) VALUES(?, '7')")
        .bind(LIMIT_PINNED_PER_ACCOUNT)
        .execute(&db.write_pool)
        .await
        .expect("записать выбранное пользователем значение");

    db.migrate().await.expect("применить оставшиеся миграции");

    assert_eq!(
        db.limit(LIMIT_PINNED_PER_ACCOUNT),
        7,
        "миграция перетёрла значение, выбранное пользователем"
    );
    // Все остальные пределы равны прежним зашитым числам: поведение у того,
    // кто обновился, не меняется.
    for spec in LIMITS {
        if spec.key == LIMIT_PINNED_PER_ACCOUNT {
            continue;
        }
        assert_eq!(
            db.limit(spec.key),
            spec.default,
            "предел {} после обновления отличается от прежнего зашитого",
            spec.key
        );
        let stored = db
            .setting(spec.key)
            .await
            .expect("прочитать настройку")
            .expect("миграция не завела настройку");
        assert_eq!(stored, spec.default.to_string());
    }
    // Письма на месте: миграция трогает только таблицу настроек.
    let (messages,): (i64,) = sqlx::query_as("SELECT count(*) FROM messages")
        .fetch_one(&db.pool)
        .await
        .expect("посчитать письма");
    assert_eq!(messages, 3);
    db.close().await;
}

/// Один проход стадии разбирает столько писем, сколько разрешает настройка.
///
/// Прежде размер пачки был числом в коде, общим на все стадии: на ящике, где
/// каждая пачка упирается в медленный сервер, уменьшить его было нельзя.
/// Проверка ведёт настоящий путь - список отправителей и конвейер стадий
/// синхронизации.
#[tokio::test]
async fn a_stage_pass_takes_as_many_messages_as_the_setting_allows() {
    let db: TestDb = open_test_db("limits-stage-batch").await;
    let account = seed_account(&db, "me@example.test").await;
    let inbox = seed_folder(&db, account).await;
    seed_named_folder(&db, account, "Trash", "trash").await;
    let mut ids = Vec::new();
    for uid in 1..=12 {
        ids.push(
            seed_message_from(
                &db,
                account,
                inbox,
                uid,
                "news@example.test",
                &days_ago(uid),
            )
            .await,
        );
    }
    db.save_sender_policy(
        POLICY_KIND_ADDRESS,
        "news@example.test",
        POLICY_DECISION_BLOCKED,
        false,
    )
    .await
    .expect("заблокировать отправителя");

    db.set_limit(LIMIT_STAGE_BATCH, 10)
        .await
        .expect("записать размер пачки");
    db.process_sync_batch_stages().await.expect("проход стадий");
    let mut queued = 0;
    for id in &ids {
        queued += takeaways(&db, *id).await;
    }
    assert_eq!(
        queued, 10,
        "проход взял не столько писем, сколько разрешает настройка"
    );

    // Следующий проход продолжает с курсора: оставшиеся письма не теряются, а
    // поднятая настройка забирает их разом.
    db.set_limit(LIMIT_STAGE_BATCH, 100)
        .await
        .expect("поднять размер пачки");
    db.process_sync_batch_stages().await.expect("второй проход");
    let mut total = 0;
    for id in &ids {
        total += takeaways(&db, *id).await;
    }
    assert_eq!(
        total, 12,
        "оставшиеся письма не разобраны следующим проходом"
    );
    db.close().await;
}

/// Состарить снимок кандидатов: подтверждение открывают и возвращаются к нему
/// спустя часы, а ждать их в проверке нельзя.
async fn age_snapshot(db: &Db, key: &str, hours: i64) {
    sqlx::query("UPDATE stage_snapshots SET created_at=datetime('now', ?) WHERE key=?")
        .bind(format!("-{hours} hours"))
        .bind(key)
        .execute(&db.write_pool)
        .await
        .expect("состарить снимок");
}

/// Список писем на уборку ждёт подтверждения столько, сколько говорит
/// настройка.
///
/// Прежде срок был вписан в оба запроса уборки сутками. Кто открывает
/// подтверждение вечером и возвращается к нему через день, получал отказ и
/// начинал сначала; кому суток много, тот не мог их сократить.
#[tokio::test]
async fn an_unconfirmed_list_of_messages_lives_as_long_as_the_setting_says() {
    let db: TestDb = open_test_db("limits-snapshot").await;
    let account = seed_account(&db, "me@example.test").await;
    let inbox = seed_folder(&db, account).await;
    seed_named_folder(&db, account, "Trash", "trash").await;
    seed_message_from(&db, account, inbox, 1, "shop@example.test", &days_ago(40)).await;
    seed_message_from(&db, account, inbox, 2, "shop@example.test", &days_ago(1)).await;
    let input = SenderSweepInput {
        address: "shop@example.test".into(),
        mode: SWEEP_MODE_OLDER_THAN.into(),
        account_id: Some(account),
        days: Some(30),
        sweep_archive: false,
    };

    // Снимок, которому тридцать часов, при сроке двое суток ещё жив.
    let preview = db
        .preview_sender_sweep(input.clone())
        .await
        .expect("предпросмотр уборки");
    db.set_limit(LIMIT_STAGE_SNAPSHOT_HOURS, 48)
        .await
        .expect("записать срок жизни снимка");
    age_snapshot(&db, &preview.snapshot_key, 30).await;
    db.purge_stale_stage_snapshots()
        .await
        .expect("уборка снимков");
    let report = db
        .start_sender_sweep(input.clone(), &preview.snapshot_key)
        .await
        .expect("подтверждение снимка, который ещё не устарел");
    assert_eq!(
        report.queued, 1,
        "подтверждённый снимок не убрал письмо, которое показывал"
    );

    // Тот же возраст при сроке в час снимок хоронит, и отказ объясняется.
    seed_message_from(&db, account, inbox, 3, "news@example.test", &days_ago(40)).await;
    let second = SenderSweepInput {
        address: "news@example.test".into(),
        ..input
    };
    let stale = db
        .preview_sender_sweep(second.clone())
        .await
        .expect("предпросмотр второй уборки");
    db.set_limit(LIMIT_STAGE_SNAPSHOT_HOURS, 1)
        .await
        .expect("сократить срок жизни снимка");
    age_snapshot(&db, &stale.snapshot_key, 30).await;
    db.purge_stale_stage_snapshots()
        .await
        .expect("уборка снимков");
    let refused = db
        .start_sender_sweep(second, &stale.snapshot_key)
        .await
        .expect_err("устаревший снимок принят: срок читается не из настроек");
    assert!(
        refused.to_string().contains("устарел"),
        "отказ не объясняет, что список устарел: {refused}"
    );
    db.close().await;
}

/// Полный проход автоочистки наступает по сроку из настроек.
///
/// Прежде срок был сутками в коде: у того, кто получает от отправителя письма
/// каждый час, запись догоняла их только назавтра.
#[tokio::test]
async fn a_full_sweep_pass_comes_due_by_the_setting() {
    let db: TestDb = open_test_db("limits-sweep-pass").await;
    let account = seed_account(&db, "me@example.test").await;
    let inbox = seed_folder(&db, account).await;
    let trash = seed_named_folder(&db, account, "Trash", "trash").await;
    let old = seed_message_from(&db, account, inbox, 1, "ads@example.test", &days_ago(40)).await;
    seed_message_from(&db, account, inbox, 2, "ads@example.test", &days_ago(1)).await;
    // Запись заведена прошлым запуском, и полный проход по ней был двенадцать
    // часов назад.
    sqlx::query(
        "INSERT INTO sender_sweep_rules(address, account_id, mode, days, last_full_pass_at)
         VALUES('ads@example.test', ?, 'older_than', 30, datetime('now','-12 hours'))",
    )
    .bind(account)
    .execute(&db.write_pool)
    .await
    .expect("запись автоочистки");

    db.set_limit(LIMIT_SWEEP_FULL_PASS_HOURS, 24)
        .await
        .expect("записать срок полного прохода");
    db.advance_sender_sweep_jobs()
        .await
        .expect("продвинуть задания");
    assert_eq!(
        takeaways(&db, old).await,
        0,
        "проход начался раньше своего срока"
    );

    db.set_limit(LIMIT_SWEEP_FULL_PASS_HOURS, 6)
        .await
        .expect("сократить срок полного прохода");
    db.advance_sender_sweep_jobs()
        .await
        .expect("продвинуть задания");
    assert_eq!(
        takeaways(&db, old).await,
        1,
        "сокращённый срок не подействовал: срок читается не из настроек"
    );
    let (target,): (Option<i64>,) = sqlx::query_as(
        "SELECT CAST(json_extract(payload,'$.target_folder_id') AS INTEGER)
           FROM outbox_ops WHERE message_id=?",
    )
    .bind(old)
    .fetch_one(&db.pool)
    .await
    .expect("прочитать очередь");
    assert_eq!(target, Some(trash), "письмо ушло не в корзину");
    db.close().await;
}

/// Занятое письмо заставляет автоочистку ждать ровно столько раз и столько
/// времени, сколько сказано в настройках.
///
/// Прежде оба числа были в коде: на медленном сервере чужая операция не
/// успевала за минуту, и проход сдавался восьмой раз подряд, ничего не убрав.
#[tokio::test]
async fn a_busy_message_makes_the_sweep_wait_as_the_settings_say() {
    let db: TestDb = open_test_db("limits-sweep-wait").await;
    let account = seed_account(&db, "me@example.test").await;
    let inbox = seed_folder(&db, account).await;
    seed_named_folder(&db, account, "Trash", "trash").await;
    let busy = seed_message_from(&db, account, inbox, 1, "ads@example.test", &days_ago(40)).await;
    seed_message_from(&db, account, inbox, 2, "ads@example.test", &days_ago(1)).await;
    // Чужая незавершённая операция по тому же письму: проход обязан отложить
    // его, а не увести второй раз.
    sqlx::query(
        "INSERT INTO outbox_ops(account_id, message_id, op_kind, payload, status)
         VALUES(?, ?, 'move', '{}', 'pending')",
    )
    .bind(account)
    .bind(busy)
    .execute(&db.write_pool)
    .await
    .expect("чужая операция");

    db.set_limit(LIMIT_SWEEP_WAIT_SECONDS, 3600)
        .await
        .expect("записать паузу ожидания");
    let input = SenderSweepInput {
        address: "ads@example.test".into(),
        mode: SWEEP_MODE_OLDER_THAN.into(),
        account_id: Some(account),
        days: Some(30),
        sweep_archive: false,
    };
    let preview = db
        .preview_sender_sweep(input.clone())
        .await
        .expect("предпросмотр уборки");
    db.start_sender_sweep(input, &preview.snapshot_key)
        .await
        .expect("уборка");
    let (state, waits, wait_minutes): (String, i64, f64) = sqlx::query_as(
        "SELECT state, waits,
                (julianday(next_check_at) - julianday('now')) * 24 * 60
           FROM sender_sweep_jobs ORDER BY id DESC LIMIT 1",
    )
    .fetch_one(&db.pool)
    .await
    .expect("прочитать задание");
    assert_eq!(state, SWEEP_JOB_WAITING, "задание не стало ждать");
    assert_eq!(waits, 1);
    assert!(
        (55.0..=60.0).contains(&wait_minutes),
        "пауза назначена не из настройки, а прежним числом: {wait_minutes} мин"
    );

    // Последнее разрешённое ожидание закрывает задание неудачей: письмо,
    // занятое навсегда, не должно держать проход вечно.
    db.set_limit(LIMIT_SWEEP_MAX_WAITS, 2)
        .await
        .expect("записать число ожиданий");
    sqlx::query("UPDATE sender_sweep_jobs SET next_check_at=datetime('now','-1 minute')")
        .execute(&db.write_pool)
        .await
        .expect("подвинуть срок проверки");
    db.advance_sender_sweep_jobs()
        .await
        .expect("продвинуть задания");
    let (state, waits): (String, i64) =
        sqlx::query_as("SELECT state, waits FROM sender_sweep_jobs ORDER BY id DESC LIMIT 1")
            .fetch_one(&db.pool)
            .await
            .expect("прочитать задание");
    assert_eq!(waits, 2);
    assert_eq!(
        state, SWEEP_JOB_FAILED,
        "задание ждёт дольше разрешённого: число ожиданий читается не из настроек"
    );
    db.close().await;
}

/// Место адресата в подсказке считается по порогам свежести из настроек.
///
/// Прежде пороги были вписаны в выражение запроса. Кто пишет одним и тем же
/// людям ежедневно, не мог отделить частых адресатов от разовых, а кто пишет
/// раз в квартал - наоборот.
#[tokio::test]
async fn the_rank_of_a_recipient_follows_the_freshness_thresholds() {
    let db: TestDb = open_test_db("limits-rank").await;
    let account = seed_account(&db, "me@example.test").await;
    db.record_recipient_touches(
        account,
        &[RecipientTouch {
            email: "client@partner.test".into(),
            name: "Клиент".into(),
            message_key: "<rank@example.test>".into(),
            used_at: days_ago(45),
        }],
        TouchOrigin::OwnSend,
    )
    .await
    .expect("записать обращение");

    assert_eq!(
        recipient_rank(&db, account).await,
        2,
        "обращение сорокапятидневной давности весит не как недавнее"
    );

    db.set_limit(LIMIT_RANK_FRESH_DAYS, 60)
        .await
        .expect("расширить порог свежести");
    assert_eq!(
        recipient_rank(&db, account).await,
        3,
        "расширенный порог свежести не подействовал: пороги читаются не из настроек"
    );

    // Сдвинутые пороги выводят давнее обращение из счёта совсем.
    db.set_limit(LIMIT_RANK_FRESH_DAYS, 1)
        .await
        .expect("сузить порог свежести");
    db.set_limit(LIMIT_RANK_RECENT_DAYS, 2)
        .await
        .expect("сузить порог недавнего");
    db.set_limit(LIMIT_RANK_OLD_DAYS, 10)
        .await
        .expect("сузить порог давнего");
    assert_eq!(
        recipient_rank(&db, account).await,
        0,
        "обращение старше последнего порога всё ещё влияет на место адресата"
    );
    db.close().await;
}

/// Место адресата в подсказке тем же путём, которым его берёт композер.
async fn recipient_rank(db: &Db, account_id: i64) -> i64 {
    db.recipient_candidates(account_id)
        .await
        .expect("кандидаты подсказки")
        .into_iter()
        .find(|candidate| candidate.email == "client@partner.test")
        .expect("адресат пропал из подсказки")
        .rank
}

/// Раздел управления историей отдаёт страницу того размера, который назначен
/// настройкой.
///
/// Прежде сотня была потолком в запросе и вторым числом в интерфейсе: кто
/// просил больше, получал сто записей молча.
#[tokio::test]
async fn the_history_section_page_comes_from_the_settings() {
    let db: TestDb = open_test_db("limits-history-page").await;
    let account = seed_account(&db, "me@example.test").await;
    let touches: Vec<RecipientTouch> = (1..=12)
        .map(|index| RecipientTouch {
            email: format!("client{index}@partner.test"),
            name: format!("Клиент {index}"),
            message_key: format!("<page{index}@example.test>"),
            used_at: days_ago(index),
        })
        .collect();
    db.record_recipient_touches(account, &touches, TouchOrigin::OwnSend)
        .await
        .expect("записать обращения");

    db.set_limit(LIMIT_HISTORY_PAGE, 10)
        .await
        .expect("записать размер страницы");
    assert_eq!(
        db.list_recipient_history(account, 12, 0)
            .await
            .expect("страница истории")
            .len(),
        10,
        "страница отдала больше, чем разрешает настройка"
    );

    db.set_limit(LIMIT_HISTORY_PAGE, 20)
        .await
        .expect("поднять размер страницы");
    assert_eq!(
        db.list_recipient_history(account, 12, 0)
            .await
            .expect("страница истории")
            .len(),
        12,
        "поднятый размер страницы не подействовал"
    );
    db.close().await;
}

/// Перечень для интерфейса несёт всё, чем интерфейс рисует поле: рабочее
/// значение, обе границы, раздел и ключи подписей. Недостающее поле заставило
/// бы интерфейс подставить своё число - ровно ту копию, из-за которой пределы
/// и разошлись.
#[tokio::test]
async fn the_limit_list_carries_everything_the_interface_draws() {
    let db: TestDb = open_test_db("limits-view").await;
    db.set_limit(LIMIT_PINNED_PER_ACCOUNT, 9)
        .await
        .expect("записать предел");
    let view = serde_json::to_value(db.limit_settings()).expect("собрать перечень");
    let rows = view.as_array().expect("перечень не массив");
    assert_eq!(rows.len(), LIMITS.len());
    let pinned = rows
        .iter()
        .find(|row| row["key"] == LIMIT_PINNED_PER_ACCOUNT)
        .expect("предела закреплений нет в перечне");
    assert_eq!(pinned["value"], 9, "перечень отдаёт не рабочее значение");
    for field in [
        "section",
        "title_key",
        "hint_key",
        "unit_key",
        "default",
        "min",
        "max",
    ] {
        assert!(
            !pinned[field].is_null(),
            "в перечне нет поля {field}: интерфейс подставит своё"
        );
    }
    db.close().await;
}

/// У предела один источник: интерфейс обращается к ядру по тем же именам
/// ключей и своих чисел не держит.
///
/// Проверка читает сам модуль интерфейса. Пока копии жили порознь, их
/// расхождение замечал только пользователь: ядро отказывало, а интерфейс
/// обещал.
#[test]
fn the_user_interface_takes_every_limit_from_the_core() {
    const LIMITS_JS: &str = include_str!("../../../../apps/desktop/ui/modules/limits.js");

    for spec in LIMITS {
        assert!(
            LIMITS_JS.contains(&format!("'{}'", spec.key)),
            "интерфейс не знает предел {}: он возьмёт своё число",
            spec.key
        );
    }
    // Обратная сторона: имя ключа, которого нет в ядре, означает настройку,
    // которую никто не читает.
    for piece in LIMITS_JS.split("'limit_").skip(1) {
        let key = format!("limit_{}", piece.split('\'').next().unwrap_or_default());
        assert!(
            limit_spec(&key).is_some(),
            "интерфейс ссылается на предел {key}, которого нет в ядре"
        );
    }

    // Модули интерфейса берут числа из этого перечня, а не из собственных
    // констант: имена прежних копий не должны вернуться.
    for (module, gone) in [
        ("pin-message.js", "PINNED_VISIBLE_LIMIT"),
        ("mail-rules.js", "RULE_MAX_GROUPS"),
        ("quick-steps.js", "MAX_MESSAGES"),
        ("outbox.js", "UNDO_MAX_SECONDS"),
        ("out-of-office.js", "OOF_MAX_TEXT_CHARS"),
        ("recipient-history.js", "HISTORY_SUGGESTION_LIMIT"),
        ("shell.js", "BACKFILL_PAGE_SIZE"),
    ] {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../apps/desktop/ui/modules")
                .join(module),
        )
        .expect("прочитать модуль интерфейса");
        assert!(
            !source.contains(gone),
            "в {module} вернулась собственная копия предела {gone}"
        );
    }
}

/// Пределы, у которых сценария нет: они действуют только на настоящем сервере
/// (сколько писем тянет фоновая догрузка тел) или в фоновом цикле, живущем вне
/// хранилища (ритм опроса, напоминаний и проверки обновлений).
///
/// Проверка читает сами исходники: прежние числа были именованными
/// константами, и вернуться незамеченными они могут только так.
#[test]
fn the_places_without_a_scenario_keep_no_number_of_their_own() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for (file, gone, key) in [
        (
            "crates/core/src/account/mod.rs",
            "BODY_PREFETCH_PER_PASS",
            LIMIT_BODY_PREFETCH_MESSAGES,
        ),
        (
            "crates/core/src/account/mod.rs",
            "BODY_PREFETCH_MAX_SIZE",
            LIMIT_BODY_PREFETCH_SIZE_MB,
        ),
        (
            "crates/core/src/storage/stages.rs",
            "STAGE_BATCH",
            LIMIT_STAGE_BATCH,
        ),
        (
            "crates/core/src/model/sender_sweep.rs",
            "SWEEP_WAIT_SECONDS: i64",
            LIMIT_SWEEP_WAIT_SECONDS,
        ),
        (
            "crates/core/src/model/recipient_history.rs",
            "HISTORY_PAGE: i64",
            LIMIT_HISTORY_PAGE,
        ),
    ] {
        let source = std::fs::read_to_string(root.join(file)).expect("прочитать исходник ядра");
        assert!(
            !source.contains(gone),
            "в {file} вернулась собственная копия предела {gone} (настройка {key})"
        );
    }

    // Фоновые циклы спрашивают настройку на каждом обороте: иначе выбранный
    // пользователем срок ждал бы перезапуска программы.
    let loops = std::fs::read_to_string(root.join("apps/desktop/src-tauri/src/commands.rs"))
        .expect("команды");
    for key in [LIMIT_GMAIL_POLL_SECONDS, LIMIT_REMINDER_CHECK_SECONDS] {
        let name = key.to_uppercase();
        assert!(
            loops.contains(&name),
            "фоновый цикл не спрашивает настройку {key}"
        );
    }
    let start =
        std::fs::read_to_string(root.join("apps/desktop/src-tauri/src/main.rs")).expect("запуск");
    assert!(
        start.contains(&LIMIT_UPDATE_CHECK_HOURS.to_uppercase()),
        "проверка обновлений идёт по числу в коде, а не по настройке"
    );
}
