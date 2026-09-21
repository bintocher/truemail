//! Сценарные проверки быстрых действий (specs/quick-steps.md).
//!
//! Проверки идут по настоящему пути: настоящая база с применёнными миграциями,
//! настоящая таблица горячих клавиш и настоящее удаление папки и метки, после
//! которого действие цепочки теряет цель.

use super::Db;
use super::repo::test_storage::{TestDb, open_test_db};

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

/// Письмо в базе. Полей ровно столько, сколько нужно снимку письма, по
/// которому идёт цепочка быстрого действия.
async fn seed_message(db: &Db, account_id: i64, folder_id: i64, uid: i64) -> i64 {
    sqlx::query_as::<_, (i64,)>(
        "INSERT INTO messages(account_id, folder_id, uid, from_addr, from_name, subject, preview,
                              date, rfc822_message_id, remote_id, size, seen)
         VALUES(?, ?, ?, 'boss@example.test', 'Начальник', 'Договор', '', datetime('now'), ?, ?, 100, 0)
         RETURNING id",
    )
    .bind(account_id)
    .bind(folder_id)
    .bind(uid)
    .bind(format!("<msg-{folder_id}-{uid}@example.test>"))
    .bind(format!("remote-{folder_id}-{uid}"))
    .fetch_one(&db.write_pool)
    .await
    .expect("сохранить письмо")
    .0
}

/// Незавершённые операции увода письма: вид и состояние.
async fn takeaways(db: &Db, message_id: i64) -> Vec<(String, String)> {
    sqlx::query_as::<_, (String, String)>(
        "SELECT op_kind, status FROM outbox_ops
          WHERE message_id=? AND op_kind IN ('move','delete') ORDER BY id",
    )
    .bind(message_id)
    .fetch_all(&db.pool)
    .await
    .expect("прочитать операции увода письма")
}

async fn seen(db: &Db, message_id: i64) -> bool {
    sqlx::query_as::<_, (i64,)>("SELECT seen FROM messages WHERE id=?")
        .bind(message_id)
        .fetch_one(&db.pool)
        .await
        .expect("прочитать признак прочтения")
        .0
        != 0
}

/// S-021, S-023, S-025, S-028 - S-032, S-042: цепочка применяется к пачке
/// писем целиком, а письмо, которое обработать нельзя, становится пропуском с
/// отдельной причиной и не роняет остальную пачку. Без проверки занятое письмо
/// либо забирало бы свою незавершённую операцию у работника очереди, либо
/// отказом откатывало бы всю пачку, а письмо чужого ящика ушло бы в папку
/// соседнего ящика; отчёт же назвал бы одно общее число вместо применённых и
/// пропущенных с причинами.
#[tokio::test]
async fn a_batch_run_skips_busy_and_foreign_messages_and_applies_the_rest() {
    let db: TestDb = open_test_db("quick-batch").await;
    let mine = seed_account(&db, "mine@example.test").await;
    let inbox = seed_folder(&db, mine, "Входящие", Some("inbox")).await;
    let archive = seed_folder(&db, mine, "Архив", Some("archive")).await;
    let other = seed_account(&db, "other@example.test").await;
    let other_inbox = seed_folder(&db, other, "Входящие", Some("inbox")).await;

    let plain = seed_message(&db, mine, inbox, 1).await;
    let busy = seed_message(&db, mine, inbox, 2).await;
    let foreign = seed_message(&db, other, other_inbox, 3).await;

    // Цепочка: сначала местная отметка о прочтении, затем увод в конкретную
    // папку первого ящика - именно номер папки делает письмо чужого ящика
    // необрабатываемым (S-042).
    let step = seed_step(&db, "В архив", 0).await;
    seed_action(&db, step, 0, "mark_read", None, None).await;
    seed_action(&db, step, 1, "move", Some(archive), None).await;

    // Занятое письмо получает операцию настоящим путём: перенос ставит команда
    // пользователя, а в состояние processing его переводит настоящий работник
    // очереди. Прямым запросом сдвигается только срок первой попытки: окно
    // отмены переноса - 10 секунд, и ждать их незачем.
    db.queue_message_move(&[busy], archive)
        .await
        .expect("поставить перенос занятому письму");
    sqlx::query(
        "UPDATE outbox_ops SET next_attempt_at=datetime('now','-1 minutes') WHERE message_id=?",
    )
    .bind(busy)
    .execute(&db.write_pool)
    .await
    .expect("окно отмены истекло");
    let claimed = db
        .claim_outbox_operations(mine, 10)
        .await
        .expect("работник очереди забрал операции");
    assert_eq!(claimed.len(), 1, "работник забрал ровно одну операцию");
    assert_eq!(
        takeaways(&db, busy).await,
        vec![("move".to_owned(), "processing".to_owned())],
        "письмо обязано быть занятым переносом на сервере"
    );

    let report = db
        .apply_quick_step(step, &[plain, busy, foreign])
        .await
        .expect("пропуск отдельного письма не отменяет применение ко всей пачке");

    assert_eq!(report.applied, 1, "цепочка применена к обычному письму");
    assert_eq!(report.skipped, 2, "пропущены занятое и чужое письмо");
    assert_eq!(report.skipped_busy, 1, "занятое письмо названо отдельно");
    assert_eq!(
        report.skipped_foreign_account, 1,
        "письмо чужого ящика названо отдельной причиной"
    );
    assert_eq!(
        report.skipped_failed, 0,
        "занятое письмо принято за письмо с операцией в состоянии отказа"
    );
    assert_eq!(
        report.skipped_no_folder, 0,
        "папка назначения есть, причина пропуска подменена"
    );

    // S-021: увод обычного письма ушёл в очередь операций, а не на сервер из
    // обработчика нажатия, и целью стоит выбранная папка.
    assert_eq!(report.operation_ids.len(), 1, "поставлен ровно один увод");
    assert_eq!(
        takeaways(&db, plain).await,
        vec![("move".to_owned(), "pending".to_owned())],
        "увод обычного письма не поставлен в очередь"
    );
    let payload: (String,) = sqlx::query_as("SELECT payload FROM outbox_ops WHERE id=?")
        .bind(report.operation_ids[0])
        .fetch_one(&db.pool)
        .await
        .expect("прочитать описание операции");
    assert!(
        payload
            .0
            .contains(&format!("\"target_folder_id\":{archive}")),
        "увод поставлен не в выбранную папку: {}",
        payload.0
    );

    // S-023: операция занятого письма осталась у работника очереди - её не
    // заменили и не продублировали.
    assert_eq!(
        takeaways(&db, busy).await,
        vec![("move".to_owned(), "processing".to_owned())],
        "операция занятого письма изменена быстрым действием"
    );
    assert!(
        takeaways(&db, foreign).await.is_empty(),
        "письму чужого ящика поставлен увод в папку соседнего ящика"
    );

    // S-027, S-031: местная отметка цепочки записана в базу, и пропуск увода
    // не отменил её ни у пропущенных писем, ни у остальной пачки.
    assert!(seen(&db, plain).await, "отметка о прочтении не записана");
    assert!(
        seen(&db, busy).await && seen(&db, foreign).await,
        "пропуск увода откатил уже выполненное местное действие цепочки"
    );
    db.close().await;
}

