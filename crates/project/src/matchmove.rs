//! Offline 3D camera match-move solves shared by a sequence.
//!
//! A [`CameraSolve`] records per-frame solved camera poses in clip-relative time alongside a
//! sparse 3D point cloud in a normalised world frame (frame 0 at the origin looking down `+Z`,
//! median positive depth at frame 0 equal to `1.0`). When a sequence enables a solve through
//! [`SequenceMatchMove`], [`crate::Sequence::matchmove_camera_at`] materialises a
//! [`SceneCamera3D`] scaled into FilmCraft's pixel world units with optional orientation trims.

use filmcraft_time::Tick;
use serde::{Deserialize, Serialize};

use crate::{CLASSIC_FOV, ClipId, SceneCamera3D, StereoLayout};

fn one() -> f32 {
    1.0
}

fn default_fov() -> f32 {
    CLASSIC_FOV
}

fn default_scale_px() -> f32 {
    SequenceMatchMove::DEFAULT_SCALE_PX
}

fn default_true() -> bool {
    true
}

fn finite_or(v: f32, fallback: f32) -> f32 {
    if v.is_finite() { v } else { fallback }
}

/// One solved camera pose at clip-relative `time`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SolvedFrame {
    pub time: Tick,
    /// Camera eye position in normalised solve units (multiplied by `scale_px` when applied).
    pub position: [f32; 3],
    /// Camera yaw in degrees, matching [`SceneCamera3D::yaw`].
    pub yaw: f32,
    /// Camera pitch in degrees, matching [`SceneCamera3D::pitch`].
    pub pitch: f32,
    /// Camera roll in degrees, matching [`SceneCamera3D::roll`].
    pub roll: f32,
    /// Vertical field of view in degrees.
    #[serde(default = "default_fov")]
    pub fov: f32,
    /// Reprojection root-mean-square error in working pixels.
    #[serde(default)]
    pub rms_px: f32,
    /// Per-frame lock confidence in `0..=1`. Below [`CameraSolve::LOST_CONFIDENCE`] (`0.25`) the
    /// frame counts as lost and [`crate::Sequence::matchmove_camera_at`] holds the last confident pose.
    #[serde(default = "one")]
    pub confidence: f32,
}

impl Default for SolvedFrame {
    fn default() -> Self {
        Self { time: Tick::ZERO, position: [0.0; 3], yaw: 0.0, pitch: 0.0, roll: 0.0, fov: CLASSIC_FOV, rms_px: 0.0, confidence: 1.0 }
    }
}

impl SolvedFrame {
    /// Whether this frame's lock confidence meets [`CameraSolve::LOST_CONFIDENCE`].
    pub fn is_confident(&self) -> bool {
        self.confidence.is_finite() && self.confidence >= CameraSolve::LOST_CONFIDENCE
    }

    /// Whether all numeric fields on this frame are finite and `time` is within supported tick bounds.
    pub fn is_finite(&self) -> bool {
        (Tick::MIN..=Tick::MAX).contains(&self.time)
            && self.position.iter().all(|v| v.is_finite())
            && self.yaw.is_finite()
            && self.pitch.is_finite()
            && self.roll.is_finite()
            && self.fov.is_finite()
            && self.rms_px.is_finite()
            && self.confidence.is_finite()
    }

    /// Smoothly interpolate two solved frames, using shortest-arc quaternion SLERP on `SO(3)` so
    /// orientation never jumps across `±180°` Euler branch cuts.
    pub fn lerp(a: Self, b: Self, t: f32) -> Self {
        if !t.is_finite() || t <= 0.0 {
            return a;
        }
        if t >= 1.0 {
            return b;
        }
        let l = |x: f32, y: f32| x + (y - x) * t;
        let (pitch, yaw, roll) = if a.yaw == b.yaw && a.pitch == b.pitch && a.roll == b.roll {
            (a.pitch, a.yaw, a.roll)
        } else {
            let va = SceneCamera3D { position: [0.0; 3], yaw: a.yaw, pitch: a.pitch, roll: a.roll, fov: CLASSIC_FOV }.view_rotation();
            let vb = SceneCamera3D { position: [0.0; 3], yaw: b.yaw, pitch: b.pitch, roll: b.roll, fov: CLASSIC_FOV }.view_rotation();
            let q = quat_slerp(quat_from_mat3(va), quat_from_mat3(vb), t);
            euler_from_view_rotation(mat3_from_quat(q))
        };
        let dt = (b.time.0 as f64 - a.time.0 as f64) * f64::from(t);
        Self {
            time: Tick(a.time.0.saturating_add(dt.round() as i64)),
            position: [l(a.position[0], b.position[0]), l(a.position[1], b.position[1]), l(a.position[2], b.position[2])],
            yaw,
            pitch,
            roll,
            fov: l(a.fov, b.fov),
            rms_px: l(a.rms_px, b.rms_px),
            confidence: l(a.confidence, b.confidence),
        }
    }

