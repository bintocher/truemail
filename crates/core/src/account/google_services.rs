//! Синхронизация Google Calendar, Contacts и Tasks через REST API.

use super::dav::{
    AuxiliarySyncCursors, DavCalendar, DavContact, DavEvent, DavSyncResult, SyncScope,
};
use crate::model::{Alarm, Attendee};
use crate::{Error, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use url::Url;

const CALENDAR_BASE: &str = "https://www.googleapis.com/calendar/v3";
const PEOPLE_CONNECTIONS: &str = "https://people.googleapis.com/v1/people/me/connections";
const TASKS_BASE: &str = "https://tasks.googleapis.com/tasks/v1";

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
            let mut requested_token = stored_token.clone();
            let (events, deleted_event_urls, sync_token, sync_scope) = 'retry: loop {
                let mut events = Vec::new();
                let mut deleted_event_urls = Vec::new();
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
                                    deleted_event_urls.push(format!("google-event:{}", event.id));
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
                        let sync_token = event_page.next_sync_token.ok_or_else(|| {
                            api_error("google-calendar", "Google не вернул nextSyncToken")
                        })?;
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
                    "metadata,names,emailAddresses,organizations",
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
                let display_name = name
                    .and_then(|value| value.display_name.clone())
                    .or_else(|| emails.first().cloned())
                    .unwrap_or_else(|| "Без имени".into());
                let raw = serde_json::to_string(&person)?;
                contacts.push(DavContact {
                    remote_url: Some(format!("google-contact:{}", person.resource_name)),
                    uid: person.resource_name,
                    display_name,
                    first_name: name.and_then(|value| value.given_name.clone()),
                    last_name: name.and_then(|value| value.family_name.clone()),
                    organization: person
                        .organizations
                        .first()
                        .and_then(|value| value.name.clone()),
                    emails,
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
        raw,
        etag: task.etag,
    })
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
            let updated_min = cursors
                .calendars
                .get(&source_url)
                .and_then(|cursor| cursor.sync_token.as_deref());
            let mut events = Vec::new();
            let mut deleted_event_urls = Vec::new();
            let mut task_page_token: Option<String> = None;
            loop {
                let mut tasks_url = api_url(TASKS_BASE, &["lists", &list.id, "tasks"])?;
                tasks_url
                    .query_pairs_mut()
                    .append_pair("maxResults", "100")
                    .append_pair("showCompleted", "true")
                    .append_pair("showHidden", "true")
                    .append_pair("showDeleted", "true");
                if let Some(updated_min) = updated_min {
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
                        deleted_event_urls.push(remote_url);
                    } else if let Some(event) = task_event(&list.id, task) {
                        events.push(event);
                    } else if updated_min.is_some() {
                        // A task that lost its due/completed date no longer
                        // belongs in the calendar projection.
                        deleted_event_urls.push(remote_url);
                    }
                }
                task_page_token = task_page.next_page_token;
                if task_page_token.is_none() {
                    break;
                }
            }
            calendars.push(DavCalendar {
                url: source_url,
                name: format!("Google Tasks · {}", list.title),
                ctag: list.etag,
                sync_token: Some(next_updated_min.clone()),
                sync_scope: if updated_min.is_some() {
                    SyncScope::Delta
                } else {
                    SyncScope::Full
                },
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
            "start": {"dateTime": "2026-07-14T10:00:00+03:00"},
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
            }
        }))
        .expect("event");
        let mapped = event_from_google(event, &[]).expect("mapped event");
        assert_eq!(mapped.rrule.as_deref(), Some("FREQ=DAILY;COUNT=3"));
        assert_eq!(mapped.dtstart, "2026-07-14T10:00:00+03:00");
        assert_eq!(mapped.attendees.len(), 1);
        assert_eq!(mapped.attendees[0].partstat.as_deref(), Some("ACCEPTED"));
        assert_eq!(mapped.alarms[0].trigger_minutes, 10);
        assert_eq!(mapped.alarms[0].action, "POPUP");
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
}
