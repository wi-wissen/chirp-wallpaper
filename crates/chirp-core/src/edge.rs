//! The background of the Microsoft Edge new tab page.
//!
//! Edge reads its page settings from the MSN configuration service
//! (`assets.msn.com/resolver/api/resolve/v3/config`). The key
//! `BackgroundImageWC/default` lists the landscape pictures Edge rotates
//! through (`simpleImages`). Each picture exists in several sizes
//! (`i100` … `i2160`), addressed by an MSN image id. Titles and search terms
//! sit in `localizedStrings` (`cmsimage_titles`, `cmsimage_querys`), keyed
//! `simple<n>` where `n` counts on after the `cmsImage` gallery pictures.
//!
//! The service is undocumented.

use crate::config::Resolution;
use crate::picture::Picture;
use crate::{Error, Result};
use serde_json::Value;

pub const CONFIG_BASE: &str = "https://assets.msn.com";
pub const IMAGE_BASE: &str = "https://img-s-msn-com.akamaized.net";

/// A configuration version the service is known to answer; used when the
/// version derived from today's date is refused.
const KNOWN_VERSION: &str = "20251101.1";

/// `CHIRP_EDGE_BASE` replaces both hosts in tests.
fn test_base() -> Option<String> {
    std::env::var("CHIRP_EDGE_BASE")
        .ok()
        .filter(|v| !v.is_empty())
}

fn config_base() -> String {
    test_base().unwrap_or_else(|| CONFIG_BASE.to_string())
}

fn image_base() -> String {
    test_base().unwrap_or_else(|| IMAGE_BASE.to_string())
}

/// Address of one picture size.
fn image_url(id: &str) -> String {
    format!("{}/tenant/amp/entityid/{id}.img", image_base())
}

/// `de-DE` → language `de`, market `de` (as Edge sends it).
fn target_scope(market: &str) -> String {
    let mut parts = market.split('-');
    let language = parts.next().unwrap_or("en").to_ascii_lowercase();
    let region = parts.next().unwrap_or("us").to_ascii_lowercase();
    let locale = serde_json::json!({
        "language": language,
        "market": region,
    });
    serde_json::json!({
        "locale": { "content": locale, "display": locale },
        "browser": { "browserType": "edgeChromium" },
    })
    .to_string()
}

fn config_url(market: &str, version: &str) -> String {
    let scope: String = url_encode(&target_scope(market));
    format!(
        "{}/resolver/api/resolve/v3/config/?expType=AppConfig&apptype=edgeChromium&v={version}&targetScope={scope}",
        config_base()
    )
}

