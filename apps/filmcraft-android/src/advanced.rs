//! Advanced phone tools: 3D objects, text extrusion, motion tracking and background removal.
//!
//! Everything here is a thin typed shell over Rust project/render data. 3D objects are
//! `object_3d` effect instances on graphic clips; text extrusion converts a text layer into that
//! effect while carrying the picked style. Motion tracks live on the sequence and are sampled by
//! the renderer when an object's Follow Motion Track is on. Background removal is a point-seeded
//! `background_removal` effect.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use filmcraft_engine::{EngineError, Session};
use filmcraft_media::{FrameRequest, SharedSource};
use filmcraft_project::{ClipId, EffectInstance, ItemKind, ParamValue, Sequence, TrackItem, find_effect, graphic};
use filmcraft_render::SourceProvider;
use filmcraft_render::matchmove::{InputFrame, MAX_SOLVER_FPS, SolveError, SolveOptions, SolveReport, solve_camera};
use filmcraft_render::track3d::{Tracker, view_rect};
use filmcraft_time::Tick;
use serde_json::json;

use crate::MobileError;
use crate::edits::{self, STILL_SECONDS, ex};
use crate::model;

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
    Model,
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
            Self::Model => 7,
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
            7 => Self::Model,
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
    pub env_reflection: f64,
    pub emissive: f64,
    pub light_angle: f64,
    pub light_elevation: f64,
    pub opacity: f64,
    pub follow_track: bool,
    pub track_id: u64,
    pub track_depth: f64,
    pub behind_foreground: bool,
    pub foreground_occlusion: f64,
    pub model_path: String,
    /// -1 = rest pose; otherwise an index into the file's animation list.
    pub model_anim: i32,
    pub model_time: f64,
    pub model_play: bool,
    pub model_loop: bool,
    pub model_speed: f64,
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
            env_reflection: 100.0,
            emissive: 0.0,
            light_angle: 135.0,
            light_elevation: 45.0,
            opacity: 100.0,
            follow_track: false,
            track_id: 0,
            track_depth: 240.0,
            behind_foreground: false,
            foreground_occlusion: 55.0,
            model_path: String::new(),
            model_anim: -1,
            model_time: 0.0,
            model_play: true,
            model_loop: true,
            model_speed: 100.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ModelAnimationInfo {
    pub name: String,
    pub duration: f64,
    /// Sorted key times in seconds (the union of every channel's keys): where the animation's
    /// keyframes sit on the time axis.
    pub keys: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ModelInfo {
    pub name: String,
    pub meshes: u32,
    pub triangles: u32,
    pub textures: u32,
    pub skins: u32,
    pub morph_targets: u32,
    pub animations: Vec<ModelAnimationInfo>,
}

/// One scene camera, as the editor reads and writes it (degrees for the angles).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SceneCamera3d {
    /// World position in frame pixels: frame-centred, x right, y down, z away from the viewer.
    pub position: Vec<f64>,
    pub yaw: f64,
    pub pitch: f64,
    pub roll: f64,
    /// Vertical field of view in degrees (the classic fixed camera is 53.13°).
    pub fov: f64,
}

impl Default for SceneCamera3d {
    fn default() -> Self {
        Self { position: vec![0.0, 0.0, 0.0], yaw: 0.0, pitch: 0.0, roll: 0.0, fov: filmcraft_project::CLASSIC_FOV as f64 }
    }
}

impl From<filmcraft_project::SceneCamera3D> for SceneCamera3d {
    fn from(c: filmcraft_project::SceneCamera3D) -> Self {
        Self { position: c.position.map(|v| v as f64).to_vec(), yaw: c.yaw as f64, pitch: c.pitch as f64, roll: c.roll as f64, fov: c.fov as f64 }
    }
}

impl From<SceneCamera3d> for filmcraft_project::SceneCamera3D {
    fn from(c: SceneCamera3d) -> Self {
        let position: [f32; 3] = std::array::from_fn(|i| c.position.get(i).copied().unwrap_or(0.0) as f32);
        Self { position, yaw: c.yaw as f32, pitch: c.pitch as f32, roll: c.roll as f32, fov: (c.fov as f32).clamp(1.0, 179.0) }
    }
}