    /// C1-continuous 4-point cubic interpolation between `p1` (`t = 0`) and `p2` (`t = 1`) using
    /// neighbors `p0` and `p3`: Catmull-Rom spline for eye position & FOV, and Shoemake's
    /// spherical cubic quaternion spline (SQUAD) on `SO(3)` for orientation. Designed for smooth
    /// playback across sampled COLMAP / GLOMAP keyframes.
    pub fn catmull_rom(p0: Self, p1: Self, p2: Self, p3: Self, t: f32) -> Self {
        if !t.is_finite() || t <= 0.0 {
            return p1;
        }
        if t >= 1.0 {
            return p2;
        }
        let t2 = t * t;
        let t3 = t2 * t;
        let cr = |a: f32, b: f32, c: f32, d: f32| -> f32 {
            0.5 * ((2.0 * b) + (-a + c) * t + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2 + (-a + 3.0 * b - 3.0 * c + d) * t3)
        };
        let position = [
            cr(p0.position[0], p1.position[0], p2.position[0], p3.position[0]),
            cr(p0.position[1], p1.position[1], p2.position[1], p3.position[1]),
            cr(p0.position[2], p1.position[2], p2.position[2], p3.position[2]),
        ];
        let q_of = |f: Self| quat_from_mat3(SceneCamera3D { position: [0.0; 3], yaw: f.yaw, pitch: f.pitch, roll: f.roll, fov: CLASSIC_FOV }.view_rotation());
        let q = quat_squad(q_of(p0), q_of(p1), q_of(p2), q_of(p3), t);
        let (pitch, yaw, roll) = euler_from_view_rotation(mat3_from_quat(q));
        let dt = (p2.time.0 as f64 - p1.time.0 as f64) * f64::from(t);
        let l = |x: f32, y: f32| x + (y - x) * t;
        Self {
            time: Tick(p1.time.0.saturating_add(dt.round() as i64)),
            position,
            yaw,
            pitch,
            roll,
            fov: cr(p0.fov, p1.fov, p2.fov, p3.fov).clamp(5.0, 170.0),
            rms_px: l(p1.rms_px, p2.rms_px),
            confidence: l(p1.confidence, p2.confidence).clamp(0.0, 1.0),
        }
    }
}

/// One sparse 3D feature landmark reconstructed during bundle adjustment, in normalised solve units.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SolvedPoint {
    pub position: [f32; 3],
    pub observations: u32,
    pub rms_px: f32,
}

impl Default for SolvedPoint {
    fn default() -> Self {
        Self { position: [0.0, 0.0, 1.0], observations: 0, rms_px: 0.0 }
    }
}

impl SolvedPoint {
    pub fn is_finite(&self) -> bool {
        self.position.iter().all(|v| v.is_finite()) && self.rms_px.is_finite()
    }
}

/// Offline 3D camera solve associated with a timeline clip.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraSolve {
    pub id: u64,
    pub clip: ClipId,
    pub name: String,
    #[serde(default)]
    pub layout: StereoLayout,
    #[serde(default)]
    pub rotation_only: bool,
    #[serde(default)]
    pub shots_skipped: u32,
    pub frames: Vec<SolvedFrame>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub points: Vec<SolvedPoint>,
}

impl CameraSolve {
    /// Hard upper bound on solved frames stored in a single solve (`60 s` at `60 fps`).
    pub const MAX_FRAMES: usize = 3600;
    /// Hard upper bound on sparse 3D points stored in a single solve.
    pub const MAX_POINTS: usize = 256;
    /// Confidence threshold below which a frame is treated as lost track.
    pub const LOST_CONFIDENCE: f32 = 0.25;

    /// Sample at clip-relative time `t`, clamping outside the solved range and interpolating
    /// smoothly with Catmull-Rom + quaternion SQUAD across sampled COLMAP / GLOMAP keyframes.
    pub fn sample_at(&self, t: Tick) -> Option<SolvedFrame> {
        let f = &self.frames;
        if f.is_empty() {
            return None;
        }
        if t <= f[0].time {
            return Some(f[0]);
        }
        let last = f.len() - 1;
        if t >= f[last].time {
            return Some(f[last]);
        }
        let i = f.partition_point(|k| k.time <= t).saturating_sub(1);
        let p1 = f[i];
        let p2 = f.get(i + 1).copied().unwrap_or(p1);
        let p0 = if i > 0 { f[i - 1] } else { p1 };
        let p3 = f.get(i + 2).copied().unwrap_or(p2);
        let span = p2.time.0.saturating_sub(p1.time.0).max(1) as f32;
        let alpha = ((t.0.saturating_sub(p1.time.0)) as f32 / span).clamp(0.0, 1.0);
        Some(SolvedFrame::catmull_rom(p0, p1, p2, p3, alpha))
    }

