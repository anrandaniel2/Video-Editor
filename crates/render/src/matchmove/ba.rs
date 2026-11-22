//! Stage 5: analytic-Jacobian Levenberg–Marquardt Bundle Adjustment with Huber robust loss
//! (`σ = 2` px, `δ = 1.5` px), `3×3` point Schur complement, partial-pivoted Gaussian elimination,
//! optional shared focal refinement (`±25%`), and weak-frame pose freezing.

use super::SolveError;
use super::init::{CameraPose, SceneModel, normalize_gauge, refine_pnp_pose};
use super::linalg::{Vec3, add3, mat3_mul, mat3_mul_vec, mat3_t_mul_vec, norm3, normalize3, project_to_so3, rodrigues_exp, solve_linear};
use super::tracks::FeatureTrack;

const HUBER_SIGMA_PX: f64 = 2.0;
const HUBER_DELTA_PX: f64 = 1.5;
const MAX_BA_ITERS: usize = 40;
const MAX_DENSE_ACTIVE_CAMS: usize = 80;
const KEYFRAME_BUDGET: usize = 60;
const FOCAL_CLAMP_RATIO: f64 = 0.25;
const INLIER_GATE_PX: f64 = 3.0;

/// Analytic Jacobian of `(r_u, r_v)` wrt camera left-`SE(3)` perturbation `(δw, δt)`, 3D point `δX`, and focal `δf`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ObsJacobian {
    pub(crate) ru: f64,
    pub(crate) rv: f64,
    pub(crate) j_cam_u: [f64; 6],
    pub(crate) j_cam_v: [f64; 6],
    pub(crate) j_pt_u: Vec3,
    pub(crate) j_pt_v: Vec3,
    pub(crate) j_f_u: f64,
    pub(crate) j_f_v: f64,
}

/// Compute exact residuals and analytic Jacobians for one `(camera, point)` observation.
pub(crate) fn observation_jacobian(cam: &CameraPose, xw: Vec3, obs: (f64, f64), (cx, cy, f): (f64, f64, f64), rotation_only: bool) -> Option<ObsJacobian> {
    let pc = if rotation_only { mat3_mul_vec(cam.r_wc, xw) } else { add3(mat3_mul_vec(cam.r_wc, xw), cam.t_wc) };
    if !pc[2].is_finite() || pc[2] <= 1e-4 {
        return None;
    }
    let inv_z = 1.0 / pc[2];
    let xn = pc[0] * inv_z;
    let yn = pc[1] * inv_z;
    let ru = (cx + f * xn) - obs.0;
    let rv = (cy + f * yn) - obs.1;

    let j_cam_u = [-f * xn * yn, f * (1.0 + xn * xn), -f * yn, f * inv_z, 0.0, -f * xn * inv_z];
    let j_cam_v = [-f * (1.0 + yn * yn), f * xn * yn, f * xn, 0.0, f * inv_z, -f * yn * inv_z];

    let gu_p = [f * inv_z, 0.0, -f * xn * inv_z];
    let gv_p = [0.0, f * inv_z, -f * yn * inv_z];
    let j_pt_u = mat3_t_mul_vec(cam.r_wc, gu_p);
    let j_pt_v = mat3_t_mul_vec(cam.r_wc, gv_p);

    Some(ObsJacobian { ru, rv, j_cam_u, j_cam_v, j_pt_u, j_pt_v, j_f_u: xn, j_f_v: yn })
}

struct ActiveLayout {
    anchor_cam: usize,
    cam_slot: Vec<Option<usize>>,
    active_cams: Vec<usize>,
    active_pts: Vec<usize>,
    cam_dof: usize,
    focal_col: Option<usize>,
    total_cam_dim: usize,
}

struct PointBlock {
    v: [[f64; 3]; 3],
    bp: Vec3,
    /// `(cam_param_offset_or_focal, block_rows, rows_data)` — stored as `(col_start, row_count, flat_rows_by_3)`.
    couplings: Vec<(usize, usize, [f64; 18])>,
}

