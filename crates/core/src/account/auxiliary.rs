//! Двусторонние операции календаря, контактов и задач для Google и Яндекса.

use crate::model::Provider;
use crate::{Error, Result};
use reqwest::{Client, Method};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use url::Url;

const GOOGLE_CALENDAR_BASE: &str = "https://www.googleapis.com/calendar/v3";
const GOOGLE_PEOPLE_BASE: &str = "https://people.googleapis.com/v1";
const GOOGLE_TASKS_BASE: &str = "https://tasks.googleapis.com/tasks/v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventInput {
    pub summary: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub dtstart: String,
    pub dtend: Option<String>,
    #[serde(default)]
    pub all_day: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactInput {
    pub display_name: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub organization: Option<String>,
    #[serde(default)]
    pub emails: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RemoteObject<'a> {
    pub uid: Option<&'a str>,
    pub remote_url: Option<&'a str>,
    pub etag: Option<&'a str>,
}

fn backend_error(backend: &str, message: impl Into<String>) -> Error {
    Error::Backend {
        backend: backend.into(),
        message: message.into(),
    }
}

fn client() -> Result<Client> {
    Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|error| backend_error("auxiliary", error.to_string()))
}

fn api_url(base: &str, segments: &[&str]) -> Result<Url> {
    let mut url =
        Url::parse(base).map_err(|error| backend_error("google-url", error.to_string()))?;
    url.path_segments_mut()
        .map_err(|_| backend_error("google-url", "базовый URL нельзя изменить"))?
        .extend(segments);
    Ok(url)
}

async fn google_json(
    method: Method,
    url: Url,
    access_token: &str,
    body: Option<Value>,
) -> Result<()> {
    let client = client()?;
    let mut request = client
        .request(method, url.clone())
        .bearer_auth(access_token);
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request
        .send()
        .await
        .map_err(|error| backend_error("google-auxiliary", error.to_string()))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(backend_error(
            "google-auxiliary",
            format!("{url}: HTTP {status}: {body}"),
        ));
    }
    Ok(())
}

fn google_event_body(input: &EventInput) -> Value {
    let date = |value: &str| {
        if input.all_day {
            json!({"date": value.get(..10).unwrap_or(value)})
        } else {
            json!({"dateTime": value})
        }
    };
    let mut body = json!({
        "summary": input.summary,
        "description": input.description,
        "location": input.location,
        "start": date(&input.dtstart),
    });
    body["end"] = input
        .dtend
        .as_deref()
        .map(date)
        .unwrap_or_else(|| date(&input.dtstart));
    body
}

fn strip_remote<'a>(value: &'a str, prefix: &str) -> Result<&'a str> {
    value.strip_prefix(prefix).ok_or_else(|| {
        backend_error(
            "auxiliary",
            format!("неизвестный серверный идентификатор: {value}"),
        )
    })
}

async fn write_google_event(
    calendar_source: &str,
    remote: RemoteObject<'_>,
    input: &EventInput,
    access_token: &str,
) -> Result<()> {
    if let Some(list_id) = calendar_source.strip_prefix("google-tasks:") {
        let task_id = remote
            .remote_url
            .map(|value| strip_remote(value, "google-task:"))
            .transpose()?;
        let mut url = api_url(GOOGLE_TASKS_BASE, &["lists", list_id, "tasks"])?;
        let method = if let Some(task_id) = task_id {
            url.path_segments_mut()
                .map_err(|_| backend_error("google-url", "URL задачи нельзя изменить"))?
                .push(task_id);
            Method::PATCH
        } else {
            Method::POST
        };
        return google_json(
            method,
            url,
            access_token,
            Some(json!({
                "title": input.summary,
                "notes": input.description,
                "due": input.dtstart,
            })),
        )
        .await;
    }

    let calendar_id = strip_remote(calendar_source, "google-calendar:")?;
    let event_id = remote
        .remote_url
        .map(|value| strip_remote(value, "google-event:"))
        .transpose()?;
    let mut url = api_url(GOOGLE_CALENDAR_BASE, &["calendars", calendar_id, "events"])?;
    let method = if let Some(event_id) = event_id {
        url.path_segments_mut()
            .map_err(|_| backend_error("google-url", "URL события нельзя изменить"))?
            .push(event_id);
        Method::PATCH
    } else {
        Method::POST
    };
    google_json(method, url, access_token, Some(google_event_body(input))).await
}

