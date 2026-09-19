//! Обезличивание строк журнала для архива диагностики.
//!
//! Спецификация: specs/diagnostics-bundle.md (issue #72). Этот модуль отвечает
//! только за текстовое обезличивание одной уже прочитанной строки - чтение
//! файлов, упаковку в zip и `manifest.json` делает `apps/desktop/src-tauri`
//! (там же закреплена прямая зависимость `zip`, S-002 - S-016 по вводу-выводу
//! проверяются там). Здесь - чистые функции без файлового ввода-вывода, чтобы
//! категории и устойчивость псевдонимов проверялись обычными модульными
//! тестами ядра.

use regex::{Captures, Regex};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// Предел длины одной обрабатываемой строки журнала (S-012): остаток строки
/// сверх этого предела отбрасывается и не попадает в архив.
pub const MAX_LINE_BYTES: usize = 64 * 1024;

/// Категория распознанного значения (термины и состояния спецификации).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Category {
    Email,
    Host,
    Path,
    Folder,
    AccountId,
    MessageId,
    FolderId,
    Uuid,
    Uid,
    MessageIdHeader,
    /// Содержимое письма и значения условий правил: тема, предпросмотр, адреса
    /// в полях записи и текст ошибки очереди операций
    /// (mail-rules-conditions-and-actions.md, S-084).
    Content,
}

impl Category {
    /// Имя категории - оно же префикс псевдонима `[<категория>-<отпечаток>]`.
    pub const fn tag(self) -> &'static str {
        match self {
            Self::Email => "email",
            Self::Host => "host",
            Self::Path => "path",
            Self::Folder => "folder",
            Self::AccountId => "account_id",
            Self::MessageId => "message_id",
            Self::FolderId => "folder_id",
            Self::Uuid => "uuid",
            Self::Uid => "uid",
            Self::MessageIdHeader => "message_id_header",
            Self::Content => "content",
        }
    }

    /// Категория по имени поля структурированной записи журнала
    /// (`tracing`-события пишут `key=value`); None - поле не из перечня.
    fn from_field_name(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "account_id" => Some(Self::AccountId),
            "folder_id" => Some(Self::FolderId),
            "message_id" => Some(Self::MessageId),
            "uid" => Some(Self::Uid),
            "collection" | "folder" | "mailbox" | "remote_path" => Some(Self::Folder),
            // S-084: тема, предпросмотр, адреса, значение условия правила и
            // текст ошибки очереди в архив открытым текстом не попадают.
            // Значения списков отправителей, адрес записи автоочистки, тема и
            // участники игнорируемой переписки попадают сюда же
            // (blocked-senders.md S-052, ignore-conversation.md S-053,
            // sweep-by-sender.md S-049).
            "subject"
            | "preview"
            | "body"
            | "value"
            | "rule_value"
            | "condition_value"
            | "from"
            | "from_addr"
            | "to"
            | "to_addrs"
            | "recipient"
            | "sender"
            | "last_error"
            | "policy_value"
            | "address"
            | "domain"
            | "participants"
            | "conversation_subject" => Some(Self::Content),
            _ => None,
        }
    }
}

/// Счётчики замен по категориям для `manifest.json` (S-007). Сериализуется
/// как обычный объект `{"email": 3, ...}` - без имени пользователя, пути или
/// сетевого адреса, только категория и число замен.
#[derive(Debug, Default, Clone, serde::Serialize)]
#[serde(transparent)]
pub struct ReplacementCounts(pub BTreeMap<String, u64>);

impl ReplacementCounts {
    fn increment(&mut self, category: Category) {
        *self.0.entry(category.tag().to_owned()).or_insert(0) += 1;
    }
}

/// Строит псевдоним значения одной категории (S-004): отпечаток - первые три
/// байта SHA-256 от соли архива, имени категории и исходного значения. Соль
/// создаётся заново на каждый архив (S-004, S-005) и никогда не уходит на
/// диск - только сюда, в память процесса на время сбора.
fn alias(salt: &[u8; 16], category: Category, value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(salt);
    hasher.update(category.tag().as_bytes());
    hasher.update([0u8]);
    hasher.update(value.as_bytes());
    let digest = hasher.finalize();
    format!(
        "[{}-{:02x}{:02x}{:02x}]",
        category.tag(),
        digest[0],
        digest[1],
        digest[2]
    )
}

