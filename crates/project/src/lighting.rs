//! The 3D scene of a sequence: its coloured, placeable light objects (instead of one global
//! light angle) and the camera it renders through. An empty light list and no camera mean the
//! classic look (one light per object, a fixed frame-height focal), so old projects render
//! unchanged.

use std::collections::BTreeMap;

use filmcraft_time::Tick;
use serde::{Deserialize, Serialize};

use crate::keyframe::{Interpolation, Param, ParamValue};

/// What a light is: an aimed beam from infinity, a bulb in space, or a cone from a point.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LightKind {
    #[default]
    Directional,
    Point,
    Spot,
}

/// One light object in the scene.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Light3D {
    pub id: u64,
    pub name: String,
    pub enabled: bool,
    pub kind: LightKind,
    /// The light's colour, sRGB 0..1 like every colour param (1,1,1 = white).
    pub color: [f32; 3],
    /// Brightness multiplier.
    pub intensity: f32,
    /// Where the light is, in the object placement units: x/y in output pixels, z the camera
    /// distance (point and spot).
    pub position: [f32; 3],
    /// Aim as azimuth/elevation in degrees — the same numbers the classic light had, so a
    /// directional light at 135°/45° lights exactly like the old default (directional and spot).
    pub angle: f32,
    pub elevation: f32,
    /// Falloff radius in output pixels; 0 = the light reaches everything (point and spot).
    pub range: f32,
    /// The spot cone's outer half-angle in degrees.
    pub cone: f32,
    /// How much of the cone is a soft edge (0 = hard, 1 = all soft).
    pub softness: f32,
    /// Sequence-timeline keyframe tracks for animatable light parameters. Empty on static lights
    /// so older project files read and write unchanged.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tracks: BTreeMap<String, Param>,
}

impl Default for Light3D {
    fn default() -> Self {
        Light3D {
            id: 0,
            name: "Light".into(),
            enabled: true,
            kind: LightKind::Directional,
            color: [1.0; 3],
            intensity: 1.0,
            position: [0.0, 0.0, 1500.0],
            angle: 135.0,
            elevation: 45.0,
            range: 0.0,
            cone: 35.0,
            softness: 0.5,
            tracks: BTreeMap::new(),
        }
    }
}

