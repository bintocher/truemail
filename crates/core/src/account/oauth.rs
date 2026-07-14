//! OAuth 2.0 helpers for provider authorization.

use crate::{Error, Result};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};
use url::Url;

pub const YANDEX_SCOPES: &str = "mail:imap_full mail:smtp calendar:all \
directory:read_external_contacts directory:write_external_contacts";

#[derive(Debug, Clone)]
pub struct PkcePair {
    pub verifier: String,
    pub challenge: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthToken {
    pub access_token: String,
    #[serde(default)]
    pub token_type: String,
    #[serde(default)]
    pub expires_in: Option<i64>,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredOAuthCredential {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<i64>,
    pub token_type: String,
}

#[derive(Debug, Deserialize)]
struct OAuthErrorResponse {
    error: String,
    #[serde(default)]
    error_description: String,
}

pub fn generate_pkce() -> PkcePair {
    let mut random = [0_u8; 48];
    rand::rng().fill_bytes(&mut random);
    let verifier = URL_SAFE_NO_PAD.encode(random);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    PkcePair {
        verifier,
        challenge,
    }
}

pub fn generate_state() -> String {
    let mut random = [0_u8; 32];
    rand::rng().fill_bytes(&mut random);
    URL_SAFE_NO_PAD.encode(random)
}

pub fn yandex_authorize_url(
    client_id: &str,
    email_hint: &str,
    state: &str,
    challenge: &str,
) -> Result<String> {
    if client_id.trim().is_empty() {
        return Err(Error::AccountConfig(
            "не задан TRUEMAIL_YANDEX_CLIENT_ID".into(),
        ));
    }

    let mut url = Url::parse("https://oauth.yandex.ru/authorize")
        .map_err(|e| Error::Other(format!("OAuth URL: {e}")))?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", "https://oauth.yandex.ru/verification_code")
        .append_pair("scope", YANDEX_SCOPES)
        .append_pair("login_hint", email_hint)
        .append_pair("force_confirm", "yes")
        .append_pair("state", state)
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256");
    Ok(url.into())
}

pub async fn exchange_yandex_code(
    client_id: &str,
    code: &str,
    verifier: &str,
) -> Result<OAuthToken> {
    let response = oauth_client()?
        .post("https://oauth.yandex.ru/token")
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code.trim()),
            ("client_id", client_id),
            ("code_verifier", verifier),
        ])
        .send()
        .await
        .map_err(|e| Error::Backend {
            backend: "yandex-oauth".into(),
            message: e.to_string(),
        })?;

    parse_token_response(response).await
}

/// Продлить OAuth-токен без участия пользователя.
pub async fn refresh_yandex_token(client_id: &str, refresh_token: &str) -> Result<OAuthToken> {
    let response = oauth_client()?
        .post("https://oauth.yandex.ru/token")
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", client_id),
        ])
        .send()
        .await
        .map_err(|e| Error::Backend {
            backend: "yandex-oauth".into(),
            message: e.to_string(),
        })?;
    parse_token_response(response).await
}

fn oauth_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|error| Error::Backend {
            backend: "yandex-oauth".into(),
            message: format!("не удалось создать HTTP-клиент: {error}"),
        })
}

async fn parse_token_response(response: reqwest::Response) -> Result<OAuthToken> {
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let message = serde_json::from_str::<OAuthErrorResponse>(&body)
            .map(|e| {
                if e.error_description.is_empty() {
                    e.error
                } else {
                    format!("{}: {}", e.error, e.error_description)
                }
            })
            .unwrap_or_else(|_| format!("HTTP {status}: {body}"));
        return Err(Error::Backend {
            backend: "yandex-oauth".into(),
            message,
        });
    }

    response.json().await.map_err(|e| Error::Backend {
        backend: "yandex-oauth".into(),
        message: format!("не удалось разобрать токен: {e}"),
    })
}

impl From<OAuthToken> for StoredOAuthCredential {
    fn from(token: OAuthToken) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        Self {
            access_token: token.access_token,
            refresh_token: token.refresh_token,
            expires_at: token.expires_in.map(|seconds| now + seconds),
            token_type: if token.token_type.is_empty() {
                "bearer".into()
            } else {
                token.token_type
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_has_expected_shape() {
        let pair = generate_pkce();
        assert!(pair.verifier.len() >= 43);
        assert_eq!(pair.challenge.len(), 43);
        assert!(!pair.verifier.contains('='));
        assert!(!pair.challenge.contains('='));
    }

    #[test]
    fn authorize_url_contains_combined_scopes_and_pkce() {
        let url =
            yandex_authorize_url("client", "me@yandex.ru", "state", "challenge").expect("url");
        let parsed = Url::parse(&url).expect("parse");
        let params: std::collections::HashMap<_, _> = parsed.query_pairs().collect();
        assert_eq!(
            params
                .get("code_challenge_method")
                .map(|value| value.as_ref()),
            Some("S256")
        );
        let scopes = params.get("scope").expect("scope");
        assert!(scopes.contains("mail:imap_full"));
        assert!(scopes.contains("calendar:all"));
        assert!(scopes.contains("directory:read_external_contacts"));
        assert!(scopes.contains("directory:write_external_contacts"));
    }
}
