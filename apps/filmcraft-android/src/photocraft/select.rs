//! Selections: rectangle, lasso and the magic wand.
//!
//! A selection is one full-canvas mask, 0 for not selected and 255 for selected. Every tool
//! consults it, so it is the engine's single notion of "where the user is working" — the panels do
//! not need to pass it around, and a command cannot forget it.
//!
//! The mask is anti-aliased only where the shape demands it (a lasso edge); a rectangle and a wand
//! are hard-edged, which is what makes them predictable to draw and to fill.

use serde_json::Value;

use crate::photocraft::compose;
use crate::photocraft::Session;

/// A selection failure, as the UI shows it.
pub type Fail = String;

/// `select.rect`: a rectangle in document pixels.
pub fn rect(session: &mut Session, params: &Value) -> Result<Value, Fail> {
    let x = number(params, "x")?;
    let y = number(params, "y")?;
    let w = number(params, "width")?;
    let h = number(params, "height")?;
    if w <= 0.0 || h <= 0.0 {
        return Err("A selection needs a width and a height.".into());
    }
    let doc = session.active_mut().ok_or("Open a picture first.")?;
    let (dw, dh) = (doc.doc.width(), doc.doc.height());
    let x0 = x.floor().max(0.0) as u32;
    let y0 = y.floor().max(0.0) as u32;
    let x1 = ((x + w).ceil().max(0.0) as u32).min(dw);
    let y1 = ((y + h).ceil().max(0.0) as u32).min(dh);
    if x1 <= x0 || y1 <= y0 {
        return Err("That rectangle is outside the canvas.".into());
    }
    let mut mask = vec![0u8; (dw as usize) * (dh as usize)];
    for yy in y0..y1 {
        let row = yy as usize * dw as usize;
        for xx in x0..x1 {
            mask[row + xx as usize] = 255;
        }
    }
    doc.push_history("Select rectangle");
    *doc.doc.selection_mut() = Some(mask);
    doc.touch();
    Ok(serde_json::json!({ "selection": [x0, y0, x1 - x0, y1 - y0] }))
}

/// `select.lasso`: a freehand closed outline through `points`.
///
/// The path is closed from the last point back to the first, as a lasso is. Its inside is found by
/// the even-odd rule at each pixel centre, and the edge pixels are anti-aliased by sampling a
/// 2×2 grid, so a curving lasso does not show stair steps.
pub fn lasso(session: &mut Session, params: &Value) -> Result<Value, Fail> {
    let points = points_of(params)?;
    if points.len() < 3 {
        return Err("A lasso needs at least three points.".into());
    }
    let doc = session.active_mut().ok_or("Open a picture first.")?;
    let (dw, dh) = (doc.doc.width(), doc.doc.height());
    let mut mask = vec![0u8; (dw as usize) * (dh as usize)];
    for y in 0..dh {
        for x in 0..dw {
            // four sub-samples per pixel: enough to soften an edge without costing real time
            let mut hits = 0u32;
            for (sx, sy) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
                if inside(&points, x as f64 + sx, y as f64 + sy) {
                    hits += 1;
                }
            }
            mask[y as usize * dw as usize + x as usize] = (hits * 255 / 4) as u8;
        }
    }
    doc.push_history("Select lasso");
    *doc.doc.selection_mut() = Some(mask);
    doc.touch();
    Ok(serde_json::json!({ "selection": "lasso", "points": points.len() }))
}

/// `select.magicWand`: everything connected to `x`,`y` whose colour is within `tolerance`.
pub fn magic_wand(session: &mut Session, params: &Value) -> Result<Value, Fail> {
    let x = number(params, "x")?;
    let y = number(params, "y")?;
    let tolerance = params.get("tolerance").and_then(Value::as_f64).unwrap_or(32.0).clamp(0.0, 255.0);
    let (dw, dh, composite) = {
        let doc = session.active().ok_or("Open a picture first.")?;
        if x < 0.0 || y < 0.0 || x >= f64::from(doc.doc.width()) || y >= f64::from(doc.doc.height()) {
            return Err("That point is outside the canvas.".into());
        }
        (doc.doc.width(), doc.doc.height(), compose::render(&doc.doc))
    };
    let seed = pixel_at(&composite, dw, x as usize, y as usize);
    let outline = flood(&composite, dw, dh, x as usize, y as usize, seed, tolerance);
    let mut mask = vec![0u8; (dw as usize) * (dh as usize)];
    for (i, &on) in outline.iter().enumerate() {
        if on {
            mask[i] = 255;
        }
    }
    let doc = session.active_mut().ok_or("Open a picture first.")?;
    doc.push_history("Select wand");
    *doc.doc.selection_mut() = Some(mask);
    doc.touch();
    Ok(serde_json::json!({ "selection": "wand" }))
}

/// Whether a point is inside the closed polygon, by the even-odd rule.
fn inside(points: &[(f64, f64)], x: f64, y: f64) -> bool {
    let mut inside = false;
    let n = points.len();
    for i in 0..n {
        let (x0, y0) = points[i];
        let (x1, y1) = points[(i + 1) % n];
        if (y0 > y) != (y1 > y) {
            let t = (y - y0) / (y1 - y0);
            if x < x0 + t * (x1 - x0) {
                inside = !inside;
            }
        }
    }
    inside
}

/// A connected region of similar colour, as one `bool` per pixel.
fn flood(composite: &[u8], width: u32, height: u32, sx: usize, sy: usize, seed: [u8; 4], tolerance: f64) -> Vec<bool> {
    let w = width as usize;
    let h = height as usize;
    let mut done = vec![false; w * h];
    let mut stack = vec![(sx, sy)];
    let matches = |p: [u8; 4]| -> bool {
        let d = |a: u8, b: u8| (f64::from(a) - f64::from(b)).abs();
        d(p[0], seed[0]).max(d(p[1], seed[1])).max(d(p[2], seed[2])).max(d(p[3], seed[3])) <= tolerance
    };
    while let Some((x, y)) = stack.pop() {
        if x >= w || y >= h {
            continue;
        }
        let i = y * w + x;
        if done[i] || !matches(pixel_at(composite, width, x, y)) {
            continue;
        }
        let mut left = x;
        while left > 0 && !done[y * w + left - 1] && matches(pixel_at(composite, width, left - 1, y)) {
            left -= 1;
        }
        let mut right = x;
        while right + 1 < w && !done[y * w + right + 1] && matches(pixel_at(composite, width, right + 1, y)) {
            right += 1;
        }
        for cx in left..=right {
            done[y * w + cx] = true;
            for ny in [y.wrapping_sub(1), y + 1] {
                if ny < h {
                    let nj = ny * w + cx;
                    if !done[nj] && matches(pixel_at(composite, width, cx, ny)) {
                        stack.push((cx, ny));
                    }
                }
            }
        }
    }
    done
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

fn points_of(params: &Value) -> Result<Vec<(f64, f64)>, Fail> {
    let Some(arr) = params.get("points").and_then(Value::as_array) else {
        return Err("A lasso needs `points`.".into());
    };
    let mut out = Vec::with_capacity(arr.len());
    for p in arr {
        let Some(pair) = p.as_array() else { return Err("Each point is `[x, y]`.".into()) };
        if pair.len() != 2 {
            return Err("Each point is `[x, y]`.".into());
        }
        out.push((
            pair[0].as_f64().ok_or("A point is not a number.")?,
            pair[1].as_f64().ok_or("A point is not a number.")?,
        ));
    }
    Ok(out)
}