/// Refine camera poses, 3D landmarks, and (optionally) focal length via Levenberg–Marquardt BA.
pub(crate) fn run_bundle_adjustment(model: &mut SceneModel, tracks: &[FeatureTrack], refine_focal: bool, cancel: &dyn Fn() -> bool) -> Result<(), SolveError> {
    let layout = build_active_layout(model, tracks, refine_focal);
    if layout.active_pts.len() < 6 {
        return Err(SolveError::InsufficientMotion);
    }

    let mut lambda = 1e-3f64;
    let mut cur_cost = total_huber_cost(model, tracks, &layout);

    for _iter in 0..MAX_BA_ITERS {
        if cancel() {
            return Err(SolveError::Cancelled);
        }
        let Some((dc, dp)) = solve_ba_step(model, tracks, &layout, lambda) else {
            lambda = (lambda * 10.0).min(1e6);
            continue;
        };
        let step_norm = l2_norm(&dc).max(l2_norm_vec3(&dp));
        let cand = apply_ba_update(model, &layout, &dc, &dp);
        let cand_cost = total_huber_cost(&cand, tracks, &layout);

        if cand_cost < cur_cost {
            let rel_drop = (cur_cost - cand_cost) / cur_cost.max(1e-9);
            *model = cand;
            cur_cost = cand_cost;
            lambda = (lambda / 3.0).max(1e-7);
            if step_norm < 1e-6 || rel_drop < 1e-6 {
                break;
            }
        } else {
            lambda = (lambda * 10.0).min(1e6);
            if lambda >= 1e6 {
                break;
            }
        }
    }

    // If keyframe subsampling was used on a long clip, refine intermediate frames against BA landmarks.
    if layout.active_cams.len() + 1 < model.cameras.iter().filter(|c| c.solved && !c.frozen).count() {
        polish_non_keyframe_poses(model, tracks, &layout, cancel)?;
    }

    if !model.rotation_only {
        normalize_gauge(model, tracks);
    }
    freeze_weak_frames(model, tracks);
    Ok(())
}

fn build_active_layout(model: &SceneModel, tracks: &[FeatureTrack], refine_focal: bool) -> ActiveLayout {
    let anchor_cam = model.cameras.iter().position(|c| c.solved && !c.frozen).unwrap_or(0);
    let solved_indices: Vec<usize> = (0..model.cameras.len()).filter(|&i| i != anchor_cam && model.cameras[i].solved && !model.cameras[i].frozen).collect();

    let selected: Vec<usize> = if solved_indices.len() <= MAX_DENSE_ACTIVE_CAMS {
        solved_indices
    } else {
        let stride = solved_indices.len().div_ceil(KEYFRAME_BUDGET);
        let mut sub: Vec<usize> = solved_indices.iter().step_by(stride).copied().collect();
        if let Some(&last) = solved_indices.last()
            && !sub.contains(&last)
        {
            sub.push(last);
        }
        if model.ref_cam != anchor_cam && model.cameras[model.ref_cam].solved && !sub.contains(&model.ref_cam) {
            sub.push(model.ref_cam);
            sub.sort_unstable();
        }
        sub
    };

    let mut cam_slot = vec![None; model.cameras.len()];
    for (slot, &cam_idx) in selected.iter().enumerate() {
        cam_slot[cam_idx] = Some(slot);
    }

    let mut active_pts = Vec::new();
    for tr in tracks {
        if model.points[tr.id].is_none() {
            continue;
        }
        let obs_count = (0..tr.pts.len())
            .filter(|&off| {
                let f = tr.start_frame + off;
                f == anchor_cam || cam_slot[f].is_some()
            })
            .count();
        if obs_count >= 2 {
            active_pts.push(tr.id);
        }
    }

    let cam_dof = if model.rotation_only { 3 } else { 6 };
    let base_dim = selected.len() * cam_dof;
    let focal_col = refine_focal.then_some(base_dim);
    let total_cam_dim = base_dim + usize::from(refine_focal);

    ActiveLayout { anchor_cam, cam_slot, active_cams: selected, active_pts, cam_dof, focal_col, total_cam_dim }
}

