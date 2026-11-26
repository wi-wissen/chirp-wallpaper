//! The background of the Microsoft Edge new tab page.
//!
//! Edge reads its page settings from the MSN configuration service
//! (`assets.msn.com/resolver/api/resolve/v3/config`). The key
//! `BackgroundImageWC/default` lists the landscape pictures Edge shows; the
//! first one is the picture of the day. Each picture exists in several sizes
//! (`i100` … `i2160`), addressed by an MSN image id.
//!
//! The service is undocumented. It has no titles or descriptions, only who
//! holds the rights to a picture.

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

/// Parse the configuration and describe the picture of the day.
pub fn parse_config(
    json: &str,
    market: &str,
    today: &str,
    resolution: Resolution,
) -> Result<Picture> {
    let root: Value =
        serde_json::from_str(json).map_err(|e| Error::Format(format!("edge config: {e}")))?;
    let first = root
        .pointer("/configs/BackgroundImageWC~1default/properties/simpleImages/data/0")
        .ok_or_else(|| Error::Format("edge config without pictures".into()))?;
    let size = |key: &str| {
        first
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
    let attribution = first
        .get("attribution")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let copyright = match attribution {
        "" => String::new(),
        a if a.starts_with('©') => a.to_string(),
        a => format!("© {a}"),
    };
    // One id per picture (not per day): the same picture on the next day is
    // not a new picture of the day.
    let key = size("i2160").unwrap_or_else(|| best.clone());
    Ok(Picture {
        id: format!("edge-{key}"),
        source: "edge".into(),
        market: market.into(),
        date: today.into(),
        // No title: the apps name it "Microsoft Edge background" in their language.
        title: String::new(),
        caption: String::new(),
        copyright,
        description: None,
        info_url: None,
        image_url: image_url(&best),
        fallback_url: fallback.as_deref().map(image_url),
        thumb_url: size("i240").or(size("i720")).as_deref().map(image_url),
        note: None,
    })
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
    fn first_picture_is_the_picture_of_the_day() {
        let p = parse_config(CONFIG, "de-DE", "2026-11-26", Resolution::Uhd).unwrap();
        assert_eq!(p.id, "edge-BB1msOP9");
        assert_eq!(p.source, "edge");
        assert_eq!(p.date, "2026-11-26");
        assert!(p.title.is_empty());
        assert_eq!(p.copyright, "© Getty Images");
        assert!(p.info_url.is_none());
        assert!(p.image_url.ends_with("/tenant/amp/entityid/BB1msOP9.img"));
        assert!(p
            .fallback_url
            .as_deref()
            .unwrap()
            .ends_with("/BB1msOP5.img"));
        assert!(p.thumb_url.as_deref().unwrap().ends_with("/BB1msFQu.img"));
    }

    #[test]
    fn full_hd_has_no_fallback() {
        let p = parse_config(CONFIG, "de-DE", "2026-11-26", Resolution::FullHd).unwrap();
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
