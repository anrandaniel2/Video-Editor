//! Live strokes: the brush rendered while a finger draws, not once on release.
//!
//! A stroke is drawn and shown as it goes (the studio's canvas, and the photo editor's), and sent
//! to the engine once, on release, so it is one History step. The engine already renders a stroke
//! as it is drawn ([`photocraft_engine::brush_cmds::LiveStroke`]); this is the bridge to it:
//!
//! 1. `stroke_begin` with the `paint.stroke` params (brush, colour, points so far).
//! 2. `stroke_add` with each new batch of points; it returns the pixels that changed, as a frame
//!    the canvas draws over what it already shows.
//! 3. `stroke_end` hands the finished stroke to the engine and runs `paint.stroke` with every
//!    point it was given; the engine adopts the previewed pixels (`"fromLive": true`), so the
//!    commit costs a history record, not a second render.
//!
//! `stroke_cancel` throws the preview away: the document is never touched until `stroke_end`, so
//! a cancelled stroke leaves no trace in the document, the history or the journal.
//!
//! The session is locked only for the command itself: the stroke's renderer lives in
//! [`LiveStroke`], so a five-second stroke holds no lock between points and the UI thread can keep
//! compositing.

use photocraft_color::PixelFormat;
use photocraft_engine::brush_cmds::LiveStroke;
use photocraft_geom::Rect;
use photocraft_paint::StrokePoint;
use photocraft_raster::Surface;
use serde_json::{Value, json};

use crate::{PhotoError, PhotoFrame, PhotoSession};

/// One live stroke's renderer, held by [`PhotoSession`] between `stroke_begin` and `stroke_end`.
pub(crate) struct LiveStrokeSlot {
    /// The command the stroke will commit as: `paint.stroke` or `paint.pencil`.
    cmd: String,
    /// The params it began with, minus the points: the commit's settings.
    settings: Value,
    stroke: LiveStroke,
    /// Every point given, in order: the commit's `points`.
    points: Vec<StrokePoint>,
}

impl LiveStrokeSlot {
    /// One point from the wire's array: `[x, y, pressure?, tiltX?, tiltY?, rotation?, timeMs?,
    /// wheel?]`. Anything else (a short array, a non-finite number) is a bad point.
    fn point(v: &Value) -> Option<StrokePoint> {
        let a = v.as_array()?;
        let num = |i: usize, d: f64| a.get(i).and_then(Value::as_f64).filter(|x| x.is_finite()).unwrap_or(d);
        let f = |i: usize, d: f32| num(i, f64::from(d)) as f32;
        let x = a.first().and_then(Value::as_f64).filter(|x| x.is_finite())?;
        let y = a.get(1).and_then(Value::as_f64).filter(|y| y.is_finite())?;
        Some(StrokePoint {
            x,
            y,
            pressure: f(2, 1.0).clamp(0.0, 1.0),
            tilt_x: f(3, 0.0).clamp(-90.0, 90.0),
            tilt_y: f(4, 0.0).clamp(-90.0, 90.0),
            rotation: f(5, 0.0).rem_euclid(360.0),
            time: num(6, 0.0),
            wheel: f(7, 0.0),
        })
    }

    /// The points of a wire batch: `[[x, y, …], …]`.
    pub(crate) fn points(v: &Value) -> Option<Vec<StrokePoint>> {
        let list = v.as_array()?;
        if list.is_empty() {
            return None;
        }
        list.iter().map(Self::point).collect()
    }
}

/// A view of a document's pixels over `rect`, for the canvas to draw over what it shows: the
/// stroke is composited into a copy of the document, so what comes back is exactly what the
/// engine will commit, with nothing the canvas has to guess at.
fn surface_frame(surf: &Surface, rect: Rect) -> Option<PhotoFrame> {
    if rect.is_empty() {
        return None;
    }
    let (w, h) = (rect.width(), rect.height());
    let mut px = vec![[0u8; 4]; w as usize * h as usize];
    surf.read_rgba8_into(rect, &mut px);
    let mut rgba = Vec::with_capacity(px.len() * 4);
    for p in &px {
        rgba.extend_from_slice(p);
    }
    Some(PhotoFrame { width: w, height: h, rgba })
}

