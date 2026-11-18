//! The painting tools: strokes, fills, gradients, the clone stamp and spot healing.
//!
//! Every tool writes into the active layer's pixels and respects the selection mask, which is what
//! makes a selection feel like a selection: paint inside it lands, paint outside it does not.
//!
//! A stroke is one command for a whole drag (`paint.stroke`), not one per motion event, so the
//! History panel shows one step per stroke — which is what the user made.

use serde_json::Value;

use crate::photocraft::compose;
use crate::photocraft::model::{Content, Kind, Layer, hex, parse_colour};
use crate::photocraft::Session;

/// A painting failure, as the UI shows it.
pub type Fail = String;

/// The layer a tool may paint on, or why it may not.
fn target(session: &mut Session) -> Result<&mut Layer, Fail> {
    let doc = session.active_mut().ok_or("Open a picture first.")?;
    let id = doc.active_layer.ok_or("Select a layer first.")?;
    let layer = doc.doc.find_mut(id).ok_or("That layer is gone.")?;
    if layer.locked {
        return Err(format!("`{}` is locked.", layer.name));
    }
    if !matches!(layer.kind, Kind::Pixels) {
        return Err("This layer has no pixels of its own to paint on.".into());
    }
    Ok(layer)
}

/// The colour a paint command uses: its own `color`, else the session's foreground.
fn colour(params: &Value, session: &Session) -> Result<[u8; 4], Fail> {
    match params.get("color").and_then(Value::as_str) {
        Some(text) => parse_colour(text)?.ok_or_else(|| "That colour is empty.".to_string()),
        None => Ok(session.tools.foreground),
    }
}

/// `paint.stroke` / `paint.pencil`: one drag as one stroke.
///
/// The points are document pixels. The stroke is stamped along the path with the session's brush
/// size and hardness, at the command's opacity, so the panels' size slider means what it says.
pub fn stroke(session: &mut Session, params: &Value) -> Result<Value, Fail> {
    let points = read_points(params)?;
    if points.is_empty() {
        return Err("A stroke needs at least one point.".into());
    }
    let colour = colour(params, session)?;
    let opacity = params
        .get("opacity")
        .and_then(Value::as_f64)
        .map(|v| if v > 1.0 { v / 100.0 } else { v })
        .unwrap_or(session.tools.brush_opacity)
        .clamp(0.0, 1.0);
    let hardness = params
        .get("hardness")
        .and_then(Value::as_f64)
        .unwrap_or(session.tools.brush_hardness)
        .clamp(0.0, 1.0);
    let size = params
        .get("size")
        .and_then(Value::as_f64)
        .unwrap_or(session.tools.brush_size)
        .clamp(0.5, 5000.0);
    // A pencil is a hard, aliased tip; the brush falls off as its hardness says. The command id
    // is what decides, because that is what the tool rail selected — and the pencil command sets
    // this flag, so `paint.pencil` is not simply the brush under another name.
    let straight = params.get("pencil").and_then(Value::as_bool).unwrap_or(false);
    // The Eraser is the brush with `erase`, and rubbing a layer out is painting it transparent.
    let erasing = params.get("erase").and_then(Value::as_bool).unwrap_or(false);
    // The preset's grain: how much the tip's edge is broken up. Without it charcoal and ink paint
    // the same smooth disc, which is what made the panel's eight names look like one brush.
    let grain = params
        .get("grain")
        .and_then(Value::as_f64)
        .unwrap_or(session.tools.brush_grain)
        .clamp(0.0, 1.0);
    let colour = if erasing { [0, 0, 0, 0] } else { colour };
    // The panel sends a `coalesce` name for the gesture: a drag that arrives as several calls is
    // one history row, not one per call.
    let coalesce = params.get("coalesce").and_then(Value::as_str);

    let (width, height) = {
        let doc = session.active_mut().ok_or("Open a picture first.")?;
        // The snapshot is the state to return to, so it is taken before the edit.
        let label = if erasing { "Eraser" } else if straight { "Pencil" } else { "Brush" };
        doc.push_history_coalescing(label, coalesce);
        (doc.doc.width(), doc.doc.height())
    };
    let selection = session.active().and_then(|d| d.doc.selection().clone());
    // The brush the stroke was made with: its tip, its grain, its spacing, and the randomness that
    // stops a hand-drawn line looking stamped. Read before the layer is borrowed mutably, and once
    // for the whole stroke, because every stamp of one stroke shares it.
    let mut brush = session.tools.brush;
    // A command may override the tip for this stroke only: the sliders send hardness and grain,
    // and a stroke that ignored them would be a slider that does nothing.
    brush.hardness = hardness;
    brush.grain = grain;
    if straight {
        brush.tip = super::brush::Tip::Round;
        brush.roundness = 1.0;
    }
    let layer = target(session)?;
    let Content::Pixels(pixels) = &mut layer.content else { unreachable!() };
    // `make_mut` is what keeps the undo snapshot cheap: the clone happened when the snapshot was
    // taken, this is the copy that the edit actually needs.
    let px = std::sync::Arc::make_mut(
        pixels.get_or_insert_with(|| std::sync::Arc::new(vec![0u8; (width as usize) * (height as usize) * 4])),
    );

    let radius = (size / 2.0).max(0.25);
    let mut stroke = Stroke {
        // A struct literal does not coerce, so the slice and the mask are taken explicitly.
        pixels: px.as_mut_slice(),
        width,
        height,
        radius,
        opacity,
        colour,
        straight,
        erasing,
        brush,
        selection: selection.as_deref(),
        // A fixed seed per stroke: the same drag paints the same way twice, so undo brings back
        // what was replaced rather than a differently speckled line.
        seed: (colour[0] as u64) << 24
            | (colour[1] as u64) << 16
            | (colour[2] as u64) << 8
            | points.len() as u64,
        stamps: 0,
        // Set below, once the path's length is known: the taper is measured against it.
        total: 0.0,
        step: 1.0,
        // A stroke that keeps its strongest stamp needs somewhere to keep it. A building stroke
        // does not, and allocating a page for it would cost every brush the memory of one.
        strongest: if brush.rendering == super::brush::Rendering::Strongest {
            Some(vec![0u8; (width as usize) * (height as usize)])
        } else {
            None
        },
    };
    // A taper needs to know where the stroke ends before it starts, so the length comes first.
    let length: f64 = points
        .windows(2)
        .map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt())
        .sum();
    stroke.total = length;
    stroke.step = (radius * 2.0 * stroke.brush.spacing).max(0.4);
    // Procraft's StreamLine: the hand's path is pulled towards a smooth curve before it is
    // stamped. Without it a stroke carries every tremor of the finger, which is most of what
    // makes a drawn line look amateur next to the reference's.
    let path = streamline(&points, stroke.brush.streamline);
    for pair in path.windows(2) {
        stroke.segment(pair[0], pair[1]);
    }
    if path.len() == 1 {
        stroke.segment(path[0], path[0]);
    }
    let stamps = stroke.stamps;
    session.active_mut().unwrap().touch();
    Ok(serde_json::json!({
        "points": points.len(),
        "brush": session.tools.brush_preset,
        "stamps": stamps,
    }))
}

