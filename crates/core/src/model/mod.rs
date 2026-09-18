//! Каноническая модель данных на основе открытых стандартов (RFC).
//! Любой backend-адаптер конвертирует своё представление в эти типы и обратно.

mod account;
mod contact;
mod event;
mod folder;
mod ignored_conversation;
mod message;
mod rule;
mod sender_policy;
mod sender_sweep;
mod settings;

pub use account::*;
pub use contact::*;
pub use event::*;
pub use folder::*;
pub use ignored_conversation::*;
pub use message::*;
pub use rule::*;
pub use sender_policy::*;
pub use sender_sweep::*;
pub use settings::*;

use serde::{Deserialize, Serialize};

/// Почтовый адрес (RFC 5322).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Addr {
    pub name: Option<String>,
    pub email: String,
}

impl Addr {
    pub fn new(email: impl Into<String>) -> Self {
        Self {
            name: None,
            email: email.into(),
        }
    }
    pub fn named(name: impl Into<String>, email: impl Into<String>) -> Self {
        Self {
            name: Some(name.into()),
            email: email.into(),
        }
    }
}

/// Результаты проверки подлинности отправителя (SPF/DKIM/DMARC).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct AuthResults {
    pub spf: Option<bool>,
    pub dkim: Option<bool>,
    pub dmarc: Option<bool>,
}
