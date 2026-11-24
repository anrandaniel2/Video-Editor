//! Stage 7: optional stereoscopic disparity scale prior for Side-by-Side and Top/Bottom footage,
//! reusing `track3d::disparity`.

use super::init::SceneModel;
use super::linalg::{mat3_mul_vec, median_f64, scale3};
use crate::track3d::{Gray, disparity};

const DISPARITY_PATCH_RADIUS: i32 = 5;
const MIN_DISPARITY_PX: f64 = 0.6;
const MIN_STEREO_ZNCC: f32 = 0.72;

#[derive(Clone, Copy, Debug)]
pub(crate) struct StereoObs {
    pub(crate) track_id: usize,
    pub(crate) frame: usize,
    pub(crate) disparity_px: f64,
}

fn zncc_patch(left: &Gray, right: &Gray, lx: f32, ly: f32, rx: f32, ry: f32, r: i32) -> Option<f32> {
    let rf = r as f32;
    if lx < rf
        || ly < rf
        || rx < rf
        || ry < rf
        || lx + rf >= left.w as f32
        || ly + rf >= left.h as f32
        || rx + rf >= right.w as f32
        || ry + rf >= right.h as f32
    {
        return None;
    }
    let mut sum_l = 0.0f32;
    let mut sum_r = 0.0f32;
    let mut n = 0usize;
    for oy in -r..=r {
        for ox in -r..=r {
            let lv = left.at(lx + ox as f32, ly + oy as f32)?;
            let rv = right.at(rx + ox as f32, ry + oy as f32)?;
            sum_l += lv;
            sum_r += rv;
            n += 1;
        }
    }
    if n == 0 {
        return None;
    }
    let mean_l = sum_l / n as f32;
    let mean_r = sum_r / n as f32;
    let (mut dot, mut nl, mut nr) = (0.0f32, 0.0f32, 0.0f32);
    for oy in -r..=r {
        for ox in -r..=r {
            let dl = left.at(lx + ox as f32, ly + oy as f32)? - mean_l;
            let dr = right.at(rx + ox as f32, ry + oy as f32)? - mean_r;
            dot += dl * dr;
            nl += dl * dl;
            nr += dr * dr;
        }
    }
    if nl < 1e-4 || nr < 1e-4 {
        return None;
    }
    Some(dot / (nl.sqrt() * nr.sqrt()))
}

/// Measure sub-pixel horizontal disparity (in working pixels) on active features of a stereo pair,
/// gated by zero-mean normalized cross-correlation and vertical epipolar consistency so non-stereo
/// footage scanned with Side-by-Side / Top-Bottom selected cannot inject bogus depth scale.
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
        if !d_px.is_finite() || f64::from(d_px) < MIN_DISPARITY_PX || d_px >= md * 0.98 {
            continue;
        }
        let Some(zncc) = zncc_patch(left, right, pt.0, pt.1, pt.0 - d_px, pt.1, DISPARITY_PATCH_RADIUS) else {
            continue;
        };
        if zncc < MIN_STEREO_ZNCC {
            continue;
        }
        // Epipolar sanity check: on a rectified horizontal stereo pair, shifting vertically by 3 px
        // should degrade correlation compared to the epipolar scanline.
        let zncc_up = zncc_patch(left, right, pt.0, pt.1, pt.0 - d_px, pt.1 - 3.0, DISPARITY_PATCH_RADIUS).unwrap_or(-1.0);
        let zncc_dn = zncc_patch(left, right, pt.0, pt.1, pt.0 - d_px, pt.1 + 3.0, DISPARITY_PATCH_RADIUS).unwrap_or(-1.0);
        if zncc + 0.02 < zncc_up.max(zncc_dn) {
            continue;
        }
        out.push((tid, frame_idx, d_px));
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
    // Require genuine stereoscopic correspondence: if median stereo/solve depth ratio spread
    // (interquartile MAD) is huge, the right/bottom half is not a calibrated stereo eye (e.g. a
    // user picked SideBySide/TopBottom on ordinary mono footage), so skip rescaling.
    ratios.sort_by(f64::total_cmp);
    let mid = ratios.len() / 2;
    let scale = if ratios.len() % 2 == 1 { ratios[mid] } else { 0.5 * (ratios[mid - 1] + ratios[mid]) };
    let mut rel_devs: Vec<f64> = ratios.iter().map(|&r| (r - scale).abs() / scale.max(1e-6)).collect();
    let rel_mad = median_f64(&mut rel_devs);
    if !scale.is_finite() || scale <= 0.0 || rel_mad > 0.55 {
        return;
    }
    let scale = scale.clamp(0.1, 10.0);
    for cam in &mut model.cameras {
        cam.t_wc = scale3(cam.t_wc, scale);
    }
    for pt in model.points.iter_mut().flatten() {
        *pt = scale3(*pt, scale);
    }
}
