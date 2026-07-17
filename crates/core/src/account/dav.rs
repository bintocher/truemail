//! Первичная синхронизация календарей и контактов Яндекса по CalDAV/CardDAV.

use crate::model::{Alarm, Attendee};
use crate::{Error, Result};
use reqwest::{Client, Method, StatusCode};
use roxmltree::Document;
use std::collections::HashMap;
use url::Url;

#[derive(Debug, Clone, Default)]
pub struct AuxiliarySyncCursors {
    pub calendars: HashMap<String, CollectionCursor>,
    pub contact_collections: HashMap<String, CollectionCursor>,
    pub contacts_sync_token: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CollectionCursor {
    pub ctag: Option<String>,
    pub sync_token: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SyncScope {
    #[default]
    Full,
    Delta,
    Unchanged,
}

#[derive(Debug, Default)]
pub struct DavSyncResult {
    pub calendars: Vec<DavCalendar>,
    pub contacts: Vec<DavContact>,
    /// Доступные CardDAV-коллекции. Нужны для создания первого контакта,
    /// когда адресная книга ещё пуста и URL нельзя вывести из vCard.
    pub contact_collections: Vec<DavCollection>,
    /// CardDAV-коллекция действительно была доступна и прочитана. Если Яндекс
    /// ещё не создал адресную книгу и вернул 404, локальные контакты удалять нельзя.
    pub contacts_available: bool,
    pub contacts_scope: SyncScope,
    pub contacts_sync_token: Option<String>,
    pub deleted_contact_urls: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DavCollection {
    pub url: String,
    pub ctag: Option<String>,
}

#[derive(Debug)]
pub struct DavCalendar {
    pub url: String,
    pub name: String,
    pub ctag: Option<String>,
    pub sync_token: Option<String>,
    pub sync_scope: SyncScope,
    pub deleted_event_urls: Vec<String>,
    pub events: Vec<DavEvent>,
}

#[derive(Debug)]
pub struct DavEvent {
    pub remote_url: Option<String>,
    pub uid: String,
    pub summary: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub dtstart: String,
    pub dtend: Option<String>,
    pub rrule: Option<String>,
    pub recurrence_id: Option<String>,
    pub exdates: Option<String>,
    pub rdates: Option<String>,
    pub status: Option<String>,
    pub attendees: Vec<Attendee>,
    pub alarms: Vec<Alarm>,
    pub timezone: Option<String>,
    pub transp: Option<String>,
    pub class: Option<String>,
    pub categories: Vec<String>,
    pub url: Option<String>,
    pub organizer: Option<String>,
    pub sequence: i64,
    pub raw: String,
    pub etag: Option<String>,
}

#[derive(Debug)]
pub struct DavContact {
    pub remote_url: Option<String>,
    pub uid: String,
    pub display_name: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub organization: Option<String>,
    pub emails: Vec<String>,
    pub raw: String,
    pub etag: Option<String>,
}

const PRINCIPAL_BODY: &str = r#"<?xml version="1.0" encoding="utf-8"?><d:propfind xmlns:d="DAV:"><d:prop><d:current-user-principal/></d:prop></d:propfind>"#;

async fn dav_request(
    client: &Client,
    method: &str,
    url: &str,
    depth: &str,
    body: &str,
    email: &str,
    token: &str,
) -> Result<String> {
    dav_request_optional(client, method, url, depth, body, email, token)
        .await?
        .ok_or_else(|| Error::Backend {
            backend: "dav".into(),
            message: format!("{method} {url}: HTTP 404 Not Found"),
        })
}

/// Выполняет DAV-запрос, но позволяет вызывающему отличить отсутствующую
/// коллекцию от ошибки транспорта. Яндекс создаёт CardDAV-книгу лениво, поэтому
/// объявленный addressbook-home-set может законно отвечать 404 до появления
/// первой синхронизируемой адресной книги.
async fn dav_request_optional(
    client: &Client,
    method: &str,
    url: &str,
    depth: &str,
    body: &str,
    email: &str,
    token: &str,
) -> Result<Option<String>> {
    let method = Method::from_bytes(method.as_bytes()).map_err(|e| Error::Other(e.to_string()))?;
    let method_name = method.to_string();
    let response = client
        .request(method, url)
        // Яндекс принимает OAuth-токен в DAV через Basic (логин + токен).
        .basic_auth(email, Some(token))
        .header("Depth", depth)
        .header("Content-Type", "application/xml; charset=utf-8")
        .body(body.to_owned())
        .send()
        .await
        .map_err(|e| Error::Backend {
            backend: "dav".into(),
            message: e.to_string(),
        })?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if status == StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if status != StatusCode::MULTI_STATUS && !status.is_success() {
        return Err(Error::Backend {
            backend: "dav".into(),
            message: format!("{method_name} {url}: HTTP {status}: {text}"),
        });
    }
    Ok(Some(text))
}

fn resolve(base: &str, href: &str) -> Result<String> {
    Url::parse(base)
        .and_then(|url| url.join(href))
        .map(String::from)
        .map_err(|e| Error::Backend {
            backend: "dav-url".into(),
            message: e.to_string(),
        })
}

async fn discover_home(
    client: &Client,
    base: &str,
    home_tag: &str,
    email: &str,
    token: &str,
) -> Result<String> {
    let principal_xml =
        dav_request(client, "PROPFIND", base, "0", PRINCIPAL_BODY, email, token).await?;
    let principal_doc = Document::parse(&principal_xml).map_err(|e| Error::Backend {
        backend: "dav-xml".into(),
        message: e.to_string(),
    })?;
    // Нельзя брать первый <href>: это обычно URL самого multistatus-response,
    // а не current-user-principal. Из-за этого calendar-home-set искался не там.
    let principal = principal_doc
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "current-user-principal")
        .and_then(|node| {
            node.descendants()
                .find(|n| n.is_element() && n.tag_name().name() == "href")
        })
        .and_then(|node| node.text())
        .map(str::to_owned)
        .ok_or_else(|| Error::Backend {
            backend: "dav".into(),
            message: "current-user-principal не найден".into(),
        })?;
    let principal_url = resolve(base, &principal)?;
    let body = r#"<?xml version="1.0" encoding="utf-8"?><d:propfind xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav" xmlns:a="urn:ietf:params:xml:ns:carddav"><d:prop><c:calendar-home-set/><a:addressbook-home-set/></d:prop></d:propfind>"#;
    let xml = dav_request(client, "PROPFIND", &principal_url, "0", body, email, token).await?;
    let doc = Document::parse(&xml).map_err(|e| Error::Backend {
        backend: "dav-xml".into(),
        message: e.to_string(),
    })?;
    let home = doc
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == home_tag)
        .and_then(|node| {
            node.descendants()
                .find(|n| n.is_element() && n.tag_name().name() == "href")
        })
        .and_then(|node| node.text())
        .ok_or_else(|| Error::Backend {
            backend: "dav".into(),
            message: format!("{home_tag} не найден"),
        })?;
    resolve(base, home)
}

/// Проверить доступ к CalDAV и CardDAV без скачивания коллекций.
pub async fn validate_yandex_dav(email: &str, access_token: &str) -> Result<()> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| Error::Backend {
            backend: "dav".into(),
            message: e.to_string(),
        })?;
    discover_home(
        &client,
        "https://caldav.yandex.ru/",
        "calendar-home-set",
        email,
        access_token,
    )
    .await?;
    discover_home(
        &client,
        "https://carddav.yandex.ru/",
        "addressbook-home-set",
        email,
        access_token,
    )
    .await?;
    Ok(())
}