    /// Sample at clip-relative time `t`, holding the last confident pose (`confidence >= 0.25`)
    /// across any lost frames and smoothly interpolating via Catmull-Rom + SQUAD across confident ones.
    pub fn confident_sample_at(&self, t: Tick) -> Option<SolvedFrame> {
        let f = &self.frames;
        if f.is_empty() {
            return None;
        }
        let hold = |up_to: usize| -> SolvedFrame {
            f[..=up_to].iter().rev().find(|s| s.is_confident()).or_else(|| f.iter().find(|s| s.is_confident())).copied().unwrap_or(f[0])
        };
        if t <= f[0].time {
            return Some(hold(0));
        }
        let last = f.len() - 1;
        if t >= f[last].time {
            return Some(hold(last));
        }
        let i = f.partition_point(|k| k.time <= t).saturating_sub(1);
        let p1 = f[i];
        let p2 = f.get(i + 1).copied().unwrap_or(p1);
        if !p1.is_confident() || !p2.is_confident() {
            return Some(hold(i));
        }
        let p0 = if i > 0 && f[i - 1].is_confident() { f[i - 1] } else { p1 };
        let p3 = f.get(i + 2).copied().filter(|s| s.is_confident()).unwrap_or(p2);
        let span = p2.time.0.saturating_sub(p1.time.0).max(1) as f32;
        let alpha = ((t.0.saturating_sub(p1.time.0)) as f32 / span).clamp(0.0, 1.0);
        Some(SolvedFrame::catmull_rom(p0, p1, p2, p3, alpha))
    }

    /// Median per-frame reprojection RMS error in working pixels (`0.0` when empty).
    pub fn median_rms(&self) -> f32 {
        median_by(&self.frames, |f| f.rms_px)
    }

    /// Median per-frame confidence (`0.0` when empty).
    pub fn median_confidence(&self) -> f32 {
        median_by(&self.frames, |f| f.confidence)
    }

    /// Count of frames whose confidence fell below [`Self::LOST_CONFIDENCE`].
    pub fn lost_frames(&self) -> u32 {
        self.frames.iter().filter(|f| !f.is_confident()).count() as u32
    }

    /// Whether frame/point counts are within caps and every stored float is finite.
    pub fn is_finite(&self) -> bool {
        self.frames.len() <= Self::MAX_FRAMES
            && self.points.len() <= Self::MAX_POINTS
            && self.frames.iter().all(SolvedFrame::is_finite)
            && self.points.iter().all(SolvedPoint::is_finite)
    }

    /// Estimate the dominant horizontal/tabletop plane `(unit_normal, offset)` (`dot(n, p) = d`,
    /// with `n[1] > 0` pointing along `+Y` down) from reconstructed 3D landmarks via deterministic
    /// 3-point RANSAC. Returns `None` when fewer than 4 usable landmarks form a floor-like plane.
    pub fn fit_ground_plane(&self) -> Option<([f32; 3], f32)> {
        let mut pts: Vec<[f32; 3]> = self.points.iter().filter(|p| p.is_finite() && p.observations >= 3).map(|p| p.position).collect();
        if pts.len() < 4 {
            pts = self.points.iter().filter(|p| p.is_finite()).map(|p| p.position).collect();
        }
        ransac_ground_plane(&pts)
    }
}

fn vec3_sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn vec3_dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn vec3_cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn vec3_unit(v: [f32; 3]) -> Option<[f32; 3]> {
    let len = vec3_dot(v, v).sqrt();
    (len.is_finite() && len > 1e-6).then(|| [v[0] / len, v[1] / len, v[2] / len])
}

fn candidate_floor_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> Option<[f32; 3]> {
    let mut n = vec3_unit(vec3_cross(vec3_sub(b, a), vec3_sub(c, a)))?;
    if n[1] < 0.0 {
        n = [-n[0], -n[1], -n[2]];
    }
    (n[1] >= 0.35).then_some(n)
}

fn ransac_ground_plane(pts: &[[f32; 3]]) -> Option<([f32; 3], f32)> {
    let n_pts = pts.len();
    if n_pts < 4 {
        return None;
    }
    let tol = 0.06f32;
    let mut seed = 0x243f_6a88_85a3_08d3u64;
    let mut next_idx = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed as usize) % n_pts
    };
    let mut best: Option<(usize, f32, [f32; 3], f32)> = None;
    for _ in 0..192 {
        let (i0, i1, i2) = (next_idx(), next_idx(), next_idx());
        if i0 == i1 || i1 == i2 || i0 == i2 {
            continue;
        }
        let Some(n) = candidate_floor_normal(pts[i0], pts[i1], pts[i2]) else { continue };
        let d = vec3_dot(n, pts[i0]);
        let (mut count, mut err_sum) = (0usize, 0.0f32);
        for &p in pts {
            let r = (vec3_dot(n, p) - d).abs();
            if r <= tol {
                count += 1;
                err_sum += r;
            }
        }
        if count >= 4 && best.is_none_or(|(bc, be, _, _)| count > bc || (count == bc && err_sum < be)) {
            best = Some((count, err_sum, n, d));
        }
    }
    best.map(|(_, _, n, d)| (n, d))
}

fn median_by(frames: &[SolvedFrame], mut pick: impl FnMut(&SolvedFrame) -> f32) -> f32 {
    if frames.is_empty() {
        return 0.0;
    }
    let mut vals: Vec<f32> = frames.iter().map(&mut pick).filter(|v| v.is_finite()).collect();
    if vals.is_empty() {
        return 0.0;
    }
    vals.sort_by(f32::total_cmp);
    let mid = vals.len() / 2;
    if vals.len() % 2 == 1 { vals[mid] } else { 0.5 * (vals[mid - 1] + vals[mid]) }
}

