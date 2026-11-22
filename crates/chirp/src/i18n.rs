//! Translations with Project Fluent.
//!
//! All `.ftl` files are compiled into the program. The system language is
//! detected automatically; English is the fallback for missing languages and
//! for single messages a translation does not have yet.

use fluent_bundle::{FluentArgs, FluentBundle, FluentResource};
use unic_langid::LanguageIdentifier;

/// (language tag, Fluent source). The tag is also the file name in `locales/`.
pub const LOCALES: &[(&str, &str)] = &[
    ("en", include_str!("../locales/en.ftl")),
    ("de", include_str!("../locales/de.ftl")),
    ("fr", include_str!("../locales/fr.ftl")),
    ("it", include_str!("../locales/it.ftl")),
    ("es", include_str!("../locales/es.ftl")),
    ("pt-PT", include_str!("../locales/pt-PT.ftl")),
    ("pt-BR", include_str!("../locales/pt-BR.ftl")),
    ("nl", include_str!("../locales/nl.ftl")),
    ("pl", include_str!("../locales/pl.ftl")),
    ("cs", include_str!("../locales/cs.ftl")),
    ("tr", include_str!("../locales/tr.ftl")),
    ("ro", include_str!("../locales/ro.ftl")),
    ("bg", include_str!("../locales/bg.ftl")),
    ("uk", include_str!("../locales/uk.ftl")),
    ("sr-Cyrl", include_str!("../locales/sr-Cyrl.ftl")),
    ("sr-Latn", include_str!("../locales/sr-Latn.ftl")),
    ("he", include_str!("../locales/he.ftl")),
    ("ja", include_str!("../locales/ja.ftl")),
    ("ko", include_str!("../locales/ko.ftl")),
    ("zh-Hans", include_str!("../locales/zh-Hans.ftl")),
    ("zh-Hant", include_str!("../locales/zh-Hant.ftl")),
];

pub struct Localizer {
    primary: FluentBundle<FluentResource>,
    fallback: FluentBundle<FluentResource>,
}

impl Localizer {
    /// Use `requested` (e.g. from `--lang`) or else the system languages.
    pub fn new(requested: Option<&str>) -> Self {
        let tag = match requested {
            Some(r) => best_match([r]),
            None => best_match(sys_locale::get_locales()),
        };
        Self {
            primary: bundle(tag),
            fallback: bundle("en"),
        }
    }

    /// Translate a message; `args` are `(name, value)` pairs.
    pub fn t(&self, id: &str, args: &[(&str, &str)]) -> String {
        let mut fargs = FluentArgs::new();
        for (k, v) in args {
            fargs.set(*k, v.to_string());
        }
        for b in [&self.primary, &self.fallback] {
            if let Some(pattern) = b.get_message(id).and_then(|m| m.value()) {
                let mut errors = vec![];
                return b
                    .format_pattern(pattern, Some(&fargs), &mut errors)
                    .into_owned();
            }
        }
        id.to_string()
    }
}

fn bundle(tag: &str) -> FluentBundle<FluentResource> {
    let source = LOCALES
        .iter()
        .find(|(t, _)| *t == tag)
        .map(|(_, s)| *s)
        .unwrap_or(LOCALES[0].1);
    let lang: LanguageIdentifier = tag.parse().unwrap_or_default();
    let mut bundle = FluentBundle::new(vec![lang]);
    // Terminal output: no Unicode isolation marks around placeholders.
    bundle.set_use_isolating(false);
    let resource = FluentResource::try_new(source.to_string()).unwrap_or_else(|(r, _)| r);
    let _ = bundle.add_resource(resource);
    bundle
}

/// Pick the best supported language for a list of locales, in order.
pub fn best_match<I, S>(locales: I) -> &'static str
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    locales
        .into_iter()
        .find_map(|l| match_one(l.as_ref()))
        .unwrap_or("en")
}

fn match_one(locale: &str) -> Option<&'static str> {
    let clean = locale.split(['.', '@']).next()?.replace('_', "-");
    let parts: Vec<String> = clean.split('-').map(str::to_ascii_lowercase).collect();
    let lang = parts.first()?.as_str();
    let has = |p: &str| parts.iter().skip(1).any(|x| x == p);
    let tag = match lang {
        "pt" if has("pt") => "pt-PT",
        "pt" => "pt-BR",
        "sr" if has("latn") => "sr-Latn",
        "sr" => "sr-Cyrl",
        "zh" if has("hant") || has("tw") || has("hk") || has("mo") => "zh-Hant",
        "zh" => "zh-Hans",
        "iw" => "he",
        other => return LOCALES.iter().map(|(t, _)| *t).find(|t| *t == other),
    };
    Some(tag)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_system_language() {
        assert_eq!(best_match(["de-DE"]), "de");
        assert_eq!(best_match(["de_AT.UTF-8"]), "de");
        assert_eq!(best_match(["nl-BE"]), "nl");
        assert_eq!(best_match(["pt-BR"]), "pt-BR");
        assert_eq!(best_match(["pt-PT"]), "pt-PT");
        assert_eq!(best_match(["zh-TW"]), "zh-Hant");
        assert_eq!(best_match(["zh-Hans-CN"]), "zh-Hans");
        assert_eq!(best_match(["sr-Latn-RS"]), "sr-Latn");
        assert_eq!(best_match(["sr-RS"]), "sr-Cyrl");
    }

    #[test]
    fn falls_back_to_english() {
        assert_eq!(best_match(["xx-YY"]), "en");
        assert_eq!(best_match(Vec::<String>::new()), "en");
        assert_eq!(best_match(["fi-FI", "de-DE"]), "de");
    }

    #[test]
    fn every_locale_parses_and_has_every_message() {
        let english = FluentResource::try_new(LOCALES[0].1.to_string()).unwrap();
        let ids: Vec<String> = english
            .entries()
            .filter_map(|e| match e {
                fluent_syntax::ast::Entry::Message(m) => Some(m.id.name.to_string()),
                _ => None,
            })
            .collect();
        for (tag, source) in LOCALES {
            let res = FluentResource::try_new(source.to_string())
                .unwrap_or_else(|(_, e)| panic!("{tag}: {e:?}"));
            let have: Vec<String> = res
                .entries()
                .filter_map(|e| match e {
                    fluent_syntax::ast::Entry::Message(m) => Some(m.id.name.to_string()),
                    _ => None,
                })
                .collect();
            for id in &ids {
                assert!(have.contains(id), "{tag} is missing '{id}'");
            }
        }
    }

    #[test]
    fn formats_arguments() {
        let l = Localizer::new(Some("de"));
        assert_eq!(
            l.t("updated", &[("title", "Möwen")]),
            "Neues Bild gesetzt: Möwen"
        );
    }
}
