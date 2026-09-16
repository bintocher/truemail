//! Сбор обезличенного архива диагностики (issue #72,
//! specs/diagnostics-bundle.md). Обезличивание строк - чистые функции
//! `truemail_core::diagnostics`; здесь - файловый ввод-вывод: чтение
//! `data_dir/logs` по дескрипторам без перехода по символическим ссылкам,
//! потоковая упаковка в zip (прямая зависимость `zip` закреплена в этом
//! пакете) и атомарное переименование готового архива.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::Serialize;
use tauri::{AppHandle, State};
use truemail_core::diagnostics::{self, ReplacementCounts};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::commands::{ApiError, AppState};

/// Предел итогового размера архива (S-015 diagnostics-bundle.md): семь
/// суточных журналов сейчас занимают единицы мегабайт, 200 МиБ оставляют
/// запас на подробное логирование, не позволяя единичному сбою раздуть
/// архив без ограничения.
pub const MAX_ARCHIVE_BYTES: u64 = 200 * 1024 * 1024;

/// Временные файлы диагностики старше суток считаются брошенными прошлым
/// прерванным сбором и удаляются перед новым (см. "Ошибки и частичные отказы").
const STALE_TMP_AGE: Duration = Duration::from_secs(24 * 60 * 60);

/// Один пропущенный журнал и причина (для `manifest.json` и предупреждения
/// пользователю, S-008).
struct SkippedLog {
    name: String,
    reason: String,
}

/// Один успешно включённый журнал: имя, исходный и итоговый размер (S-007).
#[derive(Serialize, Clone)]
struct IncludedLog {
    name: String,
    original_bytes: u64,
    final_bytes: u64,
}

#[derive(Serialize)]
struct SkippedLogEntry {
    name: String,
    reason: String,
}

#[derive(Serialize)]
struct Manifest {
    app_version: String,
    os: String,
    generated_at_utc: String,
    included_files: Vec<IncludedLog>,
    skipped_files: Vec<SkippedLogEntry>,
    warnings: Vec<String>,
    replacement_counts: BTreeMap<String, u64>,
}

/// Результат успешного сбора - ровно то, что команда Tauri отдаёт интерфейсу.
#[derive(Debug)]
pub struct DiagnosticsBundle {
    pub archive_path: PathBuf,
    pub included_files: Vec<String>,
    pub skipped_files: usize,
    pub replacement_counts: BTreeMap<String, u64>,
}

/// Собирает архив диагностики из `data_dir/logs` в `data_dir/diagnostics`.
/// `app_version`/`os_label` идут в `manifest.json` как есть - в них нет
/// пользовательских данных. Ошибка - как в спецификации: без временного
/// файла и без частично записанного `.zip` (S-009, S-016).
pub fn collect_diagnostics_bundle(
    data_dir: &Path,
    app_version: &str,
    os_label: &str,
) -> Result<DiagnosticsBundle, String> {
    collect_diagnostics_bundle_with_limit(data_dir, app_version, os_label, MAX_ARCHIVE_BYTES)
}

/// Тот же сбор с настраиваемым пределом размера архива - предел вынесен
/// параметром только ради проверки S-015 без записи реальных 200 МиБ в тесте.
fn collect_diagnostics_bundle_with_limit(
    data_dir: &Path,
    app_version: &str,
    os_label: &str,
    max_archive_bytes: u64,
) -> Result<DiagnosticsBundle, String> {
    let logs_dir = data_dir.join("logs");
    let diagnostics_dir = data_dir.join("diagnostics");
    fs::create_dir_all(&diagnostics_dir)
        .map_err(|error| format!("не удалось создать папку diagnostics: {error}"))?;
    cleanup_stale_tmp(&diagnostics_dir);

    let candidates = discover_log_candidates(&logs_dir)
        .map_err(|error| format!("не удалось прочитать каталог журналов: {error}"))?;

    let file_name = format!(
        "truemail-diagnostics-{}-{}.zip",
        chrono::Utc::now().format("%Y%m%d-%H%M%S"),
        random_suffix()
    );
    let final_path = diagnostics_dir.join(&file_name);
    let tmp_path = diagnostics_dir.join(format!("{file_name}.tmp"));

    match write_bundle(
        &candidates,
        &tmp_path,
        app_version,
        os_label,
        max_archive_bytes,
    ) {
        Ok((included, skipped, counts)) => {
            if included.is_empty() {
                let _ = fs::remove_file(&tmp_path);
                return Err(
                    "Не удалось собрать диагностику: журналы недоступны или отсутствуют".to_owned(),
                );
            }
            if let Err(error) = fs::rename(&tmp_path, &final_path) {
                let _ = fs::remove_file(&tmp_path);
                return Err(format!("не удалось сохранить архив: {error}"));
            }
            Ok(DiagnosticsBundle {
                archive_path: final_path,
                included_files: included,
                skipped_files: skipped,
                replacement_counts: counts,
            })
        }
        Err(error) => {
            // S-016: любая ошибка после создания временного файла и до
            // переименования удаляет его целиком - никакого частичного .zip.
            let _ = fs::remove_file(&tmp_path);
            Err(error)
        }
    }
}