fn url_encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Parse the configuration and describe the picture for `today`.
///
/// Edge itself rolls a random plan per browser, so no two computers show the
/// same picture. Chirp walks through the landscapes by date instead: one new
/// picture a day, the same on every computer.
pub fn parse_config(
    json: &str,
    market: &str,
    today: &str,
    resolution: Resolution,
) -> Result<Picture> {
    let root: Value =
        serde_json::from_str(json).map_err(|e| Error::Format(format!("edge config: {e}")))?;
    let props = root
        .pointer("/configs/BackgroundImageWC~1default/properties")
        .ok_or_else(|| Error::Format("edge config without pictures".into()))?;
    let pictures: Vec<(usize, &Value)> = props
        .pointer("/simpleImages/data")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .enumerate()
                .filter(|(_, v)| {
                    !v.get("removeContent")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                })
                .collect()
        })
        .unwrap_or_default();
    if pictures.is_empty() {
        return Err(Error::Format("edge config without pictures".into()));
    }
    let (index, chosen) = pictures[day_number(today) % pictures.len()];
    // Texts are numbered after the other gallery pictures (cmsImage).
    let offset = props
        .pointer("/cmsImage/data")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let text_key = format!("simple{}", offset + index);
    let text = |table: &str| {
        props
            .pointer(&format!("/localizedStrings/{table}/{text_key}"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    let title = text("cmsimage_titles").unwrap_or_default();
    let query = text("cmsimage_querys").or_else(|| (!title.is_empty()).then(|| title.clone()));
    let size = |key: &str| {
        chosen
            .pointer(&format!("/image/{key}"))
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .map(str::to_string)
    };
    let full_hd = size("i1080");
    let (best, fallback) = match resolution {
        Resolution::Uhd => (size("i2160").or_else(|| full_hd.clone()), full_hd),
        Resolution::FullHd => (full_hd.clone().or_else(|| size("i2160")), None),
    };
    let best = best.ok_or_else(|| Error::Format("edge picture without address".into()))?;
    let fallback = fallback.filter(|f| *f != best);
    let attribution = chosen
        .get("attribution")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let copyright = match attribution {
        "" => String::new(),
        a if a.starts_with('©') => a.to_string(),
        a => format!("© {a}"),
    };
    // One id per picture, whatever the size.
    let key = size("i2160").unwrap_or_else(|| best.clone());
    Ok(Picture {
        id: format!("edge-{key}"),
        source: "edge".into(),
        market: market.into(),
        date: today.into(),
        // Without a title the apps say "Microsoft Edge background".
        title,
        caption: String::new(),
        copyright,
        description: None,
        // Edge links its titles to a Bing search, so does Chirp.
        info_url: query.map(|q| {
            format!(
                "https://www.bing.com/search?q={}&mkt={market}&form=NTBGIM",
                url_encode(&q)
            )
        }),
        image_url: image_url(&best),
        fallback_url: fallback.as_deref().map(image_url),
        thumb_url: size("i240").or(size("i720")).as_deref().map(image_url),
        note: None,
    })
}

/// Days since 1970-01-01 for `YYYY-MM-DD` (0 if it cannot be read).
fn day_number(date: &str) -> usize {
    let fmt = time::macros::format_description!("[year]-[month]-[day]");
    time::Date::parse(date, fmt)
        .map(|d| {
            (d.to_julian_day()
                - time::Date::from_calendar_date(1970, time::Month::January, 1)
                    .map_or(0, |e| e.to_julian_day()))
            .max(0) as usize
        })
        .unwrap_or(0)
}

/// Fetch the Edge picture of the day.
pub fn fetch_latest(market: &str, resolution: Resolution) -> Result<Picture> {
    let agent = crate::bing::agent();
    let today = crate::today();
    // The service wants a version; one from this month is always known.
    let version = format!(
        "{}01.1",
        today.replace('-', "").get(..6).unwrap_or("202511")
    );
    let body = match crate::bing::get_text(&agent, &config_url(market, &version)) {
        Err(Error::Http(_)) | Err(Error::Format(_)) => {
            crate::bing::get_text(&agent, &config_url(market, KNOWN_VERSION))?
        }
        other => other?,
    };
    match parse_config(&body, market, &today, resolution) {
        Ok(p) => Ok(p),
        Err(Error::Format(_)) if version != KNOWN_VERSION => {
            let body = crate::bing::get_text(&agent, &config_url(market, KNOWN_VERSION))?;
            parse_config(&body, market, &today, resolution)
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = include_str!("../tests/fixtures/edge-config.json");

    #[test]
    fn picture_titles_and_links_come_from_the_texts() {
        // 1970-01-01 is day 0: the first landscape.
        let p = parse_config(CONFIG, "de-DE", "1970-01-01", Resolution::Uhd).unwrap();
        assert_eq!(p.id, "edge-BB1msOP9");
        assert_eq!(p.source, "edge");
        assert_eq!(p.date, "1970-01-01");
        assert_eq!(
            p.title,
            "Sonnenuntergang an einem Hang des Val d'Orcia in der Toskana, Italien"
        );
        assert_eq!(p.copyright, "© Getty Images");
        assert_eq!(
            p.info_url.as_deref(),
            Some("https://www.bing.com/search?q=Toskana%2C%20Italien&mkt=de-DE&form=NTBGIM")
        );
        assert!(p.image_url.ends_with("/tenant/amp/entityid/BB1msOP9.img"));
        assert!(p
            .fallback_url
            .as_deref()
            .unwrap()
            .ends_with("/BB1msOP5.img"));
        assert!(p.thumb_url.as_deref().unwrap().ends_with("/BB1msFQu.img"));
    }

    #[test]
    fn a_new_landscape_every_day_in_turn() {
        let ids: Vec<String> = ["2026-11-26", "2026-11-27", "2026-11-28", "2026-11-29"]
            .iter()
            .map(|d| {
                parse_config(CONFIG, "de-DE", d, Resolution::Uhd)
                    .unwrap()
                    .id
            })
            .collect();
        // Three landscapes in the fixture: every day another, then again.
        assert_ne!(ids[0], ids[1]);
        assert_ne!(ids[1], ids[2]);
        assert_ne!(ids[0], ids[2]);
        assert_eq!(ids[0], ids[3]);
        // The same day gives the same picture.
        let again = parse_config(CONFIG, "de-DE", "2026-11-27", Resolution::Uhd).unwrap();
        assert_eq!(again.id, ids[1]);
    }

    #[test]
    fn last_landscape_has_title_but_no_search_terms() {
        // Day 2 → third landscape, whose search terms are missing: the title is searched.
        let p = parse_config(CONFIG, "de-DE", "1970-01-03", Resolution::Uhd).unwrap();
        assert_eq!(p.title, "Wilde Gänseblümchen unter klarem blauem Himmel");
        assert!(p.info_url.unwrap().contains("q=Wilde%20G"));
    }

    #[test]
    fn full_hd_has_no_fallback() {
        let p = parse_config(CONFIG, "de-DE", "1970-01-01", Resolution::FullHd).unwrap();
        assert!(p.image_url.ends_with("/BB1msOP5.img"));
        assert!(p.fallback_url.is_none());
        // Same picture, same id, whatever the size.
        assert_eq!(p.id, "edge-BB1msOP9");
    }

    #[test]
    fn rejects_answers_without_pictures() {
        assert!(parse_config("{}", "de-DE", "2026-11-26", Resolution::Uhd).is_err());
        assert!(parse_config("not json", "de-DE", "2026-11-26", Resolution::Uhd).is_err());
        let empty = r#"{"configs":{"BackgroundImageWC/default":{"properties":
            {"simpleImages":{"data":[]}}}}}"#;
        assert!(parse_config(empty, "de-DE", "2026-11-26", Resolution::Uhd).is_err());
    }

    #[test]
    fn request_names_language_and_market() {
        let url = config_url("de-DE", "20261101.1");
        assert!(url.contains("v=20261101.1"));
        assert!(url.contains("%22language%22%3A%22de%22"));
        assert!(url.contains("%22market%22%3A%22de%22"));
        assert!(url.contains("edgeChromium"));
    }
}