/// Компилирует один раз статический шаблон - строки журнала однотипны, а
/// компиляция регулярного выражения на каждую строку заметно дороже поиска.
macro_rules! static_regex {
    ($name:ident, $pattern:expr) => {
        fn $name() -> &'static Regex {
            static CELL: OnceLock<Regex> = OnceLock::new();
            CELL.get_or_init(|| Regex::new($pattern).expect("статический шаблон диагностики"))
        }
    };
}

// Message-ID заголовка (RFC 5322: `<локальная-часть@домен>`) - должен
// обрабатываться раньше общего email, иначе угловые скобки остались бы вокруг
// уже подставленного псевдонима адреса.
static_regex!(message_id_header_re, r"<[^\s<>@]+@[^\s<>]+>");
// UUID - раньше общего адреса и узла, так как шаблон куда специфичнее.
static_regex!(
    uuid_re,
    r"(?i)\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b"
);
// Поля структурированной записи журнала: account_id=1, folder_id="2",
// message_id=3, uid=42 - значение может быть в кавычках или без них.
static_regex!(
    field_id_re,
    r#"(?i)\b(account_id|folder_id|message_id|uid)\s*=\s*("[^"]*"|[^\s,;}]+)"#
);
static_regex!(
    field_folder_re,
    r#"(?i)\b(collection|folder|mailbox|remote_path)\s*=\s*("[^"]*"|[^\s,;}]+)"#
);
// Поля с содержимым письма и значениями условий правил: subject="...",
// value="...", last_error="..." (S-084). Идут раньше шаблонов адреса, узла и
// пути: внутри такого значения может оказаться и адрес, и текст ошибки
// сервера, и всё оно целиком заменяется одним псевдонимом.
static_regex!(
    field_content_re,
    r#"(?i)\b(subject|preview|body|value|rule_value|condition_value|from|from_addr|to|to_addrs|recipient|sender|last_error|policy_value|address|domain|participants|conversation_subject)\s*=\s*("[^"]*"|[^\s,;}]+)"#
);
// Пути: якорь (начало строки или пробел/кавычка/скобка/знак равенства) не
// входит в замену - иначе разделитель перед путём терялся бы. Без якоря путь
// внутри URL (`https://host/EWS/...`) ошибочно поглотил бы имя узла.
static_regex!(windows_path_re, r#"(?:^|[\s"'=(<])([A-Za-z]:\\[^\s"'<>]+)"#);
static_regex!(unc_path_re, r#"(?:^|[\s"'=(<])(\\\\[^\s"'<>]+)"#);
static_regex!(
    unix_path_re,
    r#"(?:^|[\s"'=(<])(/(?:[^\s"'<>/]+/)+[^\s"'<>]*)"#
);
// Домашний каталог: общий шаблон пути выше обрывается на первом пробеле, а
// имя пользователя в нём может быть составным ("Ivan Petrov") - тогда
// фамилия оставалась бы открытой в архиве вместе с остатком пути. Эти три
// шаблона идут раньше общих путей (regex::Regex::replace_all применяет их по
// порядку вызова в anonymize_line) и позволяют имени внутри `Users`/`home`
// содержать пробелы вплоть до следующего разделителя пути, кавычки или конца
// строки, захватывая весь путь целиком - как и общий шаблон, одним псевдонимом.
static_regex!(
    windows_home_re,
    r#"(?:^|[\s"'=(<])([A-Za-z]:\\Users\\[^\\"'<>]+(?:\\[^\s"'<>]*)?)"#
);
static_regex!(
    unix_home_re,
    r#"(?:^|[\s"'=(<])(/home/[^/"'<>]+(?:/[^\s"'<>]*)?)"#
);
static_regex!(
    macos_home_re,
    r#"(?:^|[\s"'=(<])(/Users/[^/"'<>]+(?:/[^\s"'<>]*)?)"#
);
static_regex!(
    email_re,
    r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b"
);
// Адрес внутри URL часто записан с кодированным знаком @. Такой адрес надо
// забрать целиком раньше шаблона узла, иначе домен заменится отдельно, а
// локальная часть останется в архиве открытой.
static_regex!(
    encoded_email_re,
    r"(?i)\b[A-Za-z0-9._+%-]+%40[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b"
);
// Узел: домен или имя сервера - в тексте журнала неразличимы (одна
// категория). Идёт последним: email и путь уже забрали свои совпадения, так
// что имена файлов внутри путей ("truemail.log") сюда не попадают.
static_regex!(
    host_re,
    r"\b(?:[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?\.)+[A-Za-z]{2,}\b"
);

// Сервер могут задать голым адресом IP - для читателя архива это такой же
// узел, как и имя: без этого шаблона адрес остался бы в архиве открытым.
static_regex!(ipv4_re, r"\b(?:\d{1,3}\.){3}\d{1,3}\b");
// Только полная форма из восьми групп и сжатая форма с двойным
// двоеточием: более свободный шаблон съедал бы время в самой записи
// журнала - 20:56:39 выглядит как три группы адреса.
static_regex!(
    ipv6_re,
    r"(?i)\b(?:[0-9a-f]{1,4}:){7}[0-9a-f]{1,4}\b|(?i)\b[0-9a-f]{0,4}::[0-9a-f:]{2,}\b"
);

fn replace_whole_match(
    re: &Regex,
    category: Category,
    salt: &[u8; 16],
    text: &str,
    counts: &mut ReplacementCounts,
) -> String {
    re.replace_all(text, |caps: &Captures<'_>| {
        counts.increment(category);
        alias(salt, category, &caps[0])
    })
    .into_owned()
}

fn replace_encoded_email(salt: &[u8; 16], text: &str, counts: &mut ReplacementCounts) -> String {
    encoded_email_re()
        .replace_all(text, |caps: &Captures<'_>| {
            counts.increment(Category::Email);
            // Для обычной и URL-кодированной записи одного адреса используется
            // один исходный текст, поэтому псевдоним внутри архива совпадает.
            let canonical = caps[0].replace("%40", "@");
            alias(salt, Category::Email, &canonical)
        })
        .into_owned()
}