/// Снятие горячей клавиши идёт тем же путём, что и назначение: интерфейс шлёт
/// пустое сочетание. Хранилище отвергало его разбором, поэтому назначенную
/// клавишу освободить было нечем ни у встроенного действия, ни у слота
/// быстрого действия (issue #104).
#[tokio::test]
async fn an_empty_combo_takes_the_key_off_an_action() {
    let db: TestDb = open_test_db("keybinding-clear").await;
    db.set_keybinding("quick_step_1", "Ctrl+Shift+1")
        .await
        .expect("назначить сочетание слоту");

    db.set_keybinding("quick_step_1", "")
        .await
        .expect("снять сочетание со слота");
    let slot: (String,) =
        sqlx::query_as("SELECT combo FROM keybindings WHERE action='quick_step_1'")
            .fetch_one(&db.pool)
            .await
            .expect("прочитать строку слота");
    assert_eq!(slot.0, "", "сочетание осталось назначенным слоту");

    // Строка действия остаётся на месте: снятие освобождает клавишу, а не
    // выкидывает действие из перечня.
    let rows: (i64,) =
        sqlx::query_as("SELECT count(*) FROM keybindings WHERE action='quick_step_1'")
            .fetch_one(&db.pool)
            .await
            .expect("пересчитать строки слота");
    assert_eq!(rows.0, 1, "снятие удалило строку действия");

    // Встроенное действие снимается тем же вызовом, и освободившееся сочетание
    // достаётся другому действию: ради этого снятие и нужно.
    let built_in: (String, String) = sqlx::query_as(
        "SELECT action, combo FROM keybindings WHERE action<>'quick_step_1' AND combo<>'' LIMIT 1",
    )
    .fetch_one(&db.pool)
    .await
    .expect("взять встроенное действие с назначенным сочетанием");
    db.set_keybinding(&built_in.0, "")
        .await
        .expect("снять сочетание со встроенного действия");
    db.set_keybinding("quick_step_1", &built_in.1)
        .await
        .expect("назначить освободившееся сочетание слоту");
    let moved: (String,) =
        sqlx::query_as("SELECT combo FROM keybindings WHERE action='quick_step_1'")
            .fetch_one(&db.pool)
            .await
            .expect("прочитать сочетание слота");
    assert_eq!(
        moved.0, built_in.1,
        "освободившееся сочетание не досталось слоту"
    );

    // Мусор сочетанием по-прежнему не считается: снятие - это пустая строка, а
    // не любая неразобранная.
    assert!(
        db.set_keybinding("quick_step_1", "Ctrl+Shift+A+B")
            .await
            .is_err(),
        "принято сочетание, которое не разбирается"
    );
    db.close().await;
}
