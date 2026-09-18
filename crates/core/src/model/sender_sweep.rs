//! Автоочистка писем по отправителю: режимы уборки, границы значений и отчёт
//! прохода (specs/sweep-by-sender.md).
//!
//! Нормализация адреса сюда не переписывается: она общая со списками
//! отправителей (S-046) и живёт в `model::sender_policy`.

use serde::{Deserialize, Serialize};

/// Название стадии автоочистки в столбце результата стадии письма
/// (specs/mail-rules-conditions-and-actions.md, S-009).
pub const SENDER_SWEEP_STAGE_NAME: &str = "sender_sweep";

/// Режимы постоянной уборки. Режим "новые сразу" записи автоочистки не
/// создаёт: он целиком выражается обычным правилом (S-023, S-025).
pub const SWEEP_MODE_ONLY_LAST: &str = "only_last";
pub const SWEEP_MODE_OLDER_THAN: &str = "older_than";
/// Разовая уборка уже полученных писем, без постоянной записи (S-022).
pub const SWEEP_MODE_ONCE: &str = "once";
/// Режим "новые сразу" приходит из интерфейса и превращается в обычное правило
/// (S-023).
pub const SWEEP_MODE_NEW_NOW: &str = "new_now";

/// Границы числа дней режима "старше N дней" (S-032).
pub const MIN_SWEEP_DAYS: i64 = 1;
pub const MAX_SWEEP_DAYS: i64 = 3650;

/// Проход ждёт чужую операцию не чаще раза в минуту и не более восьми раз
/// (S-020, S-021).
pub const SWEEP_WAIT_SECONDS: i64 = 60;
pub const SWEEP_MAX_WAITS: i64 = 8;
/// Полный проход включённой записи выполняется не реже раза в сутки (S-035).
pub const SWEEP_FULL_PASS_HOURS: i64 = 24;

/// Состояния прохода (S-019 - S-021, S-041, S-043).
pub const SWEEP_JOB_PENDING: &str = "pending";
pub const SWEEP_JOB_RUNNING: &str = "running";
pub const SWEEP_JOB_WAITING: &str = "waiting_operation";
pub const SWEEP_JOB_COMPLETED: &str = "completed";
pub const SWEEP_JOB_CANCELLED: &str = "cancelled";
pub const SWEEP_JOB_FAILED: &str = "failed";

/// Запись автоочистки в общем списке правил (S-037).
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SenderSweepRule {
    pub id: i64,
    pub address: String,
    /// Пусто - область всех ящиков (S-026).
    pub account_id: Option<i64>,
    pub mode: String,
    pub days: Option<i64>,
    /// Уборка писем из папок с ролью archive по отдельному согласию (S-016).
    pub sweep_archive: bool,
    pub enabled: bool,
    pub last_full_pass_at: Option<String>,
    pub next_check_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// Счётчики последнего прохода (S-040).
    #[sqlx(default)]
    pub queued: i64,
    #[sqlx(default)]
    pub skipped: i64,
    #[sqlx(default)]
    pub failed: i64,
    #[sqlx(default)]
    pub job_state: Option<String>,
    #[sqlx(default)]
    pub last_error: Option<String>,
}

/// Что интерфейс просит создать: вид уборки, область и согласие на архив.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SenderSweepInput {
    pub address: String,
    pub mode: String,
    #[serde(default)]
    pub account_id: Option<i64>,
    #[serde(default)]
    pub days: Option<i64>,
    #[serde(default)]
    pub sweep_archive: bool,
}

/// Предварительный подсчёт: число писем, их распределение по папкам и ключ
/// снимка (S-010, S-011).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SenderSweepPreview {
    pub address: String,
    pub mode: String,
    pub account_id: Option<i64>,
    pub days: Option<i64>,
    pub sweep_archive: bool,
    pub snapshot_key: String,
    pub total: i64,
    pub folders: Vec<SenderSweepFolderCount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SenderSweepFolderCount {
    pub folder_id: i64,
    pub account_id: i64,
    pub name: String,
    pub role: Option<String>,
    pub count: i64,
}

/// Отчёт прохода (S-040).
#[derive(Debug, Clone, Default, Serialize, Deserialize, sqlx::FromRow)]
pub struct SenderSweepJobReport {
    pub id: i64,
    pub rule_id: Option<i64>,
    pub state: String,
    pub found: i64,
    pub queued: i64,
    pub skipped: i64,
    pub failed: i64,
    pub remaining: i64,
    /// Перемещения, которые уже выполняются и отменены быть не могут (S-041).
    #[sqlx(default)]
    pub irreversible: i64,
}

pub fn is_sweep_mode(mode: &str) -> bool {
    matches!(
        mode,
        SWEEP_MODE_ONLY_LAST | SWEEP_MODE_OLDER_THAN | SWEEP_MODE_ONCE | SWEEP_MODE_NEW_NOW
    )
}

/// Режим, который создаёт постоянную запись автоочистки (S-025).
pub fn is_persistent_mode(mode: &str) -> bool {
    matches!(mode, SWEEP_MODE_ONLY_LAST | SWEEP_MODE_OLDER_THAN)
}

/// Проверка состава уборки до открытия неделимой операции: отказ ничего не
/// создаёт (S-032).
pub fn validate_sweep_input(input: &SenderSweepInput) -> Result<(), String> {
    if !is_sweep_mode(&input.mode) {
        return Err(format!("вид уборки {} не поддерживается", input.mode));
    }
    if input.mode == SWEEP_MODE_OLDER_THAN {
        let days = input
            .days
            .ok_or_else(|| "для режима \"старше N дней\" нужно число дней".to_owned())?;
        if !(MIN_SWEEP_DAYS..=MAX_SWEEP_DAYS).contains(&days) {
            return Err(format!(
                "число дней задаётся целым от {MIN_SWEEP_DAYS} до {MAX_SWEEP_DAYS}"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Границы числа дней и обязательность его для режима "старше N дней"
    /// (S-032): проверка выполняется до открытия неделимой операции, иначе
    /// запись с числом вне границ упёрлась бы в ограничение схемы уже после
    /// создания задания.
    #[test]
    fn sweep_input_bounds() {
        let input = |mode: &str, days: Option<i64>| SenderSweepInput {
            address: "boss@example.test".into(),
            mode: mode.into(),
            account_id: None,
            days,
            sweep_archive: false,
        };
        assert!(validate_sweep_input(&input(SWEEP_MODE_OLDER_THAN, Some(1))).is_ok());
        assert!(validate_sweep_input(&input(SWEEP_MODE_OLDER_THAN, Some(3650))).is_ok());
        assert!(validate_sweep_input(&input(SWEEP_MODE_OLDER_THAN, Some(0))).is_err());
        assert!(validate_sweep_input(&input(SWEEP_MODE_OLDER_THAN, Some(3651))).is_err());
        assert!(validate_sweep_input(&input(SWEEP_MODE_OLDER_THAN, None)).is_err());
        assert!(validate_sweep_input(&input("вымышленный", None)).is_err());
    }
}