fn total_huber_cost(model: &SceneModel, tracks: &[FeatureTrack], layout: &ActiveLayout) -> f64 {
    let inv_var = 1.0 / (HUBER_SIGMA_PX * HUBER_SIGMA_PX);
    let mut cost = 0.0f64;
    for &tid in &layout.active_pts {
        let Some(xw) = model.points[tid] else { continue };
        let tr = &tracks[tid];
        for (off, &obs) in tr.pts.iter().enumerate() {
            let f = tr.start_frame + off;
            if f != layout.anchor_cam && layout.cam_slot[f].is_none() {
                continue;
            }
            if let Some((u, v)) = model.project_world(&model.cameras[f], xw) {
                let e = (u - obs.0).hypot(v - obs.1);
                cost += inv_var * huber_rho(e);
            } else {
                cost += inv_var * huber_rho(25.0);
            }
        }
    }
    cost
}

fn huber_rho(e: f64) -> f64 {
    if e <= HUBER_DELTA_PX { 0.5 * e * e } else { HUBER_DELTA_PX * (e - 0.5 * HUBER_DELTA_PX) }
}

fn huber_weight(e: f64) -> f64 {
    let inv_var = 1.0 / (HUBER_SIGMA_PX * HUBER_SIGMA_PX);
    if e <= HUBER_DELTA_PX { inv_var } else { inv_var * (HUBER_DELTA_PX / e) }
}

fn solve_ba_step(model: &SceneModel, tracks: &[FeatureTrack], layout: &ActiveLayout, lambda: f64) -> Option<(Vec<f64>, Vec<Vec3>)> {
    let m = layout.total_cam_dim;
    let mut u_mat = vec![0.0f64; m * m];
    let mut bc = vec![0.0f64; m];
    let mut pt_blocks: Vec<PointBlock> = layout.active_pts.iter().map(|_| PointBlock { v: [[0.0; 3]; 3], bp: [0.0; 3], couplings: Vec::new() }).collect();

    accumulate_normal_equations(model, tracks, layout, &mut u_mat, &mut bc, &mut pt_blocks);
    add_scale_and_damping(model, layout, lambda, &mut u_mat, &mut pt_blocks);

    // Eliminate all 3×3 point blocks via Schur complement.
    let mut v_invs = vec![[[0.0f64; 3]; 3]; pt_blocks.len()];
    for (p_idx, pb) in pt_blocks.iter().enumerate() {
        let v_inv = invert_sym3(pb.v)?;
        v_invs[p_idx] = v_inv;
        schur_eliminate_point(pb, v_inv, m, &mut u_mat, &mut bc);
    }

    let dc = if m > 0 { solve_linear(&mut u_mat, &mut bc, m)? } else { Vec::new() };

    let mut dp = vec![[0.0f64; 3]; pt_blocks.len()];
    for (p_idx, pb) in pt_blocks.iter().enumerate() {
        dp[p_idx] = back_substitute_point(pb, v_invs[p_idx], &dc);
    }
    Some((dc, dp))
}

