//! Platform-independent core of Chirp Wallpaper.
//!
//! The core knows how to find today's picture, download it into a cache,
//! remember what happened and decide what to do when the network is gone.
//! Setting the desktop or lock screen is left to a [`Platform`] implementation
//! supplied by the front end (CLI, Windows app, GNOME app).

pub mod apply;
pub mod bing;
pub mod caption;
pub mod config;
pub mod markets;
pub mod paths;
pub mod picture;
pub mod state;

pub use apply::{
    apply, copies_dir, history, reapply, use_cached, CachedPicture, Outcome, Platform, Target,
    Trigger,
};
pub use config::{Config, Corner, Fit, Layout, Resolution, Screens};
pub use paths::{Paths, Scope};
pub use picture::Picture;
pub use state::State;

/// Errors the core can report.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("network error: {0}")]
    Network(String),
    #[error("unexpected answer from the picture service: {0}")]
    Format(String),
    #[error("file error: {0}")]
    Io(#[from] std::io::Error),
    #[error("configuration error: {0}")]
    Config(String),
    #[error("{0}")]
    Platform(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Today's date as `YYYY-MM-DD` (UTC is precise enough for a daily picture).
pub fn today() -> String {
    let now = time::OffsetDateTime::now_utc();
    now.date()
        .format(time::macros::format_description!("[year]-[month]-[day]"))
        .unwrap_or_default()
}

/// The current time as RFC 3339, used for timestamps in the state file.
pub fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}
