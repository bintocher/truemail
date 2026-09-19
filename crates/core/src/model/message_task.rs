//! Дело, связанное с письмом: локальные сроки и состояние.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlagChangeReason {
    User,
    Completion,
    Rule,
    QuickStep,
    Sync,
    Reopen,
}

impl FlagChangeReason {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "user" => Some(Self::User),
            "completion" => Some(Self::Completion),
            "rule" => Some(Self::Rule),
            "quick_step" => Some(Self::QuickStep),
            "sync" => Some(Self::Sync),
            "reopen" => Some(Self::Reopen),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageTask {
    pub message_id: i64,
    pub start_at: Option<String>,
    pub due_at: Option<String>,
    pub reminder_at: Option<String>,
    pub state: String,
    pub completed_at: Option<String>,
    pub reminder_shown_at: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageTaskInput {
    pub message_id: i64,
    #[serde(default)]
    pub start_at: Option<String>,
    #[serde(default)]
    pub due_at: Option<String>,
    #[serde(default)]
    pub reminder_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskListItem {
    pub task: MessageTask,
    pub account_id: i64,
    pub account_email: String,
    pub folder_id: i64,
    pub subject: String,
    pub sender_name: Option<String>,
    pub sender_address: Option<String>,
    pub message_date: Option<String>,
    pub snoozed_until: Option<String>,
    pub has_takeaway: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskListPage {
    pub items: Vec<TaskListItem>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskReminder {
    pub message_id: i64,
    pub sender_name: Option<String>,
    pub sender_address: Option<String>,
    pub subject: String,
    pub due_at: Option<String>,
    pub reminder_at: String,
}