fn replace_anchored_value(
    re: &Regex,
    category: Category,
    salt: &[u8; 16],
    text: &str,
    counts: &mut ReplacementCounts,
) -> String {
    re.replace_all(text, |caps: &Captures<'_>| {
        counts.increment(category);
        let whole = &caps[0];
        let value = &caps[1];
        let anchor = &whole[..whole.len() - value.len()];
        format!("{anchor}{}", alias(salt, category, value))
    })
    .into_owned()
}

fn replace_named_field(
    re: &Regex,
    salt: &[u8; 16],
    text: &str,
    counts: &mut ReplacementCounts,
) -> String {
    re.replace_all(text, |caps: &Captures<'_>| {
        let Some(category) = Category::from_field_name(&caps[1]) else {
            return caps[0].to_owned();
        };
        counts.increment(category);
        let raw_value = &caps[2];
        let quoted = raw_value.len() >= 2 && raw_value.starts_with('"') && raw_value.ends_with('"');
        let inner = if quoted {
            &raw_value[1..raw_value.len() - 1]
        } else {
            raw_value
        };
        let replaced = alias(salt, category, inner);
        let field = &caps[1];
        if quoted {
            format!("{field}=\"{replaced}\"")
        } else {
            format!("{field}={replaced}")
        }
    })
    .into_owned()
}

