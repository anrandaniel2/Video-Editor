//! Compositing: the layer tree flattened to pixels.
//!
//! The canvas shows the document's composite, so this is the one place that decides what the
//! picture looks like. It honours what the studio's panels expose and nothing more: visibility,
//! opacity, the five blend modes, group clipping, adjustment layers applying to what is beneath
//! them, and text drawn with the app's own layout engine.
//!
//! Blending is done in premultiplied 8-bit sRGB, which is what the layers hold. Adjustment layers
//! work on straight (un-premultiplied) colour and leave alpha alone, so a curves layer under a
//! soft-edged stroke bends its colour without eating its edge.

use crate::photocraft::model::{Adjustment, Blend, Content, Document, Layer, TextRun};

/// The composite of a whole document, at canvas size, straight RGBA.
pub fn render(doc: &Document) -> Vec<u8> {
    render_rect(doc, 0, 0, doc.width, doc.height)
}

/// The composite of one rectangle of the canvas, straight RGBA.
///
/// The rectangle is clamped to the canvas; the result is exactly `width * height * 4` bytes for
/// the clamped size.
pub fn render_rect(doc: &Document, x: u32, y: u32, width: u32, height: u32) -> Vec<u8> {
    let x1 = (x + width).min(doc.width);
    let y1 = (y + height).min(doc.height);
    let x0 = x.min(doc.width);
    let y0 = y.min(doc.height);
    let (w, h) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
    if w == 0 || h == 0 {
        return Vec::new();
    }
    let mut out = vec![0u8; (w as usize) * (h as usize) * 4];
    // A slice of the composite, so the same code renders the viewport and the whole canvas.
    let mut layer = vec![0u8; (w as usize) * (h as usize) * 4];

    for l in &doc.layers {
        // An adjustment changes what is already there, so it has to see the accumulated
        // composite. Compositing it into a fresh buffer and blending that over would give it
        // nothing to adjust — the layer under a curves layer would be untouched, which is the
        // one thing the panel promises will not happen.
        if matches!(l.content, Content::Adjustment(_)) {
            composite_layer(l, doc, x0, y0, w, h, &mut out, 1.0);
            continue;
        }
        layer.fill(0);
        composite_layer(l, doc, x0, y0, w, h, &mut layer, 1.0);
        over(&mut out, &layer, w as usize, h as usize, w as usize);
    }
    out
}

/// Composite one layer (and its children) into `out`, then blend it `over` with `parent_alpha`.
fn composite_layer(
    l: &Layer,
    doc: &Document,
    x0: u32,
    y0: u32,
    w: u32,
    h: u32,
    out: &mut [u8],
    parent_alpha: f64,
) {
    if !l.visible || l.opacity <= 0.0 {
        return;
    }
    let alpha = parent_alpha * l.opacity.clamp(0.0, 1.0);
    match &l.content {
        Content::Group => {
            // Children paint into their own buffer, so the group's opacity and blend apply to the
            // group as one thing rather than to each child separately — which is the difference
            // between a group and a folder.
            let mut group = vec![0u8; out.len()];
            for child in &l.children {
                // Same rule inside a group: an adjustment among the children adjusts what its
                // siblings have already painted, not an empty buffer.
                if matches!(child.content, Content::Adjustment(_)) {
                    composite_layer(child, doc, x0, y0, w, h, &mut group, 1.0);
                    continue;
                }
                let mut one = vec![0u8; out.len()];
                composite_layer(child, doc, x0, y0, w, h, &mut one, 1.0);
                over(&mut group, &one, w as usize, h as usize, w as usize);
            }
            if l.clipped {
                clip_to_alpha(&mut group, out, w as usize, h as usize);
            }
            blend_into(out, &group, w as usize, h as usize, alpha, l.blend);
        }
        Content::Pixels(Some(px)) => {
            let mut one = vec![0u8; out.len()];
            for row in 0..h as usize {
                let src = ((y0 as usize + row) * doc.width as usize + x0 as usize) * 4;
                let dst = row * w as usize * 4;
                let len = w as usize * 4;
                if src + len <= px.len() {
                    one[dst..dst + len].copy_from_slice(&px[src..src + len]);
                }
            }
            if l.clipped {
                clip_to_alpha(&mut one, out, w as usize, h as usize);
            }
            blend_into(out, &one, w as usize, h as usize, alpha, l.blend);
        }
        Content::Pixels(None) => {}
        Content::Adjustment(a) => {
            // Adjustments change what is already there: the colour of `out`, not a new layer on
            // top. A clipped adjustment only touches where the parent has paint.
            let mut mask = None;
            if l.clipped {
                let mut m = vec![0u8; out.len()];
                for i in 0..w as usize * h as usize {
                    m[i * 4 + 3] = out[i * 4 + 3];
                }
                mask = Some(m);
            }
            apply_adjustment(out, a, mask.as_deref(), alpha);
        }
        Content::Text(t) => {
            let mut one = vec![0u8; out.len()];
            draw_text(&mut one, w, h, t, x0 as f64, y0 as f64);
            if l.clipped {
                clip_to_alpha(&mut one, out, w as usize, h as usize);
            }
            blend_into(out, &one, w as usize, h as usize, alpha, l.blend);
        }
    }
    // A group's children have already been composited; nothing else to do here.
}

