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

impl FolderRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Inbox => "inbox",
            Self::Sent => "sent",
            Self::Drafts => "drafts",
            Self::Spam => "spam",
            Self::Trash => "trash",
            Self::Archive => "archive",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "inbox" => Some(Self::Inbox),
            "sent" => Some(Self::Sent),
            "drafts" => Some(Self::Drafts),
            "spam" => Some(Self::Spam),
            "trash" => Some(Self::Trash),
            "archive" => Some(Self::Archive),
            _ => None,
        }
    }
}

pub fn infer_folder_role(remote_path: &str, display_name: &str) -> Option<FolderRole> {
    let leaf = remote_path
        .rsplit(['/', '|'])
        .next()
        .unwrap_or(remote_path)
        .trim()
        .to_lowercase();
    let display = display_name.trim().to_lowercase();
    let matches = |names: &[&str]| names.iter().any(|name| leaf == *name || display == *name);
    if remote_path.eq_ignore_ascii_case("INBOX") || matches(&["inbox", "входящие"]) {
        Some(FolderRole::Inbox)
    } else if matches(&["sent", "sent mail", "sent items", "отправленные"]) {
        Some(FolderRole::Sent)
    } else if matches(&["drafts", "черновики"]) {
        Some(FolderRole::Drafts)
    } else if matches(&["spam", "junk", "спам", "нежелательная почта"]) {
        Some(FolderRole::Spam)
    } else if matches(&[
        "trash",
        "deleted",
        "deleted items",
        "корзина",
        "удаленные",
        "удалённые",
    ]) {
        Some(FolderRole::Trash)
    } else if matches(&["archive", "archives", "all mail", "архив", "вся почта"]) {
        Some(FolderRole::Archive)
    } else {
        None
    }
}

#[cfg(test)]
mod folder_role_tests {
    use super::*;

    #[test]
    fn infers_common_localized_system_folders() {
        assert_eq!(
            infer_folder_role("Archive", "Archive"),
            Some(FolderRole::Archive)
        );
        assert_eq!(
            infer_folder_role("Архив", "Архив"),
            Some(FolderRole::Archive)
        );
        assert_eq!(infer_folder_role("INBOX", "INBOX"), Some(FolderRole::Inbox));
        assert_eq!(
            infer_folder_role("Удалённые", "Удалённые"),
            Some(FolderRole::Trash)
        );
        assert_eq!(infer_folder_role("pending", "pending"), None);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Folder {
    pub id: i64,
    pub account_id: i64,
    pub remote_path: String,
    pub display_name: String,
    pub role: Option<FolderRole>,
    pub parent_id: Option<i64>,
    pub unread_count: i64,
    pub total_count: i64,
}

/// Условие умной папки.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartCondition {
    pub field: String, // from|to|subject|body|account|status|attachment|label|folder|date
    pub op: String,    // contains|not_contains|equals
    pub value: String,
    pub unit: Option<String>,
    pub value2: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartConditionGroup {
    /// "all" (И) | "any" (ИЛИ)
    pub logic: String,
    pub conditions: Vec<SmartCondition>,
}

/// Умная папка (сохранённый фильтр).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartFolder {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub is_builtin: bool,
    pub enabled: bool,
    pub sort_order: i64,
    pub groups: Vec<SmartConditionGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedSource {
    pub folder_id: i64,
    pub included: bool,
}