impl Light3D {
    /// Parameter IDs animated when light keyframing is enabled.
    pub const ANIMATED_PARAMS: &'static [&'static str] = &[
        "color", "intensity", "x", "y", "z", "angle", "elevation", "range", "cone", "softness",
    ];

    fn static_param_value(&self, id: &str) -> Option<ParamValue> {
        Some(match id {
            "color" => ParamValue::Color([self.color[0], self.color[1], self.color[2], 1.0]),
            "intensity" => ParamValue::Float(f64::from(self.intensity)),
            "x" => ParamValue::Float(f64::from(self.position[0])),
            "y" => ParamValue::Float(f64::from(self.position[1])),
            "z" => ParamValue::Float(f64::from(self.position[2])),
            "angle" => ParamValue::Float(f64::from(self.angle)),
            "elevation" => ParamValue::Float(f64::from(self.elevation)),
            "range" => ParamValue::Float(f64::from(self.range)),
            "cone" => ParamValue::Float(f64::from(self.cone)),
            "softness" => ParamValue::Float(f64::from(self.softness)),
            _ => return None,
        })
    }

    fn eval_float(&self, id: &str, fallback: f32, t: Tick) -> f32 {
        match self.tracks.get(id).filter(|p| p.is_animated()).map(|p| p.value_at(t)) {
            Some(ParamValue::Float(v)) if v.is_finite() => v as f32,
            _ => fallback,
        }
    }

    /// Evaluate this light at sequence timeline `t`. Fast-paths static lights with no tracks.
    pub fn evaluated_at(&self, t: Tick) -> Self {
        if self.tracks.is_empty() || !self.is_animated() {
            return Self { tracks: BTreeMap::new(), ..self.clone() };
        }
        let color = match self.tracks.get("color").filter(|p| p.is_animated()).map(|p| p.value_at(t)) {
            Some(ParamValue::Color(c)) => [
                if c[0].is_finite() { c[0].clamp(0.0, 1.0) } else { self.color[0] },
                if c[1].is_finite() { c[1].clamp(0.0, 1.0) } else { self.color[1] },
                if c[2].is_finite() { c[2].clamp(0.0, 1.0) } else { self.color[2] },
            ],
            _ => self.color,
        };
        Self {
            id: self.id,
            name: self.name.clone(),
            enabled: self.enabled,
            kind: self.kind,
            color,
            intensity: self.eval_float("intensity", self.intensity, t).clamp(0.0, 20.0),
            position: [
                self.eval_float("x", self.position[0], t),
                self.eval_float("y", self.position[1], t),
                self.eval_float("z", self.position[2], t),
            ],
            angle: self.eval_float("angle", self.angle, t),
            elevation: self.eval_float("elevation", self.elevation, t).clamp(-90.0, 90.0),
            range: self.eval_float("range", self.range, t).max(0.0),
            cone: self.eval_float("cone", self.cone, t).clamp(1.0, 89.0),
            softness: self.eval_float("softness", self.softness, t).clamp(0.0, 1.0),
            tracks: BTreeMap::new(),
        }
    }

    /// Whether any light parameter has active keyframes.
    pub fn is_animated(&self) -> bool {
        self.tracks.values().any(Param::is_animated)
    }

    /// Sorted deduplicated timeline ticks where this light has keyframes.
    pub fn keyframe_times(&self) -> Vec<Tick> {
        let mut times: Vec<Tick> = self.tracks.values().flat_map(|p| p.keyframes.iter().map(|k| k.time)).collect();
        times.sort();
        times.dedup();
        times
    }

    /// Whether any animated track on this light has a keyframe at `t`.
    pub fn has_keyframe_at(&self, t: Tick) -> bool {
        self.tracks.values().any(|p| p.keyframes.iter().any(|k| k.time == t))
    }

    /// Active interpolation curve for keyframes at or immediately preceding `t`.
    pub fn interpolation_at(&self, t: Tick) -> Interpolation {
        for p in self.tracks.values() {
            if let Some(k) = p.keyframes.iter().find(|k| k.time == t) {
                return k.interp;
            }
        }
        for p in self.tracks.values() {
            if let Some(k) = p.keyframes.iter().rev().find(|k| k.time <= t).or_else(|| p.keyframes.first()) {
                return k.interp;
            }
        }
        Interpolation::Linear
    }

    /// Toggle keyframe animation across all light parameters at sequence time `t`.
    /// When turning animation off, the evaluated pose at `t` becomes the light's static state.
    pub fn toggle_animation(&mut self, t: Tick) -> bool {
        if self.is_animated() {
            let snap = self.evaluated_at(t);
            self.color = snap.color;
            self.intensity = snap.intensity;
            self.position = snap.position;
            self.angle = snap.angle;
            self.elevation = snap.elevation;
            self.range = snap.range;
            self.cone = snap.cone;
            self.softness = snap.softness;
            self.tracks.clear();
            false
        } else {
            self.add_keyframe_at(t, Interpolation::Linear);
            true
        }
    }

    /// Stamp a keyframe across all animatable light parameters at `t` using their current
    /// evaluated values at `t`.
    pub fn add_keyframe_at(&mut self, t: Tick, interp: Interpolation) {
        let snap = self.evaluated_at(t);
        for &id in Self::ANIMATED_PARAMS {
            let Some(val) = snap.static_param_value(id) else { continue };
            let entry = self.tracks.entry(id.to_string()).or_insert_with(|| Param::new(val.clone()));
            entry.value = val.clone();
            entry.put_keyframe(t, val);
            if let Some(k) = entry.keyframes.iter_mut().find(|k| k.time == t) {
                k.interp = interp;
            }
        }
    }

    /// Remove any keyframes at exact time `t`. If the last keyframe is removed, snapshots the
    /// values into static fields and clears `tracks`.
    pub fn remove_keyframe_at(&mut self, t: Tick) -> bool {
        let snap = self.evaluated_at(t);
        let mut removed = false;
        for p in self.tracks.values_mut() {
            if p.remove_keyframe_at(t) {
                removed = true;
            }
        }
        self.tracks.retain(|_, p| p.is_animated());
        if self.tracks.is_empty() {
            self.color = snap.color;
            self.intensity = snap.intensity;
            self.position = snap.position;
            self.angle = snap.angle;
            self.elevation = snap.elevation;
            self.range = snap.range;
            self.cone = snap.cone;
            self.softness = snap.softness;
        }
        removed
    }

    /// Set the segment interpolation curve at `t` (or the active keyframe on/before `t`).
    pub fn set_interpolation_at(&mut self, t: Tick, interp: Interpolation) -> bool {
        let target_time = if self.has_keyframe_at(t) {
            Some(t)
        } else {
            let times = self.keyframe_times();
            times.iter().rev().find(|&&kt| kt <= t).or_else(|| times.first()).copied()
        };
        let Some(kt) = target_time else { return false };
        let mut updated = false;
        for p in self.tracks.values_mut() {
            if let Some(k) = p.keyframes.iter_mut().find(|k| k.time == kt) {
                k.interp = interp;
                updated = true;
            }
        }
        updated
    }

    /// Apply edited light parameters at timeline `t`. Static fields are always updated; when the
    /// light is animated, any parameter whose value differs from the current evaluated pose at `t`
    /// (or already has a keyframe at `t`) writes a keyframe at `t`, preserving the active curve.
    pub fn update_from_edit(&mut self, edited: &Light3D, t: Tick) {
        let prev = self.evaluated_at(t);
        let interp = self.interpolation_at(t);
        self.name.clone_from(&edited.name);
        self.enabled = edited.enabled;
        self.kind = edited.kind;
        self.color = edited.color;
        self.intensity = edited.intensity;
        self.position = edited.position;
        self.angle = edited.angle;
        self.elevation = edited.elevation;
        self.range = edited.range;
        self.cone = edited.cone;
        self.softness = edited.softness;

        if !self.is_animated() {
            return;
        }
        for &id in Self::ANIMATED_PARAMS {
            let Some(next_val) = edited.static_param_value(id) else { continue };
            let Some(prev_val) = prev.static_param_value(id) else { continue };
            let changed = !param_values_close(&prev_val, &next_val);
            if let Some(p) = self.tracks.get_mut(id) {
                p.value = next_val.clone();
                if changed || p.keyframes.iter().any(|k| k.time == t) {
                    p.set_at(t, next_val);
                }
            } else if changed {
                let mut p = Param::new(prev_val);
                p.put_keyframe(t, next_val);
                if let Some(k) = p.keyframes.iter_mut().find(|k| k.time == t) {
                    k.interp = interp;
                }
                self.tracks.insert(id.to_string(), p);
            }
        }
    }
}