/// Удаляет только СВОИ устаревшие `.tmp`-файлы (старше суток) - брошенные
/// прошлым прерванным сбором после неожиданного завершения процесса.
fn cleanup_stale_tmp(diagnostics_dir: &Path) {
    let Ok(entries) = fs::read_dir(diagnostics_dir) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("tmp") {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        if now.duration_since(modified).unwrap_or_default() >= STALE_TMP_AGE {
            let _ = fs::remove_file(&path);
        }
    }
}

fn random_suffix() -> String {
    use rand::RngExt as _;
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::rng();
    (0..4)
        .map(|_| ALPHABET[rng.random_range(0..ALPHABET.len())] as char)
        .collect()
}

/// Перечисляет файлы журналов, лежащие непосредственно в `logs_dir`, чьё имя
/// начинается с `truemail.log` (S-002). Не заходит во вложенные каталоги;
/// собственно защита от символических ссылок - при открытии по дескриптору
/// в [`open_regular_file_no_follow`], не здесь: список путей отсюда может
/// устареть до открытия, и решение принимается заново по уже открытому файлу.
fn discover_log_candidates(logs_dir: &Path) -> io::Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(logs_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        if name.to_string_lossy().starts_with("truemail.log") {
            paths.push(entry.path());
        }
    }
    paths.sort();
    Ok(paths)
}

/// Открывает файл по пути и сразу проверяет тип уже открытого дескриптора -
/// обычный файл, не символическая ссылка и не иной особый тип (S-002). На
/// Windows флаг `FILE_FLAG_OPEN_REPARSE_POINT` не даёт `CreateFile` прозрачно
/// пройти сквозь reparse point: если путь - символическая ссылка, дескриптор
/// указывает на саму ссылку, и метаданные с этого дескриптора показывают её
/// как символическую ссылку, а не как файл цели. На Unix `O_NOFOLLOW` не
/// даёт `open` вовсе войти в символическую ссылку - подмена между проверкой
/// имени и открытием (TOCTOU) в обоих случаях не проходит, потому что решение
/// принимается только по самому дескриптору, а не по повторному имени.
fn open_regular_file_no_follow(path: &Path) -> io::Result<File> {
    let file = open_no_follow(path)?;
    let metadata = file.metadata()?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(io::Error::other(
            "не обычный файл (символическая ссылка или иной особый тип)",
        ));
    }
    Ok(file)
}

#[cfg(windows)]
fn open_no_follow(path: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    // FILE_FLAG_OPEN_REPARSE_POINT - см. winnt.h / fileapi.h.
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

#[cfg(unix)]
fn open_no_follow(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
}

/// Пишущая сторона, считающая записанные байты и обрывающая запись при
/// превышении предела (S-015 diagnostics-bundle.md): ошибка ввода-вывода из
/// этого `write` доходит до `zip::ZipWriter` как обычная ошибка записи, и вся
/// команда завершается по общей ветке "ошибка после создания временного
/// файла" (S-016), не оставляя частичного архива. То же самое покрывает
/// нехватку места на диске - `File::write` вернёт ошибку ОС независимо от
/// причины.
struct LimitedFile {
    inner: File,
    written: u64,
    limit: u64,
}

impl Write for LimitedFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.written.saturating_add(buf.len() as u64) > self.limit {
            return Err(io::Error::other(
                "предел размера архива диагностики (200 МиБ) превышен",
            ));
        }
        let written = self.inner.write(buf)?;
        self.written += written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl Seek for LimitedFile {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.inner.seek(pos)
    }
}

