//! Склейка соседних закодированных слов заголовка (RFC 2047) до разбора письма.
//!
//! Отправитель обязан резать заголовок по границам символов, но некоторые
//! рассыльщики режут по байтам. Разборщик писем декодирует каждое слово
//! отдельно, поэтому обрубки многобайтового символа превращаются в два символа
//! замены: тема "Сергеевич" доходит как "Сер??еевич" (issue #62).
//!
//! Здесь соседние слова с одной кодировкой склеиваются в одно ещё в сырых
//! байтах: их содержимое декодируется, объединяется и снова кодируется одним
//! словом. Разборщик получает целый символ и декодирует его правильно.
//! См. specs/split-encoded-words.md.

use base64::Engine as _;

/// Максимальная длина имени кодировки, которую считаем осмысленной. Всё, что
/// длиннее, - не заголовок, а совпадение символов "=?" в теле.
const MAX_CHARSET_LEN: usize = 64;

/// Склеить соседние закодированные слова в области заголовков письма.
///
/// Тело письма не трогается вовсе: оно может содержать что угодно, включая
/// последовательности, похожие на закодированные слова.
pub fn join_split_encoded_words(raw: &[u8]) -> std::borrow::Cow<'_, [u8]> {
    let headers_end = headers_end(raw);
    let (headers, body) = raw.split_at(headers_end);
    let Some(joined) = join_in_headers(headers) else {
        return std::borrow::Cow::Borrowed(raw);
    };
    let mut out = joined;
    out.extend_from_slice(body);
    std::borrow::Cow::Owned(out)
}

/// Конец области заголовков: пустая строка. Её нет - письмо состоит из одних
/// заголовков, и вся выборка считается заголовками.
fn headers_end(raw: &[u8]) -> usize {
    let mut i = 0;
    while i < raw.len() {
        if raw[i..].starts_with(b"\r\n\r\n") {
            return i + 2;
        }
        if raw[i..].starts_with(b"\n\n") {
            return i + 1;
        }
        i += 1;
    }
    raw.len()
}

/// Одно закодированное слово: границы в исходных байтах и его части.
struct EncodedWord {
    start: usize,
    end: usize,
    charset: Vec<u8>,
    encoding: u8,
    payload: Vec<u8>,
}

/// Разобрать закодированное слово, начинающееся в позиции `start`.
fn parse_encoded_word(data: &[u8], start: usize) -> Option<EncodedWord> {
    if !data[start..].starts_with(b"=?") {
        return None;
    }
    let charset_start = start + 2;
    let charset_end = find(data, charset_start, b'?')?;
    if charset_end - charset_start == 0 || charset_end - charset_start > MAX_CHARSET_LEN {
        return None;
    }
    let encoding = *data.get(charset_end + 1)?;
    if data.get(charset_end + 2) != Some(&b'?') {
        return None;
    }
    let payload_start = charset_end + 3;
    let payload_end = find_sequence(data, payload_start, b"?=")?;
    // Пробелы внутри слова запрещены: значит это не одно слово, а текст.
    if data[payload_start..payload_end]
        .iter()
        .any(|byte| byte.is_ascii_whitespace())
    {
        return None;
    }
    Some(EncodedWord {
        start,
        end: payload_end + 2,
        charset: data[charset_start..charset_end].to_ascii_lowercase(),
        encoding: encoding.to_ascii_lowercase(),
        payload: data[payload_start..payload_end].to_vec(),
    })
}

fn find(data: &[u8], from: usize, needle: u8) -> Option<usize> {
    data.get(from..)?
        .iter()
        .position(|byte| *byte == needle)
        .map(|offset| from + offset)
}

