//! Внешний API доступа к почте (см. docs/06-ai-api.md).
//! Никакого AI внутри клиента — отдаём программный доступ (MCP + REST) для внешних
//! потребителей (агенты, скрипты). Всё — через capability-права + аудит.

use crate::Result;
use serde::{Deserialize, Serialize};

/// Право доступа (capability).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Capability {
    Read,
    Search,
    Send,
    Labels,
    Calendar,
    Network,
}

/// Внешний потребитель API (агент/скрипт) с выданными правами.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiClient {
    pub id: i64,
    pub name: String,
    pub caps: Vec<Capability>,
}

impl ApiClient {
    /// Проверить право; при отсутствии — ошибка (запись в аудит делается вызывающим).
    pub fn require(&self, cap: Capability) -> Result<()> {
        if self.caps.contains(&cap) {
            Ok(())
        } else {
            Err(crate::Error::Forbidden(format!("{cap:?}")))
        }
    }
}

/// Описание инструмента MCP, экспонируемого наружу.
#[derive(Debug, Clone, Serialize)]
pub struct McpTool {
    pub name: &'static str,
    pub description: &'static str,
    pub required_cap: Capability,
}

/// Набор инструментов MCP-сервера (см. docs/06-ai-api.md).
pub fn mcp_tools() -> Vec<McpTool> {
    use Capability::*;
    vec![
        McpTool {
            name: "search",
            description: "Поиск по почте",
            required_cap: Search,
        },
        McpTool {
            name: "get_message",
            description: "Прочитать письмо/тред",
            required_cap: Read,
        },
        McpTool {
            name: "list_folders",
            description: "Список папок",
            required_cap: Read,
        },
        McpTool {
            name: "list_messages",
            description: "Список писем",
            required_cap: Read,
        },
        McpTool {
            name: "send",
            description: "Отправить письмо",
            required_cap: Send,
        },
        McpTool {
            name: "reply",
            description: "Ответить",
            required_cap: Send,
        },
        McpTool {
            name: "draft",
            description: "Создать черновик",
            required_cap: Send,
        },
        McpTool {
            name: "label",
            description: "Поставить/снять метку",
            required_cap: Labels,
        },
        McpTool {
            name: "list_events",
            description: "События календаря",
            required_cap: Calendar,
        },
        McpTool {
            name: "list_contacts",
            description: "Контакты",
            required_cap: Read,
        },
    ]
}