/// Multiply `src` by the alpha of `under`, so a clipped layer only shows where its parent paints.
fn clip_to_alpha(src: &mut [u8], under: &[u8], w: usize, h: usize) {
    for i in 0..w * h {
        let under_alpha = under[i * 4 + 3] as u32;
        for c in 0..4 {
            src[i * 4 + c] = ((src[i * 4 + c] as u32 * under_alpha) / 255) as u8;
        }
    }
}

/// Blend `src` over `dst`, applying `opacity` and `mode`.
fn blend_into(dst: &mut [u8], src: &[u8], w: usize, h: usize, opacity: f64, mode: Blend) {
    for i in 0..w * h {
        let s = &src[i * 4..i * 4 + 4];
        let sa = (s[3] as f64 / 255.0) * opacity;
        if sa <= 0.0 {
            continue;
        }
        let d = [dst[i * 4], dst[i * 4 + 1], dst[i * 4 + 2], dst[i * 4 + 3]];
        let da = d[3] as f64 / 255.0;
        // The blended colour, from the straight colour of both: a blend mode is defined on colour,
        // and applying it to premultiplied values darkens everything by the alpha.
        let mut mixed = [0.0f64; 3];
        for c in 0..3 {
            let sc = if sa > 0.0 { s[c] as f64 / 255.0 } else { 0.0 };
            let dc = if da > 0.0 { d[c] as f64 / 255.0 } else { 0.0 };
            mixed[c] = blend_channel(mode, dc, sc);
        }
        let out_a = sa + da * (1.0 - sa);
        if out_a <= 0.0 {
            continue;
        }
        for c in 0..3 {
            let dc = if da > 0.0 { d[c] as f64 / 255.0 } else { 0.0 };
            // the standard source-over with a blended source colour
            let out_c = (mixed[c] * sa + dc * da * (1.0 - sa)) / out_a;
            dst[i * 4 + c] = (out_c.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
        dst[i * 4 + 3] = (out_a.clamp(0.0, 1.0) * 255.0).round() as u8;
    }
}

/// One channel of a blend mode, straight 0..1 values.
///
/// The formulas are the ones the standard specifies (W3C compositing), except Soft Light, which
/// uses the simpler Photoshop form the panel's name comes from.
fn blend_channel(mode: Blend, b: f64, s: f64) -> f64 {
    match mode {
        Blend::Normal => s,
        Blend::Multiply => b * s,
        Blend::Screen => b + s - b * s,
        Blend::Overlay => {
            if b <= 0.5 {
                2.0 * b * s
            } else {
                1.0 - 2.0 * (1.0 - b) * (1.0 - s)
            }
        }
        Blend::SoftLight => {
            // Photoshop's soft light: a gentle dodge/burn around mid-grey.
            if s <= 0.5 {
                b - (1.0 - 2.0 * s) * b * (1.0 - b)
            } else {
                let d = if b <= 0.25 { ((16.0 * b - 12.0) * b + 4.0) * b } else { b.sqrt() };
                b + (2.0 * s - 1.0) * (d - b)
            }
        }
    }
}

/// Straight source-over: `src` over `dst`, both straight RGBA.
fn over(dst: &mut [u8], src: &[u8], w: usize, h: usize, _stride: usize) {
    for i in 0..w * h {
        let sa = src[i * 4 + 3] as f64 / 255.0;
        if sa <= 0.0 {
            continue;
        }
        let da = dst[i * 4 + 3] as f64 / 255.0;
        let out_a = sa + da * (1.0 - sa);
        if out_a <= 0.0 {
            continue;
        }
        for c in 0..3 {
            let sc = src[i * 4 + c] as f64 / 255.0;
            let dc = dst[i * 4 + c] as f64 / 255.0;
            let v = (sc * sa + dc * da * (1.0 - sa)) / out_a;
            dst[i * 4 + c] = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
        dst[i * 4 + 3] = (out_a.clamp(0.0, 1.0) * 255.0).round() as u8;
    }
}

/// Apply an adjustment to the colour already in `buf`, leaving alpha alone.
///
/// `mask`, when given, is the clipped parent's alpha: the adjustment fades in where the parent has
/// no paint, which is what "clipped" means for a layer that has no pixels of its own.
fn apply_adjustment(buf: &mut [u8], a: &Adjustment, mask: Option<&[u8]>, strength: f64) {
    match a {
        Adjustment::Curves(c) => {
            if c.is_identity() {
                return;
            }
            // `lut` already composes the RGB curve with the channel's own, so each table is the
            // whole mapping for its channel. Applying both would bend the curve twice.
            let (r, g, b) = (c.lut(1), c.lut(2), c.lut(3));
            apply_lut(buf, mask, strength, |px| {
                [r[px[0] as usize], g[px[1] as usize], b[px[2] as usize]]
            });
        }
        Adjustment::Levels { black, white, gamma } => {
            let span = (white - black).max(1e-6);
            let inv = 1.0 / gamma.clamp(0.01, 10.0);
            let map = |v: u8| -> u8 {
                let t = ((v as f64 / 255.0 - black) / span).clamp(0.0, 1.0);
                ((t.powf(inv) * 255.0).round()).clamp(0.0, 255.0) as u8
            };
            apply_lut(buf, mask, strength, |px| [map(px[0]), map(px[1]), map(px[2])]);
        }
        Adjustment::BrightnessContrast { brightness, contrast } => {
            let br = brightness * 255.0;
            // the usual contrast factor, in -1..=1
            let cf = (1.0 + contrast.clamp(-1.0, 1.0)) / (1.0 - contrast.clamp(-1.0, 0.999));
            let map = |v: u8| -> u8 {
                let x = v as f64 + br;
                (((x - 128.0) * cf + 128.0).round()).clamp(0.0, 255.0) as u8
            };
            apply_lut(buf, mask, strength, |px| [map(px[0]), map(px[1]), map(px[2])]);
        }
    }
}

/// Walk the buffer, replace each RGB triple through `map`, and mix the result by `strength` and
/// the optional mask.
fn apply_lut<F>(buf: &mut [u8], mask: Option<&[u8]>, strength: f64, map: F)
where
    F: Fn([u8; 3]) -> [u8; 3],
{
    if strength <= 0.0 {
        return;
    }
    let pixels = buf.len() / 4;
    for i in 0..pixels {
        let px = [buf[i * 4], buf[i * 4 + 1], buf[i * 4 + 2]];
        let alpha = buf[i * 4 + 3];
        if alpha == 0 {
            continue;
        }
        let m = match mask {
            Some(m) => m[i * 4 + 3] as f64 / 255.0,
            None => 1.0,
        } * strength;
        if m <= 0.0 {
            continue;
        }
        let out = map(px);
        for c in 0..3 {
            let mixed = px[c] as f64 + (out[c] as f64 - px[c] as f64) * m;
            buf[i * 4 + c] = mixed.round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// Draw a text run into a canvas-sized buffer at the given offset.
///
/// The glyphs come from the app's own text layout (the same one the video titles use), rasterised
/// at the run's size; the studio's type tool only needs a run the user can see where they tapped.
fn draw_text(buf: &mut [u8], w: u32, h: u32, t: &TextRun, ox: f64, oy: f64) {
    let Ok(Some(colour)) = crate::photocraft::model::parse_colour(&t.color) else {
        return;
    };
    let Ok(run) = crate::photocraft::text::layout(&t.text, t.size) else {
        return;
    };
    // The rasteriser reports where the ink starts relative to the run's origin (above the
    // baseline), and the run's origin is where the user tapped.
    let gx = (t.x - ox) as i32 + run.x;
    let gy = (t.y - oy) as i32 + run.y;
    for (row, span) in run.coverage.chunks(run.width.max(1) as usize).enumerate() {
        for (col, &a) in span.iter().enumerate() {
            if a == 0 {
                continue;
            }
            let (px, py) = (gx + col as i32, gy + row as i32);
            if px < 0 || py < 0 || px >= w as i32 || py >= h as i32 {
                continue;
            }
            let i = ((py as usize) * w as usize + px as usize) * 4;
            let src_a = (a as f64 / 255.0) * (colour[3] as f64 / 255.0);
            if src_a <= 0.0 {
                continue;
            }
            let da = buf[i + 3] as f64 / 255.0;
            let out_a = src_a + da * (1.0 - src_a);
            if out_a <= 0.0 {
                continue;
            }
            for c in 0..3 {
                let sc = colour[c] as f64 / 255.0;
                let dc = buf[i + c] as f64 / 255.0;
                let v = (sc * src_a + dc * da * (1.0 - src_a)) / out_a;
                buf[i + c] = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
            buf[i + 3] = (out_a.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
}