/// Обезличивает одну целиком прочитанную строку журнала (S-003): заменяет
/// каждое распознанное значение перечисленных категорий псевдонимом,
/// устойчивым в пределах архива (S-004) благодаря общей соли `salt`.
pub fn anonymize_line(line: &str, salt: &[u8; 16], counts: &mut ReplacementCounts) -> String {
    let mut text = line.to_owned();
    text = replace_whole_match(
        message_id_header_re(),
        Category::MessageIdHeader,
        salt,
        &text,
        counts,
    );
    text = replace_whole_match(uuid_re(), Category::Uuid, salt, &text, counts);
    text = replace_named_field(field_id_re(), salt, &text, counts);
    text = replace_named_field(field_folder_re(), salt, &text, counts);
    text = replace_named_field(field_content_re(), salt, &text, counts);
    // Домашний каталог - раньше общих шаблонов пути (F1): иначе общий шаблон
    // уже оборвал бы совпадение на первом пробеле имени пользователя.
    text = replace_anchored_value(windows_home_re(), Category::Path, salt, &text, counts);
    text = replace_anchored_value(unix_home_re(), Category::Path, salt, &text, counts);
    text = replace_anchored_value(macos_home_re(), Category::Path, salt, &text, counts);
    text = replace_anchored_value(windows_path_re(), Category::Path, salt, &text, counts);
    text = replace_anchored_value(unc_path_re(), Category::Path, salt, &text, counts);
    text = replace_anchored_value(unix_path_re(), Category::Path, salt, &text, counts);
    text = replace_encoded_email(salt, &text, counts);
    text = replace_whole_match(email_re(), Category::Email, salt, &text, counts);
    text = replace_whole_match(host_re(), Category::Host, salt, &text, counts);
    text = replace_whole_match(ipv4_re(), Category::Host, salt, &text, counts);
    text = replace_whole_match(ipv6_re(), Category::Host, salt, &text, counts);
    text
}

/// Декодирует байты как UTF-8, заменяя повреждённые последовательности
/// символом ASCII `?` (не U+FFFD - в архиве должны быть только символы
/// обычной клавиатуры). Используется на прочитанной по S-012 строке журнала.
pub fn lossy_utf8_question_mark(mut bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    loop {
        match std::str::from_utf8(bytes) {
            Ok(rest) => {
                out.push_str(rest);
                break;
            }
            Err(error) => {
                let valid_up_to = error.valid_up_to();
                out.push_str(
                    std::str::from_utf8(&bytes[..valid_up_to])
                        .expect("проверено выше границей ошибки"),
                );
                out.push('?');
                let invalid_len = error.error_len().unwrap_or(1);
                bytes = &bytes[valid_up_to + invalid_len..];
            }
        }
    }
    out
}

/// Одна строка журнала, прочитанная с пределом длины (S-012): `bytes` - не
/// более `MAX_LINE_BYTES` байт без завершающего перевода строки,
/// `truncated` - строка в файле была длиннее и остаток отброшен.
pub struct CappedLine {
    pub bytes: Vec<u8>,
    pub truncated: bool,
}

/// Читает одну строку из `reader` с пределом `cap` байт (S-012): остаток
/// строки сверх предела дочитывается и отбрасывается, а не копится в памяти
/// или в архиве. Возвращает `None` только на чистом конце файла (ничего не
/// прочитано вовсе).
pub fn read_capped_line<R: std::io::BufRead>(
    reader: &mut R,
    cap: usize,
) -> std::io::Result<Option<CappedLine>> {
    let mut buf = Vec::new();
    let mut truncated = false;
    let mut read_anything = false;
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            break;
        }
        read_anything = true;
        if let Some(pos) = available.iter().position(|&byte| byte == b'\n') {
            append_capped(&mut buf, &available[..pos], cap, &mut truncated);
            reader.consume(pos + 1);
            // Строка Windows (CRLF) не должна оставлять "\r" в конце.
            if buf.last() == Some(&b'\r') {
                buf.pop();
            }
            return Ok(Some(CappedLine {
                bytes: buf,
                truncated,
            }));
        }
        let len = available.len();
        append_capped(&mut buf, &available[..len], cap, &mut truncated);
        reader.consume(len);
    }
    if !read_anything {
        return Ok(None);
    }
    if buf.last() == Some(&b'\r') {
        buf.pop();
    }
    Ok(Some(CappedLine {
        bytes: buf,
        truncated,
    }))
}