fn param_values_close(a: &ParamValue, b: &ParamValue) -> bool {
    match (a, b) {
        (ParamValue::Float(x), ParamValue::Float(y)) => (x - y).abs() <= 1e-4,
        (ParamValue::Color(x), ParamValue::Color(y)) => {
            (x[0] - y[0]).abs() <= 1e-4 && (x[1] - y[1]).abs() <= 1e-4 && (x[2] - y[2]).abs() <= 1e-4
        }
        _ => a == b,
    }
}

/// The lighting of a sequence's 3D layers. When `lights` is empty the renderers fall back to the
/// classic single light each object carries, so nothing changes for projects made before lights.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SceneLighting {
    /// The colour that fills the shadows, multiplied by [`Self::ambient_level`].
    pub ambient_color: [f32; 3],
    pub ambient_level: f32,
    pub lights: Vec<Light3D>,
}

impl Default for SceneLighting {
    fn default() -> Self {
        SceneLighting { ambient_color: [1.0; 3], ambient_level: 1.0, lights: Vec::new() }
    }
}

impl SceneLighting {
    /// Nothing custom: keep the file small and the renderer on its classic single light.
    pub fn is_default(&self) -> bool {
        self.lights.is_empty() && self.ambient_level == 1.0 && self.ambient_color == [1.0; 3]
    }

    /// Evaluate every light's keyframe curves at sequence timeline `t`.
    pub fn evaluated_at(&self, t: Tick) -> Self {
        Self {
            ambient_color: self.ambient_color,
            ambient_level: self.ambient_level,
            lights: self.lights.iter().map(|l| l.evaluated_at(t)).collect(),
        }
    }
}

