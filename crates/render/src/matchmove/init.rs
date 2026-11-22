//! Stage 4: two-view geometric initialization (Hartley-normalised 4-pt RANSAC homography vs.
//! widest-baseline 8-pt RANSAC essential matrix + 4-way cheirality), pure-rotation fallback
//! (`H = K · R · K⁻¹`, `t ≡ 0`), incremental PnP + multi-view triangulation pose chaining, and
//! 7-DOF Frame-0 gauge normalisation.

use filmcraft_project::CLASSIC_FOV;

use super::SolveError;
use super::linalg::{
    IDENT3, Mat3, Rng, Vec3, add3, dot3, hartley_normalize, mat3_det, mat3_inv, mat3_mul, mat3_mul_vec, mat3_t_mul_vec, mat3_transpose, median_f64, norm3,
    normalize3, project_to_so3, rodrigues_exp, scale3, smallest_eigvec, solve_linear, sub3, svd3,
};
use super::tracks::{FeatureTrack, TrackedShot};

const RANSAC_ITERS: usize = 2000;
const HOMOGRAPHY_THRESH_PX: f64 = 3.2;
const SAMPSON_THRESH_PX: f64 = 3.0;
const PNP_INLIER_PX: f64 = 4.5;

#[derive(Clone, Copy, Debug)]
pub(crate) struct CameraPose {
    pub(crate) r_wc: Mat3,
    pub(crate) t_wc: Vec3,
    pub(crate) frozen: bool,
    pub(crate) solved: bool,
}