fn response_parts(xml: &str, data_tag: &str) -> Result<Vec<(String, Option<String>, String)>> {
    let doc = Document::parse(xml).map_err(|e| Error::Backend {
        backend: "dav-xml".into(),
        message: e.to_string(),
    })?;
    let mut out = Vec::new();
    for response in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "response")
    {
        let href = response
            .descendants()
            .find(|n| n.is_element() && n.tag_name().name() == "href")
            .and_then(|n| n.text())
            .unwrap_or("")
            .to_owned();
        let etag = response
            .descendants()
            .find(|n| n.is_element() && n.tag_name().name() == "getetag")
            .and_then(|n| n.text())
            .map(str::to_owned);
        if let Some(data) = response
            .descendants()
            .find(|n| n.is_element() && n.tag_name().name() == data_tag)
            .and_then(|n| n.text())
        {
            out.push((href, etag, data.to_owned()));
        }
    }
    Ok(out)
}

fn unfolded(raw: &str) -> String {
    raw.replace("=\r\n", "")
        .replace("\r\n ", "")
        .replace("\r\n\t", "")
}
fn decode_property(key: &str, value: &str) -> String {
    if key
        .split(';')
        .skip(1)
        .any(|part| part.eq_ignore_ascii_case("ENCODING=QUOTED-PRINTABLE"))
    {
        quoted_printable::decode(value, quoted_printable::ParseMode::Robust)
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            .unwrap_or_else(|_| value.to_owned())
    } else {
        value.to_owned()
    }
}
fn prop(raw: &str, name: &str) -> Option<String> {
    unfolded(raw).lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.split(';')
            .next()
            .filter(|key| key.eq_ignore_ascii_case(name))?;
        Some(
            decode_property(key, value)
                .replace("\\n", "\n")
                .replace("\\,", ",")
                .replace("\\;", ";"),
        )
    })
}

