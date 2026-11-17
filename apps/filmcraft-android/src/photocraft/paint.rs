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
        Some(text) => parse_colour(text).map_err(|e| e)?.ok_or_else(|| "That colour is empty.".to_string()),
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
    // is what decides, because that is what the tool rail selected.
    let straight = params.get("pencil").and_then(Value::as_bool).unwrap_or(false);

    let (width, height) = {
        let doc = session.active_mut().ok_or("Open a picture first.")?;
        // The snapshot is the state to return to, so it is taken before the edit.
        doc.push_history(if straight { "Pencil" } else { "Brush" });
        (doc.doc.width, doc.doc.height)
    };
    let selection = session.active().and_then(|d| d.doc.selection.clone());
    let layer = target(session)?;
    let Content::Pixels(pixels) = &mut layer.content else { unreachable!() };
    // `make_mut` is what keeps the undo snapshot cheap: the clone happened when the snapshot was
    // taken, this is the copy that the edit actually needs.
    let px = std::sync::Arc::make_mut(
        pixels.get_or_insert_with(|| std::sync::Arc::new(vec![0u8; (width as usize) * (height as usize) * 4])),
    );

    let radius = (size / 2.0).max(0.25);
    for pair in points.windows(2) {
        stamp_segment(px, width, height, pair[0], pair[1], radius, hardness, opacity, colour, straight, selection.as_deref());
    }
    if points.len() == 1 {
        stamp_segment(px, width, height, points[0], points[0], radius, hardness, opacity, colour, straight, selection.as_deref());
    }
    session.active_mut().unwrap().touch();
    Ok(serde_json::json!({ "points": points.len() }))
}

/// Stamp a round tip along the segment `a`..`b`.
#[allow(clippy::too_many_arguments)]
fn stamp_segment(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    a: (f64, f64),
    b: (f64, f64),
    radius: f64,
    hardness: f64,
    opacity: f64,
    colour: [u8; 4],
    straight: bool,
    selection: Option<&[u8]>,
) {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let length = (dx * dx + dy * dy).sqrt();
    // One stamp every half radius keeps the stroke solid without stamping the same pixel twice.
    let step = (radius * 0.5).max(0.5);
    let steps = ((length / step).ceil() as usize).max(1);
    for i in 0..=steps {
        let t = if steps == 0 { 0.0 } else { i as f64 / steps as f64 };
        let cx = a.0 + dx * t;
        let cy = a.1 + dy * t;
        stamp(pixels, width, height, cx, cy, radius, hardness, opacity, colour, straight, selection);
    }
}

