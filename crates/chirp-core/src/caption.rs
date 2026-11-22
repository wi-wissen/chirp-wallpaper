//! Writing the picture's title and copyright into the picture itself, so
//! the desktop shows what it is without opening the app.
//!
//! The text sits on a translucent dark box in one corner. Sizes follow the
//! picture height, and the bottom margin leaves room for the taskbar.

use crate::config::Corner;
use crate::picture::Picture;
use crate::{Error, Result};
use ab_glyph::{point, Font, FontRef, PxScale, ScaleFont};
use image::{Rgb, RgbImage};
use std::path::Path;

const REGULAR: &[u8] = include_bytes!("../../../assets/fonts/NotoSans-Regular.ttf");
const BOLD: &[u8] = include_bytes!("../../../assets/fonts/NotoSans-Bold.ttf");

/// Render `src` with the caption into `dst` (JPEG).
pub fn render(src: &Path, dst: &Path, picture: &Picture, corner: Corner) -> Result<()> {
    let regular = FontRef::try_from_slice(REGULAR).map_err(|e| Error::Format(e.to_string()))?;
    let bold = FontRef::try_from_slice(BOLD).map_err(|e| Error::Format(e.to_string()))?;
    let mut img = image::open(src)
        .map_err(|e| Error::Format(format!("picture: {e}")))?
        .to_rgb8();
    let (w, h) = img.dimensions();
    let unit = h as f32 / 1080.0;

    let mut lines: Vec<(&FontRef, f32, String)> = vec![(&bold, 30.0 * unit, picture.title.clone())];
    let mut second = Vec::new();
    if !picture.caption.is_empty() && picture.caption != picture.title {
        second.push(picture.caption.clone());
    }
    if !picture.copyright.is_empty() {
        second.push(picture.copyright.clone());
    }
    if !second.is_empty() {
        lines.push((&regular, 19.0 * unit, second.join("  ·  ")));
    }

    let pad = 18.0 * unit;
    let gap = 6.0 * unit;
    let max_text = w as f32 * 0.45;
    let lines: Vec<_> = lines
        .into_iter()
        .map(|(f, size, text)| {
            let text = fit_text(f, size, &text, max_text);
            let width = text_width(f, size, &text);
            (f, size, text, width)
        })
        .collect();
    let box_w = lines.iter().map(|l| l.3).fold(0.0, f32::max) + 2.0 * pad;
    let box_h = lines.iter().map(|l| l.1 * 1.25).sum::<f32>()
        + gap * (lines.len() as f32 - 1.0)
        + 2.0 * pad;

    let margin_x = 40.0 * unit;
    let margin_top = 40.0 * unit;
    let margin_bottom = 90.0 * unit; // clear of the taskbar
    let x0 = match corner {
        Corner::BottomLeft | Corner::TopLeft => margin_x,
        Corner::BottomRight | Corner::TopRight => w as f32 - margin_x - box_w,
    };
    let y0 = match corner {
        Corner::TopLeft | Corner::TopRight => margin_top,
        Corner::BottomLeft | Corner::BottomRight => h as f32 - margin_bottom - box_h,
    };

    shade(&mut img, x0, y0, box_w, box_h, 12.0 * unit, 0.55);
    let mut y = y0 + pad;
    for (font, size, text, _) in &lines {
        draw_text(&mut img, font, *size, text, x0 + pad, y);
        y += size * 1.25 + gap;
    }

    if let Some(dir) = dst.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let part = dst.with_extension("part");
    {
        let file = std::fs::File::create(&part)?;
        let mut writer = std::io::BufWriter::new(file);
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut writer, 95)
            .encode_image(&img)
            .map_err(|e| Error::Format(e.to_string()))?;
    }
    std::fs::rename(&part, dst)?;
    Ok(())
}

fn text_width(font: &FontRef, size: f32, text: &str) -> f32 {
    let scaled = font.as_scaled(PxScale::from(size));
    let mut width = 0.0;
    let mut prev = None;
    for c in text.chars() {
        let id = scaled.glyph_id(c);
        if let Some(p) = prev {
            width += scaled.kern(p, id);
        }
        width += scaled.h_advance(id);
        prev = Some(id);
    }
    width
}

