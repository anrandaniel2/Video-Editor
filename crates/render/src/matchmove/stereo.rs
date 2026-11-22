//! Stage 7: optional stereoscopic disparity scale prior for Side-by-Side and Top/Bottom footage,
//! reusing `track3d::disparity`.

use super::init::SceneModel;
use super::linalg::{mat3_mul_vec, median_f64, scale3};
use crate::track3d::{Gray, disparity};

const DISPARITY_PATCH_RADIUS: i32 = 5;
const MIN_DISPARITY_PX: f64 = 0.6;

#[derive(Clone, Copy, Debug)]
pub(crate) struct StereoObs {
    pub(crate) track_id: usize,
    pub(crate) frame: usize,
    pub(crate) disparity_px: f64,
}

/// Measure sub-pixel horizontal disparity (in working pixels) on active features of a stereo pair.
pub(crate) fn sample_stereo_disparities(
    left: &Gray,
    right: &Gray,
    frame_idx: usize,
    features: &[(usize, (f32, f32))],
    max_disp: f32,
    out: &mut Vec<(usize, usize, f32)>,
) {
    let md = if max_disp.is_finite() { max_disp.clamp(4.0, 192.0) } else { 48.0 };
    for &(tid, pt) in features {
        let norm_d = disparity(left, right, pt, DISPARITY_PATCH_RADIUS, md);
        let d_px = norm_d * md;
        if d_px.is_finite() && f64::from(d_px) >= MIN_DISPARITY_PX && d_px < md * 0.98 {
            out.push((tid, frame_idx, d_px));
        }
    }
}

/// Rescale a translational solve's world coordinates using median stereo depth (`Z = f · B / d`)
/// when at least 6 valid stereo disparity observations agree.
pub(crate) fn apply_stereo_scale_prior(model: &mut SceneModel, stereo_obs: &[StereoObs], view_width: usize, baseline_px: f32) {
    if model.rotation_only || stereo_obs.is_empty() || view_width == 0 {
        return;
    }
    let baseline_norm = (f64::from(if baseline_px.is_finite() { baseline_px } else { 48.0 }) / view_width as f64).clamp(0.02, 0.5);
    let mut ratios: Vec<f64> = Vec::new();
    for obs in stereo_obs {
        let Some(Some(xw)) = model.points.get(obs.track_id).copied() else { continue };
        let Some(cam) = model.cameras.get(obs.frame) else { continue };
        if cam.frozen || obs.disparity_px < MIN_DISPARITY_PX {
            continue;
        }
        let pc = mat3_mul_vec(cam.r_wc, xw);
        let z_solve = pc[2] + cam.t_wc[2];
        if !z_solve.is_finite() || z_solve <= 1e-3 {
            continue;
        }
        let z_stereo = (model.focal * baseline_norm) / obs.disparity_px;
        let ratio = z_stereo / z_solve;
        if ratio.is_finite() && (0.05..=20.0).contains(&ratio) {
            ratios.push(ratio);
        }
    }
    if ratios.len() < 6 {
        return;
    }
    let scale = median_f64(&mut ratios).clamp(0.1, 10.0);
    if !scale.is_finite() || scale <= 0.0 {
        return;
    }
    for cam in &mut model.cameras {
        cam.t_wc = scale3(cam.t_wc, scale);
    }
    for pt in model.points.iter_mut().flatten() {
        *pt = scale3(*pt, scale);
    }
}
