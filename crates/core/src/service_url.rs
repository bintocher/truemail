use crate::{Error, Result};
use url::Url;

fn allowed(url: &Url) -> bool {
    let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    url.host().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none()
        && (url.scheme() == "https" || (local && url.scheme() == "http"))
}

pub(crate) fn parse(value: &str) -> Result<Url> {
    let url = Url::parse(value.trim()).map_err(|_| invalid_service_url())?;
    allowed(&url).then_some(url).ok_or_else(invalid_service_url)
}

pub(crate) fn resolve(base: &str, href: &str) -> Result<Url> {
    let base = parse(base)?;
    let target = base.join(href).map_err(|_| invalid_service_url())?;
    (allowed(&target) && base.origin() == target.origin())
        .then_some(target)
        .ok_or_else(invalid_service_url)
}

pub(crate) fn redirects() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        if allowed(attempt.url())
            && attempt.previous().len() < 6
            && attempt
                .previous()
                .first()
                .is_some_and(|base| base.origin() == attempt.url().origin())
        {
            attempt.follow()
        } else {
            attempt.stop()
        }
    })
}

fn invalid_service_url() -> Error {
    Error::AccountConfig("Адрес службы должен использовать защищённое соединение и совпадать с адресом настроенного сервера".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_the_configured_origin() {
        let base = "https://mail.example/dav/";
        assert!(resolve(base, "calendar.ics").is_ok());
        assert!(resolve(base, "https://mail.example:443/item").is_ok());
        assert!(resolve(base, "https://mail.example:444/item").is_err());
        assert!(resolve(base, "http://mail.example/item").is_err());
        assert!(resolve(base, "//other.example/item").is_err());
        assert!(resolve(base, "https://user@mail.example/item").is_err());
    }
}
