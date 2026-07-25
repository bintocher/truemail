//! Core-owned automatic mail processing rules.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailRule {
    pub id: String,
    pub name: String,
    pub field: String,
    pub operator: String,
    pub value: String,
    pub account_id: Option<i64>,
    pub action: String,
    pub folder_id: Option<i64>,
    pub label_id: Option<i64>,
    pub enabled: bool,
    pub progress_message_id: i64,
    pub sort_order: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailRuleInput {
    pub id: String,
    pub name: String,
    pub field: String,
    pub operator: String,
    pub value: String,
    pub account_id: Option<i64>,
    pub action: String,
    pub folder_id: Option<i64>,
    #[serde(default)]
    pub label_id: Option<i64>,
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
}

fn enabled_by_default() -> bool {
    true
}
