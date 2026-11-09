//! Advanced phone tools: 3D objects, text extrusion, motion tracking and background removal.
//!
//! Everything here is a thin typed shell over Rust project/render data. 3D objects are
//! `object_3d` effect instances on graphic clips; text extrusion converts a text layer into that
//! effect while carrying the picked style. Motion tracks live on the sequence and are sampled by
//! the renderer when an object's Follow Motion Track is on. Background removal is a point-seeded
//! `background_removal` effect.

use std::sync::Arc;

use filmcraft_engine::{EngineError, Session};
use filmcraft_media::FrameRequest;
use filmcraft_project::{ClipId, EffectInstance, ItemKind, ParamValue, Sequence, TrackItem, find_effect, graphic};
use filmcraft_render::track3d::{Gray, track_sequence};
use filmcraft_time::Tick;
use serde_json::json;

use crate::edits::{self, STILL_SECONDS, ex};
use crate::model;
use crate::MobileError;

const OBJECT3D: &str = "object_3d";
const BACKGROUND: &str = "background_removal";

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Object3DMesh {
    Cube,
    Sphere,
    Cylinder,
    Cone,
    Torus,
    Plane,
    Text,
}

impl Object3DMesh {
    fn index(self) -> u32 {
        match self {
            Self::Cube => 0,
            Self::Sphere => 1,
            Self::Cylinder => 2,
            Self::Cone => 3,
            Self::Torus => 4,
            Self::Plane => 5,
            Self::Text => 6,
        }
    }
    fn from_index(i: u32) -> Self {
        match i {
            1 => Self::Sphere,
            2 => Self::Cylinder,
            3 => Self::Cone,
            4 => Self::Torus,
            5 => Self::Plane,
            6 => Self::Text,
            _ => Self::Cube,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum StereoLayout {
    Mono,
    SideBySide,
    TopBottom,
}

impl StereoLayout {
    fn project(self) -> filmcraft_project::StereoLayout {
        match self {
            Self::Mono => filmcraft_project::StereoLayout::Mono,
            Self::SideBySide => filmcraft_project::StereoLayout::SideBySide,
            Self::TopBottom => filmcraft_project::StereoLayout::TopBottom,
        }
    }
    fn from_project(l: filmcraft_project::StereoLayout) -> Self {
        match l {
            filmcraft_project::StereoLayout::Mono => Self::Mono,
            filmcraft_project::StereoLayout::SideBySide => Self::SideBySide,
            filmcraft_project::StereoLayout::TopBottom => Self::TopBottom,
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct NormPoint {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Object3DSettings {
    pub mesh: Object3DMesh,
    pub text: String,
    pub font: String,
    pub font_style: String,
    pub align: u32,
    pub tracking: f64,
    pub kerning: bool,
    pub ligatures: bool,
    pub leading: f64,
    pub baseline_shift: f64,
    pub faux_bold: bool,
    pub faux_italic: bool,
    pub caps: u32,
    pub underline: bool,
    pub box_width: f64,
    pub vertical: bool,
    pub size: f64,
    pub depth: f64,
    pub bevel: f64,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub rotation_x: f64,
    pub rotation_y: f64,
    pub rotation_z: f64,
    pub scale: f64,
    /// ARGB.
    pub color: u32,
    pub side_color: u32,
    pub metalness: f64,
    pub roughness: f64,
    pub light_angle: f64,
    pub light_elevation: f64,
    pub opacity: f64,
    pub follow_track: bool,
    pub track_id: u64,
    pub track_depth: f64,
}

impl Default for Object3DSettings {
    fn default() -> Self {
        Self {
            mesh: Object3DMesh::Cube,
            text: String::new(),
            font: "Inter".into(),
            font_style: "Regular".into(),
            align: 1,
            tracking: 0.0,
            kerning: true,
            ligatures: true,
            leading: 0.0,
            baseline_shift: 0.0,
            faux_bold: false,
            faux_italic: false,
            caps: 0,
            underline: false,
            box_width: 0.0,
            vertical: false,
            size: 220.0,
            depth: 48.0,
            bevel: 4.0,
            x: 0.0,
            y: 0.0,
            z: 700.0,
            rotation_x: 0.0,
            rotation_y: 0.0,
            rotation_z: 0.0,
            scale: 100.0,
            color: 0xff_eb_73_33,
            side_color: 0xff_73_2e_1a,
            metalness: 25.0,
            roughness: 45.0,
            light_angle: 135.0,
            light_elevation: 45.0,
            opacity: 100.0,
            follow_track: false,
            track_id: 0,
            track_depth: 240.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Object3DState {
    pub clip: u64,
    pub has_object: bool,
    pub animated: bool,
    pub settings: Object3DSettings,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct TrackOptions {
    /// Target in the tracked view, normalised 0..1.
    pub target_x: f64,
    pub target_y: f64,
    pub patch_size: u32,
    pub stereo: StereoLayout,
    /// Maximum stereoscopic disparity in working pixels (depth sensitivity).
    pub baseline: f64,
    pub max_seconds: f64,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MotionTrackInfo {
    pub id: u64,
    pub clip: u64,
    pub name: String,
    pub layout: StereoLayout,
    pub samples: u32,
    pub start: i64,
    pub end: i64,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct BackgroundSettings {
    pub threshold: f64,
    pub feather: f64,
    pub smooth: f64,
    pub invert: bool,
    pub point_radius: f64,
    pub point_strength: f64,
    pub keep_points: Vec<NormPoint>,
    pub remove_points: Vec<NormPoint>,
}

impl Default for BackgroundSettings {
    fn default() -> Self {
        Self { threshold: 38.0, feather: 6.0, smooth: 2.0, invert: false, point_radius: 12.0, point_strength: 70.0, keep_points: Vec::new(), remove_points: Vec::new() }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct BackgroundState {
    pub clip: u64,
    pub enabled: bool,
    pub settings: BackgroundSettings,
}

fn other(msg: impl Into<String>) -> EngineError {
    EngineError::Other(msg.into())
}

fn seq_id(s: &Session) -> Result<filmcraft_project::ItemId, MobileError> {
    s.state.active_sequence.ok_or(MobileError::NoSequence)
}

fn edit_seq<R>(
    s: &mut Session,
    label: &str,
    merge: Option<&str>,
    f: impl FnOnce(&mut Sequence) -> filmcraft_engine::Result<R>,
) -> Result<R, MobileError> {
    let id = seq_id(s)?;
    let r = if let Some(key) = merge {
        s.edit_merged(label, key, |p, _| {
            let q = p.sequence_mut(id).ok_or(EngineError::NoSequence)?;
            f(q)
        })?
    } else {
        s.edit(label, |p, _| {
            let q = p.sequence_mut(id).ok_or(EngineError::NoSequence)?;
            f(q)
        })?
    };
    Ok(r)
}

fn clip_mut<'a>(q: &'a mut Sequence, id: u64) -> filmcraft_engine::Result<&'a mut TrackItem> {
    q.find_item_mut(ClipId(id)).map(|(_, c)| c).ok_or_else(|| other("That clip is gone."))
}

fn effect_mut<'a>(c: &'a mut TrackItem, id: &str) -> filmcraft_engine::Result<&'a mut EffectInstance> {
    c.effects.iter_mut().find(|e| e.effect == id).ok_or_else(|| other(format!("This clip has no {id}.")))
}

fn instance(id: &str) -> filmcraft_engine::Result<EffectInstance> {
    find_effect(id).map(|d| d.instance()).ok_or_else(|| other(format!("Unknown effect {id}.")))
}

fn insert_before_intrinsic(c: &mut TrackItem, e: EffectInstance) {
    let pos = c.effects.iter().position(|x| x.def().is_some_and(|d| d.intrinsic)).unwrap_or(c.effects.len());
    c.effects.insert(pos, e);
}

fn finite(v: f64, d: f64) -> f64 {
    if v.is_finite() { v } else { d }
}

fn argb_to_rgba(c: u32) -> [f32; 4] {
    let b = |x: u32| (x & 0xff) as f32 / 255.0;
    [b(c >> 16), b(c >> 8), b(c), b(c >> 24)]
}

fn rgba_to_argb(c: [f32; 4]) -> u32 {
    let b = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u32;
    (b(c[3]) << 24) | (b(c[0]) << 16) | (b(c[1]) << 8) | b(c[2])
}

fn set_f(e: &mut EffectInstance, id: &str, v: f64, t: Tick) {
    if let Some(p) = e.params.get_mut(id) {
        p.set_at(t, ParamValue::Float(finite(v, 0.0)));
    }
}

fn set_text(e: &mut EffectInstance, id: &str, v: &str) {
    if let Some(p) = e.params.get_mut(id) {
        p.value = ParamValue::Text(v.to_string());
    }
}

fn set_color(e: &mut EffectInstance, id: &str, v: u32, t: Tick) {
    if let Some(p) = e.params.get_mut(id) {
        p.set_at(t, ParamValue::Color(argb_to_rgba(v)));
    }
}

fn set_bool(e: &mut EffectInstance, id: &str, v: bool) {
    if let Some(p) = e.params.get_mut(id) {
        p.value = ParamValue::Bool(v);
    }
}

fn set_choice(e: &mut EffectInstance, id: &str, v: u32) {
    if let Some(p) = e.params.get_mut(id) {
        p.value = ParamValue::Choice(v);
    }
}

fn bool_at(e: &EffectInstance, id: &str, d: bool) -> bool {
    e.param(id).and_then(|p| p.value.as_bool()).unwrap_or(d)
}

fn choice_at(e: &EffectInstance, id: &str, d: u32) -> u32 {
    e.param(id).and_then(|p| p.value.as_f64()).map(|v| v.max(0.0) as u32).unwrap_or(d)
}

fn f_at(e: &EffectInstance, id: &str, t: Tick, d: f64) -> f64 {
    e.param(id).map(|p| p.f64_at(t)).filter(|v| v.is_finite()).unwrap_or(d)
}

fn text_at(e: &EffectInstance, id: &str, t: Tick) -> String {
    match e.param(id).map(|p| p.value_at(t)) {
        Some(ParamValue::Text(s)) => s,
        _ => String::new(),
    }
}

fn color_at(e: &EffectInstance, id: &str, t: Tick, d: u32) -> u32 {
    e.param(id).and_then(|p| p.value_at(t).as_color()).map(rgba_to_argb).unwrap_or(d)
}

fn points_at(e: &EffectInstance, id: &str) -> Vec<NormPoint> {
    match e.param(id).map(|p| &p.value) {
        Some(ParamValue::Curve(c)) => c.iter().map(|p| NormPoint { x: p[0] as f64, y: p[1] as f64 }).collect(),
        _ => Vec::new(),
    }
}

fn set_points(e: &mut EffectInstance, id: &str, pts: &[NormPoint]) {
    if let Some(p) = e.params.get_mut(id) {
        p.value = ParamValue::Curve(pts.iter().take(512).map(|p| [finite(p.x, 0.5).clamp(0.0, 1.0) as f32, finite(p.y, 0.5).clamp(0.0, 1.0) as f32]).collect());
    }
}

fn object_settings(e: &EffectInstance, t: Tick, center: (f64, f64)) -> Object3DSettings {
    let pos = e.param("position").map(|p| p.vec2_at(t)).unwrap_or_default();
    let d = Object3DSettings::default();
    Object3DSettings {
        mesh: Object3DMesh::from_index(e.param("mesh").and_then(|p| p.value.as_f64()).unwrap_or(0.0) as u32),
        text: text_at(e, "text", t),
        font: text_at(e, "font", t),
        font_style: text_at(e, "font_style", t),
        align: choice_at(e, "align", d.align),
        tracking: f_at(e, "tracking", t, d.tracking),
        kerning: bool_at(e, "kerning", d.kerning),
        ligatures: bool_at(e, "ligatures", d.ligatures),
        leading: f_at(e, "leading", t, d.leading),
        baseline_shift: f_at(e, "baseline_shift", t, d.baseline_shift),
        faux_bold: bool_at(e, "faux_bold", d.faux_bold),
        faux_italic: bool_at(e, "faux_italic", d.faux_italic),
        caps: choice_at(e, "caps", d.caps),
        underline: bool_at(e, "underline", d.underline),
        box_width: f_at(e, "box_width", t, d.box_width),
        vertical: bool_at(e, "vertical", d.vertical),
        size: f_at(e, "size", t, d.size),
        depth: f_at(e, "depth", t, d.depth),
        bevel: f_at(e, "bevel", t, d.bevel),
        x: if pos.x.is_nan() { center.0 } else { pos.x },
        y: if pos.y.is_nan() { center.1 } else { pos.y },
        z: f_at(e, "z", t, d.z),
        rotation_x: f_at(e, "rotation_x", t, 0.0),
        rotation_y: f_at(e, "rotation_y", t, 0.0),
        rotation_z: f_at(e, "rotation_z", t, 0.0),
        scale: f_at(e, "scale", t, d.scale),
        color: color_at(e, "color", t, d.color),
        side_color: color_at(e, "side_color", t, d.side_color),
        metalness: f_at(e, "metalness", t, d.metalness),
        roughness: f_at(e, "roughness", t, d.roughness),
        light_angle: f_at(e, "light_angle", t, d.light_angle),
        light_elevation: f_at(e, "light_elevation", t, d.light_elevation),
        opacity: f_at(e, "opacity", t, d.opacity),
        follow_track: e.param("follow_track").and_then(|p| p.value.as_bool()).unwrap_or(false),
        track_id: f_at(e, "track_id", t, 0.0).max(0.0) as u64,
        track_depth: f_at(e, "track_depth", t, d.track_depth),
    }
}

fn write_object(e: &mut EffectInstance, st: &Object3DSettings, t: Tick) {
    if let Some(p) = e.params.get_mut("mesh") {
        p.value = ParamValue::Choice(st.mesh.index());
    }
    set_text(e, "text", &st.text);
    set_text(e, "font", &st.font);
    set_text(e, "font_style", &st.font_style);
    set_choice(e, "align", st.align.min(3));
    set_f(e, "tracking", st.tracking.clamp(-1000.0, 10_000.0), t);
    set_bool(e, "kerning", st.kerning);
    set_bool(e, "ligatures", st.ligatures);
    set_f(e, "leading", st.leading.clamp(-5000.0, 5000.0), t);
    set_f(e, "baseline_shift", st.baseline_shift.clamp(-5000.0, 5000.0), t);
    set_bool(e, "faux_bold", st.faux_bold);
    set_bool(e, "faux_italic", st.faux_italic);
    set_choice(e, "caps", st.caps.min(2));
    set_bool(e, "underline", st.underline);
    set_f(e, "box_width", st.box_width.clamp(0.0, 10_000.0), t);
    set_bool(e, "vertical", st.vertical);
    set_f(e, "size", st.size.clamp(1.0, 10_000.0), t);
    set_f(e, "depth", st.depth.clamp(0.0, 5000.0), t);
    set_f(e, "bevel", st.bevel.clamp(0.0, 1000.0), t);
    if let Some(p) = e.params.get_mut("position") {
        p.set_at(t, ParamValue::Vec2(filmcraft_geom::Vec2::new(finite(st.x, 0.0), finite(st.y, 0.0))));
    }
    set_f(e, "z", st.z.clamp(20.0, 20_000.0), t);
    set_f(e, "rotation_x", finite(st.rotation_x, 0.0), t);
    set_f(e, "rotation_y", finite(st.rotation_y, 0.0), t);
    set_f(e, "rotation_z", finite(st.rotation_z, 0.0), t);
    set_f(e, "scale", st.scale.clamp(0.0, 10_000.0), t);
    set_color(e, "color", st.color, t);
    set_color(e, "side_color", st.side_color, t);
    set_f(e, "metalness", st.metalness.clamp(0.0, 100.0), t);
    set_f(e, "roughness", st.roughness.clamp(0.0, 100.0), t);
    set_f(e, "light_angle", finite(st.light_angle, 135.0), t);
    set_f(e, "light_elevation", st.light_elevation.clamp(-90.0, 90.0), t);
    set_f(e, "opacity", st.opacity.clamp(0.0, 100.0), t);
    set_bool(e, "follow_track", st.follow_track);
    set_f(e, "track_id", st.track_id as f64, t);
    set_f(e, "track_depth", st.track_depth.clamp(-5000.0, 5000.0), t);
}

const ANIMATED_PARAMS: &[&str] = &[
    "position", "z", "rotation_x", "rotation_y", "rotation_z", "scale", "depth", "bevel", "color", "side_color", "metalness", "roughness", "light_angle", "light_elevation", "opacity", "track_depth",
];

fn object_animated(e: &EffectInstance) -> bool {
    ANIMATED_PARAMS.iter().any(|id| e.param(id).is_some_and(|p| p.is_animated()))
}

pub(crate) fn add_object(s: &mut Session, mesh: Object3DMesh) -> Result<u64, MobileError> {
    let (_, seq) = model::sequence(s)?;
    let (w, h) = (seq.settings.width, seq.settings.height);
    let rate = s.sequence_rate();
    let at = rate.snap(s.playhead());
    let end = at + rate.snap_nearest(Tick::from_seconds_f64(STILL_SECONDS));
    let track = edits::free_lane_index(s, filmcraft_project::TrackKind::Video, 1, None, at, end, None)?;
    let clip = s.edit_group("Add 3D Object", |s| {
        let r = s.execute(
            "graphics.newText",
            json!({"text": "3D", "time": at.0, "seconds": STILL_SECONDS, "size": 100.0, "font": "Inter", "fontStyle": "Regular", "box": [w, 1], "track": track}),
        )?;
        let id = r["clip"].as_u64().ok_or_else(|| other("The 3D object could not be added."))?;
        let sid = s.state.active_sequence.ok_or(EngineError::NoSequence)?;
        let p = Arc::make_mut(&mut s.project);
        let q = p.sequence_mut(sid).ok_or(EngineError::NoSequence)?;
        let c = clip_mut(q, id)?;
        c.effects.retain(|e| !graphic::is_layer(e));
        let mut e = instance(OBJECT3D)?;
        if let Some(pm) = e.params.get_mut("mesh") {
            pm.value = ParamValue::Choice(mesh.index());
        }
        if mesh == Object3DMesh::Text {
            set_text(&mut e, "text", "Text");
        } else {
            // Primitives start as deep as they are wide; the Extrusion slider then changes thickness.
            set_f(&mut e, "depth", 220.0, Tick::ZERO);
        }
        insert_before_intrinsic(c, e);
        c.name = format!("3D {}", match mesh {
            Object3DMesh::Cube => "Cube",
            Object3DMesh::Sphere => "Sphere",
            Object3DMesh::Cylinder => "Cylinder",
            Object3DMesh::Cone => "Cone",
            Object3DMesh::Torus => "Torus",
            Object3DMesh::Plane => "Plane",
            Object3DMesh::Text => "Text",
        });
        edits::settle(s);
        Ok(id)
    })?;
    ex(s, "timeline.select", json!({"clips": [clip]}))?;
    Ok(clip)
}

pub(crate) fn object_state(s: &Session, id: u64) -> Result<Object3DState, MobileError> {
    let (_, seq) = model::sequence(s)?;
    let c = seq.find_item(ClipId(id)).map(|(_, c)| c).ok_or_else(|| MobileError::Command("That clip is gone.".into()))?;
    let t = c.effect_time_at(s.playhead());
    let center = (f64::from(seq.settings.width) / 2.0, f64::from(seq.settings.height) / 2.0);
    Ok(match c.effects.iter().find(|e| e.effect == OBJECT3D) {
        Some(e) => Object3DState { clip: id, has_object: true, animated: object_animated(e), settings: object_settings(e, t, center) },
        None => Object3DState { clip: id, has_object: false, animated: false, settings: Object3DSettings::default() },
    })
}

pub(crate) fn set_object(s: &mut Session, id: u64, st: &Object3DSettings, merge: Option<&str>) -> Result<(), MobileError> {
    let t = s.playhead();
    edit_seq(s, "Edit 3D Object", merge, |q| {
        let c = clip_mut(q, id)?;
        let mt = c.effect_time_at(t);
        let e = effect_mut(c, OBJECT3D)?;
        write_object(e, st, mt);
        Ok(())
    })
}

pub(crate) fn toggle_object_animation(s: &mut Session, id: u64) -> Result<bool, MobileError> {
    let t = s.playhead();
    edit_seq(s, "Toggle 3D Animation", None, |q| {
        let c = clip_mut(q, id)?;
        let mt = c.effect_time_at(t);
        let e = effect_mut(c, OBJECT3D)?;
        let target = !object_animated(e);
        for id in ANIMATED_PARAMS {
            if let Some(p) = e.params.get_mut(*id) {
                if p.is_animated() != target {
                    p.toggle_animation(mt);
                }
            }
        }
        Ok(target)
    })
}

pub(crate) fn extrude_text(s: &mut Session, id: u64, depth: f64, bevel: f64) -> Result<u64, MobileError> {
    let ph = s.playhead();
    edit_seq(s, "Extrude Text", None, |q| {
        let frame = (q.settings.width, q.settings.height);
        let c = clip_mut(q, id)?;
        let t = c.effect_time_at(ph);
        let layer = c.effects.iter().find(|e| e.effect == graphic::TEXT_LAYER).ok_or_else(|| other("Select a text clip to extrude."))?;
        let spec = graphic::eval_layer(layer, t, frame).ok_or_else(|| other("This text layer could not be read."))?;
        let props = match &spec.content {
            graphic::LayerContent::Text(t) => t.clone(),
            _ => return Err(other("Select a text clip to extrude.")),
        };
        let mut e = instance(OBJECT3D)?;
        set_choice(&mut e, "mesh", Object3DMesh::Text.index());
        set_text(&mut e, "text", &props.text);
        set_text(&mut e, "font", &props.font);
        set_text(&mut e, "font_style", &props.style);
        set_choice(&mut e, "align", props.align);
        set_f(&mut e, "tracking", f64::from(props.tracking), t);
        set_bool(&mut e, "kerning", props.kerning);
        set_bool(&mut e, "ligatures", props.ligatures);
        set_f(&mut e, "leading", f64::from(props.leading), t);
        set_f(&mut e, "baseline_shift", f64::from(props.baseline_shift), t);
        set_bool(&mut e, "faux_bold", props.faux_bold);
        set_bool(&mut e, "faux_italic", props.faux_italic);
        set_choice(&mut e, "caps", props.caps);
        set_bool(&mut e, "underline", props.underline);
        set_f(&mut e, "box_width", f64::from(props.box_width), t);
        set_bool(&mut e, "vertical", props.vertical);
        set_f(&mut e, "size", f64::from(props.size), t);
        set_f(&mut e, "depth", depth.clamp(1.0, 2000.0), t);
        set_f(&mut e, "bevel", bevel.clamp(0.0, 500.0), t);
        let fill = spec.appearance.fill.unwrap_or([1.0; 4]);
        let side = spec.appearance.strokes.last().map(|(c, _, _)| *c).unwrap_or([fill[0] * 0.45, fill[1] * 0.45, fill[2] * 0.45, fill[3]]);
        if let Some(p) = e.params.get_mut("color") {
            p.value = ParamValue::Color(fill);
        }
        if let Some(p) = e.params.get_mut("side_color") {
            p.value = ParamValue::Color(side);
        }
        if let Some(p) = e.params.get_mut("position") {
            p.value = ParamValue::Vec2(spec.transform.position);
        }
        set_f(&mut e, "rotation_z", spec.transform.rotation, t);
        set_f(&mut e, "scale", spec.transform.scale.y * 100.0, t);
        set_f(&mut e, "opacity", f64::from(spec.transform.opacity) * 100.0, t);
        c.effects.retain(|x| x.effect != graphic::TEXT_LAYER);
        insert_before_intrinsic(c, e);
        Ok(id)
    })
}

fn view_rect(layout: StereoLayout, w: usize, h: usize) -> (usize, usize, usize, usize) {
    match layout {
        StereoLayout::Mono => (0, 0, w, h),
        StereoLayout::SideBySide => (0, 0, w / 2, h),
        StereoLayout::TopBottom => (0, 0, w, h / 2),
    }
}

fn crop_gray(g: &Gray, x0: usize, y0: usize, w: usize, h: usize) -> Gray {
    let w = w.max(1);
    let h = h.max(1);
    let mut p = vec![0.0; w * h];
    for y in 0..h {
        for x in 0..w {
            let sx = (x0 + x).min(g.w.saturating_sub(1));
            let sy = (y0 + y).min(g.h.saturating_sub(1));
            p[y * w + x] = g.p[sy * g.w + sx];
        }
    }
    Gray { w, h, p }
}

pub(crate) fn track_motion(s: &mut Session, id: u64, opts: &TrackOptions) -> Result<MotionTrackInfo, MobileError> {
    let (sid, seq) = model::sequence(s)?;
    let c = seq.find_item(ClipId(id)).map(|(_, c)| c.clone()).ok_or_else(|| MobileError::Command("That clip is gone.".into()))?;
    let pi = s.project.item(c.item).ok_or_else(|| MobileError::Command("That clip has no media.".into()))?;
    if !matches!(&pi.kind, ItemKind::Media(m) if m.info.has_video()) {
        return Err(MobileError::Command("Tracking needs a video clip.".into()));
    }
    let rate = s.sequence_rate();
    let frame = rate.frame_duration().max(Tick(1));
    let start = s.playhead().max(c.start).min(c.end() - frame);
    let max_ticks = Tick::from_seconds_f64(finite(opts.max_seconds, 10.0).clamp(0.5, 60.0));
    let end = c.end().min(start + max_ticks);
    let provider = s.media.provider(s.project.clone(), s.services.clone());
    let source = provider.source(c.item).ok_or_else(|| MobileError::Command("The clip's media could not be opened.".into()))?;
    let (src_w, src_h) = source.info().video.as_ref().map(|v| (v.width, v.height)).unwrap_or((seq.settings.width, seq.settings.height));
    let scale = (480.0 / src_w.max(src_h).max(1) as f32).min(1.0);
    let mut left = Vec::new();
    let mut right = Vec::new();
    let mut times = Vec::new();
    let mut t = start;
    let mut view = (0usize, 0usize, 1usize, 1usize);
    let mut full = (1usize, 1usize);
    while t < end && times.len() < 2000 {
        let st = c.source_time_at(t);
        let Ok(f) = source.video_frame(FrameRequest { time: st, scale }) else { break };
        let rgba = f.to_rgba8();
        let g = Gray::from_rgba8(&rgba, f.width as usize, f.height as usize);
        full = (g.w, g.h);
        let (x0, y0, vw, vh) = view_rect(opts.stereo, g.w, g.h);
        view = (x0, y0, vw, vh);
        left.push(crop_gray(&g, x0, y0, vw, vh));
        if opts.stereo != StereoLayout::Mono {
            let (rx, ry) = match opts.stereo {
                StereoLayout::SideBySide => (vw, 0),
                StereoLayout::TopBottom => (0, vh),
                StereoLayout::Mono => (0, 0),
            };
            right.push(crop_gray(&g, rx, ry, vw, vh));
        }
        times.push(t - c.start);
        t += frame;
    }
    if left.len() < 2 {
        return Err(MobileError::Command("Not enough frames to track.".into()));
    }
    let (vx, vy, vw, vh) = view;
    let tx = (finite(opts.target_x, 0.5).clamp(0.0, 1.0) * vw as f64) as f32;
    let ty = (finite(opts.target_y, 0.5).clamp(0.0, 1.0) * vh as f64) as f32;
    let pts = track_sequence(&left, if right.is_empty() { None } else { Some(&right) }, (tx, ty), opts.patch_size as usize, finite(opts.baseline, 64.0).clamp(4.0, 256.0) as f32);
    let samples: Vec<filmcraft_project::TrackSample> = pts
        .iter()
        .zip(times)
        .map(|(p, time)| filmcraft_project::TrackSample {
            time,
            x: ((vx as f32 + p.x) / full.0.max(1) as f32).clamp(0.0, 1.0),
            y: ((vy as f32 + p.y) / full.1.max(1) as f32).clamp(0.0, 1.0),
            z: p.z,
            scale: p.scale,
            rotation: p.rotation,
        })
        .collect();
    let count = samples.len() as u32;
    let first = samples.first().map(|s| s.time).unwrap_or(Tick::ZERO);
    let last = samples.last().map(|s| s.time).unwrap_or(Tick::ZERO);
    let layout = opts.stereo;
    let name = format!("Track {}", seq.motion_tracks.len() + 1);
    let track_name = name.clone();
    let track_id = s.edit("Track Motion", |p, _| {
        let id = p.alloc_id();
        let q = p.sequence_mut(sid).ok_or(EngineError::NoSequence)?;
        q.motion_tracks.push(filmcraft_project::MotionTrack { id, clip: c.id, name: track_name, layout: layout.project(), samples });
        Ok(id)
    })?;
    Ok(MotionTrackInfo { id: track_id, clip: c.id.0, name, layout, samples: count, start: c.start.0 + first.0, end: c.start.0 + last.0 })
}

pub(crate) fn motion_tracks(s: &Session) -> Result<Vec<MotionTrackInfo>, MobileError> {
    let (_, seq) = model::sequence(s)?;
    Ok(seq
        .motion_tracks
        .iter()
        .map(|t| MotionTrackInfo {
            id: t.id,
            clip: t.clip.0,
            name: t.name.clone(),
            layout: StereoLayout::from_project(t.layout),
            samples: t.samples.len() as u32,
            start: seq.find_item(t.clip).map(|(_, c)| c.start.0).unwrap_or(0) + t.samples.first().map(|s| s.time.0).unwrap_or(0),
            end: seq.find_item(t.clip).map(|(_, c)| c.start.0).unwrap_or(0) + t.samples.last().map(|s| s.time.0).unwrap_or(0),
        })
        .collect())
}

pub(crate) fn delete_motion_track(s: &mut Session, id: u64) -> Result<(), MobileError> {
    edit_seq(s, "Delete Motion Track", None, |q| {
        q.motion_tracks.retain(|t| t.id != id);
        Ok(())
    })
}

pub(crate) fn remove_background(s: &mut Session, id: u64) -> Result<BackgroundState, MobileError> {
    edit_seq(s, "Remove Background", None, |q| {
        let c = clip_mut(q, id)?;
        if !c.effects.iter().any(|e| e.effect == BACKGROUND) {
            insert_before_intrinsic(c, instance(BACKGROUND)?);
        }
        if let Some(e) = c.effects.iter_mut().find(|e| e.effect == BACKGROUND) {
            e.enabled = true;
        }
        Ok(())
    })?;
    background_state(s, id)
}

pub(crate) fn background_state(s: &Session, id: u64) -> Result<BackgroundState, MobileError> {
    let (_, seq) = model::sequence(s)?;
    let c = seq.find_item(ClipId(id)).map(|(_, c)| c).ok_or_else(|| MobileError::Command("That clip is gone.".into()))?;
    let t = c.effect_time_at(s.playhead());
    Ok(match c.effects.iter().find(|e| e.effect == BACKGROUND) {
        Some(e) => BackgroundState {
            clip: id,
            enabled: e.enabled,
            settings: BackgroundSettings {
                threshold: f_at(e, "threshold", t, 38.0),
                feather: f_at(e, "feather", t, 6.0),
                smooth: f_at(e, "smooth", t, 2.0),
                invert: bool_at(e, "invert", false),
                point_radius: f_at(e, "point_radius", t, 12.0),
                point_strength: f_at(e, "point_strength", t, 70.0),
                keep_points: points_at(e, "keep_points"),
                remove_points: points_at(e, "remove_points"),
            },
        },
        None => BackgroundState { clip: id, enabled: false, settings: BackgroundSettings::default() },
    })
}

pub(crate) fn set_background(s: &mut Session, id: u64, st: &BackgroundSettings, merge: Option<&str>) -> Result<(), MobileError> {
    let ph = s.playhead();
    edit_seq(s, "Edit Background Removal", merge, |q| {
        let c = clip_mut(q, id)?;
        let t = c.effect_time_at(ph);
        if !c.effects.iter().any(|e| e.effect == BACKGROUND) {
            insert_before_intrinsic(c, instance(BACKGROUND)?);
        }
        let e = effect_mut(c, BACKGROUND)?;
        e.enabled = true;
        set_f(e, "threshold", st.threshold.clamp(1.0, 100.0), t);
        set_f(e, "feather", st.feather.clamp(0.0, 200.0), t);
        set_f(e, "smooth", st.smooth.clamp(0.0, 20.0), t);
        set_bool(e, "invert", st.invert);
        set_f(e, "point_radius", st.point_radius.clamp(0.0, 100.0), t);
        set_f(e, "point_strength", st.point_strength.clamp(0.0, 100.0), t);
        set_points(e, "keep_points", &st.keep_points);
        set_points(e, "remove_points", &st.remove_points);
        Ok(())
    })
}
