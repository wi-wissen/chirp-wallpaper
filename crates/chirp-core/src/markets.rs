//! Bing markets ("mkt" parameter).
//!
//! Bing publishes its own daily picture, with localized texts, only for some
//! markets. Every other market receives the "rest of the world" picture with
//! English texts. We therefore map a system locale to the closest market that
//! has its own picture and texts.

/// Markets known to publish their own daily picture and localized texts.
pub const MARKETS_WITH_OWN_PICTURE: &[&str] = &[
    "de-DE", "en-AU", "en-CA", "en-GB", "en-IN", "en-US", "es-ES", "fr-CA", "fr-FR", "it-IT",
    "ja-JP", "pt-BR", "zh-CN",
];

/// Fallback market when nothing fits.
pub const DEFAULT_MARKET: &str = "en-US";

/// Pick the Bing market for a locale such as `de-AT`, `de_DE.UTF-8` or `fr`.
///
/// 1. An exact match with a market that has its own picture wins.
/// 2. Otherwise the language decides (`de-AT` → `de-DE`, `pt-PT` → `pt-BR`).
/// 3. Otherwise the default market is used.
pub fn market_for_locale(locale: &str) -> &'static str {
    let normalized = normalize(locale);
    if let Some(m) = MARKETS_WITH_OWN_PICTURE
        .iter()
        .find(|m| m.eq_ignore_ascii_case(&normalized))
    {
        return m;
    }
    let lang = normalized
        .split('-')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    match lang.as_str() {
        "de" => "de-DE",
        "fr" => "fr-FR",
        "es" => "es-ES",
        "it" => "it-IT",
        "ja" => "ja-JP",
        "pt" => "pt-BR",
        "zh" => "zh-CN",
        _ => DEFAULT_MARKET,
    }
}

/// Turn `de_DE.UTF-8` or `de-de` into `de-DE`.
fn normalize(locale: &str) -> String {
    let base = locale.split(['.', '@']).next().unwrap_or("");
    let mut parts = base.split(['-', '_']);
    let lang = parts.next().unwrap_or("").to_ascii_lowercase();
    match parts.next() {
        Some(region) if region.len() == 2 => format!("{lang}-{}", region.to_ascii_uppercase()),
        _ => lang,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_market() {
        assert_eq!(market_for_locale("de-DE"), "de-DE");
        assert_eq!(market_for_locale("en-GB"), "en-GB");
    }

    #[test]
    fn posix_style_locale() {
        assert_eq!(market_for_locale("de_DE.UTF-8"), "de-DE");
        assert_eq!(market_for_locale("fr_CA"), "fr-CA");
    }

    #[test]
    fn language_fallback() {
        assert_eq!(market_for_locale("de-AT"), "de-DE");
        assert_eq!(market_for_locale("de-CH"), "de-DE");
        assert_eq!(market_for_locale("pt-PT"), "pt-BR");
        assert_eq!(market_for_locale("fr"), "fr-FR");
    }

    #[test]
    fn unknown_language_uses_default() {
        assert_eq!(market_for_locale("pl-PL"), "en-US");
        assert_eq!(market_for_locale(""), "en-US");
    }
}
