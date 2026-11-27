use crate::commands::{AppState, CmdResult, core};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use chrono::{Days, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::io::BufReader;
use std::path::Path;
use tauri::State;
use truemail_core::{crypto::StorageCrypto, diagnostics::read_capped_line};

use tracing_subscriber::fmt::{
    FmtContext,
    format::{Format, FormatEvent, FormatFields, Writer},
};
use tracing_subscriber::registry::LookupSpan;

/// Redact complete log events, including their messages, fields and span names.
pub struct MaskedEvent(Format);

impl Default for MaskedEvent {
    fn default() -> Self {
        Self(tracing_subscriber::fmt::format().with_ansi(false))
    }
}

impl<S, N> FormatEvent<S, N> for MaskedEvent
where
    S: tracing::Subscriber + for<'lookup> LookupSpan<'lookup>,
    N: for<'writer> FormatFields<'writer> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &tracing::Event<'_>,
    ) -> std::fmt::Result {
        let mut complete = String::new();
        self.0
            .format_event(ctx, Writer::new(&mut complete), event)?;
        writer.write_str(&truemail_core::logging::mask_error_text(&complete))
    }
}

pub const LOG_DAYS: u64 = 7;
const LOG_PREFIX: &str = "truemail.log.";
const RECORD_PREFIX: &str = " truemail::activity: ui_activity ";
const RECORD_AAD: &[u8] = b"truemail:activity-log:v1";
const MAX_RECORD_SIZE: usize = 16 * 1024;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityRecord {
    id: String,
    time: i64,
    level: String,
    kind: String,
    text: String,
    details: String,
    count: usize,
    action_status: String,
}

pub fn prune_logs(directory: &Path, today: NaiveDate) {
    let Some(oldest) = today.checked_sub_days(Days::new(LOG_DAYS - 1)) else {
        return;
    };
    let Ok(files) = std::fs::read_dir(directory) else {
        return;
    };
    for file in files.flatten() {
        let name = file.file_name();
        let date = name
            .to_str()
            .and_then(|name| name.strip_prefix(LOG_PREFIX))
            .and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok());
        if date.is_some_and(|date| date < oldest) {
            let _ = std::fs::remove_file(file.path());
        }
    }
}

#[tauri::command]
pub async fn record_ui_activity(
    state: State<'_, AppState>,
    mut record: Option<ActivityRecord>,
) -> CmdResult<()> {
    let core = core(&state).await?;
    if let Some(record) = record.as_mut() {
        record.id.truncate(record.id.floor_char_boundary(100));
        record
            .kind
            .retain(|ch| ch.is_ascii_alphanumeric() || ch == '_');
        record.kind.truncate(60);
        if record.level != "error" {
            record.level = "info".into();
        }
        record.text.truncate(record.text.floor_char_boundary(1_600));
        record
            .details
            .truncate(record.details.floor_char_boundary(800));
        record
            .action_status
            .truncate(record.action_status.floor_char_boundary(160));
    }
    let Ok(json) = serde_json::to_vec(&record) else {
        return Ok(());
    };
    if json.len() <= MAX_RECORD_SIZE {
        let payload = STANDARD.encode(core.crypto.encrypt_with_aad(&json, RECORD_AAD)?);
        // Текст записи может содержать тему и адресатов. В открытом журнале
        // доступны только вид события и уровень, содержимое читает приложение.
        let kind = record
            .as_ref()
            .map_or("clear", |record| record.kind.as_str());
        let level = record
            .as_ref()
            .map_or("info", |record| record.level.as_str());
        write_event(kind, level, &payload);
    }
    Ok(())
}

fn write_event(kind: &str, level: &str, payload: &str) {
    tracing::info!(target: "truemail::activity", kind, level, "ui_activity {payload}");
}

fn read_history(directory: &Path, crypto: &StorageCrypto, limit: usize) -> Vec<ActivityRecord> {
    let today = Utc::now().date_naive();
    let mut history: VecDeque<ActivityRecord> = VecDeque::new();
    for age in (0..LOG_DAYS).rev() {
        let Some(date) = today.checked_sub_days(Days::new(age)) else {
            continue;
        };
        let Ok(file) = std::fs::File::open(directory.join(format!("{LOG_PREFIX}{date}"))) else {
            continue;
        };
        let mut reader = BufReader::new(file);
        while let Ok(Some(line)) = read_capped_line(&mut reader, 2 * MAX_RECORD_SIZE) {
            let Some(record) = decode_record(&line.bytes, crypto) else {
                continue;
            };
            if let Some(record) = record {
                history.retain(|old| old.id != record.id);
                let index = history
                    .iter()
                    .position(|old| old.time <= record.time)
                    .unwrap_or(history.len());
                history.insert(index, record);
                history.truncate(limit);
            } else {
                history.clear();
            }
        }
    }
    history.into()
}

fn decode_record(line: &[u8], crypto: &StorageCrypto) -> Option<Option<ActivityRecord>> {
    let encrypted = STANDARD.decode(event_payload(line)?).ok()?;
    let json = crypto.decrypt_with_aad(&encrypted, RECORD_AAD).ok()?;
    serde_json::from_slice(&json).ok()
}

fn event_payload(line: &[u8]) -> Option<&str> {
    let (_, tail) = std::str::from_utf8(line).ok()?.split_once(RECORD_PREFIX)?;
    tail.split_ascii_whitespace().next()
}

#[tauri::command]
pub async fn read_ui_activity(
    state: State<'_, AppState>,
    limit: usize,
) -> CmdResult<Vec<ActivityRecord>> {
    let core = core(&state).await?;
    let directory = state.log_dir.clone();
    Ok(tauri::async_runtime::spawn_blocking(move || {
        read_history(&directory, &core.crypto, limit.clamp(1, 1000))
    })
    .await
    .unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::{MaskedEvent, event_payload, write_event};
    use std::io::{self, Write};
    use std::sync::{Arc, Mutex};

    #[test]
    fn reads_the_payload_from_the_programs_log_format() {
        struct Buffer(Arc<Mutex<Vec<u8>>>);

        impl Write for Buffer {
            fn write(&mut self, data: &[u8]) -> io::Result<usize> {
                self.0.lock().unwrap().write(data)
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        let buffer = Arc::new(Mutex::new(Vec::new()));
        let writer = buffer.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .event_format(MaskedEvent::default())
            .with_writer(move || Buffer(writer.clone()))
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            write_event("network_unavailable", "error", "YWJjZA==");
            tracing::warn!(recipient = "person@example.test", error = ?String::from("550 <copy@example.org>"), "Rejected bcc@example.com");
        });
        let output = buffer.lock().unwrap();
        assert_eq!(event_payload(&output), Some("YWJjZA=="));
        let output = std::str::from_utf8(&output).unwrap();
        assert!(!output.contains('\x1b'));
        for (original, masked) in [
            ("person@example.test", "p***@example.test"),
            ("copy@example.org", "c***@example.org"),
            ("bcc@example.com", "b***@example.com"),
        ] {
            assert!(!output.contains(original));
            assert!(output.contains(masked));
        }
    }
}
