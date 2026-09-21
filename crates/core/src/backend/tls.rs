//! Узлы, для которых проверка сертификата выключена человеком (issue #118).
//!
//! Признак принадлежит ящику, но соединение устанавливают два десятка мест -
//! от проверки настроек до фонового наблюдения за папкой, - и до каждого из
//! них запись ящика не доходит. Поэтому решение ящика разворачивается в
//! перечень имён узлов: ящик знает свой сервер, а слой соединения спрашивает
//! про узел, к которому идёт.
//!
//! Сертификат принадлежит узлу, поэтому два ящика на одном сервере получают
//! одно решение - и это то же самое решение, которое человек принял для
//! своего ящика.

use std::collections::HashSet;
use std::sync::RwLock;

static INSECURE_HOSTS: RwLock<Option<HashSet<String>>> = RwLock::new(None);

/// Задать перечень узлов, чьи сертификаты не проверяются. Зовётся при загрузке
/// ящиков и после каждой правки их настроек: перечень целиком заменяется,
/// поэтому снятый человеком признак сразу перестаёт действовать.
pub fn set_insecure_hosts<I: IntoIterator<Item = String>>(hosts: I) {
    let set: HashSet<String> = hosts
        .into_iter()
        .map(|host| host.trim().to_ascii_lowercase())
        .filter(|host| !host.is_empty())
        .collect();
    if !set.is_empty() {
        tracing::warn!(
            hosts = set.len(),
            "проверка сертификата выключена для части серверов - соединение с ними можно подменить"
        );
    }
    if let Ok(mut guard) = INSECURE_HOSTS.write() {
        *guard = Some(set);
    }
}

/// Проверяется ли сертификат этого узла. Узел, о котором ничего не известно,
/// проверяется: молчание никогда не означает отказ от проверки.
pub fn is_insecure(host: &str) -> bool {
    let host = host.trim().to_ascii_lowercase();
    if host.is_empty() {
        return false;
    }
    INSECURE_HOSTS
        .read()
        .ok()
        .and_then(|guard| guard.as_ref().map(|set| set.contains(&host)))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Перечень задаётся целиком, поэтому снятый признак перестаёт действовать,
    /// а неизвестный узел проверяется всегда.
    #[test]
    fn only_the_named_hosts_skip_the_check() {
        set_insecure_hosts(["Mail.Corp.Test".to_owned(), "  ".to_owned()]);
        assert!(is_insecure("mail.corp.test"), "имя узла сверяется без учёта регистра");
        assert!(is_insecure("MAIL.CORP.TEST"));
        assert!(!is_insecure("imap.yandex.com"), "чужой узел проверяется как обычно");
        assert!(!is_insecure(""), "пустое имя проверку не отключает");

        set_insecure_hosts(Vec::<String>::new());
        assert!(!is_insecure("mail.corp.test"), "снятый признак перестаёт действовать");
    }
}
