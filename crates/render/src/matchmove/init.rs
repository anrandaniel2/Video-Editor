//! Stage 4: two-view geometric initialization (Hartley-normalised 4-pt RANSAC homography vs.
//! widest-baseline 8-pt RANSAC essential matrix + 4-way cheirality), pure-rotation fallback
//! (`H = K · R · K⁻¹`, `t ≡ 0`), incremental PnP + multi-view triangulation pose chaining, and
//! 7-DOF Frame-0 gauge normalisation.

use filmcraft_project::CLASSIC_FOV;

use super::SolveError;
use super::linalg::{
    IDENT3, Mat3, Rng, Vec3, add3, cross3, dot3, hartley_normalize, mat3_det, mat3_inv, mat3_mul, mat3_mul_vec, mat3_t_mul_vec, mat3_transpose, median_f64,
    norm3, normalize3, project_to_so3, rodrigues_exp, scale3, skew3, smallest_eigvec, solve_linear, sub3, svd3,
};
use super::tracks::{FeatureTrack, TrackedShot};

const RANSAC_ITERS: usize = 2000;
const HOMOGRAPHY_THRESH_PX: f64 = 2.5;
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
    r: Mat3,
    inliers: usize,
    median_err_px: f64,
}

/// Build an initial [`SceneModel`] (either 6-DOF or `rotation_only`) across all shot frames.
pub(crate) fn initialize_scene(shot: &TrackedShot, refine_focal: bool, cancel: &dyn Fn() -> bool) -> Result<SceneModel, SolveError> {
    let cx = shot.width as f64 * 0.5;
    let cy = shot.height as f64 * 0.5;
    let nominal_focal = (shot.height as f64 * 0.5) / (f64::from(CLASSIC_FOV).to_radians() * 0.5).tan();

    let (f0, f1, matches) = choose_initial_pair(shot)?;
    if cancel() {
        return Err(SolveError::Cancelled);
    }

    let (h_mat, h_inliers) = ransac_homography(&matches);
    let seeds: &[f64] = if refine_focal {
        &[1.0, 0.86, 0.72, 0.60, 0.48, 1.15, 1.28, 1.45]
    } else if shot.height > shot.width {
        // Portrait 9:16 smartphone footage has physical focal length proportional to the short
        // sensor axis (`~0.50..0.82 * nominal_focal`), not a 50mm telephoto along the tall axis.
        &[1.0, 0.68, 0.58, 0.50, 0.82, 1.15]
    } else {
        // Landscape smartphone ultra-wide (~0.55x), main 24-26mm (~0.80..1.0x), and tele (~1.25x).
        &[1.0, 0.82, 0.68, 1.18]
    };
    let (focal, rot_cand, ess_cand) = best_focal_hypothesis(&matches, h_mat, (cx, cy, nominal_focal), seeds, cancel)?;
    let n = matches.len();
    let h_dom = h_inliers * 100 >= n * 40 && h_inliers >= 40.min((n * 2) / 3);

    let use_rotation_only = match (&rot_cand, &ess_cand) {
        (Some(rot), Some(ess)) => ess.median_parallax_deg < 1.0 || (h_dom && rot.inliers * 100 >= ess.inliers * 88 && rot.median_err_px <= 2.2),
        (Some(_), None) => true,
        (None, Some(_)) => false,
        (None, None) => return Err(SolveError::InsufficientMotion),
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
        chain_rotations(&mut model, &shot.tracks, f0)?;
        normalize_rotation_gauge(&mut model);
    } else if let Some(ess) = ess_cand {
        chain_six_dof(&mut model, &shot.tracks, f0, f1, ess, cancel)?;
        let solved_count = model.cameras.iter().filter(|c| c.solved && !c.frozen).count();
        // If 6-DOF PnP starved on a low-parallax / planar / zoomed smartphone clip (solving fewer
        // than 8 frames or < 35% of the shot) while pure SO(3) rotation fits the pair cleanly,
        // recover with bidirectional SO(3) rotation chaining rather than failing the scan!
        if (solved_count < 8 || solved_count * 100 < shot.times.len() * 35) && rot_cand.is_some() {
            let mut rot_model = SceneModel {
                cx,
                cy,
                focal,
                initial_focal: focal,
                rotation_only: true,
                ref_cam: f0,
                cameras: vec![CameraPose::default(); shot.times.len()],
                points: vec![None; shot.tracks.len()],
            };
            if chain_rotations(&mut rot_model, &shot.tracks, f0).is_ok() {
                normalize_rotation_gauge(&mut rot_model);
                let rot_solved = rot_model.cameras.iter().filter(|c| c.solved && !c.frozen).count();
                if rot_solved > solved_count {
                    return Ok(rot_model);
                }
            }
        }
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
        let ess = ransac_essential(matches, h_mat, cx, cy, f);
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
    let mut rays: Vec<(Vec3, Vec3)> = Vec::with_capacity(matches.len());
    let mut valid_matches: Vec<&PairMatch> = Vec::with_capacity(matches.len());
    for m in matches {
        if let (Some(d1), Some(d2)) = (
            normalize3([(m.p1.0 - cx) / f, (m.p1.1 - cy) / f, 1.0]),
            normalize3([(m.p2.0 - cx) / f, (m.p2.1 - cy) / f, 1.0]),
        ) {
            rays.push((d1, d2));
            valid_matches.push(m);
        }
    }
    if rays.len() < 6 {
        return None;
    }

    let score_rot = |cand: Mat3, tol: f64| -> (usize, f64) {
        let mut inl = 0usize;
        let mut err_sum = 0.0f64;
        for m in &valid_matches {
            let q = mat3_mul_vec(cand, [(m.p1.0 - cx) / f, (m.p1.1 - cy) / f, 1.0]);
            if q[2] > 1e-5 {
                let err = (cx + f * q[0] / q[2] - m.p2.0).hypot(cy + f * q[1] / q[2] - m.p2.1);
                if err <= tol {
                    inl += 1;
                    err_sum += err;
                }
            }
        }
        (inl, err_sum)
    };

    // Never trust unconstrained 8-DOF homography `project_to_so3(K^-1 H K)` alone: slight tracking
    // noise or subtle parallax in `H[2][0..1]` gets amplified by `f ~ 500`, corrupting `R` by 20-90°!
    // Evaluate direct Wahba SVD, 2-point SO(3) RANSAC (robust against moving foreground people/cars),
    // and `K^-1 H K`, then pick the strongest rotation seed.
    let mut best_seed: Option<(Mat3, usize, f64)> = None;
    let mut consider = |cand: Mat3| {
        let (inl, err_sum) = score_rot(cand, 4.5);
        if best_seed.is_none_or(|(_, bi, be)| inl > bi || (inl == bi && err_sum < be)) {
            best_seed = Some((cand, inl, err_sum));
        }
    };

    if let Some(r_wahba) = wahba_rotation(&rays, None) {
        consider(r_wahba);
    }
    if let Some(h) = h_opt {
        let s = h[2][2].signum();
        let h_signed = if s < 0.0 {
            [[-h[0][0], -h[0][1], -h[0][2]], [-h[1][0], -h[1][1], -h[1][2]], [-h[2][0], -h[2][1], -h[2][2]]]
        } else {
            h
        };
        let k = [[f, 0.0, cx], [0.0, f, cy], [0.0, 0.0, 1.0]];
        let k_inv = [[1.0 / f, 0.0, -cx / f], [0.0, 1.0 / f, -cy / f], [0.0, 0.0, 1.0]];
        consider(project_to_so3(mat3_mul(k_inv, mat3_mul(h_signed, k))));
    }
    let mut rng = Rng::new(0x524f_5433_5f32_5054 ^ (rays.len() as u64));
    for _ in 0..180 {
        let Some([i, j]) = rng.sample_indices::<2>(rays.len()) else { break };
        if dot3(rays[i].0, rays[j].0) > 0.9985 {
            continue;
        }
        let sub = [rays[i], rays[j]];
        if let Some(r_cand) = wahba_rotation(&sub, None) {
            consider(r_cand);
        }
    }

    let mut r = best_seed.map(|(r0, _, _)| r0)?;

    // Three rounds of coarse-to-fine trimmed Wahba SVD on pixel reprojection inliers.
    for &tol in &[5.0, 3.6, 2.8] {
        let keep: Vec<bool> = valid_matches
            .iter()
            .map(|m| {
                let d1 = [(m.p1.0 - cx) / f, (m.p1.1 - cy) / f, 1.0];
                let q = mat3_mul_vec(r, d1);
                q[2] > 1e-5 && (cx + f * q[0] / q[2] - m.p2.0).hypot(cy + f * q[1] / q[2] - m.p2.1) <= tol
            })
            .collect();
        if let Some(refined) = wahba_rotation(&rays, Some(&keep)) {
            r = refined;
        }
    }

    let mut errs = Vec::new();
    let mut inliers = 0usize;
    for m in &valid_matches {
        let q = mat3_mul_vec(r, [(m.p1.0 - cx) / f, (m.p1.1 - cy) / f, 1.0]);
        if q[2] <= 1e-5 {
            continue;
        }
        let err = (cx + f * q[0] / q[2] - m.p2.0).hypot(cy + f * q[1] / q[2] - m.p2.1);
        if err <= 3.6 {
            inliers += 1;
            errs.push(err);
        }
    }
    if inliers < 6 {
        return None;
    }
    let median_err_px = median_f64(&mut errs);
    (median_err_px <= 3.2).then_some(RotationInit { r, inliers, median_err_px })
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

fn ransac_essential(matches: &[PairMatch], h_opt: Option<Mat3>, cx: f64, cy: f64, f: f64) -> Option<EssentialInit> {
    if matches.len() < 8 {
        return None;
    }
    let cal1: Vec<(f64, f64)> = matches.iter().map(|m| ((m.p1.0 - cx) / f, (m.p1.1 - cy) / f)).collect();
    let cal2: Vec<(f64, f64)> = matches.iter().map(|m| ((m.p2.0 - cx) / f, (m.p2.1 - cy) / f)).collect();
    let all_refs: Vec<&PairMatch> = matches.iter().collect();

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

    let mut best_pose: Option<EssentialInit> = None;
    if best_count >= 8 {
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
        if let Some(e_final) = fit_essential_8pt(&in1, &in2).or(best_e)
            && let Some(cand) = select_cheiral_pose(e_final, &in1, &in2, &in_matches, (cx, cy, f))
        {
            best_pose = Some(cand);
        }
    }

    // Planar / dominant-ground-plane fallback: 8-point linear essential matrix is algebraically
    // rank-2 degenerate on planar scenes (tables, floors, walls, sidewalks), returning random
    // nullspace mixes that fail cheirality! Decompose `H_cal = K^-1 H K` via Malis & Vargas (2007)
    // analytical homography decomposition and compete directly on triangulated cheirality inliers.
    if let Some(h) = h_opt
        && let Some(planar) = decompose_planar_homography(h, &cal1, &cal2, &all_refs, (cx, cy, f))
        && best_pose.as_ref().is_none_or(|bp| {
            planar.inliers > bp.inliers + 2 || (planar.inliers + 2 >= bp.inliers && planar.median_parallax_deg > bp.median_parallax_deg)
        })
    {
        best_pose = Some(planar);
    }

    // Approximate-rotation + 2-point translation RANSAC fallback: when gentle handheld motion has
    // small translation (`1..5°` parallax), `R` is well-constrained by rays while `t` is cleanly
    // recovered by cross-product epipolar intersection of 2 ray pairs (`E = [t]_x R`).
    if best_pose.as_ref().is_none_or(|bp| bp.inliers * 100 < matches.len() * 45)
        && let Some(rot_approx) = estimate_pair_rotation(matches, h_opt, cx, cy, f)
    {
        let mut best_t_e: Option<(Mat3, usize, Vec<bool>)> = None;
        let mut rng_t = Rng::new(0x5452_414e_5332_5054 ^ (matches.len() as u64));
        for _ in 0..240 {
            let Some([i, j]) = rng_t.sample_indices::<2>(matches.len()) else { break };
            let rx1_i = mat3_mul_vec(rot_approx.r, [cal1[i].0, cal1[i].1, 1.0]);
            let x2_i = [cal2[i].0, cal2[i].1, 1.0];
            let n_i = cross3(rx1_i, x2_i);
            let rx1_j = mat3_mul_vec(rot_approx.r, [cal1[j].0, cal1[j].1, 1.0]);
            let x2_j = [cal2[j].0, cal2[j].1, 1.0];
            let n_j = cross3(rx1_j, x2_j);
            let Some(t_dir) = normalize3(cross3(n_i, n_j)) else { continue };
            let e_cand = mat3_mul(skew3(t_dir), rot_approx.r);
            let (cnt, msk) = score_sampson(&cal1, &cal2, e_cand, f);
            if cnt >= 8 && best_t_e.as_ref().is_none_or(|(_, bc, _)| cnt > *bc) {
                best_t_e = Some((e_cand, cnt, msk));
            }
        }
        if let Some((e_cand, cnt, msk)) = best_t_e {
            let mut in1 = Vec::with_capacity(cnt);
            let mut in2 = Vec::with_capacity(cnt);
            let mut in_m = Vec::with_capacity(cnt);
            for i in 0..matches.len() {
                if msk[i] {
                    in1.push(cal1[i]);
                    in2.push(cal2[i]);
                    in_m.push(&matches[i]);
                }
            }
            if let Some(cand) = select_cheiral_pose(e_cand, &in1, &in2, &in_m, (cx, cy, f))
                && best_pose.as_ref().is_none_or(|bp| cand.inliers > bp.inliers)
            {
                best_pose = Some(cand);
            }
        }
    }

    best_pose
}

/// Analytical planar homography decomposition (`H_cal = R + t * n^T`) into up to 4 `(R, t)` poses,
/// evaluated by positive-depth two-view triangulation across all correspondences.
fn decompose_planar_homography(
    h_px: Mat3,
    cal1: &[(f64, f64)],
    cal2: &[(f64, f64)],
    matches: &[&PairMatch],
    (cx, cy, f): (f64, f64, f64),
) -> Option<EssentialInit> {
    let k = [[f, 0.0, cx], [0.0, f, cy], [0.0, 0.0, 1.0]];
    let k_inv = [[1.0 / f, 0.0, -cx / f], [0.0, 1.0 / f, -cy / f], [0.0, 0.0, 1.0]];
    let h_raw = mat3_mul(k_inv, mat3_mul(h_px, k));
    let (_, sv, _) = svd3(h_raw);
    let gamma = sv[1];
    if !gamma.is_finite() || gamma <= 1e-9 {
        return None;
    }
    let det_h = mat3_det(h_raw);
    let sign = if det_h < 0.0 { -1.0 } else { 1.0 };
    let scale = sign / gamma;
    let h = [
        [h_raw[0][0] * scale, h_raw[0][1] * scale, h_raw[0][2] * scale],
        [h_raw[1][0] * scale, h_raw[1][1] * scale, h_raw[1][2] * scale],
        [h_raw[2][0] * scale, h_raw[2][1] * scale, h_raw[2][2] * scale],
    ];

    // Symmetric matrix `S = H^T H - I`. Its middle eigenvalue is ~0; `s1 >= 0 >= s3`.
    let ht = mat3_transpose(h);
    let hth = mat3_mul(ht, h);
    let (u_ht, d_sq, v_h) = svd3(hth);
    let _ = u_ht;
    let d1 = d_sq[0].max(1.0).sqrt();
    let d3 = d_sq[2].clamp(0.0, 1.0).sqrt();
    if (d1 - d3).abs() < 1e-5 {
        return None;
    }
    let v1 = [v_h[0][0], v_h[1][0], v_h[2][0]];
    let mut v2 = [v_h[0][1], v_h[1][1], v_h[2][1]];
    let v3 = [v_h[0][2], v_h[1][2], v_h[2][2]];
    if dot3(cross3(v1, v2), v3) < 0.0 {
        v2 = scale3(v2, -1.0);
    }

    let denom = (d1 * d1 - d3 * d3).max(1e-12).sqrt();
    let a1 = (d1 * d1 - 1.0).max(0.0).sqrt() / denom;
    let b1 = (1.0 - d3 * d3).max(0.0).sqrt() / denom;

    let mut best: Option<EssentialInit> = None;
    for eps1 in [1.0f64, -1.0] {
        for eps3 in [1.0f64, -1.0] {
            let n_cand = add3(scale3(v1, eps1 * a1), scale3(v3, eps3 * b1));
            let Some(n_unit) = normalize3(n_cand) else { continue };
            // Wait: `H = R + t * n^T` where `||t|| = d1 - d3` up to sign!
            // More directly: for unit normal `n` in frame 1, any vector orthogonal to `n` satisfies
            // `H * u = R * u`! Pick orthonormal basis `(u_a, u_b, n)` with `u_a = v2`:
            let u_a = v2;
            let Some(u_b) = normalize3(cross3(n_unit, u_a)) else { continue };
            let Some(ru_a) = normalize3(mat3_mul_vec(h, u_a)) else { continue };
            let Some(ru_b) = normalize3(mat3_mul_vec(h, u_b)) else { continue };
            let rn = cross3(ru_a, ru_b);
            // Since `R * [u_a | u_b | n] = [ru_a | ru_b | rn]`, `R = [ru_a | ru_b | rn] * [u_a | u_b | n]^T`.
            let b_src = [[u_a[0], u_b[0], n_unit[0]], [u_a[1], u_b[1], n_unit[1]], [u_a[2], u_b[2], n_unit[2]]];
            let b_dst = [[ru_a[0], ru_b[0], rn[0]], [ru_a[1], ru_b[1], rn[1]], [ru_a[2], ru_b[2], rn[2]]];
            let r_cand = project_to_so3(mat3_mul(b_dst, mat3_transpose(b_src)));

            for h_sign in [1.0f64, -1.0] {
                let hn = scale3(mat3_mul_vec(h, n_unit), h_sign);
                let rn_vec = mat3_mul_vec(r_cand, n_unit);
                let Some(t_unit) = normalize3(sub3(hn, rn_vec)) else { continue };
                let mut good = 0usize;
                let mut angles = Vec::new();
                for i in 0..cal1.len() {
                    let Some(xw) = triangulate_two_view(IDENT3, [0.0; 3], cal1[i], r_cand, t_unit, cal2[i]) else { continue };
                    let x2 = add3(mat3_mul_vec(r_cand, xw), t_unit);
                    if xw[2] <= 1e-3 || x2[2] <= 1e-3 {
                        continue;
                    }
                    let e1 = (cx + f * xw[0] / xw[2] - matches[i].p1.0).hypot(cy + f * xw[1] / xw[2] - matches[i].p1.1);
                    let e2 = (cx + f * x2[0] / x2[2] - matches[i].p2.0).hypot(cy + f * x2[1] / x2[2] - matches[i].p2.1);
                    if e1 <= PNP_INLIER_PX && e2 <= PNP_INLIER_PX {
                        good += 1;
                        if let (Some(ray1), Some(ray2_w)) = (normalize3(xw), normalize3(mat3_t_mul_vec(r_cand, x2))) {
                            angles.push(dot3(ray1, ray2_w).clamp(-1.0, 1.0).acos().to_degrees());
                        }
                    }
                }
                if good >= 8 && good * 100 >= cal1.len() * 35 && best.as_ref().is_none_or(|b| good > b.inliers) {
                    let median_parallax_deg = median_f64(&mut angles);
                    best = Some(EssentialInit { r: r_cand, t: t_unit, inliers: good, median_parallax_deg });
                }
            }
        }
    }
    best
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

fn chain_rotations(model: &mut SceneModel, tracks: &[FeatureTrack], start_f: usize) -> Result<(), SolveError> {
    let nf = model.cameras.len();
    let seed = start_f.min(nf.saturating_sub(1));
    model.cameras[seed] = CameraPose { r_wc: IDENT3, t_wc: [0.0; 3], frozen: false, solved: true };
    for tr in tracks {
        if let Some((u, v)) = tr.at_frame(seed) {
            model.points[tr.id] = normalize3([(u - model.cx) / model.focal, (v - model.cy) / model.focal, 1.0]);
        }
    }

    let min_inliers = 6.min(nf);
    let solve_and_seed = |model: &mut SceneModel, f: usize, last_r: &mut Mat3| {
        let mut pairs: Vec<(Vec3, Vec3, (f64, f64))> = Vec::new();
        for tr in tracks {
            if let (Some(dw), Some((u, v))) = (model.points[tr.id], tr.at_frame(f))
                && let Some(dc) = normalize3([(u - model.cx) / model.focal, (v - model.cy) / model.focal, 1.0])
            {
                pairs.push((dw, dc, (u, v)));
            }
        }
        if let Some((r_f, inliers)) = solve_frame_rotation(&pairs, *last_r, model.cx, model.cy, model.focal)
            && inliers >= min_inliers
        {
            model.cameras[f] = CameraPose { r_wc: r_f, t_wc: [0.0; 3], frozen: false, solved: true };
            *last_r = r_f;
            for tr in tracks {
                if model.points[tr.id].is_none()
                    && let Some((u, v)) = tr.at_frame(f)
                    && let Some(dc) = normalize3([(u - model.cx) / model.focal, (v - model.cy) / model.focal, 1.0])
                {
                    model.points[tr.id] = Some(mat3_t_mul_vec(r_f, dc));
                }
            }
        } else {
            model.cameras[f] = CameraPose { r_wc: *last_r, t_wc: [0.0; 3], frozen: true, solved: false };
        }
    };

    let mut fwd_r = IDENT3;
    for f in (seed + 1)..nf {
        solve_and_seed(model, f, &mut fwd_r);
    }
    let mut bwd_r = IDENT3;
    for f in (0..seed).rev() {
        solve_and_seed(model, f, &mut bwd_r);
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

/// Rebase a rotation-only model so the first solved frame (typically Frame 0) has identity rotation `I`.
pub(crate) fn normalize_rotation_gauge(model: &mut SceneModel) {
    if !model.rotation_only || model.cameras.is_empty() {
        return;
    }
    let anchor = model.cameras.iter().position(|c| c.solved && !c.frozen).unwrap_or(0);
    let r0 = model.cameras[anchor].r_wc;
    let r0_t = mat3_transpose(r0);
    for cam in &mut model.cameras {
        cam.r_wc = project_to_so3(mat3_mul(cam.r_wc, r0_t));
    }
    for pt in model.points.iter_mut().flatten() {
        *pt = normalize3(mat3_mul_vec(r0, *pt)).unwrap_or(*pt);
    }
    model.cameras[anchor].r_wc = IDENT3;
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

    // Coplanar-safe 4-point RANSAC PnP + 6-point DLT RANSAC when seed extrapolation did not capture >= 70% inliers.
    // Note: 12x12 6-pt DLT is singular on coplanar scenes (floors, tables, walls), whereas `coplanar_pnp_4pt`
    // solves planar 3D-2D homography cleanly and works on both planar and general point subsets!
    if best.is_none_or(|(_, bi, _)| bi * 100 < corrs.len() * 70) {
        let mut rng = Rng::new(0x506e_505f_3670_7400 ^ (corrs.len() as u64));
        for _ in 0..90 {
            if let Some(idx4) = rng.sample_indices::<4>(corrs.len()) {
                let sub4 = idx4.map(|i| corrs[i]);
                if let Some((r_pl, t_pl)) = coplanar_pnp_4pt(model, &sub4) {
                    let (inl0, _) = score_pnp(model, corrs, r_pl, t_pl);
                    if best.is_none_or(|(_, bi, _)| inl0 + 4 >= bi) {
                        let (r_ref, t_ref) = refine_pnp_pose(model, corrs, r_pl, t_pl, 10);
                        let (inl, cost) = score_pnp(model, corrs, r_ref, t_ref);
                        if best.is_none_or(|(_, bi, bc)| inl > bi || (inl == bi && cost < bc)) {
                            best = Some(((r_ref, t_ref), inl, cost));
                        }
                    }
                }
            }
            let Some(idx) = rng.sample_indices::<6>(corrs.len()) else { continue };
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

/// Coplanar-safe 4-point PnP: fits local plane frame `(c, e1, e2, e3)` to 4 world points and solves
/// the calibrated 2D-to-2D homography `H = [r1 | r2 | R*c + t]` for `(R, t)`.
fn coplanar_pnp_4pt(model: &SceneModel, sub: &[(Vec3, (f64, f64)); 4]) -> Option<(Mat3, Vec3)> {
    let mut c = [0.0; 3];
    for &(xw, _) in sub {
        c = add3(c, xw);
    }
    c = scale3(c, 0.25);
    let mut cov = [[0.0f64; 3]; 3];
    for &(xw, _) in sub {
        let d = sub3(xw, c);
        for r in 0..3 {
            for col in 0..3 {
                cov[r][col] += d[r] * d[col];
            }
        }
    }
    let (_, sv, v_pca) = svd3(cov);
    // Reject only if the 4 points are nearly collinear (`sv[1]` tiny) OR strongly non-planar (`sv[2]` large vs `sv[1]`).
    if sv[1] < 1e-8 || sv[2] > sv[1] * 0.12 {
        return None;
    }
    let e1 = [v_pca[0][0], v_pca[1][0], v_pca[2][0]];
    let e2 = [v_pca[0][1], v_pca[1][1], v_pca[2][1]];
    let e3 = cross3(e1, e2);

    let mut pts_plane = [(0.0, 0.0); 4];
    let mut pts_cal = [(0.0, 0.0); 4];
    for i in 0..4 {
        let d = sub3(sub[i].0, c);
        pts_plane[i] = (dot3(d, e1), dot3(d, e2));
        pts_cal[i] = ((sub[i].1.0 - model.cx) / model.focal, (sub[i].1.1 - model.cy) / model.focal);
    }
    let h = fit_homography_dlt(&pts_plane, &pts_cal)?;
    let c0 = [h[0][0], h[1][0], h[2][0]];
    let c1 = [h[0][1], h[1][1], h[2][1]];
    let c2 = [h[0][2], h[1][2], h[2][2]];
    let sc = 0.5 * (norm3(c0) + norm3(c1));
    if !sc.is_finite() || sc < 1e-9 {
        return None;
    }
    // Sign of `H` is fixed by requiring positive camera depth at the centroid (`t_plane.z > 0`).
    let sign = if c2[2] < 0.0 { -1.0 } else { 1.0 };
    let r1_p = scale3(c0, sign / sc);
    let r2_p = scale3(c1, sign / sc);
    let r3_p = cross3(r1_p, r2_p);
    let r_plane = project_to_so3([[r1_p[0], r2_p[0], r3_p[0]], [r1_p[1], r2_p[1], r3_p[1]], [r1_p[2], r2_p[2], r3_p[2]]]);
    let t_plane = scale3(c2, sign / sc);
    if t_plane[2] <= 1e-4 {
        return None;
    }
    // Convert from plane basis `Q = [e1 | e2 | e3]` back to world: `R_wc = R_plane * Q^T`, `t_wc = t_plane - R_wc * c`.
    let qt = [e1, e2, e3];
    let r_wc = project_to_so3(mat3_mul(r_plane, qt));
    let t_wc = sub3(t_plane, mat3_mul_vec(r_wc, c));
    Some((r_wc, t_wc))
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
