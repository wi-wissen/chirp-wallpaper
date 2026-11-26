//! Configuration.
//!
//! Settings are read from several TOML files and merged key by key, later
//! files winning: built-in defaults → machine-wide file → portable file next
//! to the program → the user's own file. On reset ("frozen") school machines
//! the machine-wide file is written once while the image is prepared.

use crate::markets;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Resolution {
    /// The largest version Bing offers, usually 3840×2160 (4K) or more.
    #[default]
    Uhd,
    /// 1920×1080.
    #[serde(rename = "1080p")]
    FullHd,
}

/// Where the pictures come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// The Bing picture of the day, with title and description.
    #[default]
    Bing,
    /// The landscape background of the Microsoft Edge new tab page.
    Edge,
}

/// How the picture fills the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Fit {
    /// Cover the screen, cutting off what does not fit.
    #[default]
    Fill,
    /// Show the whole picture, with bars if needed.
    Fit,
    /// Stretch to the screen size.
    Stretch,
    /// Original size in the middle.
    Center,
    /// One picture across all screens.
    Span,
}

/// Which screens get the picture (Windows).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Screens {
    #[default]
    All,
    Primary,
}

/// Corner for the description written into the picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Corner {
    #[default]
    BottomRight,
    BottomLeft,
    TopRight,
    TopLeft,
}

/// How the desktop picture is placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Layout {
    pub fit: Fit,
    pub screens: Screens,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Where the pictures come from.
    pub source: Source,
    /// Bing market such as `de-DE`, or `auto` to follow the system language.
    pub market: String,
    /// Picture size to download.
    pub resolution: Resolution,
    /// Set the desktop background.
    pub desktop: bool,
    /// Set the lock screen as well.
    pub lockscreen: bool,
    /// How many days downloaded pictures stay in the cache.
    pub keep_days: u32,
    /// Do not change the picture until this day (`YYYY-MM-DD`), inclusive.
    pub paused_until: Option<String>,
    /// How the picture fills the screen.
    pub fit: Fit,
    /// All screens or only the main screen.
    pub screens: Screens,
    /// Write title and copyright into the picture.
    pub caption: bool,
    /// Where to write them.
    pub caption_position: Corner,
    /// Keep a copy of every new picture in the Pictures folder.
    pub save_copies: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            source: Source::Bing,
            market: "auto".into(),
            resolution: Resolution::Uhd,
            desktop: true,
            lockscreen: true,
            keep_days: 14,
            paused_until: None,
            fit: Fit::Fill,
            screens: Screens::All,
            caption: false,
            caption_position: Corner::BottomRight,
            save_copies: false,
        }
    }
}

impl Config {
    /// Merge the given files (missing files are skipped) over the defaults.
    pub fn load(files: &[impl AsRef<Path>]) -> Result<Self> {
        let mut merged =
            toml::Table::try_from(Config::default()).map_err(|e| Error::Config(e.to_string()))?;
        for file in files {
            let file = file.as_ref();
            let text = match std::fs::read_to_string(file) {
                Ok(t) => t,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
            };
            let table: toml::Table = text
                .parse()
                .map_err(|e| Error::Config(format!("{}: {e}", file.display())))?;
            merged.extend(table);
        }
        merged
            .try_into()
            .map_err(|e: toml::de::Error| Error::Config(e.to_string()))
    }

    /// Write this configuration as TOML.
    pub fn save(&self, file: &Path) -> Result<()> {
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = toml::to_string_pretty(self).map_err(|e| Error::Config(e.to_string()))?;
        std::fs::write(file, text)?;
        Ok(())
    }

    /// The market to request, resolving `auto` with the system locale.
    pub fn resolved_market(&self, system_locale: Option<&str>) -> String {
        if self.market.trim().is_empty() || self.market.eq_ignore_ascii_case("auto") {
            markets::market_for_locale(system_locale.unwrap_or("")).to_string()
        } else {
            self.market.trim().to_string()
        }
    }

    /// Placement of the desktop picture.
    pub fn layout(&self) -> Layout {
        Layout {
            fit: self.fit,
            screens: self.screens,
        }
    }

    /// Whether updates are paused on `today` (`YYYY-MM-DD`).
    pub fn is_paused(&self, today: &str) -> bool {
        self.paused_until
            .as_deref()
            .is_some_and(|until| today <= until)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn later_files_win_key_by_key() {
        let dir = std::env::temp_dir().join(format!("chirp-cfg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let machine = dir.join("machine.toml");
        let user = dir.join("user.toml");
        std::fs::write(&machine, "market = \"de-DE\"\nlockscreen = false\n").unwrap();
        std::fs::write(&user, "resolution = \"1080p\"\n").unwrap();
        let cfg = Config::load(&[machine, dir.join("missing.toml"), user]).unwrap();
        assert_eq!(cfg.market, "de-DE");
        assert!(!cfg.lockscreen);
        assert!(cfg.desktop);
        assert_eq!(cfg.resolution, Resolution::FullHd);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unknown_keys_are_reported() {
        let dir = std::env::temp_dir().join(format!("chirp-cfg-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("c.toml");
        std::fs::write(&f, "markt = \"de-DE\"\n").unwrap();
        assert!(Config::load(&[f]).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn auto_market_follows_locale() {
        let cfg = Config::default();
        assert_eq!(cfg.resolved_market(Some("de-AT")), "de-DE");
        assert_eq!(cfg.resolved_market(None), "en-US");
        let fixed = Config {
            market: "fr-FR".into(),
            ..Config::default()
        };
        assert_eq!(fixed.resolved_market(Some("de-DE")), "fr-FR");
    }

    #[test]
    fn new_settings_round_trip() {
        let dir = std::env::temp_dir().join(format!("chirp-cfg-rt-{}", std::process::id()));
        let f = dir.join("c.toml");
        let cfg = Config {
            fit: Fit::Span,
            screens: Screens::Primary,
            caption: true,
            caption_position: Corner::TopLeft,
            save_copies: true,
            source: Source::Edge,
            ..Config::default()
        };
        cfg.save(&f).unwrap();
        let text = std::fs::read_to_string(&f).unwrap();
        assert!(text.contains("source = \"edge\""));
        assert!(text.contains("fit = \"span\""));
        assert!(text.contains("caption_position = \"top-left\""));
        assert_eq!(Config::load(&[&f]).unwrap(), cfg);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn pause_is_inclusive() {
        let cfg = Config {
            paused_until: Some("2026-11-30".into()),
            ..Config::default()
        };
        assert!(cfg.is_paused("2026-11-22"));
        assert!(cfg.is_paused("2026-11-30"));
        assert!(!cfg.is_paused("2026-12-01"));
    }
}