fn accumulate_normal_equations(
    model: &SceneModel,
    tracks: &[FeatureTrack],
    layout: &ActiveLayout,
    u_mat: &mut [f64],
    bc: &mut [f64],
    pt_blocks: &mut [PointBlock],
) {
    let m = layout.total_cam_dim;
    let d = layout.cam_dof;
    let intr = (model.cx, model.cy, model.focal);

    for (p_idx, &tid) in layout.active_pts.iter().enumerate() {
        let Some(xw) = model.points[tid] else { continue };
        let pb = &mut pt_blocks[p_idx];
        let mut focal_w = [0.0f64; 3];
        let mut has_focal = false;

        for (off, &obs) in tracks[tid].pts.iter().enumerate() {
            let f = tracks[tid].start_frame + off;
            let cam_slot = layout.cam_slot[f];
            if f != layout.anchor_cam && cam_slot.is_none() {
                continue;
            }
            let Some(jac) = observation_jacobian(&model.cameras[f], xw, obs, intr, model.rotation_only) else { continue };
            let w = huber_weight(jac.ru.hypot(jac.rv));

            // Point block V_j and bp_j
            for a in 0..3 {
                pb.bp[a] -= w * (jac.j_pt_u[a] * jac.ru + jac.j_pt_v[a] * jac.rv);
                for b in 0..3 {
                    pb.v[a][b] += w * (jac.j_pt_u[a] * jac.j_pt_u[b] + jac.j_pt_v[a] * jac.j_pt_v[b]);
                }
            }

            // Camera block U_{ii}, bc_i, and camera-point W_{ij}
            if let Some(slot) = cam_slot {
                let col0 = slot * d;
                let mut w_flat = [0.0f64; 18];
                for a in 0..d {
                    bc[col0 + a] -= w * (jac.j_cam_u[a] * jac.ru + jac.j_cam_v[a] * jac.rv);
                    for b in 0..d {
                        u_mat[(col0 + a) * m + (col0 + b)] += w * (jac.j_cam_u[a] * jac.j_cam_u[b] + jac.j_cam_v[a] * jac.j_cam_v[b]);
                    }
                    for p in 0..3 {
                        w_flat[a * 3 + p] = w * (jac.j_cam_u[a] * jac.j_pt_u[p] + jac.j_cam_v[a] * jac.j_pt_v[p]);
                    }
                    if let Some(fc) = layout.focal_col {
                        let c_af = w * (jac.j_cam_u[a] * jac.j_f_u + jac.j_cam_v[a] * jac.j_f_v);
                        u_mat[(col0 + a) * m + fc] += c_af;
                        u_mat[fc * m + (col0 + a)] += c_af;
                    }
                }
                pb.couplings.push((col0, d, w_flat));
            }

            // Shared focal length terms
            if let Some(fc) = layout.focal_col {
                has_focal = true;
                bc[fc] -= w * (jac.j_f_u * jac.ru + jac.j_f_v * jac.rv);
                u_mat[fc * m + fc] += w * (jac.j_f_u * jac.j_f_u + jac.j_f_v * jac.j_f_v);
                for p in 0..3 {
                    focal_w[p] += w * (jac.j_f_u * jac.j_pt_u[p] + jac.j_f_v * jac.j_pt_v[p]);
                }
            }
        }

        if has_focal && let Some(fc) = layout.focal_col {
            let mut w_flat = [0.0f64; 18];
            w_flat[0] = focal_w[0];
            w_flat[1] = focal_w[1];
            w_flat[2] = focal_w[2];
            pb.couplings.push((fc, 1, w_flat));
        }
    }
}

fn add_scale_and_damping(model: &SceneModel, layout: &ActiveLayout, lambda: f64, u_mat: &mut [f64], pt_blocks: &mut [PointBlock]) {
    let m = layout.total_cam_dim;
    // Fix the global scale gauge on `ref_cam` along its translation direction without biasing the gradient.
    if !model.rotation_only
        && let Some(slot) = layout.cam_slot.get(model.ref_cam).copied().flatten()
        && let Some(ut) = normalize3(model.cameras[model.ref_cam].t_wc)
    {
        let t_col = slot * layout.cam_dof + 3;
        let weight = 1e4;
        for a in 0..3 {
            for b in 0..3 {
                u_mat[(t_col + a) * m + (t_col + b)] += weight * ut[a] * ut[b];
            }
        }
    }

    for i in 0..m {
        u_mat[i * m + i] = u_mat[i * m + i] * (1.0 + lambda) + 1e-6;
    }

    for (p_idx, pb) in pt_blocks.iter_mut().enumerate() {
        if model.rotation_only
            && let Some(dw) = model.points[layout.active_pts[p_idx]].and_then(normalize3)
        {
            // Radial gauge tether on S^2 so radial direction has unit curvature and zero residual bias.
            for a in 0..3 {
                for b in 0..3 {
                    pb.v[a][b] += dw[a] * dw[b];
                }
            }
        }
        for d in 0..3 {
            pb.v[d][d] = pb.v[d][d] * (1.0 + lambda) + 1e-6;
        }
    }
}

