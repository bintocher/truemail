//! Вспомогательные функции для логирования.
//!
//! Вся почта хранится в зашифрованной SQLCipher-базе, но лог-файлы лежат
//! рядом открытым текстом. Чтобы в них не накапливалась карта переписки
//! (адреса всех аккаунтов), email в tracing-событиях маскируется этой
//! функцией. Домен оставляем как есть - он нужен для диагностики (отличить
//! один аккаунт от другого), а вот локальную часть адреса прятать нужно.

/// Маскирует email для логов: оставляет первый символ локальной части и
/// домен целиком, остальное заменяет на "***".
/// "stanislav.chernov@ligastavok.ru" -> "s***@ligastavok.ru".
///
/// Это только для логов. Значение, которое идёт в БД, UI, сетевые запросы
/// или тексты ошибок пользователю, трогать нельзя - там нужен полный адрес.
pub fn mask_email(email: &str) -> String {
    match email.split_once('@') {
        Some((local, domain)) => {
            let first = local.chars().next();
            match first {
                Some(ch) => format!("{ch}***@{domain}"),
                // Локальная часть пустая (адрес вида "@domain") - маскировать нечего.
                None => format!("***@{domain}"),
            }
        }
        // Не похоже на email (нет "@") - возвращаем маску целиком, чтобы не
        // затирать что-то полезное для диагностики полной строкой.
        None => "***".to_string(),
    }
}

/// Маскирует текст ошибки сервера: он часто повторяет адрес получателя, тему
/// или строку письма целиком, а попадает и в журнал, и в поле `last_error`
/// очереди операций, которое спецификация правил прямо называет среди
/// маскируемого (mail-rules-conditions-and-actions.md, S-084). Остальной текст
/// сохраняется: без кода и слов сервера ошибка перестала бы что-либо объяснять.
pub fn mask_error_text(text: &str) -> String {
    static ADDRESS: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let address = ADDRESS.get_or_init(|| {
        regex::Regex::new(r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}")
            .expect("статический шаблон адреса")
    });
    address
        .replace_all(text, |found: &regex::Captures<'_>| {
            mask_email(found.get(0).map_or("", |value| value.as_str()))
        })
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::{mask_email, mask_error_text};

    #[test]
    fn masks_the_local_part_and_keeps_the_domain() {
        // Домен в журнале нужен, чтобы отличить один аккаунт от другого, а
        // локальная часть - уже карта переписки. Строка без "@" не адрес, и
        // отдать её в журнал целиком нельзя: это может быть что угодно.
        let cases = [
            ("stanislav.chernov@ligastavok.ru", "s***@ligastavok.ru"),
            ("a@example.com", "a***@example.com"),
            ("@example.com", "***@example.com"),
            ("not-an-email", "***"),
        ];
        for (source, expected) in cases {
            assert_eq!(mask_email(source), expected, "адрес: {source}");
        }
    }

    #[test]
    fn masks_every_address_inside_server_error_and_keeps_the_rest() {
        // Текст ошибки сервера идёт и в журнал, и в last_error очереди
        // операций (S-084): адреса маскируются, код и слова сервера остаются -
        // без них ошибка перестала бы что-либо объяснять.
        let cases = [
            (
                "NO [OVERQUOTA] mailbox boss@example.test is full",
                "NO [OVERQUOTA] mailbox b***@example.test is full",
            ),
            (
                "550 5.1.1 boss@example.test, copy@example.test unknown",
                "550 5.1.1 b***@example.test, c***@example.test unknown",
            ),
            ("connection reset", "connection reset"),
        ];
        for (source, expected) in cases {
            assert_eq!(mask_error_text(source), expected, "текст: {source}");
        }
    }
}