/// Sequence-level binding that drives 3D layers from a [`CameraSolve`].
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SequenceMatchMove {
    pub solve: u64,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_scale_px")]
    pub scale_px: f32,
    #[serde(default)]
    pub yaw_trim: f32,
    #[serde(default)]
    pub pitch_trim: f32,
    #[serde(default)]
    pub roll_trim: f32,
    #[serde(default)]
    pub offset: [f32; 3],
}

impl SequenceMatchMove {
    /// Default world-unit scale in pixels (normalised depth `1.0` maps to `700` scene pixels).
    pub const DEFAULT_SCALE_PX: f32 = 700.0;

    pub fn new(solve: u64) -> Self {
        Self {
            solve,
            enabled: true,
            scale_px: Self::DEFAULT_SCALE_PX,
            yaw_trim: 0.0,
            pitch_trim: 0.0,
            roll_trim: 0.0,
            offset: [0.0; 3],
        }
    }

    /// Whether every stored float is finite and `scale_px` is non-negative.
    pub fn is_finite(&self) -> bool {
        self.scale_px.is_finite()
            && self.scale_px >= 0.0
            && self.yaw_trim.is_finite()
            && self.pitch_trim.is_finite()
            && self.roll_trim.is_finite()
            && self.offset.iter().all(|v| v.is_finite())
    }

    /// Materialise a [`SceneCamera3D`] from a solved frame, applying `scale_px`, rigid
    /// world-orientation trims (`yaw_trim`, `pitch_trim`, `roll_trim`), and world `offset`.
    pub fn apply_to_sample(&self, sample: &SolvedFrame) -> SceneCamera3D {
        let scale = if self.scale_px.is_finite() { self.scale_px.max(0.0) } else { Self::DEFAULT_SCALE_PX };
        let pos = [finite_or(sample.position[0], 0.0) * scale, finite_or(sample.position[1], 0.0) * scale, finite_or(sample.position[2], 0.0) * scale];
        let off = [finite_or(self.offset[0], 0.0), finite_or(self.offset[1], 0.0), finite_or(self.offset[2], 0.0)];
        let yaw_s = finite_or(sample.yaw, 0.0);
        let pitch_s = finite_or(sample.pitch, 0.0);
        let roll_s = finite_or(sample.roll, 0.0);
        let fov = finite_or(sample.fov, CLASSIC_FOV).clamp(1.0, 179.0);
        let yt = finite_or(self.yaw_trim, 0.0);
        let pt = finite_or(self.pitch_trim, 0.0);
        let rt = finite_or(self.roll_trim, 0.0);
        if yt == 0.0 && pt == 0.0 && rt == 0.0 {
            return SceneCamera3D { position: [pos[0] + off[0], pos[1] + off[1], pos[2] + off[2]], yaw: yaw_s, pitch: pitch_s, roll: roll_s, fov };
        }
        let vr_s = SceneCamera3D { position: [0.0; 3], yaw: yaw_s, pitch: pitch_s, roll: roll_s, fov }.view_rotation();
        let vr_t = SceneCamera3D { position: [0.0; 3], yaw: yt, pitch: pt, roll: rt, fov }.view_rotation();
        let vr = mat3_mul(vr_s, vr_t);
        let rotated = mat3_transpose_mul_vec(vr_t, pos);
        let position = [rotated[0] + off[0], rotated[1] + off[1], rotated[2] + off[2]];
        let (pitch, yaw, roll) = euler_from_view_rotation(vr);
        SceneCamera3D { position, yaw, pitch, roll, fov }
    }

    /// Transform a solver-normalised 3D landmark into world pixel coordinates under `scale_px`,
    /// rigid world-orientation trims (`yaw_trim`, `pitch_trim`, `roll_trim`), and world `offset`.
    pub fn apply_to_point(&self, point: [f32; 3]) -> [f32; 3] {
        let scale = if self.scale_px.is_finite() { self.scale_px.max(0.0) } else { Self::DEFAULT_SCALE_PX };
        let p = [finite_or(point[0], 0.0) * scale, finite_or(point[1], 0.0) * scale, finite_or(point[2], 0.0) * scale];
        let off = [finite_or(self.offset[0], 0.0), finite_or(self.offset[1], 0.0), finite_or(self.offset[2], 0.0)];
        let yt = finite_or(self.yaw_trim, 0.0);
        let pt = finite_or(self.pitch_trim, 0.0);
        let rt = finite_or(self.roll_trim, 0.0);
        let rotated = if yt == 0.0 && pt == 0.0 && rt == 0.0 {
            p
        } else {
            let vr_t = SceneCamera3D { position: [0.0; 3], yaw: yt, pitch: pt, roll: rt, fov: CLASSIC_FOV }.view_rotation();
            mat3_transpose_mul_vec(vr_t, p)
        };
        [rotated[0] + off[0], rotated[1] + off[1], rotated[2] + off[2]]
    }

