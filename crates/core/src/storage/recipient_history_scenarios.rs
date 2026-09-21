//! Сценарные проверки истории получателей (specs/recipient-history.md).
//!
//! Проверки идут по настоящему пути: настоящая база, настоящие письма папки с
//! ролью `sent` и настоящий перенос почтовых контактов прежнего сбора.

use super::Db;
use super::recipient_history::{RecipientTouch, TouchOrigin};
use super::repo::test_storage::{TestDb, open_test_db};
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

/// Отправленное письмо в папке с ролью `sent`.
async fn seed_sent_message(
    db: &Db,
    account_id: i64,
    folder_id: i64,
    uid: i64,
    message_id: Option<&str>,
    to: &str,
    date: &str,
) -> i64 {
    sqlx::query_as::<_, (i64,)>(
        "INSERT INTO messages(account_id, folder_id, uid, from_addr, subject, preview, date,
                              rfc822_message_id, to_addrs, remote_id, size)
         VALUES(?, ?, ?, 'me@example.test', 'письмо', '', ?, ?, ?, ?, 100)
         RETURNING id",
    )
    .bind(account_id)
    .bind(folder_id)
    .bind(uid)
    .bind(date)
    .bind(message_id)
    .bind(format!(r#"[{{"name":"Иван Петров","email":"{to}"}}]"#))
    .bind(format!("remote-{folder_id}-{uid}"))
    .fetch_one(&db.write_pool)
    .await
    .expect("сохранить письмо")
    .0
}

async fn visible_addresses(db: &Db, account_id: i64) -> Vec<String> {
    sqlx::query_as::<_, (String,)>(
        "SELECT display_address FROM recipient_history
          WHERE account_id=? AND hidden_by_user=0 AND evicted=0
          ORDER BY display_address",
    )
    .bind(account_id)
    .fetch_all(&db.pool)
    .await
    .expect("прочитать видимые записи")
    .into_iter()
    .map(|(value,)| value)
    .collect()
}

async fn touch_count(db: &Db, account_id: i64, address: &str) -> i64 {
    sqlx::query_as::<_, (i64,)>(
        "SELECT count(*) FROM recipient_history_touches t
           JOIN recipient_history h ON h.id=t.history_id
          WHERE h.account_id=? AND h.address_key=?",
    )
    .bind(account_id)
    .bind(address)
    .fetch_one(&db.pool)
    .await
    .expect("прочитать отметки обращений")
    .0
}

/// Отмотать курсор прохода: так выглядит следующая синхронизация, которая
/// перечитывает папку отправленных с начала.
async fn rewind_history_cursor(db: &Db, account_id: i64) {
    sqlx::query("UPDATE recipient_history_state SET cursor_message_id=0 WHERE account_id=?")
        .bind(account_id)
        .execute(&db.write_pool)
        .await
        .expect("отмотать курсор");
}

fn iso(days_ago: i64) -> String {
    (chrono::Utc::now() - chrono::Duration::days(days_ago))
        .format("%Y-%m-%dT%H:%M:%S+00:00")
        .to_string()
}

/// Собственная подтверждённая отправка записывает обращение один раз, а её
/// копия, пришедшая из папки с ролью `sent`, второго обращения не добавляет.
/// Письмо, отправленное с другого устройства, наоборот, учитывается по
/// появлению в этой папке (S-009 - S-012, S-017).
#[tokio::test]
async fn own_send_counts_once_while_a_letter_from_another_device_still_counts() {
    let db: TestDb = open_test_db("history-own").await;
    let account = seed_account(&db, "me@example.test").await;
    let sent = seed_folder(&db, account, "Sent", Some("sent")).await;

    // Письмо ушло из этой программы: обращение записано в момент подтверждения
    // сервером, а закреплённый идентификатор запомнен.
    db.record_own_send(account, "<own-1@example.test>")
        .await
        .expect("отметка собственной отправки");
    db.record_recipient_touches(
        account,
        &[RecipientTouch {
            email: "client@partner.test".into(),
            name: "Клиент".into(),
            message_key: "<own-1@example.test>".into(),
            used_at: iso(0),
        }],
        TouchOrigin::OwnSend,
    )
    .await
    .expect("записать обращение");

    // Та же копия появилась в папке отправленных.
    seed_sent_message(
        &db,
        account,
        sent,
        1,
        Some("<own-1@example.test>"),
        "client@partner.test",
        &iso(0),
    )
    .await;
    // Письмо с другого устройства отметки собственной отправки не имеет.
    seed_sent_message(
        &db,
        account,
        sent,
        2,
        Some("<phone-1@example.test>"),
        "partner@partner.test",
        &iso(1),
    )
    .await;
    db.advance_recipient_history(account)
        .await
        .expect("пополнение из папки отправленных");

    assert_eq!(
        touch_count(&db, account, "client@partner.test").await,
        1,
        "своя копия обращения не удваивает"
    );
    assert_eq!(
        touch_count(&db, account, "partner@partner.test").await,
        1,
        "письмо с другого устройства учитывается"
    );

    // Папку пересобрали: локальные номера писем сменились, а ключ письма нет.
    sqlx::query("UPDATE messages SET uid=uid+100 WHERE folder_id=?")
        .bind(sent)
        .execute(&db.write_pool)
        .await
        .expect("пересобрать папку");
    sqlx::query("UPDATE recipient_history_state SET cursor_message_id=0 WHERE account_id=?")
        .bind(account)
        .execute(&db.write_pool)
        .await
        .expect("отмотать курсор пополнения");
    db.advance_recipient_history(account)
        .await
        .expect("повторное пополнение");
    assert_eq!(
        touch_count(&db, account, "partner@partner.test").await,
        1,
        "ключ письма не даёт учесть одно письмо дважды"
    );
    db.close().await;
}

/// Почтовые контакты прежнего сбора переезжают в историю, а контакт, который
/// пользователь дополнил телефоном, остаётся в адресной книге со всеми своими
/// данными (S-002 - S-005).
#[tokio::test]
async fn mail_contacts_move_to_history_but_the_enriched_one_stays() {
    let db: TestDb = open_test_db("history-migration").await;
    let account = seed_account(&db, "me@example.test").await;
    let plain = sqlx::query_as::<_, (i64,)>(
        "INSERT INTO contacts(account_id, uid, display_name) VALUES(?, 'mail:plain@partner.test', 'Простой')
         RETURNING id",
    )
    .bind(account)
    .fetch_one(&db.write_pool)
    .await
    .expect("почтовый контакт")
    .0;
    sqlx::query("INSERT INTO contact_emails(contact_id, email, kind) VALUES(?, 'plain@partner.test', 'mail')")
        .bind(plain)
        .execute(&db.write_pool)
        .await
        .expect("адрес почтового контакта");
    let hidden = sqlx::query_as::<_, (i64,)>(
        "INSERT INTO contacts(account_id, uid, display_name, hidden)
         VALUES(?, 'mail:hidden@partner.test', 'Скрытый', 1) RETURNING id",
    )
    .bind(account)
    .fetch_one(&db.write_pool)
    .await
    .expect("скрытый почтовый контакт")
    .0;
    sqlx::query("INSERT INTO contact_emails(contact_id, email, kind) VALUES(?, 'hidden@partner.test', 'mail')")
        .bind(hidden)
        .execute(&db.write_pool)
        .await
        .expect("адрес скрытого контакта");
    let enriched = sqlx::query_as::<_, (i64,)>(
        "INSERT INTO contacts(account_id, uid, display_name)
         VALUES(?, 'mail:rich@partner.test', 'Дополненный') RETURNING id",
    )
    .bind(account)
    .fetch_one(&db.write_pool)
    .await
    .expect("дополненный почтовый контакт")
    .0;
    sqlx::query("INSERT INTO contact_emails(contact_id, email, kind) VALUES(?, 'rich@partner.test', 'mail')")
        .bind(enriched)
        .execute(&db.write_pool)
        .await
        .expect("адрес дополненного контакта");
    sqlx::query("INSERT INTO contact_phones(contact_id, number, kind) VALUES(?, '+7 000 000-00-00', 'mobile')")
        .bind(enriched)
        .execute(&db.write_pool)
        .await
        .expect("телефон, добавленный пользователем");
    let explicit = sqlx::query_as::<_, (i64,)>(
        "INSERT INTO contacts(account_id, uid, display_name, etag)
         VALUES(?, 'local:manual', 'Созданный вручную', 'etag-1') RETURNING id",
    )
    .bind(account)
    .fetch_one(&db.write_pool)
    .await
    .expect("явный контакт")
    .0;

    let report = db
        .migrate_mail_contacts_to_history()
        .await
        .expect("перенос почтовых контактов");
    assert_eq!(
        report.moved, 2,
        "перенесены только простые почтовые контакты"
    );

    let left: Vec<(i64,)> = sqlx::query_as("SELECT id FROM contacts ORDER BY id")
        .fetch_all(&db.pool)
        .await
        .expect("прочитать адресную книгу");
    let left: Vec<i64> = left.into_iter().map(|(id,)| id).collect();
    assert!(
        left.contains(&enriched),
        "дополненный контакт остаётся как есть"
    );
    assert!(left.contains(&explicit), "явный контакт не трогается");
    assert!(!left.contains(&plain) && !left.contains(&hidden));
    let (phones,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM contact_phones WHERE contact_id=?")
            .bind(enriched)
            .fetch_one(&db.pool)
            .await
            .expect("прочитать телефоны");
    assert_eq!(phones, 1, "телефон пользователя сохранён");

    // S-005: скрытый почтовый контакт стал записью, скрытой пользователем.
    assert_eq!(
        visible_addresses(&db, account).await,
        vec!["plain@partner.test"]
    );
    let (hidden_rows,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM recipient_history WHERE account_id=? AND hidden_by_user=1",
    )
    .bind(account)
    .fetch_one(&db.pool)
    .await
    .expect("прочитать скрытые записи");
    assert_eq!(hidden_rows, 1);

    // Повторный запуск переносить уже нечего: контактов прежнего сбора не
    // осталось.
    assert_eq!(
        db.migrate_mail_contacts_to_history()
            .await
            .expect("повторный перенос")
            .moved,
        0
    );
    db.close().await;
}

/// Подсказка ставит адресата переписки выше алфавитного однофамильца из
/// контактов, объединяет один адрес из обоих источников в один кандидат и
/// помечает кандидата, взятого только из истории (S-029, S-030, S-035, S-039).
#[tokio::test]
async fn correspondence_outranks_an_alphabetic_namesake_from_the_address_book() {
    let db: TestDb = open_test_db("history-rank").await;
    let account = seed_account(&db, "me@example.test").await;
    let contact = sqlx::query_as::<_, (i64,)>(
        "INSERT INTO contacts(account_id, uid, display_name, etag)
         VALUES(?, 'local:alpha', 'Абрамов Алексей', 'etag') RETURNING id",
    )
    .bind(account)
    .fetch_one(&db.write_pool)
    .await
    .expect("контакт адресной книги")
    .0;
    sqlx::query("INSERT INTO contact_emails(contact_id, email, kind) VALUES(?, 'abramov@partner.test', 'work')")
        .bind(contact)
        .execute(&db.write_pool)
        .await
        .expect("адрес контакта");
    let both = sqlx::query_as::<_, (i64,)>(
        "INSERT INTO contacts(account_id, uid, display_name, etag)
         VALUES(?, 'local:both', 'Яковлев из книги', 'etag') RETURNING id",
    )
    .bind(account)
    .fetch_one(&db.write_pool)
    .await
    .expect("контакт того же адреса")
    .0;
    sqlx::query("INSERT INTO contact_emails(contact_id, email, kind) VALUES(?, 'yakovlev@partner.test', 'work')")
        .bind(both)
        .execute(&db.write_pool)
        .await
        .expect("адрес контакта");

    // Свежая переписка с двумя адресатами: у одного она чаще.
    for (email, days) in [
        ("yakovlev@partner.test", 0_i64),
        ("yakovlev@partner.test", 5),
        ("zhukov@partner.test", 200),
    ] {
        db.record_recipient_touches(
            account,
            &[RecipientTouch {
                email: email.into(),
                name: "Адресат".into(),
                message_key: format!("<{email}-{days}@example.test>"),
                used_at: iso(days),
            }],
            TouchOrigin::OwnSend,
        )
        .await
        .expect("записать обращение");
    }

    let candidates = db
        .recipient_candidates(account)
        .await
        .expect("кандидаты подсказки");
    assert_eq!(
        candidates.len(),
        3,
        "один адрес двух источников даёт один кандидат"
    );
    assert_eq!(candidates[0].email, "yakovlev@partner.test");
    assert_eq!(candidates[0].source, CANDIDATE_SOURCE_BOTH);
    assert_eq!(
        candidates[0].name, "Яковлев из книги",
        "имя контакта важнее имени из письма"
    );
    assert_eq!(candidates[1].email, "zhukov@partner.test");
    assert_eq!(candidates[1].source, CANDIDATE_SOURCE_HISTORY);
    assert_eq!(
        candidates[2].email, "abramov@partner.test",
        "кандидат без переписки стоит ниже, хотя по алфавиту он первый"
    );
    assert_eq!(candidates[2].source, CANDIDATE_SOURCE_CONTACT);
    assert!(candidates[0].rank > candidates[1].rank);
    db.close().await;
}

/// Один адрес одного письма даёт одно обращение, собственный адрес в историю не
/// попадает, а конфликт ключа при изменении адреса не сливает записи молча
/// (S-016, S-019, S-045).
#[tokio::test]
async fn duplicates_own_address_and_key_conflicts_are_handled_explicitly() {
    let db: TestDb = open_test_db("history-edges").await;
    let account = seed_account(&db, "me@example.test").await;
    db.record_recipient_touches(
        account,
        &[
            RecipientTouch {
                email: "Client@Partner.Test".into(),
                name: "Клиент".into(),
                message_key: "<one@example.test>".into(),
                used_at: iso(0),
            },
            // Тот же адрес в другом поле того же письма.
            RecipientTouch {
                email: "client@partner.test".into(),
                name: "Клиент".into(),
                message_key: "<one@example.test>".into(),
                used_at: iso(0),
            },
            // Собственный адрес ящика.
            RecipientTouch {
                email: "me@example.test".into(),
                name: "Я".into(),
                message_key: "<one@example.test>".into(),
                used_at: iso(0),
            },
        ],
        TouchOrigin::OwnSend,
    )
    .await
    .expect("записать обращения");
    assert_eq!(touch_count(&db, account, "client@partner.test").await, 1);
    // Написание локальной части сохраняется, домен приводится к общей форме -
    // тот же ключ адреса, что и у списков отправителей.
    assert_eq!(
        visible_addresses(&db, account).await,
        vec!["Client@partner.test"]
    );

    db.record_recipient_touches(
        account,
        &[RecipientTouch {
            email: "other@partner.test".into(),
            name: "Другой".into(),
            message_key: "<two@example.test>".into(),
            used_at: iso(0),
        }],
        TouchOrigin::OwnSend,
    )
    .await
    .expect("записать второе обращение");
    let (entry_id,): (i64,) =
        sqlx::query_as("SELECT id FROM recipient_history WHERE account_id=? AND address_key=?")
            .bind(account)
            .bind("other@partner.test")
            .fetch_one(&db.pool)
            .await
            .expect("найти запись");
    let conflict = db
        .update_recipient_history_entry(account, entry_id, None, Some("client@partner.test".into()))
        .await;
    assert!(conflict.is_err(), "занятый ключ не сливает записи молча");
    assert_eq!(
        visible_addresses(&db, account).await,
        vec![
            "Client@partner.test".to_owned(),
            "other@partner.test".to_owned()
        ]
    );
    db.close().await;
}

/// Решение человека убрать адрес и очистка истории держатся против повторной
/// синхронизации и держатся в пределах одних суток: письмо, пришедшее раньше
/// решения, адрес не возвращает, письмо, пришедшее позже, возвращает, а
/// собственная новая отправка возвращает адрес сразу (S-046 - S-049).
///
/// Время обращения и время решения человека пишутся разными запросами, и
/// сравнение их как строк разных видов давало на одной дате обратный ответ.
/// Синхронизация же перечитывает ту же папку с начала: без границы решения
/// убранный адрес возвращался бы первым же проходом, а очищенная история
/// наполнялась бы теми же прежними письмами.
#[tokio::test]
async fn hiding_and_clearing_hold_within_the_same_day() {
    let db: TestDb = open_test_db("history-same-day").await;
    let account = seed_account(&db, "me@example.test").await;
    let sent = seed_folder(&db, account, "Sent", Some("sent")).await;
    let morning = (chrono::Utc::now() - chrono::Duration::minutes(5))
        .format("%Y-%m-%dT%H:%M:%S+00:00")
        .to_string();
    seed_sent_message(
        &db,
        account,
        sent,
        1,
        Some("<morning@example.test>"),
        "client@partner.test",
        &morning,
    )
    .await;
    // Второе письмо давнее: решение человека и граница очистки проверяются не
    // только на одной дате, но и на разнице в дни.
    seed_sent_message(
        &db,
        account,
        sent,
        2,
        Some("<old@example.test>"),
        "typo@partner.test",
        &iso(10),
    )
    .await;
    db.advance_recipient_history(account)
        .await
        .expect("первичное заполнение");
    assert_eq!(
        visible_addresses(&db, account).await,
        vec!["client@partner.test", "typo@partner.test"],
        "первичное заполнение собрало не все адреса"
    );

    // Человек убрал оба адреса, а прежние письма остались в папке.
    let entries = db
        .list_recipient_history(account, 100, 0)
        .await
        .expect("записи истории");
    assert_eq!(entries.len(), 2, "в истории не обе записи");
    for entry in &entries {
        db.hide_recipient_history_entry(account, entry.id)
            .await
            .expect("убрать адрес");
    }
    // Решение человека состоялось минуту назад: иначе оно и письмо того же дня
    // попали бы в одну и ту же секунду, и проверка зависела бы от скорости
    // машины.
    sqlx::query(
        "UPDATE recipient_history
            SET hidden_at=strftime('%Y-%m-%dT%H:%M:%S+00:00','now','-1 minute')
          WHERE account_id=?",
    )
    .bind(account)
    .execute(&db.write_pool)
    .await
    .expect("сдвинуть время решения");

    // Следующая синхронизация перечитывает ту же папку с начала: ни письмо
    // того же дня, ни давнее письмо убранный адрес не возвращают.
    rewind_history_cursor(&db, account).await;
    db.advance_recipient_history(account)
        .await
        .expect("повторное пополнение");
    assert!(
        visible_addresses(&db, account).await.is_empty(),
        "письмо, пришедшее до решения человека, вернуло убранный адрес"
    );

    // Собственная новая отправка - действие человека, а не перечитанная папка:
    // адрес возвращается сразу и без нового письма в папке отправленных.
    db.record_recipient_touches(
        account,
        &[RecipientTouch {
            email: "typo@partner.test".into(),
            name: String::new(),
            message_key: "<new@example.test>".into(),
            used_at: iso(0),
        }],
        TouchOrigin::OwnSend,
    )
    .await
    .expect("новая отправка");
    assert_eq!(
        visible_addresses(&db, account).await,
        vec!["typo@partner.test"],
        "собственная отправка не сняла скрытие"
    );

    // Новое письмо тому же адресату пришло уже после решения: адрес вернулся и
    // по папке отправленных.
    let later = (chrono::Utc::now() + chrono::Duration::seconds(2))
        .format("%Y-%m-%dT%H:%M:%S+00:00")
        .to_string();
    seed_sent_message(
        &db,
        account,
        sent,
        3,
        Some("<later@example.test>"),
        "client@partner.test",
        &later,
    )
    .await;
    db.advance_recipient_history(account)
        .await
        .expect("пополнение после нового письма");
    assert_eq!(
        visible_addresses(&db, account).await,
        vec!["client@partner.test", "typo@partner.test"],
        "новое письмо не вернуло адрес"
    );

    // Очистка держит ту же границу: прежние письма - и сегодняшнее, и давнее -
    // историю заново не наполняют.
    db.clear_recipient_history(account)
        .await
        .expect("очистить историю");
    rewind_history_cursor(&db, account).await;
    db.advance_recipient_history(account)
        .await
        .expect("пополнение после очистки");
    assert!(
        visible_addresses(&db, account).await.is_empty(),
        "граница очистки не удержала прежние письма"
    );
    db.close().await;
}

/// Первичное заполнение проходит папку до конца, а не одну пачку: на ящике с
/// сотнями отправленных писем история иначе осталась бы неполной, объявив проход
/// законченным (S-008, S-018).
#[tokio::test]
async fn initial_backfill_walks_past_the_first_batch() {
    let db: TestDb = open_test_db("history-backfill").await;
    let account = seed_account(&db, "me@example.test").await;
    let sent = seed_folder(&db, account, "Sent", Some("sent")).await;
    let letters = LimitSet::defaults().get(LIMIT_PURGE_BATCH) + 20;
    let mut tx = db.begin_write().await.expect("открыть запись");
    for uid in 1..=letters {
        sqlx::query(
            "INSERT INTO messages(account_id, folder_id, uid, from_addr, subject, preview, date,
                                  rfc822_message_id, to_addrs, remote_id, size)
             VALUES(?, ?, ?, 'me@example.test', 'письмо', '', ?, ?, ?, ?, 100)",
        )
        .bind(account)
        .bind(sent)
        .bind(uid)
        .bind(iso(1))
        .bind(format!("<letter-{uid}@example.test>"))
        .bind(format!(
            r#"[{{"name":"Клиент","email":"client{uid}@partner.test"}}]"#
        ))
        .bind(format!("remote-{sent}-{uid}"))
        .execute(&mut *tx)
        .await
        .expect("сохранить письмо");
    }
    tx.commit().await.expect("записать письма");

    let recorded = db
        .advance_recipient_history(account)
        .await
        .expect("первичное заполнение");
    assert_eq!(recorded, letters, "проход учёл все письма папки");
    let (visible,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM recipient_history WHERE account_id=? AND hidden_by_user=0",
    )
    .bind(account)
    .fetch_one(&db.pool)
    .await
    .expect("прочитать историю");
    assert_eq!(visible, letters, "в истории все адресаты папки");
    let (done, cursor): (i64, i64) = sqlx::query_as(
        "SELECT initial_done, cursor_message_id FROM recipient_history_state WHERE account_id=?",
    )
    .bind(account)
    .fetch_one(&db.pool)
    .await
    .expect("прочитать состояние заполнения");
    assert_eq!(done, 1, "проход объявлен законченным только по исчерпании");
    assert!(cursor > 0, "курсор сдвинут на последнее письмо");
    db.close().await;
}

/// Ранг записи считает запрос базы, и его границы закрыты явно: обращение
/// возрастом ровно 30, 90 или 365 суток принадлежит следующей группе, а не
/// теряется между ними (S-028).
#[tokio::test]
async fn rank_boundaries_belong_to_the_next_group() {
    let db: TestDb = open_test_db("history-rank-borders").await;
    let account = seed_account(&db, "me@example.test").await;
    let cases = [
        ("fresh@partner.test", 29_i64, 3_i64),
        ("month@partner.test", 30, 2),
        ("quarter@partner.test", 89, 2),
        ("season@partner.test", 90, 1),
        ("year@partner.test", 364, 1),
        ("ancient@partner.test", 365, 0),
    ];
    for (email, days, _) in cases {
        db.record_recipient_touches(
            account,
            &[RecipientTouch {
                email: email.into(),
                name: String::new(),
                message_key: format!("<{email}@example.test>"),
                used_at: iso(days),
            }],
            TouchOrigin::OwnSend,
        )
        .await
        .expect("записать обращение");
    }
    let candidates = db
        .recipient_candidates(account)
        .await
        .expect("кандидаты подсказки");
    for (email, days, rank) in cases {
        let candidate = candidates
            .iter()
            .find(|item| item.email == email)
            .expect("кандидат истории");
        assert_eq!(
            candidate.rank, rank,
            "обращение возрастом {days} суток весит не столько"
        );
    }
    db.close().await;
}

/// Место адресата в подсказке тем же путём, которым его берёт композер.
async fn rank_of(db: &Db, account_id: i64, email: &str) -> i64 {
    db.recipient_candidates(account_id)
        .await
        .expect("кандидаты подсказки")
        .into_iter()
        .find(|candidate| candidate.email == email)
        .expect("адресат пропал из подсказки")
        .rank
}

/// Пороги свежести остаются шкалой: порог, догнавший соседний, отклоняется с
/// именем зависимого поля, а упорядоченный сдвиг принимается и действует
/// (S-028, configurable-limits.md S-021).
///
/// Пороги проверялись поодиночке, каждый в своих границах. Недавнее, опущенное
/// ниже свежего, перехватывалось первым условием каскада, вес недавнего
/// становился недостижимым, и подсказка молча переставала различать свежие и
/// недавние обращения.
#[tokio::test]
async fn freshness_thresholds_stay_ordered_so_the_scale_keeps_working() {
    let db: TestDb = open_test_db("history-rank-order").await;
    let account = seed_account(&db, "me@example.test").await;
    for (email, days) in [("fresh@partner.test", 10_i64), ("recent@partner.test", 45)] {
        db.record_recipient_touches(
            account,
            &[RecipientTouch {
                email: email.into(),
                name: String::new(),
                message_key: format!("<{email}>"),
                used_at: iso(days),
            }],
            TouchOrigin::OwnSend,
        )
        .await
        .expect("записать обращение");
    }
    assert_eq!(rank_of(&db, account, "fresh@partner.test").await, 3);
    assert_eq!(rank_of(&db, account, "recent@partner.test").await, 2);

    let refused = db
        .set_limit(LIMIT_RANK_RECENT_DAYS, 10)
        .await
        .expect_err("недавнее принято ниже свежего: пороги проверяются поодиночке");
    let message = refused.to_string();
    assert!(
        message.contains("считать свежим"),
        "отказ не называет зависимое поле: {message}"
    );
    assert!(
        message.contains("30"),
        "отказ не называет значение зависимого поля: {message}"
    );
    assert_eq!(
        db.limit(LIMIT_RANK_RECENT_DAYS),
        90,
        "отклонённое значение всё-таки изменило порог"
    );
    // Равенство порогов ломает шкалу так же, как их перестановка.
    assert!(
        db.set_limit(LIMIT_RANK_FRESH_DAYS, 90).await.is_err(),
        "свежее, догнавшее недавнее, принято: вес недавнего стал недостижим"
    );
    assert_eq!(rank_of(&db, account, "fresh@partner.test").await, 3);
    assert_eq!(
        rank_of(&db, account, "recent@partner.test").await,
        2,
        "подсказка перестала различать свежие и недавние обращения"
    );

    // Упорядоченный сдвиг шкалы принимается, и обращения меняют вес вместе с
    // ним: обе записи сдвигаются на ступень вниз.
    db.set_limit(LIMIT_RANK_FRESH_DAYS, 5)
        .await
        .expect("сузить порог свежести");
    db.set_limit(LIMIT_RANK_RECENT_DAYS, 20)
        .await
        .expect("сузить порог недавнего");
    assert_eq!(rank_of(&db, account, "fresh@partner.test").await, 2);
    assert_eq!(rank_of(&db, account, "recent@partner.test").await, 1);
    db.close().await;
}