type Bundle = (Vec<String>, usize, BTreeMap<String, u64>);

fn write_bundle(
    candidates: &[PathBuf],
    tmp_path: &Path,
    app_version: &str,
    os_label: &str,
    max_archive_bytes: u64,
) -> Result<Bundle, String> {
    let file = File::create(tmp_path)
        .map_err(|error| format!("не удалось создать временный файл: {error}"))?;
    let limited = LimitedFile {
        inner: file,
        written: 0,
        limit: max_archive_bytes,
    };
    let mut zip = ZipWriter::new(limited);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    // Соль - на один архив (S-004, S-005): создаётся здесь и никогда не
    // покидает эту функцию.
    let mut salt = [0u8; 16];
    {
        use rand::Rng as _;
        rand::rng().fill_bytes(&mut salt);
    }
    let mut counts = ReplacementCounts::default();
    let mut included = Vec::new();
    let mut skipped = Vec::new();
    let mut warnings = Vec::new();

    for path in candidates {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        match write_one_log(
            &mut zip,
            options,
            &salt,
            &mut counts,
            path,
            &name,
            &mut warnings,
        ) {
            Ok(summary) => included.push(summary),
            Err(reason) => skipped.push(SkippedLog { name, reason }),
        }
    }

    let manifest = Manifest {
        app_version: app_version.to_owned(),
        os: os_label.to_owned(),
        generated_at_utc: chrono::Utc::now().to_rfc3339(),
        included_files: included.clone(),
        skipped_files: skipped
            .iter()
            .map(|entry| SkippedLogEntry {
                name: entry.name.clone(),
                reason: entry.reason.clone(),
            })
            .collect(),
        warnings,
        replacement_counts: counts.0.clone(),
    };
    let manifest_json = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| format!("не удалось собрать manifest.json: {error}"))?;
    zip.start_file("manifest.json", options)
        .map_err(|error| format!("zip: {error}"))?;
    zip.write_all(&manifest_json)
        .map_err(|error| format!("zip: {error}"))?;
    zip.finish()
        .map_err(|error| format!("не удалось завершить архив: {error}"))?;

    Ok((
        included.into_iter().map(|entry| entry.name).collect(),
        skipped.len(),
        counts.0,
    ))
}

#[allow(clippy::too_many_arguments)]
fn write_one_log(
    zip: &mut ZipWriter<LimitedFile>,
    options: SimpleFileOptions,
    salt: &[u8; 16],
    counts: &mut ReplacementCounts,
    path: &Path,
    name: &str,
    warnings: &mut Vec<String>,
) -> Result<IncludedLog, String> {
    let file = open_regular_file_no_follow(path).map_err(|error| error.to_string())?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("не удалось прочитать метаданные: {error}"))?;
    // S-013: снимок длины на момент открытия - активный журнал может расти
    // дальше, но читаем не больше того, что было на момент открытия.
    let initial_len = metadata.len();
    zip.start_file(name, options)
        .map_err(|error| format!("zip: {error}"))?;
    let mut reader = BufReader::new(file).take(initial_len);
    let mut final_bytes: u64 = 0;
    loop {
        match diagnostics::read_capped_line(&mut reader, diagnostics::MAX_LINE_BYTES) {
            Ok(Some(line)) => {
                if line.truncated {
                    warnings.push(format!("{name}: строка длиннее 64 КиБ обрезана"));
                }
                let text = diagnostics::lossy_utf8_question_mark(&line.bytes);
                if text.contains('\u{fffd}') {
                    // lossy_utf8_question_mark сам не оставляет U+FFFD - это
                    // невозможная ветка, но на всякий случай не молчим о ней.
                    warnings.push(format!("{name}: не полностью обработанная кодировка"));
                }
                let anonymized = diagnostics::anonymize_line(&text, salt, counts);
                zip.write_all(anonymized.as_bytes())
                    .map_err(|error| format!("zip: {error}"))?;
                zip.write_all(b"\n")
                    .map_err(|error| format!("zip: {error}"))?;
                final_bytes += anonymized.len() as u64 + 1;
            }
            Ok(None) => break,
            Err(error) => return Err(format!("ошибка чтения журнала: {error}")),
        }
    }
    Ok(IncludedLog {
        name: name.to_owned(),
        original_bytes: initial_len,
        final_bytes,
    })
}

