//! The Bing picture of the day.
//!
//! Two public endpoints are used:
//! * `HPImageArchive.aspx` lists the pictures of the last days (title,
//!   copyright, link, image path).
//! * `hp/api/model` adds a longer description for today's picture.
//!
//! Both are undocumented but have been stable for many years. Only metadata
//! is parsed here; the network calls live in [`fetch_latest`].

use crate::config::Resolution;
use crate::picture::Picture;
use crate::{Error, Result};
use serde::Deserialize;
use std::time::Duration;

pub const BASE_URL: &str = "https://www.bing.com";

#[derive(Debug, Deserialize)]
struct Archive {
    images: Vec<ArchiveImage>,
}

#[derive(Debug, Deserialize)]
struct ArchiveImage {
    startdate: String,
    urlbase: String,
    #[serde(default)]
    copyright: String,
    #[serde(default)]
    copyrightlink: String,
    #[serde(default)]
    title: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Model {
    #[serde(default)]
    media_contents: Vec<MediaContent>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct MediaContent {
    #[serde(default)]
    ssd: String,
    image_content: ImageContent,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ImageContent {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    headline: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

/// Parse the answer of `HPImageArchive.aspx?format=js`.
pub fn parse_archive(json: &str, market: &str, resolution: Resolution) -> Result<Vec<Picture>> {
    let archive: Archive =
        serde_json::from_str(json).map_err(|e| Error::Format(format!("archive: {e}")))?;
    archive
        .images
        .into_iter()
        .map(|img| picture_from_archive(img, market, resolution))
        .collect()
}

fn picture_from_archive(
    img: ArchiveImage,
    market: &str,
    resolution: Resolution,
) -> Result<Picture> {
    let date = format_date(&img.startdate)
        .ok_or_else(|| Error::Format(format!("unknown date '{}'", img.startdate)))?;
    if img.urlbase.is_empty() {
        return Err(Error::Format("picture without address".into()));
    }
    let base = absolute(&img.urlbase);
    let (caption, copyright) = split_copyright(&img.copyright);
    let info_url = (!img.copyrightlink.is_empty() && !img.copyrightlink.contains("javascript:"))
        .then(|| absolute(&img.copyrightlink));
    let full_hd = format!("{base}_1920x1080.jpg");
    let (image_url, fallback_url) = match resolution {
        Resolution::Uhd => (format!("{base}_UHD.jpg"), Some(full_hd)),
        Resolution::FullHd => (full_hd, None),
    };
    Ok(Picture {
        id: format!("bing-{market}-{date}"),
        source: "bing".into(),
        market: market.into(),
        date,
        title: if img.title.is_empty() {
            caption.clone()
        } else {
            img.title
        },
        caption,
        copyright,
        description: None,
        info_url,
        image_url,
        fallback_url,
        note: None,
    })
}

/// Add the description from `hp/api/model` to the matching picture.
pub fn merge_details(pictures: &mut [Picture], model_json: &str) -> Result<()> {
    let model: Model =
        serde_json::from_str(model_json).map_err(|e| Error::Format(format!("details: {e}")))?;
    for content in model.media_contents {
        let Some(date) = format_date(&content.ssd) else {
            continue;
        };
        let Some(pic) = pictures.iter_mut().find(|p| p.date == date) else {
            continue;
        };
        let ic = content.image_content;
        if let Some(d) = ic.description.filter(|d| !d.trim().is_empty()) {
            pic.description = Some(d.trim().to_string());
        }
        if let Some(h) = ic.headline.filter(|h| !h.trim().is_empty()) {
            pic.title = h.trim().to_string();
        }
        if let Some(t) = ic.title.filter(|t| !t.trim().is_empty()) {
            pic.caption = t.trim().to_string();
        }
    }
    Ok(())
}

/// Fetch today's picture (with description, if available) for a market.
pub fn fetch_latest(market: &str, resolution: Resolution) -> Result<Picture> {
    let agent = agent();
    let archive_url = format!("{BASE_URL}/HPImageArchive.aspx?format=js&idx=0&n=1&mkt={market}");
    let body = get_text(&agent, &archive_url)?;
    let mut pictures = parse_archive(&body, market, resolution)?;
    if pictures.is_empty() {
        return Err(Error::Format("no picture in answer".into()));
    }
    // The description is a bonus: ignore failures.
    if let Ok(details) = get_text(&agent, &format!("{BASE_URL}/hp/api/model?mkt={market}")) {
        let _ = merge_details(&mut pictures, &details);
    }
    Ok(pictures.remove(0))
}

/// HTTP agent honoring proxy settings from the environment.
pub fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout(Duration::from_secs(120))
        .user_agent(concat!("chirp-wallpaper/", env!("CARGO_PKG_VERSION")))
        .try_proxy_from_env(true)
        .build()
}

fn get_text(agent: &ureq::Agent, url: &str) -> Result<String> {
    agent
        .get(url)
        .call()
        .map_err(|e| Error::Network(short_error(&e)))?
        .into_string()
        .map_err(|e| Error::Network(e.to_string()))
}

/// Download `url` into `writer`.
pub fn download(agent: &ureq::Agent, url: &str, writer: &mut impl std::io::Write) -> Result<u64> {
    let response = agent
        .get(url)
        .call()
        .map_err(|e| Error::Network(short_error(&e)))?;
    let mut reader = response.into_reader();
    Ok(std::io::copy(&mut reader, writer)?)
}

fn short_error(e: &ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, _) => format!("HTTP {code}"),
        ureq::Error::Transport(t) => t.kind().to_string(),
    }
}

fn absolute(path: &str) -> String {
    if path.starts_with("http://") || path.starts_with("https://") {
        path.to_string()
    } else {
        format!("{BASE_URL}{path}")
    }
}

/// `20261122` → `2026-11-22`.
fn format_date(raw: &str) -> Option<String> {
    let d = raw.get(..8)?;
    if !d.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(format!("{}-{}-{}", &d[..4], &d[4..6], &d[6..8]))
}

/// `Place, Country (© Photographer/Agency)` → (`Place, Country`, `© Photographer/Agency`).
fn split_copyright(text: &str) -> (String, String) {
    let text = text.trim();
    if let Some(start) = text.rfind(" (©") {
        let caption = text[..start].trim().to_string();
        let copyright = text[start + 2..].trim_end_matches(')').trim().to_string();
        return (caption, copyright);
    }
    (text.to_string(), String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARCHIVE: &str = include_str!("../tests/fixtures/archive.json");
    const MODEL: &str = include_str!("../tests/fixtures/model.json");

    #[test]
    fn parses_archive_uhd() {
        let pics = parse_archive(ARCHIVE, "de-DE", Resolution::Uhd).unwrap();
        assert_eq!(pics.len(), 2);
        let p = &pics[0];
        assert_eq!(p.id, "bing-de-DE-2026-11-22");
        assert_eq!(p.date, "2026-11-22");
        assert_eq!(p.title, "Möwen über den Hügeln");
        assert_eq!(p.caption, "Hügel bei Sonnenaufgang, Beispielland");
        assert_eq!(p.copyright, "© Beispiel Fotografin/Beispiel Agentur");
        assert_eq!(
            p.image_url,
            "https://www.bing.com/th?id=OHR.SampleHills_DE-DE123_UHD.jpg"
        );
        assert_eq!(
            p.fallback_url.as_deref(),
            Some("https://www.bing.com/th?id=OHR.SampleHills_DE-DE123_1920x1080.jpg")
        );
        assert!(p
            .info_url
            .as_deref()
            .unwrap()
            .starts_with("https://www.bing.com/search?q="));
    }

    #[test]
    fn parses_archive_full_hd() {
        let pics = parse_archive(ARCHIVE, "de-DE", Resolution::FullHd).unwrap();
        assert!(pics[0].image_url.ends_with("_1920x1080.jpg"));
        assert!(pics[0].fallback_url.is_none());
    }

    #[test]
    fn javascript_links_are_dropped() {
        let pics = parse_archive(ARCHIVE, "de-DE", Resolution::Uhd).unwrap();
        assert!(pics[1].info_url.is_none());
    }

    #[test]
    fn merges_description() {
        let mut pics = parse_archive(ARCHIVE, "de-DE", Resolution::Uhd).unwrap();
        merge_details(&mut pics, MODEL).unwrap();
        assert!(pics[0]
            .description
            .as_deref()
            .unwrap()
            .starts_with("Ein Beispieltext"));
        assert!(pics[1].description.is_none());
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_archive("{}", "de-DE", Resolution::Uhd).is_err());
        assert!(parse_archive("not json", "de-DE", Resolution::Uhd).is_err());
    }

    #[test]
    fn copyright_split() {
        assert_eq!(
            split_copyright("A, B (© C/D)"),
            ("A, B".to_string(), "© C/D".to_string())
        );
        assert_eq!(
            split_copyright("No copyright"),
            ("No copyright".to_string(), String::new())
        );
    }
}