async fn delete_google_event(
    calendar_source: &str,
    remote_url: &str,
    access_token: &str,
) -> Result<()> {
    let url = if let Some(list_id) = calendar_source.strip_prefix("google-tasks:") {
        let task_id = strip_remote(remote_url, "google-task:")?;
        api_url(GOOGLE_TASKS_BASE, &["lists", list_id, "tasks", task_id])?
    } else {
        let calendar_id = strip_remote(calendar_source, "google-calendar:")?;
        let event_id = strip_remote(remote_url, "google-event:")?;
        api_url(
            GOOGLE_CALENDAR_BASE,
            &["calendars", calendar_id, "events", event_id],
        )?
    };
    google_json(Method::DELETE, url, access_token, None).await
}

fn ical_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace(',', "\\,")
        .replace(';', "\\;")
}

fn ical_date(value: &str, all_day: bool) -> String {
    if all_day {
        return value.chars().filter(char::is_ascii_digit).take(8).collect();
    }
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|date| date.to_utc().format("%Y%m%dT%H%M%SZ").to_string())
        .unwrap_or_else(|_| value.to_owned())
}

fn yandex_event_body(uid: &str, input: &EventInput) -> String {
    let start_key = if input.all_day {
        "DTSTART;VALUE=DATE"
    } else {
        "DTSTART"
    };
    let end_key = if input.all_day {
        "DTEND;VALUE=DATE"
    } else {
        "DTEND"
    };
    let mut lines = vec![
        "BEGIN:VCALENDAR".to_owned(),
        "VERSION:2.0".to_owned(),
        "PRODID:-//truemail//EN".to_owned(),
        "BEGIN:VEVENT".to_owned(),
        format!("UID:{uid}"),
        format!("{start_key}:{}", ical_date(&input.dtstart, input.all_day)),
        format!("SUMMARY:{}", ical_escape(&input.summary)),
    ];
    if let Some(end) = &input.dtend {
        lines.push(format!("{end_key}:{}", ical_date(end, input.all_day)));
    }
    if let Some(description) = &input.description {
        lines.push(format!("DESCRIPTION:{}", ical_escape(description)));
    }
    if let Some(location) = &input.location {
        lines.push(format!("LOCATION:{}", ical_escape(location)));
    }
    lines.extend([
        "END:VEVENT".to_owned(),
        "END:VCALENDAR".to_owned(),
        String::new(),
    ]);
    lines.join("\r\n")
}

async fn yandex_dav_write(
    method: Method,
    url: &str,
    email: &str,
    access_token: &str,
    content_type: Option<&str>,
    body: Option<String>,
    etag: Option<&str>,
    create: bool,
) -> Result<()> {
    let client = client()?;
    let mut request = client
        .request(method, url)
        .basic_auth(email, Some(access_token));
    if let Some(content_type) = content_type {
        request = request.header("Content-Type", content_type);
    }
    if create {
        request = request.header("If-None-Match", "*");
    } else if let Some(etag) = etag {
        request = request.header("If-Match", etag);
    }
    if let Some(body) = body {
        request = request.body(body);
    }
    let response = request
        .send()
        .await
        .map_err(|error| backend_error("yandex-dav-write", error.to_string()))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(backend_error(
            "yandex-dav-write",
            format!("{url}: HTTP {status}: {body}"),
        ));
    }
    Ok(())
}