/// Pull a hand's path towards a smooth curve.
///
/// Procraft calls this StreamLine. Each point is pulled a proportion of the way towards the
/// midpoint of its neighbours, which removes the tremor from a drag while leaving its overall
/// shape -- a corner drawn deliberately survives, a shaky straight line becomes straight. At 0 the
/// path is returned untouched, so a brush that does not ask for it costs nothing.
fn streamline(points: &[(f64, f64)], amount: f64) -> Vec<(f64, f64)> {
    if amount <= 0.0 || points.len() < 3 {
        return points.to_vec();
    }
    // Three passes rather than one: one pass softens a corner, three make a wobble into a curve.
    // The pull is capped well below 1 so the path cannot collapse to a straight line.
    let pull = amount.clamp(0.0, 1.0) * 0.45;
    let mut path = points.to_vec();
    for _ in 0..3 {
        let mut next = path.clone();
        for i in 1..path.len() - 1 {
            let mx = (path[i - 1].0 + path[i + 1].0) / 2.0;
            let my = (path[i - 1].1 + path[i + 1].1) / 2.0;
            next[i] = (
                path[i].0 + (mx - path[i].0) * pull,
                path[i].1 + (my - path[i].1) * pull,
            );
        }
        path = next;
    }
    // The ends stay where the artist put them, which is what a taper and a joined stroke rely on.
    path[0] = points[0];
    let last = path.len() - 1;
    path[last] = points[points.len() - 1];
    path
}

/// One stroke being painted: everything a stamp needs, and the running position along the path
/// that the taper is measured against.
struct Stroke<'a> {
    pixels: &'a mut [u8],
    width: u32,
    height: u32,
    radius: f64,
    opacity: f64,
    colour: [u8; 4],
    straight: bool,
    erasing: bool,
    brush: super::brush::Brush,
    selection: Option<&'a [u8]>,
    seed: u64,
    /// How many stamps have been laid so far, and the stroke's whole length: together they place
    /// the taper.
    stamps: u64,
    total: f64,
    step: f64,
    /// For a "strongest stamp wins" stroke: what the strongest stamp has laid at each pixel so
    /// far, and the colour it laid. Overlapping discs then cannot darken one another, which is
    /// what a Light Glaze stroke is.
    strongest: Option<Vec<u8>>,
}

