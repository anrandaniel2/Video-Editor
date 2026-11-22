//! Offline 3D match-move camera solver powered by stripped-down COLMAP / GLOMAP
//! (headless OpenGL + sparse SfM, zero UI).
//!
//! Input frames are sampled at a configurable rate (**10 fps** by default, `1..=60 fps`) so
//! COLMAP / GLOMAP completes quickly without having to process every video frame; intermediate
//! timeline frames are smoothly interpolated via 4-point Catmull-Rom + quaternion SQUAD splines and
//! routed directly to the sequence's 3D Scene Camera (`Sequence::scene_camera` +
//! `Sequence::scene_camera_tracks`).

use filmcraft_colmap::{ColmapError, GrayFrame, SolveConfig, solve_sequence};
use filmcraft_project::{
    CameraSolve, ClipId, SolvedFrame, SolvedPoint, StereoLayout,
    euler_from_view_rotation,
};
use filmcraft_time::Tick;

use crate::scene::{SceneDetector, SceneOptions};
use crate::track3d::view_rect;

/// Default frame rate fed into COLMAP / GLOMAP (frames per second).
pub const DEFAULT_SOLVER_FPS: f64 = 10.0;
/// Minimum configurable solver sampling rate (frames per second).
pub const MIN_SOLVER_FPS: f64 = 1.0;
/// Maximum configurable solver sampling rate (frames per second).
pub const MAX_SOLVER_FPS: f64 = 60.0;

const MAX_WORKING_SIDE: usize = 840;

/// One decoded RGBA8 video frame streamed into [`solve_camera`].
#[derive(Clone, Debug)]
pub struct InputFrame {
    pub time: Tick,
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

/// Configuration for a match-move solve run.
#[derive(Clone, Debug)]
pub struct SolveOptions {
    pub id: u64,
    pub clip: ClipId,
    pub name: String,
    /// Clip-relative playhead time used to anchor the reference camera pose and isolate cuts.
    pub playhead: Tick,
    /// Maximum clip duration to sample (`0.5..=60.0` seconds; default `15.0`).
    pub max_seconds: f64,
    /// Target frame rate to sample into COLMAP / GLOMAP (`1.0..=60.0` fps; default `10.0`).
    pub sample_fps: f64,
    /// Allow shared focal length refinement during COLMAP / GLOMAP bundle adjustment.
    pub refine_focal: bool,
    /// Stereoscopic frame packing layout.
    pub layout: StereoLayout,
    /// Maximum stereo disparity search range in working pixels.
    pub stereo_baseline: f32,
}

impl Default for SolveOptions {
    fn default() -> Self {
        Self {
            id: 1,
            clip: ClipId(1),
            name: "Camera Match".into(),
            playhead: Tick::ZERO,
            max_seconds: 15.0,
            sample_fps: DEFAULT_SOLVER_FPS,
            refine_focal: false,
            layout: StereoLayout::Mono,
            stereo_baseline: 48.0,
        }
    }
}

impl SolveOptions {
    /// Clamp `max_seconds` into the supported `0.5..=60.0` range (`15.0` if non-finite).
    pub fn clamped_max_seconds(&self) -> f64 {
        if self.max_seconds.is_finite() {
            self.max_seconds.clamp(0.5, 60.0)
        } else {
            15.0
        }
    }

