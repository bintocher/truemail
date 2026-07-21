//! Событие календаря (RFC 5545, iCalendar VEVENT). Простой набор + Эксперт-поля.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Transp {
    /// Занят
    Opaque,
    /// Свободен
    Transparent,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EventClass {
    Public,
    Private,
    Confidential,
}

/// STATUS встречи (RFC5545). Читаем то же значение, что пишем в БД: провайдеры
/// отдают его в разном регистре (Google - lowercase, CalDAV/EWS - uppercase),
/// поэтому разбор регистронезависимый.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EventStatus {
    Confirmed,
    Tentative,
    Cancelled,
}

impl EventStatus {
    pub fn parse(value: &str) -> Option<Self> {
        if value.eq_ignore_ascii_case("CONFIRMED") {
            Some(Self::Confirmed)
        } else if value.eq_ignore_ascii_case("TENTATIVE") {
            Some(Self::Tentative)
        } else if value.eq_ignore_ascii_case("CANCELLED") {
            Some(Self::Cancelled)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attendee {
    pub email: String,
    pub name: Option<String>,
    /// REQ-PARTICIPANT | OPT-PARTICIPANT | CHAIR
    pub role: Option<String>,
    /// NEEDS-ACTION | ACCEPTED | DECLINED | TENTATIVE
    pub partstat: Option<String>,
    pub rsvp: bool,
}

/// Ответ пользователя на приглашение (подмножество PARTSTAT из RFC 5545,
/// без NEEDS-ACTION - это отсутствие ответа, а не сам ответ - и без
/// DELEGATED - делегирование в этом релизе не реализуем, только читаем его
/// как есть, если сервер прислал).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RsvpResponse {
    Accepted,
    Declined,
    Tentative,
}

impl RsvpResponse {
    /// Значение PARTSTAT, которое пишем во все протоколы.
    pub fn partstat(self) -> &'static str {
        match self {
            Self::Accepted => "ACCEPTED",
            Self::Declined => "DECLINED",
            Self::Tentative => "TENTATIVE",
        }
    }

    /// Разбор значения, приходящего от фронтенда ("accepted"/"declined"/"tentative").
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "accepted" => Some(Self::Accepted),
            "declined" => Some(Self::Declined),
            "tentative" => Some(Self::Tentative),
            _ => None,
        }
    }
}

