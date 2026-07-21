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

#[cfg(test)]
mod tests {
    use super::mask_email;

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

    #[test]
    fn does_not_panic_on_empty_string() {
        assert_eq!(mask_email(""), "***");
    }
}
