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

use crate::photocraft::model::{
    Adjustment, Blend, Content, Document, Layer, Node, PictureFit, PictureRef, Shape, ShapeKind, TextRun,
};

/// The composite of a whole document, at canvas size, straight RGBA.
pub fn render(doc: &Document) -> Vec<u8> {
    render_rect(doc, 0, 0, doc.width(), doc.height())
}

/// The composite of one rectangle of the canvas, straight RGBA.
///
/// The rectangle is clamped to the canvas; the result is exactly `width * height * 4` bytes for
/// the clamped size.
pub fn render_rect(doc: &Document, x: u32, y: u32, width: u32, height: u32) -> Vec<u8> {
    let x1 = (x + width).min(doc.width());
    let y1 = (y + height).min(doc.height());
    let x0 = x.min(doc.width());
    let y0 = y.min(doc.height());
    let (w, h) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
    if w == 0 || h == 0 {
        return Vec::new();
    }
    let mut out = vec![0u8; (w as usize) * (h as usize) * 4];

    // Every top-level layer composites straight into the accumulated result. `composite_layer`
    // already blends source-over with the layer's own mode, so a separate scratch buffer would
    // only throw that mode away: the layer would be blended against blank pixels and then laid
    // over the picture as Normal. It also gives an adjustment layer the composite it is meant to
    // adjust, rather than an empty buffer.
    for l in doc.layers() {
        composite_layer(l, doc, x0, y0, w, h, &mut out, 1.0);
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
                let src = ((y0 as usize + row) * doc.width() as usize + x0 as usize) * 4;
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
        Content::Shape(shape) => {
            let mut one = vec![0u8; out.len()];
            draw_shape(&mut one, w, h, shape, x0 as f64, y0 as f64);
            if l.clipped {
                clip_to_alpha(&mut one, out, w as usize, h as usize);
            }
            blend_into(out, &one, w as usize, h as usize, alpha, l.blend);
        }
        Content::Picture(pic) => {
            let mut one = vec![0u8; out.len()];
            draw_picture(&mut one, w, h, pic, x0 as f64, y0 as f64);
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
pub fn adjust_pixels(buf: &mut [u8], a: &Adjustment) {
    apply_adjustment(buf, a, None, 1.0);
}

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

/// Draw a vector shape into a buffer: fill, then stroke, both anti-aliased.
///
/// Shapes are rasterised here rather than held as pixels so a stroke width or a colour can change
/// after the fact without resampling the edge — the whole point of keeping them as geometry. The
/// coverage test uses four sub-samples per pixel in each axis, which is enough for a line to look
/// like a line without the cost of a real scanline filler.
fn draw_shape(one: &mut [u8], w: u32, h: u32, shape: &Shape, ox: f64, oy: f64) {
    let fill = shape.fill.as_deref().and_then(|c| crate::photocraft::model::parse_colour(c).ok().flatten());
    let stroke = shape.stroke.as_deref().and_then(|c| crate::photocraft::model::parse_colour(c).ok().flatten());
    if fill.is_none() && stroke.is_none() {
        return;
    }
    let f = &shape.frame;
    let half = shape.stroke_width.max(0.0) / 2.0;
    // The shape's bounding box in the buffer, grown by the stroke's half width so the outline is
    // not clipped at the frame's own edges.
    let x0 = ((f.x - ox - half - 1.0).floor() as i64).max(0);
    let y0 = ((f.y - oy - half - 1.0).floor() as i64).max(0);
    let x1 = ((f.x + f.width - ox + half + 1.0).ceil() as i64).min(i64::from(w) - 1);
    let y1 = ((f.y + f.height - oy + half + 1.0).ceil() as i64).min(i64::from(h) - 1);
    if x1 < x0 || y1 < y0 {
        return;
    }
    let sub = |v: f64| [0.125, 0.375, 0.625, 0.875].map(|s| v + s);
    for py in y0..=y1 {
        for px in x0..=x1 {
            // Signed distance from the shape's edge in document pixels: negative inside.
            let mut inside = 0u32;
            let mut near = 0u32;
            for sy in sub(py as f64 + oy) {
                for sx in sub(px as f64 + ox) {
                    let d = shape_distance(shape, sx, sy);
                    if d <= 0.0 {
                        inside += 1;
                    }
                    // Inside the stroke's band: the distance to the edge is within half a width.
                    if d.abs() <= half {
                        near += 1;
                    }
                }
            }
            if inside > 0 && fill.is_some() {
                let c = fill.unwrap();
                let a = (inside as f64 / 16.0 * f64::from(c[3]) / 255.0 * 255.0).round() as u32;
                blend_px(one, w, px as usize, py as usize, c[0], c[1], c[2], a as u8);
            }
            if near > 0 && stroke.is_some() && half > 0.0 {
                let c = stroke.unwrap();
                let a = (near as f64 / 16.0 * f64::from(c[3]) / 255.0 * 255.0).round() as u8;
                blend_px(one, w, px as usize, py as usize, c[0], c[1], c[2], a);
            }
        }
    }
}

/// Signed distance from a point to the shape's outline, in document pixels.
fn shape_distance(shape: &Shape, x: f64, y: f64) -> f64 {
    let f = &shape.frame;
    // Rotation is undone about the frame's centre, so the test is always axis-aligned.
    let (dx, dy) = (x - (f.x + f.width / 2.0), y - (f.y + f.height / 2.0));
    let (sx, sy) = if f.rotation.abs() > 1e-6 {
        let r = -f.rotation.to_radians();
        (dx * r.cos() - dy * r.sin(), dx * r.sin() + dy * r.cos())
    } else {
        (dx, dy)
    };
    match shape.kind {
        ShapeKind::Rectangle => {
            let (hw, hh) = (f.width / 2.0, f.height / 2.0);
            let qx = sx.abs() - hw + f.radius;
            let qy = sy.abs() - hh + f.radius;
            let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
            outside + qx.max(qy).min(0.0) - f.radius.max(0.0)
        }
        ShapeKind::Ellipse => {
            // A scaled circle distance: exact enough for an outline of a few pixels, and far
            // cheaper than an iterative solve.
            let (rx, ry) = ((f.width / 2.0).max(1e-6), (f.height / 2.0).max(1e-6));
            let k = ((sx / rx).powi(2) + (sy / ry).powi(2)).sqrt();
            (k - 1.0) * rx.min(ry)
        }
        ShapeKind::Path => path_distance(shape, x, y),
    }
}

/// The distance from a point to the nearest segment of a path, signed by containment.
///
/// A path's outline is its node list joined by straight segments between the nodes' handles.
/// Containment is decided by the even-odd rule over those same segments, so the fill and the
/// stroke agree about where the shape is.
fn path_distance(shape: &Shape, x: f64, y: f64) -> f64 {
    if shape.nodes.len() < 2 {
        return f64::MAX;
    }
    let f = &shape.frame;
    let pt = |n: &Node| (f.x + n.x, f.y + n.y);
    let mut best = f64::MAX;
    let mut inside = false;
    let count = if shape.closed { shape.nodes.len() } else { shape.nodes.len() - 1 };
    for i in 0..count {
        let a = &shape.nodes[i];
        let b = &shape.nodes[(i + 1) % shape.nodes.len()];
        let (ax, ay) = pt(a);
        let (bx, by) = pt(b);
        // Crossing test for even-odd containment, against a ray running to +x.
        if (ay > y) != (by > y) {
            let t = (y - ay) / (by - ay);
            if x < ax + (bx - ax) * t {
                inside = !inside;
            }
        }
        let (vx, vy) = (bx - ax, by - ay);
        let len2 = vx * vx + vy * vy;
        let t = if len2 > 0.0 { (((x - ax) * vx + (y - ay) * vy) / len2).clamp(0.0, 1.0) } else { 0.0 };
        let d = ((x - (ax + vx * t)).powi(2) + (y - (ay + vy * t)).powi(2)).sqrt();
        best = best.min(d);
    }
    if inside {
        -best
    } else {
        best
    }
}

/// Draw a framed picture into a buffer, honouring its fit.
fn draw_picture(one: &mut [u8], w: u32, h: u32, pic: &PictureRef, ox: f64, oy: f64) {
    let f = &pic.frame;
    if f.width <= 0.0 || f.height <= 0.0 || pic.width == 0 || pic.height == 0 {
        return;
    }
    let x0 = ((f.x - ox).floor() as i64).max(0);
    let y0 = ((f.y - oy).floor() as i64).max(0);
    let x1 = ((f.x + f.width - ox).ceil() as i64).min(i64::from(w));
    let y1 = ((f.y + f.height - oy).ceil() as i64).min(i64::from(h));
    // The source rectangle the frame shows, per the fit rule.
    let (sw, sh) = (pic.width as f64, pic.height as f64);
    let (fx, fy) = (f.width, f.height);
    let (src_w, src_h, src_x, src_y) = match pic.fit {
        PictureFit::Stretch => (sw, sh, 0.0, 0.0),
        PictureFit::Fit => {
            // Whole picture visible: the frame shows it centred, letterboxed.
            let k = (fx / sw).min(fy / sh);
            let (vw, vh) = (fx / k, fy / k);
            (vw, vh, (sw - vw) / 2.0, (sh - vh) / 2.0)
        }
        PictureFit::Fill => {
            let k = (fx / sw).max(fy / sh);
            let (vw, vh) = (fx / k, fy / k);
            (vw, vh, (sw - vw) / 2.0, (sh - vh) / 2.0)
        }
    };
    for py in y0..y1 {
        for px in x0..x1 {
            // Where in the picture this pixel lands, in source pixels.
            let u = (px as f64 + 0.5 - f.x + ox) / fx;
            let v = (py as f64 + 0.5 - f.y + oy) / fy;
            let sx = src_x + u * src_w;
            let sy = src_y + v * src_h;
            if sx < 0.0 || sy < 0.0 || sx >= sw || sy >= sh {
                continue;
            }
            let si = ((sy as usize) * pic.width as usize + sx as usize) * 4;
            if si + 3 >= pic.pixels.len() {
                continue;
            }
            blend_px(
                one,
                w,
                px as usize,
                py as usize,
                pic.pixels[si],
                pic.pixels[si + 1],
                pic.pixels[si + 2],
                pic.pixels[si + 3],
            );
        }
    }
}

/// Source-over one pixel into a straight-RGBA buffer.
fn blend_px(buf: &mut [u8], width: u32, x: usize, y: usize, r: u8, g: u8, b: u8, a: u8) {
    let i = (y * width as usize + x) * 4;
    if i + 3 >= buf.len() {
        return;
    }
    let sa = f64::from(a) / 255.0;
    if sa <= 0.0 {
        return;
    }
    let da = f64::from(buf[i + 3]) / 255.0;
    let out_a = sa + da * (1.0 - sa);
    if out_a <= 0.0 {
        return;
    }
    let mix = |s: u8, d: u8| {
        let v = (f64::from(s) / 255.0 * sa + f64::from(d) / 255.0 * da * (1.0 - sa)) / out_a;
        (v.clamp(0.0, 1.0) * 255.0).round() as u8
    };
    buf[i] = mix(r, buf[i]);
    buf[i + 1] = mix(g, buf[i + 1]);
    buf[i + 2] = mix(b, buf[i + 2]);
    buf[i + 3] = (out_a.clamp(0.0, 1.0) * 255.0).round() as u8;
}
