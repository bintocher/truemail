//! Автоконфигурация: известные провайдеры + autoconfig/ISPDB/SRV (см. docs/05-protocols.md).

use crate::model::{AuthKind, BackendKind, Provider, Security, ServerConfig};
use hickory_resolver::proto::rr::RData;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub provider: Provider,
    pub backend_kind: BackendKind,
    pub auth_kind: AuthKind,
    pub imap: Option<ServerConfig>,
    pub smtp: Option<ServerConfig>,
    pub ews_url: Option<String>,
    pub jmap_url: Option<String>,
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
            jmap_url: None,
        },
        "mail.ru" | "inbox.ru" | "list.ru" | "bk.ru" => ProviderConfig {
            provider: Provider::Mailru,
            backend_kind: BackendKind::Imap,
            auth_kind: AuthKind::AppPassword,
            imap: imap("imap.mail.ru", 993),
            smtp: smtp("smtp.mail.ru", 465),
            ews_url: None,
            jmap_url: None,
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
            jmap_url: None,
        },
        "gmail.com" | "googlemail.com" => ProviderConfig {
            provider: Provider::Gmail,
            backend_kind: BackendKind::Imap,
            auth_kind: AuthKind::Oauth2,
            imap: imap("imap.gmail.com", 993),
            smtp: smtp("smtp.gmail.com", 465),
            ews_url: None,
            jmap_url: None,
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
            jmap_url: None,
        },
        _ => ProviderConfig {
            provider: Provider::Generic,
            backend_kind: BackendKind::Imap,
            auth_kind: AuthKind::Password,
            imap: None,
            smtp: None,
            ews_url: None,
            jmap_url: None,
        },
    }
}

fn provider_from_hosts<'a>(hosts: impl IntoIterator<Item = &'a str>) -> Provider {
    let hosts = hosts
        .into_iter()
        .map(|host| host.trim_end_matches('.').to_ascii_lowercase())
        .collect::<Vec<_>>();
    let has = |suffixes: &[&str]| {
        hosts.iter().any(|host| {
            suffixes
                .iter()
                .any(|suffix| host == suffix || host.ends_with(&format!(".{suffix}")))
        })
    };
    if has(&["yandex.net", "yandex.ru", "yandex.com"]) {
        Provider::Yandex
    } else if has(&["google.com", "googlemail.com", "gmail.com"]) {
        Provider::Gmail
    } else if has(&["outlook.com", "office365.com", "protection.outlook.com"]) {
        Provider::Outlook
    } else if has(&["mail.ru"]) {
        Provider::Mailru
    } else if has(&["icloud.com", "me.com"]) {
        Provider::Icloud
    } else {
        Provider::Generic
    }
}

fn config_for_provider(provider: Provider) -> ProviderConfig {
    let sample = match provider {
        Provider::Yandex => "user@yandex.ru",
        Provider::Gmail => "user@gmail.com",
        Provider::Outlook => "user@outlook.com",
        Provider::Mailru => "user@mail.ru",
        Provider::Icloud => "user@icloud.com",
        _ => "user@invalid.local",
    };
    autoconfig(sample)
}

fn jmap_config(session_url: String) -> ProviderConfig {
    ProviderConfig {
        provider: Provider::Generic,
        backend_kind: BackendKind::Jmap,
        auth_kind: AuthKind::AppPassword,
        imap: None,
        smtp: None,
        ews_url: None,
        jmap_url: Some(session_url),
    }
}

async fn xml_autoconfig(email: &str, domain: &str) -> Option<ProviderConfig> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(6))
        .redirect(reqwest::redirect::Policy::limited(3))
        .build()
        .ok()?;
    let urls = [
        format!("https://autoconfig.{domain}/mail/config-v1.1.xml?emailaddress={email}"),
        format!(
            "https://{domain}/.well-known/autoconfig/mail/config-v1.1.xml?emailaddress={email}"
        ),
        format!("https://autoconfig.thunderbird.net/v1.1/{domain}"),
    ];
    for url in urls {
        let Ok(response) = client.get(url).send().await else {
            continue;
        };
        if !response.status().is_success() {
            continue;
        }
        let Ok(body) = response.text().await else {
            continue;
        };
        let Ok(document) = roxmltree::Document::parse(&body) else {
            continue;
        };
        let incoming = document.descendants().find(|node| {
            node.has_tag_name("incomingServer")
                && node
                    .attribute("type")
                    .is_some_and(|kind| kind.eq_ignore_ascii_case("imap"))
        });
        let outgoing = document
            .descendants()
            .find(|node| node.has_tag_name("outgoingServer"));
        let server = |node: roxmltree::Node<'_, '_>, default_port: u16| {
            let text = |name: &str| {
                node.children()
                    .find(|child| child.has_tag_name(name))
                    .and_then(|child| child.text())
                    .map(str::trim)
            };
            let host = text("hostname")?.to_string();
            let port = text("port")
                .and_then(|value| value.parse().ok())
                .unwrap_or(default_port);
            let security = match text("socketType")
                .unwrap_or("SSL")
                .to_ascii_uppercase()
                .as_str()
            {
                "STARTTLS" => Security::Starttls,
                "PLAIN" | "NONE" => Security::None,
                _ => Security::Ssl,
            };
            Some(ServerConfig {
                host,
                port,
                security,
            })
        };
        let imap = incoming.and_then(|node| server(node, 993));
        let smtp = outgoing.and_then(|node| server(node, 465));
        if imap.is_none() && smtp.is_none() {
            continue;
        }
        let provider = provider_from_hosts(
            imap.iter()
                .map(|value| value.host.as_str())
                .chain(smtp.iter().map(|value| value.host.as_str())),
        );
        if provider != Provider::Generic {
            return Some(config_for_provider(provider));
        }
        return Some(ProviderConfig {
            provider,
            backend_kind: BackendKind::Imap,
            auth_kind: AuthKind::Password,
            imap,
            smtp,
            ews_url: None,
            jmap_url: None,
        });
    }
    None
}

