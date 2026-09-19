//! Сценарные проверки быстрых действий (specs/quick-steps.md).
//!
//! Проверки идут по настоящему пути: настоящая база с применёнными миграциями,
//! настоящая таблица горячих клавиш и настоящее удаление папки и метки, после
//! которого действие цепочки теряет цель.

use super::Db;
use super::repo::test_storage::{TestDb, open_test_db};

/// Имена таблиц взяты из миграции `0051_quick_steps.sql`.
const STEPS: &str = "quick_steps";
const ACTIONS: &str = "quick_step_actions";

async fn table_exists(db: &Db, table: &str) -> bool {
    sqlx::query_as::<_, (i64,)>("SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?")
        .bind(table)
        .fetch_one(&db.pool)
        .await
        .expect("прочитать схему")
        .0
        > 0
}

/// Без таблиц быстрых действий ни один сценарий этого файла не имеет смысла:
/// сказать об этом словами честнее, чем уронить проверку невнятной ошибкой
/// запроса.
async fn require_tables(db: &Db, what_breaks: &str) {
    assert!(
        table_exists(db, STEPS).await && table_exists(db, ACTIONS).await,
        "таблиц быстрых действий нет: {what_breaks}"
    );
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

/// Быстрое действие с одним действием цепочки. Команда сохранения - предмет
/// зелёного этапа, а состояние обязано пересчитываться независимо от того, кто
/// завёл запись.
async fn seed_step(db: &Db, name: &str, sort_order: i64) -> i64 {
    sqlx::query_as::<_, (i64,)>(
        "INSERT INTO quick_steps(name, icon, sort_order, state) VALUES(?, 'bolt', ?, 'ok') RETURNING id",
    )
    .bind(name)
    .bind(sort_order)
    .fetch_one(&db.write_pool)
    .await
    .expect("завести быстрое действие")
    .0
}

async fn seed_action(
    db: &Db,
    step_id: i64,
    sort_order: i64,
    kind: &str,
    folder_id: Option<i64>,
    label_id: Option<i64>,
) {
    sqlx::query(
        "INSERT INTO quick_step_actions(quick_step_id, sort_order, kind, folder_id, label_id)
         VALUES(?, ?, ?, ?, ?)",
    )
    .bind(step_id)
    .bind(sort_order)
    .bind(kind)
    .bind(folder_id)
    .bind(label_id)
    .execute(&db.write_pool)
    .await
    .expect("завести действие цепочки");
}

async fn step_state(db: &Db, step_id: i64) -> String {
    sqlx::query_as::<_, (String,)>("SELECT state FROM quick_steps WHERE id=?")
        .bind(step_id)
        .fetch_one(&db.pool)
        .await
        .expect("прочитать состояние быстрого действия")
        .0
}

/// S-013, S-060: миграция заводит хранение быстрых действий, запрещает в
/// цепочке остановку обработки и держит номер слота в пределах десяти, но ни
/// одной строки слота в таблицу горячих клавиш не добавляет. Готовое
/// сочетание, вставленное миграцией, дало бы дубль с уже назначенным
/// пользователем, после которого любое изменение любой горячей клавиши
/// отвергалось бы проверкой занятости.
#[tokio::test]
async fn migration_0051_adds_quick_step_storage_without_hotkey_rows() {
    let db: TestDb = open_test_db("quick-schema").await;
    let applied: Vec<(i64,)> =
        sqlx::query_as("SELECT version FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&db.pool)
            .await
            .expect("прочитать применённые миграции");
    let versions = applied.iter().map(|row| row.0).collect::<Vec<_>>();
    assert!(
        versions.contains(&51),
        "миграции 0051 нет: собрать цепочку действий одной кнопкой негде"
    );
    require_tables(&db, "быстрое действие сохранить негде").await;

    let slots: (i64,) = sqlx::query_as(
        "SELECT count(*) FROM keybindings WHERE action LIKE 'quick\\_step\\_%' ESCAPE '\\'",
    )
    .fetch_one(&db.pool)
    .await
    .expect("прочитать горячие клавиши");
    assert_eq!(
        slots.0, 0,
        "миграция завела строки слотов: дубль сочетания запретил бы менять любую горячую клавишу"
    );

    let account = seed_account(&db, "schema@example.test").await;
    let folder = seed_folder(&db, account, "Работа", None).await;
    let step = seed_step(&db, "В работу", 0).await;
    seed_action(&db, step, 0, "move", Some(folder), None).await;

    // Остановка обработки быстрым действиям запрещена: останавливать в цепочке
    // нечего, прогона правил при нажатии кнопки нет.
    let stop = sqlx::query(
        "INSERT INTO quick_step_actions(quick_step_id, sort_order, kind) VALUES(?, 1, 'stop')",
    )
    .bind(step)
    .execute(&db.write_pool)
    .await;
    assert!(
        stop.is_err(),
        "схема приняла действие остановки обработки в цепочке быстрого действия"
    );

    // Номер слота ограничен десятью и принадлежит одному быстрому действию.
    let eleventh = sqlx::query("UPDATE quick_steps SET hotkey_slot=11 WHERE id=?")
        .bind(step)
        .execute(&db.write_pool)
        .await;
    assert!(
        eleventh.is_err(),
        "схема приняла одиннадцатый слот горячей клавиши"
    );
    sqlx::query("UPDATE quick_steps SET hotkey_slot=1 WHERE id=?")
        .bind(step)
        .execute(&db.write_pool)
        .await
        .expect("первый слот");
    let other = seed_step(&db, "В архив", 1).await;
    let taken = sqlx::query("UPDATE quick_steps SET hotkey_slot=1 WHERE id=?")
        .bind(other)
        .execute(&db.write_pool)
        .await;
    assert!(
        taken.is_err(),
        "один слот достался двум быстрым действиям: одно нажатие вызвало бы два действия"
    );
    db.close().await;
}

/// S-058, S-059, S-061: сочетание назначается слоту тем же путём, что и
/// встроенным действиям клавиатуры, и строка слота заводится при первом
/// назначении. Сейчас хранилище меняет сочетание только у существующей строки,
/// поэтому назначить слоту сочетание нечем и все десять слотов молча не
/// работают.
#[tokio::test]
async fn assigning_a_slot_combo_creates_its_hotkey_row() {
    let db: TestDb = open_test_db("quick-hotkeys").await;
    db.set_keybinding("quick_step_1", "Ctrl+Shift+1")
        .await
        .expect("назначить сочетание первому слоту");

    let rows: Vec<(String, String, String)> =
        sqlx::query_as("SELECT action, scope, combo FROM keybindings WHERE action='quick_step_1'")
            .fetch_all(&db.pool)
            .await
            .expect("прочитать строку слота");
    assert_eq!(rows.len(), 1, "строка слота заведена ровно одна");
    assert_eq!(rows[0].1, "local", "слот живёт в области local");
    assert_eq!(rows[0].2, "Ctrl+Shift+1");

    // Повторное назначение меняет сочетание, а не заводит вторую строку.
    db.set_keybinding("quick_step_1", "Ctrl+Shift+2")
        .await
        .expect("переназначить сочетание слота");
    let again: (i64,) =
        sqlx::query_as("SELECT count(*) FROM keybindings WHERE action='quick_step_1'")
            .fetch_one(&db.pool)
            .await
            .expect("пересчитать строки слота");
    assert_eq!(again.0, 1, "переназначение завело вторую строку слота");

    // Десятый слот тоже известен, а одиннадцатого не существует: перечень имён
    // действий клавиатуры остаётся закрытым.
    db.set_keybinding("quick_step_10", "Ctrl+Shift+0")
        .await
        .expect("назначить сочетание десятому слоту");
    assert!(
        db.set_keybinding("quick_step_11", "Ctrl+Shift+Q")
            .await
            .is_err(),
        "принято имя действия клавиатуры вне закрытого перечня"
    );
    db.close().await;
}

/// S-043, S-045, S-047, S-078: удаление папки и удаление метки обнуляют ссылку
/// действия цепочки и переводят быстрое действие в состояние
/// `needs_attention` в той же неделимой операции. Иначе нажатие кнопки
/// выполнило бы половину цепочки и унесло письмо неизвестно куда.
#[tokio::test]
async fn losing_a_target_marks_the_quick_step_as_needing_attention() {
    let db: TestDb = open_test_db("quick-attention").await;
    let account = seed_account(&db, "attention@example.test").await;
    let folder = seed_folder(&db, account, "Работа", None).await;
    let label = db
        .create_label("Разобрать", "#00ff00")
        .await
        .expect("создать метку");
    require_tables(&db, "потерявшую цель цепочку отметить негде").await;

    let by_folder = seed_step(&db, "В работу", 0).await;
    seed_action(&db, by_folder, 0, "move", Some(folder), None).await;
    let by_label = seed_step(&db, "Пометить", 1).await;
    seed_action(&db, by_label, 0, "label_add", None, Some(label)).await;

    db.delete_folder_local(folder).await.expect("удалить папку");
    let folder_ref: (Option<i64>,) =
        sqlx::query_as("SELECT folder_id FROM quick_step_actions WHERE quick_step_id=?")
            .bind(by_folder)
            .fetch_one(&db.pool)
            .await
            .expect("прочитать ссылку на папку");
    assert_eq!(
        folder_ref.0, None,
        "ссылка на удалённую папку осталась в цепочке"
    );
    assert_eq!(
        step_state(&db, by_folder).await,
        "needs_attention",
        "быстрое действие без папки обязано отказывать в запуске, а не выполнять половину цепочки"
    );

    db.delete_label(label).await.expect("удалить метку");
    let label_ref: (Option<i64>,) =
        sqlx::query_as("SELECT label_id FROM quick_step_actions WHERE quick_step_id=?")
            .bind(by_label)
            .fetch_one(&db.pool)
            .await
            .expect("прочитать ссылку на метку");
    assert_eq!(
        label_ref.0, None,
        "ссылка на удалённую метку осталась в цепочке"
    );
    assert_eq!(
        step_state(&db, by_label).await,
        "needs_attention",
        "быстрое действие без метки обязано отказывать в запуске"
    );
    db.close().await;
}