    /// Adjust `(yaw_trim, pitch_trim, roll_trim)` so a raw solver floor normal `normal` maps onto
    /// world `+Y` (`[0, 1, 0]`) while preserving the current horizontal `yaw_trim` heading.
    pub fn level_to_normal(&mut self, normal: [f32; 3]) -> bool {
        let Some(mut d) = vec3_unit(normal) else { return false };
        if d[1] < 0.0 {
            d = [-d[0], -d[1], -d[2]];
        }
        let yaw_rad = finite_or(self.yaw_trim, 0.0).to_radians();
        let heading = [yaw_rad.sin(), 0.0, yaw_rad.cos()];
        let r = vec3_unit(vec3_cross(d, heading)).or_else(|| vec3_unit(vec3_cross(d, [0.0, 0.0, 1.0]))).or_else(|| vec3_unit(vec3_cross(d, [1.0, 0.0, 0.0])));
        let Some(r) = r else { return false };
        let Some(f) = vec3_unit(vec3_cross(r, d)) else { return false };
        // Columns `[r, d, f]` give `vr_t^T · d = [0, 1, 0]` in world coordinates.
        let vr_t = [[r[0], d[0], f[0]], [r[1], d[1], f[1]], [r[2], d[2], f[2]]];
        let (pitch, yaw, roll) = euler_from_view_rotation(vr_t);
        self.pitch_trim = pitch;
        self.yaw_trim = yaw;
        self.roll_trim = roll;
        true
    }
}

impl Default for SequenceMatchMove {
    fn default() -> Self {
        Self::new(0)
    }
}

/// One solved frame materialised at sequence timeline time in calibrated world coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EvaluatedMatchMoveFrame {
    pub timeline: Tick,
    pub camera: SceneCamera3D,
    pub rms_px: f32,
    pub confidence: f32,
    pub lost: bool,
}

/// Recover `(pitch, yaw, roll)` in degrees from a [`SceneCamera3D::view_rotation`] matrix `R_wc`.
///
/// `SceneCamera3D::view_rotation` builds rows `[right, down, forward]` via `Rz(roll) · Ry(yaw) · Rx(pitch)`
/// applied to the basis vectors, so `vr[0][2] = -sin(yaw)`. Across all of `SO(3)` — including
/// gimbal lock at `yaw = ±90°` — feeding the returned angles back into `SceneCamera3D::view_rotation`
/// reproduces `vr`.
pub fn euler_from_view_rotation(vr: [[f32; 3]; 3]) -> (f32, f32, f32) {
    let sy = (-vr[0][2]).clamp(-1.0, 1.0);
    let yaw = sy.asin();
    let cy = yaw.cos();
    let (pitch, roll) = if cy.abs() > 1e-6 { (vr[1][2].atan2(vr[2][2]), vr[0][1].atan2(vr[0][0])) } else { (0.0, (-vr[1][0]).atan2(vr[1][1])) };
    (pitch.to_degrees(), yaw.to_degrees(), roll.to_degrees())
}

fn mat3_mul(a: [[f32; 3]; 3], b: [[f32; 3]; 3]) -> [[f32; 3]; 3] {
    let mut out = [[0.0f32; 3]; 3];
    for r in 0..3 {
        for c in 0..3 {
            out[r][c] = a[r][0] * b[0][c] + a[r][1] * b[1][c] + a[r][2] * b[2][c];
        }
    }
    out
}

fn mat3_transpose_mul_vec(m: [[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    [m[0][0] * v[0] + m[1][0] * v[1] + m[2][0] * v[2], m[0][1] * v[0] + m[1][1] * v[1] + m[2][1] * v[2], m[0][2] * v[0] + m[1][2] * v[1] + m[2][2] * v[2]]
}

/// Unit quaternion `[w, x, y, z]` from a proper orthogonal 3×3 matrix (Shepperd's method).
fn quat_from_mat3(m: [[f32; 3]; 3]) -> [f32; 4] {
    let tr = m[0][0] + m[1][1] + m[2][2];
    let q = if tr > 0.0 {
        let s = (tr + 1.0).sqrt() * 2.0;
        [0.25 * s, (m[2][1] - m[1][2]) / s, (m[0][2] - m[2][0]) / s, (m[1][0] - m[0][1]) / s]
    } else if m[0][0] > m[1][1] && m[0][0] > m[2][2] {
        let s = (1.0 + m[0][0] - m[1][1] - m[2][2]).sqrt() * 2.0;
        [(m[2][1] - m[1][2]) / s, 0.25 * s, (m[0][1] + m[1][0]) / s, (m[0][2] + m[2][0]) / s]
    } else if m[1][1] > m[2][2] {
        let s = (1.0 + m[1][1] - m[0][0] - m[2][2]).sqrt() * 2.0;
        [(m[0][2] - m[2][0]) / s, (m[0][1] + m[1][0]) / s, 0.25 * s, (m[1][2] + m[2][1]) / s]
    } else {
        let s = (1.0 + m[2][2] - m[0][0] - m[1][1]).sqrt() * 2.0;
        [(m[1][0] - m[0][1]) / s, (m[0][2] + m[2][0]) / s, (m[1][2] + m[2][1]) / s, 0.25 * s]
    };
    quat_normalize(q)
}

fn quat_normalize(q: [f32; 4]) -> [f32; 4] {
    let n = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if n > 1e-12 { [q[0] / n, q[1] / n, q[2] / n, q[3] / n] } else { [1.0, 0.0, 0.0, 0.0] }
}

fn quat_slerp(a: [f32; 4], mut b: [f32; 4], t: f32) -> [f32; 4] {
    let mut dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
    if dot < 0.0 {
        b = [-b[0], -b[1], -b[2], -b[3]];
        dot = -dot;
    }
    if dot > 0.9995 {
        return quat_normalize([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t, a[3] + (b[3] - a[3]) * t]);
    }
    let theta = dot.clamp(-1.0, 1.0).acos();
    let sin_theta = theta.sin().max(1e-6);
    let wa = ((1.0 - t) * theta).sin() / sin_theta;
    let wb = (t * theta).sin() / sin_theta;
    quat_normalize([a[0] * wa + b[0] * wb, a[1] * wa + b[1] * wb, a[2] * wa + b[2] * wb, a[3] * wa + b[3] * wb])
}

fn quat_hemisphere(ref_q: [f32; 4], q: [f32; 4]) -> [f32; 4] {
    if ref_q[0] * q[0] + ref_q[1] * q[1] + ref_q[2] * q[2] + ref_q[3] * q[3] < 0.0 {
        [-q[0], -q[1], -q[2], -q[3]]
    } else {
        q
    }
}

fn quat_mul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[0] * b[0] - a[1] * b[1] - a[2] * b[2] - a[3] * b[3],
        a[0] * b[1] + a[1] * b[0] + a[2] * b[3] - a[3] * b[2],
        a[0] * b[2] - a[1] * b[3] + a[2] * b[0] + a[3] * b[1],
        a[0] * b[3] + a[1] * b[2] - a[2] * b[1] + a[3] * b[0],
    ]
}

