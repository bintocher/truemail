//! Синхронизация Google Calendar, Contacts и Tasks через REST API.

use super::dav::{
    AuxiliarySyncCursors, DavCalendar, DavContact, DavEvent, DavSyncResult, SyncScope,
};
use crate::model::{Alarm, Attendee, ContactPhone, clean_contact_name};
use crate::{Error, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use url::Url;

const CALENDAR_BASE: &str = "https://www.googleapis.com/calendar/v3";
const PEOPLE_CONNECTIONS: &str = "https://people.googleapis.com/v1/people/me/connections";
const TASKS_BASE: &str = "https://tasks.googleapis.com/tasks/v1";
const CALENDAR_CURSOR_PREFIX: &str = "google-calendar-v1:";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GoogleCalendarCursor {
    sync_token: String,
    last_full: String,
    consecutive_expired: u8,
}

fn decode_calendar_cursor(value: Option<&str>) -> Option<GoogleCalendarCursor> {
    let value = value?;
    if let Some(json) = value.strip_prefix(CALENDAR_CURSOR_PREFIX) {
        serde_json::from_str(json).ok()
    } else {
        Some(GoogleCalendarCursor {
            sync_token: value.to_owned(),
            last_full: String::new(),
            consecutive_expired: 0,
        })
    }
}

fn encode_calendar_cursor(cursor: &GoogleCalendarCursor) -> Result<String> {
    Ok(format!(
        "{CALENDAR_CURSOR_PREFIX}{}",
        serde_json::to_string(cursor)?
    ))
}

fn calendar_full_is_fresh(cursor: &GoogleCalendarCursor) -> bool {
    chrono::DateTime::parse_from_rfc3339(&cursor.last_full)
        .map(|last| {
            chrono::Utc::now().signed_duration_since(last.with_timezone(&chrono::Utc))
                < chrono::Duration::days(1)
        })
        .unwrap_or(false)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CalendarListPage {
    #[serde(default)]
    items: Vec<GoogleCalendar>,
    next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GoogleCalendar {
    id: String,
    summary: String,
    description: Option<String>,
    etag: Option<String>,
    #[serde(default)]
    default_reminders: Vec<GoogleReminder>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EventPage {
    #[serde(default)]
    items: Option<Vec<Value>>,
    next_page_token: Option<String>,
    next_sync_token: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct GoogleEvent {
    id: String,
    etag: Option<String>,
    summary: Option<String>,
    description: Option<String>,
    location: Option<String>,
    status: Option<String>,
    // Служебные события (дни рождения, "рабочее место" и т.п.) приходят без start.
    // Делаем поле необязательным, чтобы такое событие пропускалось штатно, а не
    // ломало разбор всей страницы и не сыпало предупреждениями.
    #[serde(default)]
    start: Option<GoogleDateTime>,
    end: Option<GoogleDateTime>,
    #[serde(default)]
    recurrence: Vec<String>,
    recurring_event_id: Option<String>,
    original_start_time: Option<GoogleDateTime>,
    #[serde(default)]
    attendees: Vec<GoogleAttendee>,
    reminders: Option<GoogleReminders>,
    transparency: Option<String>,
    visibility: Option<String>,
    organizer: Option<GoogleAttendee>,
    sequence: Option<i64>,
    source: Option<GoogleEventSource>,
    extended_properties: Option<GoogleExtendedProperties>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct GoogleEventSource {
    url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct GoogleExtendedProperties {
    #[serde(default)]
    private: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct GoogleReminder {
    method: String,
    minutes: i32,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct GoogleReminders {
    #[serde(default)]
    use_default: bool,
    #[serde(default)]
    overrides: Vec<GoogleReminder>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct GoogleAttendee {
    email: String,
    display_name: Option<String>,
    #[serde(default)]
    organizer: bool,
    #[serde(default)]
    optional: bool,
    response_status: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct GoogleDateTime {
    date: Option<String>,
    date_time: Option<String>,
    time_zone: Option<String>,
}

impl GoogleDateTime {
    fn value(&self) -> Option<String> {
        self.date_time.clone().or_else(|| self.date.clone())
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConnectionsPage {
    #[serde(default)]
    connections: Vec<GooglePerson>,
    next_page_token: Option<String>,
    next_sync_token: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct GooglePerson {
    resource_name: String,
    etag: Option<String>,
    #[serde(default)]
    names: Vec<PersonName>,
    #[serde(default)]
    email_addresses: Vec<PersonEmail>,
    #[serde(default)]
    phone_numbers: Vec<PersonPhone>,
    #[serde(default)]
    organizations: Vec<PersonOrganization>,
    metadata: Option<PersonMetadata>,
}

#[derive(Debug, Deserialize, Serialize)]
struct PersonMetadata {
    #[serde(default)]
    deleted: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PersonName {
    display_name: Option<String>,
    given_name: Option<String>,
    family_name: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct PersonEmail {
    value: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct PersonPhone {
    value: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct PersonOrganization {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TaskListsPage {
    #[serde(default)]
    items: Vec<GoogleTaskList>,
    next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GoogleTaskList {
    id: String,
    title: String,
    etag: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TasksPage {
    #[serde(default)]
    items: Vec<GoogleTask>,
    next_page_token: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct GoogleTask {
    id: String,
    etag: Option<String>,
    #[serde(default)]
    title: String,
    notes: Option<String>,
    due: Option<String>,
    status: Option<String>,
    completed: Option<String>,
    #[serde(default)]
    deleted: bool,
}

enum SyncResponse<T> {
    Data(T),
    Expired,
}

fn api_error(backend: &str, message: impl Into<String>) -> Error {
    Error::Backend {
        backend: backend.into(),
        message: message.into(),
    }
}

async fn get_json<T: DeserializeOwned>(
    client: &Client,
    url: Url,
    access_token: &str,
    backend: &str,
) -> Result<T> {
    let response = client
        .get(url.clone())
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|error| api_error(backend, error.to_string()))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(api_error(
            backend,
            format!("GET {url}: HTTP {status}: {body}"),
        ));
    }
    response
        .json()
        .await
        .map_err(|error| api_error(backend, format!("ответ Google не разобран: {error}")))
}

async fn get_sync_json<T: DeserializeOwned>(
    client: &Client,
    url: Url,
    access_token: &str,
    backend: &str,
) -> Result<SyncResponse<T>> {
    let response = client
        .get(url.clone())
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|error| api_error(backend, error.to_string()))?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if status == reqwest::StatusCode::GONE || body.contains("EXPIRED_SYNC_TOKEN") {
        return Ok(SyncResponse::Expired);
    }
    if !status.is_success() {
        return Err(api_error(
            backend,
            format!("GET {url}: HTTP {status}: {body}"),
        ));
    }
    serde_json::from_str(&body)
        .map(SyncResponse::Data)
        .map_err(|error| api_error(backend, format!("ответ Google не разобран: {error}")))
}

fn api_url(base: &str, segments: &[&str]) -> Result<Url> {
    let mut url = Url::parse(base).map_err(|error| api_error("google-url", error.to_string()))?;
    url.path_segments_mut()
        .map_err(|_| api_error("google-url", "базовый URL нельзя изменить"))?
        .extend(segments);
    Ok(url)
}

fn recurrence_value(lines: &[String], name: &str) -> Option<String> {
    lines.iter().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.eq_ignore_ascii_case(name).then(|| value.to_owned())
    })
}

fn google_partstat(value: &str) -> String {
    match value {
        "needsAction" => "NEEDS-ACTION",
        "accepted" => "ACCEPTED",
        "declined" => "DECLINED",
        "tentative" => "TENTATIVE",
        other => other,
    }
    .into()
}

/// Минимальное представление отменённой встречи Google. При delta-синхронизации
/// Google присылает такое событие огрызком - заполнен только id (иногда ещё
/// recurringEventId/originalStartTime), а summary и start отсутствуют. Пустые
/// summary/dtstart - намеренный сигнал для save_auxiliary_data: оно подставит
/// прежние значения из БД вместо того, чтобы затереть их пустотой (см. задачу A
/// в crates/core/src/storage/repo.rs).
fn cancelled_google_event(event: GoogleEvent) -> DavEvent {
    let uid = event
        .recurring_event_id
        .clone()
        .unwrap_or_else(|| event.id.clone());
    let dtstart = event
        .start
        .as_ref()
        .and_then(GoogleDateTime::value)
        .unwrap_or_default();
    let recurrence_id = event
        .original_start_time
        .as_ref()
        .and_then(GoogleDateTime::value);
    let sequence = event.sequence.unwrap_or(0);
    let raw = serde_json::to_string(&event).unwrap_or_default();
    DavEvent {
        remote_url: Some(format!("google-event:{}", event.id)),
        uid,
        summary: event.summary.unwrap_or_default(),
        description: event.description,
        location: event.location,
        dtstart,
        dtend: event.end.and_then(|value| value.value()),
        rrule: None,
        recurrence_id,
        exdates: None,
        rdates: None,
        status: Some("CANCELLED".into()),
        attendees: Vec::new(),
        alarms: Vec::new(),
        timezone: None,
        transp: None,
        class: None,
        categories: Vec::new(),
        url: None,
        organizer: None,
        sequence,
        raw,
        etag: event.etag,
    }
}

fn event_from_google(event: GoogleEvent, default_reminders: &[GoogleReminder]) -> Option<DavEvent> {
    let start = event.start.as_ref().and_then(GoogleDateTime::value)?;
    let uid = event
        .recurring_event_id
        .clone()
        .unwrap_or_else(|| event.id.clone());
    let raw = serde_json::to_string(&event).ok()?;
    let attendees = event
        .attendees
        .iter()
        .map(|attendee| Attendee {
            email: attendee.email.clone(),
            name: attendee.display_name.clone(),
            role: Some(if attendee.organizer {
                "CHAIR".into()
            } else if attendee.optional {
                "OPT-PARTICIPANT".into()
            } else {
                "REQ-PARTICIPANT".into()
            }),
            partstat: attendee.response_status.as_deref().map(google_partstat),
            rsvp: attendee.response_status.as_deref() == Some("needsAction"),
        })
        .collect();
    let reminders = event.reminders.as_ref();
    let reminder_rows = if reminders.is_some_and(|value| value.use_default) {
        default_reminders
    } else {
        reminders.map_or(&[][..], |value| value.overrides.as_slice())
    };
    let alarms = reminder_rows
        .iter()
        .map(|reminder| Alarm {
            trigger_minutes: reminder.minutes,
            action: reminder.method.to_ascii_uppercase(),
        })
        .collect();
    let private = event
        .extended_properties
        .as_ref()
        .map(|value| &value.private);
    Some(DavEvent {
        remote_url: Some(format!("google-event:{}", event.id)),
        uid,
        summary: event.summary.unwrap_or_else(|| "Без названия".into()),
        description: event.description,
        location: event.location,
        dtstart: start,
        dtend: event.end.and_then(|value| value.value()),
        rrule: recurrence_value(&event.recurrence, "RRULE"),
        recurrence_id: event.original_start_time.and_then(|value| value.value()),
        exdates: recurrence_value(&event.recurrence, "EXDATE"),
        rdates: recurrence_value(&event.recurrence, "RDATE"),
        status: event.status,
        attendees,
        alarms,
        timezone: event
            .start
            .as_ref()
            .and_then(|value| value.time_zone.clone()),
        transp: event.transparency.map(|value| value.to_ascii_uppercase()),
        class: event.visibility.map(|value| value.to_ascii_uppercase()),
        categories: private
            .and_then(|values| values.get("truemailCategories"))
            .map(|value| {
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default(),
        url: event.source.and_then(|value| value.url),
        organizer: private
            .and_then(|values| values.get("truemailOrganizer"))
            .cloned()
            .or_else(|| event.organizer.map(|value| value.email)),
        sequence: event.sequence.unwrap_or_default(),
        raw,
        etag: event.etag,
    })
}

async fn fetch_calendars(
    client: &Client,
    access_token: &str,
    cursors: &AuxiliarySyncCursors,
) -> Result<Vec<DavCalendar>> {
    let mut calendars = Vec::new();
    let mut page_token: Option<String> = None;
    loop {
        let mut url = api_url(CALENDAR_BASE, &["users", "me", "calendarList"])?;
        url.query_pairs_mut().append_pair("maxResults", "250");
        if let Some(token) = &page_token {
            url.query_pairs_mut().append_pair("pageToken", token);
        }
        let page: CalendarListPage = get_json(client, url, access_token, "google-calendar").await?;
        for calendar in page.items {
            let source_url = format!("google-calendar:{}", calendar.id);
            let stored_token = cursors
                .calendars
                .get(&source_url)
                .and_then(|cursor| cursor.sync_token.clone());
            let stored_cursor = decode_calendar_cursor(stored_token.as_deref());
            if let Some(cursor) = stored_cursor.as_ref()
                && cursor.consecutive_expired >= 2
                && calendar_full_is_fresh(cursor)
            {
                tracing::info!(
                    provider = "google-calendar",
                    collection = %source_url,
                    "Google Calendar collection unchanged; unsupported syncToken is in daily cooldown"
                );
                calendars.push(DavCalendar {
                    url: source_url,
                    name: calendar.summary,
                    ctag: calendar.etag.or(calendar.description),
                    sync_token: stored_token,
                    sync_scope: SyncScope::Delta,
                    deleted_event_urls: Vec::new(),
                    events: Vec::new(),
                });
                continue;
            }
            let mut requested_token = stored_cursor
                .as_ref()
                .map(|cursor| cursor.sync_token.clone());
            let mut rebaseline_after_expired = false;
            let (events, deleted_event_urls, sync_token, sync_scope) = 'retry: loop {
                let mut events = Vec::new();
                // Google больше не шлёт отменённые события через deleted_event_urls
                // (см. классификацию status == "cancelled" ниже) - но переменная
                // остаётся: провайдер физически удалённых событий не отличает от
                // отменённых, поэтому showDeleted=true формально может прислать и
                // настоящее удаление resourceId без событий. На практике Google
                // Calendar API отдаёт такие случаи тем же cancelled-огрызком.
                let deleted_event_urls: Vec<String> = Vec::new();
                let mut event_page_token: Option<String> = None;
                loop {
                    let mut events_url =
                        api_url(CALENDAR_BASE, &["calendars", &calendar.id, "events"])?;
                    events_url
                        .query_pairs_mut()
                        .append_pair("maxResults", "2500")
                        .append_pair("showDeleted", "true")
                        .append_pair("singleEvents", "false");
                    if let Some(token) = requested_token.as_deref() {
                        events_url.query_pairs_mut().append_pair("syncToken", token);
                    }
                    if let Some(token) = &event_page_token {
                        events_url.query_pairs_mut().append_pair("pageToken", token);
                    }
                    let event_page: EventPage =
                        match get_sync_json(client, events_url, access_token, "google-calendar")
                            .await?
                        {
                            SyncResponse::Data(page) => page,
                            SyncResponse::Expired if requested_token.is_some() => {
                                if let Some(cursor) = stored_cursor.as_ref()
                                    && cursor.consecutive_expired >= 1
                                    && calendar_full_is_fresh(cursor)
                                {
                                    let mut cooldown = cursor.clone();
                                    cooldown.consecutive_expired = 2;
                                    break 'retry (
                                        Vec::new(),
                                        Vec::new(),
                                        encode_calendar_cursor(&cooldown)?,
                                        SyncScope::Delta,
                                    );
                                }
                                rebaseline_after_expired = true;
                                requested_token = None;
                                continue 'retry;
                            }
                            SyncResponse::Expired => {
                                return Err(api_error(
                                    "google-calendar",
                                    "Google отклонил полную синхронизацию как устаревшую",
                                ));
                            }
                        };
                    for raw_event in event_page.items.unwrap_or_default() {
                        match serde_json::from_value::<GoogleEvent>(raw_event) {
                            Ok(event) => {
                                if event.status.as_deref() == Some("cancelled") {
                                    // Отменённая встреча остаётся в календаре
                                    // со статусом cancelled, а не удаляется
                                    // (продуктовое решение). Google отдаёт
                                    // такое событие огрызком без start/summary,
                                    // поэтому обычный event_from_google для
                                    // него не годится - у него dtstart
                                    // обязателен.
                                    events.push(cancelled_google_event(event));
                                } else if let Some(event) =
                                    event_from_google(event, &calendar.default_reminders)
                                {
                                    events.push(event);
                                }
                            }
                            Err(error) => tracing::warn!(
                                calendar = %calendar.id,
                                %error,
                                "служебное событие Google Calendar пропущено"
                            ),
                        }
                    }
                    event_page_token = event_page.next_page_token;
                    if event_page_token.is_none() {
                        let next_sync_token = event_page.next_sync_token.ok_or_else(|| {
                            api_error("google-calendar", "Google не вернул nextSyncToken")
                        })?;
                        let sync_token = if rebaseline_after_expired {
                            encode_calendar_cursor(&GoogleCalendarCursor {
                                sync_token: next_sync_token,
                                last_full: chrono::Utc::now()
                                    .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                                consecutive_expired: 1,
                            })?
                        } else {
                            next_sync_token
                        };
                        break 'retry (
                            events,
                            deleted_event_urls,
                            sync_token,
                            if requested_token.is_some() {
                                SyncScope::Delta
                            } else {
                                SyncScope::Full
                            },
                        );
                    }
                }
            };
            let changed = events.len();
            let deleted = deleted_event_urls.len();
            if changed == 0 && deleted == 0 {
                tracing::debug!(
                    provider = "google-calendar",
                    collection = %source_url,
                    scope = ?sync_scope,
                    changed,
                    deleted,
                    "Google Calendar collection delta fetched"
                );
            } else {
                tracing::info!(
                    provider = "google-calendar",
                    collection = %source_url,
                    scope = ?sync_scope,
                    changed,
                    deleted,
                    "Google Calendar collection delta fetched"
                );
            }
            calendars.push(DavCalendar {
                url: source_url,
                name: calendar.summary,
                ctag: calendar.etag.or(calendar.description),
                sync_token: Some(sync_token),
                sync_scope,
                deleted_event_urls,
                events,
            });
        }
        page_token = page.next_page_token;
        if page_token.is_none() {
            break;
        }
    }
    Ok(calendars)
}

async fn fetch_contacts(
    client: &Client,
    access_token: &str,
    stored_token: Option<&str>,
) -> Result<(Vec<DavContact>, Vec<String>, String, SyncScope)> {
    let mut requested_token = stored_token.map(str::to_owned);
    'retry: loop {
        let mut contacts = Vec::new();
        let mut deleted_contact_urls = Vec::new();
        let mut page_token: Option<String> = None;
        loop {
            let mut url = Url::parse(PEOPLE_CONNECTIONS)
                .map_err(|error| api_error("google-contacts", error.to_string()))?;
            url.query_pairs_mut()
                .append_pair("pageSize", "1000")
                .append_pair(
                    "personFields",
                    "metadata,names,emailAddresses,phoneNumbers,organizations",
                )
                .append_pair("requestSyncToken", "true");
            if let Some(token) = requested_token.as_deref() {
                url.query_pairs_mut().append_pair("syncToken", token);
            }
            if let Some(token) = &page_token {
                url.query_pairs_mut().append_pair("pageToken", token);
            }
            let page: ConnectionsPage =
                match get_sync_json(client, url, access_token, "google-contacts").await? {
                    SyncResponse::Data(page) => page,
                    SyncResponse::Expired if requested_token.is_some() => {
                        requested_token = None;
                        continue 'retry;
                    }
                    SyncResponse::Expired => {
                        return Err(api_error(
                            "google-contacts",
                            "Google отклонил полную синхронизацию как устаревшую",
                        ));
                    }
                };
            for person in page.connections {
                if person
                    .metadata
                    .as_ref()
                    .is_some_and(|metadata| metadata.deleted)
                {
                    deleted_contact_urls.push(format!("google-contact:{}", person.resource_name));
                    continue;
                }
                let name = person.names.first();
                let emails: Vec<String> = person
                    .email_addresses
                    .iter()
                    .filter_map(|item| item.value.clone())
                    .collect();
                let phones = person
                    .phone_numbers
                    .iter()
                    .filter_map(|item| {
                        item.value
                            .as_deref()
                            .map(|value| ContactPhone::from_remote(value, item.kind.clone()))
                    })
                    .filter(|phone| !phone.number.is_empty())
                    .collect();
                let display_name = name
                    .and_then(|value| value.display_name.clone())
                    .or_else(|| emails.first().cloned())
                    .unwrap_or_else(|| "Без имени".into());
                let raw = serde_json::to_string(&person)?;
                contacts.push(DavContact {
                    remote_url: Some(format!("google-contact:{}", person.resource_name)),
                    uid: person.resource_name,
                    display_name: clean_contact_name(&display_name),
                    first_name: name.and_then(|value| value.given_name.clone()),
                    last_name: name.and_then(|value| value.family_name.clone()),
                    organization: person
                        .organizations
                        .first()
                        .and_then(|value| value.name.clone()),
                    emails,
                    phones,
                    raw,
                    etag: person.etag,
                });
            }
            page_token = page.next_page_token;
            if page_token.is_none() {
                let sync_token = page.next_sync_token.ok_or_else(|| {
                    api_error("google-contacts", "Google не вернул nextSyncToken")
                })?;
                return Ok((
                    contacts,
                    deleted_contact_urls,
                    sync_token,
                    if requested_token.is_some() {
                        SyncScope::Delta
                    } else {
                        SyncScope::Full
                    },
                ));
            }
        }
    }
}

fn task_event(list_id: &str, task: GoogleTask) -> Option<DavEvent> {
    let due = task.due.clone().or(task.completed.clone())?;
    let raw = serde_json::to_string(&task).ok()?;
    Some(DavEvent {
        remote_url: Some(format!("google-task:{}", task.id)),
        uid: format!("google-task:{list_id}:{}", task.id),
        summary: task.title,
        description: task.notes,
        location: None,
        dtstart: due,
        dtend: None,
        rrule: None,
        recurrence_id: None,
        exdates: None,
        rdates: None,
        status: task.status,
        attendees: Vec::new(),
        alarms: Vec::new(),
        timezone: None,
        transp: None,
        class: None,
        categories: Vec::new(),
        url: None,
        organizer: None,
        sequence: 0,
        raw,
        etag: task.etag,
    })
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct GoogleTasksCursor {
    updated_min: String,
    last_full: String,
}

fn decode_tasks_cursor(value: Option<&str>) -> Option<GoogleTasksCursor> {
    let value = value?;
    if let Some(json) = value.strip_prefix("google-tasks-v1:") {
        serde_json::from_str(json).ok()
    } else {
        // Legacy cursor contained only updatedMin. Treat its timestamp as the
        // last reconciliation so upgraded installations naturally perform a
        // full scoped pass once it becomes one day old.
        Some(GoogleTasksCursor {
            updated_min: value.to_owned(),
            last_full: value.to_owned(),
        })
    }
}

fn encode_tasks_cursor(cursor: &GoogleTasksCursor) -> Result<String> {
    Ok(format!(
        "google-tasks-v1:{}",
        serde_json::to_string(cursor)?
    ))
}

async fn fetch_task_calendars(
    client: &Client,
    access_token: &str,
    cursors: &AuxiliarySyncCursors,
) -> Result<Vec<DavCalendar>> {
    let mut calendars = Vec::new();
    let mut page_token: Option<String> = None;
    // Baseline is captured before requests so a concurrent edit is never
    // skipped; at worst it is returned once again in the next delta.
    let next_updated_min = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    loop {
        let mut url = api_url(TASKS_BASE, &["users", "@me", "lists"])?;
        url.query_pairs_mut().append_pair("maxResults", "100");
        if let Some(token) = &page_token {
            url.query_pairs_mut().append_pair("pageToken", token);
        }
        let page: TaskListsPage = get_json(client, url, access_token, "google-tasks").await?;
        for list in page.items {
            let source_url = format!("google-tasks:{}", list.id);
            let previous_cursor = cursors
                .calendars
                .get(&source_url)
                .and_then(|cursor| decode_tasks_cursor(cursor.sync_token.as_deref()));
            let full_reconciliation = previous_cursor.as_ref().is_none_or(|cursor| {
                chrono::DateTime::parse_from_rfc3339(&cursor.last_full)
                    .map(|last| {
                        chrono::Utc::now().signed_duration_since(last.with_timezone(&chrono::Utc))
                            >= chrono::Duration::days(1)
                    })
                    .unwrap_or(true)
            });
            let updated_min = if full_reconciliation {
                None
            } else {
                previous_cursor.as_ref().and_then(|cursor| {
                    chrono::DateTime::parse_from_rfc3339(&cursor.updated_min)
                        .ok()
                        .map(|value| {
                            (value.with_timezone(&chrono::Utc) - chrono::Duration::minutes(5))
                                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
                        })
                })
            };
            let mut events_by_url = std::collections::HashMap::new();
            let mut deleted_event_urls = std::collections::HashSet::new();
            let mut task_page_token: Option<String> = None;
            loop {
                let mut tasks_url = api_url(TASKS_BASE, &["lists", &list.id, "tasks"])?;
                tasks_url
                    .query_pairs_mut()
                    .append_pair("maxResults", "100")
                    .append_pair("showCompleted", "true")
                    .append_pair("showHidden", "true")
                    .append_pair("showDeleted", "true");
                if let Some(updated_min) = updated_min.as_deref() {
                    tasks_url
                        .query_pairs_mut()
                        .append_pair("updatedMin", updated_min);
                }
                if let Some(token) = &task_page_token {
                    tasks_url.query_pairs_mut().append_pair("pageToken", token);
                }
                let task_page: TasksPage =
                    get_json(client, tasks_url, access_token, "google-tasks").await?;
                for task in task_page.items {
                    let remote_url = format!("google-task:{}", task.id);
                    if task.deleted {
                        events_by_url.remove(&remote_url);
                        deleted_event_urls.insert(remote_url);
                    } else if let Some(event) = task_event(&list.id, task) {
                        deleted_event_urls.remove(&remote_url);
                        events_by_url.insert(remote_url, event);
                    } else if updated_min.is_some() {
                        // A task that lost its due/completed date no longer
                        // belongs in the calendar projection.
                        events_by_url.remove(&remote_url);
                        deleted_event_urls.insert(remote_url);
                    }
                }
                task_page_token = task_page.next_page_token;
                if task_page_token.is_none() {
                    break;
                }
            }
            let next_cursor = GoogleTasksCursor {
                updated_min: next_updated_min.clone(),
                last_full: if full_reconciliation {
                    next_updated_min.clone()
                } else {
                    previous_cursor
                        .map(|cursor| cursor.last_full)
                        .unwrap_or_else(|| next_updated_min.clone())
                },
            };
            let changed = events_by_url.len();
            let deleted = deleted_event_urls.len();
            if changed == 0 && deleted == 0 {
                tracing::debug!(
                    provider = "google-tasks",
                    collection = %source_url,
                    scope = if full_reconciliation { "full" } else { "delta" },
                    changed,
                    deleted,
                    "Google Tasks collection delta fetched"
                );
            } else {
                tracing::info!(
                    provider = "google-tasks",
                    collection = %source_url,
                    scope = if full_reconciliation { "full" } else { "delta" },
                    changed,
                    deleted,
                    "Google Tasks collection delta fetched"
                );
            }
            calendars.push(DavCalendar {
                url: source_url,
                name: format!("Google Tasks · {}", list.title),
                ctag: list.etag,
                sync_token: Some(encode_tasks_cursor(&next_cursor)?),
                sync_scope: if full_reconciliation {
                    SyncScope::Full
                } else {
                    SyncScope::Delta
                },
                deleted_event_urls: deleted_event_urls.into_iter().collect(),
                events: events_by_url.into_values().collect(),
            });
        }
        page_token = page.next_page_token;
        if page_token.is_none() {
            break;
        }
    }
    Ok(calendars)
}

/// Загрузить календари, контакты и задачи одним Google OAuth-токеном.
pub async fn sync_google_services(
    access_token: &str,
    cursors: &AuxiliarySyncCursors,
) -> Result<DavSyncResult> {
    let client = Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(45))
        .build()
        .map_err(|error| api_error("google-services", error.to_string()))?;
    let (calendar_result, contacts_result, tasks_result) = tokio::join!(
        fetch_calendars(&client, access_token, cursors),
        fetch_contacts(
            &client,
            access_token,
            cursors.contacts_sync_token.as_deref()
        ),
        fetch_task_calendars(&client, access_token, cursors)
    );
    let mut calendars = calendar_result?;
    calendars.extend(tasks_result?);
    let (contacts, deleted_contact_urls, contacts_sync_token, contacts_scope) = contacts_result?;
    Ok(DavSyncResult {
        calendars,
        calendars_available: true,
        contacts,
        contact_collections: Vec::new(),
        contacts_available: true,
        contacts_scope,
        contacts_sync_token: Some(contacts_sync_token),
        deleted_contact_urls,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_recurring_google_event() {
        let event: GoogleEvent = serde_json::from_value(serde_json::json!({
            "id": "instance",
            "summary": "Daily",
            "start": {"dateTime": "2026-07-14T10:00:00+03:00", "timeZone": "Europe/Moscow"},
            "end": {"dateTime": "2026-07-14T11:00:00+03:00"},
            "recurrence": ["RRULE:FREQ=DAILY;COUNT=3"],
            "recurringEventId": null,
            "originalStartTime": null,
            "attendees": [{
                "email": "guest@example.test",
                "displayName": "Guest",
                "responseStatus": "accepted"
            }],
            "reminders": {
                "useDefault": false,
                "overrides": [{"method": "popup", "minutes": 10}]
            },
            "transparency": "transparent",
            "visibility": "private",
            "organizer": {"email": "owner@example.test", "organizer": true},
            "sequence": 3,
            "source": {"url": "https://example.test/meeting"},
            "extendedProperties": {"private": {"truemailCategories": "Team,Demo"}}
        }))
        .expect("event");
        let mapped = event_from_google(event, &[]).expect("mapped event");
        assert_eq!(mapped.rrule.as_deref(), Some("FREQ=DAILY;COUNT=3"));
        assert_eq!(mapped.dtstart, "2026-07-14T10:00:00+03:00");
        assert_eq!(mapped.attendees.len(), 1);
        assert_eq!(mapped.attendees[0].partstat.as_deref(), Some("ACCEPTED"));
        assert_eq!(mapped.alarms[0].trigger_minutes, 10);
        assert_eq!(mapped.alarms[0].action, "POPUP");
        assert_eq!(mapped.timezone.as_deref(), Some("Europe/Moscow"));
        assert_eq!(mapped.transp.as_deref(), Some("TRANSPARENT"));
        assert_eq!(mapped.class.as_deref(), Some("PRIVATE"));
        assert_eq!(mapped.categories, ["Team", "Demo"]);
        assert_eq!(mapped.url.as_deref(), Some("https://example.test/meeting"));
        assert_eq!(mapped.organizer.as_deref(), Some("owner@example.test"));
        assert_eq!(mapped.sequence, 3);
    }

    #[test]
    fn skips_task_without_due_or_completed_date() {
        let task = GoogleTask {
            id: "1".into(),
            etag: None,
            title: "Someday".into(),
            notes: None,
            due: None,
            status: Some("needsAction".into()),
            completed: None,
            deleted: false,
        };
        assert!(task_event("list", task).is_none());
    }

    #[test]
    fn google_tasks_cursor_preserves_last_full_reconciliation() {
        let cursor = GoogleTasksCursor {
            updated_min: "2026-07-18T01:00:00Z".into(),
            last_full: "2026-07-17T01:00:00Z".into(),
        };
        let encoded = encode_tasks_cursor(&cursor).expect("encode cursor");
        let decoded = decode_tasks_cursor(Some(&encoded)).expect("decode cursor");
        assert_eq!(decoded.updated_min, cursor.updated_min);
        assert_eq!(decoded.last_full, cursor.last_full);
    }

    #[test]
    fn google_calendar_cursor_round_trips_expiry_cooldown() {
        let cursor = GoogleCalendarCursor {
            sync_token: "opaque-token".into(),
            last_full: "2026-07-18T01:00:00Z".into(),
            consecutive_expired: 2,
        };
        let encoded = encode_calendar_cursor(&cursor).expect("encode cursor");
        let decoded = decode_calendar_cursor(Some(&encoded)).expect("decode cursor");
        assert_eq!(decoded.sync_token, cursor.sync_token);
        assert_eq!(decoded.last_full, cursor.last_full);
        assert_eq!(decoded.consecutive_expired, 2);
    }

    #[test]
    fn legacy_google_calendar_token_remains_usable() {
        let decoded = decode_calendar_cursor(Some("legacy-opaque-token")).expect("legacy cursor");
        assert_eq!(decoded.sync_token, "legacy-opaque-token");
        assert_eq!(decoded.consecutive_expired, 0);
        assert!(!calendar_full_is_fresh(&decoded));
    }
}
