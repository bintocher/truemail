//! Очередь отправки: происхождение письма, состояния операции, формат данных
//! операции и границы окна отмены (specs/undo-send.md).
//!
//! Тело и вложения в данных операции не лежат: письмо предельного размера дало
//! бы порядка 35 МБ строки, а очередь перечитывается на каждом проходе
//! работника (S-006). В данных остаются только ссылки на хранилище больших
//! объектов.

use super::{LIMIT_UNDO_SEND_MAX, LIMIT_UNDO_SEND_MIN, LimitSet};
use serde::{Deserialize, Serialize};

/// Происхождение отправки (S-055, S-057 - S-059).
pub const SEND_ORIGIN_ORDINARY: &str = "ordinary";
pub const SEND_ORIGIN_SCHEDULED: &str = "scheduled";
pub const SEND_ORIGIN_AUTOMATIC: &str = "automatic";
pub const SEND_ORIGIN_EXTERNAL: &str = "external";

/// Состояния операции отправки. Первые четыре уже были у очереди операций,
/// два последних добавлены этой задачей и допускаются только у отправки (S-030).
pub const SEND_STATUS_PENDING: &str = "pending";
pub const SEND_STATUS_PROCESSING: &str = "processing";
pub const SEND_STATUS_RETRY: &str = "retry";
pub const SEND_STATUS_FAILED: &str = "failed";
pub const SEND_STATUS_CANCELLED: &str = "cancelled";
pub const SEND_STATUS_UNCERTAIN: &str = "uncertain";

/// Ключ настройки длительности окна отмены. Значение одно на все ящики (S-014).
pub const UNDO_SEND_SETTING: &str = "undo_send_seconds";
// Длительность окна отмены по умолчанию (S-011), её границы (S-012) и срок
// хранения ключа запроса (S-042, S-054) задаются настройками: ключи
// LIMIT_UNDO_SEND_DEFAULT, LIMIT_UNDO_SEND_MIN, LIMIT_UNDO_SEND_MAX,
// LIMIT_REQUEST_KEY_DAYS и LIMIT_PURGE_BATCH в crates/core/src/model/limits.rs.
// Прежде границы окна отмены жили ещё и в модуле интерфейса, и третьей копией
// в атрибутах min/max самой разметки.

/// Номер формата данных операции отправки. Формат 1 - прежняя отложенная
/// отправка: в нём лежит само письмо, а не ссылки (S-046).
pub const SEND_PAYLOAD_VERSION: u32 = 2;

/// Проверить длительность окна отмены. Границы интерфейс получает отсюда же,
/// а решение принимает ядро (S-013).
pub fn validate_undo_seconds(value: i64, limits: &LimitSet) -> Result<i64, String> {
    let min = limits.get(LIMIT_UNDO_SEND_MIN);
    let max = limits.get(LIMIT_UNDO_SEND_MAX);
    if !(min..=max).contains(&value) {
        return Err(format!(
            "окно отмены задаётся целым числом секунд от {min} до {max}"
        ));
    }
    Ok(value)
}

/// Вложение исходящего письма в данных операции: байты лежат в хранилище
/// больших объектов, в данных остаётся ссылка.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendPayloadAttachment {
    pub filename: String,
    pub mime_type: String,
    pub blob_ref: String,
    pub size: i64,
}

/// Данные операции отправки формата 2.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendPayload {
    pub version: u32,
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    /// Закреплённый идентификатор письма: один и тот же при всех повторах
    /// этого запроса (S-045).
    pub message_id: String,
    /// Ссылка на тело письма в хранилище больших объектов.
    pub body_ref: String,
    #[serde(default)]
    pub attachments: Vec<SendPayloadAttachment>,
    /// Заголовки служебного письма: автоответ помечает себя ими (S-056 - S-058
    /// спецификации автоответа).
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    /// Точные байты письма, переданные серверу. Появляются только у операции
    /// дозаписи копии: повторная сборка MIME дала бы другие границы частей.
    #[serde(default)]
    pub raw_ref: Option<String>,
}

impl SendPayload {
    /// Все ссылки операции на хранилище больших объектов: по ним сборка мусора
    /// узнаёт достижимые объекты, а удаление операции - что стирать (S-007).
    pub fn blob_refs(&self) -> Vec<String> {
        let mut refs = vec![self.body_ref.clone()];
        refs.extend(self.attachments.iter().map(|item| item.blob_ref.clone()));
        refs.extend(self.raw_ref.clone());
        refs.retain(|item| !item.is_empty());
        refs
    }
}

