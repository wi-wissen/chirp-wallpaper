use serde::{Deserialize, Serialize};

/// Everything we know about one daily picture.
///
/// Stored as JSON next to the cached image, so the front ends can always show
/// what a picture is, where it came from and how to learn more about it, even
/// when the machine is offline or was reset to a frozen state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Picture {
    /// Stable id, e.g. `bing-de-DE-2026-11-22`.
    pub id: String,
    /// Source of the picture, e.g. `bing`.
    pub source: String,
    /// Market the picture was requested for, e.g. `de-DE`.
    pub market: String,
    /// Day the picture belongs to, `YYYY-MM-DD`.
    pub date: String,
    /// Short headline, e.g. "Kapelle im Fels".
    pub title: String,
    /// Longer caption naming the place.
    pub caption: String,
    /// Copyright line, always shown with the picture.
    pub copyright: String,
    /// A paragraph about the picture, when the source provides one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Link to read more about the picture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub info_url: Option<String>,
    /// Download address of the best available resolution.
    pub image_url: String,
    /// Fallback download address (1920×1080) if the best one fails.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_url: Option<String>,
    /// Small preview (about 400×240) for galleries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumb_url: Option<String>,
    /// Text marking a demo picture (e.g. AI-generated sample), if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl Picture {
    /// File name (without extension) used in the cache.
    pub fn file_stem(&self) -> String {
        self.id
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    }

    /// Text suitable for searching the web for the picture's subject.
    pub fn search_text(&self) -> &str {
        if self.caption.is_empty() {
            &self.title
        } else {
            &self.caption
        }
    }
}