#[uniffi::export]
impl PhotoSession {
    /// Start a live stroke of `cmd` (`"paint.stroke"` for the Brush and Eraser, `"paint.pencil"`
    /// for the Pencil) with the command's params: `points` (the first sample), `size`, `flow`,
    /// `smoothing`, `color`, … as `paint.stroke` takes them. Nothing is committed: the document,
    /// the history and the journal are untouched until [`Self::stroke_end`].
    ///
    /// Returns `false` when there is nothing to paint on (no document, or the layer is hidden or
    /// locked), so the caller can fall back to a tap without an exception.
    pub fn stroke_begin(&self, cmd: String, params: String) -> Result<bool, PhotoError> {
        let params: Value = if params.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(&params).map_err(|e| PhotoError::Command(format!("bad params for `{cmd}`: {e}")))?
        };
        let points = LiveStrokeSlot::points(params.get("points").unwrap_or(&Value::Null))
            .ok_or_else(|| PhotoError::Command(format!("`{cmd}` needs at least one point, [x, y]")))?;
        let mut settings = params.clone();
        if let Some(o) = settings.as_object_mut() {
            o.remove("points");
        }
        let mut s = self.lock();
        s.clear_live_stroke();
        let stroke = match LiveStroke::begin_with(&s, &cmd, &params) {
            Ok(stroke) => stroke,
            // A locked layer or no document is not an error the shell should have to catch: the
            // gesture just doesn't paint.
            Err(_) => return Ok(false),
        };
        drop(s);
        let mut slot = self.live.lock().unwrap_or_else(|e| e.into_inner());
        *slot = Some(LiveStrokeSlot { cmd, settings, stroke, points });
        Ok(true)
    }

    /// Render more points into the live stroke; `points` is JSON `[[x, y, …], …]`. Returns the
    /// rectangle that changed, as `{x, y, width, height}` — its pixels are [`Self::stroke_pixels`]'s
    /// — or `null` when nothing changed.
    pub fn stroke_add(&self, points: String) -> Result<Option<String>, PhotoError> {
        let points: Value = serde_json::from_str(&points).map_err(|e| PhotoError::Command(format!("bad points: {e}")))?;
        let batch = LiveStrokeSlot::points(&points).ok_or_else(|| PhotoError::Command("`points` must be a non-empty [[x, y, …], …]".into()))?;
        let mut slot = self.live.lock().unwrap_or_else(|e| e.into_inner());
        let live = slot.as_mut().ok_or_else(|| PhotoError::Command("no live stroke: call stroke_begin first".into()))?;
        let dmg = live.stroke.push(&batch)?;
        live.points.extend(batch);
        if dmg.is_empty() {
            return Ok(None);
        }
        Ok(Some(json!({ "x": dmg.x0, "y": dmg.y0, "width": dmg.width(), "height": dmg.height() }).to_string()))
    }

    /// The rectangle the live stroke has touched so far, as [`Self::stroke_add`] returns a
    /// damage rect: the pixels of a stroke's first dab, before any more points arrive.
    pub fn stroke_bounds(&self) -> Result<Option<String>, PhotoError> {
        let slot = self.live.lock().unwrap_or_else(|e| e.into_inner());
        let live = slot.as_ref().ok_or_else(|| PhotoError::Command("no live stroke: call stroke_begin first".into()))?;
        let r = live.stroke.bounds();
        if r.is_empty() {
            return Ok(None);
        }
        Ok(Some(json!({ "x": r.x0, "y": r.y0, "width": r.width(), "height": r.height() }).to_string()))
    }

    /// The live stroke's pixels over `rect` (document pixels): a copy of the active document with
    /// the stroke so far composited in, ready to draw over what the canvas already shows. An error
    /// when no stroke is being drawn. The canvas asks for the damage rect [`Self::stroke_add`]
    /// returned.
    pub fn stroke_pixels(&self, x: i32, y: i32, width: u32, height: u32) -> Result<Option<PhotoFrame>, PhotoError> {
        let rect = Rect::from_xywh(x, y, width, height);
        let slot = self.live.lock().unwrap_or_else(|e| e.into_inner());
        let live = slot.as_ref().ok_or_else(|| PhotoError::Render("no live stroke".into()))?;
        let doc = live.stroke.doc.clone();
        drop(slot);
        // The composite is a `Buffer`, not a `Surface`: the same path `render_region` takes.
        Ok(Some(crate::frame_from_buffer(photocraft_compose::render(&doc, rect))))
    }

    /// Whether the live stroke keeps changing while the pointer is still (an airbrush's build-up,
    /// or smoothing still catching up). The canvas then calls [`Self::stroke_tick`] on a timer.
    pub fn stroke_wants_time(&self) -> bool {
        let slot = self.live.lock().unwrap_or_else(|e| e.into_inner());
        slot.as_ref().is_some_and(|live| live.stroke.wants_time())
    }

    /// Let the live stroke's brush catch up with the pointer where it is: the airbrush's dabs
    /// laid down over time, and the smoothing tail drawn out. Returns the damage rectangle, as
    /// [`Self::stroke_add`] does.
    pub fn stroke_tick(&self) -> Result<Option<String>, PhotoError> {
        let mut slot = self.live.lock().unwrap_or_else(|e| e.into_inner());
        let live = slot.as_mut().ok_or_else(|| PhotoError::Command("no live stroke: call stroke_begin first".into()))?;
        let last = live.points.last().copied().ok_or_else(|| PhotoError::Command("the live stroke has no points".into()))?;
        let dmg = live.stroke.push(&[last])?;
        drop(slot);
        if dmg.is_empty() {
            return Ok(None);
        }
        Ok(Some(json!({ "x": dmg.x0, "y": dmg.y0, "width": dmg.width(), "height": dmg.height() }).to_string()))
    }

    /// Finish the live stroke: hand its pixels to the engine and run the command it was begun
    /// for with every point it was given. One stroke is one History step. Returns the command's
    /// JSON result (with `"fromLive": true`).
    pub fn stroke_end(&self) -> Result<String, PhotoError> {
        let slot = self.live.lock().unwrap_or_else(|e| e.into_inner()).take();
        let Some(live) = slot else {
            return Err(PhotoError::Command("no live stroke: call stroke_begin first".into()));
        };
        // The commit must carry the stroke's own seed: it is what makes the command render the
        // same jitters the preview did, and what the engine compares to adopt the previewed
        // pixels instead of drawing the stroke again.
        let seed = live.stroke.seed;
        let mut params = live.settings;
        if let Some(o) = params.as_object_mut() {
            o.insert("points".into(), points_json(&live.points));
            o.insert("seed".into(), json!(seed));
        }
        let mut s = self.lock();
        s.offer_live_stroke(live.stroke).map_err(PhotoError::from)?;
        let out = s.execute(&live.cmd, params);
        match out {
            Ok(v) => Ok(v.to_string()),
            Err(e) => {
                s.clear_live_stroke();
                Err(PhotoError::from(e))
            }
        }
    }

    /// Throw the live stroke away. The document was never touched, so there is nothing to undo.
    pub fn stroke_cancel(&self) {
        self.live.lock().unwrap_or_else(|e| e.into_inner()).take();
        self.lock().clear_live_stroke();
    }

    /// Whether a live stroke is in progress.
    pub fn stroke_active(&self) -> bool {
        self.live.lock().unwrap_or_else(|e| e.into_inner()).is_some()
    }

    /// The brush the engine will paint the next stroke with, as JSON: the same shape
    /// `tools.setBrush` takes, so the studio's sliders read the engine's brush rather than keeping
    /// their own copy of it.
    pub fn brush(&self) -> String {
        let s = self.lock();
        serde_json::to_value(&s.tools.brush).map(|v| v.to_string()).unwrap_or_else(|_| "{}".into())
    }

    /// The value of one key of the current brush, as JSON (`"size"`, `"opacity"`, …). `null` when
    /// the brush has no such key.
    pub fn brush_value(&self, key: String) -> String {
        let s = self.lock();
        let v = serde_json::to_value(&s.tools.brush).ok();
        let out = v.as_ref().and_then(|v| v.get(&key)).cloned().unwrap_or(Value::Null);
        out.to_string()
    }

    /// A preview of the brush's next stroke, drawn at `height` pixels tall over `width` × `height`
    /// pixels: the same tip, dynamics and texture the stroke will use, with a stroke that bends
    /// and varies its pressure, so the thumbnail shows what the brush does.
    ///
    /// `params` is a `tools.setBrush` patch to preview instead of the current brush (empty for the
    /// brush as it is); the studio's preset list uses it to show every preset before it is picked.
    pub fn brush_preview(&self, width: u32, height: u32, params: String) -> Result<PhotoFrame, PhotoError> {
        let (w, h) = (width.clamp(8, 512), height.clamp(8, 512));
        let patch: Value = if params.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(&params).map_err(|e| PhotoError::Render(format!("bad brush params: {e}")))?
        };
        // `preset` picks a library brush the way a stroke's `preset` does (its own tip, dynamics
        // and stabilisation, over the current tool state); anything else in the patch is merged
        // into that, exactly as `tools.setBrush` merges it.
        let (mut brush, rest) = {
            let s = self.lock();
            let mut rest = patch.clone();
            let preset = rest.get("preset").and_then(Value::as_str).map(str::to_owned);
            if let Some(o) = rest.as_object_mut() {
                o.remove("preset");
            }
            let mut picked = match preset.as_deref() {
                Some(name) => photocraft_paint::presets::find(&s.tools.presets, name)
                    .ok_or_else(|| PhotoError::Render(format!("no brush preset named `{name}`")))?
                    .picked_over(&s.tools.brush),
                None => s.tools.brush.clone(),
            };
            // A library preset's sampled tip lives in the store until a stroke needs it: a preview
            // is a stroke, so load it here or the thumbnail of a textural brush is blank.
            s.load_brush_tips(&mut picked).map_err(PhotoError::Render)?;
            (picked, rest)
        };
        if rest.as_object().is_some_and(|o| !o.is_empty()) {
            // The same deep merge `tools.setBrush` does, so a partial patch is enough.
            brush = photocraft_engine::brush_cmds::merge_brush(&brush, &rest, "brush.preview")
                .map_err(|e| PhotoError::Render(e.to_string()))?;
        }
        // The preview stroke: a low arc from left to right, pressure rising and falling, timed so
        // the brush's own spacing and speed rules act as they would under a finger.
        let (fw, fh) = (f64::from(w), f64::from(h));
        let steps = (w / 2).clamp(12, 96);
        let mut points = Vec::with_capacity(steps as usize + 1);
        for i in 0..=steps {
            let t = f64::from(i) / f64::from(steps);
            let pressure = (0.25 + 0.75 * (std::f64::consts::PI * t).sin()) as f32;
            points.push(StrokePoint {
                x: fw * (0.06 + 0.88 * t),
                y: fh * (0.5 + 0.16 * (std::f64::consts::TAU * t).sin()),
                pressure,
                tilt_x: 0.0,
                tilt_y: 0.0,
                rotation: 0.0,
                time: t * 320.0,
                wheel: 0.0,
            });
        }
        let mut surf = Surface::new(PixelFormat::RGBA8);
        let rect = Rect::from_xywh(0, 0, w, h);
        // A preview paints its own stroke on nothing: it never erases, and it is drawn in the
        // panel's light ink so that it reads on the studio's dark surface.
        brush.erase = false;
        brush.behind = false;
        brush.color = [PREVIEW_INK[0], PREVIEW_INK[1], PREVIEW_INK[2], 1.0];
        brush.background = brush.color;
        // A thumbnail shows the brush's shape, not its size: a sampled tip is scaled so its larger
        // side equals `size`, and the shelf's brushes run to 380 px against a thumbnail 34 px tall,
        // so drawing them at their own size fills the box and every wide brush looks the same
        // clipped slab. Fit the brush to the preview instead, keeping its proportions (a dual
        // tip scales with it, as `resize_with_dual_brush` does for a picked preset).
        let room = fw.min(fh) * PREVIEW_FIT;
        if brush.size > room && brush.size.is_finite() && room > 0.0 {
            brush.resize_with_dual_brush((room as f32).max(1.0));
        }
        photocraft_paint::render_stroke_clipped(&mut surf, &brush, &points, None, false, 1.0, Some(rect));
        surface_frame(&surf, rect).ok_or_else(|| PhotoError::Render("the preview is empty".into()))
    }
}

