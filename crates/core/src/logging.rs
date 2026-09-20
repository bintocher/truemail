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
    fn masks_addresses_inside_server_error() {
        assert_eq!(
            mask_error_text("NO [OVERQUOTA] mailbox boss@example.test is full"),
            "NO [OVERQUOTA] mailbox b***@example.test is full",
            "адрес маскируется, остальной текст ошибки остаётся"
        );
    }

    #[test]
    fn keeps_error_without_addresses_as_is() {
        assert_eq!(mask_error_text("connection reset"), "connection reset");
    }

    #[test]
    fn masks_regular_address() {
        assert_eq!(
            mask_email("stanislav.chernov@ligastavok.ru"),
            "s***@ligastavok.ru"
        );
    }

    #[test]
    fn masks_single_char_local_part() {
        assert_eq!(mask_email("a@example.com"), "a***@example.com");
    }

    #[test]
    fn does_not_panic_without_at_sign() {
        assert_eq!(mask_email("not-an-email"), "***");
    }
}
