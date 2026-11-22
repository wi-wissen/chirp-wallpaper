//! Data shown in the window, prepared off the UI thread.

use chirp_core::{history, CachedPicture, Paths, Picture, State};
pub use chirp_native::ui::*;
use slint::{Rgb8Pixel, SharedPixelBuffer};
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct Snapshot {
    pub current: Option<Picture>,
    pub current_file: Option<PathBuf>,
    pub history: Vec<CachedPicture>,
}

pub struct Loaded {
    pub state: State,
    pub picture: Option<SharedPixelBuffer<Rgb8Pixel>>,
    pub baseline: Option<Picture>,
    pub history: Vec<(CachedPicture, Option<SharedPixelBuffer<Rgb8Pixel>>)>,
}

/// Read state and decode pictures (scaled down) on a worker thread.
pub fn load(paths: &Paths) -> Loaded {
    let state = State::load(&paths.state_file());
    let picture = state.current_file.as_deref().and_then(|f| decode(f, 1280));
    let baseline = chirp_core::apply::baseline(paths).map(|(p, _)| p);
    let history = history(paths)
        .into_iter()
        .take(30)
        .map(|c| {
            let thumb = decode(&c.thumb, 480);
            (c, thumb)
        })
        .collect();
    Loaded {
        state,
        picture,
        baseline,
        history,
    }
}

fn decode(file: &Path, max: u32) -> Option<SharedPixelBuffer<Rgb8Pixel>> {
    let img = image::open(file).ok()?;
    let img = if img.width() > max || img.height() > max {
        img.thumbnail(max, max)
    } else {
        img
    };
    let rgb = img.to_rgb8();
    Some(SharedPixelBuffer::clone_from_slice(
        rgb.as_raw(),
        rgb.width(),
        rgb.height(),
    ))
}

pub fn fit_index(fit: chirp_core::Fit) -> i32 {
    use chirp_core::Fit::*;
    match fit {
        Fill => 0,
        Fit => 1,
        Stretch => 2,
        Center => 3,
        Span => 4,
    }
}

pub fn fit_from(index: i32) -> chirp_core::Fit {
    use chirp_core::Fit::*;
    [Fill, Fit, Stretch, Center, Span]
        .get(index.max(0) as usize)
        .copied()
        .unwrap_or(Fill)
}

pub fn corner_index(corner: chirp_core::Corner) -> i32 {
    use chirp_core::Corner::*;
    match corner {
        BottomRight => 0,
        BottomLeft => 1,
        TopRight => 2,
        TopLeft => 3,
    }
}

pub fn corner_from(index: i32) -> chirp_core::Corner {
    use chirp_core::Corner::*;
    [BottomRight, BottomLeft, TopRight, TopLeft]
        .get(index.max(0) as usize)
        .copied()
        .unwrap_or(BottomRight)
}
