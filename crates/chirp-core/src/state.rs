//! What happened last time: the picture in use and the last success or error.

use crate::picture::Picture;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    /// The picture currently set.
    pub current: Option<Picture>,
    /// Its file on disk.
    pub current_file: Option<PathBuf>,
    /// The newest picture of the day that was set. Automatic checks only
    /// change the screen when a different one appears.
    pub daily_id: Option<String>,
    /// Time of the last attempt (RFC 3339).
    pub last_attempt: Option<String>,
    /// Time of the last successful update (RFC 3339).
    pub last_success: Option<String>,
    /// Why the last attempt failed; cleared after a success.
    pub last_error: Option<String>,
}

impl State {
    /// Read the state; a missing or broken file gives an empty state.
    pub fn load(file: &Path) -> Self {
        std::fs::read_to_string(file)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, file: &Path) -> Result<()> {
        write_json(file, self)
    }
}

/// Write JSON atomically (temporary file, then rename).
pub fn write_json(file: &Path, value: &impl Serialize) -> Result<()> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = file.with_extension("json.part");
    let text =
        serde_json::to_string_pretty(value).map_err(|e| crate::Error::Format(e.to_string()))?;
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, file)?;
    Ok(())
}

/// Read a picture's metadata file.
pub fn read_picture(file: &Path) -> Option<Picture> {
    std::fs::read_to_string(file)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
}
