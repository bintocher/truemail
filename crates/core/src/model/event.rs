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
    pub transp: Option<Transp>,
    pub class: Option<EventClass>,
    pub categories: Vec<String>,
    pub url: Option<String>,
    pub organizer: Option<String>,
    pub sequence: i64,
}