/// The vertical field of view in degrees whose focal equals the frame height — the classic
/// fixed camera's (`focal = frame height` ⇔ `fov = 2·atan(0.5)`).
pub const CLASSIC_FOV: f32 = 53.130_102;

/// The app's rotation, matching the object/gizmo convention exactly (`Rx`, then `Ry`, then `Rz`):
/// `v' = Rz(Ry(Rx(v)))`. Angles in degrees.
fn rotate_xyz(v: [f32; 3], rx: f32, ry: f32, rz: f32) -> [f32; 3] {
    let (sx, cx) = rx.to_radians().sin_cos();
    let (sy, cy) = ry.to_radians().sin_cos();
    let (sz, cz) = rz.to_radians().sin_cos();
    let y = v[1] * cx - v[2] * sx;
    let z = v[1] * sx + v[2] * cx;
    let x = v[0] * cy + z * sy;
    let z = -v[0] * sy + z * cy;
    [x * cz - y * sz, x * sz + y * cz, z]
}

/// The camera a sequence's 3D layers render through, when one is added. Absent: the classic
/// fixed camera (frame-centred principal point, frame-height focal, looking down +Z from the
/// origin). Adding one reproduces the classic look exactly; moving it makes it a real camera.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SceneCamera3D {
    /// Eye position in the scene's world units: frame pixels, frame-centred, at the object
    /// placement depth. x to the right, y down, z away from the viewer (the render direction).
    pub position: [f32; 3],
    /// Degrees: yaw around the vertical axis, pitch around the horizontal, roll around the view.
    pub yaw: f32,
    pub pitch: f32,
    pub roll: f32,
    /// Vertical field of view in degrees. The classic camera's is [`CLASSIC_FOV`].
    pub fov: f32,
}

impl Default for SceneCamera3D {
    fn default() -> Self {
        Self { position: [0.0; 3], yaw: 0.0, pitch: 0.0, roll: 0.0, fov: CLASSIC_FOV }
    }
}

impl SceneCamera3D {
    /// Whether this camera renders exactly like the classic fixed camera.
    pub fn is_classic(&self) -> bool {
        *self == Self::default()
    }

    /// Focal length in pixels for a frame `height` pixels tall.
    pub fn focal(&self, height: f32) -> f32 {
        (height * 0.5) / (self.fov.clamp(1.0, 179.0).to_radians() * 0.5).tan()
    }

    /// The camera's axes in world space: right, down, forward (row-major). The view rotation
    /// `Rᵀ` is these rows directly: `v' = [dot(right, w - eye), dot(down, w - eye),
    /// dot(forward, w - eye)]`.
    pub fn view_rotation(&self) -> [[f32; 3]; 3] {
        [
            rotate_xyz([1.0, 0.0, 0.0], self.pitch, self.yaw, self.roll),
            rotate_xyz([0.0, 1.0, 0.0], self.pitch, self.yaw, self.roll),
            rotate_xyz([0.0, 0.0, 1.0], self.pitch, self.yaw, self.roll),
        ]
    }

    /// The view matrix (world-to-camera), `[Rᵀ | -Rᵀ·eye]`. Column-major, ready for WGSL's
    /// `mat4x4`: `view * vec4(p, 1)` maps world to camera space.
    pub fn view_matrix(&self) -> [[f32; 4]; 4] {
        let vr = self.view_rotation();
        let t = [
            -(vr[0][0] * self.position[0] + vr[0][1] * self.position[1] + vr[0][2] * self.position[2]),
            -(vr[1][0] * self.position[0] + vr[1][1] * self.position[1] + vr[1][2] * self.position[2]),
            -(vr[2][0] * self.position[0] + vr[2][1] * self.position[1] + vr[2][2] * self.position[2]),
        ];
        // WGSL reads a mat4x4 as four columns; the columns are Vr's columns (Vr's rows are the
        // world-space camera axes).
        [
            [vr[0][0], vr[1][0], vr[2][0], 0.0],
            [vr[0][1], vr[1][1], vr[2][1], 0.0],
            [vr[0][2], vr[1][2], vr[2][2], 0.0],
            [t[0], t[1], t[2], 1.0],
        ]
    }
}