/// Определить фактического почтового провайдера для собственного домена.
/// Проверяются DNS MX/SRV и стандартные XML autoconfig endpoints. Ошибки отдельных
/// источников не блокируют остальные способы обнаружения.
pub async fn discover_provider(email: &str) -> ProviderConfig {
    let known = autoconfig(email);
    if known.provider != Provider::Generic {
        return known;
    }
    let domain = email
        .rsplit('@')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if domain.is_empty() {
        return known;
    }

    if let Ok(builder) = hickory_resolver::Resolver::builder_tokio()
        && let Ok(resolver) = builder.build()
    {
        if let Ok(mx) = resolver.mx_lookup(format!("{domain}.")).await {
            let hosts = mx
                .answers()
                .iter()
                .filter_map(|record| match &record.data {
                    RData::MX(value) => Some(value.exchange.to_utf8()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let provider = provider_from_hosts(hosts.iter().map(String::as_str));
            if provider != Provider::Generic {
                return config_for_provider(provider);
            }
        }

        // Exchange публикует SRV _autodiscover._tcp; наличие записи означает
        // локальный Exchange, а её target - хост с EWS. Точный адрес EWS
        // уточняется при подключении через autodiscover с учётными данными.
        if let Ok(records) = resolver
            .srv_lookup(format!("_autodiscover._tcp.{domain}."))
            .await
            && let Some(record) = records
                .answers()
                .iter()
                .filter_map(|record| match &record.data {
                    RData::SRV(value) => Some(value),
                    _ => None,
                })
                .min_by_key(|record| record.priority)
        {
            let host = record.target.to_utf8();
            let host = host.trim_end_matches('.');
            if !host.is_empty() {
                let provider = if host.eq_ignore_ascii_case("autodiscover.outlook.com") {
                    Provider::Outlook
                } else {
                    Provider::Exchange
                };
                let authority = if record.port == 443 {
                    host.to_owned()
                } else {
                    format!("{host}:{}", record.port)
                };
                return ProviderConfig {
                    provider,
                    backend_kind: BackendKind::Ews,
                    auth_kind: if provider == Provider::Outlook {
                        AuthKind::Oauth2
                    } else {
                        AuthKind::Password
                    },
                    imap: None,
                    smtp: None,
                    ews_url: Some(format!("https://{authority}/EWS/Exchange.asmx")),
                    jmap_url: None,
                };
            }
        }

        if let Ok(records) = resolver.srv_lookup(format!("_jmap._tcp.{domain}.")).await
            && let Some(record) = records
                .answers()
                .iter()
                .filter_map(|record| match &record.data {
                    RData::SRV(value) => Some(value),
                    _ => None,
                })
                .min_by_key(|record| record.priority)
        {
            let host = record.target.to_utf8();
            let host = host.trim_end_matches('.');
            let port = record.port;
            let authority = if port == 443 {
                host.to_owned()
            } else {
                format!("{host}:{port}")
            };
            return jmap_config(format!("https://{authority}/.well-known/jmap"));
        }

        let mut imap = None;
        let mut smtp = None;
        for (service, target) in [("_imaps._tcp", &mut imap), ("_submission._tcp", &mut smtp)] {
            if let Ok(records) = resolver.srv_lookup(format!("{service}.{domain}.")).await
                && let Some(record) = records
                    .answers()
                    .iter()
                    .filter_map(|record| match &record.data {
                        RData::SRV(value) => Some(value),
                        _ => None,
                    })
                    .min_by_key(|record| record.priority)
            {
                *target = Some(ServerConfig {
                    host: record.target.to_utf8().trim_end_matches('.').to_string(),
                    port: record.port,
                    security: if service == "_imaps._tcp" {
                        Security::Ssl
                    } else {
                        Security::Starttls
                    },
                });
            }
        }
        let provider = provider_from_hosts(
            imap.iter()
                .map(|value| value.host.as_str())
                .chain(smtp.iter().map(|value| value.host.as_str())),
        );
        if provider != Provider::Generic {
            return config_for_provider(provider);
        }
        if imap.is_some() || smtp.is_some() {
            return ProviderConfig {
                provider: Provider::Generic,
                backend_kind: BackendKind::Imap,
                auth_kind: AuthKind::Password,
                imap,
                smtp,
                ews_url: None,
                jmap_url: None,
            };
        }
    }

    let (jmap, xml) = tokio::join!(
        crate::backend::probe_jmap_session_url(email),
        xml_autoconfig(email, &domain)
    );
    if let Some(url) = jmap {
        return jmap_config(url);
    }
    if let Some(config) = xml {
        return config;
    }
    known
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_hosted_provider_from_resolved_hosts() {
        assert_eq!(provider_from_hosts(["mx.yandex.net."]), Provider::Yandex);
        assert_eq!(
            provider_from_hosts(["aspmx.l.google.com."]),
            Provider::Gmail
        );
        assert_eq!(
            provider_from_hosts(["example-com.mail.protection.outlook.com."]),
            Provider::Outlook
        );
        assert_eq!(
            provider_from_hosts(["mail.example.org."]),
            Provider::Generic
        );
    }
}
