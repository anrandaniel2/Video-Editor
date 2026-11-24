//! Offline 3D match-move camera solver (`Camera Match`).
//!
//! Pipeline stages:
//! - **Stages 0–3 (`tracks`)**: working conform (`<= 480` px long side), streaming `SceneDetector` shot
//!   isolation around `playhead`, 4×3 grid Shi–Tomasi corner seeding, and pyramidal Lucas–Kanade tracking
//!   reusing `crate::track3d`.
//! - **Stage 4 (`init`)**: Hartley-normalised 4-pt RANSAC homography, widest-baseline 8-pt RANSAC
//!   essential matrix + 4-way cheirality, pure-rotation fallback (`H = K · R · K⁻¹`, `t = 0`), and
//!   incremental PnP + multi-view triangulation pose chaining in the 7-DOF Frame-0 gauge.
//! - **Stage 5 (`ba`)**: analytic-Jacobian Levenberg–Marquardt bundle adjustment with Huber loss,
//!   `3×3` point Schur complement, optional focal refinement (`±25%`), and `< 12`-inlier frame freezing.
//! - **Stages 6–8 (`mod`, `stereo`)**: optional SBS/TB disparity scale prior, quality gates, top-256
//!   3D landmark selection, and conversion into [`CameraSolve`].

pub(crate) mod ba;
pub(crate) mod init;
pub(crate) mod linalg;
pub(crate) mod stereo;
pub(crate) mod tracks;

#[cfg(test)]
mod tests;

use filmcraft_project::{CameraSolve, ClipId, SolvedFrame, SolvedPoint, StereoLayout, euler_from_view_rotation};
use filmcraft_time::Tick;

use self::ba::run_bundle_adjustment;
use self::init::{SceneModel, initialize_scene};
use self::linalg::{mat3_t_mul_vec, median_f64, scale3};
use self::stereo::apply_stereo_scale_prior;
use self::tracks::{FeatureTrack, TrackedShot, stream_and_track};

const MAX_MEDIAN_RMS_PX: f64 = 4.8;
const MIN_MEDIAN_CONFIDENCE: f64 = 0.14;

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
    /// Clip-relative playhead time used to select which shot to solve when cuts are detected.
    pub playhead: Tick,
    /// Maximum clip duration to sample (`0.5..=60.0` seconds; default `8.0`).
    pub max_seconds: f64,
    /// Allow shared focal length refinement (`±25%` around `CLASSIC_FOV`) during bundle adjustment.
    pub refine_focal: bool,
    /// Stereoscopic frame packing layout for optional disparity depth-scale prior.
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
            max_seconds: 8.0,
            refine_focal: false,
            layout: StereoLayout::Mono,
            stereo_baseline: 48.0,
        }
    }
}