async fn write_yandex_event(
    calendar_source: &str,
    remote: RemoteObject<'_>,
    input: &EventInput,
    email: &str,
    access_token: &str,
) -> Result<()> {
    let create = remote.remote_url.is_none();
    let uid = remote
        .uid
        .map(str::to_owned)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let url = match remote.remote_url {
        Some(url) => url.to_owned(),
        None => Url::parse(calendar_source)
            .and_then(|base| base.join(&format!("{uid}.ics")))
            .map(String::from)
            .map_err(|error| backend_error("yandex-caldav", error.to_string()))?,
    };
    yandex_dav_write(
        Method::PUT,
        &url,
        email,
        access_token,
        Some("text/calendar; charset=utf-8"),
        Some(yandex_event_body(&uid, input)),
        remote.etag,
        create,
    )
    .await
}

/// Создать или изменить событие/задачу на сервере.
pub async fn write_event(
    provider: Provider,
    email: &str,
    access_token: &str,
    calendar_source: &str,
    remote: RemoteObject<'_>,
    input: &EventInput,
) -> Result<()> {
    match provider {
        Provider::Gmail => write_google_event(calendar_source, remote, input, access_token).await,
        Provider::Yandex => {
            write_yandex_event(calendar_source, remote, input, email, access_token).await
        }
        _ => Err(backend_error(
            "auxiliary",
            "изменение календаря для провайдера не поддерживается",
        )),
    }
}

/// Удалить событие/задачу на сервере.
pub async fn delete_event(
    provider: Provider,
    email: &str,
    access_token: &str,
    calendar_source: &str,
    remote_url: &str,
    etag: Option<&str>,
) -> Result<()> {
    match provider {
        Provider::Gmail => delete_google_event(calendar_source, remote_url, access_token).await,
        Provider::Yandex => {
            yandex_dav_write(
                Method::DELETE,
                remote_url,
                email,
                access_token,
                None,
                None,
                etag,
                false,
            )
            .await
        }
        _ => Err(backend_error(
            "auxiliary",
            "удаление события для провайдера не поддерживается",
        )),
    }
}

fn google_contact_body(input: &ContactInput, etag: Option<&str>) -> Value {
    let mut body = json!({
        "names": [{
            "displayName": input.display_name,
            "givenName": input.first_name,
            "familyName": input.last_name,
        }],
        "emailAddresses": input.emails.iter().map(|email| json!({"value": email})).collect::<Vec<_>>(),
        "organizations": input.organization.as_ref().map(|name| vec![json!({"name": name})]).unwrap_or_default(),
    });
    if let Some(etag) = etag {
        body["etag"] = json!(etag);
    }
    body
}

async fn write_google_contact(
    remote: RemoteObject<'_>,
    input: &ContactInput,
    access_token: &str,
) -> Result<()> {
    let (method, url) = if let Some(remote_url) = remote.remote_url {
        let resource = strip_remote(remote_url, "google-contact:")?;
        let segments: Vec<&str> = resource.split('/').collect();
        let mut url = api_url(GOOGLE_PEOPLE_BASE, &segments)?;
        let path = format!("{}:updateContact", url.path());
        url.set_path(&path);
        url.query_pairs_mut()
            .append_pair("updatePersonFields", "names,emailAddresses,organizations")
            .append_pair("personFields", "names,emailAddresses,organizations");
        (Method::PATCH, url)
    } else {
        let mut url = Url::parse(&format!("{GOOGLE_PEOPLE_BASE}/people:createContact"))
            .map_err(|error| backend_error("google-contacts", error.to_string()))?;
        url.query_pairs_mut()
            .append_pair("personFields", "names,emailAddresses,organizations");
        (Method::POST, url)
    };
    google_json(
        method,
        url,
        access_token,
        Some(google_contact_body(input, remote.etag)),
    )
    .await
}

