//! Text on the canvas, drawn with the app's own text engine.
//!
//! The studio's type tool places one run where the user tapped. The glyphs come from
//! `filmcraft-text` — the same font database, shaping and anti-aliased rasterisation the video
//! titles use — so a type layer looks like the rest of the app rather than like a second
//! renderer. This module is only the adapter: layout a run at a size, rasterise it, and hand back
//! a coverage mask the compositor can blend.

use filmcraft_text::{Align, ParagraphStyle, TextStyle, Xform};

/// One rasterised run: a coverage mask and where its top-left corner goes.
pub struct Run {
    pub width: u32,
    pub height: u32,
    /// Coverage, 0..=255, `width * height` bytes, row-major.
    pub coverage: Vec<u8>,
    /// The mask's top-left corner relative to the run's origin: text sits on a baseline, so the
    /// ink starts above it and the rasteriser reports where.
    pub x: i32,
    pub y: i32,
}

/// Lay out and rasterise one run of text.
///
/// `size` is the cap height in pixels; the family is the app's UI font so a type layer matches what
/// the rest of the editor draws.
pub fn layout(text: &str, size: f64) -> Result<Run, String> {
    layout_run(text, size, "Inter", "Regular", "left", 0.0, None)
}

/// Lay out one run with its face, alignment and, for a Layout text frame, its wrap box.
///
/// The box is what makes Frame Text a frame: the run wraps to its width and is cut at its height,
/// which the point-text run in the Sketch workspace never does.
#[allow(clippy::too_many_arguments)]
pub fn layout_run(
    text: &str,
    size: f64,
    family: &str,
    style_name: &str,
    align: &str,
    leading: f64,
    wrap: Option<(f64, f64)>,
) -> Result<Run, String> {
    if text.is_empty() {
        return Err("An empty run has nothing to draw.".into());
    }
    let px = size.clamp(1.0, 2048.0) as f32;
    let style = TextStyle {
        family: family.to_string(),
        style: style_name.to_string(),
        size: px,
        ..Default::default()
    };
    let para = ParagraphStyle {
        align: match align {
            "center" => Align::Center,
            "right" => Align::Right,
            "justify" => Align::Justify,
            _ => Align::Left,
        },
        // The layout's leading is extra spacing on top of the font's own line height.
        leading: ((leading.clamp(0.5, 4.0) - 1.0) * f64::from(px)).max(0.0) as f32,
        width: wrap.map(|(w, _)| w.max(1.0) as f32),
        height: wrap.map(|(_, h)| h.max(1.0) as f32),
        ..Default::default()
    };
    let laid = filmcraft_text::layout(text, &style, &para);
    // The origin is the baseline start: `at` places the layout at that point, and `rasterize`
    // reports the ink's offset from it.
    let xform: Xform = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    let (mask, x, y) = filmcraft_text::render::rasterize(&laid, &xform);
    if mask.w == 0 || mask.h == 0 {
        return Err("That text produced no ink.".into());
    }
    // The rasteriser keeps coverage as f32 (it supports wide gamut shapes); 8-bit is what the
    // compositor blends.
    let coverage = mask.a.iter().map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8).collect();
    Ok(Run { width: mask.w as u32, height: mask.h as u32, coverage, x, y })
}
