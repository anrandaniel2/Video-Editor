//! Clip masks for the phone editor, built on the engine's `masks.*` commands.
//!
//! A phone mask belongs to the clip's intrinsic Opacity effect: it cuts the picture out before
//! Motion places it over the tracks below. Geometry stays in full-resolution source pixels, just
//! like every other frontend; [`MaskState::transform`] is the engine's own source-to-frame matrix
//! at the playhead, so Kotlin can draw and edit the same path without duplicating placement math.
//! Every mutation is an engine edit (and therefore saved, rendered and exported by the existing
//! pipelines). A nonempty `merge` token folds one continuous gesture into one undo step.

use filmcraft_engine::Session;
use filmcraft_geom::{Affine, Vec2};
use filmcraft_project::{ClipId, ItemKind, TrackKind};
use filmcraft_time::Tick;
use serde_json::{Value, json};

use crate::edits::ex;
use crate::{MobileError, model};

/// The most vertices a phone-created path may carry (free-draw is simplified before this bridge).
const MAX_VERTICES: usize = 2048;

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MaskShape {
    Ellipse,
    Rectangle,
    /// A five-sided starting shape, or the supplied free-draw points when they are not empty.
    Polygon,
}

/// How a mask combines with the masks before it. `None` keeps the mask but bypasses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MaskCombine {
    None,
    Add,
    Subtract,
    Intersect,
    Lighten,
    Darken,
    Difference,
}

impl MaskCombine {
    fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Add => "Add",
            Self::Subtract => "Subtract",
            Self::Intersect => "Intersect",
            Self::Lighten => "Lighten",
            Self::Darken => "Darken",
            Self::Difference => "Difference",
        }
    }

    fn from_label(s: &str) -> Option<Self> {
        [Self::None, Self::Add, Self::Subtract, Self::Intersect, Self::Lighten, Self::Darken, Self::Difference]
            .into_iter()
            .find(|m| m.label() == s)
    }
}

/// A point in source pixels (or a tangent relative to its vertex).
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct MaskPoint {
    pub x: f64,
    pub y: f64,
}

/// A cubic Bézier vertex. Zero handles make a sharp polygon corner.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct MaskVertex {
    pub point: MaskPoint,
    pub handle_in: MaskPoint,
    pub handle_out: MaskPoint,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MaskPath {
    pub vertices: Vec<MaskVertex>,
    pub closed: bool,
}

/// The editable, non-geometric mask controls. Feather and expansion are source pixels; opacity
/// is a percentage. Values are evaluated at the playhead when read.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct MaskSettings {
    pub combine: MaskCombine,
    pub inverted: bool,
    pub feather: f64,
    pub opacity: f64,
    pub expansion: f64,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MaskInfo {
    /// Index within Opacity's masks, not a project-wide id.
    pub index: u32,
    pub name: String,
    pub settings: MaskSettings,
    pub path: MaskPath,
    /// At least one mask parameter has keyframes; edits apply at the current playhead.
    pub animated: bool,
    /// Mask Path keyframes in source ticks, for the editor's animation hint.
    pub path_keyframes: Vec<i64>,
}

/// An affine map from source pixels to sequence pixels: `(a*x + c*y + e, b*x + d*y + f)`.
/// Includes Motion, Scale to Frame, pixel aspect ratio, and the phone's Mirror/Flip effects.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct MaskTransform {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

/// A consistent snapshot for the mask panel and its on-preview handles.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MaskState {
    pub revision: u64,
    pub time: i64,
    pub source_width: u32,
    pub source_height: u32,
    pub transform: MaskTransform,
    pub masks: Vec<MaskInfo>,
}

fn bad(message: impl Into<String>) -> MobileError {
    MobileError::Command(message.into())
}

fn finite(values: &[f64]) -> Result<(), MobileError> {
    if values.iter().all(|v| v.is_finite() && v.abs() <= 1.0e7) { Ok(()) } else { Err(bad("Mask coordinates and values must be finite numbers.")) }
}