fn yandex_contact_body(uid: &str, input: &ContactInput) -> String {
    let mut lines = vec![
        "BEGIN:VCARD".to_owned(),
        "VERSION:3.0".to_owned(),
        format!("UID:{uid}"),
        format!("FN:{}", ical_escape(&input.display_name)),
        format!(
            "N:{};{};;;",
            ical_escape(input.last_name.as_deref().unwrap_or("")),
            ical_escape(input.first_name.as_deref().unwrap_or(""))
        ),
    ];
    if let Some(organization) = &input.organization {
        lines.push(format!("ORG:{}", ical_escape(organization)));
    }
    lines.extend(
        input
            .emails
            .iter()
            .map(|email| format!("EMAIL;TYPE=INTERNET:{}", ical_escape(email))),
    );
    lines.extend(["END:VCARD".to_owned(), String::new()]);
    lines.join("\r\n")
}

/// Создать или изменить контакт. Для нового CardDAV-контакта `collection_url`
/// должен указывать на найденную адресную книгу.
pub async fn write_contact(
    provider: Provider,
    email: &str,
    access_token: &str,
    collection_url: Option<&str>,
    remote: RemoteObject<'_>,
    input: &ContactInput,
) -> Result<()> {
    match provider {
        Provider::Gmail => write_google_contact(remote, input, access_token).await,
        Provider::Yandex => {
            let create = remote.remote_url.is_none();
            let uid = remote
                .uid
                .map(str::to_owned)
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let url = match remote.remote_url {
                Some(url) => url.to_owned(),
                None => {
                    let collection = collection_url.ok_or_else(|| {
                        backend_error(
                            "yandex-carddav",
                            "адресная книга ещё не обнаружена; сначала выполните синхронизацию",
                        )
                    })?;
                    Url::parse(collection)
                        .and_then(|base| base.join(&format!("{uid}.vcf")))
                        .map(String::from)
                        .map_err(|error| backend_error("yandex-carddav", error.to_string()))?
                }
            };
            yandex_dav_write(
                Method::PUT,
                &url,
                email,
                access_token,
                Some("text/vcard; charset=utf-8"),
                Some(yandex_contact_body(&uid, input)),
                remote.etag,
                create,
            )
            .await
        }
        _ => Err(backend_error(
            "auxiliary",
            "изменение контактов для провайдера не поддерживается",
        )),
    }
}

/// Удалить контакт на сервере.
pub async fn delete_contact(
    provider: Provider,
    email: &str,
    access_token: &str,
    remote_url: &str,
    etag: Option<&str>,
) -> Result<()> {
    match provider {
        Provider::Gmail => {
            let resource = strip_remote(remote_url, "google-contact:")?;
            let segments: Vec<&str> = resource.split('/').collect();
            let mut url = api_url(GOOGLE_PEOPLE_BASE, &segments)?;
            let path = format!("{}:deleteContact", url.path());
            url.set_path(&path);
            google_json(Method::DELETE, url, access_token, None).await
        }
        Provider::Yandex => {
            yandex_dav_write(
                Method::DELETE,
                remote_url,
                email,
                access_token,
                None,
                None,
                etag,
                false,
            )
            .await
        }
        _ => Err(backend_error(
            "auxiliary",
            "удаление контакта для провайдера не поддерживается",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_writable_google_event_body() {
        let body = google_event_body(&EventInput {
            summary: "Demo".into(),
            description: None,
            location: None,
            dtstart: "2026-07-14T10:00:00+03:00".into(),
            dtend: Some("2026-07-14T11:00:00+03:00".into()),
            all_day: false,
        });
        assert_eq!(body["summary"], "Demo");
        assert_eq!(body["start"]["dateTime"], "2026-07-14T10:00:00+03:00");
    }

    #[test]
    fn builds_rfc5545_event_for_yandex() {
        let body = yandex_event_body(
            "uid",
            &EventInput {
                summary: "A, B".into(),
                description: None,
                location: None,
                dtstart: "2026-07-14T10:00:00Z".into(),
                dtend: None,
                all_day: false,
            },
        );
        assert!(body.contains("SUMMARY:A\\, B"));
        assert!(body.contains("DTSTART:20260714T100000Z"));
    }
}