fn property_param(key: &str, name: &str) -> Option<String> {
    key.split(';').skip(1).find_map(|part| {
        let (param, value) = part.split_once('=')?;
        param
            .eq_ignore_ascii_case(name)
            .then(|| value.trim_matches('"').to_owned())
    })
}

fn parse_attendees(raw: &str) -> Vec<Attendee> {
    unfolded(raw)
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.split(';')
                .next()
                .is_some_and(|name| name.eq_ignore_ascii_case("ATTENDEE"))
                .then_some(())?;
            let value = value.trim();
            let email = value
                .get(..7)
                .filter(|prefix| prefix.eq_ignore_ascii_case("mailto:"))
                .map(|_| &value[7..])
                .unwrap_or(value)
                .trim()
                .to_owned();
            (!email.is_empty()).then(|| Attendee {
                email,
                name: property_param(key, "CN"),
                role: property_param(key, "ROLE"),
                partstat: property_param(key, "PARTSTAT"),
                rsvp: property_param(key, "RSVP")
                    .is_some_and(|value| value.eq_ignore_ascii_case("TRUE")),
            })
        })
        .collect()
}

fn parse_duration_minutes(value: &str) -> Option<i32> {
    let value = value.trim();
    let before_start = value.starts_with('-');
    let value = value.trim_start_matches(['-', '+']);
    let chars = value.strip_prefix('P')?.chars();
    let mut in_time = false;
    let mut number = String::new();
    let mut total_seconds: i64 = 0;
    for ch in chars {
        if ch == 'T' {
            in_time = true;
            continue;
        }
        if ch.is_ascii_digit() {
            number.push(ch);
            continue;
        }
        let amount: i64 = number.parse().ok()?;
        number.clear();
        total_seconds += match (ch, in_time) {
            ('W', false) => amount * 7 * 24 * 60 * 60,
            ('D', false) => amount * 24 * 60 * 60,
            ('H', true) => amount * 60 * 60,
            ('M', true) => amount * 60,
            ('S', true) => amount,
            _ => return None,
        };
    }
    if !number.is_empty() {
        return None;
    }
    let minutes = i32::try_from((total_seconds + 59) / 60).ok()?;
    Some(if before_start { minutes } else { -minutes })
}

fn parse_alarms(raw: &str) -> Vec<Alarm> {
    let mut alarms = Vec::new();
    let mut rest = raw;
    while let Some(start) = rest.find("BEGIN:VALARM") {
        rest = &rest[start..];
        let Some(relative_end) = rest.find("END:VALARM") else {
            break;
        };
        let block = &rest[..relative_end + "END:VALARM".len()];
        if let Some(trigger_minutes) = prop(block, "TRIGGER").and_then(|value| {
            // Absolute RFC5545 triggers are retained in raw data but cannot be
            // represented by the current minute-offset model.
            parse_duration_minutes(&value)
        }) {
            alarms.push(Alarm {
                trigger_minutes,
                action: prop(block, "ACTION").unwrap_or_else(|| "DISPLAY".into()),
            });
        }
        rest = &rest[relative_end + "END:VALARM".len()..];
    }
    alarms
}

