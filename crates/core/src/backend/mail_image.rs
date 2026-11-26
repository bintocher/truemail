use crate::{Error, Result};
use std::{net::IpAddr, time::Duration};
use url::Url;

fn public_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();
            !ip.is_private()
                && !ip.is_link_local()
                && !ip.is_loopback()
                && !(a == 0
                    || a >= 224
                    || (a == 100 && (64..=127).contains(&b))
                    || (a == 198 && (b == 18 || b == 19))
                    || matches!(
                        (a, b, c),
                        (192, 0, 0 | 2) | (192, 88, 99) | (198, 51, 100) | (203, 0, 113)
                    ))
        }
        IpAddr::V6(ip) => {
            if let Some(mapped) = ip.to_ipv4_mapped() {
                return public_address(IpAddr::V4(mapped));
            }
            let [first, second, ..] = ip.segments();
            first & 0xe000 == 0x2000
                && first != 0x2002
                && !(first == 0x2001 && (second < 0x0200 || second == 0x0db8))
                && !(first == 0x3fff && second < 0x1000)
        }
    }
}

#[cfg(test)]
#[test]
fn image_loading_rejects_ipv6_transition_and_nonpublic_ranges() {
    for address in [
        "2002::1",
        "2001::1",
        "::ffff:192.0.0.3",
        "192.0.2.1",
        "198.51.100.1",
        "203.0.113.1",
        "3fff::1",
        "fd00::1",
        "::1",
    ] {
        assert!(!public_address(address.parse().unwrap()));
    }
    assert!(public_address("2606:4700::1111".parse().unwrap()));
}

pub async fn fetch_remote_mail_image(value: &str) -> Result<(String, Vec<u8>)> {
    let invalid = || Error::AccountConfig("Недопустимый адрес внешнего изображения".into());
    let url = Url::parse(value).map_err(|_| invalid())?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || !matches!(
            (url.scheme(), url.port_or_known_default()),
            ("http", Some(80)) | ("https", Some(443))
        )
    {
        return Err(invalid());
    }
    let host = url.host_str().ok_or_else(invalid)?;
    let lookup = (
        host.trim_matches(['[', ']']),
        url.port_or_known_default().ok_or_else(invalid)?,
    );
    let addresses = tokio::time::timeout(Duration::from_secs(8), tokio::net::lookup_host(lookup))
        .await
        .map_err(|_| invalid())??
        .collect::<Vec<_>>();
    if addresses.is_empty()
        || addresses
            .iter()
            .any(|address| !public_address(address.ip()))
    {
        return Err(invalid());
    }
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .resolve_to_addrs(host, &addresses)
        .timeout(Duration::from_secs(12))
        .build()
        .map_err(|_| invalid())?;
    let mut response = client.get(url).send().await.map_err(|_| invalid())?;
    const MAX_BYTES: u64 = 5 * 1024 * 1024;
    let mime = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .filter(|mime| {
            matches!(
                *mime,
                "image/png"
                    | "image/jpeg"
                    | "image/gif"
                    | "image/webp"
                    | "image/avif"
                    | "image/bmp"
            )
        })
        .ok_or_else(invalid)?
        .to_owned();
    if !response.status().is_success()
        || response
            .content_length()
            .is_some_and(|size| size > MAX_BYTES)
    {
        return Err(invalid());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| invalid())? {
        if (bytes.len() + chunk.len()) as u64 > MAX_BYTES {
            return Err(invalid());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok((mime, bytes))
}