impl Default for CameraPose {
    fn default() -> Self {
        Self { r_wc: IDENT3, t_wc: [0.0; 3], frozen: false, solved: false }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SceneModel {
    pub(crate) cx: f64,
    pub(crate) cy: f64,
    pub(crate) focal: f64,
    pub(crate) initial_focal: f64,
    pub(crate) rotation_only: bool,
    pub(crate) ref_cam: usize,
    pub(crate) cameras: Vec<CameraPose>,
    pub(crate) points: Vec<Option<Vec3>>,
}

impl SceneModel {
    pub(crate) fn project_world(&self, cam: &CameraPose, xw: Vec3) -> Option<(f64, f64)> {
        let pc = if self.rotation_only { mat3_mul_vec(cam.r_wc, xw) } else { add3(mat3_mul_vec(cam.r_wc, xw), cam.t_wc) };
        if !pc[2].is_finite() || pc[2] <= 1e-5 {
            return None;
        }
        let inv_z = 1.0 / pc[2];
        Some((self.cx + self.focal * pc[0] * inv_z, self.cy + self.focal * pc[1] * inv_z))
    }
}

struct PairMatch {
    p1: (f64, f64),
    p2: (f64, f64),
}

struct EssentialInit {
    r: Mat3,
    t: Vec3,
    inliers: usize,
    median_parallax_deg: f64,
}

#[derive(Clone, Debug)]
struct RotationInit {
    inliers: usize,
    median_err_px: f64,
    all_median_err_px: f64,
}

/// Build an initial [`SceneModel`] (either 6-DOF or `rotation_only`) across all shot frames.
pub(crate) fn initialize_scene(shot: &TrackedShot, refine_focal: bool, is_mono: bool, cancel: &dyn Fn() -> bool) -> Result<SceneModel, SolveError> {
    let cx = shot.width as f64 * 0.5;
    let cy = shot.height as f64 * 0.5;
    let nominal_focal = (shot.height as f64 * 0.5) / (f64::from(CLASSIC_FOV).to_radians() * 0.5).tan();

    let (f0, f1, matches) = match choose_initial_pair(shot) {
        Ok(triple) => triple,
        Err(SolveError::InsufficientMotion) if is_mono && shot.tracks.len() >= 6 => {
            return Err(SolveError::Planar2DVideo);
        }
        Err(e) => return Err(e),
    };
    if cancel() {
        return Err(SolveError::Cancelled);
    }

    let (h_mat, h_inliers) = ransac_homography(&matches);
    let seeds: &[f64] = if refine_focal {
        &[1.0, 0.86, 0.72, 0.60, 1.15, 1.28]
    } else if shot.height > shot.width {
        // Portrait 9:16 smartphone footage has physical focal length proportional to the short
        // sensor axis (`~0.56..0.75 * nominal_focal`), not a 50mm telephoto along the tall axis.
        &[1.0, 0.68, 0.58, 0.82]
    } else {
        &[1.0]
    };
    let (focal, rot_cand, ess_cand) = best_focal_hypothesis(&matches, h_mat, (cx, cy, nominal_focal), seeds, cancel)?;
    let nat_rot = rot_cand.clone().or_else(|| estimate_pair_rotation(&matches, h_mat, cx, cy, focal));
    if is_mono && is_planar_2d_shot(shot, &matches, (cx, cy, focal), h_inliers, nat_rot.as_ref(), ess_cand.as_ref()) {
        return Err(SolveError::Planar2DVideo);
    }
    let n = matches.len();
    let h_dom = h_inliers * 100 >= n * 40 && h_inliers >= 40.min((n * 2) / 3);

    let use_rotation_only = match (&rot_cand, &ess_cand) {
        (Some(rot), Some(ess)) => ess.median_parallax_deg < 1.0 || (h_dom && rot.inliers * 100 >= ess.inliers * 88 && rot.median_err_px <= 2.2),
        (Some(_), None) => true,
        (None, Some(_)) => false,
        (None, None) => {
            return Err(if is_mono && shot.tracks.len() >= 6 {
                SolveError::Planar2DVideo
            } else {
                SolveError::InsufficientMotion
            });
        }
    };

    let mut model = SceneModel {
        cx,
        cy,
        focal,
        initial_focal: focal,
        rotation_only: use_rotation_only,
        ref_cam: f1,
        cameras: vec![CameraPose::default(); shot.times.len()],
        points: vec![None; shot.tracks.len()],
    };

    if use_rotation_only {
        chain_rotations(&mut model, &shot.tracks)?;
    } else if let Some(ess) = ess_cand {
        chain_six_dof(&mut model, &shot.tracks, f0, f1, ess, cancel)?;
        normalize_gauge(&mut model, &shot.tracks);
    } else {
        return Err(SolveError::InsufficientMotion);
    }
    Ok(model)
}

fn choose_initial_pair(shot: &TrackedShot) -> Result<(usize, usize, Vec<PairMatch>), SolveError> {
    let nf = shot.times.len();
    let step = (nf / 48).max(1);
    let total_obs: usize = shot.tracks.iter().map(|t| t.pts.len()).sum();
    let avg_active = total_obs / nf.max(1);
    let min_shared = (avg_active / 3).clamp(8, 22).min(shot.tracks.len().max(8));
    let mut best_pair: Option<(usize, usize, f64)> = None;

    let mut f0 = 0usize;
    while f0 < nf {
        let mut f1 = f0 + 1;
        while f1 < nf {
            let mut steps = Vec::new();
            for tr in &shot.tracks {
                if let (Some(a), Some(b)) = (tr.at_frame(f0), tr.at_frame(f1)) {
                    steps.push((b.0 - a.0).hypot(b.1 - a.1));
                }
            }
            if steps.len() >= min_shared {
                let med = median_f64(&mut steps);
                // Saturate baseline benefit around ~85 px so long panning clips select a well-linked
                // moderate frame span rather than starving intermediate PnP frames across the clip.
                let eff_disp = med.min(85.0);
                let span_penalty = 1.0 / (1.0 + ((f1 - f0) as f64 / 45.0).powi(2) * 0.25);
                let score = eff_disp * (steps.len().min(140) as f64).sqrt() * span_penalty;
                if best_pair.is_none_or(|(_, _, bs)| score > bs) {
                    best_pair = Some((f0, f1, score));
                }
            }
            f1 += step;
        }
        f0 += step;
    }

    let Some((i0, i1, _)) = best_pair else { return Err(SolveError::InsufficientMotion) };
    let matches: Vec<PairMatch> = shot.tracks.iter().filter_map(|tr| Some(PairMatch { p1: tr.at_frame(i0)?, p2: tr.at_frame(i1)? })).collect();
    if matches.len() < 8 {
        return Err(SolveError::InsufficientMotion);
    }
    Ok((i0, i1, matches))
}

fn best_focal_hypothesis(
    matches: &[PairMatch],
    h_mat: Option<Mat3>,
    (cx, cy, nominal_f): (f64, f64, f64),
    scales: &[f64],
    cancel: &dyn Fn() -> bool,
) -> Result<(f64, Option<RotationInit>, Option<EssentialInit>), SolveError> {
    let mut best: Option<(usize, f64, Option<RotationInit>, Option<EssentialInit>)> = None;
    for &s in scales {
        if cancel() {
            return Err(SolveError::Cancelled);
        }
        let f = nominal_f * s;
        let rot = estimate_pair_rotation(matches, h_mat, cx, cy, f);
        let ess = ransac_essential(matches, cx, cy, f);
        let score = ess.as_ref().map_or(0, |e| e.inliers).max(rot.as_ref().map_or(0, |r| r.inliers));
        if score > 0 && best.as_ref().is_none_or(|b| score > b.0) {
            best = Some((score, f, rot, ess));
        }
    }
    Ok(match best {
        Some((_, f, rot, ess)) => (f, rot, ess),
        None => (nominal_f, None, None),
    })
}

// ------------------------- Homography & Pure Rotation -------------------------

fn ransac_homography(matches: &[PairMatch]) -> (Option<Mat3>, usize) {
    if matches.len() < 4 {
        return (None, 0);
    }
    let mut rng = Rng::new(0x484f_4d4f_3230_3236);
    let mut best_h: Option<Mat3> = None;
    let mut best_mask = vec![false; matches.len()];
    let mut best_count = 0usize;

    for _ in 0..RANSAC_ITERS {
        let Some(idx) = rng.sample_indices::<4>(matches.len()) else { break };
        let sub1 = idx.map(|i| matches[i].p1);
        let sub2 = idx.map(|i| matches[i].p2);
        let Some(h) = fit_homography_dlt(&sub1, &sub2) else { continue };
        let (count, mask) = score_homography(matches, h);
        if count > best_count {
            best_count = count;
            best_mask = mask;
            best_h = Some(h);
        }
    }

    if best_count >= 4 {
        let mut p1 = Vec::with_capacity(best_count);
        let mut p2 = Vec::with_capacity(best_count);
        for (m, &keep) in matches.iter().zip(&best_mask) {
            if keep {
                p1.push(m.p1);
                p2.push(m.p2);
            }
        }
        if let Some(polished) = fit_homography_dlt(&p1, &p2) {
            let (c2, _) = score_homography(matches, polished);
            if c2 >= best_count {
                best_h = Some(polished);
                best_count = c2;
            }
        }
    }
    (best_h, best_count)
}

fn fit_homography_dlt(p1: &[(f64, f64)], p2: &[(f64, f64)]) -> Option<Mat3> {
    if p1.len() < 4 || p1.len() != p2.len() {
        return None;
    }
    let (n1, t1) = hartley_normalize(p1)?;
    let (n2, t2) = hartley_normalize(p2)?;
    let t2_inv = mat3_inv(t2)?;

    let mut ata = [[0.0f64; 9]; 9];
    for (&(x, y), &(u, v)) in n1.iter().zip(&n2) {
        let r0 = [-x, -y, -1.0, 0.0, 0.0, 0.0, u * x, u * y, u];
        let r1 = [0.0, 0.0, 0.0, -x, -y, -1.0, v * x, v * y, v];
        accum_row9(&mut ata, &r0);
        accum_row9(&mut ata, &r1);
    }
    let h_vec = smallest_eigvec(ata);
    let h_norm = [[h_vec[0], h_vec[1], h_vec[2]], [h_vec[3], h_vec[4], h_vec[5]], [h_vec[6], h_vec[7], h_vec[8]]];
    let h = mat3_mul(t2_inv, mat3_mul(h_norm, t1));
    (h[2][2].is_finite() && mat3_det(h).abs() > 1e-12).then_some(h)
}

fn score_homography(matches: &[PairMatch], h: Mat3) -> (usize, Vec<bool>) {
    let mut mask = vec![false; matches.len()];
    let mut count = 0usize;
    for (i, m) in matches.iter().enumerate() {
        let q = mat3_mul_vec(h, [m.p1.0, m.p1.1, 1.0]);
        if q[2] > 1e-6 {
            let err = (q[0] / q[2] - m.p2.0).hypot(q[1] / q[2] - m.p2.1);
            if err <= HOMOGRAPHY_THRESH_PX {
                mask[i] = true;
                count += 1;
            }
        }
    }
    (count, mask)
}

fn estimate_pair_rotation(matches: &[PairMatch], h_opt: Option<Mat3>, cx: f64, cy: f64, f: f64) -> Option<RotationInit> {
    let rays: Vec<(Vec3, Vec3)> = matches
        .iter()
        .filter_map(|m| {
            let d1 = normalize3([(m.p1.0 - cx) / f, (m.p1.1 - cy) / f, 1.0])?;
            let d2 = normalize3([(m.p2.0 - cx) / f, (m.p2.1 - cy) / f, 1.0])?;
            Some((d1, d2))
        })
        .collect();
    if rays.len() < 8 {
        return None;
    }

    let mut r = if let Some(h) = h_opt {
        let k = [[f, 0.0, cx], [0.0, f, cy], [0.0, 0.0, 1.0]];
        let k_inv = [[1.0 / f, 0.0, -cx / f], [0.0, 1.0 / f, -cy / f], [0.0, 0.0, 1.0]];
        project_to_so3(mat3_mul(k_inv, mat3_mul(h, k)))
    } else {
        wahba_rotation(&rays, None)?
    };

    // Two rounds of trimmed Wahba SVD on pixel reprojection inliers.
    for _ in 0..2 {
        let keep: Vec<bool> = matches
            .iter()
            .map(|m| {
                let d1 = [(m.p1.0 - cx) / f, (m.p1.1 - cy) / f, 1.0];
                let q = mat3_mul_vec(r, d1);
                q[2] > 1e-5 && (cx + f * q[0] / q[2] - m.p2.0).hypot(cy + f * q[1] / q[2] - m.p2.1) <= 2.2
            })
            .collect();
        if let Some(refined) = wahba_rotation(&rays, Some(&keep)) {
            r = refined;
        }
    }

    let mut errs = Vec::new();
    let mut all_errs = Vec::with_capacity(matches.len());
    let mut inliers = 0usize;
    for m in matches {
        let q = mat3_mul_vec(r, [(m.p1.0 - cx) / f, (m.p1.1 - cy) / f, 1.0]);
        if q[2] <= 1e-5 {
            all_errs.push(1e6);
            continue;
        }
        let err = (cx + f * q[0] / q[2] - m.p2.0).hypot(cy + f * q[1] / q[2] - m.p2.1);
        all_errs.push(err);
        if err <= 3.0 {
            inliers += 1;
            errs.push(err);
        }
    }
    if inliers < 8 {
        return None;
    }
    let median_err_px = median_f64(&mut errs);
    let all_median_err_px = median_f64(&mut all_errs);
    (median_err_px <= 2.6).then_some(RotationInit { inliers, median_err_px, all_median_err_px })
}

/// Detect flat 2D footage (screen recordings, 2D Ken Burns pan/zoom/spin, or locked-off static
/// plates) by testing whether a single 4-DOF 2D similarity transform
/// Multi-cue detector for flat 2D video motion (2D pan/scroll, 2D zoom/Ken-Burns, 2D rotation,
/// 2D footage with static UI/overlays, or static-camera 2D animation) vs genuine 3D camera motion.
fn is_planar_2d_shot(
    shot: &TrackedShot,
    primary_matches: &[PairMatch],
    (cx, cy, nominal_f): (f64, f64, f64),
    h_inliers: usize,
    nat_rot: Option<&RotationInit>,
    ess_cand: Option<&EssentialInit>,
) -> bool {
    // If multi-depth 3D parallax clearly exceeds any single plane (`ess.inliers` substantially beats
    // planar homography `h_inliers` with >= 1.2° triangulation parallax), the scene has real 3D depth!
    if let Some(ess) = ess_cand
        && ess.median_parallax_deg >= 1.2
        && h_inliers * 100 < ess.inliers * 86
    {
        return false;
    }

    if is_planar_2d_pair(primary_matches, (cx, cy), nat_rot) {
        return true;
    }

    // Also test supplementary temporal spans across the shot (`0..N/2`, `N/4..3N/4`, `0..N-1`)
    // so multi-phase 2D pans/zooms or 2D clips with transient foreground occlusions are caught.
    let nf = shot.times.len();
    if nf >= 8 {
        let extra_pairs = [(0, nf - 1), (0, nf / 2), (nf / 4, (3 * nf) / 4)];
        let mut votes_2d = 0usize;
        let mut tested = 0usize;
        for (a, b) in extra_pairs {
            if b <= a {
                continue;
            }
            let m: Vec<PairMatch> = shot
                .tracks
                .iter()
                .filter_map(|tr| Some(PairMatch { p1: tr.at_frame(a)?, p2: tr.at_frame(b)? }))
                .collect();
            if m.len() < 12 {
                continue;
            }
            tested += 1;
            let (hm, hi) = ransac_homography(&m);
            let nr = estimate_pair_rotation(&m, hm, cx, cy, nominal_f);
            let ec = ransac_essential(&m, cx, cy, nominal_f);
            let has_3d_depth = ec.as_ref().is_some_and(|e| e.median_parallax_deg >= 1.2 && hi * 100 < e.inliers * 86);
            if !has_3d_depth && is_planar_2d_pair(&m, (cx, cy), nr.as_ref()) {
                votes_2d += 1;
            }
        }
        if tested >= 2 && votes_2d * 2 >= tested {
            return true;
        }
    }
    false
}

fn is_planar_2d_pair(matches: &[PairMatch], (cx, cy): (f64, f64), nat_rot: Option<&RotationInit>) -> bool {
    let n = matches.len();
    if n < 12 {
        return false;
    }
    let mut disps: Vec<f64> = matches.iter().map(|m| (m.p2.0 - m.p1.0).hypot(m.p2.1 - m.p1.1)).collect();
    let median_disp = median_f64(&mut disps);
    // Nearly static textured shot: no 3D parallax to triangulate, better served by 2D tracking.
    if median_disp < 1.8 {
        return true;
    }

    let pts: Vec<(f64, f64, f64, f64)> = matches.iter().map(|m| (m.p1.0 - cx, m.p1.1 - cy, m.p2.0 - cx, m.p2.1 - cy)).collect();
    let tol = 1.85f64;
    let mut rng = Rng::new(0x3244_5349_4d32_3032);
    let mut best_model = (1.0f64, 0.0f64, 0.0f64, 0.0f64);
    let mut best_mask = vec![false; n];
    let mut best_inliers = 0usize;

    for _ in 0..RANSAC_ITERS {
        let Some([i, j]) = rng.sample_indices::<2>(n) else { break };
        let (x1i, y1i, x2i, y2i) = pts[i];
        let (x1j, y1j, x2j, y2j) = pts[j];
        let dx1 = x1j - x1i;
        let dy1 = y1j - y1i;
        let denom = dx1 * dx1 + dy1 * dy1;
        if denom < 100.0 {
            continue;
        }
        let dx2 = x2j - x2i;
        let dy2 = y2j - y2i;
        let a = (dx1 * dx2 + dy1 * dy2) / denom;
        let b = (dx1 * dy2 - dy1 * dx2) / denom;
        let scale = a.hypot(b);
        if !(0.25..=4.0).contains(&scale) {
            continue;
        }
        let tx = x2i - (a * x1i - b * y1i);
        let ty = y2i - (b * x1i + a * y1i);
        let mut count = 0usize;
        let mut mask = vec![false; n];
        for (k, &(x1, y1, x2, y2)) in pts.iter().enumerate() {
            let err = (a * x1 - b * y1 + tx - x2).hypot(b * x1 + a * y1 + ty - y2);
            if err <= tol {
                count += 1;
                mask[k] = true;
            }
        }
        if count > best_inliers {
            best_inliers = count;
            best_mask = mask;
            best_model = (a, b, tx, ty);
        }
    }

    if best_inliers >= 4
        && let Some(refined) = refine_similarity_2d(&pts, &best_mask)
    {
        best_model = refined;
    }

    let (a, b, tx, ty) = best_model;
    let mut all_errs = Vec::with_capacity(n);
    let mut inlier_errs = Vec::with_capacity(n);
    let mut sim_inliers = 0usize;
    let mut static_hud_inliers = 0usize;
    for &(x1, y1, x2, y2) in &pts {
        let err = (a * x1 - b * y1 + tx - x2).hypot(b * x1 + a * y1 + ty - y2);
        let static_disp = (x2 - x1).hypot(y2 - y1);
        if err <= tol {
            sim_inliers += 1;
            inlier_errs.push(err);
        } else if static_disp <= 1.5 {
            // Stationary 2D HUD / watermark / subtitle / border over a panning 2D video.
            static_hud_inliers += 1;
        }
        all_errs.push(err);
    }
    let two_layer_ratio = (sim_inliers + static_hud_inliers) as f64 / n as f64;
    let sim_ratio = sim_inliers as f64 / n as f64;
    let sim_all_med = median_f64(&mut all_errs);
    let sim_inlier_med = if inlier_errs.is_empty() { sim_all_med } else { median_f64(&mut inlier_errs) };

    if sim_ratio < 0.62 || two_layer_ratio < 0.78 || sim_inlier_med > 1.35 {
        return false;
    }

    let sim_scale = a.hypot(b);
    // Clear 2D zoom without 3D multi-depth parallax is flat 2D footage (Ken-Burns / 2D camera zoom).
    if (sim_scale - 1.0).abs() >= 0.012 {
        return true;
    }
    // If there is a distinct static HUD/background layer alongside a 2D similarity layer, rigid 3D
    // camera rotation cannot produce zero motion on one layer and 2D translation on the rest!
    if static_hud_inliers >= 3 && sim_ratio >= 0.62 {
        return true;
    }

    // Distinguish pure 2D pan/spin from genuine 3D tripod rotation (`H = K R K⁻¹`):
    // Only keep 3D rotation if natural-FOV 3D perspective keystone distinctly beats 2D similarity!
    let beats_2d_via_3d_keystone = nat_rot.is_some_and(|rot| {
        median_disp >= 3.0 && rot.all_median_err_px + 0.25 < sim_all_med && rot.median_err_px <= 1.2
    });
    !beats_2d_via_3d_keystone
}

fn refine_similarity_2d(pts: &[(f64, f64, f64, f64)], mask: &[bool]) -> Option<(f64, f64, f64, f64)> {
    let mut cnt = 0usize;
    let (mut mx1, mut my1, mut mx2, mut my2) = (0.0, 0.0, 0.0, 0.0);
    for (&(x1, y1, x2, y2), &keep) in pts.iter().zip(mask) {
        if keep {
            mx1 += x1;
            my1 += y1;
            mx2 += x2;
            my2 += y2;
            cnt += 1;
        }
    }
    if cnt < 2 {
        return None;
    }
    let inv = 1.0 / cnt as f64;
    mx1 *= inv;
    my1 *= inv;
    mx2 *= inv;
    my2 *= inv;

    let (mut sum_r2, mut num_a, mut num_b) = (0.0, 0.0, 0.0);
    for (&(x1, y1, x2, y2), &keep) in pts.iter().zip(mask) {
        if keep {
            let ux1 = x1 - mx1;
            let uy1 = y1 - my1;
            let ux2 = x2 - mx2;
            let uy2 = y2 - my2;
            sum_r2 += ux1 * ux1 + uy1 * uy1;
            num_a += ux1 * ux2 + uy1 * uy2;
            num_b += ux1 * uy2 - uy1 * ux2;
        }
    }
    if sum_r2 <= 1e-6 {
        return None;
    }
    let a = num_a / sum_r2;
    let b = num_b / sum_r2;
    let tx = mx2 - (a * mx1 - b * my1);
    let ty = my2 - (b * mx1 + a * my1);
    Some((a, b, tx, ty))
}

fn wahba_rotation(rays: &[(Vec3, Vec3)], mask: Option<&[bool]>) -> Option<Mat3> {
    let mut cov = [[0.0f64; 3]; 3];
    let mut count = 0usize;
    for (idx, &(d1, d2)) in rays.iter().enumerate() {
        if mask.is_some_and(|m| !m[idx]) {
            continue;
        }
        for r in 0..3 {
            for c in 0..3 {
                cov[r][c] += d2[r] * d1[c];
            }
        }
        count += 1;
    }
    (count >= 2).then(|| project_to_so3(cov))
}

// ------------------------- Essential Matrix & Cheirality -------------------------

fn ransac_essential(matches: &[PairMatch], cx: f64, cy: f64, f: f64) -> Option<EssentialInit> {
    if matches.len() < 8 {
        return None;
    }
    let cal1: Vec<(f64, f64)> = matches.iter().map(|m| ((m.p1.0 - cx) / f, (m.p1.1 - cy) / f)).collect();
    let cal2: Vec<(f64, f64)> = matches.iter().map(|m| ((m.p2.0 - cx) / f, (m.p2.1 - cy) / f)).collect();

    let mut rng = Rng::new(0x4553_5345_4e54_3344);
    let mut best_e: Option<Mat3> = None;
    let mut best_mask = vec![false; matches.len()];
    let mut best_count = 0usize;

    for _ in 0..RANSAC_ITERS {
        let Some(idx) = rng.sample_indices::<8>(matches.len()) else { break };
        let s1 = idx.map(|i| cal1[i]);
        let s2 = idx.map(|i| cal2[i]);
        let Some(e) = fit_essential_8pt(&s1, &s2) else { continue };
        let (count, mask) = score_sampson(&cal1, &cal2, e, f);
        if count > best_count {
            best_count = count;
            best_mask = mask;
            best_e = Some(e);
        }
    }

    if best_count < 8 {
        return None;
    }
    let mut in1 = Vec::with_capacity(best_count);
    let mut in2 = Vec::with_capacity(best_count);
    let mut in_matches = Vec::with_capacity(best_count);
    for i in 0..matches.len() {
        if best_mask[i] {
            in1.push(cal1[i]);
            in2.push(cal2[i]);
            in_matches.push(&matches[i]);
        }
    }
    let e_final = fit_essential_8pt(&in1, &in2).or(best_e)?;
    select_cheiral_pose(e_final, &in1, &in2, &in_matches, (cx, cy, f))
}

fn fit_essential_8pt(p1: &[(f64, f64)], p2: &[(f64, f64)]) -> Option<Mat3> {
    if p1.len() < 8 || p1.len() != p2.len() {
        return None;
    }
    let (n1, t1) = hartley_normalize(p1)?;
    let (n2, t2) = hartley_normalize(p2)?;
    let mut ata = [[0.0f64; 9]; 9];
    for (&(u1, v1), &(u2, v2)) in n1.iter().zip(&n2) {
        let row = [u2 * u1, u2 * v1, u2, v2 * u1, v2 * v1, v2, u1, v1, 1.0];
        accum_row9(&mut ata, &row);
    }
    let ev = smallest_eigvec(ata);
    let e_norm = [[ev[0], ev[1], ev[2]], [ev[3], ev[4], ev[5]], [ev[6], ev[7], ev[8]]];
    let e_raw = mat3_mul(mat3_transpose(t2), mat3_mul(e_norm, t1));
    Some(project_to_essential(e_raw))
}

fn project_to_essential(e_raw: Mat3) -> Mat3 {
    let (mut u, _, mut v) = svd3(e_raw);
    if mat3_det(u) < 0.0 {
        for r in 0..3 {
            u[r][2] = -u[r][2];
        }
    }
    if mat3_det(v) < 0.0 {
        for r in 0..3 {
            v[r][2] = -v[r][2];
        }
    }
    let u_diag = [[u[0][0], u[0][1], 0.0], [u[1][0], u[1][1], 0.0], [u[2][0], u[2][1], 0.0]];
    mat3_mul(u_diag, mat3_transpose(v))
}

fn score_sampson(p1: &[(f64, f64)], p2: &[(f64, f64)], e: Mat3, f: f64) -> (usize, Vec<bool>) {
    let mut mask = vec![false; p1.len()];
    let mut count = 0usize;
    let et = mat3_transpose(e);
    let max_d2 = (SAMPSON_THRESH_PX / f).powi(2);
    for i in 0..p1.len() {
        let x1 = [p1[i].0, p1[i].1, 1.0];
        let x2 = [p2[i].0, p2[i].1, 1.0];
        let ex1 = mat3_mul_vec(e, x1);
        let etx2 = mat3_mul_vec(et, x2);
        let num = dot3(x2, ex1);
        let den = ex1[0] * ex1[0] + ex1[1] * ex1[1] + etx2[0] * etx2[0] + etx2[1] * etx2[1];
        if den > 1e-15 && (num * num) / den <= max_d2 {
            mask[i] = true;
            count += 1;
        }
    }
    (count, mask)
}

fn select_cheiral_pose(e: Mat3, cal1: &[(f64, f64)], cal2: &[(f64, f64)], in_matches: &[&PairMatch], (cx, cy, f): (f64, f64, f64)) -> Option<EssentialInit> {
    let (mut u, _, mut v) = svd3(e);
    if mat3_det(u) < 0.0 {
        for r in 0..3 {
            u[r][2] = -u[r][2];
        }
    }
    if mat3_det(v) < 0.0 {
        for r in 0..3 {
            v[r][2] = -v[r][2];
        }
    }
    let w = [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
    let wt = mat3_transpose(w);
    let vt = mat3_transpose(v);
    let r1 = mat3_mul(u, mat3_mul(w, vt));
    let r2 = mat3_mul(u, mat3_mul(wt, vt));
    let t_pos = [u[0][2], u[1][2], u[2][2]];
    let t_neg = scale3(t_pos, -1.0);

    let candidates = [(r1, t_pos), (r1, t_neg), (r2, t_pos), (r2, t_neg)];
    let mut best: Option<EssentialInit> = None;

    for (r, t) in candidates {
        let mut good = 0usize;
        let mut angles = Vec::new();
        for i in 0..cal1.len() {
            let Some(xw) = triangulate_two_view(IDENT3, [0.0; 3], cal1[i], r, t, cal2[i]) else { continue };
            let x2 = add3(mat3_mul_vec(r, xw), t);
            if xw[2] <= 1e-3 || x2[2] <= 1e-3 {
                continue;
            }
            let e1 = (cx + f * xw[0] / xw[2] - in_matches[i].p1.0).hypot(cy + f * xw[1] / xw[2] - in_matches[i].p1.1);
            let e2 = (cx + f * x2[0] / x2[2] - in_matches[i].p2.0).hypot(cy + f * x2[1] / x2[2] - in_matches[i].p2.1);
            if e1 <= PNP_INLIER_PX && e2 <= PNP_INLIER_PX {
                good += 1;
                if let (Some(ray1), Some(ray2_w)) = (normalize3(xw), normalize3(mat3_t_mul_vec(r, x2))) {
                    angles.push(dot3(ray1, ray2_w).clamp(-1.0, 1.0).acos().to_degrees());
                }
            }
        }
        // Cheirality check: at least 50% of Sampson inliers must triangulate with positive depth.
        if good * 100 >= cal1.len() * 50 && good >= 8 && best.as_ref().is_none_or(|b| good > b.inliers) {
            let median_parallax_deg = median_f64(&mut angles);
            best = Some(EssentialInit { r, t, inliers: good, median_parallax_deg });
        }
    }
    best
}

pub(crate) fn triangulate_two_view(r1: Mat3, t1: Vec3, p1: (f64, f64), r2: Mat3, t2: Vec3, p2: (f64, f64)) -> Option<Vec3> {
    triangulate_multiview(&[(r1, t1, p1), (r2, t2, p2)])
}

pub(crate) fn triangulate_multiview(obs: &[(Mat3, Vec3, (f64, f64))]) -> Option<Vec3> {
    if obs.len() < 2 {
        return None;
    }
    let mut ata = [[0.0f64; 4]; 4];
    for &(r, t, (xn, yn)) in obs {
        let p0 = [r[0][0], r[0][1], r[0][2], t[0]];
        let p1 = [r[1][0], r[1][1], r[1][2], t[1]];
        let p2 = [r[2][0], r[2][1], r[2][2], t[2]];
        let row_u = [xn * p2[0] - p0[0], xn * p2[1] - p0[1], xn * p2[2] - p0[2], xn * p2[3] - p0[3]];
        let row_v = [yn * p2[0] - p1[0], yn * p2[1] - p1[1], yn * p2[2] - p1[2], yn * p2[3] - p1[3]];
        accum_row4(&mut ata, row_u);
        accum_row4(&mut ata, row_v);
    }
    let h = smallest_eigvec(ata);
    if !h[3].is_finite() || h[3].abs() < 1e-10 {
        return None;
    }
    let pt = [h[0] / h[3], h[1] / h[3], h[2] / h[3]];
    (pt.iter().all(|v| v.is_finite()) && norm3(pt) < 1e5).then_some(pt)
}

// ------------------------- Pure-Rotation Pose Chaining -------------------------

fn chain_rotations(model: &mut SceneModel, tracks: &[FeatureTrack]) -> Result<(), SolveError> {
    let nf = model.cameras.len();
    model.cameras[0] = CameraPose { r_wc: IDENT3, t_wc: [0.0; 3], frozen: false, solved: true };
    for tr in tracks {
        if let Some((u, v)) = tr.at_frame(0) {
            model.points[tr.id] = normalize3([(u - model.cx) / model.focal, (v - model.cy) / model.focal, 1.0]);
        }
    }

    let min_inliers = 6.min(nf);
    let mut last_r = IDENT3;
    for f in 1..nf {
        let mut pairs: Vec<(Vec3, Vec3, (f64, f64))> = Vec::new();
        for tr in tracks {
            if let (Some(dw), Some((u, v))) = (model.points[tr.id], tr.at_frame(f))
                && let Some(dc) = normalize3([(u - model.cx) / model.focal, (v - model.cy) / model.focal, 1.0])
            {
                pairs.push((dw, dc, (u, v)));
            }
        }
        if let Some((r_f, inliers)) = solve_frame_rotation(&pairs, last_r, model.cx, model.cy, model.focal)
            && inliers >= min_inliers
        {
            model.cameras[f] = CameraPose { r_wc: r_f, t_wc: [0.0; 3], frozen: false, solved: true };
            last_r = r_f;
            for tr in tracks {
                if model.points[tr.id].is_none()
                    && let Some((u, v)) = tr.at_frame(f)
                    && let Some(dc) = normalize3([(u - model.cx) / model.focal, (v - model.cy) / model.focal, 1.0])
                {
                    model.points[tr.id] = Some(mat3_t_mul_vec(r_f, dc));
                }
            }
        } else {
            model.cameras[f] = CameraPose { r_wc: last_r, t_wc: [0.0; 3], frozen: true, solved: false };
        }
    }

    // Average world ray directions over all solved frames observing each track.
    for tr in tracks {
        let mut sum = [0.0; 3];
        let mut cnt = 0usize;
        for (offset, &(u, v)) in tr.pts.iter().enumerate() {
            let f = tr.start_frame + offset;
            if model.cameras[f].solved
                && let Some(dc) = normalize3([(u - model.cx) / model.focal, (v - model.cy) / model.focal, 1.0])
            {
                sum = add3(sum, mat3_t_mul_vec(model.cameras[f].r_wc, dc));
                cnt += 1;
            }
        }
        if cnt >= 2 {
            model.points[tr.id] = normalize3(sum);
        }
    }
    Ok(())
}

fn solve_frame_rotation(pairs: &[(Vec3, Vec3, (f64, f64))], seed_r: Mat3, cx: f64, cy: f64, f: f64) -> Option<(Mat3, usize)> {
    if pairs.len() < 4 {
        return None;
    }
    let rays: Vec<(Vec3, Vec3)> = pairs.iter().map(|&(dw, dc, _)| (dw, dc)).collect();
    let mut r = wahba_rotation(&rays, None).unwrap_or(seed_r);
    for _ in 0..3 {
        let keep: Vec<bool> = pairs
            .iter()
            .map(|&(dw, _, (u, v))| {
                let q = mat3_mul_vec(r, dw);
                q[2] > 1e-5 && (cx + f * q[0] / q[2] - u).hypot(cy + f * q[1] / q[2] - v) <= 2.5
            })
            .collect();
        if let Some(next_r) = wahba_rotation(&rays, Some(&keep)) {
            r = next_r;
        }
    }
    let inliers = pairs
        .iter()
        .filter(|&&(dw, _, (u, v))| {
            let q = mat3_mul_vec(r, dw);
            q[2] > 1e-5 && (cx + f * q[0] / q[2] - u).hypot(cy + f * q[1] / q[2] - v) <= 4.2
        })
        .count();
    Some((r, inliers))
}

// ------------------------- 6-DOF Pose Chaining (PnP + Triangulation) -------------------------

fn chain_six_dof(
    model: &mut SceneModel,
    tracks: &[FeatureTrack],
    f0: usize,
    f1: usize,
    ess: EssentialInit,
    cancel: &dyn Fn() -> bool,
) -> Result<(), SolveError> {
    model.cameras[f0] = CameraPose { r_wc: IDENT3, t_wc: [0.0; 3], frozen: false, solved: true };
    model.cameras[f1] = CameraPose { r_wc: ess.r, t_wc: ess.t, frozen: false, solved: true };
    triangulate_new_points(model, tracks, 0.35);

    let nf = model.cameras.len();
    let min_inliers = 8.min(nf);
    for step in 0..nf {
        if step.is_multiple_of(16) && cancel() {
            return Err(SolveError::Cancelled);
        }
        let Some(next_f) = pick_next_pnp_frame(model, tracks) else { break };
        let corrs = gather_pnp_correspondences(model, tracks, next_f);
        let seeds = neighbour_pose_seeds(&model.cameras, next_f);
        if let Some((pose, inliers)) = solve_pnp_frame(model, &corrs, &seeds)
            && inliers >= min_inliers.min(corrs.len())
            && inliers >= 6
        {
            model.cameras[next_f] = CameraPose { r_wc: pose.0, t_wc: pose.1, frozen: false, solved: true };
            triangulate_new_points(model, tracks, 0.45);
        } else {
            let fallback = seeds.first().copied().unwrap_or((IDENT3, [0.0; 3]));
            model.cameras[next_f] = CameraPose { r_wc: fallback.0, t_wc: fallback.1, frozen: true, solved: false };
        }
    }

    // Fill any remaining unvisited frames with their nearest solved pose as frozen.
    let mut carry = model.cameras.iter().find(|c| c.solved).copied().unwrap_or_default();
    for cam in &mut model.cameras {
        if cam.solved {
            carry = *cam;
        } else {
            cam.r_wc = carry.r_wc;
            cam.t_wc = carry.t_wc;
            cam.frozen = true;
        }
    }
    triangulate_new_points(model, tracks, 0.6);
    Ok(())
}

fn pick_next_pnp_frame(model: &SceneModel, tracks: &[FeatureTrack]) -> Option<usize> {
    let mut best: Option<(usize, usize, usize)> = None;
    for f in 0..model.cameras.len() {
        if model.cameras[f].solved || model.cameras[f].frozen {
            continue;
        }
        let obs3d = tracks.iter().filter(|tr| model.points[tr.id].is_some() && tr.at_frame(f).is_some()).count();
        if obs3d == 0 {
            continue;
        }
        let dist_solved = (0..model.cameras.len()).filter(|&k| model.cameras[k].solved).map(|k| k.abs_diff(f)).min().unwrap_or(usize::MAX);
        let cand = (f, obs3d, dist_solved);
        if best.is_none_or(|(_, bo, bd)| (dist_solved <= 2 && bd > 2 && obs3d >= 16) || obs3d > bo || (obs3d == bo && dist_solved < bd)) {
            best = Some(cand);
        }
    }
    best.map(|(f, _, _)| f)
}

fn gather_pnp_correspondences(model: &SceneModel, tracks: &[FeatureTrack], frame: usize) -> Vec<(Vec3, (f64, f64))> {
    tracks.iter().filter_map(|tr| Some((model.points[tr.id]?, tr.at_frame(frame)?))).collect()
}

fn neighbour_pose_seeds(cameras: &[CameraPose], frame: usize) -> Vec<(Mat3, Vec3)> {
    let mut seeds = Vec::new();
    let prev = (0..frame).rev().find(|&k| cameras[k].solved);
    let next = ((frame + 1)..cameras.len()).find(|&k| cameras[k].solved);
    if let (Some(p), Some(n)) = (prev, next) {
        let alpha = (frame - p) as f64 / (n - p).max(1) as f64;
        let t_mid = add3(scale3(cameras[p].t_wc, 1.0 - alpha), scale3(cameras[n].t_wc, alpha));
        seeds.push((cameras[p].r_wc, t_mid));
    }
    if let Some(p) = prev {
        seeds.push((cameras[p].r_wc, cameras[p].t_wc));
        if let Some(p2) = (0..p).rev().find(|&k| cameras[k].solved)
            && p - p2 == frame - p
        {
            let dt = sub3(cameras[p].t_wc, cameras[p2].t_wc);
            let dr = mat3_mul(cameras[p].r_wc, mat3_transpose(cameras[p2].r_wc));
            seeds.push((mat3_mul(dr, cameras[p].r_wc), add3(cameras[p].t_wc, dt)));
        }
    }
    if let Some(n) = next {
        seeds.push((cameras[n].r_wc, cameras[n].t_wc));
    }
    if seeds.is_empty() {
        seeds.push((IDENT3, [0.0; 3]));
    }
    seeds
}

pub(crate) fn solve_pnp_frame(model: &SceneModel, corrs: &[(Vec3, (f64, f64))], seeds: &[(Mat3, Vec3)]) -> Option<((Mat3, Vec3), usize)> {
    if corrs.len() < 6 {
        return None;
    }
    let mut best: Option<((Mat3, Vec3), usize, f64)> = None;
    for &(r0, t0) in seeds {
        let (r_ref, t_ref) = refine_pnp_pose(model, corrs, r0, t0, 12);
        let (inl, cost) = score_pnp(model, corrs, r_ref, t_ref);
        if best.is_none_or(|(_, bi, bc)| inl > bi || (inl == bi && cost < bc)) {
            best = Some(((r_ref, t_ref), inl, cost));
        }
    }

    // If seed extrapolation did not capture >= 70% inliers, run DLT-PnP RANSAC hypotheses.
    if best.is_none_or(|(_, bi, _)| bi * 100 < corrs.len() * 70) {
        let mut rng = Rng::new(0x506e_505f_3670_7400 ^ (corrs.len() as u64));
        for _ in 0..100 {
            let Some(idx) = rng.sample_indices::<6>(corrs.len()) else { break };
            let sub = idx.map(|i| corrs[i]);
            let Some((r_dlt, t_dlt)) = dlt_pnp_6pt(model, &sub) else { continue };
            let (inl0, _) = score_pnp(model, corrs, r_dlt, t_dlt);
            if best.is_some_and(|(_, bi, _)| inl0 + 4 < bi) {
                continue;
            }
            let (r_ref, t_ref) = refine_pnp_pose(model, corrs, r_dlt, t_dlt, 10);
            let (inl, cost) = score_pnp(model, corrs, r_ref, t_ref);
            if best.is_none_or(|(_, bi, bc)| inl > bi || (inl == bi && cost < bc)) {
                best = Some(((r_ref, t_ref), inl, cost));
            }
        }
    }
    best.map(|(pose, inl, _)| (pose, inl))
}

fn dlt_pnp_6pt(model: &SceneModel, sub: &[(Vec3, (f64, f64)); 6]) -> Option<(Mat3, Vec3)> {
    // 12x12 homogeneous DLT on calibrated rays (x_n, y_n) vs world 3D points.
    let mut ata = [[0.0f64; 12]; 12];
    for &(xw, (u, v)) in sub {
        let xn = (u - model.cx) / model.focal;
        let yn = (v - model.cy) / model.focal;
        let r0 = [-xw[0], -xw[1], -xw[2], -1.0, 0.0, 0.0, 0.0, 0.0, xn * xw[0], xn * xw[1], xn * xw[2], xn];
        let r1 = [0.0, 0.0, 0.0, 0.0, -xw[0], -xw[1], -xw[2], -1.0, yn * xw[0], yn * xw[1], yn * xw[2], yn];
        for r in 0..12 {
            for c in 0..12 {
                ata[r][c] += r0[r] * r0[c] + r1[r] * r1[c];
            }
        }
    }
    let p = smallest_eigvec(ata);
    let m = [[p[0], p[1], p[2]], [p[4], p[5], p[6]], [p[8], p[9], p[10]]];
    let (_, s, _) = svd3(m);
    let scale = (s[0] + s[1] + s[2]) / 3.0;
    if !scale.is_finite() || scale < 1e-10 {
        return None;
    }
    let sign = if mat3_det(m) < 0.0 { -1.0 } else { 1.0 };
    let r = project_to_so3([
        [sign * m[0][0] / scale, sign * m[0][1] / scale, sign * m[0][2] / scale],
        [sign * m[1][0] / scale, sign * m[1][1] / scale, sign * m[1][2] / scale],
        [sign * m[2][0] / scale, sign * m[2][1] / scale, sign * m[2][2] / scale],
    ]);
    let t = [sign * p[3] / scale, sign * p[7] / scale, sign * p[11] / scale];
    Some((r, t))
}

/// Analytic Huber Levenberg–Marquardt 6-DOF single-camera PnP pose refinement (`SE(3)` left update).
pub(crate) fn refine_pnp_pose(model: &SceneModel, corrs: &[(Vec3, (f64, f64))], mut r: Mat3, mut t: Vec3, iters: usize) -> (Mat3, Vec3) {
    let f = model.focal;
    let mut lambda = 1e-3f64;
    let (_, mut cur_cost) = score_pnp(model, corrs, r, t);

    for _ in 0..iters {
        let mut h = [0.0f64; 36];
        let mut g = [0.0f64; 6];
        let mut used = 0usize;

        for &(xw, (u_obs, v_obs)) in corrs {
            let pc = add3(mat3_mul_vec(r, xw), t);
            if pc[2] <= 1e-4 {
                continue;
            }
            let inv_z = 1.0 / pc[2];
            let xn = pc[0] * inv_z;
            let yn = pc[1] * inv_z;
            let ru = (model.cx + f * xn) - u_obs;
            let rv = (model.cy + f * yn) - v_obs;
            let err = ru.hypot(rv);
            if err > 24.0 {
                continue;
            }
            let w = if err <= 1.5 { 0.25 } else { 0.25 * (1.5 / err) };
            let ju = [-f * xn * yn, f * (1.0 + xn * xn), -f * yn, f * inv_z, 0.0, -f * xn * inv_z];
            let jv = [-f * (1.0 + yn * yn), f * xn * yn, f * xn, 0.0, f * inv_z, -f * yn * inv_z];
            for a in 0..6 {
                g[a] -= w * (ju[a] * ru + jv[a] * rv);
                for b in 0..6 {
                    h[a * 6 + b] += w * (ju[a] * ju[b] + jv[a] * jv[b]);
                }
            }
            used += 1;
        }
        if used < 6 {
            break;
        }
        for d in 0..6 {
            h[d * 6 + d] = h[d * 6 + d] * (1.0 + lambda) + 1e-6;
        }
        let Some(step) = solve_linear(&mut h, &mut g, 6) else {
            lambda *= 10.0;
            continue;
        };
        let dw = [step[0], step[1], step[2]];
        let dt = [step[3], step[4], step[5]];
        let dr = rodrigues_exp(dw);
        let r_cand = project_to_so3(mat3_mul(dr, r));
        let t_cand = add3(mat3_mul_vec(dr, t), dt);
        let (_, cand_cost) = score_pnp(model, corrs, r_cand, t_cand);
        if cand_cost < cur_cost {
            r = r_cand;
            t = t_cand;
            cur_cost = cand_cost;
            lambda = (lambda / 3.0).max(1e-7);
            if norm3(dw) < 1e-6 && norm3(dt) < 1e-6 {
                break;
            }
        } else {
            lambda = (lambda * 10.0).min(1e5);
        }
    }
    (r, t)
}

fn score_pnp(model: &SceneModel, corrs: &[(Vec3, (f64, f64))], r: Mat3, t: Vec3) -> (usize, f64) {
    let mut inliers = 0usize;
    let mut cost = 0.0f64;
    for &(xw, (u, v)) in corrs {
        let pc = add3(mat3_mul_vec(r, xw), t);
        if pc[2] <= 1e-4 {
            cost += 12.0;
            continue;
        }
        let ru = model.cx + model.focal * (pc[0] / pc[2]) - u;
        let rv = model.cy + model.focal * (pc[1] / pc[2]) - v;
        let e = ru.hypot(rv);
        if e <= PNP_INLIER_PX {
            inliers += 1;
        }
        let capped = e.min(16.0);
        cost += if capped <= 1.5 { 0.5 * capped * capped * 0.25 } else { 1.5 * (capped - 0.75) * 0.25 };
    }
    (inliers, cost)
}

fn triangulate_new_points(model: &mut SceneModel, tracks: &[FeatureTrack], min_parallax_deg: f64) {
    for tr in tracks {
        if model.points[tr.id].is_some() {
            continue;
        }
        let mut obs = Vec::new();
        let mut raw_uv = Vec::new();
        for (offset, &(u, v)) in tr.pts.iter().enumerate() {
            let f = tr.start_frame + offset;
            let cam = &model.cameras[f];
            if cam.solved {
                obs.push((cam.r_wc, cam.t_wc, ((u - model.cx) / model.focal, (v - model.cy) / model.focal)));
                raw_uv.push((f, u, v));
            }
        }
        if obs.len() < 2 {
            continue;
        }
        let Some(xw) = triangulate_multiview(&obs) else { continue };
        if !validate_triangulated_point(model, xw, &raw_uv, min_parallax_deg) {
            continue;
        }
        model.points[tr.id] = Some(xw);
    }
}

fn validate_triangulated_point(model: &SceneModel, xw: Vec3, raw_uv: &[(usize, f64, f64)], min_parallax_deg: f64) -> bool {
    let mut rays_world = Vec::with_capacity(raw_uv.len());
    let mut good = 0usize;
    for &(f, u, v) in raw_uv {
        let cam = &model.cameras[f];
        let pc = add3(mat3_mul_vec(cam.r_wc, xw), cam.t_wc);
        if pc[2] <= 1e-3 {
            return false;
        }
        let err = (model.cx + model.focal * pc[0] / pc[2] - u).hypot(model.cy + model.focal * pc[1] / pc[2] - v);
        if err <= PNP_INLIER_PX {
            good += 1;
        }
        if let Some(rw) = normalize3(mat3_t_mul_vec(cam.r_wc, pc)) {
            rays_world.push(rw);
        }
    }
    if good * 4 < raw_uv.len() * 3 || rays_world.len() < 2 {
        return false;
    }
    // Check widest angle between first, middle, and last observing rays.
    let first = rays_world[0];
    let mid = rays_world[rays_world.len() / 2];
    let last = rays_world[rays_world.len() - 1];
    let max_ang = dot3(first, last).clamp(-1.0, 1.0).acos().max(dot3(first, mid).clamp(-1.0, 1.0).acos()).to_degrees();
    max_ang >= min_parallax_deg
}

/// Transform poses and 3D landmarks into the canonical 7-DOF Frame-0 gauge:
/// frame 0 (or first solved frame) at `(I, [0, 0, 0])`, median positive depth at frame 0 equal to `1.0`.
pub(crate) fn normalize_gauge(model: &mut SceneModel, tracks: &[FeatureTrack]) {
    if model.rotation_only || model.cameras.is_empty() {
        return;
    }
    let anchor = model.cameras.iter().position(|c| c.solved && !c.frozen).unwrap_or(0);
    let r0 = model.cameras[anchor].r_wc;
    let t0 = model.cameras[anchor].t_wc;
    let r0_t = mat3_transpose(r0);

    for cam in &mut model.cameras {
        let r_new = project_to_so3(mat3_mul(cam.r_wc, r0_t));
        let t_new = sub3(cam.t_wc, mat3_mul_vec(r_new, t0));
        cam.r_wc = r_new;
        cam.t_wc = t_new;
    }
    for pt in model.points.iter_mut().flatten() {
        *pt = add3(mat3_mul_vec(r0, *pt), t0);
    }

    let mut depths: Vec<f64> = tracks
        .iter()
        .filter(|tr| tr.at_frame(anchor).is_some())
        .filter_map(|tr| model.points[tr.id].map(|p| p[2]))
        .filter(|&z| z.is_finite() && z > 1e-3)
        .collect();
    if depths.is_empty() {
        depths = model.points.iter().flatten().map(|p| p[2]).filter(|&z| z.is_finite() && z > 1e-3).collect();
    }
    let med_z = median_f64(&mut depths);
    if !med_z.is_finite() || med_z <= 1e-5 {
        return;
    }
    let inv_scale = 1.0 / med_z;
    for cam in &mut model.cameras {
        cam.t_wc = scale3(cam.t_wc, inv_scale);
    }
    for pt in model.points.iter_mut().flatten() {
        *pt = scale3(*pt, inv_scale);
    }
    model.cameras[anchor].r_wc = IDENT3;
    model.cameras[anchor].t_wc = [0.0; 3];
    model.ref_cam = model
        .cameras
        .iter()
        .enumerate()
        .filter(|&(idx, c)| idx != anchor && c.solved && !c.frozen)
        .max_by(|(_, a), (_, b)| norm3(a.t_wc).total_cmp(&norm3(b.t_wc)))
        .map(|(idx, _)| idx)
        .unwrap_or(anchor);
}

fn accum_row9(ata: &mut [[f64; 9]; 9], row: &[f64; 9]) {
    for r in 0..9 {
        for c in 0..9 {
            ata[r][c] += row[r] * row[c];
        }
    }
}

fn accum_row4(ata: &mut [[f64; 4]; 4], row: [f64; 4]) {
    let norm = (row[0] * row[0] + row[1] * row[1] + row[2] * row[2] + row[3] * row[3]).sqrt();
    if norm < 1e-12 {
        return;
    }
    let s = 1.0 / norm;
    let nr = [row[0] * s, row[1] * s, row[2] * s, row[3] * s];
    for r in 0..4 {
        for c in 0..4 {
            ata[r][c] += nr[r] * nr[c];
        }
    }
}