fn parse_events(raw: String, etag: Option<String>, remote_url: Option<String>) -> Vec<DavEvent> {
    let mut events = Vec::new();
    let mut rest = raw.as_str();
    while let Some(relative_start) = rest.find("BEGIN:VEVENT") {
        rest = &rest[relative_start..];
        let Some(relative_end) = rest.find("END:VEVENT") else {
            break;
        };
        let end = relative_end + "END:VEVENT".len();
        let event = rest[..end].to_owned();
        if let (Some(uid), Some(dtstart)) = (prop(&event, "UID"), prop(&event, "DTSTART")) {
            events.push(DavEvent {
                remote_url: remote_url.clone(),
                uid,
                summary: prop(&event, "SUMMARY").unwrap_or_default(),
                description: prop(&event, "DESCRIPTION"),
                location: prop(&event, "LOCATION"),
                dtstart,
                dtend: prop(&event, "DTEND"),
                rrule: prop(&event, "RRULE"),
                recurrence_id: prop(&event, "RECURRENCE-ID"),
                exdates: prop(&event, "EXDATE"),
                rdates: prop(&event, "RDATE"),
                status: prop(&event, "STATUS"),
                attendees: parse_attendees(&event),
                alarms: parse_alarms(&event),
                timezone: prop(&event, "X-WR-TIMEZONE").or_else(|| {
                    unfolded(&event).lines().find_map(|line| {
                        let (key, _) = line.split_once(':')?;
                        key.split(';')
                            .next()
                            .is_some_and(|name| name.eq_ignore_ascii_case("DTSTART"))
                            .then(|| property_param(key, "TZID"))
                            .flatten()
                    })
                }),
                transp: prop(&event, "TRANSP"),
                class: prop(&event, "CLASS"),
                categories: prop(&event, "CATEGORIES")
                    .unwrap_or_default()
                    .split(',')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned)
                    .collect(),
                url: prop(&event, "URL"),
                organizer: prop(&event, "ORGANIZER").map(|value| {
                    value
                        .strip_prefix("mailto:")
                        .or_else(|| value.strip_prefix("MAILTO:"))
                        .unwrap_or(&value)
                        .to_owned()
                }),
                sequence: prop(&event, "SEQUENCE")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or_default(),
                raw: event,
                etag: etag.clone(),
            });
        }
        rest = &rest[end..];
    }
    events
}

fn parse_contact(
    raw: String,
    etag: Option<String>,
    remote_url: Option<String>,
) -> Option<DavContact> {
    let name = prop(&raw, "N").unwrap_or_default();
    let mut names = name.split(';');
    let last_name = names.next().filter(|v| !v.is_empty()).map(str::to_owned);
    let first_name = names.next().filter(|v| !v.is_empty()).map(str::to_owned);
    let emails = unfolded(&raw)
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.split(';')
                .next()
                .filter(|key| key.eq_ignore_ascii_case("EMAIL"))?;
            Some(decode_property(key, value))
        })
        .collect();
    Some(DavContact {
        remote_url,
        uid: prop(&raw, "UID")?,
        display_name: prop(&raw, "FN").unwrap_or_else(|| name.replace(';', " ").trim().to_owned()),
        first_name,
        last_name,
        organization: prop(&raw, "ORG"),
        emails,
        raw,
        etag,
    })
}

fn collection_unchanged(
    cursors: &HashMap<String, CollectionCursor>,
    url: &str,
    ctag: Option<&str>,
) -> bool {
    ctag.is_some() && cursors.get(url).and_then(|cursor| cursor.ctag.as_deref()) == ctag
}

fn collections_unchanged(
    cursors: &HashMap<String, CollectionCursor>,
    collections: &[DavCollection],
) -> bool {
    collections.len() == cursors.len()
        && collections.iter().all(|collection| {
            collection_unchanged(cursors, &collection.url, collection.ctag.as_deref())
        })
}