/// Тело письма в хранилище больших объектов.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SendBody {
    pub body_text: String,
    pub body_html: Option<String>,
}

/// Итог приёма письма в очередь: его показывает композер и по нему строится
/// карточка окна отмены.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendQueued {
    pub operation_id: i64,
    pub account_id: i64,
    pub status: String,
    pub cancel_until: String,
    pub undo_seconds: i64,
    /// Повторное нажатие с тем же ключом запроса второй отправки не создаёт
    /// (S-053).
    pub duplicate: bool,
}

/// Строка раздела "Исходящие". Тело и вложения сюда не попадают: интерфейсу
/// для списка нужны только тема, адресаты, состояние и сроки.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboxSendEntry {
    pub id: i64,
    pub account_id: i64,
    pub account_email: String,
    /// Выключенный ящик работника очереди не запускает, и это сказано прямо
    /// (S-029).
    pub account_enabled: bool,
    pub origin: String,
    pub status: String,
    pub subject: String,
    pub to: Vec<String>,
    pub cancel_until: Option<String>,
    pub next_attempt_at: Option<String>,
    pub attempts: i64,
    pub last_error: Option<String>,
    pub created_at: String,
    pub attachments: i64,
}

/// Отменённое письмо, возвращаемое в композер целиком (S-040).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelledSendMessage {
    pub operation_id: i64,
    pub account_id: i64,
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub body_text: String,
    pub body_html: Option<String>,
    pub attachments: Vec<CancelledSendAttachment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelledSendAttachment {
    pub filename: String,
    pub mime_type: String,
    pub data: Vec<u8>,
}

/// Исход отмены отправки: отмена спорит за одну строку с захватом работника, и
/// победить может только одно действие (S-037, S-038). Проигравшей отмене
/// называется настоящее состояние операции: неотправленное письмо не выдаётся
/// за отправленное (S-064).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancelSendOutcome {
    /// Письмо возвращено пользователю и серверу передано не будет.
    Cancelled,
    /// Работник успел начать передачу: обещать отзыв программа не вправе.
    AlreadySending,
    /// Сервер уже подтвердил принятие письма либо операции больше нет.
    AlreadySent,
    /// Итог передачи неизвестен, и решение о нём принимает пользователь.
    Uncertain,
    /// Отправка окончательно отказала: отменять нечего.
    AlreadyFailed,
    /// Операция уже отменена - обычно повторным нажатием.
    AlreadyCancelled,
}

/// Число писем с истёкшим окном отмены на запуске программы: показывается одним
/// сообщением, а не карточкой на каждое письмо (S-036).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StartupSendState {
    pub expired: i64,
    pub pending: Vec<SendQueued>,
    pub uncertain: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// S-012, S-013: границы окна отмены проверяет ядро, а не только
    /// интерфейс, и берёт их из настроек. Сами границы на краях допустимы,
    /// соседние с ними значения - нет.
    #[test]
    fn undo_window_accepts_only_whole_seconds_within_its_bounds() {
        let limits = LimitSet::defaults();
        let min = limits.get(LIMIT_UNDO_SEND_MIN);
        let max = limits.get(LIMIT_UNDO_SEND_MAX);
        assert_eq!(validate_undo_seconds(min, &limits), Ok(min));
        assert_eq!(validate_undo_seconds(max, &limits), Ok(max));
        assert!(validate_undo_seconds(min - 1, &limits).is_err());
        assert!(validate_undo_seconds(max + 1, &limits).is_err());
    }

    /// S-007: сборка мусора узнаёт достижимые объекты по данным операции,
    /// поэтому в перечень ссылок входят и тело, и каждое вложение, и точные
    /// байты письма у дозаписи копии.
    #[test]
    fn payload_lists_every_blob_reference_it_owns() {
        let payload = SendPayload {
            version: SEND_PAYLOAD_VERSION,
            from: "me@example.test".into(),
            to: vec!["you@example.test".into()],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: "тема".into(),
            message_id: "<id@example.test>".into(),
            body_ref: "ab/body".into(),
            attachments: vec![SendPayloadAttachment {
                filename: "отчёт.pdf".into(),
                mime_type: "application/pdf".into(),
                blob_ref: "cd/attachment".into(),
                size: 10,
            }],
            headers: Vec::new(),
            raw_ref: Some("ef/raw".into()),
        };
        assert_eq!(
            payload.blob_refs(),
            vec![
                "ab/body".to_owned(),
                "cd/attachment".to_owned(),
                "ef/raw".to_owned()
            ]
        );
    }
}