/// Ответ команды Tauri интерфейсу (раздел "Интерфейсы и данные"
/// diagnostics-bundle.md): `folder_opened` - удалась ли попытка открыть
/// папку системным средством, не влияет на успех самого сбора (S-010).
#[derive(Serialize)]
pub struct DiagnosticsBundleResponse {
    pub archive_path: String,
    pub included_files: Vec<String>,
    pub skipped_files: usize,
    pub replacement_counts: BTreeMap<String, u64>,
    pub folder_opened: bool,
}

/// Команда интерфейса (S-001): одна кнопка, один вызов ядра без передачи
/// журналов через JavaScript. Тяжёлый ввод-вывод и сжатие идут в
/// `spawn_blocking`, чтобы не занимать поток исполнителя Tokio.
#[tauri::command]
pub async fn create_diagnostics_bundle(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<DiagnosticsBundleResponse, ApiError> {
    // "Скорость и потребление ресурсов": одновременно разрешён один сбор -
    // повторное нажатие видит уже активное состояние, а не второй сбор.
    if state
        .diagnostics_running
        .swap(true, std::sync::atomic::Ordering::SeqCst)
    {
        return Err(ApiError {
            message: "Сбор диагностики уже выполняется".into(),
        });
    }
    let data_dir = truemail_core::crypto::load_data_dir()
        .ok()
        .flatten()
        .unwrap_or_else(crate::default_data_dir);
    let app_version = app.package_info().version.to_string();
    let os_label = std::env::consts::OS.to_owned();
    let outcome = tokio::task::spawn_blocking(move || {
        collect_diagnostics_bundle(&data_dir, &app_version, &os_label)
    })
    .await;
    state
        .diagnostics_running
        .store(false, std::sync::atomic::Ordering::SeqCst);
    let bundle = match outcome {
        Ok(Ok(bundle)) => bundle,
        Ok(Err(message)) => return Err(ApiError { message }),
        Err(join_error) => {
            return Err(ApiError {
                message: format!("сбор диагностики прерван: {join_error}"),
            });
        }
    };
    // S-010: открытие папки системным средством не должно проваливать
    // успешный сбор, даже если сама попытка открытия не удалась - путь в
    // любом случае показывается пользователю.
    let folder_opened = bundle
        .archive_path
        .parent()
        .map(open_in_file_manager)
        .transpose()
        .map(|opened| opened.is_some())
        .unwrap_or(false);
    Ok(DiagnosticsBundleResponse {
        archive_path: bundle.archive_path.display().to_string(),
        included_files: bundle.included_files,
        skipped_files: bundle.skipped_files,
        replacement_counts: bundle.replacement_counts,
        folder_opened,
    })
}

// Открытие папки системным средством - тот же приём, что у open_data_dir
// (commands.rs), но отдельная маленькая копия здесь: правки одного места не
// должны рисковать поведением другого, а код короче совместного рефакторинга.
#[cfg(target_os = "windows")]
fn open_in_file_manager(path: &Path) -> io::Result<()> {
    std::process::Command::new("explorer.exe")
        .arg(path)
        .spawn()?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn open_in_file_manager(path: &Path) -> io::Result<()> {
    std::process::Command::new("open").arg(path).spawn()?;
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_in_file_manager(path: &Path) -> io::Result<()> {
    std::process::Command::new("xdg-open").arg(path).spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "truemail-diagnostics-test-{tag}-{}-{}",
            std::process::id(),
            random_suffix()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("временный каталог создан");
        dir
    }

    fn read_zip_names(path: &Path) -> Vec<String> {
        let file = File::open(path).expect("архив открыт");
        let mut archive = zip::ZipArchive::new(file).expect("это zip");
        (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_owned())
            .collect()
    }

    fn read_zip_entry(path: &Path, entry: &str) -> String {
        let file = File::open(path).expect("архив открыт");
        let mut archive = zip::ZipArchive::new(file).expect("это zip");
        let mut out = String::new();
        archive
            .by_name(entry)
            .expect("запись есть")
            .read_to_string(&mut out)
            .expect("запись читается");
        out
    }

    #[test]
    fn s009_no_logs_leaves_no_archive_or_temp_file() {
        let root = temp_dir("no-logs");
        let data_dir = root.join("data");
        fs::create_dir_all(data_dir.join("logs")).unwrap();
        let error = collect_diagnostics_bundle(&data_dir, "0.0.0-test", "test-os").unwrap_err();
        assert!(error.contains("журналы недоступны"));
        let diagnostics_dir = data_dir.join("diagnostics");
        let leftovers: Vec<_> = fs::read_dir(&diagnostics_dir)
            .map(|entries| entries.flatten().collect())
            .unwrap_or_default();
        assert!(
            leftovers.is_empty(),
            "не должно остаться файлов: {leftovers:?}"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn s009_missing_logs_dir_is_not_an_io_error() {
        let root = temp_dir("missing-dir");
        let data_dir = root.join("data");
        fs::create_dir_all(&data_dir).unwrap();
        let error = collect_diagnostics_bundle(&data_dir, "0.0.0-test", "test-os").unwrap_err();
        assert!(error.contains("журналы недоступны"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn s006_and_s003_creates_named_archive_with_anonymized_content_and_manifest() {
        let root = temp_dir("happy-path");
        let data_dir = root.join("data");
        let logs_dir = data_dir.join("logs");
        fs::create_dir_all(&logs_dir).unwrap();
        fs::write(
            logs_dir.join("truemail.log"),
            b"account_id=1 email ivan@example.com host mail.example.com\n",
        )
        .unwrap();
        let result = collect_diagnostics_bundle(&data_dir, "1.2.3", "windows-11").unwrap();
        assert!(
            result
                .archive_path
                .starts_with(data_dir.join("diagnostics"))
        );
        let file_name = result
            .archive_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert!(file_name.starts_with("truemail-diagnostics-"));
        assert!(file_name.ends_with(".zip"));
        assert!(!file_name.ends_with(".tmp"));
        assert!(result.archive_path.exists());
        assert!(!result.archive_path.with_extension("zip.tmp").exists());
        assert_eq!(result.included_files, vec!["truemail.log".to_owned()]);
        assert_eq!(result.skipped_files, 0);

        let names = read_zip_names(&result.archive_path);
        assert!(names.contains(&"manifest.json".to_owned()));
        assert!(names.contains(&"truemail.log".to_owned()));

        let log_content = read_zip_entry(&result.archive_path, "truemail.log");
        assert!(!log_content.contains("ivan@example.com"));
        assert!(!log_content.contains("mail.example.com"));

        let manifest = read_zip_entry(&result.archive_path, "manifest.json");
        assert!(manifest.contains("\"app_version\": \"1.2.3\""));
        assert!(manifest.contains("\"os\": \"windows-11\""));
        assert!(!manifest.contains("ivan@example.com"));
        assert!(!manifest.contains(&data_dir.to_string_lossy().into_owned()));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn s002_rejects_symlink_and_nested_and_non_prefixed_files() {
        let root = temp_dir("s002");
        let data_dir = root.join("data");
        let logs_dir = data_dir.join("logs");
        fs::create_dir_all(logs_dir.join("nested")).unwrap();
        fs::write(logs_dir.join("truemail.log"), b"regular ok\n").unwrap();
        fs::write(logs_dir.join("other.log"), b"not our prefix\n").unwrap();
        fs::write(
            logs_dir.join("nested").join("truemail.log.nested"),
            b"deep\n",
        )
        .unwrap();
        let target = root.join("outside-target.log");
        fs::write(&target, b"secret target content ivan@example.com\n").unwrap();
        #[cfg(windows)]
        {
            let _ = std::os::windows::fs::symlink_file(&target, logs_dir.join("truemail.log.link"));
        }
        #[cfg(unix)]
        {
            let _ = std::os::unix::fs::symlink(&target, logs_dir.join("truemail.log.link"));
        }
        let result = collect_diagnostics_bundle(&data_dir, "1.0.0", "test-os").unwrap();
        assert_eq!(result.included_files, vec!["truemail.log".to_owned()]);
        let content = read_zip_entry(&result.archive_path, "truemail.log");
        assert_eq!(content.trim_end(), "regular ok");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn s008_partial_archive_when_one_log_is_unreadable() {
        let root = temp_dir("s008");
        let data_dir = root.join("data");
        let logs_dir = data_dir.join("logs");
        fs::create_dir_all(&logs_dir).unwrap();
        fs::write(logs_dir.join("truemail.log"), b"ok line\n").unwrap();
        // Каталог с именем truemail.log.dir не должен читаться как файл -
        // это тоже "не обычный файл" по S-002, что и создаёт частичный сбор.
        fs::create_dir_all(logs_dir.join("truemail.log.dir")).unwrap();
        let result = collect_diagnostics_bundle(&data_dir, "1.0.0", "test-os").unwrap();
        assert_eq!(result.included_files, vec!["truemail.log".to_owned()]);
        assert_eq!(result.skipped_files, 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn s015_and_s016_size_limit_aborts_and_leaves_no_file_at_all() {
        let root = temp_dir("s015-s016");
        let data_dir = root.join("data");
        let logs_dir = data_dir.join("logs");
        fs::create_dir_all(&logs_dir).unwrap();
        // Строка длиннее заведомо крошечного предела архива - запись в zip
        // обязана упасть ещё на первом файле.
        fs::write(logs_dir.join("truemail.log"), "x".repeat(4096) + "\n").unwrap();
        let error =
            collect_diagnostics_bundle_with_limit(&data_dir, "1.0.0", "test-os", 256).unwrap_err();
        assert!(
            error.contains("недоступны") || error.contains("архив") || error.contains("zip"),
            "неожиданный текст ошибки: {error}"
        );
        let diagnostics_dir = data_dir.join("diagnostics");
        let leftovers: Vec<_> = fs::read_dir(&diagnostics_dir)
            .map(|entries| {
                entries
                    .flatten()
                    .map(|entry| entry.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        assert!(
            leftovers.is_empty(),
            "не должно остаться ни временного, ни частичного архива: {leftovers:?}"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn limited_file_rejects_writes_past_its_cap() {
        let root = temp_dir("limited-file");
        let path = root.join("probe.bin");
        let file = File::create(&path).unwrap();
        let mut limited = LimitedFile {
            inner: file,
            written: 0,
            limit: 8,
        };
        assert_eq!(limited.write(b"1234").unwrap(), 4);
        assert!(
            limited.write(b"56789").is_err(),
            "запись сверх предела должна падать"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn cleanup_only_removes_own_stale_tmp_files() {
        let root = temp_dir("cleanup");
        let diagnostics_dir = root.join("diagnostics");
        fs::create_dir_all(&diagnostics_dir).unwrap();
        let stale = diagnostics_dir.join("truemail-diagnostics-old.zip.tmp");
        let fresh = diagnostics_dir.join("truemail-diagnostics-fresh.zip.tmp");
        let kept_zip = diagnostics_dir.join("truemail-diagnostics-done.zip");
        fs::write(&stale, b"stale").unwrap();
        fs::write(&fresh, b"fresh").unwrap();
        fs::write(&kept_zip, b"done").unwrap();
        let old_time = SystemTime::now() - Duration::from_secs(25 * 60 * 60);
        set_mtime(&stale, old_time);
        cleanup_stale_tmp(&diagnostics_dir);
        assert!(!stale.exists());
        assert!(fresh.exists());
        assert!(kept_zip.exists());
        let _ = fs::remove_dir_all(&root);
    }

    fn set_mtime(path: &Path, time: SystemTime) {
        let file = OpenOptions::new().write(true).open(path).unwrap();
        file.set_modified(time).unwrap();
    }
}
