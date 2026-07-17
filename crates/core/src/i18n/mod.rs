//! Единый каталог локализации для UI и core.
//! JSON-файлы из desktop UI встраиваются в core при сборке, поэтому ключи и
//! переводы не расходятся между слоями приложения.

use std::collections::HashMap;

const RU_JSON: &str = include_str!("../../../../apps/desktop/ui/locales/ru.json");
const EN_JSON: &str = include_str!("../../../../apps/desktop/ui/locales/en.json");

pub struct I18n {
    catalog: HashMap<String, String>,
}

impl I18n {
    pub fn new(locale: &str) -> Self {
        Self {
            catalog: parse_catalog(locale),
        }
    }

    pub fn t(&self, key: &str) -> String {
        self.catalog
            .get(key)
            .cloned()
            .unwrap_or_else(|| key.to_owned())
    }

    pub fn set_locale(&mut self, locale: &str) {
        self.catalog = parse_catalog(locale);
    }

    pub fn catalog(&self) -> HashMap<String, String> {
        self.catalog.clone()
    }
}

fn parse_catalog(locale: &str) -> HashMap<String, String> {
    let source = if locale.eq_ignore_ascii_case("en") {
        EN_JSON
    } else {
        RU_JSON
    };
    serde_json::from_str(source).expect("встроенный каталог локализации должен быть валидным JSON")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    const INDEX_HTML: &str = include_str!("../../../../apps/desktop/ui/index.html");
    const I18N_ATTRIBUTES: &[&str] = &[
        "data-i18n=\"",
        "data-i18n-placeholder=\"",
        "data-i18n-title=\"",
        "data-i18n-aria=\"",
        "data-i18n-tip=\"",
        "data-i18n-ph=\"",
    ];

    fn keys(catalog: &HashMap<String, String>) -> BTreeSet<&str> {
        catalog.keys().map(String::as_str).collect()
    }

    fn html_keys() -> BTreeSet<&'static str> {
        let mut result = BTreeSet::new();
        for marker in I18N_ATTRIBUTES {
            let mut rest = INDEX_HTML;
            while let Some(start) = rest.find(marker) {
                rest = &rest[start + marker.len()..];
                let end = rest.find('"').expect("незакрытый data-i18n атрибут");
                result.insert(&rest[..end]);
                rest = &rest[end + 1..];
            }
        }
        result
    }

    fn contains_cyrillic(value: &str) -> bool {
        value
            .chars()
            .any(|ch| ('\u{0400}'..='\u{04ff}').contains(&ch))
    }

    fn unmarked_russian_html() -> Vec<String> {
        let mut result = Vec::new();
        for fragment in INDEX_HTML.split('<').skip(1) {
            let Some(tag_end) = fragment.find('>') else {
                continue;
            };
            let tag = &fragment[..tag_end];
            if tag.starts_with("!--") {
                continue;
            }
            let text = fragment[tag_end + 1..].trim();
            if contains_cyrillic(text) && !tag.contains("data-i18n") {
                result.push(text.to_owned());
            }
            for (attribute, marker) in [
                ("title=\"", "data-i18n-title"),
                ("placeholder=\"", "data-i18n-placeholder"),
                ("aria-label=\"", "data-i18n-aria"),
            ] {
                if let Some(start) = tag.find(attribute) {
                    let value = &tag[start + attribute.len()..];
                    let value = &value[..value.find('"').unwrap_or(value.len())];
                    if contains_cyrillic(value) && !tag.contains(marker) {
                        result.push(format!("{attribute}{value}"));
                    }
                }
            }
        }
        result
    }

    #[test]
    fn locale_catalogs_have_identical_non_empty_keys() {
        let ru = parse_catalog("ru");
        let en = parse_catalog("en");
        assert_eq!(keys(&ru), keys(&en));
        assert!(ru.values().all(|value| !value.trim().is_empty()));
        assert!(en.values().all(|value| !value.trim().is_empty()));
    }

    #[test]
    fn every_localized_html_attribute_exists_in_catalog() {
        let catalog = parse_catalog("ru");
        let missing: Vec<_> = html_keys()
            .into_iter()
            .filter(|key| !catalog.contains_key(*key))
            .collect();
        assert!(missing.is_empty(), "нет переводов для ключей: {missing:?}");
    }

    #[test]
    fn russian_html_literals_are_marked_for_localization() {
        let unmarked = unmarked_russian_html();
        assert!(
            unmarked.is_empty(),
            "русский текст без data-i18n атрибута: {unmarked:?}"
        );
    }

    #[test]
    fn locale_switch_replaces_catalog_and_falls_back_to_key() {
        let mut i18n = I18n::new("ru");
        assert_eq!(i18n.t("actionReply"), "Ответить");
        i18n.set_locale("en");
        assert_eq!(i18n.t("actionReply"), "Reply");
        assert_eq!(i18n.t("missing-key"), "missing-key");
    }
}
