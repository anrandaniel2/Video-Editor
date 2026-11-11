//! Where a picture, video or text sits on the frame, and moving, scaling and turning it with a
//! finger on the preview (one undo step per gesture).

use filmcraft_engine::{EngineError, Session};
use filmcraft_project::{ClipId, ItemKind, ParamValue, TrackKind};
use serde_json::json;

use crate::text::{self, Placement};
use crate::{MobileError, edits, model};

/// How small and large an item may get, as a fraction / multiple of the frame's width.
const MIN_WIDTH: f64 = 0.01;
const MAX_WIDTH: f64 = 5.0;
const MAX_TEXT_WIDTH: f64 = 2.0;

fn is_text(s: &Session, id: u64) -> bool {
    model::sequence(s).ok().and_then(|(_,q)|model::find(q,ClipId(id)))
        .is_some_and(|f| f.clip.effects.iter().any(|e| filmcraft_project::graphic::is_layer_id(&e.effect)))
}

/// Where clip `id` sits on the frame.
pub(crate) fn placement(s: &mut Session, id: u64) -> Result<Placement, MobileError> {
    if is_text(s, id) {
        return text::placement(s, id);
    }
    let (_, seq) = model::sequence(s)?;
    let (fw, fh) = (f64::from(seq.settings.width), f64::from(seq.settings.height));
    let f = model::find(seq, ClipId(id)).filter(|f| f.kind == TrackKind::Video).ok_or_else(|| MobileError::Command("That clip has no picture.".into()))?;
    let c = f.clip;
    let (sw, sh) = s.project.source_size(c.item).map(|(w, h)| (f64::from(w.max(1)), f64::from(h.max(1)))).unwrap_or((fw, fh));
    let mt = c.effect_time_at(s.playhead().clamp(c.start, (c.end() - filmcraft_time::Tick(1)).max(c.start)));
    let (mut x, mut y, mut scale, mut rotation) = (fw / 2.0, fh / 2.0, 1.0, 0.0);
    if let Some(m) = c.effects.iter().find(|e| e.effect == "motion" && e.enabled) {
        let p = m.vec2_at("position", mt);
        if !p.x.is_nan() && !p.y.is_nan() {
            (x, y) = (p.x, p.y);
        }
        scale = m.f64_at("scale", mt) / 100.0;
        rotation = m.f64_at("rotation", mt);
    }
    let fit = if c.scale_to_frame { (fw / sw).min(fh / sh) } else { 1.0 };
    Ok(Placement { x, y, width: sw * fit, height: sh * fit, scale, rotation })
}

/// The upright rectangle (left, top, right, bottom; frame pixels, clamped to the frame) a picture
/// clip covers, ignoring rotation: what the look pickers crop their previews to.
pub(crate) fn clip_rect(project: &filmcraft_project::Project, seq: &filmcraft_project::Sequence, id: u64) -> Option<(f64, f64, f64, f64)> {
    let (fw, fh) = (f64::from(seq.settings.width), f64::from(seq.settings.height));
    let f = model::find(seq, ClipId(id)).filter(|f| f.kind == TrackKind::Video)?;
    let c = f.clip;
    let (sw, sh) = project.source_size(c.item).map(|(w, h)| (f64::from(w.max(1)), f64::from(h.max(1))))?;
    let mt = c.source_in;
    let (mut x, mut y, mut scale) = (fw / 2.0, fh / 2.0, 1.0);
    if let Some(m) = c.effects.iter().find(|e| e.effect == "motion" && e.enabled) {
        let p = m.vec2_at("position", mt);
        if !p.x.is_nan() && !p.y.is_nan() {
            (x, y) = (p.x, p.y);
        }
        scale = m.f64_at("scale", mt) / 100.0;
    }
    let fit = if c.scale_to_frame { (fw / sw).min(fh / sh) } else { 1.0 };
    let (w, h) = (sw * fit * scale, sh * fit * scale);
    let r = ((x - w / 2.0).max(0.0), (y - h / 2.0).max(0.0), (x + w / 2.0).min(fw), (y + h / 2.0).min(fh));
    (r.2 - r.0 >= 1.0 && r.3 - r.1 >= 1.0).then_some(r)
}

/// Move, scale and turn clip `id`: centre at (`x`, `y`) frame pixels, `scale` × its placed size,
/// `rotation` degrees. `merge` folds a gesture into one undo step.
pub(crate) fn set_placement(s: &mut Session, id: u64, x: f64, y: f64, scale: f64, rotation: f64, merge: bool) -> Result<(), MobileError> {
    if ![x, y, scale, rotation].iter().all(|v| v.is_finite()) {
        return Err(MobileError::Command("The placement must be numbers.".into()));
    }
    let now = placement(s, id)?;
    let (_, seq) = model::sequence(s)?;
    let fw = f64::from(seq.settings.width.max(1));
    let text = is_text(s, id);
    let max = if text { MAX_TEXT_WIDTH } else { MAX_WIDTH };
    let base = now.width.max(1.0);
    let scale = scale.clamp(MIN_WIDTH * fw / base, max * fw / base);
    let key = format!("place:{id}");
    if !merge { s.history.merge_key = None; }
    if text {
        // a text layer is placed by its anchor: move that by however far the centre moves
        let v = edits::ex(s, "graphics.list", json!({"clip": id}))?;
        let l = v["layers"].as_array().and_then(|a| a.first()).ok_or_else(|| MobileError::Command("That clip has no text.".into()))?;
        let px = l["position"][0].as_f64().unwrap_or(now.x);
        let py = l["position"][1].as_f64().unwrap_or(now.y);
        let props = json!({"position": [px + x - now.x, py + y - now.y], "scale": scale * 100.0, "rotation": rotation});
        let mut q = json!({"clip": id, "layer": 0, "props": props});
        q["merge"] = json!(key);
        edits::ex(s, "graphics.set", q)?;
        return Ok(());
    }
    let playhead = s.playhead();
    s.edit_sequence_as("Transform", Some(key.as_str()), |q, _, _| {
        let c = q.find_item_mut(ClipId(id)).map(|x| x.1).ok_or_else(|| EngineError::Other("clip gone".into()))?;
        let mt = c.effect_time_at(playhead.clamp(c.start, (c.end() - filmcraft_time::Tick(1)).max(c.start)));
        let m = c.effects.iter_mut().find(|e| e.effect == "motion").ok_or_else(|| EngineError::Other("no motion".into()))?;
        let mut put = |p: &str, v: ParamValue| {
            if let Some(param) = m.param_mut(p) {
                param.set_at(mt, v);
            }
        };
        put("position", ParamValue::Vec2(filmcraft_geom::Vec2::new(x, y)));
        put("scale", ParamValue::Float(scale * 100.0));
        put("rotation", ParamValue::Float(rotation));
        m.enabled = true;
        Ok(())
    })?;
    edits::settle(s);
    Ok(())
}
