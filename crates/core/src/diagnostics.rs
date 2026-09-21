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
    /// Пароль, токен, ключ и заголовок авторизации (issue #111). Отдельная
    /// категория, а не содержимое письма: цена такой утечки другая, и число
    /// замен видно в manifest.json отдельной строкой. Ядро таких строк в
    /// журнал не пишет, но архив уходит наружу, и первая же отладочная
    /// строка со значением токена попала бы туда открытым текстом.
    Secret,
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
            Self::Secret => "secret",
        }
    }

    /// Категория по имени поля структурированной записи журнала
    /// (`tracing`-события пишут `key=value`); None - поле не из перечня.
    fn from_field_name(name: &str) -> Option<Self> {
        let lowered = name.to_ascii_lowercase();
        FIELD_CATEGORIES
            .iter()
            .find(|(field, _)| *field == lowered)
            .map(|(_, category)| *category)
    }
}

/// Имена полей записи журнала и их категории - одним перечнем, а не
/// ветвями `match`: тот же список нужен проверке полноты обезличивания, а
/// список, переписанный в проверке руками, молча разошёлся бы с разбором.
///
/// S-084: тема, предпросмотр, адреса, значение условия правила и текст
/// ошибки очереди в архив открытым текстом не попадают. Туда же идут
/// значения списков отправителей, адрес записи автоочистки, тема и
/// участники игнорируемой переписки (blocked-senders.md S-052,
/// ignore-conversation.md S-053, sweep-by-sender.md S-049).
const FIELD_CATEGORIES: &[(&str, Category)] = &[
    ("account_id", Category::AccountId),
    ("folder_id", Category::FolderId),
    ("message_id", Category::MessageId),
    ("uid", Category::Uid),
    ("collection", Category::Folder),
    ("folder", Category::Folder),
    ("mailbox", Category::Folder),
    ("remote_path", Category::Folder),
    ("subject", Category::Content),
    ("preview", Category::Content),
    ("body", Category::Content),
    ("value", Category::Content),
    ("rule_value", Category::Content),
    ("condition_value", Category::Content),
    ("from", Category::Content),
    ("from_addr", Category::Content),
    ("to", Category::Content),
    ("to_addrs", Category::Content),
    ("recipient", Category::Content),
    ("sender", Category::Content),
    ("last_error", Category::Content),
    ("policy_value", Category::Content),
    ("address", Category::Content),
    ("domain", Category::Content),
    ("participants", Category::Content),
    ("conversation_subject", Category::Content),
    ("password", Category::Secret),
    ("passwd", Category::Secret),
    ("pwd", Category::Secret),
    ("new_password", Category::Secret),
    ("old_password", Category::Secret),
    ("secret", Category::Secret),
    ("client_secret", Category::Secret),
    ("token", Category::Secret),
    ("access_token", Category::Secret),
    ("refresh_token", Category::Secret),
    ("id_token", Category::Secret),
    ("api_key", Category::Secret),
    ("apikey", Category::Secret),
    ("api_token", Category::Secret),
    ("key", Category::Secret),
    ("credential", Category::Secret),
    ("credentials", Category::Secret),
    ("authorization", Category::Secret),
    ("auth", Category::Secret),
    ("bearer", Category::Secret),
];

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
    r#"(?i)\b(subject|preview|body|value|rule_value|condition_value|from|from_addr|to|to_addrs|recipient|sender|last_error|policy_value|address|domain|participants|conversation_subject)\s*=\s*("[^"]*"|[^,;}]+)"#
);
// Поля, несущие секрет: пароль, токен, ключ, заголовок авторизации
// (issue #111). Разбираются раньше прочих полей и раньше адреса с узлом:
// значение токена бывает похоже на путь или на имя узла, и разобранное
// чужим шаблоном оно осталось бы в архиве наполовину.
static_regex!(
    field_secret_re,
    r#"(?i)\b(password|passwd|pwd|new_password|old_password|secret|client_secret|token|access_token|refresh_token|id_token|api_key|apikey|api_token|key|credential|credentials|authorization|auth|bearer)\s*[=:]\s*("[^"]*"|[^,;}]+)"#
);
// Заголовок авторизации в свободном тексте: "Authorization: Bearer abc",
// "Basic dXNlcjpwYXNz". Схема остаётся - по ней видно способ входа, - а само
// значение заменяется псевдонимом.
static_regex!(
    auth_header_re,
    r#"(?i)\b(bearer|basic|digest|ntlm|negotiate)\s+([A-Za-z0-9._~+/=-]{8,})"#
);
// Готовый псевдоним: значение, уже заменённое разбором выше. Второй раз его
// заменять нельзя - рядом с ним стоит то, что решено было сохранить (схема
// входа в заголовке авторизации).
static_regex!(alias_re, r"\[[a-z_]+-[0-9a-f]{6}\]");
// Начало следующего поля записи. Значение без кавычек доходит до разделителя
// записи, но внутри одной записи полей бывает несколько
// ("subject=Отчёт за месяц account_id=42"), и без этой границы обезличивание
// съело бы вместе с темой и чужие поля.
static_regex!(next_field_re, r#"(?i)\s+[a-z_][a-z0-9_]*\s*="#);
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
// Полная форма из восьми групп и сжатая форма с двойным двоеточием, причём
// хотя бы одна группа цифр обязательна с одной из сторон, а сам адрес
// ограничен символами, которые не могут быть его частью. Без этих границ
// шаблон резал бы обычные имена исходников: в "truemail_core::account"
// кусок "e::acc" состоит из тех же шестнадцатеричных букв и выглядел бы
// адресом. Время в самой записи (20:56:39) двойного двоеточия не содержит и
// сюда не попадает.
static_regex!(
    ipv6_re,
    r"(?i)(?:^|[^0-9a-z_.:])((?:[0-9a-f]{1,4}:){7}[0-9a-f]{1,4}|(?:[0-9a-f]{1,4}:)*[0-9a-f]{1,4}::(?:[0-9a-f]{1,4}:)*[0-9a-f]{1,4}|[0-9a-f]{1,4}(?::[0-9a-f]{1,4})*::|::[0-9a-f]{1,4}(?::[0-9a-f]{1,4})*)(?:$|[^0-9a-z_.:])"
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

/// Заменить значение, обрамлённое с обеих сторон: сами границы в замену не
/// входят и возвращаются на место. Нужно там, где значение нельзя опознать
/// без соседей - адрес IPv6 иначе совпадает внутри обычного имени исходника.
/// Заменить значение заголовка авторизации, оставив схему: по схеме видно,
/// каким способом шёл вход, а само значение - секрет (issue #111).
fn replace_auth_header(salt: &[u8; 16], text: &str, counts: &mut ReplacementCounts) -> String {
    auth_header_re()
        .replace_all(text, |caps: &Captures<'_>| {
            counts.increment(Category::Secret);
            format!("{} {}", &caps[1], alias(salt, Category::Secret, &caps[2]))
        })
        .into_owned()
}

fn replace_bounded_value(
    re: &Regex,
    category: Category,
    salt: &[u8; 16],
    text: &str,
    counts: &mut ReplacementCounts,
) -> String {
    re.replace_all(text, |caps: &Captures<'_>| {
        counts.increment(category);
        let whole = &caps[0];
        let value = caps.get(1).expect("значение внутри границ");
        let start = value.start() - caps.get(0).expect("совпадение").start();
        let left = &whole[..start];
        let right = &whole[start + value.as_str().len()..];
        format!("{left}{}{right}", alias(salt, category, value.as_str()))
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

/// Заменить значения именованных полей записи журнала псевдонимами.
///
/// Проход ручной, а не `replace_all`: значение без кавычек доходит до
/// разделителя записи, поэтому совпадение захватывает и хвост строки, а
/// `replace_all` продолжил бы поиск уже за ним. Второе содержательное поле той
/// же записи ("subject=Тема last_error=текст") осталось бы тогда в архиве
/// открытым текстом. Здесь поиск продолжается ровно с конца значения.
fn replace_named_field(
    re: &Regex,
    salt: &[u8; 16],
    text: &str,
    counts: &mut ReplacementCounts,
) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(caps) = re.captures(rest) {
        let whole = caps.get(0).expect("совпадение целиком");
        let value = caps.get(2).expect("значение поля");
        let raw_value = value.as_str();
        let quoted = raw_value.len() >= 2 && raw_value.starts_with('"') && raw_value.ends_with('"');
        let inner = if quoted {
            &raw_value[1..raw_value.len() - 1]
        } else {
            raw_value
        };
        // Начало следующего поля обрывает значение без кавычек: хвост за ним
        // принадлежит не этому полю и просматривается следующим витком.
        let inner = if quoted {
            inner
        } else {
            match next_field_re().find(inner) {
                Some(found) => &inner[..found.start()],
                None => inner,
            }
        };
        if alias_re().is_match(inner) {
            out.push_str(&rest[whole.start()..value.end()]);
            rest = &rest[value.end()..];
            continue;
        }
        let value_end = if quoted {
            value.end()
        } else {
            value.start() + inner.len()
        };
        out.push_str(&rest[..whole.start()]);
        match Category::from_field_name(&caps[1]) {
            Some(category) => {
                counts.increment(category);
                let replaced = alias(salt, category, inner);
                let field = &caps[1];
                if quoted {
                    out.push_str(&format!("{field}=\"{replaced}\""));
                } else {
                    out.push_str(&format!("{field}={replaced}"));
                }
            }
            // Поле не из словаря категорий: оставляем запись как есть.
            None => out.push_str(&rest[whole.start()..value_end]),
        }
        rest = &rest[value_end..];
    }
    out.push_str(rest);
    out
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
    // Секреты - раньше всех прочих шаблонов: значение токена похоже и на
    // путь, и на имя узла, и чужой шаблон унёс бы из него только часть.
    // Заголовок авторизации идёт первым: схема входа ("Bearer", "Basic")
    // остаётся видна, а поле целиком заменил бы разбор полей ниже.
    text = replace_auth_header(salt, &text, counts);
    text = replace_named_field(field_secret_re(), salt, &text, counts);
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
    text = replace_bounded_value(ipv6_re(), Category::Host, salt, &text, counts);
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

    /// Отпечаток из псевдонима "[<категория>-<отпечаток>]": сравнивать
    /// надо именно его, а не всю строку - разный префикс категории сам по себе
    /// делает строки разными даже при совпавшем отпечатке.
    fn alias_fingerprint(line: &str) -> String {
        let inside = line
            .split_once('[')
            .and_then(|(_, rest)| rest.split_once(']'))
            .map(|(inside, _)| inside)
            .unwrap_or_else(|| panic!("в строке нет псевдонима: {line}"));
        inside
            .rsplit_once('-')
            .unwrap_or_else(|| panic!("псевдоним без отпечатка: {line}"))
            .1
            .to_owned()
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
    fn every_known_log_field_value_is_replaced_by_an_alias() {
        // Перечень полей берётся из самого разбора (FIELD_CATEGORIES): поле,
        // которое разбор знает, а шаблон строки нет, ушло бы в архив открытым
        // текстом и об этом никто бы не узнал. В поле попадает и тема с
        // адресом внутри, и путь папки ящика, и текст ошибки сервера
        // (S-084, blocked-senders.md S-052, ignore-conversation.md S-053,
        // sweep-by-sender.md S-049).
        let salt = salt(9);
        let values = [
            "Счет от ООО boss@example.test",
            "INBOX/Клиенты/Договоры",
            "NO [OVERQUOTA] Иван Петров",
        ];
        for (field, category) in FIELD_CATEGORIES {
            for value in values {
                let mut counts = ReplacementCounts::default();
                let line = anonymize_line(
                    &format!("rule_id=r1 {field}=\"{value}\" правило применено"),
                    &salt,
                    &mut counts,
                );
                assert!(!line.contains(value), "{field}: {line}");
                assert!(
                    line.contains(&format!("{field}=\"[{}-", category.tag())),
                    "{field}: {line}"
                );
                assert_eq!(
                    counts.0.get(category.tag()),
                    Some(&1),
                    "{field}: счётчик категории для manifest.json, {line}"
                );
                // Служебное поле и текст записи остаются: без них архив
                // перестал бы годиться для разбора отказа.
                assert!(line.contains("rule_id=r1"), "{field}: {line}");
                assert!(line.contains("правило применено"), "{field}: {line}");
            }
        }
    }

    #[test]
    fn home_directory_with_a_space_in_the_user_name_is_fully_hidden() {
        // F1: имя пользователя "Ivan Petrov" содержит пробел - общий шаблон
        // пути обрывается на первом пробеле и оставил бы фамилию в архиве.
        // У каждой системы своя форма домашнего каталога и свой шаблон.
        let salt = salt(11);
        let cases = [
            r"путь C:\Users\Ivan Petrov\AppData\Local\truemail\logs\truemail.log открыт",
            "путь /home/Ivan Petrov/.config/truemail открыт",
            "путь /Users/Ivan Petrov/Library/Application Support/truemail открыт",
        ];
        for case in cases {
            let mut counts = ReplacementCounts::default();
            let line = anonymize_line(case, &salt, &mut counts);
            assert!(!line.contains("Ivan"), "{line}");
            assert!(!line.contains("Petrov"), "{line}");
            assert!(line.contains("[path-"), "{line}");
            assert!(line.starts_with("путь ["), "{line}");
        }
    }

    #[test]
    fn ipv6_server_address_is_replaced_whole() {
        // Сервер могут задать адресом IPv6, в том числе в сжатой форме с "::".
        // Заменять его надо целиком: половина адреса в архиве - это всё ещё
        // адрес сервера пользователя.
        let salt = salt(21);
        // Проверяем по самому адресу, а не по его кускам: псевдоним записан
        // шестнадцатеричными цифрами и может случайно содержать "db8" или
        // "2001" внутри себя, не раскрывая при этом ничего.
        let cases = [
            "2001:0db8:85a3:0000:0000:8a2e:0370:7334",
            "2001:db8::1",
            "fe80::1",
            "2001:db8:0:1::8a2e:370",
        ];
        for address in cases {
            let mut counts = ReplacementCounts::default();
            let case = format!("соединение с {address} разорвано");
            let line = anonymize_line(&case, &salt, &mut counts);
            assert!(!line.contains(address), "{line}");
            assert!(!line.contains("::"), "{line}");
            assert!(line.contains("[host-"), "{line}");
            assert!(line.starts_with("соединение с [host-"), "{line}");
            assert!(line.ends_with(" разорвано"), "{line}");
        }
    }

    #[test]
    fn a_secret_never_reaches_the_archive(){
        // Ядро таких строк в журнал не пишет, но архив уходит в поддержку, и
        // первая же отладочная строка со значением токена попала бы туда
        // открытым текстом - а заметить это было бы некому (issue #111).
        let salt = salt(25);
        let cases = [
            ("password=hunter2 account_id=42", "hunter2", "account_id="),
            ("access_token=ya29.AbCdEf-1234_xyz", "ya29.AbCdEf-1234_xyz", ""),
            ("refresh_token=\"1//04aBcD-eFgH\"", "1//04aBcD-eFgH", ""),
            ("api_key=sk-live-0123456789abcdef", "sk-live-0123456789abcdef", ""),
            ("client_secret=GOCSPX-abc_def, op=login", "GOCSPX-abc_def", "op=login"),
        ];
        for (source, secret, tail) in cases {
            let mut counts = ReplacementCounts::default();
            let line = anonymize_line(source, &salt, &mut counts);
            assert!(!line.contains(secret), "секрет остался в строке: {line}");
            assert!(line.contains("[secret-"), "{line}");
            if !tail.is_empty() {
                assert!(line.contains(tail), "соседнее поле съедено: {line}");
            }
        }

        // Заголовок авторизации: способ входа виден, значение - нет.
        let mut counts = ReplacementCounts::default();
        let line = anonymize_line(
            "запрос отклонён: Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.abc",
            &salt,
            &mut counts,
        );
        assert!(!line.contains("eyJhbGciOiJIUzI1NiJ9.abc"), "{line}");
        assert!(line.contains("Bearer [secret-"), "{line}");

        let mut counts = ReplacementCounts::default();
        let line = anonymize_line("proxy: Basic dXNlcjpwYXNzd29yZA==", &salt, &mut counts);
        assert!(!line.contains("dXNlcjpwYXNzd29yZA=="), "{line}");
        assert!(line.contains("Basic [secret-"), "{line}");

        // Псевдоним секрета отличается от псевдонима того же текста в другой
        // категории: иначе по архиву можно было бы связать их между собой.
        let mut counts = ReplacementCounts::default();
        let secret_line = anonymize_line("token=example.test", &salt, &mut counts);
        let host_line = anonymize_line("соединение с example.test", &salt, &mut counts);
        let secret_alias = secret_line.split("[secret-").nth(1).unwrap_or_default();
        let host_alias = host_line.split("[host-").nth(1).unwrap_or_default();
        assert_ne!(secret_alias, host_alias, "{secret_line} / {host_line}");
    }

    #[test]
    fn a_source_module_name_is_not_mistaken_for_an_address() {
        // Имя исходника в записи журнала содержит двойное двоеточие, а вокруг
        // него - те же шестнадцатеричные буквы ("truemail_core::account"):
        // шаблон адреса без границ вырезал бы из имени кусок "e::acc" и
        // читатель архива не нашёл бы источник записи.
        let salt = salt(24);
        let mut counts = ReplacementCounts::default();
        let line = anonymize_line(
            "truemail_core::account: синхронизация завершена",
            &salt,
            &mut counts,
        );
        assert_eq!(line, "truemail_core::account: синхронизация завершена");

        // Настоящий адрес рядом с именем исходника всё равно заменяется.
        let mut counts = ReplacementCounts::default();
        let line = anonymize_line(
            "truemail_core::backend: отказ узла fe80::1 при обращении",
            &salt,
            &mut counts,
        );
        assert!(line.starts_with("truemail_core::backend: "), "{line}");
        assert!(!line.contains("fe80::1"), "{line}");
        assert!(line.contains("[host-"), "{line}");
    }

    #[test]
    fn every_content_field_of_one_record_is_replaced() {
        // Значение без кавычек доходит до разделителя записи, поэтому
        // совпадение захватывает и хвост строки. Пока замена шла обходом всех
        // совпадений сразу, поиск продолжался уже за этим хвостом, и второе
        // содержательное поле той же записи оставалось в архиве открытым.
        let salt = salt(23);
        let mut counts = ReplacementCounts::default();
        let line = anonymize_line(
            "subject=Отчёт за месяц last_error=NO mailbox is full preview=первые строки",
            &salt,
            &mut counts,
        );
        assert!(!line.contains("Отчёт"), "{line}");
        assert!(!line.contains("mailbox"), "{line}");
        assert!(!line.contains("первые строки"), "{line}");
        assert_eq!(line.matches("[content-").count(), 3, "{line}");
    }

    #[test]
    fn field_value_with_spaces_is_replaced_to_the_end_of_the_value() {
        // Значение поля записи журнала не всегда берётся в кавычки, а тема или
        // текст ошибки почти всегда с пробелами: шаблон, обрывающийся на
        // первом пробеле, оставил бы хвост темы в архиве открытым текстом
        // (S-084). Граница значения - разделитель записи или начало
        // следующего поля "имя=", а не любой пробел.
        let salt = salt(22);
        let cases = [
            (
                "subject=Отчёт за месяц account_id=42",
                "Отчёт за месяц",
                "account_id=",
            ),
            (
                "last_error=NO [OVERQUOTA] mailbox is full, op=move",
                "NO [OVERQUOTA] mailbox is full",
                "op=move",
            ),
            ("value=Счет от ООО", "Счет от ООО", ""),
        ];
        for (source, secret, tail) in cases {
            let mut counts = ReplacementCounts::default();
            let line = anonymize_line(source, &salt, &mut counts);
            assert!(!line.contains(secret), "{line}");
            assert!(!line.contains("месяц"), "{line}");
            assert!(!line.contains("mailbox"), "{line}");
            assert!(!line.contains("ООО"), "{line}");
            if !tail.is_empty() {
                // Следующее поле записи не должно попасть внутрь значения:
                // иначе обезличивание съело бы всю оставшуюся строку.
                assert!(line.contains(tail), "{line}");
            }
        }
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
    fn one_value_in_two_categories_gets_two_different_fingerprints() {
        // S-004: отпечаток считается вместе с именем категории. Без этого
        // одно и то же значение, встретившееся в двух разных категориях, получило бы
        // одинаковый отпечаток, и читатель архива связал бы тему письма с именем
        // папки, а адрес в поле записи - с адресом в свободном тексте строки.
        let salt = salt(3);
        let cases = [
            ("folder=\"INBOX\"", "subject=\"INBOX\""),
            ("письмо от x@example.com", "from=\"x@example.com\""),
        ];
        for (first_line, second_line) in cases {
            let mut counts = ReplacementCounts::default();
            let first = alias_fingerprint(&anonymize_line(first_line, &salt, &mut counts));
            let second = alias_fingerprint(&anonymize_line(second_line, &salt, &mut counts));
            assert_ne!(first, second, "{first_line} и {second_line}");
        }
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
