//! Быстрые действия, собранные из словаря действий почтовых правил.

use super::MailRuleAction;
use serde::{Deserialize, Serialize};

pub const MAX_QUICK_STEPS: usize = 20;
pub const MAX_QUICK_STEP_MESSAGES: usize = 500;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickStep {
    pub id: i64,
    pub name: String,
    pub icon: String,
    pub sort_order: i64,
    pub hotkey_slot: Option<i64>,
    pub state: String,
    pub actions: Vec<MailRuleAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickStepInput {
    #[serde(default)]
    pub id: Option<i64>,
    pub name: String,
    #[serde(default = "default_icon")]
    pub icon: String,
    #[serde(default)]
    pub sort_order: i64,
    #[serde(default)]
    pub hotkey_slot: Option<i64>,
    #[serde(default)]
    pub actions: Vec<MailRuleAction>,
}

fn default_icon() -> String {
    "star".into()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QuickStepReport {
    pub applied: i64,
    pub skipped: i64,
    pub skipped_busy: i64,
    pub skipped_failed: i64,
    pub skipped_no_folder: i64,
    pub skipped_foreign_account: i64,
    pub traits_at_risk: i64,
    pub operation_ids: Vec<i64>,
}