fn find_sequence(data: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    let mut i = from;
    while i + needle.len() <= data.len() {
        if data[i..].starts_with(needle) {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Пропустить разделитель между соседними словами: пробелы, а также перенос
/// свёрнутой строки заголовка. Возвращает позицию следующего слова, если между
/// словами нет ничего, кроме пробельных символов.
fn skip_word_separator(data: &[u8], from: usize) -> Option<usize> {
    let mut i = from;
    let mut seen_space = false;
    while let Some(byte) = data.get(i) {
        match byte {
            b' ' | b'\t' => {
                seen_space = true;
                i += 1;
            }
            b'\r' | b'\n' => {
                seen_space = true;
                i += 1;
            }
            _ => break,
        }
    }
    if seen_space && data.get(i..)?.starts_with(b"=?") {
        Some(i)
    } else {
        None
    }
}

/// Декодировать содержимое слова в байты.
fn decode_payload(word: &EncodedWord) -> Option<Vec<u8>> {
    match word.encoding {
        b'b' => base64::engine::general_purpose::STANDARD
            .decode(&word.payload)
            .ok()
            .or_else(|| {
                base64::engine::general_purpose::STANDARD_NO_PAD
                    .decode(&word.payload)
                    .ok()
            }),
        b'q' => decode_quoted_printable(&word.payload),
        _ => None,
    }
}

/// Печатаемое представление RFC 2047: "_" - пробел, "=XX" - байт.
fn decode_quoted_printable(payload: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(payload.len());
    let mut i = 0;
    while i < payload.len() {
        match payload[i] {
            b'_' => {
                out.push(b' ');
                i += 1;
            }
            b'=' => {
                let high = hex_value(*payload.get(i + 1)?)?;
                let low = hex_value(*payload.get(i + 2)?)?;
                out.push(high * 16 + low);
                i += 3;
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    Some(out)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Собрать одно закодированное слово из объединённых байтов. Кодировку берём
/// base64: она не зависит от того, какие байты внутри.
fn build_encoded_word(charset: &[u8], bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(charset.len() + bytes.len() * 2 + 8);
    out.extend_from_slice(b"=?");
    out.extend_from_slice(charset);
    out.extend_from_slice(b"?B?");
    out.extend_from_slice(
        base64::engine::general_purpose::STANDARD
            .encode(bytes)
            .as_bytes(),
    );
    out.extend_from_slice(b"?=");
    out
}

/// Пройти по заголовкам и склеить группы соседних слов. Возвращает None, если
/// склеивать было нечего: тогда письмо остаётся прежним, без лишнего копирования.
fn join_in_headers(headers: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(headers.len());
    let mut i = 0;
    let mut changed = false;
    while i < headers.len() {
        let Some(first) = parse_encoded_word(headers, i) else {
            out.push(headers[i]);
            i += 1;
            continue;
        };
        // Собираем всю группу подряд идущих слов с той же кодировкой.
        let mut bytes = match decode_payload(&first) {
            Some(bytes) => bytes,
            None => {
                out.extend_from_slice(&headers[first.start..first.end]);
                i = first.end;
                continue;
            }
        };
        let mut end = first.end;
        let mut joined = false;
        while let Some(next_start) = skip_word_separator(headers, end) {
            let Some(next) = parse_encoded_word(headers, next_start) else {
                break;
            };
            if next.charset != first.charset || next.encoding != first.encoding {
                break;
            }
            let Some(next_bytes) = decode_payload(&next) else {
                break;
            };
            bytes.extend_from_slice(&next_bytes);
            end = next.end;
            joined = true;
        }
        if joined {
            out.extend_from_slice(&build_encoded_word(&first.charset, &bytes));
            changed = true;
        } else {
            out.extend_from_slice(&headers[first.start..first.end]);
        }
        i = end;
    }
    if changed { Some(out) } else { None }
}

#[cfg(test)]
mod tests {
    use super::join_split_encoded_words;

    /// Настоящий путь склейки: письмо проходит через join_split_encoded_words и
    /// попадает в разборщик, как в хранилище. Сверяется декодированная тема, а
    /// не промежуточные байты: промежуточное представление - внутреннее дело
    /// склейки, а ценность проверки в том, что пользователь видит целые слова.
    fn subject_after_join(raw: &str) -> String {
        let joined = join_split_encoded_words(raw.as_bytes());
        let message = mail_parser::MessageParser::default()
            .parse(joined.as_ref())
            .expect("письмо разбирается");
        message.subject().unwrap_or_default().to_string()
    }

    /// Слова одной кодировки склеиваются: обрубки многобайтового символа
    /// собираются обратно, и тема читается целиком (S-001, S-002). Без склейки
    /// разборщик декодирует каждое слово отдельно и на месте разрезанного
    /// символа ставит символы замены.
    #[test]
    fn split_words_are_joined_into_a_readable_subject() {
        // "Сергеевич" разрезан посреди байтов буквы "р": первое слово
        // заканчивается байтом 0xD1, второе начинается с 0x80.
        let cases = [
            (
                "base64 в одной строке",
                concat!(
                    "Subject: =?UTF-8?B?0KHQtdE=?= =?UTF-8?B?gNCz0LXQtdCy0LjRhw==?=\r\n",
                    "\r\n",
                    "body"
                ),
            ),
            (
                "свёрнутая строка заголовка",
                concat!(
                    "Subject: =?UTF-8?B?0KHQtdE=?=\r\n",
                    " =?UTF-8?B?gNCz0LXQtdCy0LjRhw==?=\r\n",
                    "\r\n",
                    "body"
                ),
            ),
            (
                "печатаемое представление",
                concat!(
                    "Subject: =?UTF-8?Q?=D0=A1=D0=B5=D1?=",
                    " =?UTF-8?Q?=80=D0=B3=D0=B5=D0=B5=D0=B2=D0=B8=D1=87?=\r\n",
                    "\r\n",
                    "body"
                ),
            ),
        ];
        for (name, raw) in cases {
            assert_eq!(subject_after_join(raw), "Сергеевич", "случай: {name}");
        }

        // Письмо без пустой строки состоит из одних заголовков: склейка обязана
        // работать и там, иначе тема теряется при разборе заголовочной выборки.
        let headers_only = "Subject: =?UTF-8?B?0KHQtdE=?= =?UTF-8?B?gNCz0LXQtdCy0LjRhw==?=";
        let joined = join_split_encoded_words(headers_only.as_bytes()).into_owned();
        let with_body = [joined.as_slice(), b"\r\n\r\nbody"].concat();
        let message = mail_parser::MessageParser::default()
            .parse(with_body.as_slice())
            .expect("письмо разбирается");
        assert_eq!(message.subject().unwrap_or_default(), "Сергеевич");
    }

    /// Склеивать можно только соседние слова одной кодировки. Остальные случаи
    /// обязаны дойти до разборщика ровно теми же байтами: склейка чужих байтов
    /// превратила бы тему в мусор, а потеря слова - в пустую строку (S-003,
    /// S-004, S-006, S-007).
    #[test]
    fn words_that_must_not_be_joined_reach_the_parser_unchanged() {
        let cases = [
            (
                "разные кодировки",
                "Subject: =?UTF-8?B?0KHQtdGA?= =?koi8-r?B?98XU?=\r\n\r\nbody",
                "СерВет",
            ),
            (
                "между словами обычный текст",
                "Subject: =?UTF-8?B?0KHQtdGA?= и =?UTF-8?B?0LPQtdC1?=\r\n\r\nbody",
                "Сер и гее",
            ),
            (
                "одно слово",
                "Subject: =?UTF-8?B?0KHQtdGA?=\r\n\r\nbody",
                "Сер",
            ),
        ];
        for (name, raw, subject) in cases {
            let joined = join_split_encoded_words(raw.as_bytes());
            assert_eq!(joined.as_ref(), raw.as_bytes(), "случай: {name}");
            assert_eq!(subject_after_join(raw), subject, "случай: {name}");
        }
    }

    /// Тело письма не трогаем: там встречается что угодно, включая похожие
    /// последовательности (S-005).
    #[test]
    fn leaves_the_body_untouched() {
        let raw = concat!(
            "Subject: тема\r\n",
            "\r\n",
            "=?UTF-8?B?0KHQtdGA?= =?UTF-8?B?0LPQtdC1?=\r\n"
        );
        let joined = join_split_encoded_words(raw.as_bytes());
        assert_eq!(joined.as_ref(), raw.as_bytes());
    }

    /// Испорченное слово оставляем нетронутым: чинить нечего, а терять
    /// содержимое заголовка нельзя (S-007).
    #[test]
    fn leaves_broken_word_untouched() {
        let raw = "Subject: =?UTF-8?B?!!!не base64!!!?= =?UTF-8?B?0KHQtdGA?=\r\n\r\nbody";
        let joined = join_split_encoded_words(raw.as_bytes());
        assert_eq!(joined.as_ref(), raw.as_bytes());
    }
}