fn schur_eliminate_point(pb: &PointBlock, v_inv: [[f64; 3]; 3], m: usize, u_mat: &mut [f64], bc: &mut [f64]) {
    let vinv_bp = mat3_mul_vec(v_inv, pb.bp);
    for (idx_a, &(col_a, rows_a, ref wa)) in pb.couplings.iter().enumerate() {
        // rhs -= W_a * (V^-1 * bp)
        for ra in 0..rows_a {
            let w_row = [wa[ra * 3], wa[ra * 3 + 1], wa[ra * 3 + 2]];
            bc[col_a + ra] -= w_row[0] * vinv_bp[0] + w_row[1] * vinv_bp[1] + w_row[2] * vinv_bp[2];
        }
        // Pre-multiply W_a * V^-1 (rows_a x 3)
        let mut wa_vinv = [[0.0f64; 3]; 6];
        for ra in 0..rows_a {
            let w_row = [wa[ra * 3], wa[ra * 3 + 1], wa[ra * 3 + 2]];
            wa_vinv[ra] = mat3_mul_vec(v_inv, w_row);
        }
        for &(col_b, rows_b, ref wb) in &pb.couplings[idx_a..] {
            for ra in 0..rows_a {
                for rb in 0..rows_b {
                    let s = wa_vinv[ra][0] * wb[rb * 3] + wa_vinv[ra][1] * wb[rb * 3 + 1] + wa_vinv[ra][2] * wb[rb * 3 + 2];
                    u_mat[(col_a + ra) * m + (col_b + rb)] -= s;
                    if col_a != col_b {
                        u_mat[(col_b + rb) * m + (col_a + ra)] -= s;
                    }
                }
            }
        }
    }
}

fn back_substitute_point(pb: &PointBlock, v_inv: [[f64; 3]; 3], dc: &[f64]) -> Vec3 {
    let mut rhs = pb.bp;
    for &(col, rows, ref w) in &pb.couplings {
        for r in 0..rows {
            let step_r = dc[col + r];
            rhs[0] -= w[r * 3] * step_r;
            rhs[1] -= w[r * 3 + 1] * step_r;
            rhs[2] -= w[r * 3 + 2] * step_r;
        }
    }
    mat3_mul_vec(v_inv, rhs)
}

fn apply_ba_update(model: &SceneModel, layout: &ActiveLayout, dc: &[f64], dp: &[Vec3]) -> SceneModel {
    let mut next = model.clone();
    let d = layout.cam_dof;
    for (slot, &cam_idx) in layout.active_cams.iter().enumerate() {
        let base = slot * d;
        let dw = [dc[base], dc[base + 1], dc[base + 2]];
        let dr = rodrigues_exp(dw);
        next.cameras[cam_idx].r_wc = project_to_so3(mat3_mul(dr, model.cameras[cam_idx].r_wc));
        if !model.rotation_only {
            let dt = [dc[base + 3], dc[base + 4], dc[base + 5]];
            next.cameras[cam_idx].t_wc = add3(mat3_mul_vec(dr, model.cameras[cam_idx].t_wc), dt);
        }
    }
    if let Some(fc) = layout.focal_col {
        let f_min = model.initial_focal * (1.0 - FOCAL_CLAMP_RATIO);
        let f_max = model.initial_focal * (1.0 + FOCAL_CLAMP_RATIO);
        next.focal = (model.focal + dc[fc]).clamp(f_min, f_max);
    }
    for (p_idx, &tid) in layout.active_pts.iter().enumerate() {
        if let Some(xw) = model.points[tid] {
            let updated = add3(xw, dp[p_idx]);
            next.points[tid] = if model.rotation_only { normalize3(updated).or(Some(xw)) } else { Some(updated) };
        }
    }
    next
}