impl Stroke<'_> {
    /// Stamp along one segment of the hand's path.
    fn segment(&mut self, a: (f64, f64), b: (f64, f64)) {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let length = (dx * dx + dy * dy).sqrt();
        let steps = ((length / self.step).ceil() as usize).max(1);
        // The direction this segment runs in: a chisel or a flat brush turns with the hand, which
        // is what makes the width of the line change as it curves.
        let angle = dy.atan2(dx);
        for i in 0..=steps {
            let t = if steps == 0 { 0.0 } else { i as f64 / steps as f64 };
            self.stamp_at(a.0 + dx * t, a.1 + dy * t, angle);
        }
    }

    /// One step of the path: `count` stamps, each with the brush's own rotation and jitter, laid at
    /// the flow the brush and the opacity slider give.
    fn stamp_at(&mut self, cx: f64, cy: f64, direction: f64) {
        let count = self.brush.count.max(1);
        for i in 0..count {
            // Deterministic pseudo-randomness: a hash of the stamp's index and the stroke's seed.
            let mut h = self.seed ^ self.stamps.wrapping_mul(0x9E37_79B9_7F4A_7C15);
            h ^= (i as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
            h ^= h >> 31;
            h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
            h ^= h >> 29;
            let rnd = |bits: u32| ((h >> bits) & 0xFFFF) as f64 / 65535.0;

            // The stamp's size: jitter reduces it, and the taper shrinks it at each end of the
            // stroke, which is what makes a chalk line start and finish thin.
            let mut radius = self.radius;
            if self.brush.jitter > 0.0 {
                radius *= 1.0 - self.brush.jitter * 0.7 * rnd(0);
            }
            radius *= self.taper_scale();
            if radius < 0.25 {
                continue;
            }

            // The stamp's rotation: the brush's own, plus the stroke's direction if it follows the
            // hand, plus the scatter.
            let mut rotation = self.brush.rotation + self.brush.follow * direction;
            if self.brush.scatter > 0.0 {
                rotation += (rnd(16) * 2.0 - 1.0) * self.brush.scatter * std::f64::consts::PI;
            }

            // A scattered stamp is thrown off the path a little, in stamp diameters.
            let (mut sx, mut sy) = (cx, cy);
            if self.brush.scatter > 0.0 && count > 1 {
                let offset = self.brush.scatter * 0.25 * radius;
                sx += (rnd(32) * 2.0 - 1.0) * offset;
                sy += (rnd(48) * 2.0 - 1.0) * offset;
            }

            // Flow: what one stamp lays down. The opacity slider scales the whole stroke.
            let mut alpha = self.opacity * self.brush.flow.min(1.0);
            if self.brush.jitter > 0.0 {
                alpha *= 1.0 - self.brush.jitter * 0.5 * rnd(8);
            }
            alpha *= self.taper_scale();

            self.stamp_one(sx, sy, radius, rotation, alpha);
            self.stamps += 1;
        }
    }

    /// The taper's size at the current stamp: 1 in the middle of the stroke, less at each end.
    fn taper_scale(&self) -> f64 {
        let taper = self.brush.taper;
        if taper <= 0.0 || self.total <= 0.0 {
            return 1.0;
        }
        let along = (self.stamps as f64 * self.step) / self.total;
        // Distance from the nearest end, in fractions of the stroke.
        let from_end = along.min(1.0 - along).min(0.5);
        // A linear ramp over `taper` of the stroke, floored so the tip still paints something.
        (from_end / taper.max(1e-6)).clamp(0.15, 1.0)
    }

    /// One stamp: the tip's silhouette, the grain that breaks it up, and the blend.
    fn stamp_one(&mut self, cx: f64, cy: f64, radius: f64, rotation: f64, opacity: f64) {
        use super::brush::{GrainMode, Tip};
        let width = self.width;
        let height = self.height;
        let x0 = ((cx - radius).floor() as i64).max(0);
        let y0 = ((cy - radius).floor() as i64).max(0);
        let x1 = ((cx + radius).ceil() as i64).min(i64::from(width) - 1);
        let y1 = ((cy + radius).ceil() as i64).min(i64::from(height) - 1);
        if x1 < x0 || y1 < y0 {
            return;
        }
        // The stamp's own axes: roundness squashes it across the rotation.
        let (sin, cos) = rotation.sin_cos();
        let roundness = self.brush.roundness.clamp(0.05, 1.0);
        let hardness = self.brush.hardness;
        let grain = self.brush.grain;
        let grain_scale = self.brush.grain_scale;
        let grain_mode = self.brush.grain_mode;
        let tip = self.brush.tip;
        let straight = self.straight;
        let colour = self.colour;
        let erasing = self.erasing;
        let seed = self.seed;

        for y in y0..=y1 {
            for x in x0..=x1 {
                if let Some(mask) = self.selection {
                    if mask[y as usize * width as usize + x as usize] == 0 {
                        continue;
                    }
                }
                // Into the stamp's own frame, where it is a circle of radius one.
                let dx = x as f64 + 0.5 - cx;
                let dy = y as f64 + 0.5 - cy;
                let u = (dx * cos + dy * sin) / radius;
                let v = (-dx * sin + dy * cos) / (radius * roundness);
                let d = (u * u + v * v).sqrt();
                if d >= 1.0 {
                    continue;
                }
                // The tip's silhouette: a disc eased by hardness, except where the tip is not a
                // disc. `straight` is the pencil's hard, aliased edge.
                let inner = hardness.clamp(0.0, 1.0);
                let mut edge = if straight || d <= inner {
                    1.0
                } else {
                    1.0 - (d - inner) / (1.0 - inner).max(1e-6)
                };
                match tip {
                    // A ragged rim is eaten into by the grain: the tooth shows at the silhouette
                    // rather than over the whole stamp.
                    Tip::Ragged => {
                        if grain > 0.0 {
                            let n = hash01(x as u32, y as u32, seed);
                            edge *= (1.0 - grain * d * d * (1.0 - n) * 1.6).clamp(0.0, 1.0);
                        }
                    }
                    // A cloud is three lobes: soft all over, with no hard edge anywhere, which is
                    // what an airbrush and a chalk have in common.
                    Tip::Cloud => {
                        let mut best: f64 = 0.0;
                        for (ox, oy) in [(0.0, 0.32), (-0.28, -0.2), (0.28, -0.2)] {
                            let lu = u - ox;
                            let lv = v - oy;
                            let ld = (lu * lu + lv * lv).sqrt() / 0.72;
                            let falloff = if ld <= inner {
                                1.0
                            } else {
                                1.0 - (ld - inner) / (1.0 - inner).max(1e-6)
                            };
                            best = best.max(falloff.clamp(0.0, 1.0));
                        }
                        edge *= best;
                    }
                    _ => {}
                }
                if edge <= 0.0 {
                    continue;
                }
                // Grain inside the stamp: travelling with it (stamp mode) or fixed to the canvas.
                if grain > 0.0 && !matches!(tip, Tip::Ragged) {
                    let (gx, gy) = match grain_mode {
                        GrainMode::Stamp => (
                            (((u * 0.5 + 0.5) * 255.0) * grain_scale.max(0.05)).round() as u32,
                            (((v * 0.5 + 0.5) * 255.0) * grain_scale.max(0.05)).round() as u32,
                        ),
                        GrainMode::Canvas => (x as u32, y as u32),
                    };
                    let n = hash01(gx, gy, seed);
                    // Grain shows most where the stamp is thin, and only as deep as its setting.
                    edge *= (1.0 - grain * (1.0 - d * 0.6) * (1.0 - n)).clamp(0.0, 1.0);
                }
                if edge <= 0.0 {
                    continue;
                }

                let i = ((y as usize) * width as usize + x as usize) * 4;
                if erasing {
                    let da = self.pixels[i + 3] as f64 / 255.0;
                    let out_a = (da * (1.0 - edge * opacity)).clamp(0.0, 1.0);
                    self.pixels[i + 3] = (out_a * 255.0).round() as u8;
                    if out_a <= 0.0 {
                        self.pixels[i] = 0;
                        self.pixels[i + 1] = 0;
                        self.pixels[i + 2] = 0;
                    }
                    continue;
                }
                let mut sa = edge * opacity * (colour[3] as f64 / 255.0);
                if sa <= 0.0 {
                    continue;
                }
                // A stroke that keeps its strongest stamp: the pixel is set to this stamp's
                // strength rather than composited over what is there. That is what a glaze is --
                // overlapping discs of one stroke do not darken one another -- and it is why a
                // hard brush's line is an even ribbon instead of a chain of darker blobs.
                if let Some(seen) = self.strongest.as_mut() {
                    let here = (sa * 255.0).round().clamp(0.0, 255.0) as u8;
                    if here > seen[i / 4] {
                        seen[i / 4] = here;
                        self.pixels[i] = colour[0];
                        self.pixels[i + 1] = colour[1];
                        self.pixels[i + 2] = colour[2];
                        self.pixels[i + 3] = here;
                    }
                    continue;
                }
                let da = self.pixels[i + 3] as f64 / 255.0;
                // A gradual build-up: what one stamp adds shrinks the closer the pixel already is
                // to the stroke's opacity, so the stroke arrives smoothly instead of in steps.
                if self.brush.rendering == super::brush::Rendering::Gradual {
                    let left = (1.0 - da).max(0.0);
                    sa *= left * left;
                    if sa <= 0.0 {
                        continue;
                    }
                }
                let out_a = sa + da * (1.0 - sa);
                if out_a <= 0.0 {
                    continue;
                }
                for c in 0..3 {
                    let sc = colour[c] as f64 / 255.0;
                    let dc = self.pixels[i + c] as f64 / 255.0;
                    let v = (sc * sa + dc * da * (1.0 - sa)) / out_a;
                    self.pixels[i + c] = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                }
                self.pixels[i + 3] = (out_a.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }
    }
}

/// A deterministic value in 0..1 for a position and a seed.
fn hash01(x: u32, y: u32, seed: u64) -> f64 {
    let mut h = seed ^ (x as u64).wrapping_mul(0x9E37_79B1) ^ (y as u64).wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 13;
    h = h.wrapping_mul(0x27D4_EB2F_1656_67C5);
    h ^= h >> 16;
    (h & 0xFFFF) as f64 / 65535.0
}

/// `paint.bucket`: flood fill from a point, bounded by `tolerance` (0..=255).
///
/// The fill reads the **composite**, so what you fill is what you see — the usual expectation,
/// and the only one that works on a layer whose pixels start empty. It writes into the active
/// layer, and it spreads only where the selection allows.
pub fn bucket(session: &mut Session, params: &Value) -> Result<Value, Fail> {
    let x = number(params, "x")? as i64;
    let y = number(params, "y")? as i64;
    let tolerance = params.get("tolerance").and_then(Value::as_f64).unwrap_or(32.0).clamp(0.0, 255.0);
    let colour = colour(params, session)?;

    let doc = session.active().ok_or("Open a picture first.")?;
    let (width, height) = (doc.doc.width(), doc.doc.height());
    if x < 0 || y < 0 || x >= i64::from(width) || y >= i64::from(height) {
        return Err("That point is outside the canvas.".into());
    }
    let composite = compose::render(&doc.doc);
    let selection = doc.doc.selection().clone();
    // A seed outside the selection has nothing it may fill.
    if let Some(mask) = &selection {
        if mask[(y as usize) * width as usize + x as usize] == 0 {
            return Err("That point is outside the selection.".into());
        }
    }
    let seed = pixel_at(&composite, width, x as usize, y as usize);
    let filled = flood(&composite, &selection, width, height, x as usize, y as usize, seed, tolerance);

    {
        let doc = session.active_mut().ok_or("Open a picture first.")?;
        doc.push_history_coalescing("Fill", params.get("coalesce").and_then(Value::as_str));
    }
    let layer = target(session)?;
    let Content::Pixels(pixels) = &mut layer.content else { unreachable!() };
    // `make_mut` is what keeps the undo snapshot cheap: the clone happened when the snapshot was
    // taken, this is the copy that the edit actually needs.
    let px = std::sync::Arc::make_mut(
        pixels.get_or_insert_with(|| std::sync::Arc::new(vec![0u8; (width as usize) * (height as usize) * 4])),
    );
    let mut count = 0usize;
    for (i, inside) in filled.iter().enumerate() {
        if !inside {
            continue;
        }
        let o = i * 4;
        let sa = colour[3] as f64 / 255.0;
        let da = px[o + 3] as f64 / 255.0;
        let out_a = sa + da * (1.0 - sa);
        if out_a <= 0.0 {
            continue;
        }
        for c in 0..3 {
            let sc = colour[c] as f64 / 255.0;
            let dc = px[o + c] as f64 / 255.0;
            px[o + c] = (((sc * sa + dc * da * (1.0 - sa)) / out_a).clamp(0.0, 1.0) * 255.0).round() as u8;
        }
        px[o + 3] = (out_a.clamp(0.0, 1.0) * 255.0).round() as u8;
        count += 1;
    }
    session.active_mut().unwrap().touch();
    Ok(serde_json::json!({ "filled": count }))
}

/// The flood-fill region: one `bool` per pixel, true where the fill reached.
///
/// Scanned, not recursive: a 16 MP canvas would blow the stack as a recursive fill, and a
/// column-wise scanline fill is both faster and bounded.
fn flood(
    composite: &[u8],
    selection: &Option<Vec<u8>>,
    width: u32,
    height: u32,
    sx: usize,
    sy: usize,
    seed: [u8; 4],
    tolerance: f64,
) -> Vec<bool> {
    let w = width as usize;
    let h = height as usize;
    let mut done = vec![false; w * h];
    let mut stack = vec![(sx, sy)];
    let matches = |p: [u8; 4]| -> bool {
        let d = |a: u8, b: u8| (f64::from(a) - f64::from(b)).abs();
        d(p[0], seed[0]).max(d(p[1], seed[1])).max(d(p[2], seed[2])).max(d(p[3], seed[3])) <= tolerance
    };
    let allowed = |i: usize| selection.as_ref().is_none_or(|m| m[i] > 0);
    while let Some((x, y)) = stack.pop() {
        if x >= w || y >= h {
            continue;
        }
        let i = y * w + x;
        if done[i] || !allowed(i) || !matches(pixel_at(composite, width, x, y)) {
            continue;
        }
        // run to the left and right along this row
        let mut left = x;
        while left > 0 {
            let j = y * w + (left - 1);
            if done[j] || !allowed(j) || !matches(pixel_at(composite, width, left - 1, y)) {
                break;
            }
            left -= 1;
        }
        let mut right = x;
        while right + 1 < w {
            let j = y * w + (right + 1);
            if done[j] || !allowed(j) || !matches(pixel_at(composite, width, right + 1, y)) {
                break;
            }
            right += 1;
        }
        for cx in left..=right {
            let j = y * w + cx;
            done[j] = true;
            for ny in [y.wrapping_sub(1), y + 1] {
                if ny < h {
                    let nj = ny * w + cx;
                    if !done[nj] && allowed(nj) && matches(pixel_at(composite, width, cx, ny)) {
                        stack.push((cx, ny));
                    }
                }
            }
        }
    }
    done
}

/// `paint.gradient`: a two-stop linear gradient from `x`,`y` to `x2`,`y2`.
pub fn gradient(session: &mut Session, params: &Value) -> Result<Value, Fail> {
    let (x0, y0) = (number(params, "x")?, number(params, "y")?);
    let x1 = params.get("x2").and_then(Value::as_f64).unwrap_or(x0);
    let y1 = params.get("y2").and_then(Value::as_f64).unwrap_or(y0);
    let start = match params.get("color").and_then(Value::as_str) {
        Some(c) => parse_colour(c)?.ok_or("The gradient needs a colour.")?,
        None => session.tools.foreground,
    };
    let end = match params.get("color2").and_then(Value::as_str) {
        Some(c) => parse_colour(c)?.ok_or("The gradient needs a second colour.")?,
        None => session.tools.background,
    };
    let (width, height) = {
        let doc = session.active().ok_or("Open a picture first.")?;
        (doc.doc.width(), doc.doc.height())
    };
    let selection = session.active().unwrap().doc.selection().clone();
    let dx = x1 - x0;
    let dy = y1 - y0;
    let len2 = dx * dx + dy * dy;
    if len2 <= 1e-9 {
        return Err("A gradient needs two different points.".into());
    }
    {
        let doc = session.active_mut().ok_or("Open a picture first.")?;
        doc.push_history_coalescing("Gradient", params.get("coalesce").and_then(Value::as_str));
    }
    let layer = target(session)?;
    let Content::Pixels(pixels) = &mut layer.content else { unreachable!() };
    // `make_mut` is what keeps the undo snapshot cheap: the clone happened when the snapshot was
    // taken, this is the copy that the edit actually needs.
    let px = std::sync::Arc::make_mut(
        pixels.get_or_insert_with(|| std::sync::Arc::new(vec![0u8; (width as usize) * (height as usize) * 4])),
    );
    for y in 0..height {
        for x in 0..width {
            let i = (y as usize * width as usize + x as usize) * 4;
            if let Some(mask) = &selection {
                if mask[y as usize * width as usize + x as usize] == 0 {
                    continue;
                }
            }
            let t = (((x as f64 + 0.5 - x0) * dx + (y as f64 + 0.5 - y0) * dy) / len2).clamp(0.0, 1.0);
            let mut c = [0u8; 4];
            for k in 0..4 {
                c[k] = (f64::from(start[k]) + t * (f64::from(end[k]) - f64::from(start[k]))).round() as u8;
            }
            let sa = c[3] as f64 / 255.0;
            let da = px[i + 3] as f64 / 255.0;
            let out_a = sa + da * (1.0 - sa);
            if out_a <= 0.0 {
                continue;
            }
            for k in 0..3 {
                let sc = c[k] as f64 / 255.0;
                let dc = px[i + k] as f64 / 255.0;
                px[i + k] = (((sc * sa + dc * da * (1.0 - sa)) / out_a).clamp(0.0, 1.0) * 255.0).round() as u8;
            }
            px[i + 3] = (out_a.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
    session.active_mut().unwrap().touch();
    Ok(serde_json::json!({ "gradient": true }))
}

/// `paint.cloneStamp`: copy pixels from a source point to the stroke.
///
/// The source is where the user alt-tapped, as `source: [x, y]` (the studio's shape) or
/// `sourceX`/`sourceY`. The offset from the first stroke point is held for the whole stroke, which
/// is how a clone stamp is expected to behave.
pub fn clone_stamp(session: &mut Session, params: &Value) -> Result<Value, Fail> {
    let points = read_points(params)?;
    if points.is_empty() {
        return Err("The stamp needs at least one point.".into());
    }
    // The studio sends the source as a two-element array; the named pair is kept because a
    // command is easier to drive from a test or a script that way.
    let (sx, sy) = match params.get("source").and_then(Value::as_array) {
        Some(pair) if pair.len() == 2 => {
            let get = |i: usize| {
                pair[i].as_f64().ok_or_else(|| "`source` wants two numbers.".to_string())
            };
            (get(0)?, get(1)?)
        }
        _ => (number(params, "sourceX")?, number(params, "sourceY")?),
    };
    let size = params.get("size").and_then(Value::as_f64).unwrap_or(session.tools.brush_size).clamp(1.0, 5000.0);
    let opacity = params
        .get("opacity")
        .and_then(Value::as_f64)
        .map(|v| if v > 1.0 { v / 100.0 } else { v })
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    let (width, height) = {
        let doc = session.active().ok_or("Open a picture first.")?;
        (doc.doc.width(), doc.doc.height())
    };
    // The offset from the stroke's first point to the source stays fixed for the stroke.
    let dx = sx - points[0].0;
    let dy = sy - points[0].1;
    let selection = session.active().unwrap().doc.selection().clone();
    let source = compose::render(&session.active().unwrap().doc);

    {
        let doc = session.active_mut().ok_or("Open a picture first.")?;
        doc.push_history_coalescing("Clone stamp", params.get("coalesce").and_then(Value::as_str));
    }
    let layer = target(session)?;
    let Content::Pixels(pixels) = &mut layer.content else { unreachable!() };
    // `make_mut` is what keeps the undo snapshot cheap: the clone happened when the snapshot was
    // taken, this is the copy that the edit actually needs.
    let px = std::sync::Arc::make_mut(
        pixels.get_or_insert_with(|| std::sync::Arc::new(vec![0u8; (width as usize) * (height as usize) * 4])),
    );
    let radius = (size / 2.0).max(0.5);
    for p in &points {
        stamp_from(px, &source, width, height, *p, (p.0 + dx, p.1 + dy), radius, opacity, selection.as_deref());
    }
    session.active_mut().unwrap().touch();
    Ok(serde_json::json!({ "points": points.len() }))
}

/// Stamp from a source point onto a destination point, copying the composite's colour.
#[allow(clippy::too_many_arguments)]
fn stamp_from(
    pixels: &mut [u8],
    source: &[u8],
    width: u32,
    height: u32,
    to: (f64, f64),
    from: (f64, f64),
    radius: f64,
    opacity: f64,
    selection: Option<&[u8]>,
) {
    let x0 = ((to.0 - radius).floor() as i64).max(0);
    let y0 = ((to.1 - radius).floor() as i64).max(0);
    let x1 = ((to.0 + radius).ceil() as i64).min(i64::from(width) - 1);
    let y1 = ((to.1 + radius).ceil() as i64).min(i64::from(height) - 1);
    for y in y0..=y1 {
        for x in x0..=x1 {
            if let Some(mask) = selection {
                if mask[y as usize * width as usize + x as usize] == 0 {
                    continue;
                }
            }
            let d = ((x as f64 + 0.5 - to.0).powi(2) + (y as f64 + 0.5 - to.1).powi(2)).sqrt();
            if d > radius {
                continue;
            }
            let edge = (1.0 - (d / radius).powi(4)).clamp(0.0, 1.0);
            let src_x = (from.0 + (x as f64 + 0.5 - to.0)) as i64;
            let src_y = (from.1 + (y as f64 + 0.5 - to.1)) as i64;
            if src_x < 0 || src_y < 0 || src_x >= i64::from(width) || src_y >= i64::from(height) {
                continue;
            }
            let si = (src_y as usize * width as usize + src_x as usize) * 4;
            let di = (y as usize * width as usize + x as usize) * 4;
            let sa = edge * opacity * (source[si + 3] as f64 / 255.0);
            if sa <= 0.0 {
                continue;
            }
            let da = pixels[di + 3] as f64 / 255.0;
            let out_a = sa + da * (1.0 - sa);
            for c in 0..3 {
                let sc = source[si + c] as f64 / 255.0;
                let dc = pixels[di + c] as f64 / 255.0;
                pixels[di + c] = (((sc * sa + dc * da * (1.0 - sa)) / out_a).clamp(0.0, 1.0) * 255.0).round() as u8;
            }
            pixels[di + 3] = (out_a.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
}

/// `paint.spotHealing`: blend a patch of the composite with its surroundings.
///
/// A true healing brush reconstructs texture; this is the honest smaller thing the studio's button
/// promises — it lightly blurs what it covers, which removes small blemishes without moving what is
/// around them. It is not a clone stamp.
pub fn spot_healing(session: &mut Session, params: &Value) -> Result<Value, Fail> {
    let points = read_points(params)?;
    if points.is_empty() {
        return Err("The healing brush needs at least one point.".into());
    }
    let radius = (params.get("size").and_then(Value::as_f64).unwrap_or(session.tools.brush_size) / 2.0)
        .max(1.0)
        .clamp(1.0, 500.0);
    let (width, height) = {
        let doc = session.active().ok_or("Open a picture first.")?;
        (doc.doc.width(), doc.doc.height())
    };
    let selection = session.active().unwrap().doc.selection().clone();
    let before = compose::render(&session.active().unwrap().doc);
    // A small box blur of the whole composite, then mixed in under the brush.
    let blurred = box_blur(&before, width, height, (radius / 2.0).max(1.0) as usize);
    {
        let doc = session.active_mut().ok_or("Open a picture first.")?;
        doc.push_history_coalescing("Spot healing", params.get("coalesce").and_then(Value::as_str));
    }
    let layer = target(session)?;
    let Content::Pixels(pixels) = &mut layer.content else { unreachable!() };
    // `make_mut` is what keeps the undo snapshot cheap: the clone happened when the snapshot was
    // taken, this is the copy that the edit actually needs.
    let px = std::sync::Arc::make_mut(
        pixels.get_or_insert_with(|| std::sync::Arc::new(vec![0u8; (width as usize) * (height as usize) * 4])),
    );
    for p in &points {
        for y in 0..height {
            for x in 0..width {
                if let Some(mask) = &selection {
                    if mask[y as usize * width as usize + x as usize] == 0 {
                        continue;
                    }
                }
                let d = ((x as f64 + 0.5 - p.0).powi(2) + (y as f64 + 0.5 - p.1).powi(2)).sqrt();
                if d > radius {
                    continue;
                }
                let edge = (1.0 - (d / radius).powi(2)).clamp(0.0, 1.0);
                let i = (y as usize * width as usize + x as usize) * 4;
                let sa = edge;
                if sa <= 0.0 {
                    continue;
                }
                let da = px[i + 3] as f64 / 255.0;
                let out_a = sa + da * (1.0 - sa);
                for c in 0..3 {
                    let sc = blurred[i + c] as f64 / 255.0;
                    let dc = px[i + c] as f64 / 255.0;
                    px[i + c] = (((sc * sa + dc * da * (1.0 - sa)) / out_a).clamp(0.0, 1.0) * 255.0).round() as u8;
                }
                // Healing must not punch a hole in the layer: it takes the blurred colour, and
                // keeps the layer's own alpha where it already has paint.
                px[i + 3] = (da.max(sa * (blurred[i + 3] as f64 / 255.0)).clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }
    }
    session.active_mut().unwrap().touch();
    Ok(serde_json::json!({ "points": points.len() }))
}

/// A box blur, separable, on straight RGBA.
fn box_blur(src: &[u8], width: u32, height: u32, radius: usize) -> Vec<u8> {
    if radius == 0 {
        return src.to_vec();
    }
    let (w, h) = (width as usize, height as usize);
    let mut tmp = vec![0u8; src.len()];
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0u32; 4];
            let mut n = 0u32;
            for k in x.saturating_sub(radius)..=(x + radius).min(w - 1) {
                let i = (y * w + k) * 4;
                for c in 0..4 {
                    acc[c] += u32::from(src[i + c]);
                }
                n += 1;
            }
            let o = (y * w + x) * 4;
            for c in 0..4 {
                tmp[o + c] = (acc[c] / n.max(1)) as u8;
            }
        }
    }
    let mut out = vec![0u8; src.len()];
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0u32; 4];
            let mut n = 0u32;
            for k in y.saturating_sub(radius)..=(y + radius).min(h - 1) {
                let i = (k * w + x) * 4;
                for c in 0..4 {
                    acc[c] += u32::from(tmp[i + c]);
                }
                n += 1;
            }
            let o = (y * w + x) * 4;
            for c in 0..4 {
                out[o + c] = (acc[c] / n.max(1)) as u8;
            }
        }
    }
    out
}

fn pixel_at(buf: &[u8], width: u32, x: usize, y: usize) -> [u8; 4] {
    let i = (y * width as usize + x) * 4;
    if i + 3 >= buf.len() {
        return [0; 4];
    }
    [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]
}

fn number(params: &Value, key: &str) -> Result<f64, Fail> {
    params.get(key).and_then(Value::as_f64).ok_or_else(|| format!("`{key}` is required and must be a number."))
}

/// The stroke's points: `[[x, y], …]`, in document pixels.
fn read_points(params: &Value) -> Result<Vec<(f64, f64)>, Fail> {
    let Some(arr) = params.get("points").and_then(Value::as_array) else {
        return Err("A stroke needs `points`.".into());
    };
    let mut out = Vec::with_capacity(arr.len());
    for p in arr {
        let Some(pair) = p.as_array() else { return Err("Each point is `[x, y]`.".into()) };
        if pair.len() != 2 {
            return Err("Each point is `[x, y]`.".into());
        }
        let x = pair[0].as_f64().ok_or("A point is not a number.")?;
        let y = pair[1].as_f64().ok_or("A point is not a number.")?;
        out.push((x, y));
    }
    Ok(out)
}

/// The foreground colour's hex, for a command's reply.
pub fn foreground_hex(session: &Session) -> String {
    hex(session.tools.foreground)
}