fn merge_key(token: Option<String>, clip: u64, mask: u32, action: &str) -> Option<String> {
    token.filter(|s| !s.trim().is_empty()).map(|s| format!("phone:{clip}:{mask}:{action}:{s}"))
}

/// Resolve a picture clip and its source size. Audio, text and missing source dimensions cannot
/// take a phone clip mask; the toolbar likewise only offers the tool on video and images.
fn picture(s: &Session, clip: u64) -> Result<(u32, u32), MobileError> {
    let (_, seq) = model::sequence(s)?;
    let f = model::find(seq, ClipId(clip)).filter(|f| f.kind == TrackKind::Video)
        .ok_or_else(|| bad("Select a video or image to mask."))?;
    if s.project.item(f.clip.item).is_some_and(|p| matches!(p.kind, ItemKind::Graphic { .. })) {
        return Err(bad("Text has its own shape and style tools."));
    }
    filmcraft_render::source_size(&s.project, f.clip.item).ok_or_else(|| bad("That clip has no picture to mask."))
}

fn path_json(path: &MaskPath) -> Result<Value, MobileError> {
    if path.vertices.len() < 3 || path.vertices.len() > MAX_VERTICES {
        return Err(bad("A mask needs 3 to 2048 points."));
    }
    let values: Vec<f64> = path.vertices.iter().flat_map(|v| [v.point.x, v.point.y, v.handle_in.x, v.handle_in.y, v.handle_out.x, v.handle_out.y]).collect();
    finite(&values)?;
    Ok(json!({
        "closed": path.closed,
        "vertices": path.vertices.iter().map(|v| json!({
            "p": [v.point.x, v.point.y],
            "in": [v.handle_in.x, v.handle_in.y],
            "out": [v.handle_out.x, v.handle_out.y],
        })).collect::<Vec<_>>(),
    }))
}

fn read_path(v: &Value) -> Result<MaskPath, MobileError> {
    let p = filmcraft_engine::masks::path_from_json(v).ok_or_else(|| bad("The engine returned an invalid mask path."))?;
    let zero = MaskPoint { x: 0.0, y: 0.0 };
    Ok(MaskPath {
        closed: p.closed,
        vertices: p.vertices.into_iter().map(|v| MaskVertex {
            point: MaskPoint { x: v.p.x, y: v.p.y },
            handle_in: if v.t_in == Vec2::ZERO { zero } else { MaskPoint { x: v.t_in.x, y: v.t_in.y } },
            handle_out: if v.t_out == Vec2::ZERO { zero } else { MaskPoint { x: v.t_out.x, y: v.t_out.y } },
        }).collect(),
    })
}

fn read_info(v: &Value) -> Result<MaskInfo, MobileError> {
    let field = |key: &str| v.get(key).and_then(Value::as_f64).ok_or_else(|| bad("The engine returned an invalid mask."));
    let mode = v.get("mode").and_then(Value::as_str).and_then(MaskCombine::from_label)
        .ok_or_else(|| bad("The engine returned an unknown mask mode."))?;
    let index = u32::try_from(v.get("mask").and_then(Value::as_u64).unwrap_or(u64::MAX)).map_err(|_| bad("Too many masks on this clip."))?;
    Ok(MaskInfo {
        index,
        name: v.get("name").and_then(Value::as_str).unwrap_or("Mask").to_string(),
        settings: MaskSettings {
            combine: mode,
            inverted: v.get("inverted").and_then(Value::as_bool).unwrap_or(false),
            feather: field("feather")?,
            opacity: field("opacity")?,
            expansion: field("expansion")?,
        },
        path: read_path(v.get("path").unwrap_or(&Value::Null))?,
        animated: v.get("pathKeyframes").and_then(Value::as_array).is_some_and(|a| !a.is_empty()),
        path_keyframes: v.get("pathKeyframes").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_i64).collect()).unwrap_or_default(),
    })
}

