//! Автоконфигурация: известные провайдеры + autoconfig/ISPDB/SRV (см. docs/05-protocols.md).

use crate::model::{AuthKind, BackendKind, Provider, Security, ServerConfig};

#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub provider: Provider,
    pub backend_kind: BackendKind,
    pub auth_kind: AuthKind,
    pub imap: Option<ServerConfig>,
    pub smtp: Option<ServerConfig>,
    pub ews_url: Option<String>,
}

/// Подобрать известного провайдера и его стандартные серверы по домену адреса.
pub fn autoconfig(email: &str) -> ProviderConfig {
    let domain = email.rsplit('@').next().unwrap_or("").to_lowercase();
    let imap = |h: &str, p: u16| {
        Some(ServerConfig {
            host: h.into(),
            port: p,
            security: Security::Ssl,
        })
    };
    let smtp = |h: &str, p: u16| {
        Some(ServerConfig {
            host: h.into(),
            port: p,
            security: Security::Ssl,
        })
    };

    match domain.as_str() {
        "yandex.ru" | "yandex.com" | "ya.ru" => ProviderConfig {
            provider: Provider::Yandex,
            backend_kind: BackendKind::Imap,
            auth_kind: AuthKind::Oauth2,
            imap: imap("imap.yandex.com", 993),
            smtp: smtp("smtp.yandex.com", 465),
            ews_url: None,
        },
        "mail.ru" | "inbox.ru" | "list.ru" | "bk.ru" => ProviderConfig {
            provider: Provider::Mailru,
            backend_kind: BackendKind::Imap,
            auth_kind: AuthKind::AppPassword,
            imap: imap("imap.mail.ru", 993),
            smtp: smtp("smtp.mail.ru", 465),
            ews_url: None,
        },
        "icloud.com" | "me.com" | "mac.com" => ProviderConfig {
            provider: Provider::Icloud,
            backend_kind: BackendKind::Imap,
            auth_kind: AuthKind::AppPassword,
            imap: imap("imap.mail.me.com", 993),
            smtp: Some(ServerConfig {
                host: "smtp.mail.me.com".into(),
                port: 587,
                security: Security::Starttls,
            }),
            ews_url: None,
        },
        "gmail.com" | "googlemail.com" => ProviderConfig {
            provider: Provider::Gmail,
            backend_kind: BackendKind::Imap,
            auth_kind: AuthKind::Oauth2,
            imap: imap("imap.gmail.com", 993),
            smtp: smtp("smtp.gmail.com", 465),
            ews_url: None,
        },
        "outlook.com" | "hotmail.com" | "live.com" => ProviderConfig {
            provider: Provider::Outlook,
            backend_kind: BackendKind::Imap,
            auth_kind: AuthKind::Oauth2,
            imap: imap("outlook.office365.com", 993),
            smtp: Some(ServerConfig {
                host: "smtp.office365.com".into(),
                port: 587,
                security: Security::Starttls,
            }),
            ews_url: None,
        },
        _ => ProviderConfig {
            provider: Provider::Generic,
            backend_kind: BackendKind::Imap,
            auth_kind: AuthKind::Password,
            imap: None,
            smtp: None,
            ews_url: None,
        },
    }
}
