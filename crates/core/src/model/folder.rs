//! Папки и умные папки.

use serde::{Deserialize, Serialize};

/// Роль спецпапки (RFC 6154 SPECIAL-USE).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FolderRole {
    Inbox,
    Sent,
    Drafts,
    Spam,
    Trash,
    Archive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Folder {
    pub id: i64,
    pub account_id: i64,
    pub remote_path: String,
    pub display_name: String,
    pub role: Option<FolderRole>,
    pub unread_count: i64,
    pub total_count: i64,
}

/// Условие умной папки.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartCondition {
    pub field: String, // from|to|subject|body|account|status|attachment|label|folder|date
    pub op: String,    // contains|not_contains|equals
    pub value: String,
}

/// Умная папка (сохранённый фильтр).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartFolder {
    pub id: i64,
    pub name: String,
    pub icon: Option<String>,
    /// "all" (И) | "any" (ИЛИ)
    pub match_logic: String,
    pub is_builtin: bool,
    pub enabled: bool,
    pub conditions: Vec<SmartCondition>,
}