/// Stamp one round tip, anti-aliased at the edge and eased by `hardness`.
#[allow(clippy::too_many_arguments)]
fn stamp(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    cx: f64,
    cy: f64,
    radius: f64,
    hardness: f64,
    opacity: f64,
    colour: [u8; 4],
    straight: bool,
    selection: Option<&[u8]>,
) {
    let x0 = ((cx - radius).floor() as i64).max(0);
    let y0 = ((cy - radius).floor() as i64).max(0);
    let x1 = ((cx + radius).ceil() as i64).min(i64::from(width) - 1);
    let y1 = ((cy + radius).ceil() as i64).min(i64::from(height) - 1);
    if x1 < x0 || y1 < y0 {
        return;
    }
    for y in y0..=y1 {
        for x in x0..=x1 {
            // A selection confines paint; that is the whole point of making one.
            if let Some(mask) = selection {
                if mask[y as usize * width as usize + x as usize] == 0 {
                    continue;
                }
            }
            let d = ((x as f64 + 0.5 - cx).powi(2) + (y as f64 + 0.5 - cy).powi(2)).sqrt();
            // `hardness` is where the falloff begins: 1 is a hard edge, 0 a soft one.
            let edge = if straight {
                1.0
            } else {
                let inner = radius * hardness.clamp(0.0, 1.0);
                if d <= inner {
                    1.0
                } else if d >= radius {
                    0.0
                } else {
                    1.0 - (d - inner) / (radius - inner).max(1e-6)
                }
            };
            if edge <= 0.0 {
                continue;
            }
            let i = ((y as usize) * width as usize + x as usize) * 4;
            let sa = edge * opacity * (colour[3] as f64 / 255.0);
            if sa <= 0.0 {
                continue;
            }
            let da = pixels[i + 3] as f64 / 255.0;
            let out_a = sa + da * (1.0 - sa);
            if out_a <= 0.0 {
                continue;
            }
            for c in 0..3 {
                let sc = colour[c] as f64 / 255.0;
                let dc = pixels[i + c] as f64 / 255.0;
                let v = (sc * sa + dc * da * (1.0 - sa)) / out_a;
                pixels[i + c] = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
            pixels[i + 3] = (out_a.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
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
    let (width, height) = (doc.doc.width, doc.doc.height);
    if x < 0 || y < 0 || x >= i64::from(width) || y >= i64::from(height) {
        return Err("That point is outside the canvas.".into());
    }
    let composite = compose::render(&doc.doc);
    let selection = doc.doc.selection.clone();
    // A seed outside the selection has nothing it may fill.
    if let Some(mask) = &selection {
        if mask[(y as usize) * width as usize + x as usize] == 0 {
            return Err("That point is outside the selection.".into());
        }
    }
    let seed = pixel_at(&composite, width, x as usize, y as usize);
    let filled = flood(&composite, &selection, width, height, x as usize, y as usize, seed, tolerance);

    session.active_mut().ok_or("Open a picture first.")?.push_history("Fill");
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
        Some(c) => parse_colour(c).map_err(|e| e)?.ok_or("The gradient needs a colour.")?,
        None => session.tools.foreground,
    };
    let end = match params.get("color2").and_then(Value::as_str) {
        Some(c) => parse_colour(c).map_err(|e| e)?.ok_or("The gradient needs a second colour.")?,
        None => session.tools.background,
    };
    let (width, height) = {
        let doc = session.active().ok_or("Open a picture first.")?;
        (doc.doc.width, doc.doc.height)
    };
    let selection = session.active().unwrap().doc.selection.clone();
    let dx = x1 - x0;
    let dy = y1 - y0;
    let len2 = dx * dx + dy * dy;
    if len2 <= 1e-9 {
        return Err("A gradient needs two different points.".into());
    }
    session.active_mut().ok_or("Open a picture first.")?.push_history("Gradient");
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
/// The source is `sourceX`,`sourceY` — where the user alt-tapped — and the offset from the first
/// stroke point is held for the whole stroke, which is how a clone stamp is expected to behave.
pub fn clone_stamp(session: &mut Session, params: &Value) -> Result<Value, Fail> {
    let points = read_points(params)?;
    if points.is_empty() {
        return Err("The stamp needs at least one point.".into());
    }
    let sx = number(params, "sourceX")?;
    let sy = number(params, "sourceY")?;
    let size = params.get("size").and_then(Value::as_f64).unwrap_or(session.tools.brush_size).clamp(1.0, 5000.0);
    let opacity = params
        .get("opacity")
        .and_then(Value::as_f64)
        .map(|v| if v > 1.0 { v / 100.0 } else { v })
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    let (width, height) = {
        let doc = session.active().ok_or("Open a picture first.")?;
        (doc.doc.width, doc.doc.height)
    };
    // The offset from the stroke's first point to the source stays fixed for the stroke.
    let dx = sx - points[0].0;
    let dy = sy - points[0].1;
    let selection = session.active().unwrap().doc.selection.clone();
    let source = compose::render(&session.active().unwrap().doc);

    session.active_mut().ok_or("Open a picture first.")?.push_history("Clone stamp");
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
        (doc.doc.width, doc.doc.height)
    };
    let selection = session.active().unwrap().doc.selection.clone();
    let before = compose::render(&session.active().unwrap().doc);
    // A small box blur of the whole composite, then mixed in under the brush.
    let blurred = box_blur(&before, width, height, (radius / 2.0).max(1.0) as usize);
    session.active_mut().ok_or("Open a picture first.")?.push_history("Spot healing");
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