fn transform(s: &Session, clip: u64, source: (u32, u32)) -> Result<MaskTransform, MobileError> {
    let (_, seq) = model::sequence(s)?;
    let item = model::find(seq, ClipId(clip)).map(|f| f.clip).ok_or_else(|| bad("That clip is gone."))?;
    let t = s.playhead().clamp(item.start, item.end() - Tick(1));
    let motion = filmcraft_render::motion_matrix(seq, item, source, filmcraft_render::source_par(&s.project, item.item), item.effect_time_at(t));
    // Opacity masks run after the phone's flip effects, but before Motion. A reflection about
    // the image edge maps pixel centres correctly (0.5 -> width - 0.5).
    let mut flip = Affine::IDENTITY;
    if item.effect("horizontal_flip").is_some_and(|e| e.enabled) {
        flip = Affine::translate(f64::from(source.0), 0.0).then_apply(&Affine::scale(-1.0, 1.0)).then_apply(&flip);
    }
    if item.effect("vertical_flip").is_some_and(|e| e.enabled) {
        flip = Affine::translate(0.0, f64::from(source.1)).then_apply(&Affine::scale(1.0, -1.0)).then_apply(&flip);
    }
    let m = motion.then_apply(&flip);
    Ok(MaskTransform { a: m.a, b: m.b, c: m.c, d: m.d, e: m.e, f: m.f })
}

pub(crate) fn state(s: &mut Session, clip: u64) -> Result<MaskState, MobileError> {
    let source = picture(s, clip)?;
    let time = s.playhead().0;
    let value = ex(s, "masks.list", json!({"clip": clip, "time": time}))?;
    let mut masks = value.get("masks").and_then(Value::as_array).ok_or_else(|| bad("The engine returned no mask list."))?
        .iter().filter(|v| v.get("effectId").and_then(Value::as_str) == Some("opacity"))
        .map(read_info).collect::<Result<Vec<_>, _>>()?;
    // `masks.list` exposes path keys; the model also tells us about animated feather/opacity/etc.
    let (_, seq) = model::sequence(s)?;
    let opacity = model::find(seq, ClipId(clip)).and_then(|f| f.clip.effect("opacity"));
    for m in &mut masks {
        m.animated |= opacity.and_then(|e| e.masks.get(m.index as usize)).is_some_and(filmcraft_project::Mask::is_animated);
    }
    Ok(MaskState { revision: s.revision, time, source_width: source.0, source_height: source.1, transform: transform(s, clip, source)?, masks })
}

/// Add a centered shape, or a closed polygon through `points` (source pixels). The new mask is
/// last in the combine stack. Empty points give the shape's useful default size.
pub(crate) fn add(s: &mut Session, clip: u64, shape: MaskShape, points: Vec<MaskPoint>) -> Result<MaskState, MobileError> {
    let (w, h) = picture(s, clip)?;
    if points.len() > MAX_VERTICES { return Err(bad("That drawing has too many points.")); }
    finite(&points.iter().flat_map(|p| [p.x, p.y]).collect::<Vec<_>>())?;
    let shape_name = match shape {
        MaskShape::Ellipse => "ellipse",
        MaskShape::Rectangle => "rectangle",
        MaskShape::Polygon => "polygon",
    };
    let mut params = json!({"clip": clip, "effect": "opacity", "shape": shape_name});
    if !points.is_empty() {
        if shape != MaskShape::Polygon { return Err(bad("Custom points need a polygon mask.")); }
        let path = MaskPath { vertices: points.into_iter().map(|point| MaskVertex { point, handle_in: MaskPoint { x: 0.0, y: 0.0 }, handle_out: MaskPoint { x: 0.0, y: 0.0 } }).collect(), closed: true };
        params["path"] = path_json(&path)?;
    } else if shape == MaskShape::Polygon {
        let (cx, cy, r) = (f64::from(w) / 2.0, f64::from(h) / 2.0, f64::from(w.min(h)) * 0.25);
        let points: Vec<Vec2> = (0..5).map(|i| {
            let a = -std::f64::consts::FRAC_PI_2 + i as f64 * std::f64::consts::TAU / 5.0;
            Vec2::new(cx + r * a.cos(), cy + r * a.sin())
        }).collect();
        params["path"] = filmcraft_engine::masks::path_to_json(&filmcraft_project::MaskPath::polygon(&points));
    }
    ex(s, "masks.add", params)?;
    state(s, clip)
}