fn polish_non_keyframe_poses(model: &mut SceneModel, tracks: &[FeatureTrack], layout: &ActiveLayout, cancel: &dyn Fn() -> bool) -> Result<(), SolveError> {
    for f in 0..model.cameras.len() {
        if f.is_multiple_of(32) && cancel() {
            return Err(SolveError::Cancelled);
        }
        if f == layout.anchor_cam || layout.cam_slot[f].is_some() || model.cameras[f].frozen || !model.cameras[f].solved {
            continue;
        }
        if model.rotation_only {
            polish_single_rotation(model, tracks, f);
        } else {
            let corrs: Vec<(Vec3, (f64, f64))> = tracks.iter().filter_map(|tr| Some((model.points[tr.id]?, tr.at_frame(f)?))).collect();
            if corrs.len() >= 6 {
                let (r, t) = refine_pnp_pose(model, &corrs, model.cameras[f].r_wc, model.cameras[f].t_wc, 10);
                model.cameras[f].r_wc = r;
                model.cameras[f].t_wc = t;
            }
        }
    }
    Ok(())
}

fn polish_single_rotation(model: &mut SceneModel, tracks: &[FeatureTrack], frame: usize) {
    let mut r = model.cameras[frame].r_wc;
    let intr = (model.cx, model.cy, model.focal);
    for _ in 0..8 {
        let mut h = [[0.0f64; 3]; 3];
        let mut g = [0.0f64; 3];
        for tr in tracks {
            let (Some(dw), Some(obs)) = (model.points[tr.id], tr.at_frame(frame)) else { continue };
            let cam = CameraPose { r_wc: r, t_wc: [0.0; 3], frozen: false, solved: true };
            let Some(jac) = observation_jacobian(&cam, dw, obs, intr, true) else { continue };
            let w = huber_weight(jac.ru.hypot(jac.rv));
            for a in 0..3 {
                g[a] -= w * (jac.j_cam_u[a] * jac.ru + jac.j_cam_v[a] * jac.rv);
                for b in 0..3 {
                    h[a][b] += w * (jac.j_cam_u[a] * jac.j_cam_u[b] + jac.j_cam_v[a] * jac.j_cam_v[b]);
                }
            }
        }
        for d in 0..3 {
            h[d][d] += 1e-4;
        }
        let Some(h_inv) = invert_sym3(h) else { break };
        let step = mat3_mul_vec(h_inv, g);
        r = project_to_so3(mat3_mul(rodrigues_exp(step), r));
        if norm3(step) < 1e-6 {
            break;
        }
    }
    model.cameras[frame].r_wc = r;
}

fn freeze_weak_frames(model: &mut SceneModel, tracks: &[FeatureTrack]) {
    let total_3d = model.points.iter().flatten().count();
    let min_inliers = 12.min(total_3d);
    for f in 0..model.cameras.len() {
        let inliers = tracks
            .iter()
            .filter(|tr| {
                let (Some(xw), Some(obs)) = (model.points[tr.id], tr.at_frame(f)) else { return false };
                model.project_world(&model.cameras[f], xw).is_some_and(|(u, v)| (u - obs.0).hypot(v - obs.1) <= INLIER_GATE_PX)
            })
            .count();
        if inliers < min_inliers {
            model.cameras[f].frozen = true;
            model.cameras[f].solved = false;
        }
    }

    let Some(first_good) = model.cameras.iter().find(|c| !c.frozen && c.solved).copied() else { return };
    let mut carry = first_good;
    for cam in &mut model.cameras {
        if !cam.frozen && cam.solved {
            carry = *cam;
        } else {
            cam.r_wc = carry.r_wc;
            cam.t_wc = carry.t_wc;
            cam.frozen = true;
        }
    }
}

fn invert_sym3(m: [[f64; 3]; 3]) -> Option<[[f64; 3]; 3]> {
    super::linalg::mat3_inv(m)
}

fn l2_norm(v: &[f64]) -> f64 {
    v.iter().map(|x| x * x).sum::<f64>().sqrt()
}

fn l2_norm_vec3(v: &[Vec3]) -> f64 {
    v.iter().map(|p| p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sum::<f64>().sqrt()
}