fn quat_conj(q: [f32; 4]) -> [f32; 4] {
    [q[0], -q[1], -q[2], -q[3]]
}

fn quat_log(q: [f32; 4]) -> [f32; 3] {
    let v_len = (q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if v_len <= 1e-7 {
        [q[1], q[2], q[3]]
    } else {
        let theta = v_len.atan2(q[0]);
        let s = theta / v_len;
        [q[1] * s, q[2] * s, q[3] * s]
    }
}

fn quat_exp(v: [f32; 3]) -> [f32; 4] {
    let theta = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if theta <= 1e-7 {
        quat_normalize([1.0, v[0], v[1], v[2]])
    } else {
        let (st, ct) = theta.sin_cos();
        let s = st / theta;
        quat_normalize([ct, v[0] * s, v[1] * s, v[2] * s])
    }
}

fn quat_squad_tangent(q_prev: [f32; 4], q_curr: [f32; 4], q_next: [f32; 4]) -> [f32; 4] {
    let inv = quat_conj(q_curr);
    let l1 = quat_log(quat_hemisphere([1.0, 0.0, 0.0, 0.0], quat_mul(inv, q_next)));
    let l2 = quat_log(quat_hemisphere([1.0, 0.0, 0.0, 0.0], quat_mul(inv, q_prev)));
    let step = quat_exp([-0.25 * (l1[0] + l2[0]), -0.25 * (l1[1] + l2[1]), -0.25 * (l1[2] + l2[2])]);
    quat_normalize(quat_mul(q_curr, step))
}

fn quat_squad(q0_in: [f32; 4], q1: [f32; 4], q2_in: [f32; 4], q3_in: [f32; 4], t: f32) -> [f32; 4] {
    let q0 = quat_hemisphere(q1, q0_in);
    let q2 = quat_hemisphere(q1, q2_in);
    let q3 = quat_hemisphere(q2, q3_in);
    let s1 = quat_squad_tangent(q0, q1, q2);
    let s2 = quat_squad_tangent(q1, q2, q3);
    let slerp_q = quat_slerp(q1, q2, t);
    let slerp_s = quat_slerp(s1, s2, t);
    quat_slerp(slerp_q, slerp_s, 2.0 * t * (1.0 - t))
}

fn mat3_from_quat(q: [f32; 4]) -> [[f32; 3]; 3] {
    let [w, x, y, z] = quat_normalize(q);
    let (xx, yy, zz) = (x * x, y * y, z * z);
    let (xy, xz, yz) = (x * y, x * z, y * z);
    let (wx, wy, wz) = (w * x, w * y, w * z);
    [
        [1.0 - 2.0 * (yy + zz), 2.0 * (xy - wz), 2.0 * (xz + wy)],
        [2.0 * (xy + wz), 1.0 - 2.0 * (xx + zz), 2.0 * (yz - wx)],
        [2.0 * (xz - wy), 2.0 * (yz + wx), 1.0 - 2.0 * (xx + yy)],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ItemKind, Label, Project, SequenceSettings, TrackKind};
    use filmcraft_time::{FrameRate, TimeRange};

    fn next_u64(state: &mut u64) -> u64 {
        let mut x = *state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *state = x;
        x
    }

    fn rand_f32(state: &mut u64, lo: f32, hi: f32) -> f32 {
        let u = (next_u64(state) >> 40) as f32 / ((1u64 << 24) as f32);
        lo + (hi - lo) * u
    }

    #[test]
    fn euler_view_rotation_roundtrips_10000_random_and_gimbal_poses() {
        let mut seed = 0x9e37_79b9_7f4a_7c15u64;
        for i in 0..10_000 {
            let (pitch, yaw, roll) = if i < 200 {
                // Stress gimbal lock at yaw = ±90° as well as ±180° roll/pitch boundaries.
                let yaw = if i % 2 == 0 { 90.0 } else { -90.0 };
                (rand_f32(&mut seed, -180.0, 180.0), yaw, rand_f32(&mut seed, -180.0, 180.0))
            } else {
                (rand_f32(&mut seed, -180.0, 180.0), rand_f32(&mut seed, -89.9, 89.9), rand_f32(&mut seed, -180.0, 180.0))
            };
            let cam = SceneCamera3D { position: [0.0; 3], yaw, pitch, roll, fov: CLASSIC_FOV };
            let vr = cam.view_rotation();
            let (p2, y2, r2) = euler_from_view_rotation(vr);
            let vr2 = SceneCamera3D { position: [0.0; 3], yaw: y2, pitch: p2, roll: r2, fov: CLASSIC_FOV }.view_rotation();
            for r in 0..3 {
                for c in 0..3 {
                    let err = (vr[r][c] - vr2[r][c]).abs();
                    assert!(err < 1e-4, "iter {i}: ({pitch}, {yaw}, {roll}) -> ({p2}, {y2}, {r2}), diff[{r}][{c}] = {err}");
                }
            }
            // Check quat_from_mat3 <-> mat3_from_quat agrees with vr as well.
            let vr_q = mat3_from_quat(quat_from_mat3(vr));
            for r in 0..3 {
                for c in 0..3 {
                    let err = (vr[r][c] - vr_q[r][c]).abs();
                    assert!(err < 1e-4, "quat iter {i}: diff[{r}][{c}] = {err}");
                }
            }
        }
    }

    #[test]
    fn sample_at_interpolates_slerps_branch_cut_and_clamps() {
        let solve = CameraSolve {
            id: 1,
            clip: ClipId(2),
            name: "Solve 1".into(),
            layout: StereoLayout::Mono,
            rotation_only: false,
            shots_skipped: 0,
            frames: vec![
                SolvedFrame { time: Tick(0), position: [0.0, 0.0, 0.0], yaw: 0.0, pitch: 0.0, roll: 179.0, fov: 50.0, rms_px: 0.4, confidence: 1.0 },
                SolvedFrame { time: Tick(100), position: [2.0, -1.0, 4.0], yaw: 0.0, pitch: 0.0, roll: -179.0, fov: 60.0, rms_px: 0.8, confidence: 0.8 },
            ],
            points: vec![SolvedPoint { position: [0.1, 0.2, 1.0], observations: 24, rms_px: 0.5 }],
        };
        assert_eq!(solve.sample_at(Tick(-50)).unwrap().position, [0.0, 0.0, 0.0]);
        assert_eq!(solve.sample_at(Tick(250)).unwrap().position, [2.0, -1.0, 4.0]);
        let mid = solve.sample_at(Tick(50)).unwrap();
        assert!((mid.position[0] - 1.0).abs() < 1e-5);
        assert!((mid.fov - 55.0).abs() < 1e-5);
        // Shortest path between +179° and -179° passes through ±180°, never 0°.
        assert!(mid.roll.abs() > 178.5, "expected roll near ±180°, got {}", mid.roll);
        assert!((solve.median_rms() - 0.6).abs() < 1e-6);
        assert!(solve.is_finite());
    }

    #[test]
    fn matchmove_camera_holds_last_confident_pose_scales_and_trims() {
        let mut p = Project::new("MatchMove");
        let rate = FrameRate::FPS_24;
        let layer = p.add_item("Clip", Label::Iris, ItemKind::AdjustmentLayer { width: 1920, height: 1080, rate, duration: rate.tick_of(48) }, None);
        let sid = p.new_sequence("Seq", SequenceSettings::default(), 1, 1, None);
        let clip_start = Tick(1_000);
        let ti = p.make_track_item(layer, TrackKind::Video, clip_start, TimeRange::new(Tick::ZERO, rate.tick_of(48)), rate).unwrap();
        let cid = ti.id;
        p.sequence_mut(sid).unwrap().video_tracks[0].items.push(ti);

        let solve_id = p.alloc_id();
        let solve = CameraSolve {
            id: solve_id,
            clip: cid,
            name: "Camera Match 1".into(),
            layout: StereoLayout::Mono,
            rotation_only: false,
            shots_skipped: 1,
            frames: vec![
                SolvedFrame { time: Tick(0), position: [0.0, 0.0, 0.0], yaw: 0.0, pitch: 0.0, roll: 0.0, fov: CLASSIC_FOV, rms_px: 0.3, confidence: 1.0 },
                SolvedFrame { time: Tick(100), position: [1.0, 0.0, 0.0], yaw: 10.0, pitch: 0.0, roll: 0.0, fov: CLASSIC_FOV, rms_px: 0.4, confidence: 0.9 },
                // Lost frame (< 0.25) with wild outlier pose that must never leak out.
                SolvedFrame { time: Tick(200), position: [99.0, -88.0, 77.0], yaw: 145.0, pitch: -70.0, roll: 90.0, fov: 120.0, rms_px: 9.0, confidence: 0.1 },
                SolvedFrame { time: Tick(300), position: [2.0, 0.0, 0.0], yaw: 20.0, pitch: 0.0, roll: 0.0, fov: CLASSIC_FOV, rms_px: 0.5, confidence: 0.85 },
            ],
            points: Vec::new(),
        };
        {
            let seq = p.sequence_mut(sid).unwrap();
            seq.camera_solves.push(solve);
            assert!(seq.matchmove_camera_at(clip_start).is_none());
            seq.matchmove = Some(SequenceMatchMove { solve: solve_id, enabled: false, scale_px: 500.0, yaw_trim: 0.0, pitch_trim: 0.0, roll_trim: 0.0 });
            assert!(seq.matchmove_camera_at(clip_start).is_none());
            seq.matchmove.as_mut().unwrap().enabled = true;
        }
        let seq = p.sequence(sid).unwrap();
        assert_eq!(seq.camera_solves[0].lost_frames(), 1);
        assert!((seq.camera_solves[0].median_rms() - 0.45).abs() < 1e-6);

        // Confident frame 1 at clip_start + 100 scales position by 500.
        let c1 = seq.matchmove_camera_at(clip_start + Tick(100)).unwrap();
        assert!((c1.position[0] - 500.0).abs() < 1e-4);
        assert!((c1.yaw - 10.0).abs() < 1e-4);

        // Lost frame 2 (and ticks around it before frame 3) hold frame 1's pose.
        for dt in [150, 200, 250] {
            let held = seq.matchmove_camera_at(clip_start + Tick(dt)).unwrap();
            assert!((held.position[0] - 500.0).abs() < 1e-4, "dt={dt}: {held:?}");
            assert!((held.yaw - 10.0).abs() < 1e-4, "dt={dt}: {held:?}");
        }

        // Re-acquired at frame 3.
        let c3 = seq.matchmove_camera_at(clip_start + Tick(300)).unwrap();
        assert!((c3.position[0] - 1000.0).abs() < 1e-4);
        assert!((c3.yaw - 20.0).abs() < 1e-4);

        // Rigid trim invariance: camera-space coordinates of a trimmed world point stay identical.
        let mut trimmed = seq.matchmove.unwrap();
        trimmed.yaw_trim = 15.0;
        trimmed.pitch_trim = -8.0;
        trimmed.roll_trim = 6.0;
        let r_trim = SceneCamera3D { position: [0.0; 3], yaw: 15.0, pitch: -8.0, roll: 6.0, fov: CLASSIC_FOV }.view_rotation();
        let x_user = [120.0f32, -45.0, 900.0];
        // x_solved = R_trim * x_user
        let x_solved = [
            r_trim[0][0] * x_user[0] + r_trim[0][1] * x_user[1] + r_trim[0][2] * x_user[2],
            r_trim[1][0] * x_user[0] + r_trim[1][1] * x_user[1] + r_trim[1][2] * x_user[2],
            r_trim[2][0] * x_user[0] + r_trim[2][1] * x_user[1] + r_trim[2][2] * x_user[2],
        ];
        let sample = seq.camera_solves[0].frames[1];
        let untrimmed_cam =
            SequenceMatchMove { solve: solve_id, enabled: true, scale_px: 500.0, yaw_trim: 0.0, pitch_trim: 0.0, roll_trim: 0.0 }.apply_to_sample(&sample);
        let trimmed_cam = trimmed.apply_to_sample(&sample);
        let to_cam = |cam: SceneCamera3D, w: [f32; 3]| -> [f32; 3] {
            let vr = cam.view_rotation();
            let d = [w[0] - cam.position[0], w[1] - cam.position[1], w[2] - cam.position[2]];
            [
                vr[0][0] * d[0] + vr[0][1] * d[1] + vr[0][2] * d[2],
                vr[1][0] * d[0] + vr[1][1] * d[1] + vr[1][2] * d[2],
                vr[2][0] * d[0] + vr[2][1] * d[1] + vr[2][2] * d[2],
            ]
        };
        let p_untrimmed = to_cam(untrimmed_cam, x_solved);
        let p_trimmed = to_cam(trimmed_cam, x_user);
        let mapped_pt = trimmed.apply_to_point([x_solved[0] / 500.0, x_solved[1] / 500.0, x_solved[2] / 500.0]);
        for axis in 0..3 {
            assert!((p_untrimmed[axis] - p_trimmed[axis]).abs() < 1e-2, "axis {axis}: {p_untrimmed:?} vs {p_trimmed:?}");
            assert!((mapped_pt[axis] - x_user[axis]).abs() < 1e-2, "apply_to_point axis {axis}: {mapped_pt:?} vs {x_user:?}");
        }

        // Deleting the timeline clip leaves a harmless orphan solve anchored at Tick::ZERO (AC7),
        // and max_id still accounts for solve_id and cid.
        p.sequence_mut(sid).unwrap().video_tracks[0].items.clear();
        assert!(p.max_id() >= solve_id && p.max_id() >= cid.0);
        let orphan_cam = p.sequence(sid).unwrap().matchmove_camera_at(Tick(100)).unwrap();
        assert!((orphan_cam.position[0] - 500.0).abs() < 1e-4);
    }
}
