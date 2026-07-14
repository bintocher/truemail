//! Локализация через Fluent (.ftl). Встроенные RU + EN; языки подключаемые.
//! Локализация — адаптация, не дословный перевод (см. docs/10-design.md).

use fluent::{FluentBundle, FluentResource};
use std::collections::HashMap;
use unic_langid::{LanguageIdentifier, langid};

pub struct I18n {
    bundles: HashMap<String, FluentBundle<FluentResource>>,
    current: String,
}

const RU_FTL: &str = include_str!("../../../../locales/ru.ftl");
const EN_FTL: &str = include_str!("../../../../locales/en.ftl");

impl I18n {
    pub fn new(locale: &str) -> Self {
        let mut bundles = HashMap::new();
        bundles.insert("ru".into(), make_bundle(langid!("ru"), RU_FTL));
        bundles.insert("en".into(), make_bundle(langid!("en"), EN_FTL));
        Self {
            bundles,
            current: locale.to_string(),
        }
    }

    /// Получить перевод по ключу с подстановкой аргументов.
    pub fn t(&self, key: &str) -> String {
        let bundle = self
            .bundles
            .get(&self.current)
            .or_else(|| self.bundles.get("ru"))
            .expect("нет базовой локали");
        match bundle.get_message(key).and_then(|m| m.value()) {
            Some(pattern) => {
                let mut errs = vec![];
                bundle.format_pattern(pattern, None, &mut errs).to_string()
            }
            None => key.to_string(),
        }
    }

    pub fn set_locale(&mut self, locale: &str) {
        self.current = locale.to_string();
    }

    pub fn catalog(&self, keys: &[&str]) -> HashMap<String, String> {
        keys.iter()
            .map(|key| ((*key).to_owned(), self.t(key)))
            .collect()
    }
}

fn make_bundle(lang: LanguageIdentifier, ftl: &str) -> FluentBundle<FluentResource> {
    let res = FluentResource::try_new(ftl.to_string())
        .unwrap_or_else(|_| FluentResource::try_new(String::new()).unwrap());
    let mut bundle = FluentBundle::new(vec![lang]);
    let _ = bundle.add_resource(res);
    bundle
}