    /// Clamp `sample_fps` into [`MIN_SOLVER_FPS`]..=[`MAX_SOLVER_FPS`] ([`DEFAULT_SOLVER_FPS`] if non-finite or `<= 0`).
    pub fn clamped_sample_fps(&self) -> f64 {
        if self.sample_fps.is_finite() && self.sample_fps > 0.0 {
            self.sample_fps.clamp(MIN_SOLVER_FPS, MAX_SOLVER_FPS)
        } else {
            DEFAULT_SOLVER_FPS
        }
    }
}

/// Completed camera solve together with quality diagnostics.
#[derive(Clone, Debug)]
pub struct SolveReport {
    pub solve: CameraSolve,
    pub median_rms_px: f32,
    pub median_confidence: f32,
    pub lost_frames: u32,
    pub shots_skipped: u32,
}

/// Failure modes of [`solve_camera`].
#[derive(Clone, Debug, PartialEq)]
pub enum SolveError {
    Cancelled,
    TooShort,
    InsufficientMotion,
    Planar2DVideo,
    PoorQuality { median_rms_px: f32, median_confidence: f32 },
    Decode(String),
}

impl std::fmt::Display for SolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => write!(f, "camera solve cancelled"),
            Self::TooShort => write!(f, "clip is too short to solve the camera (need at least 3 sampled frames)"),
            Self::InsufficientMotion => write!(f, "not enough clean motion to solve the camera"),
            Self::Planar2DVideo => {
                write!(f, "footage appears to be flat 2D motion without 3D perspective; use 2D motion tracking")
            }
            Self::PoorQuality { median_rms_px, median_confidence } => {
                write!(
                    f,
                    "not enough clean motion to solve the camera (RMS {median_rms_px:.2} px, confidence {median_confidence:.2})"
                )
            }
            Self::Decode(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for SolveError {}

struct SampledFrame {
    time: Tick,
    width: u32,
    height: u32,
    gray: Vec<u8>,
}

fn conformed_dims(w: usize, h: usize) -> (usize, usize) {
    let cw = w.max(1);
    let ch = h.max(1);
    let long = cw.max(ch);
    if long <= MAX_WORKING_SIDE {
        return (cw.max(16), ch.max(16));
    }
    let scale = MAX_WORKING_SIDE as f64 / long as f64;
    (
        ((cw as f64 * scale).round() as usize).max(16),
        ((ch as f64 * scale).round() as usize).max(16),
    )
}

fn rgba_crop_to_conformed_gray(
    rgba: &[u8],
    src_w: usize,
    src_h: usize,
    crop: (usize, usize, usize, usize),
    dst: (usize, usize),
) -> Vec<u8> {
    let (cx, cy, cw, ch) = crop;
    let (dw, dh) = dst;
    let mut out = vec![0u8; dw * dh];
    let sx_scale = cw.max(1) as f64 / dw.max(1) as f64;
    let sy_scale = ch.max(1) as f64 / dh.max(1) as f64;
    for y in 0..dh {
        let sy = (cy as f64 + (y as f64 + 0.5) * sy_scale - 0.5).clamp(0.0, (src_h.saturating_sub(1)) as f64);
        let y0 = sy.floor() as usize;
        let y1 = (y0 + 1).min(src_h.saturating_sub(1));
        let wy = (sy - y0 as f64) as f32;
        for x in 0..dw {
            let sx = (cx as f64 + (x as f64 + 0.5) * sx_scale - 0.5).clamp(0.0, (src_w.saturating_sub(1)) as f64);
            let x0 = sx.floor() as usize;
            let x1 = (x0 + 1).min(src_w.saturating_sub(1));
            let wx = (sx - x0 as f64) as f32;

            let luma = |px: usize, py: usize| -> f32 {
                let idx = (py * src_w + px) * 4;
                0.2126 * rgba[idx] as f32 + 0.7152 * rgba[idx + 1] as f32 + 0.0722 * rgba[idx + 2] as f32
            };
            let v0 = luma(x0, y0) * (1.0 - wx) + luma(x1, y0) * wx;
            let v1 = luma(x0, y1) * (1.0 - wx) + luma(x1, y1) * wx;
            out[y * dw + x] = (v0 * (1.0 - wy) + v1 * wy).round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

fn select_playhead_shot(times: &[Tick], cuts: &[usize], playhead: Tick) -> (std::ops::Range<usize>, u32) {
    if cuts.is_empty() || times.is_empty() {
        return (0..times.len(), 0);
    }
    let mut bounds = Vec::with_capacity(cuts.len() + 2);
    bounds.push(0usize);
    for &c in cuts {
        if c > *bounds.last().unwrap_or(&0) && c < times.len() {
            bounds.push(c);
        }
    }
    bounds.push(times.len());

    let mut chosen = 0usize;
    let mut best_dist = i64::MAX;
    for (si, win) in bounds.windows(2).enumerate() {
        let (s, e) = (win[0], win[1]);
        if e <= s {
            continue;
        }
        let t0 = times[s];
        let t1 = times[e - 1];
        let dist = if playhead < t0 {
            t0.0.saturating_sub(playhead.0)
        } else if playhead > t1 {
            playhead.0.saturating_sub(t1.0)
        } else {
            0
        };
        if dist < best_dist {
            best_dist = dist;
            chosen = si;
        }
    }
    let skipped = (bounds.len().saturating_sub(2)) as u32;
    (bounds[chosen]..bounds[chosen + 1], skipped)
}

fn mat3_mul(a: [[f64; 3]; 3], b: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let mut out = [[0.0; 3]; 3];
    for r in 0..3 {
        for c in 0..3 {
            out[r][c] = a[r][0] * b[0][c] + a[r][1] * b[1][c] + a[r][2] * b[2][c];
        }
    }
    out
}

fn mat3_transpose(m: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
    [
        [m[0][0], m[1][0], m[2][0]],
        [m[0][1], m[1][1], m[2][1]],
        [m[0][2], m[1][2], m[2][2]],
    ]
}

fn mat3_mul_vec(m: [[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

fn median_f64(vals: &mut [f64]) -> f64 {
    if vals.is_empty() {
        return 0.0;
    }
    vals.sort_by(f64::total_cmp);
    let mid = vals.len() / 2;
    if vals.len() % 2 == 1 {
        vals[mid]
    } else {
        0.5 * (vals[mid - 1] + vals[mid])
    }
}

/// Stream decoded frames into the stripped-down COLMAP / GLOMAP sparse solver at the configured
/// `sample_fps` rate (`10 fps` by default, clamped to `1..=60 fps`).
pub fn solve_camera<F>(
    opts: &SolveOptions,
    cancel: &dyn Fn() -> bool,
    mut next_frame: F,
) -> Result<SolveReport, SolveError>
where
    F: FnMut() -> Result<Option<InputFrame>, SolveError>,
{
    let min_step = Tick::from_seconds_f64(1.0 / opts.clamped_sample_fps());
    let max_span = Tick::from_seconds_f64(opts.clamped_max_seconds());

    let mut sampled: Vec<SampledFrame> = Vec::new();
    let mut pending_tail: Option<SampledFrame> = None;
    let mut detector = SceneDetector::new();
    let mut expected_dims: Option<(usize, usize)> = None;
    let mut first_time: Option<Tick> = None;

    while sampled.len() < CameraSolve::MAX_FRAMES {
        if cancel() {
            return Err(SolveError::Cancelled);
        }
        let Some(frame) = next_frame()? else { break };
        if frame.width == 0
            || frame.height == 0
            || frame.rgba.len() < frame.width.saturating_mul(frame.height).saturating_mul(4)
        {
            return Err(SolveError::Decode("Decoded frame has invalid dimensions.".into()));
        }
        match expected_dims {
            None => expected_dims = Some((frame.width, frame.height)),
            Some((ew, eh)) if ew != frame.width || eh != frame.height => {
                return Err(SolveError::Decode("Frame dimensions changed mid-clip.".into()));
            }
            _ => {}
        }

        let t0 = *first_time.get_or_insert(frame.time);
        if frame.time.0.saturating_sub(t0.0) > max_span.0 {
            break;
        }

        let crop = view_rect(opts.layout, frame.width, frame.height);
        let dst = conformed_dims(crop.2, crop.3);

        // Sample at the configured solver FPS into COLMAP / GLOMAP, keeping a trailing frame so the
        // end of the clip is anchored for smooth interpolation.
        let accept = match sampled.last() {
            None => true,
            Some(prev) => frame.time.0.saturating_sub(prev.time.0) >= min_step.0,
        };
        if accept {
            detector.push_rgba(&frame.rgba, frame.width, frame.height);
            let gray = rgba_crop_to_conformed_gray(&frame.rgba, frame.width, frame.height, crop, dst);
            sampled.push(SampledFrame {
                time: frame.time,
                width: dst.0 as u32,
                height: dst.1 as u32,
                gray,
            });
            pending_tail = None;
        } else {
            let gray = rgba_crop_to_conformed_gray(&frame.rgba, frame.width, frame.height, crop, dst);
            pending_tail = Some(SampledFrame {
                time: frame.time,
                width: dst.0 as u32,
                height: dst.1 as u32,
                gray,
            });
        }
    }

    if let Some(tail) = pending_tail
        && sampled.last().is_some_and(|last| tail.time > last.time)
    {
        if sampled.len() >= 2
            && sampled.last().is_some_and(|last| tail.time.0.saturating_sub(last.time.0) < min_step.0 / 3)
        {
            // Replace the penultimate close sample so cadence stays close to `opts.sample_fps`.
            if let Some(last) = sampled.last_mut() {
                *last = tail;
            }
        } else {
            sampled.push(tail);
        }
    }

    if sampled.len() < 3 {
        return Err(SolveError::TooShort);
    }

    let times: Vec<Tick> = sampled.iter().map(|s| s.time).collect();
    let cuts = detector.cuts(&SceneOptions { sensitivity: 45.0, min_shot_frames: 2 });
    let (shot_range, shots_skipped) = select_playhead_shot(&times, &cuts, opts.playhead);
    let shot_frames = &sampled[shot_range];
    if shot_frames.len() < 3 {
        return Err(SolveError::TooShort);
    }

    if cancel() {
        return Err(SolveError::Cancelled);
    }

    let work_w = shot_frames[0].width.max(1) as f64;
    let work_h = shot_frames[0].height.max(1) as f64;
    // Standard smartphone main-camera focal prior (`0.90 * max(w, h)` ~ 26mm eq, ~56 deg HFOV / ~34 deg VFOV in 16:9
    // and ~56 deg VFOV in 9:16 portrait), avoiding the 62% portrait focal underestimate of fixed `CLASSIC_FOV`.
    let prior_focal = 0.90 * work_w.max(work_h);

    let colmap_inputs: Vec<GrayFrame<'_>> = shot_frames
        .iter()
        .map(|s| GrayFrame {
            time_s: s.time.seconds(),
            width: s.width,
            height: s.height,
            gray: &s.gray,
        })
        .collect();

    let cfg = SolveConfig {
        initial_focal_px: prior_focal,
        lock_fov: !opts.refine_focal,
        max_features: 2048,
        use_opengl: true,
        prefer_glomap: true,
    };

    let recon = solve_sequence(&colmap_inputs, &cfg).map_err(|err| match err {
        ColmapError::Planar2D(_) => SolveError::Planar2DVideo,
        ColmapError::InvalidInput(_) | ColmapError::InsufficientMotion(_) | ColmapError::Internal(_) => {
            SolveError::InsufficientMotion
        }
    })?;
    if cancel() {
        return Err(SolveError::Cancelled);
    }

    let reg_indices: Vec<usize> = recon
        .poses
        .iter()
        .enumerate()
        .filter_map(|(i, p)| p.registered.then_some(i))
        .collect();
    if reg_indices.len() < 2 {
        return Err(SolveError::InsufficientMotion);
    }

    // Anchor normalization to the registered pose closest to `opts.playhead` so 3D objects placed
    // at the playhead remain stationary at their exact screen coordinates while the camera moves.
    let ref_idx = *reg_indices
        .iter()
        .min_by_key(|&&i| shot_frames[i].time.0.saturating_sub(opts.playhead.0).abs())
        .unwrap_or(&reg_indices[0]);
    let ref_pose = &recon.poses[ref_idx];
    let r_ref = ref_pose.r_cw;
    let r_ref_t = mat3_transpose(r_ref);
    let c_ref = {
        let rt_t = mat3_mul_vec(r_ref_t, ref_pose.t_cw);
        [-rt_t[0], -rt_t[1], -rt_t[2]]
    };

    // Transform sparse 3D landmarks into `ref_idx`'s camera frame (`X_ref = R_ref * X + t_ref`).
    let mut ref_points: Vec<([f64; 3], u32, f64)> = Vec::with_capacity(recon.points.len());
    let mut pos_depths: Vec<f64> = Vec::new();
    for pt in &recon.points {
        if !pt.xyz.iter().all(|v| v.is_finite()) {
            continue;
        }
        let xr = mat3_mul_vec(r_ref, [pt.xyz[0] - c_ref[0], pt.xyz[1] - c_ref[1], pt.xyz[2] - c_ref[2]]);
        if xr.iter().all(|v| v.is_finite()) {
            if xr[2] > 1e-3 && pt.observations >= 2 {
                pos_depths.push(xr[2]);
            }
            ref_points.push((xr, pt.observations, pt.reproj_error));
        }
    }

    // Compute camera eyes in `ref_idx`'s camera frame before normalizing scale.
    let mut raw_eyes = vec![[0.0f64; 3]; shot_frames.len()];
    let mut max_baseline = 0.0f64;
    for &i in &reg_indices {
        let p = &recon.poses[i];
        let p_rt = mat3_transpose(p.r_cw);
        let c_i = {
            let v = mat3_mul_vec(p_rt, p.t_cw);
            [-v[0], -v[1], -v[2]]
        };
        let rel = mat3_mul_vec(r_ref, [c_i[0] - c_ref[0], c_i[1] - c_ref[1], c_i[2] - c_ref[2]]);
        let dist = (rel[0] * rel[0] + rel[1] * rel[1] + rel[2] * rel[2]).sqrt();
        if dist.is_finite() && dist > max_baseline {
            max_baseline = dist;
        }
        raw_eyes[i] = rel;
    }

    let min_3d_landmarks = if shot_frames.len() >= 6 { 8usize } else { 4usize };
    let rotation_only = pos_depths.len() < min_3d_landmarks || max_baseline <= 1e-5;
    let depth_scale = if !rotation_only && !pos_depths.is_empty() {
        median_f64(&mut pos_depths).max(1e-4)
    } else if !rotation_only && max_baseline > 1e-5 {
        (max_baseline * 4.0).max(1e-4)
    } else {
        1.0
    };

    let solved_focal = if recon.focal_px.is_finite() && recon.focal_px > 10.0 {
        recon.focal_px
    } else {
        prior_focal
    };
    let fov_deg = (2.0 * ((work_h * 0.5) / solved_focal).atan().to_degrees()).clamp(10.0, 140.0) as f32;
    let mean_rms = if recon.mean_reproj_px.is_finite() && recon.mean_reproj_px >= 0.0 {
        recon.mean_reproj_px as f32
    } else {
        0.5
    };
    let reg_conf = (1.0 / (1.0 + (f64::from(mean_rms) / 2.0).powi(2))).clamp(0.35, 1.0) as f32;

    // Build SolvedFrame entries at every sampled frame, then cubic-interpolate any unregistered gap.
    let mut frames_opt: Vec<Option<SolvedFrame>> = vec![None; shot_frames.len()];
    for &i in &reg_indices {
        let p = &recon.poses[i];
        let v_norm = mat3_mul(p.r_cw, r_ref_t);
        let vr_f32 = [
            [v_norm[0][0] as f32, v_norm[0][1] as f32, v_norm[0][2] as f32],
            [v_norm[1][0] as f32, v_norm[1][1] as f32, v_norm[1][2] as f32],
            [v_norm[2][0] as f32, v_norm[2][1] as f32, v_norm[2][2] as f32],
        ];
        let (pitch, yaw, roll) = euler_from_view_rotation(vr_f32);
        let pos = if rotation_only {
            [0.0; 3]
        } else {
            [
                (raw_eyes[i][0] / depth_scale) as f32,
                (raw_eyes[i][1] / depth_scale) as f32,
                (raw_eyes[i][2] / depth_scale) as f32,
            ]
        };
        let sf = SolvedFrame {
            time: shot_frames[i].time,
            position: pos,
            yaw,
            pitch,
            roll,
            fov: fov_deg,
            rms_px: mean_rms,
            confidence: reg_conf,
        };
        if sf.is_finite() {
            frames_opt[i] = Some(sf);
        }
    }

    let valid_regs: Vec<(usize, SolvedFrame)> = frames_opt
        .iter()
        .enumerate()
        .filter_map(|(i, sf)| sf.map(|f| (i, f)))
        .collect();
    if valid_regs.len() < 2 {
        return Err(SolveError::InsufficientMotion);
    }

    let mut solved_frames = Vec::with_capacity(shot_frames.len());
    let mut lost_frames = 0u32;
    for (i, slot) in frames_opt.iter().enumerate() {
        if let Some(sf) = slot {
            solved_frames.push(*sf);
            continue;
        }
        // Smoothly interpolate dropped/unregistered frame from surrounding registered COLMAP keyframes.
        let t = shot_frames[i].time;
        let pos_in_reg = valid_regs.partition_point(|(_, rf)| rf.time <= t);
        let interp_sf = if pos_in_reg == 0 {
            let mut held = valid_regs[0].1;
            held.time = t;
            held.confidence = (reg_conf * 0.7).max(CameraSolve::LOST_CONFIDENCE);
            held
        } else if pos_in_reg >= valid_regs.len() {
            let mut held = valid_regs[valid_regs.len() - 1].1;
            held.time = t;
            held.confidence = (reg_conf * 0.7).max(CameraSolve::LOST_CONFIDENCE);
            held
        } else {
            let k1 = pos_in_reg - 1;
            let k2 = pos_in_reg;
            let p1 = valid_regs[k1].1;
            let p2 = valid_regs[k2].1;
            let p0 = if k1 > 0 { valid_regs[k1 - 1].1 } else { p1 };
            let p3 = valid_regs.get(k2 + 1).map(|x| x.1).unwrap_or(p2);
            let span = p2.time.0.saturating_sub(p1.time.0).max(1) as f32;
            let alpha = ((t.0.saturating_sub(p1.time.0)) as f32 / span).clamp(0.0, 1.0);
            let mut cr = SolvedFrame::catmull_rom(p0, p1, p2, p3, alpha);
            cr.time = t;
            cr.confidence = (reg_conf * 0.85).max(CameraSolve::LOST_CONFIDENCE);
            cr
        };
        if !interp_sf.is_confident() {
            lost_frames += 1;
        }
        solved_frames.push(interp_sf);
    }

    // Light zero-phase Savitzky-Golay / geodesic smoothing across interior frames while keeping
    // the playhead anchor frame (`ref_idx`) and clip endpoints exact so 3D object locking is drift-free.
    if solved_frames.len() >= 5 {
        let raw_sf = solved_frames.clone();
        for i in 1..(raw_sf.len() - 1) {
            if i == ref_idx {
                continue;
            }
            let mid_neighbors = SolvedFrame::lerp(raw_sf[i - 1], raw_sf[i + 1], 0.5);
            let mut sm = SolvedFrame::lerp(raw_sf[i], mid_neighbors, 0.30);
            sm.time = raw_sf[i].time;
            sm.fov = raw_sf[i].fov;
            sm.rms_px = raw_sf[i].rms_px;
            sm.confidence = raw_sf[i].confidence;
            if rotation_only {
                sm.position = [0.0; 3];
            }
            if sm.is_finite() {
                solved_frames[i] = sm;
            }
        }
    }

    // Rank and normalize up to MAX_POINTS sparse 3D landmarks for ground-plane leveling & snapping.
    let mut scored_pts: Vec<(f64, SolvedPoint)> = Vec::new();
    if !rotation_only {
        for (xr, obs, err) in ref_points {
            let nx = (xr[0] / depth_scale) as f32;
            let ny = (xr[1] / depth_scale) as f32;
            let nz = (xr[2] / depth_scale) as f32;
            if !nx.is_finite() || !ny.is_finite() || !nz.is_finite() || nz <= 0.02 || nz > 50.0 {
                continue;
            }
            let rms_p = if err.is_finite() && err >= 0.0 { err as f32 } else { mean_rms };
            let score = f64::from(obs.max(1)) / (1.0 + f64::from(rms_p).powi(2));
            let sp = SolvedPoint {
                position: [nx, ny, nz],
                observations: obs.max(2),
                rms_px: rms_p,
            };
            if sp.is_finite() && score.is_finite() {
                scored_pts.push((score, sp));
            }
        }
        scored_pts.sort_by(|a, b| b.0.total_cmp(&a.0));
        scored_pts.truncate(CameraSolve::MAX_POINTS);
    }
    let points: Vec<SolvedPoint> = scored_pts.into_iter().map(|(_, p)| p).collect();

    let solve = CameraSolve {
        id: opts.id,
        clip: opts.clip,
        name: if opts.name.trim().is_empty() {
            if recon.used_glomap { "GLOMAP Solve".into() } else { "COLMAP Solve".into() }
        } else {
            opts.name.clone()
        },
        layout: opts.layout,
        rotation_only,
        shots_skipped,
        frames: solved_frames,
        points,
    };
    if !solve.is_finite() {
        return Err(SolveError::InsufficientMotion);
    }
    let median_rms_px = solve.median_rms();
    let median_confidence = solve.median_confidence();
    Ok(SolveReport {
        solve,
        median_rms_px,
        median_confidence,
        lost_frames,
        shots_skipped,
    })
}

/// Convenience wrapper over [`solve_camera`] for an in-memory slice of frames.
pub fn solve_frames(
    opts: &SolveOptions,
    frames: &[InputFrame],
    cancel: &dyn Fn() -> bool,
) -> Result<SolveReport, SolveError> {
    let mut iter = frames.iter();
    solve_camera(opts, cancel, || Ok(iter.next().cloned()))
}