impl SolveOptions {
    /// Clamp `max_seconds` into the supported `0.5..=60.0` range (`8.0` if non-finite).
    pub fn clamped_max_seconds(&self) -> f64 {
        if self.max_seconds.is_finite() { self.max_seconds.clamp(0.5, 60.0) } else { 8.0 }
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
    /// Footage exhibits flat 2D image-plane motion (2D pan/zoom/spin or static plate) without 3D
    /// perspective foreshortening or multi-depth parallax; callers can switch to 2D motion tracking.
    Planar2DVideo,
    PoorQuality { median_rms_px: f32, median_confidence: f32 },
    Decode(String),
}

impl std::fmt::Display for SolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => write!(f, "camera solve cancelled"),
            Self::TooShort => write!(f, "clip is too short to solve the camera (need at least 8 frames)"),
            Self::InsufficientMotion => write!(f, "not enough clean motion to solve the camera"),
            Self::Planar2DVideo => write!(f, "footage appears to be flat 2D motion without 3D perspective; use 2D motion tracking"),
            Self::PoorQuality { median_rms_px, median_confidence } => {
                write!(f, "not enough clean motion to solve the camera (RMS {median_rms_px:.2} px, confidence {median_confidence:.2})")
            }
            Self::Decode(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for SolveError {}

/// Stream decoded frames through the full 3D match-move pipeline, keeping at most two grayscale
/// pyramids in memory at once.
pub fn solve_camera<F>(opts: &SolveOptions, cancel: &dyn Fn() -> bool, next_frame: F) -> Result<SolveReport, SolveError>
where
    F: FnMut() -> Result<Option<InputFrame>, SolveError>,
{
    let shot = stream_and_track(opts, cancel, next_frame)?;
    if cancel() {
        return Err(SolveError::Cancelled);
    }
    let mut model = initialize_scene(&shot, opts.refine_focal, cancel)?;
    // Refine shared focal length whenever non-default lens seed was chosen (e.g. portrait 9:16
    // smartphone wide-angle) or when explicitly requested via `opts.refine_focal`.
    let nominal_focal = (shot.height as f64 * 0.5) / (f64::from(filmcraft_project::CLASSIC_FOV).to_radians() * 0.5).tan();
    let refine_f = opts.refine_focal || (model.focal - nominal_focal).abs() > nominal_focal * 0.05;
    run_bundle_adjustment(&mut model, &shot.tracks, refine_f, cancel)?;
    if opts.layout != StereoLayout::Mono {
        apply_stereo_scale_prior(&mut model, &shot.stereo_obs, shot.width, opts.stereo_baseline);
    }
    assemble_report(opts, &shot, &model)
}

/// Convenience wrapper over [`solve_camera`] for an in-memory slice of frames.
pub fn solve_frames(opts: &SolveOptions, frames: &[InputFrame], cancel: &dyn Fn() -> bool) -> Result<SolveReport, SolveError> {
    let mut iter = frames.iter();
    solve_camera(opts, cancel, || Ok(iter.next().cloned()))
}

fn assemble_report(opts: &SolveOptions, shot: &TrackedShot, model: &SceneModel) -> Result<SolveReport, SolveError> {
    let fov_deg = (2.0 * (shot.height as f64 / (2.0 * model.focal)).atan().to_degrees()).clamp(5.0, 150.0) as f32;
    if !fov_deg.is_finite() {
        return Err(SolveError::InsufficientMotion);
    }

    let mut solved_frames = Vec::with_capacity(shot.times.len().min(CameraSolve::MAX_FRAMES));
    let mut usable_rms = Vec::new();
    let mut all_conf = Vec::new();
    let mut usable_count = 0usize;
    let mut lost_frames = 0u32;

    for (f, &time) in shot.times.iter().take(CameraSolve::MAX_FRAMES).enumerate() {
        let cam = &model.cameras[f];
        let (rms_f, conf_f) = frame_quality(model, &shot.tracks, f);
        let confidence = if cam.frozen { 0.1f32 } else { conf_f };
        if confidence < CameraSolve::LOST_CONFIDENCE {
            lost_frames += 1;
        } else {
            usable_count += 1;
            usable_rms.push(rms_f);
        }
        all_conf.push(f64::from(confidence));

        let eye_f64 = if model.rotation_only { [0.0; 3] } else { scale3(mat3_t_mul_vec(cam.r_wc, cam.t_wc), -1.0) };
        let r_f32 = cam.r_wc.map(|row| row.map(|v| v as f32));
        let (pitch, yaw, roll) = euler_from_view_rotation(r_f32);
        let sf = SolvedFrame {
            time,
            position: [eye_f64[0] as f32, eye_f64[1] as f32, eye_f64[2] as f32],
            yaw,
            pitch,
            roll,
            fov: fov_deg,
            rms_px: rms_f as f32,
            confidence,
        };
        if !sf.is_finite() {
            return Err(SolveError::InsufficientMotion);
        }
        solved_frames.push(sf);
    }

    if usable_count < 8 {
        return Err(SolveError::InsufficientMotion);
    }
    let median_rms = median_f64(&mut usable_rms);
    let median_conf = median_f64(&mut all_conf);
    if !median_rms.is_finite() || median_rms > MAX_MEDIAN_RMS_PX || median_conf < MIN_MEDIAN_CONFIDENCE {
        return Err(SolveError::PoorQuality { median_rms_px: median_rms as f32, median_confidence: median_conf as f32 });
    }

    let points = select_solved_points(model, &shot.tracks);
    smooth_solved_frames(&mut solved_frames);
    let solve = CameraSolve {
        id: opts.id,
        clip: opts.clip,
        name: if opts.name.trim().is_empty() { "Camera Match".into() } else { opts.name.clone() },
        layout: opts.layout,
        rotation_only: model.rotation_only,
        shots_skipped: shot.shots_skipped,
        frames: solved_frames,
        points,
    };
    if !solve.is_finite() {
        return Err(SolveError::InsufficientMotion);
    }
    Ok(SolveReport { solve, median_rms_px: median_rms as f32, median_confidence: median_conf as f32, lost_frames, shots_skipped: shot.shots_skipped })
}

fn frame_quality(model: &SceneModel, tracks: &[FeatureTrack], frame: usize) -> (f64, f32) {
    let cam = &model.cameras[frame];
    if cam.frozen {
        return (10.0, 0.1);
    }
    let mut sq_sum = 0.0f64;
    let mut rms_cnt = 0usize;
    let mut inliers = 0usize;

    for tr in tracks {
        let (Some(xw), Some(obs)) = (model.points[tr.id], tr.at_frame(frame)) else { continue };
        let Some((u, v)) = model.project_world(cam, xw) else { continue };
        let e = (u - obs.0).hypot(v - obs.1);
        if e <= 3.5 {
            inliers += 1;
        }
        if e <= 4.8 {
            sq_sum += e * e;
            rms_cnt += 1;
        }
    }
    if rms_cnt == 0 || inliers == 0 {
        return (10.0, 0.1);
    }
    let rms = (sq_sum / rms_cnt as f64).sqrt();
    let cov = (inliers as f64 / 30.0).clamp(0.0, 1.0).sqrt();
    let acc = 1.0 / (1.0 + (rms / 2.0).powi(2));
    let conf = (cov * acc).clamp(0.05, 1.0) as f32;
    (rms, conf)
}

fn select_solved_points(model: &SceneModel, tracks: &[FeatureTrack]) -> Vec<SolvedPoint> {
    let mut scored: Vec<(f64, usize, SolvedPoint)> = Vec::new();
    for tr in tracks {
        let Some(xw) = model.points[tr.id] else { continue };
        if !xw.iter().all(|v| v.is_finite()) {
            continue;
        }
        let mut sq = 0.0f64;
        let mut inliers = 0u32;
        for (off, &obs) in tr.pts.iter().enumerate() {
            let f = tr.start_frame + off;
            if model.cameras[f].frozen {
                continue;
            }
            if let Some((u, v)) = model.project_world(&model.cameras[f], xw) {
                let e = (u - obs.0).hypot(v - obs.1);
                if e <= 3.5 {
                    sq += e * e;
                    inliers += 1;
                }
            }
        }
        if inliers < 3 {
            continue;
        }
        let rms = (sq / f64::from(inliers)).sqrt();
        let rank_score = (f64::from(inliers) / 24.0).clamp(0.1, 1.0) / (1.0 + (rms / 1.5).powi(2));
        let pt = SolvedPoint { position: [xw[0] as f32, xw[1] as f32, xw[2] as f32], observations: inliers, rms_px: rms as f32 };
        if pt.is_finite() && rank_score.is_finite() {
            scored.push((rank_score, tr.id, pt));
        }
    }
    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    scored.truncate(CameraSolve::MAX_POINTS);
    scored.into_iter().map(|(_, _, pt)| pt).collect()
}

/// Zero-phase, confidence-weighted temporal smoothing on solved 3D camera poses: removes
/// high-frequency frame-to-frame micro-shake while keeping Frame 0 exact and preserving crisp
/// camera motion using local linear velocity prediction.
fn smooth_solved_frames(frames: &mut [SolvedFrame]) {
    let n = frames.len();
    if n < 5 {
        return;
    }
    let orig = frames.to_vec();
    let unwrap_deg = |base: f32, v: f32| -> f32 {
        let mut d = v - base;
        while d > 180.0 {
            d -= 360.0;
        }
        while d < -180.0 {
            d += 360.0;
        }
        base + d
    };
    for (i, out) in frames.iter_mut().enumerate() {
        if i == 0 || !orig[i].is_confident() {
            continue;
        }
        let j0 = i.saturating_sub(2);
        let j1 = (i + 2).min(n - 1);
        let mut w_sum = 0.0f32;
        let mut pos = [0.0f32; 3];
        let mut ang = [0.0f32; 3]; // yaw, pitch, roll unwrapped around orig[i]
        for (j, fj) in orig.iter().enumerate().take(j1 + 1).skip(j0) {
            if !fj.is_confident() && j != i {
                continue;
            }
            let dt = (j as f32 - i as f32).abs();
            let wt = (-0.5 * (dt / 1.15f32).powi(2)).exp() * fj.confidence.max(0.2);
            let step = (i as isize - j as isize) as f32;
            let has_neighbors = j > 0 && j + 1 < n && orig[j - 1].is_confident() && orig[j + 1].is_confident();
            for axis in 0..3 {
                let vel = if has_neighbors { 0.5 * (orig[j + 1].position[axis] - orig[j - 1].position[axis]) } else { 0.0 };
                pos[axis] += wt * (fj.position[axis] + vel * step);
            }
            let yj = unwrap_deg(orig[i].yaw, fj.yaw);
            let pj = unwrap_deg(orig[i].pitch, fj.pitch);
            let rj = unwrap_deg(orig[i].roll, fj.roll);
            let (vy, vp, vr) = if has_neighbors {
                (
                    0.5 * (unwrap_deg(yj, orig[j + 1].yaw) - unwrap_deg(yj, orig[j - 1].yaw)),
                    0.5 * (unwrap_deg(pj, orig[j + 1].pitch) - unwrap_deg(pj, orig[j - 1].pitch)),
                    0.5 * (unwrap_deg(rj, orig[j + 1].roll) - unwrap_deg(rj, orig[j - 1].roll)),
                )
            } else {
                (0.0, 0.0, 0.0)
            };
            ang[0] += wt * (yj + vy * step);
            ang[1] += wt * (pj + vp * step);
            ang[2] += wt * (rj + vr * step);
            w_sum += wt;
        }
        if w_sum > 1e-6 {
            out.position = [pos[0] / w_sum, pos[1] / w_sum, pos[2] / w_sum];
            out.yaw = ang[0] / w_sum;
            out.pitch = (ang[1] / w_sum).clamp(-89.9, 89.9);
            out.roll = ang[2] / w_sum;
        }
    }
}