/// The ink a brush preview paints with: the studio panel's light stroke colour.
const PREVIEW_INK: [f32; 3] = [0.93, 0.93, 0.93];

/// How much of the preview's shorter side a brush may fill before it is scaled down to fit: the
/// stroke's arc and its own radius stay inside the thumbnail, so the brush's edge is visible.
const PREVIEW_FIT: f64 = 0.42;

/// `points` as the command's JSON: `[[x, y, pressure, tiltX, tiltY, rotation, time, wheel], …]`.
fn points_json(points: &[StrokePoint]) -> Value {
    Value::Array(
        points
            .iter()
            .map(|p| json!([p.x, p.y, p.pressure, p.tilt_x, p.tilt_y, p.rotation, p.time, p.wheel]))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An 8-bit document with one empty layer, ready to paint on.
    fn session() -> PhotoSession {
        let s = PhotoSession::new();
        s.execute("file.new".into(), r#"{"width":160,"height":120,"background":"white"}"#.into()).unwrap();
        s
    }

    #[test]
    fn a_stroke_is_drawn_before_it_is_committed() {
        let s = session();
        let before = s.status().revision;
        let began = s
            .stroke_begin("paint.stroke".into(), r##"{"points":[[20,20]],"size":24,"flow":1.0,"color":"#ff0000","smoothing":0.0}"##.into())
            .unwrap();
        assert!(began, "a blank document paints");
        // The document has not changed yet: the stroke lives in the preview.
        assert_eq!(s.status().revision, before);
        assert!(s.stroke_active());
        let damage: Value = serde_json::from_str(&s.stroke_add(r#"[[40,30],[60,40],[80,50]]"#.into()).unwrap().expect("the stroke moved")).unwrap();
        assert!(damage["width"].as_u64().unwrap() > 0);
        // The preview's pixels are the stroke's, over the damage rect.
        let frame = s
            .stroke_pixels(damage["x"].as_i64().unwrap() as i32, damage["y"].as_i64().unwrap() as i32, damage["width"].as_u64().unwrap() as u32, damage["height"].as_u64().unwrap() as u32)
            .unwrap()
            .expect("the stroke has pixels");
        assert_eq!(frame.rgba.len(), frame.width as usize * frame.height as usize * 4);
        assert!(frame.rgba.chunks_exact(4).any(|p| p[0] > p[1] && p[0] > p[2]), "the stroke is red");
        let out: Value = serde_json::from_str(&s.stroke_end().unwrap()).unwrap();
        // The commit adopts the previewed pixels instead of rendering the stroke again.
        assert_eq!(out["fromLive"], Value::Bool(true), "{out}");
        assert!(!s.stroke_active());
        assert_ne!(s.status().revision, before);
        assert!(s.status().can_undo);
    }

    #[test]
    fn a_stroke_is_one_history_step() {
        let s = session();
        assert!(!s.status().can_undo);
        s.stroke_begin("paint.stroke".into(), r##"{"points":[[10,10]],"size":12,"color":"#000000","smoothing":0.0}"##.into()).unwrap();
        s.stroke_add(r#"[[30,20]]"#.into()).unwrap();
        s.stroke_add(r#"[[50,30]]"#.into()).unwrap();
        s.stroke_end().unwrap();
        assert!(s.status().can_undo);
        // One undo takes the whole stroke away, and there is nothing more to undo.
        assert!(s.undo());
        assert!(!s.status().can_undo);
    }

    #[test]
    fn a_cancelled_stroke_leaves_no_trace() {
        let s = session();
        let before = s.status().revision;
        s.stroke_begin("paint.stroke".into(), r#"{"points":[[10,10]],"size":12,"smoothing":0.0}"#.into()).unwrap();
        s.stroke_add(r#"[[90,90]]"#.into()).unwrap();
        s.stroke_cancel();
        assert!(!s.stroke_active());
        assert_eq!(s.status().revision, before, "the document never saw the stroke");
        assert!(!s.status().can_undo, "a cancelled stroke is not an undo step");
    }

    #[test]
    fn a_stroke_on_a_locked_layer_does_not_begin() {
        let s = session();
        s.execute("layer.setProps".into(), r#"{"locked":true}"#.into()).unwrap();
        assert!(!s.stroke_begin("paint.stroke".into(), r#"{"points":[[10,10]],"size":12}"#.into()).unwrap());
        assert!(!s.stroke_active());
    }

    #[test]
    fn a_bad_batch_is_refused_and_keeps_the_stroke() {
        let s = session();
        s.stroke_begin("paint.stroke".into(), r#"{"points":[[10,10]],"size":12}"#.into()).unwrap();
        assert!(s.stroke_add("[]".into()).is_err());
        assert!(s.stroke_add(r#"[["x",10]]"#.into()).is_err());
        assert!(s.stroke_active(), "a refused batch leaves the stroke going");
        s.stroke_add(r#"[[40,40]]"#.into()).unwrap();
        s.stroke_end().unwrap();
    }

    #[test]
    fn the_pencil_is_a_live_stroke_too() {
        let s = session();
        assert!(s.stroke_begin("paint.pencil".into(), r##"{"points":[[10,10]],"size":8,"color":"#000000","smoothing":0.0}"##.into()).unwrap());
        s.stroke_add(r#"[[60,50]]"#.into()).unwrap();
        let out: Value = serde_json::from_str(&s.stroke_end().unwrap()).unwrap();
        assert_eq!(out["fromLive"], Value::Bool(true), "{out}");
    }

    #[test]
    fn the_erasers_stroke_erases() {
        let s = session();
        // Paint a band, then erase across it: the eraser's stroke commits as `paint.stroke`
        // with `erase`.
        s.execute("paint.stroke".into(), r##"{"points":[[0,60],[160,60]],"size":40,"color":"#000000","smoothing":0.0}"##.into()).unwrap();
        let size = s.status().active.expect("the document is open");
        assert!(s.stroke_pixels(0, 0, size.width, size.height).is_err(), "no stroke, no preview");
        let began = s.stroke_begin("paint.stroke".into(), r#"{"points":[[0,60]],"size":20,"erase":true,"smoothing":0.0}"#.into()).unwrap();
        assert!(began);
        s.stroke_add(r#"[[160,60]]"#.into()).unwrap();
        s.stroke_end().unwrap();
        // The erased band is white now: the document changed, and the stroke stands as one step.
        assert!(s.undo());
    }

    #[test]
    fn a_brush_preview_is_the_engines_own_stroke() {
        let s = session();
        let frame = s.brush_preview(180, 56, String::new()).unwrap();
        assert_eq!((frame.width, frame.height), (180, 56));
        assert_eq!(frame.rgba.len(), 180 * 56 * 4);
        // The preview paints: some pixel is the panel's light ink over nothing.
        assert!(frame.rgba.chunks_exact(4).any(|p| p[0] > 180 && p[3] > 0), "the preview has a stroke");
        // A preset patches it without being picked.
        let other = s.brush_preview(96, 40, r#"{"preset":"Soft Airbrush"}"#.into());
        assert!(other.is_ok(), "{:?}", other.err());
    }

    #[test]
    fn a_wide_brush_still_shows_a_stroke_in_the_thumbnail() {
        // The shelf's brushes run to 380 px against a 34 px thumbnail: drawn at their own size a
        // sampled tip covers the whole box, so every wide brush's thumbnail was the same filled
        // slab with no stroke to see. The preview fits the brush to the box instead.
        let s = session();
        for size in [70.0, 160.0, 380.0] {
            let params = format!(r#"{{"size":{size}}}"#);
            let f = s.brush_preview(180, 34, params).unwrap();
            assert_eq!((f.width, f.height), (180, 34));
            let painted = f.rgba.chunks_exact(4).filter(|p| p[3] > 0).count();
            let share = painted as f64 / f64::from(f.width * f.height);
            // A stroke through the box, not a flood: at its own size the widest brush painted
            // every pixel, which is why its thumbnail showed no shape at all.
            assert!(share < 0.60, "size {size}: the preview is a flood, not a stroke ({share:.2})");
            assert!(share > 0.02, "size {size}: the preview is blank ({share:.2})");
            // And the stroke spans the box, as a stroke sample does.
            let row_opaque = |y: u32| (0..f.width).filter(|&x| f.rgba[((y * f.width + x) * 4) as usize + 3] > 0).count();
            assert!((0..f.height).any(|y| row_opaque(y) > f.width / 2), "size {size}: a stroke crosses the box");
        }
    }

    #[test]
    fn the_brush_reads_back_the_engines_own() {
        let s = session();
        s.execute("tools.setBrush".into(), r#"{"size":37}"#.into()).unwrap();
        assert_eq!(s.brush_value("size".into()), "37.0");
        let brush: Value = serde_json::from_str(&s.brush()).unwrap();
        assert_eq!(brush["size"], Value::from(37.0));
        assert_eq!(s.brush_value("nonesuch".into()), "null");
    }

    #[test]
    fn the_studio_library_replaces_the_generic_presets() {
        let s = session();
        let before: Value = serde_json::from_str(&s.execute("brush.presets.list".into(), "{}".into()).unwrap()).unwrap();
        let count_before = before["presets"].as_array().unwrap().len();
        let after = s.studio_install_brushes().unwrap();
        assert!(after as usize > count_before, "the studio's shelf is bigger than the photo editor's");
        let sets = s.studio_brush_sets().unwrap();
        assert!(sets.len() >= 12, "{} sets", sets.len());
        assert_eq!(sets[0].name, "Sketching");
        assert_eq!(sets[0].brushes[0].name, "Graphite");
        assert!(sets.iter().all(|set| !set.brushes.is_empty()));
        // Installing twice does not duplicate the shelf.
        let again = s.studio_install_brushes().unwrap();
        assert_eq!(again, after);
    }
}
