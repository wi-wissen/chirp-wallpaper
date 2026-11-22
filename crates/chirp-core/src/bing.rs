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

/// The Bing address; `CHIRP_BING_BASE` replaces it in tests (e.g. a local
/// server that simulates being offline).
pub fn base_url() -> String {
    std::env::var("CHIRP_BING_BASE")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| BASE_URL.to_string())
}

#[derive(Debug, Deserialize)]
struct Archive {
    images: Vec<ArchiveImage>,
}

#[derive(Debug, Deserialize)]
struct ArchiveImage {
    startdate: String,
    /// Start time in UTC, `YYYYMMDDHHMM`, e.g. `202611212300` for the German
    /// picture of 22 November (midnight in Germany).
    #[serde(default)]
    fullstartdate: String,
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
    image: Option<ImageRef>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    headline: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ImageRef {
    #[serde(default)]
    url: String,
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
    let date = local_date(&img.fullstartdate)
        .or_else(|| format_date(&img.startdate))
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
        thumb_url: Some(format!("{base}_400x240.jpg")),
        note: None,
    })
}

/// Add the description from `hp/api/model` to the matching picture.
///
/// The details endpoint does not always answer for the requested market's
/// current picture, so it is matched by the picture's name (`OHR.<Name>_…`)
/// and only by date when no image address is given.
pub fn merge_details(pictures: &mut [Picture], model_json: &str) -> Result<()> {
    let model: Model =
        serde_json::from_str(model_json).map_err(|e| Error::Format(format!("details: {e}")))?;
    for content in model.media_contents {
        let ic = content.image_content;
        let name = ic.image.as_ref().and_then(|i| image_name(&i.url));
        let pic = match name {
            Some(name) => pictures
                .iter_mut()
                .find(|p| image_name(&p.image_url).as_deref() == Some(name.as_str())),
            None => {
                let Some(date) = format_date(&content.ssd) else {
                    continue;
                };
                pictures.iter_mut().find(|p| p.date == date)
            }
        };
        let Some(pic) = pic else { continue };
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

/// `…?id=OHR.JustaHermitage_DE-DE4182731523_UHD.jpg` → `OHR.JustaHermitage`.
fn image_name(url: &str) -> Option<String> {
    let start = url.find("OHR.")?;
    let rest = &url[start..];
    let end = rest.find('_').unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

/// The day a picture is shown on: its UTC start time plus twelve hours lands
/// on the local calendar day for every market (Europe starts at 23:00 UTC of
/// the previous day, America at 08:00 UTC, Asia at 15:00 UTC).
fn local_date(fullstartdate: &str) -> Option<String> {
    if fullstartdate.len() < 12 || !fullstartdate.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let fmt = time::macros::format_description!("[year][month][day][hour][minute]");
    let start = time::PrimitiveDateTime::parse(&fullstartdate[..12], fmt).ok()?;
    let shown = start + time::Duration::hours(12);
    shown
        .date()
        .format(time::macros::format_description!("[year]-[month]-[day]"))
        .ok()
}

/// Fetch today's picture (with description, if available) for a market.
pub fn fetch_latest(market: &str, resolution: Resolution) -> Result<Picture> {
    let agent = agent();
    let base = base_url();
    let archive_url = format!("{base}/HPImageArchive.aspx?format=js&idx=0&n=1&mkt={market}");
    let body = get_text(&agent, &archive_url)?;
    let mut pictures = parse_archive(&body, market, resolution)?;
    if pictures.is_empty() {
        return Err(Error::Format("no picture in answer".into()));
    }
    // The description is a bonus: ignore failures.
    if let Ok(details) = get_text(&agent, &format!("{base}/hp/api/model?mkt={market}")) {
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
        .map_err(request_error)?
        .into_string()
        .map_err(|e| Error::Network(e.to_string()))
}

/// Download `url` into `writer`.
pub fn download(agent: &ureq::Agent, url: &str, writer: &mut impl std::io::Write) -> Result<u64> {
    let response = agent.get(url).call().map_err(request_error)?;
    let mut reader = response.into_reader();
    Ok(std::io::copy(&mut reader, writer)?)
}

fn request_error(e: ureq::Error) -> Error {
    match e {
        ureq::Error::Status(code, _) => Error::Http(code),
        ureq::Error::Transport(t) => Error::Network(t.kind().to_string()),
    }
}

fn absolute(path: &str) -> String {
    if path.starts_with("http://") || path.starts_with("https://") {
        path.to_string()
    } else {
        format!("{}{path}", base_url())
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
    fn local_dates() {
        assert_eq!(local_date("202611212300").as_deref(), Some("2026-11-22"));
        assert_eq!(local_date("202611220800").as_deref(), Some("2026-11-22"));
        assert_eq!(local_date("202611211500").as_deref(), Some("2026-11-22"));
        assert_eq!(local_date(""), None);
    }

    #[test]
    fn details_are_matched_by_picture_name() {
        let mut pics = parse_archive(ARCHIVE, "de-DE", Resolution::Uhd).unwrap();
        let other = r#"{"MediaContents":[{"Ssd":"20261122","ImageContent":{
            "Image":{"Url":"/th?id=OHR.SomethingElse_EN-US1_1920x1080.webp"},
            "Description":"Wrong picture","Headline":"Wrong","Title":"Wrong"}}]}"#;
        merge_details(&mut pics, other).unwrap();
        assert!(pics[0].description.is_none());
        assert_eq!(pics[0].title, "Möwen über den Hügeln");
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