pub(crate) fn set(s: &mut Session, clip: u64, mask: u32, settings: MaskSettings, merge: Option<String>) -> Result<MaskState, MobileError> {
    picture(s, clip)?;
    finite(&[settings.feather, settings.opacity, settings.expansion])?;
    let mut params = json!({
        "clip": clip, "effect": "opacity", "mask": mask, "mode": settings.combine.label(),
        "inverted": settings.inverted, "feather": settings.feather.clamp(0.0, 10_000.0),
        "opacity": settings.opacity.clamp(0.0, 100.0), "expansion": settings.expansion.clamp(-10_000.0, 10_000.0),
    });
    if let Some(key) = merge_key(merge, clip, mask, "settings") { params["merge"] = json!(key); }
    ex(s, "masks.set", params)?;
    state(s, clip)
}

pub(crate) fn move_mask(s: &mut Session, clip: u64, mask: u32, dx: f64, dy: f64, merge: Option<String>) -> Result<MaskState, MobileError> {
    picture(s, clip)?;
    finite(&[dx, dy])?;
    let mut params = json!({"clip": clip, "effect": "opacity", "mask": mask, "delta": [dx, dy]});
    if let Some(key) = merge_key(merge, clip, mask, "move") { params["merge"] = json!(key); }
    ex(s, "masks.translate", params)?;
    state(s, clip)
}

/// Replace the path at the playhead (the engine updates an existing keyframe when animated).
pub(crate) fn set_path(s: &mut Session, clip: u64, mask: u32, path: MaskPath, merge: Option<String>) -> Result<MaskState, MobileError> {
    picture(s, clip)?;
    let mut params = json!({"clip": clip, "effect": "opacity", "mask": mask, "path": path_json(&path)?});
    if let Some(key) = merge_key(merge, clip, mask, "path") { params["merge"] = json!(key); }
    ex(s, "masks.set", params)?;
    state(s, clip)
}

pub(crate) fn add_vertex(s: &mut Session, clip: u64, mask: u32, after: u32, at: MaskPoint) -> Result<MaskState, MobileError> {
    let st = state(s, clip)?;
    let count = st.masks.iter().find(|m| m.index == mask).map_or(0, |m| m.path.vertices.len());
    if count >= MAX_VERTICES { return Err(bad("A mask can have at most 2048 points.")); }
    finite(&[at.x, at.y])?;
    ex(s, "masks.addVertex", json!({"clip": clip, "effect": "opacity", "mask": mask, "after": after, "at": [at.x, at.y]}))?;
    state(s, clip)
}

pub(crate) fn remove_vertex(s: &mut Session, clip: u64, mask: u32, vertex: u32) -> Result<MaskState, MobileError> {
    picture(s, clip)?;
    ex(s, "masks.removeVertex", json!({"clip": clip, "effect": "opacity", "mask": mask, "vertex": vertex}))?;
    state(s, clip)
}

pub(crate) fn remove(s: &mut Session, clip: u64, mask: u32) -> Result<MaskState, MobileError> {
    picture(s, clip)?;
    ex(s, "masks.remove", json!({"clip": clip, "effect": "opacity", "mask": mask}))?;
    state(s, clip)
}

/// Remove every clip mask as one undoable edit, leaving filters and other effects alone.
pub(crate) fn clear(s: &mut Session, clip: u64) -> Result<MaskState, MobileError> {
    let st = state(s, clip)?;
    if !st.masks.is_empty() {
        s.edit_group("Clear Masks", |s| {
            for m in st.masks.iter().rev() {
                s.execute("masks.remove", json!({"clip": clip, "effect": "opacity", "mask": m.index}))?;
            }
            Ok(())
        })?;
    }
    state(s, clip)
}