/// The sequence's scene camera at the playhead (absent = the classic fixed camera), for the editor to show.
pub(crate) fn scene_camera_of(s: &Session) -> Option<SceneCamera3d> {
    let at = s.playhead();
    s.active_sequence().and_then(|seq| seq.scene_camera_at(at).map(Into::into))
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Object3DState {
    pub clip: u64,
    pub has_object: bool,
    pub animated: bool,
    pub settings: Object3DSettings,
    /// Effective screen-space pose at the current playhead after applying `follow_track` (when
    /// enabled), so 3D gizmos stay locked onto the tracked object on every frame.
    pub tracked_x: f64,
    pub tracked_y: f64,
    pub tracked_z: f64,
    pub tracked_scale: f64,
    pub tracked_rotation_z: f64,
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
    /// Mean tracking confidence, 0..100: how much of the feature set stayed locked. A low
    /// number means the subject was occluded, left the frame, or had no texture to follow.
    pub quality: f64,
    /// Frames where the target was lost and the sample only holds its last known place.
    pub lost: u32,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SolveCameraOptions {
    /// Maximum clip span to analyse in seconds (`0.5..=60.0`, default `15.0`).
    pub max_seconds: f64,
    /// Jointly refine camera focal length during bundle adjustment.
    pub refine_focal: bool,
    pub stereo: StereoLayout,
    /// Maximum stereoscopic disparity in working pixels (depth sensitivity).
    pub stereo_baseline: f64,
}

impl Default for SolveCameraOptions {
    fn default() -> Self {
        Self { max_seconds: 15.0, refine_focal: false, stereo: StereoLayout::Mono, stereo_baseline: 64.0 }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CameraSolveInfo {
    pub id: u64,
    pub clip: u64,
    pub name: String,
    pub layout: StereoLayout,
    pub rotation_only: bool,
    pub shots_skipped: u32,
    pub frames: u32,
    pub points: u32,
    pub start: i64,
    pub end: i64,
    pub median_rms_px: f64,
    /// Median per-frame confidence, 0..100.
    pub quality: f64,
    pub lost: u32,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CameraSolveStatus {
    /// 0.0 to 1.0.
    pub progress: f64,
    pub stage: String,
    pub finished: bool,
    pub error: Option<String>,
    pub solve: Option<CameraSolveInfo>,
    /// Populated when 2D video motion was detected and automatically tracked with 2D point tracking.
    pub switched_to_motion_track: Option<MotionTrackInfo>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MatchMoveSettings {
    pub solve: u64,
    pub enabled: bool,
    pub scale_px: f64,
    pub yaw_trim: f64,
    pub pitch_trim: f64,
    pub roll_trim: f64,
    pub offset_x: f64,
    pub offset_y: f64,
    pub offset_z: f64,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SolvedPoint3d {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub observations: u32,
    pub rms_px: f64,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MatchMoveSceneData {
    pub solve: u64,
    pub rotation_only: bool,
    pub points: Vec<SolvedPoint3d>,
    pub path: Vec<SceneCamera3d>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MatchMovePose {
    pub camera: SceneCamera3d,
    /// Raw sample confidence at this frame, 0..100.
    pub confidence: f64,
    pub rms_px: f64,
    /// True when confidence fell below `LOST_CONFIDENCE` (holding last good pose).
    pub lost: bool,
    pub rotation_only: bool,
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
        Self {
            threshold: 38.0,
            feather: 6.0,
            smooth: 2.0,
            invert: false,
            point_radius: 12.0,
            point_strength: 70.0,
            keep_points: Vec::new(),
            remove_points: Vec::new(),
        }
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

fn edit_seq<R>(s: &mut Session, label: &str, merge: Option<&str>, f: impl FnOnce(&mut Sequence) -> filmcraft_engine::Result<R>) -> Result<R, MobileError> {
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
        env_reflection: f_at(e, "env_reflection", t, d.env_reflection),
        emissive: f_at(e, "emissive", t, d.emissive),
        light_angle: f_at(e, "light_angle", t, d.light_angle),
        light_elevation: f_at(e, "light_elevation", t, d.light_elevation),
        opacity: f_at(e, "opacity", t, d.opacity),
        follow_track: e.param("follow_track").and_then(|p| p.value.as_bool()).unwrap_or(false),
        track_id: f_at(e, "track_id", t, 0.0).max(0.0) as u64,
        track_depth: f_at(e, "track_depth", t, d.track_depth),
        behind_foreground: bool_at(e, "behind_foreground", false),
        foreground_occlusion: f_at(e, "foreground_occlusion", t, d.foreground_occlusion),
        model_path: text_at(e, "model_path", t),
        model_anim: f_at(e, "model_anim", t, -1.0).round().clamp(-1.0, 4096.0) as i32,
        model_time: f_at(e, "model_time", t, 0.0).max(0.0),
        model_play: bool_at(e, "model_play", true),
        model_loop: bool_at(e, "model_loop", true),
        model_speed: f_at(e, "model_speed", t, 100.0),
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
    set_f(e, "env_reflection", st.env_reflection.clamp(0.0, 200.0), t);
    set_f(e, "emissive", st.emissive.clamp(0.0, 100.0), t);
    set_f(e, "light_angle", finite(st.light_angle, 135.0), t);
    set_f(e, "light_elevation", st.light_elevation.clamp(-90.0, 90.0), t);
    set_f(e, "opacity", st.opacity.clamp(0.0, 100.0), t);
    set_bool(e, "follow_track", st.follow_track);
    set_f(e, "track_id", st.track_id as f64, t);
    set_f(e, "track_depth", st.track_depth.clamp(-5000.0, 5000.0), t);
    set_bool(e, "behind_foreground", st.behind_foreground);
    set_f(e, "foreground_occlusion", st.foreground_occlusion.clamp(0.0, 100.0), t);
    set_text(e, "model_path", &st.model_path);
    set_f(e, "model_anim", st.model_anim.clamp(-1, 4096) as f64, t);
    set_f(e, "model_time", finite(st.model_time, 0.0).clamp(0.0, 86_400.0), t);
    set_bool(e, "model_play", st.model_play);
    set_bool(e, "model_loop", st.model_loop);
    set_f(e, "model_speed", finite(st.model_speed, 100.0).clamp(-400.0, 400.0), t);
}

const ANIMATED_PARAMS: &[&str] = &[
    "position",
    "z",
    "rotation_x",
    "rotation_y",
    "rotation_z",
    "scale",
    "depth",
    "bevel",
    "color",
    "side_color",
    "metalness",
    "roughness",
    "env_reflection",
    "emissive",
    "light_angle",
    "light_elevation",
    "opacity",
    "track_depth",
    "model_time",
    "model_speed",
];

fn object_animated(e: &EffectInstance) -> bool {
    ANIMATED_PARAMS.iter().any(|id| e.param(id).is_some_and(|p| p.is_animated()))
}

pub(crate) fn add_object(s: &mut Session, mesh: Object3DMesh) -> Result<u64, MobileError> {
    add_object_with_path(s, mesh, None)
}

/// Validate and report a model's animation list before/after import.
pub(crate) fn model_info(path: &str) -> Result<ModelInfo, MobileError> {
    let m =
        filmcraft_render::model::load_cached(std::path::Path::new(path)).map_err(|e| MobileError::Command(format!("That model could not be opened: {e}")))?;
    Ok(ModelInfo {
        name: m.name.clone(),
        meshes: m.meshes.len() as u32,
        triangles: m.triangles.min(u32::MAX as usize) as u32,
        textures: m.textures.len() as u32,
        skins: m.skins.len() as u32,
        morph_targets: m.morph_targets.min(u32::MAX as usize) as u32,
        animations: m
            .animations
            .iter()
            .map(|a| {
                let mut keys: Vec<f64> = a.channels.iter().flat_map(|c| c.times.iter().map(|&t| t as f64)).collect();
                keys.sort_by(|x, y| x.total_cmp(y));
                keys.dedup_by(|x, y| (*x - *y).abs() < 1e-4);
                ModelAnimationInfo { name: a.name.clone(), duration: a.duration as f64, keys }
            })
            .collect(),
    })
}

pub(crate) fn add_model(s: &mut Session, path: &str) -> Result<u64, MobileError> {
    let info = model_info(path)?;
    if info.triangles == 0 {
        return Err(MobileError::Command("That model has no triangles.".into()));
    }
    add_object_with_path(s, Object3DMesh::Model, Some(path))
}

fn add_object_with_path(s: &mut Session, mesh: Object3DMesh, model_path: Option<&str>) -> Result<u64, MobileError> {
    let (_, seq) = model::sequence(s)?;
    let w = seq.settings.width;
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
        } else if mesh == Object3DMesh::Model {
            set_color(&mut e, "color", 0xffffffff, Tick::ZERO);
            if let Some(path) = model_path { set_text(&mut e, "model_path", path); }
            set_f(&mut e, "model_anim", 0.0, Tick::ZERO);
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
            Object3DMesh::Model => "Model",
        });
        edits::settle(s);
        Ok(id)
    })?;
    ex(s, "timeline.select", json!({"clips": [clip]}))?;
    Ok(clip)
}

fn track_2d_pair_at(
    seq: &Sequence,
    track_id: u64,
    playhead: Tick,
) -> Option<(filmcraft_project::TrackSample, filmcraft_project::TrackSample)> {
    if track_id == 0 {
        return None;
    }
    let tr = seq.motion_tracks.iter().find(|t| t.id == track_id)?;
    let start = seq.find_item(tr.clip).map(|(_, c)| c.start).unwrap_or(Tick::ZERO);
    let anchor = *tr.samples.first()?;
    let now = tr.sample_at(playhead - start)?;
    Some((now, anchor))
}

/// Forward 2D camera-space transform: maps anchor-space `(x, y, z, scale, rotation_z)` to
/// effective screen-space values at `playhead`.
fn forward_track_2d(
    seq: &Sequence,
    track_id: u64,
    playhead: Tick,
    pose: (f64, f64, f64, f64, f64),
    track_depth: f64,
) -> (f64, f64, f64, f64, f64) {
    let Some((s, anchor)) = track_2d_pair_at(seq, track_id, playhead) else {
        return pose;
    };
    let (x, y, z, scale, rz) = pose;
    let fw = f64::from(seq.settings.width.max(1));
    let fh = f64::from(seq.settings.height.max(1));
    let rel_scale = f64::from(s.scale.max(0.01) / anchor.scale.max(0.01));
    let dtheta = f64::from(s.rotation - anchor.rotation);
    let rad = dtheta.to_radians();
    let (cos_t, sin_t) = (rad.cos(), rad.sin());
    let ax = f64::from(anchor.x) * fw;
    let ay = f64::from(anchor.y) * fh;
    let sx = f64::from(s.x) * fw;
    let sy = f64::from(s.y) * fh;
    let (du, dv) = (x - ax, y - ay);
    (
        sx + rel_scale * (cos_t * du - sin_t * dv),
        sy + rel_scale * (sin_t * du + cos_t * dv),
        z + f64::from(s.z - anchor.z) * track_depth,
        scale * rel_scale,
        rz + dtheta,
    )
}

/// Inverse 2D camera-space transform: converts a desired screen-space pose at `playhead` into
/// anchor-space parameters so enabling `follow_track` never causes the 3D object to jump.
fn inverse_track_2d(
    seq: &Sequence,
    track_id: u64,
    playhead: Tick,
    screen_pose: (f64, f64, f64, f64, f64),
    track_depth: f64,
) -> (f64, f64, f64, f64, f64) {
    let Some((s, anchor)) = track_2d_pair_at(seq, track_id, playhead) else {
        return screen_pose;
    };
    let (sx_in, sy_in, sz_in, sc_in, rz_in) = screen_pose;
    let fw = f64::from(seq.settings.width.max(1));
    let fh = f64::from(seq.settings.height.max(1));
    let rel_scale = f64::from(s.scale.max(0.01) / anchor.scale.max(0.01));
    let dtheta = f64::from(s.rotation - anchor.rotation);
    let rad = dtheta.to_radians();
    let (cos_t, sin_t) = (rad.cos(), rad.sin());
    let ax = f64::from(anchor.x) * fw;
    let ay = f64::from(anchor.y) * fh;
    let sx = f64::from(s.x) * fw;
    let sy = f64::from(s.y) * fh;
    let rx = (sx_in - sx) / rel_scale;
    let ry = (sy_in - sy) / rel_scale;
    let du = cos_t * rx + sin_t * ry;
    let dv = -sin_t * rx + cos_t * ry;
    (
        ax + du,
        ay + dv,
        (sz_in - f64::from(s.z - anchor.z) * track_depth).clamp(20.0, 20_000.0),
        (sc_in / rel_scale).clamp(1.0, 10_000.0),
        rz_in - dtheta,
    )
}

pub(crate) fn object_state_at(seq: &Sequence, id: u64, ph: Tick) -> Option<Object3DState> {
    let c = seq.find_item(ClipId(id)).map(|(_, c)| c)?;
    let t = c.effect_time_at(ph);
    let center = (f64::from(seq.settings.width) / 2.0, f64::from(seq.settings.height) / 2.0);
    Some(match c.effects.iter().find(|e| e.effect == OBJECT3D) {
        Some(e) => {
            let st = object_settings(e, t, center);
            let (tx, ty, tz, tsc, trz) = if st.follow_track && st.track_id != 0 {
                forward_track_2d(seq, st.track_id, ph, (st.x, st.y, st.z, st.scale, st.rotation_z), st.track_depth)
            } else {
                (st.x, st.y, st.z, st.scale, st.rotation_z)
            };
            Object3DState {
                clip: id,
                has_object: true,
                animated: object_animated(e),
                settings: st,
                tracked_x: tx,
                tracked_y: ty,
                tracked_z: tz,
                tracked_scale: tsc,
                tracked_rotation_z: trz,
            }
        }
        None => {
            let d = Object3DSettings::default();
            Object3DState {
                clip: id,
                has_object: false,
                animated: false,
                tracked_x: d.x,
                tracked_y: d.y,
                tracked_z: d.z,
                tracked_scale: d.scale,
                tracked_rotation_z: d.rotation_z,
                settings: d,
            }
        }
    })
}

pub(crate) fn object_state(s: &Session, id: u64) -> Result<Object3DState, MobileError> {
    let (_, seq) = model::sequence(s)?;
    object_state_at(seq, id, s.playhead()).ok_or_else(|| MobileError::Command("That clip is gone.".into()))
}

pub(crate) fn set_object(s: &mut Session, id: u64, st: &Object3DSettings, merge: Option<&str>) -> Result<(), MobileError> {
    let t = s.playhead();
    edit_seq(s, "Edit 3D Object", merge, |q| {
        let (prev_follow, prev_tid, prev_td) = {
            let c = clip_mut(q, id)?;
            let mt = c.effect_time_at(t);
            let e = effect_mut(c, OBJECT3D)?;
            (
                bool_at(e, "follow_track", false),
                f_at(e, "track_id", mt, 0.0).max(0.0) as u64,
                f_at(e, "track_depth", mt, 240.0),
            )
        };
        let mut adjusted = st.clone();
        // When toggling `follow_track` or switching `track_id` at a non-zero playhead, rebase the
        // object's stored pose so it stays visually locked at its current on-screen spot!
        if (prev_follow != st.follow_track || (st.follow_track && prev_tid != st.track_id))
            && (prev_tid != 0 || st.track_id != 0)
        {
            let screen_pose = if prev_follow && prev_tid != 0 {
                forward_track_2d(
                    q,
                    prev_tid,
                    t,
                    (adjusted.x, adjusted.y, adjusted.z, adjusted.scale, adjusted.rotation_z),
                    prev_td,
                )
            } else {
                (adjusted.x, adjusted.y, adjusted.z, adjusted.scale, adjusted.rotation_z)
            };
            let (nx, ny, nz, nsc, nrz) = if adjusted.follow_track && adjusted.track_id != 0 {
                inverse_track_2d(q, adjusted.track_id, t, screen_pose, adjusted.track_depth)
            } else {
                screen_pose
            };
            adjusted.x = nx;
            adjusted.y = ny;
            adjusted.z = nz;
            adjusted.scale = nsc;
            adjusted.rotation_z = nrz;
        }
        let c = clip_mut(q, id)?;
        let mt = c.effect_time_at(t);
        let e = effect_mut(c, OBJECT3D)?;
        write_object(e, &adjusted, mt);
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
    let scale = (640.0f32 / src_w.max(src_h).max(1) as f32).min(1.0);
    let patch = opts.patch_size.clamp(9, 41) as usize;
    let max_disp = finite(opts.baseline, 64.0).clamp(4.0, 256.0) as f32;
    // Streamed directly to the GPU tracker (`Tracker::new_rgba` / `push_rgba`), keeping at most
    // two pyramids in VRAM (or RAM fallback) at a time.
    let mut tracker: Option<Tracker> = None;
    let mut samples: Vec<filmcraft_project::TrackSample> = Vec::new();
    let mut t = start;
    while t < end && samples.len() < 2000 {
        if filmcraft_media::cancel::cancelled() {
            break;
        }
        let st = c.source_time_at(t);
        let Ok(f) = source.video_frame(FrameRequest { time: st, scale }) else { break };
        let (full_w, full_h, rgba) = f.to_rgba8_max_side(match opts.stereo {
            StereoLayout::Mono => 640,
            StereoLayout::SideBySide | StereoLayout::TopBottom => 1280,
        });
        let (vx, vy, vw, vh) = view_rect(opts.stereo.project(), full_w, full_h);
        let right_crop = match opts.stereo {
            StereoLayout::Mono => None,
            StereoLayout::SideBySide => Some((vx + vw, vy, vw, vh)),
            StereoLayout::TopBottom => Some((vx, vy + vh, vw, vh)),
        };
        let p = match &mut tracker {
            None => {
                let target = (
                    (finite(opts.target_x, 0.5).clamp(0.0, 1.0) * vw as f64) as f32,
                    (finite(opts.target_y, 0.5).clamp(0.0, 1.0) * vh as f64) as f32,
                );
                let tr = Tracker::new_rgba(&rgba, full_w, full_h, (vx, vy, vw, vh), target, patch, max_disp);
                let a = tr.anchor();
                tracker = Some(tr);
                a
            }
            Some(tr) => tr.push_rgba(&rgba, full_w, full_h, (vx, vy, vw, vh), right_crop),
        };
        samples.push(filmcraft_project::TrackSample {
            time: t - c.start,
            x: ((vx as f32 + p.x) / full_w as f32).clamp(-64.0, 64.0),
            y: ((vy as f32 + p.y) / full_h as f32).clamp(-64.0, 64.0),
            z: p.z,
            scale: p.scale,
            rotation: p.rotation,
            confidence: p.confidence,
        });
        t += frame;
    }
    if samples.len() < 2 {
        return Err(MobileError::Command("Not enough frames to track.".into()));
    }
    filmcraft_render::track3d::smooth_track_samples(&mut samples);
    let count = samples.len() as u32;
    let quality = f64::from(samples.iter().map(|s| s.confidence).sum::<f32>()) / f64::from(count) * 100.0;
    let lost = samples.iter().filter(|s| s.confidence <= 0.0).count() as u32;
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
    Ok(MotionTrackInfo { id: track_id, clip: c.id.0, name, layout, samples: count, start: c.start.0 + first.0, end: c.start.0 + last.0, quality, lost })
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
            quality: if t.samples.is_empty() { 0.0 } else { f64::from(t.samples.iter().map(|s| s.confidence).sum::<f32>()) / t.samples.len() as f64 * 100.0 },
            lost: t.samples.iter().filter(|s| s.confidence <= 0.0).count() as u32,
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

// ---- 3D camera match-move (COLMAP / GLOMAP sparse + OpenGL -> Scene Camera) ----

pub(crate) struct SolveJob {
    seq: filmcraft_project::ItemId,
    progress: AtomicU64,
    stage: Mutex<String>,
    cancel: AtomicBool,
    solved: Mutex<Option<Result<SolveReport, String>>>,
    applied: Mutex<Option<Result<CameraSolveInfo, String>>>,
}

impl SolveJob {
    fn set(&self, progress: f64, stage: Option<&str>) {
        self.progress.store(progress.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
        if let Some(st) = stage {
            *self.stage.lock().unwrap_or_else(PoisonError::into_inner) = st.to_string();
        }
    }

    pub(crate) fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    pub(crate) fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub(crate) fn is_settled(&self) -> bool {
        self.applied.lock().unwrap_or_else(PoisonError::into_inner).is_some()
            || (self.cancelled() && self.solved.lock().unwrap_or_else(PoisonError::into_inner).is_some())
    }
}

struct SolveWorkerArgs {
    clip: TrackItem,
    source: SharedSource,
    win_start: Tick,
    win_end: Tick,
    frame_step: Tick,
    native_step: Tick,
    scale: f32,
    opts: SolveOptions,
}

fn run_solve_worker(job: &SolveJob, args: SolveWorkerArgs) -> Result<SolveReport, String> {
    let est_frames = (((args.win_end - args.win_start).0 / args.frame_step.0.max(1)).max(1) as usize + 1)
        .min(filmcraft_project::CameraSolve::MAX_FRAMES);
    let working_side = match args.opts.layout {
        filmcraft_project::StereoLayout::Mono => 640,
        filmcraft_project::StereoLayout::SideBySide | filmcraft_project::StereoLayout::TopBottom => 1280,
    };
    job.set(0.02, Some("Sampling frames (≤ 3 fps)…"));
    let last_valid_t = (args.win_end - args.native_step).max(args.win_start);
    let mut t = args.win_start;
    let mut emitted_tail = false;
    let mut last_emitted: Option<Tick> = None;
    let mut decoded = 0usize;
    let cancel_fn = || job.cancelled();

    filmcraft_media::cancel::with_background(true, || {
        solve_camera(&args.opts, &cancel_fn, || {
            if job.cancelled() {
                return Err(SolveError::Cancelled);
            }
            let sample_t = if t < args.win_end && decoded < filmcraft_project::CameraSolve::MAX_FRAMES {
                let cur = t;
                t += args.frame_step;
                Some(cur)
            } else if !emitted_tail
                && last_emitted.is_some_and(|prev| last_valid_t.0.saturating_sub(prev.0) >= args.native_step.0)
            {
                emitted_tail = true;
                Some(last_valid_t)
            } else {
                None
            };

            let Some(cur) = sample_t else {
                job.set(0.50, Some("Solving 3D camera (COLMAP)…"));
                return Ok(None);
            };
            last_emitted = Some(cur);
            let st = args.clip.source_time_at(cur);
            let f = args
                .source
                .video_frame(FrameRequest { time: st, scale: args.scale })
                .map_err(|e| SolveError::Decode(e.to_string()))?;
            let (w, h, rgba) = f.to_rgba8_max_side(working_side);
            decoded += 1;
            job.set(0.46 * (decoded as f64 / est_frames as f64).clamp(0.0, 1.0), None);
            Ok(Some(InputFrame {
                time: Tick(cur.0.saturating_sub(args.clip.start.0)),
                width: w,
                height: h,
                rgba,
            }))
        })
    })
    .map_err(|e| match e {
        SolveError::Cancelled => "cancelled".to_string(),
        other_err => other_err.to_string(),
    })
}

fn is_solvable_video_item(s: &Session, item_id: filmcraft_project::ItemId) -> bool {
    matches!(
        s.project.item(item_id).map(|pi| &pi.kind),
        Some(ItemKind::Media(m))
            if m.info.has_video()
                && !matches!(m.info.kind, filmcraft_media::MediaKind::Still)
                && matches!(m.media, filmcraft_project::MediaRef::File { .. })
    )
}

pub(crate) fn start_camera_solve(s: &Session, id: u64, opts: &SolveCameraOptions) -> Result<Arc<SolveJob>, MobileError> {
    let (sid, seq) = model::sequence(s)?;
    let requested = seq.find_item(ClipId(id)).map(|(_, c)| c.clone());
    let c = match requested {
        Some(cand) if is_solvable_video_item(s, cand.item) => cand,
        other => {
            // If a 3D object / title / generator clip was selected in UI, automatically resolve the
            // underlying file-backed camera video clip at playhead (or first video clip on timeline).
            let ph = s.playhead();
            let fallback = seq
                .video_tracks
                .iter()
                .flat_map(|tr| tr.items.iter())
                .find(|it| ph >= it.start && ph < it.end() && is_solvable_video_item(s, it.item))
                .or_else(|| {
                    seq.video_tracks
                        .iter()
                        .flat_map(|tr| tr.items.iter())
                        .find(|it| is_solvable_video_item(s, it.item))
                })
                .cloned();
            match (fallback, other) {
                (Some(vclip), _) => vclip,
                (None, Some(cand)) => {
                    let pi = s.project.item(cand.item).ok_or_else(|| MobileError::Command("That clip has no media.".into()))?;
                    if !matches!(&pi.kind, ItemKind::Media(m) if m.info.has_video()) {
                        return Err(MobileError::Command("Camera solving needs a video clip.".into()));
                    }
                    cand
                }
                (None, None) => return Err(MobileError::Command("That clip is gone.".into())),
            }
        }
    };
    let pi = s.project.item(c.item).ok_or_else(|| MobileError::Command("That clip has no media.".into()))?;
    if !matches!(&pi.kind, ItemKind::Media(m) if m.info.has_video()) {
        return Err(MobileError::Command("Camera solving needs a video clip.".into()));
    }
    let provider = s.media.provider(s.project.clone(), s.services.clone());
    let source = provider.source(c.item).ok_or_else(|| MobileError::Command("The clip's media could not be opened.".into()))?;

    let rate = s.sequence_rate();
    let seq_step = rate.frame_duration().max(Tick(1));
    let src_step = source
        .info()
        .video
        .as_ref()
        .map(|v| {
            let spd = c.speed.abs().clamp(0.01, 100.0);
            Tick(((v.frame_rate.frame_duration().0 as f64) / spd).round() as i64).max(Tick(1))
        })
        .unwrap_or(seq_step);
    let native_step = seq_step.max(src_step);
    let clip_len = c.end() - c.start;

    // Limit COLMAP / GLOMAP input to at most 3 fps (`1/3 s` step); intermediate frames are
    // smoothly interpolated and routed to the scene camera.
    let step_3fps = Tick::from_seconds_f64(1.0 / MAX_SOLVER_FPS);
    let frame_step = native_step.max(step_3fps);

    let max_sec = finite(opts.max_seconds, 15.0).clamp(0.5, 60.0);
    let max_ticks = Tick::from_seconds_f64(max_sec);
    let clamped_ph = s.playhead().max(c.start).min((c.end() - native_step).max(c.start));
    let (win_start, win_end) = if clip_len <= max_ticks {
        (c.start, c.end())
    } else {
        let half = Tick(max_ticks.0 / 2);
        let ws = (clamped_ph - half).max(c.start).min(c.end() - max_ticks);
        (ws, (ws + max_ticks).min(c.end()))
    };

    let (src_w, src_h) = source.info().video.as_ref().map(|v| (v.width, v.height)).unwrap_or((seq.settings.width, seq.settings.height));
    let target_long = match opts.stereo {
        StereoLayout::Mono => 640.0f32,
        StereoLayout::SideBySide | StereoLayout::TopBottom => 1280.0f32,
    };
    let scale = (target_long / src_w.max(src_h).max(1) as f32).min(1.0);
    let solve_opts = SolveOptions {
        id: 0,
        clip: c.id,
        name: String::new(),
        playhead: Tick(clamped_ph.0.saturating_sub(c.start.0)),
        max_seconds: max_sec,
        refine_focal: opts.refine_focal,
        layout: opts.stereo.project(),
        stereo_baseline: finite(opts.stereo_baseline, 64.0).clamp(4.0, 256.0) as f32,
    };

    let job = Arc::new(SolveJob {
        seq: sid,
        progress: AtomicU64::new(0),
        stage: Mutex::new("Getting ready…".into()),
        cancel: AtomicBool::new(false),
        solved: Mutex::new(None),
        applied: Mutex::new(None),
    });
    let worker_job = job.clone();
    let args = SolveWorkerArgs {
        clip: c,
        source,
        win_start,
        win_end,
        frame_step,
        native_step,
        scale,
        opts: solve_opts,
    };
    std::thread::Builder::new()
        .name("filmcraft-matchmove".into())
        .spawn(move || {
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_solve_worker(&worker_job, args)))
                .unwrap_or_else(|_| Err("Solving the camera failed.".to_string()));
            *worker_job.solved.lock().unwrap_or_else(PoisonError::into_inner) = Some(r);
        })
        .map_err(|e| MobileError::Command(format!("Camera solve couldn't start: {e}")))?;
    Ok(job)
}

fn solve_info(seq: &Sequence, cs: &filmcraft_project::CameraSolve) -> CameraSolveInfo {
    let base = seq.find_item(cs.clip).map(|(_, c)| c.start.0).unwrap_or(0);
    CameraSolveInfo {
        id: cs.id,
        clip: cs.clip.0,
        name: cs.name.clone(),
        layout: StereoLayout::from_project(cs.layout),
        rotation_only: cs.rotation_only,
        shots_skipped: cs.shots_skipped,
        frames: cs.frames.len() as u32,
        points: cs.points.len() as u32,
        start: base.saturating_add(cs.frames.first().map(|f| f.time.0).unwrap_or(0)),
        end: base.saturating_add(cs.frames.last().map(|f| f.time.0).unwrap_or(0)),
        median_rms_px: f64::from(cs.median_rms()),
        quality: (f64::from(cs.median_confidence()) * 100.0).clamp(0.0, 100.0),
        lost: cs.lost_frames(),
    }
}

pub(crate) fn camera_solve_status(s: &mut Session, job: &SolveJob) -> CameraSolveStatus {
    let finished_report = job.solved.lock().unwrap_or_else(PoisonError::into_inner).take();
    if let Some(res) = finished_report {
        let ph = s.playhead();
        let applied = if job.cancelled() {
            Err("cancelled".to_string())
        } else {
            match res {
                Ok(rep) => s
                    .edit("Solve Camera", |p, _| {
                        let id = p.alloc_id();
                        let q = p.sequence_mut(job.seq).ok_or(EngineError::NoSequence)?;
                        let name = format!("Solve {}", q.camera_solves.len() + 1);
                        let mut solve = rep.solve;
                        solve.id = id;
                        solve.name = name;
                        let info = solve_info(q, &solve);
                        q.camera_solves.push(solve);
                        q.matchmove = Some(filmcraft_project::SequenceMatchMove::new(id));
                        // Route COLMAP / GLOMAP camera trajectory directly to the 3D Scene Camera!
                        q.route_matchmove_to_scene_camera(ph);
                        Ok(info)
                    })
                    .map_err(|e| e.to_string()),
                Err(e) => Err(e),
            }
        };
        *job.applied.lock().unwrap_or_else(PoisonError::into_inner) = Some(applied);
    }
    let progress = f64::from_bits(job.progress.load(Ordering::Relaxed));
    let stage = job.stage.lock().unwrap_or_else(PoisonError::into_inner).clone();
    match job.applied.lock().unwrap_or_else(PoisonError::into_inner).as_ref() {
        Some(Ok(info)) => CameraSolveStatus {
            progress: 1.0,
            stage: "Done".into(),
            finished: true,
            error: None,
            solve: Some(info.clone()),
            switched_to_motion_track: None,
        },
        Some(Err(e)) => CameraSolveStatus {
            progress,
            stage,
            finished: true,
            error: Some(e.clone()),
            solve: None,
            switched_to_motion_track: None,
        },
        None => CameraSolveStatus {
            progress,
            stage,
            finished: false,
            error: None,
            solve: None,
            switched_to_motion_track: None,
        },
    }
}

pub(crate) fn camera_solves(s: &Session) -> Result<Vec<CameraSolveInfo>, MobileError> {
    let (_, seq) = model::sequence(s)?;
    Ok(seq.camera_solves.iter().map(|cs| solve_info(seq, cs)).collect())
}

pub(crate) fn delete_camera_solve(s: &mut Session, id: u64) -> Result<(), MobileError> {
    let ph = s.playhead();
    edit_seq(s, "Delete Camera Solve", None, |q| {
        q.camera_solves.retain(|cs| cs.id != id);
        if q.matchmove.is_some_and(|m| m.solve == id) {
            q.matchmove = None;
            q.route_matchmove_to_scene_camera(ph);
        }
        Ok(())
    })
}

pub(crate) fn matchmove_state(s: &Session) -> Result<Option<MatchMoveSettings>, MobileError> {
    let (_, seq) = model::sequence(s)?;
    Ok(seq.matchmove.map(|m| MatchMoveSettings {
        solve: m.solve,
        enabled: m.enabled,
        scale_px: f64::from(m.scale_px),
        yaw_trim: f64::from(m.yaw_trim),
        pitch_trim: f64::from(m.pitch_trim),
        roll_trim: f64::from(m.roll_trim),
        offset_x: f64::from(m.offset[0]),
        offset_y: f64::from(m.offset[1]),
        offset_z: f64::from(m.offset[2]),
    }))
}

pub(crate) fn set_matchmove(s: &mut Session, st: &MatchMoveSettings, merge: Option<&str>) -> Result<(), MobileError> {
    let ph = s.playhead();
    edit_seq(s, "Edit Match-Move", merge, |q| {
        if st.solve == 0 {
            q.matchmove = None;
            q.route_matchmove_to_scene_camera(ph);
            return Ok(());
        }
        if !q.camera_solves.iter().any(|cs| cs.id == st.solve) {
            return Err(other("That camera solve is gone."));
        }
        q.matchmove = Some(filmcraft_project::SequenceMatchMove {
            solve: st.solve,
            enabled: st.enabled,
            scale_px: finite(st.scale_px, f64::from(filmcraft_project::SequenceMatchMove::DEFAULT_SCALE_PX)).clamp(1.0, 100_000.0) as f32,
            yaw_trim: finite(st.yaw_trim, 0.0).clamp(-180.0, 180.0) as f32,
            pitch_trim: finite(st.pitch_trim, 0.0).clamp(-180.0, 180.0) as f32,
            roll_trim: finite(st.roll_trim, 0.0).clamp(-180.0, 180.0) as f32,
            offset: [
                finite(st.offset_x, 0.0).clamp(-100_000.0, 100_000.0) as f32,
                finite(st.offset_y, 0.0).clamp(-100_000.0, 100_000.0) as f32,
                finite(st.offset_z, 0.0).clamp(-100_000.0, 100_000.0) as f32,
            ],
        });
        q.route_matchmove_to_scene_camera(ph);
        Ok(())
    })
}

pub(crate) fn matchmove_scene_data(s: &Session) -> Result<Option<MatchMoveSceneData>, MobileError> {
    let (_, seq) = model::sequence(s)?;
    let Some(mm) = seq.matchmove.filter(|m| m.enabled) else { return Ok(None) };
    let Some(cs) = seq.camera_solves.iter().find(|c| c.id == mm.solve) else { return Ok(None) };
    let points = cs
        .points
        .iter()
        .map(|pt| {
            let w = mm.apply_to_point(pt.position);
            SolvedPoint3d { x: f64::from(w[0]), y: f64::from(w[1]), z: f64::from(w[2]), observations: pt.observations, rms_px: f64::from(pt.rms_px) }
        })
        .collect();
    let max_path = 256usize;
    let step = cs.frames.len().div_ceil(max_path).max(1);
    let mut path: Vec<SceneCamera3d> = cs.frames.iter().step_by(step).map(|f| mm.apply_to_sample(f).into()).collect();
    if cs.frames.len() > 1
        && !(cs.frames.len() - 1).is_multiple_of(step)
        && let Some(last) = cs.frames.last()
    {
        path.push(mm.apply_to_sample(last).into());
    }
    Ok(Some(MatchMoveSceneData { solve: cs.id, rotation_only: cs.rotation_only, points, path }))
}

pub(crate) fn matchmove_pose_at(seq: &Sequence, timeline: Tick) -> Option<MatchMovePose> {
    let mm = seq.matchmove.filter(|m| m.enabled)?;
    let cs = seq.camera_solves.iter().find(|c| c.id == mm.solve)?;
    let camera = seq
        .scene_camera_at(timeline)
        .or_else(|| seq.matchmove_camera_at(timeline))?
        .into();
    let start = seq.find_item(cs.clip).map(|(_, c)| c.start).unwrap_or(Tick::ZERO);
    let local = Tick(timeline.0.saturating_sub(start.0));
    let raw = cs.sample_at(local)?;
    Some(MatchMovePose {
        camera,
        confidence: (f64::from(raw.confidence) * 100.0).clamp(0.0, 100.0),
        rms_px: f64::from(raw.rms_px),
        lost: !raw.is_confident(),
        rotation_only: cs.rotation_only,
    })
}

pub(crate) fn level_matchmove_ground(s: &mut Session) -> Result<MatchMoveSettings, MobileError> {
    let ph = s.playhead();
    let normal = {
        let (_, seq) = model::sequence(s)?;
        let mm = seq.matchmove.ok_or_else(|| MobileError::Command("Solve or select a 3D camera track first.".into()))?;
        let cs = seq.camera_solves.iter().find(|c| c.id == mm.solve).ok_or_else(|| MobileError::Command("That camera solve is gone.".into()))?;
        cs.fit_ground_plane().map(|(n, _)| n).ok_or_else(|| MobileError::Command("Not enough floor landmarks in this clip to auto-level the ground.".into()))?
    };
    edit_seq(s, "Auto-Level Match-Move", None, |q| {
        if let Some(mm) = q.matchmove.as_mut() {
            mm.enabled = true;
            let _ = mm.level_to_normal(normal);
        }
        q.route_matchmove_to_scene_camera(ph);
        Ok(())
    })?;
    matchmove_state(s)?.ok_or_else(|| MobileError::Command("No active match-move.".into()))
}

fn nearest_landmark_world(seq: &Sequence, timeline: Tick, norm_x: f64, norm_y: f64) -> Option<[f32; 3]> {
    let mm = seq.matchmove.filter(|m| m.enabled)?;
    let cs = seq.camera_solves.iter().find(|c| c.id == mm.solve)?;
    let cam = seq.scene_camera_at(timeline).or_else(|| seq.matchmove_camera_at(timeline))?;
    let (fw, fh) = (seq.settings.width.max(1) as f32, seq.settings.height.max(1) as f32);
    let tx = finite(norm_x, 0.5).clamp(0.0, 1.0) as f32 * fw;
    let ty = finite(norm_y, 0.5).clamp(0.0, 1.0) as f32 * fh;
    let vr = cam.view_rotation();
    let focal = cam.focal(fh);
    let mut best: Option<(f32, [f32; 3])> = None;
    for pt in &cs.points {
        let w = mm.apply_to_point(pt.position);
        if w[2] < 20.0 {
            continue;
        }
        let d = [w[0] - cam.position[0], w[1] - cam.position[1], w[2] - cam.position[2]];
        let vz = vr[2][0] * d[0] + vr[2][1] * d[1] + vr[2][2] * d[2];
        if vz <= 10.0 {
            continue;
        }
        let vx = vr[0][0] * d[0] + vr[0][1] * d[1] + vr[0][2] * d[2];
        let vy = vr[1][0] * d[0] + vr[1][1] * d[1] + vr[1][2] * d[2];
        let sx = fw * 0.5 + focal * vx / vz;
        let sy = fh * 0.5 + focal * vy / vz;
        let dist2 = (sx - tx) * (sx - tx) + (sy - ty) * (sy - ty);
        if best.is_none_or(|(bd, _)| dist2 < bd) {
            best = Some((dist2, w));
        }
    }
    best.map(|(_, w)| w)
}

pub(crate) fn snap_object_to_matchmove(s: &mut Session, id: u64, norm_x: f64, norm_y: f64) -> Result<Object3DSettings, MobileError> {
    let (fw, fh, world) = {
        let (_, seq) = model::sequence(s)?;
        let w = nearest_landmark_world(seq, s.playhead(), norm_x, norm_y)
            .ok_or_else(|| MobileError::Command("No visible 3D track points at this frame to snap onto.".into()))?;
        (f64::from(seq.settings.width.max(1)), f64::from(seq.settings.height.max(1)), w)
    };
    let cur = object_state(s, id)?;
    if !cur.has_object {
        return Err(MobileError::Command("Select a 3D object clip first.".into()));
    }
    let mut st = cur.settings;
    let z = f64::from(world[2]).max(8.0);
    st.z = z;
    st.x = fw * 0.5 + f64::from(world[0]) * fh / z;
    st.y = fh * 0.5 + f64::from(world[1]) * fh / z;
    st.follow_track = false;
    set_object(s, id, &st, None)?;
    Ok(st)
}

pub(crate) fn place_object_on_matchmove(s: &mut Session, mesh: Object3DMesh, norm_x: f64, norm_y: f64) -> Result<u64, MobileError> {
    {
        let (_, seq) = model::sequence(s)?;
        if nearest_landmark_world(seq, s.playhead(), norm_x, norm_y).is_none() {
            return Err(MobileError::Command("No visible 3D track points at this frame to place onto.".into()));
        }
    }
    let mut created = 0u64;
    s.edit_group("Place 3D Object on Landmark", |s| {
        let id = add_object(s, mesh).map_err(|e| EngineError::Other(e.to_string()))?;
        snap_object_to_matchmove(s, id, norm_x, norm_y).map_err(|e| EngineError::Other(e.to_string()))?;
        created = id;
        Ok(())
    })?;
    Ok(created)
}

pub(crate) fn bake_matchmove_to_scene_camera(s: &mut Session) -> Result<SceneCamera3d, MobileError> {
    let ph = s.playhead();
    {
        let (_, seq) = model::sequence(s)?;
        seq.matchmove_camera_at(ph).ok_or_else(|| MobileError::Command("Enable a 3D camera solve before baking to the scene camera.".into()))?;
    }
    let mut out_cam = filmcraft_project::SceneCamera3D::default();
    edit_seq(s, "Bake Match-Move Camera", None, |q| {
        if let Some(cam) = q.route_matchmove_to_scene_camera(ph) {
            out_cam = cam;
        }
        if let Some(mm) = q.matchmove.as_mut() {
            mm.enabled = false;
        }
        Ok(())
    })?;
    Ok(out_cam.into())
}
