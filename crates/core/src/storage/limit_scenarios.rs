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