pub async fn sync_yandex_dav(
    email: &str,
    access_token: &str,
    cursors: &AuxiliarySyncCursors,
) -> Result<DavSyncResult> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(45))
        .build()
        .map_err(|e| Error::Backend {
            backend: "dav".into(),
            message: e.to_string(),
        })?;
    let cal_base = "https://caldav.yandex.ru/";
    let card_base = "https://carddav.yandex.ru/";
    let cal_home =
        discover_home(&client, cal_base, "calendar-home-set", email, access_token).await?;
    let card_home = discover_home(
        &client,
        card_base,
        "addressbook-home-set",
        email,
        access_token,
    )
    .await?;
    let list_body = r#"<?xml version="1.0"?><d:propfind xmlns:d="DAV:" xmlns:cs="http://calendarserver.org/ns/"><d:prop><d:displayname/><d:resourcetype/><cs:getctag/></d:prop></d:propfind>"#;
    let cal_xml = dav_request(
        &client,
        "PROPFIND",
        &cal_home,
        "1",
        list_body,
        email,
        access_token,
    )
    .await?;
    let doc = Document::parse(&cal_xml).map_err(|e| Error::Backend {
        backend: "dav-xml".into(),
        message: e.to_string(),
    })?;
    let mut calendars = Vec::new();
    for response in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "response")
    {
        if !response
            .descendants()
            .any(|n| n.is_element() && n.tag_name().name() == "calendar")
        {
            continue;
        }
        let href = response
            .descendants()
            .find(|n| n.is_element() && n.tag_name().name() == "href")
            .and_then(|n| n.text())
            .unwrap_or("");
        let url = resolve(cal_base, href)?;
        let name = response
            .descendants()
            .find(|n| n.is_element() && n.tag_name().name() == "displayname")
            .and_then(|n| n.text())
            .unwrap_or("Календарь")
            .to_owned();
        let ctag = response
            .descendants()
            .find(|n| n.is_element() && n.tag_name().name() == "getctag")
            .and_then(|n| n.text())
            .map(str::to_owned);
        let unchanged = collection_unchanged(&cursors.calendars, &url, ctag.as_deref());
        let events = if unchanged {
            Vec::new()
        } else {
            let report = r#"<?xml version="1.0"?><c:calendar-query xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav"><d:prop><d:getetag/><c:calendar-data/></d:prop><c:filter><c:comp-filter name="VCALENDAR"><c:comp-filter name="VEVENT"/></c:comp-filter></c:filter></c:calendar-query>"#;
            let event_xml =
                dav_request(&client, "REPORT", &url, "1", report, email, access_token).await?;
            response_parts(&event_xml, "calendar-data")?
                .into_iter()
                .flat_map(|(href, etag, raw)| {
                    let remote_url = resolve(cal_base, &href).ok();
                    parse_events(raw, etag, remote_url)
                })
                .collect()
        };
        calendars.push(DavCalendar {
            url,
            name,
            ctag,
            sync_token: None,
            sync_scope: if unchanged {
                SyncScope::Unchanged
            } else {
                SyncScope::Full
            },
            deleted_event_urls: Vec::new(),
            events,
        });
    }
    let Some(card_xml) = dav_request_optional(
        &client,
        "PROPFIND",
        &card_home,
        "1",
        list_body,
        email,
        access_token,
    )
    .await?
    else {
        return Ok(DavSyncResult {
            calendars,
            contacts: Vec::new(),
            contact_collections: Vec::new(),
            contacts_available: false,
            contacts_scope: SyncScope::Unchanged,
            contacts_sync_token: None,
            deleted_contact_urls: Vec::new(),
        });
    };
    let card_doc = Document::parse(&card_xml).map_err(|e| Error::Backend {
        backend: "dav-xml".into(),
        message: e.to_string(),
    })?;
    let mut addressbooks = Vec::new();
    for response in card_doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "response")
    {
        if !response
            .descendants()
            .any(|n| n.is_element() && n.tag_name().name() == "addressbook")
        {
            continue;
        }
        if let Some(href) = response
            .descendants()
            .find(|n| n.is_element() && n.tag_name().name() == "href")
            .and_then(|n| n.text())
        {
            let url = resolve(card_base, href)?;
            let ctag = response
                .descendants()
                .find(|n| n.is_element() && n.tag_name().name() == "getctag")
                .and_then(|n| n.text())
                .map(str::to_owned);
            addressbooks.push(DavCollection { url, ctag });
        }
    }
    if addressbooks.is_empty() {
        addressbooks.push(DavCollection {
            url: card_home,
            ctag: None,
        });
    }
    let contacts_unchanged = collections_unchanged(&cursors.contact_collections, &addressbooks);
    if contacts_unchanged {
        return Ok(DavSyncResult {
            calendars,
            contacts: Vec::new(),
            contact_collections: addressbooks,
            contacts_available: false,
            contacts_scope: SyncScope::Unchanged,
            contacts_sync_token: None,
            deleted_contact_urls: Vec::new(),
        });
    }
    let contacts_report = r#"<?xml version="1.0"?><a:addressbook-query xmlns:d="DAV:" xmlns:a="urn:ietf:params:xml:ns:carddav"><d:prop><d:getetag/><a:address-data/></d:prop></a:addressbook-query>"#;
    let mut contacts = Vec::new();
    for addressbook in &addressbooks {
        let Some(contact_xml) = dav_request_optional(
            &client,
            "REPORT",
            &addressbook.url,
            "1",
            contacts_report,
            email,
            access_token,
        )
        .await?
        else {
            continue;
        };
        contacts.extend(
            response_parts(&contact_xml, "address-data")?
                .into_iter()
                .filter_map(|(href, etag, raw)| {
                    let remote_url = resolve(card_base, &href).ok();
                    parse_contact(raw, etag, remote_url)
                }),
        );
    }
    Ok(DavSyncResult {
        calendars,
        contacts,
        contact_collections: addressbooks,
        contacts_available: true,
        contacts_scope: SyncScope::Full,
        contacts_sync_token: None,
        deleted_contact_urls: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_vevent_and_recurrence_override() {
        let raw = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:a\r\nDTSTART;TZID=Europe/Moscow:20260714T100000\r\nSUMMARY:Base\r\nRRULE:FREQ=DAILY\r\nEXDATE:20260715T100000Z\r\nTRANSP:TRANSPARENT\r\nCLASS:PRIVATE\r\nCATEGORIES:Team,Demo\r\nURL:https://example.test/meeting\r\nORGANIZER:mailto:owner@example.test\r\nSEQUENCE:4\r\nATTENDEE;CN=Guest;ROLE=REQ-PARTICIPANT;PARTSTAT=ACCEPTED;RSVP=FALSE:mailto:guest@example.test\r\nBEGIN:VALARM\r\nTRIGGER:-PT15M\r\nACTION:DISPLAY\r\nEND:VALARM\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:a\r\nRECURRENCE-ID:20260716T100000Z\r\nDTSTART:20260716T120000Z\r\nSUMMARY:Moved\r\nEND:VEVENT\r\nEND:VCALENDAR";
        let events = parse_events(raw.to_owned(), Some("etag".to_owned()), None);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].exdates.as_deref(), Some("20260715T100000Z"));
        assert_eq!(events[0].attendees[0].email, "guest@example.test");
        assert_eq!(events[0].attendees[0].partstat.as_deref(), Some("ACCEPTED"));
        assert_eq!(events[0].alarms[0].trigger_minutes, 15);
        assert_eq!(events[0].timezone.as_deref(), Some("Europe/Moscow"));
        assert_eq!(events[0].transp.as_deref(), Some("TRANSPARENT"));
        assert_eq!(events[0].class.as_deref(), Some("PRIVATE"));
        assert_eq!(events[0].categories, ["Team", "Demo"]);
        assert_eq!(
            events[0].url.as_deref(),
            Some("https://example.test/meeting")
        );
        assert_eq!(events[0].organizer.as_deref(), Some("owner@example.test"));
        assert_eq!(events[0].sequence, 4);
        assert_eq!(events[1].recurrence_id.as_deref(), Some("20260716T100000Z"));
    }

    #[test]
    fn decodes_vcard_quoted_printable_properties() {
        let raw = "BEGIN:VCARD\r\nVERSION:2.1\r\nUID:1\r\nFN;CHARSET=UTF-8;ENCODING=QUOTED-PRINTABLE:=D0=98=D0=B2=D0=B0=D0=BD\r\nEMAIL:test@example.com\r\nEND:VCARD";
        let contact = parse_contact(raw.to_owned(), None, None).expect("valid contact");
        assert_eq!(contact.display_name, "Иван");
        assert_eq!(contact.emails, ["test@example.com"]);
    }

    #[test]
    fn skips_only_collections_with_a_matching_ctag() {
        let cursors = HashMap::from([(
            "https://dav.test/calendar/".into(),
            CollectionCursor {
                ctag: Some("42".into()),
                sync_token: None,
            },
        )]);
        assert!(collection_unchanged(
            &cursors,
            "https://dav.test/calendar/",
            Some("42")
        ));
        assert!(!collection_unchanged(
            &cursors,
            "https://dav.test/calendar/",
            Some("43")
        ));
        assert!(!collection_unchanged(
            &cursors,
            "https://dav.test/calendar/",
            None
        ));
        assert!(!collections_unchanged(
            &cursors,
            &[
                DavCollection {
                    url: "https://dav.test/calendar/".into(),
                    ctag: Some("42".into()),
                },
                DavCollection {
                    url: "https://dav.test/new/".into(),
                    ctag: Some("1".into()),
                },
            ]
        ));
    }
}