/// Найти среди участников события того, кто отвечает за аккаунт с адресом
/// `account_email`, и вычислить удобное для UI представление: свой PARTSTAT
/// (по умолчанию NEEDS-ACTION, см. RFC5545) и признак, что ответ вообще
/// нужен показывать кнопками. Сопоставление по email регистронезависимое;
/// алиасов у аккаунта модель пока не знает (см. `Account` в account.rs) -
/// когда они появятся, сюда нужно будет передать список адресов, а не один.
///
/// Организатором считаем как совпадение `organizer` (поле события) с адресом
/// аккаунта, так и роль CHAIR у самой записи участника - на случай, если
/// сервер не прислал ORGANIZER, но выставил роль. Ответ нужен, только когда
/// пользователь есть среди участников, не организатор, и у его записи
/// RSVP=TRUE - иначе кнопки появлялись бы на информационных календарях и на
/// собственных встречах.
pub fn resolve_my_attendance(
    attendees: &[Attendee],
    organizer: Option<&str>,
    account_email: &str,
) -> (Option<String>, bool) {
    let Some(mine) = attendees
        .iter()
        .find(|attendee| attendee.email.eq_ignore_ascii_case(account_email))
    else {
        return (None, false);
    };
    let is_organizer = organizer.is_some_and(|value| value.eq_ignore_ascii_case(account_email))
        || mine.role.as_deref() == Some("CHAIR");
    let partstat = mine
        .partstat
        .clone()
        .unwrap_or_else(|| "NEEDS-ACTION".into());
    // Намеренно не смотрим на RSVP: у Google он вычисляется из responseStatus
    // (rsvp = "ещё не ответил"), поэтому после первого же ответа кнопки
    // исчезли бы и передумать стало бы нельзя. Право ответить есть у любого
    // участника, кроме организатора, независимо от того, ответил он раньше.
    let needs_response = !is_organizer;
    (Some(partstat), needs_response)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alarm {
    /// За сколько минут до начала
    pub trigger_minutes: i32,
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: Option<i64>,
    pub calendar_id: i64,
    pub uid: Option<String>,

    // Простой набор
    pub summary: String,
    pub description: Option<String>,
    pub location: Option<String>,
    /// ISO 8601
    pub dtstart: String,
    pub dtend: Option<String>,
    pub all_day: bool,
    pub attendees: Vec<Attendee>,
    pub alarms: Vec<Alarm>,
    /// RFC5545 RRULE
    pub rrule: Option<String>,
    pub recurrence_id: Option<String>,
    pub exdates: Option<String>,
    pub rdates: Option<String>,

    // Эксперт-набор (RFC 5545)
    pub timezone: Option<String>,
    /// CONFIRMED | TENTATIVE | CANCELLED. Раньше колонка status в БД была
    /// write-only - теперь читаем её, чтобы не напоминать об отменённых
    /// встречах и (на следующем этапе) показывать их статус в интерфейсе.
    pub status: Option<EventStatus>,
    pub transp: Option<Transp>,
    pub class: Option<EventClass>,
    pub categories: Vec<String>,
    pub url: Option<String>,
    pub organizer: Option<String>,
    pub sequence: i64,

    // Вычисляется при чтении из БД (см. list_calendars_and_events/event_for_response
    // в storage/repo.rs) сопоставлением участников с адресом аккаунта, которому
    // принадлежит календарь - удобное представление для UI, самим полям события
    // не принадлежит и на сервер не пишется.
    /// Свой PARTSTAT; None, если пользователя нет среди участников.
    pub my_partstat: Option<String>,
    /// Показывать ли кнопки "Пойду/Не пойду/Возможно" - см. resolve_my_attendance.
    pub needs_response: bool,
}

#[cfg(test)]
mod attendance_tests {
    use super::*;

    fn attendee(email: &str, role: Option<&str>, partstat: Option<&str>, rsvp: bool) -> Attendee {
        Attendee {
            email: email.into(),
            name: None,
            role: role.map(str::to_owned),
            partstat: partstat.map(str::to_owned),
            rsvp,
        }
    }

    #[test]
    fn matches_own_attendee_case_insensitively() {
        let attendees = vec![attendee(
            "User@Example.Test",
            Some("REQ-PARTICIPANT"),
            Some("TENTATIVE"),
            true,
        )];
        let (partstat, needs_response) =
            resolve_my_attendance(&attendees, Some("owner@example.test"), "user@example.test");
        assert_eq!(partstat.as_deref(), Some("TENTATIVE"));
        assert!(needs_response);
    }

    #[test]
    fn absent_from_attendees_needs_no_response() {
        let attendees = vec![attendee(
            "other@example.test",
            Some("REQ-PARTICIPANT"),
            None,
            true,
        )];
        let (partstat, needs_response) =
            resolve_my_attendance(&attendees, Some("owner@example.test"), "user@example.test");
        assert_eq!(partstat, None);
        assert!(!needs_response);
    }

    #[test]
    fn organizer_never_needs_to_respond_even_with_rsvp() {
        let attendees = vec![attendee(
            "user@example.test",
            Some("CHAIR"),
            Some("ACCEPTED"),
            true,
        )];
        // ORGANIZER не совпадает (например, не пришёл от сервера), но роль CHAIR
        // всё равно должна распознать организатора.
        let (partstat, needs_response) =
            resolve_my_attendance(&attendees, None, "user@example.test");
        assert_eq!(partstat.as_deref(), Some("ACCEPTED"));
        assert!(!needs_response);
    }

    #[test]
    fn already_answered_participant_can_still_change_the_answer() {
        // У Google rsvp означает "ещё не ответил" и после ответа становится
        // false. Кнопки от этого зависеть не должны, иначе передумать нельзя.
        let attendees = vec![attendee(
            "user@example.test",
            Some("REQ-PARTICIPANT"),
            Some("ACCEPTED"),
            false,
        )];
        let (partstat, needs_response) =
            resolve_my_attendance(&attendees, Some("owner@example.test"), "user@example.test");
        assert_eq!(partstat.as_deref(), Some("ACCEPTED"));
        assert!(needs_response);
    }

    #[test]
    fn missing_partstat_defaults_to_needs_action() {
        let attendees = vec![attendee("user@example.test", None, None, true)];
        let (partstat, needs_response) =
            resolve_my_attendance(&attendees, Some("owner@example.test"), "user@example.test");
        assert_eq!(partstat.as_deref(), Some("NEEDS-ACTION"));
        assert!(needs_response);
    }

    #[test]
    fn rsvp_response_maps_to_rfc5545_partstat_and_parses_back() {
        assert_eq!(RsvpResponse::Accepted.partstat(), "ACCEPTED");
        assert_eq!(RsvpResponse::Declined.partstat(), "DECLINED");
        assert_eq!(RsvpResponse::Tentative.partstat(), "TENTATIVE");
        assert_eq!(
            RsvpResponse::parse("accepted"),
            Some(RsvpResponse::Accepted)
        );
        assert_eq!(
            RsvpResponse::parse("declined"),
            Some(RsvpResponse::Declined)
        );
        assert_eq!(
            RsvpResponse::parse("tentative"),
            Some(RsvpResponse::Tentative)
        );
        assert_eq!(RsvpResponse::parse("needs-action"), None);
    }
}