fn append_capped(buf: &mut Vec<u8>, chunk: &[u8], cap: usize, truncated: &mut bool) {
    if buf.len() >= cap {
        if !chunk.is_empty() {
            *truncated = true;
        }
        return;
    }
    let room = cap - buf.len();
    if chunk.len() > room {
        buf.extend_from_slice(&chunk[..room]);
        *truncated = true;
    } else {
        buf.extend_from_slice(chunk);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn salt(byte: u8) -> [u8; 16] {
        [byte; 16]
    }

    #[test]
    fn replaces_every_listed_category() {
        let salt = salt(1);
        let mut counts = ReplacementCounts::default();
        let line = concat!(
            "аккаунт stanislav.chernov@ligastavok.ru подключился к imap.mail.ru, ",
            "путь C:\\Users\\st\\AppData\\Local\\truemail\\logs\\truemail.log, ",
            "collection=\"INBOX/Архив\" account_id=42 folder_id=7 message_id=99 uid=5 ",
            "uuid=550e8400-e29b-41d4-a716-446655440000 ",
            "message-id <abc123@mail.example.com>"
        );
        let out = anonymize_line(line, &salt, &mut counts);
        assert!(!out.contains("stanislav.chernov@ligastavok.ru"));
        assert!(!out.contains("imap.mail.ru"));
        assert!(!out.contains("C:\\Users\\st"));
        assert!(!out.contains("INBOX/Архив"));
        assert!(!out.contains("account_id=42"));
        assert!(!out.contains("folder_id=7"));
        assert!(!out.contains("message_id=99"));
        assert!(!out.contains("uid=5"));
        assert!(!out.contains("550e8400-e29b-41d4-a716-446655440000"));
        assert!(!out.contains("<abc123@mail.example.com>"));
        for tag in [
            "email",
            "host",
            "path",
            "folder",
            "account_id",
            "folder_id",
            "message_id",
            "uid",
            "uuid",
            "message_id_header",
        ] {
            assert!(
                counts.0.contains_key(tag),
                "категория {tag} не сработала: {out}"
            );
        }
    }

    #[test]
    fn same_value_same_alias_inside_one_archive() {
        // S-004: одно значение одной категории - один и тот же псевдоним в
        // разных строках одного архива (одна и та же соль).
        let salt = salt(7);
        let mut counts = ReplacementCounts::default();
        let first = anonymize_line("отправитель a@example.com пишет", &salt, &mut counts);
        let second = anonymize_line("получатель a@example.com читает", &salt, &mut counts);
        let alias_in = |s: &str| {
            s.split('[')
                .nth(1)
                .unwrap()
                .split(']')
                .next()
                .unwrap()
                .to_owned()
        };
        assert_eq!(alias_in(&first), alias_in(&second));
    }

    #[test]
    fn encoded_email_in_dav_url_is_replaced_as_one_value() {
        let salt = salt(17);
        let mut counts = ReplacementCounts::default();
        let line = "транспорт (dav): error sending request for url (https://mail.example/calendars/schernov%40yandex.ru/)";
        let out = anonymize_line(line, &salt, &mut counts);
        assert!(!out.contains("schernov"), "{out}");
        assert!(!out.contains("40yandex.ru"), "{out}");
        assert!(out.contains("/calendars/[email-"), "{out}");
    }

    #[test]
    fn plain_and_encoded_email_share_one_alias() {
        let salt = salt(18);
        let mut counts = ReplacementCounts::default();
        let plain = anonymize_line("schernov@yandex.ru", &salt, &mut counts);
        let encoded = anonymize_line("schernov%40yandex.ru", &salt, &mut counts);
        assert_eq!(plain, encoded);
    }

    #[test]
    fn message_content_and_rule_values_are_masked() {
        // mail-rules-conditions-and-actions.md, S-084: тема письма, значение
        // условия правила и текст ошибки очереди в архив открытым текстом не
        // попадают.
        let mut counts = ReplacementCounts::default();
        let line = anonymize_line(
            "rule_id=r1 value=\"Счет от ООО\" subject=\"Отчёт за месяц\" last_error=\"NO [OVERQUOTA] boss@example.test\"",
            &salt(9),
            &mut counts,
        );
        assert!(!line.contains("Счет от ООО"), "{line}");
        assert!(!line.contains("Отчёт за месяц"), "{line}");
        assert!(!line.contains("OVERQUOTA"), "{line}");
        assert!(line.contains("rule_id=r1"), "{line}");
    }

    #[test]
    fn sender_lists_and_conversation_metadata_are_masked() {
        // blocked-senders.md S-052, ignore-conversation.md S-053,
        // sweep-by-sender.md S-049: значения списков отправителей, адрес
        // записи автоочистки, тема и участники переписки и идентификатор
        // письма открытым текстом в архив не попадают.
        let mut counts = ReplacementCounts::default();
        let line = anonymize_line(
            "policy_id=7 policy_value=\"boss@example.test\" domain=\"spam.test\" participants=\"Иван Петров\" conversation_subject=\"Договор на поставку\" header=<abc123@example.test>",
            &salt(11),
            &mut counts,
        );
        assert!(!line.contains("boss@example.test"), "{line}");
        assert!(!line.contains("spam.test"), "{line}");
        assert!(!line.contains("Иван Петров"), "{line}");
        assert!(!line.contains("Договор на поставку"), "{line}");
        assert!(!line.contains("abc123"), "{line}");
        assert!(line.contains("policy_id=7"), "{line}");
    }

    #[test]
    fn mailbox_folder_field_is_replaced() {
        // Путь папки ящика приходит в журнал отдельным полем remote_path
        // (догрузка старых писем, удаление папки): без него структура ящика
        // читалась бы из архива как есть.
        let salt = salt(7);
        let mut counts = ReplacementCounts::default();
        let line = anonymize_line(
            "remote_path=\"INBOX/Клиенты/Договоры\" folder_id=3",
            &salt,
            &mut counts,
        );
        assert!(!line.contains("Клиенты"), "{line}");
        assert!(!line.contains("Договоры"), "{line}");
        assert!(line.contains("[folder-"), "{line}");
    }

    #[test]
    fn home_directory_with_space_in_username_is_fully_hidden() {
        // F1: имя пользователя "Ivan Petrov" содержит пробел - общий шаблон
        // пути обрывался бы на первом пробеле, оставляя фамилию в архиве.
        let salt = salt(11);
        let mut counts = ReplacementCounts::default();
        let line = anonymize_line(
            r"путь C:\Users\Ivan Petrov\AppData\Local\truemail\logs\truemail.log открыт",
            &salt,
            &mut counts,
        );
        assert!(!line.contains("Ivan"), "{line}");
        assert!(!line.contains("Petrov"), "{line}");
        assert!(line.contains("[path-"), "{line}");
    }

    #[test]
    fn unix_home_directory_with_space_in_username_is_fully_hidden() {
        let salt = salt(12);
        let mut counts = ReplacementCounts::default();
        let line = anonymize_line(
            "путь /home/Ivan Petrov/.config/truemail открыт",
            &salt,
            &mut counts,
        );
        assert!(!line.contains("Ivan"), "{line}");
        assert!(!line.contains("Petrov"), "{line}");
        assert!(line.contains("[path-"), "{line}");
    }

    #[test]
    fn macos_home_directory_with_space_in_username_is_fully_hidden() {
        let salt = salt(13);
        let mut counts = ReplacementCounts::default();
        let line = anonymize_line(
            "путь /Users/Ivan Petrov/Library/Application Support/truemail открыт",
            &salt,
            &mut counts,
        );
        assert!(!line.contains("Ivan"), "{line}");
        assert!(!line.contains("Petrov"), "{line}");
        assert!(line.contains("[path-"), "{line}");
    }

    #[test]
    fn bare_ip_address_is_replaced() {
        // Сервер могут задать голым адресом: он такой же узел, как и имя.
        let salt = salt(8);
        let mut counts = ReplacementCounts::default();
        let line = anonymize_line("соединение с 192.168.31.14 разорвано", &salt, &mut counts);
        assert!(!line.contains("192.168.31.14"), "{line}");
        assert!(line.contains("[host-"), "{line}");
    }

    #[test]
    fn timestamp_is_not_taken_for_an_address() {
        // Время в самой записи журнала похоже на сжатую запись адреса IPv6;
        // шаблон не должен его трогать, иначе архив станет нечитаемым.
        let salt = salt(9);
        let mut counts = ReplacementCounts::default();
        let line = anonymize_line("2026-09-16T20:56:39.123456Z INFO старт", &salt, &mut counts);
        assert!(line.contains("20:56:39"), "{line}");
    }

    #[test]
    fn different_archives_give_independent_aliases() {
        // S-004: разная соль - разный псевдоним для того же исходного значения.
        let mut counts_a = ReplacementCounts::default();
        let mut counts_b = ReplacementCounts::default();
        let a = anonymize_line("a@example.com", &salt(1), &mut counts_a);
        let b = anonymize_line("a@example.com", &salt(2), &mut counts_b);
        assert_ne!(a, b);
    }

    #[test]
    fn categories_are_salted_independently() {
        // Отпечаток категорий независим: тот же текст в роли email и в роли
        // произвольного узла не обязан давать одинаковый отпечаток, потому
        // что соль применяется вместе с именем категории.
        let salt = salt(3);
        let mut counts = ReplacementCounts::default();
        let email_alias = anonymize_line("x@example.com", &salt, &mut counts);
        let mut counts2 = ReplacementCounts::default();
        let host_alias = anonymize_line("example.com", &salt, &mut counts2);
        assert_ne!(email_alias, host_alias);
    }

    #[test]
    fn plain_text_without_categories_is_untouched() {
        let salt = salt(4);
        let mut counts = ReplacementCounts::default();
        let line = "IMAP incremental capabilities qresync=true condstore=true";
        let out = anonymize_line(line, &salt, &mut counts);
        assert_eq!(out, line);
        assert!(counts.0.is_empty());
    }

    #[test]
    fn lossy_utf8_replaces_broken_bytes_with_question_mark() {
        let mut bytes = b"before-".to_vec();
        bytes.push(0xff);
        bytes.extend_from_slice(b"-after");
        let decoded = lossy_utf8_question_mark(&bytes);
        assert_eq!(decoded, "before-?-after");
    }

    #[test]
    fn capped_line_truncates_and_marks_warning() {
        let cap = 16;
        let long_line = "x".repeat(40);
        let mut data = long_line.clone().into_bytes();
        data.push(b'\n');
        data.extend_from_slice(b"short\n");
        let mut reader = Cursor::new(data);
        let first = read_capped_line(&mut reader, cap).unwrap().unwrap();
        assert_eq!(first.bytes.len(), cap);
        assert!(first.truncated);
        let second = read_capped_line(&mut reader, cap).unwrap().unwrap();
        assert_eq!(second.bytes, b"short");
        assert!(!second.truncated);
        assert!(read_capped_line(&mut reader, cap).unwrap().is_none());
    }

    #[test]
    fn capped_line_handles_missing_trailing_newline() {
        let mut reader = Cursor::new(b"no-newline-at-end".to_vec());
        let line = read_capped_line(&mut reader, MAX_LINE_BYTES)
            .unwrap()
            .unwrap();
        assert_eq!(line.bytes, b"no-newline-at-end");
        assert!(!line.truncated);
    }

    #[test]
    fn capped_line_strips_crlf() {
        let mut reader = Cursor::new(b"line-one\r\nline-two".to_vec());
        let first = read_capped_line(&mut reader, MAX_LINE_BYTES)
            .unwrap()
            .unwrap();
        assert_eq!(first.bytes, b"line-one");
        let second = read_capped_line(&mut reader, MAX_LINE_BYTES)
            .unwrap()
            .unwrap();
        assert_eq!(second.bytes, b"line-two");
    }
}