/// Shorten with an ellipsis until the text fits.
fn fit_text(font: &FontRef, size: f32, text: &str, max: f32) -> String {
    if text_width(font, size, text) <= max {
        return text.to_string();
    }
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let candidate: String = chars.iter().collect::<String>().trim_end().to_string() + "…";
        if text_width(font, size, &candidate) <= max {
            return candidate;
        }
    }
    String::new()
}

fn draw_text(img: &mut RgbImage, font: &FontRef, size: f32, text: &str, x: f32, top: f32) {
    let scaled = font.as_scaled(PxScale::from(size));
    let baseline = top + scaled.ascent();
    let mut caret = x;
    let mut prev = None;
    let (w, h) = img.dimensions();
    for c in text.chars() {
        let id = scaled.glyph_id(c);
        if let Some(p) = prev {
            caret += scaled.kern(p, id);
        }
        let glyph = id.with_scale_and_position(PxScale::from(size), point(caret, baseline));
        caret += scaled.h_advance(id);
        prev = Some(id);
        if let Some(outline) = font.outline_glyph(glyph) {
            let bounds = outline.px_bounds();
            outline.draw(|gx, gy, coverage| {
                let px = bounds.min.x as i32 + gx as i32;
                let py = bounds.min.y as i32 + gy as i32;
                if px >= 0 && py >= 0 && (px as u32) < w && (py as u32) < h {
                    let pixel = img.get_pixel_mut(px as u32, py as u32);
                    blend(pixel, [255, 255, 255], coverage.min(1.0));
                }
            });
        }
    }
}

/// Darken a rounded rectangle.
fn shade(img: &mut RgbImage, x: f32, y: f32, w: f32, h: f32, radius: f32, alpha: f32) {
    let (iw, ih) = img.dimensions();
    let x1 = (x + w).min(iw as f32);
    let y1 = (y + h).min(ih as f32);
    for py in y.max(0.0) as u32..y1 as u32 {
        for px in x.max(0.0) as u32..x1 as u32 {
            let fx = px as f32 + 0.5;
            let fy = py as f32 + 0.5;
            let dx = (x + radius - fx).max(fx - (x + w - radius)).max(0.0);
            let dy = (y + radius - fy).max(fy - (y + h - radius)).max(0.0);
            let d = (dx * dx + dy * dy).sqrt();
            let a = alpha * (radius - d + 0.5).clamp(0.0, 1.0);
            if a > 0.0 {
                blend(img.get_pixel_mut(px, py), [0, 0, 0], a);
            }
        }
    }
}

fn blend(pixel: &mut Rgb<u8>, color: [u8; 3], a: f32) {
    for i in 0..3 {
        let v = pixel[i] as f32 * (1.0 - a) + color[i] as f32 * a;
        pixel[i] = v.round() as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_caption_into_corner() {
        let dir = std::env::temp_dir().join(format!("chirp-caption-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("src.png");
        RgbImage::from_pixel(1920, 1080, Rgb([200, 200, 200]))
            .save(&src)
            .unwrap();
        let picture = Picture {
            id: "x".into(),
            source: "bing".into(),
            market: "de-DE".into(),
            date: "2026-11-22".into(),
            title: "Möwen über den Hügeln".into(),
            caption: "Beispielland".into(),
            copyright: "© Beispiel".into(),
            description: None,
            info_url: None,
            image_url: String::new(),
            fallback_url: None,
            thumb_url: None,
            note: None,
        };
        let dst = dir.join("out.jpg");
        render(&src, &dst, &picture, Corner::BottomRight).unwrap();
        let out = image::open(&dst).unwrap().to_rgb8();
        assert_eq!(out.dimensions(), (1920, 1080));
        // Top left stays untouched, the bottom right corner got the dark box.
        assert!(out.get_pixel(10, 10)[0] > 190);
        let p = out.get_pixel(1920 - 60, 1080 - 110);
        assert!(p[0] < 140, "expected shade, got {p:?}");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
