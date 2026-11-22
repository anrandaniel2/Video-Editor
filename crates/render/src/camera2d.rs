//! Global 2D camera-space solver for flat / 2D / planar footage (anime, cartoons, 2D games,
//! screen recordings, locked-off pans & zooms, and zero-parallax shots).
//!
//! Unlike [`crate::track3d::Tracker`], which tracks a single small patch around a local feature
//! (and therefore latches onto deforming foreground characters or clamps to `[0, 1]`),
//! [`solve_2d_camera_space`] solves the **dominant full-frame 2D background camera motion** across
//! an `8 × 6` spatial grid using GPU-accelerated pyramidal IC-LK, coarse ZNCC whip-pan recovery,
//! hierarchical multi-model RANSAC (stationary hold -> 2-DOF pan -> 3-DOF pan+zoom -> 4-DOF
//! similarity) weighted by spatial cell coverage, and direct world-space keyframe refinement.
//!
//! Foreground characters, UI overlays, and animated sprites confined to part of the frame are
//! rejected as RANSAC outliers, and the solved 2D world origin `(x, y, scale, rotation)` is
//! emitted **unclamped** so placed 3D objects smoothly scroll off-screen and return to the exact
//! same background spot when the camera pans back.

use filmcraft_project::{StereoLayout, TrackSample};
use filmcraft_time::Tick;

use crate::scene::SceneDetector;
use crate::track3d::{
    GpuTrackBackend, GpuTrackQuery, Gray, Level, corners, crop_gray, gpu_backend, search, smooth_track_samples, track_point, view_rect,
};

/// Borrowed RGBA8 frame passed into [`solve_2d_camera_space`].
#[derive(Clone, Copy, Debug)]
pub struct Frame2D<'a> {
    pub time: Tick,
    pub width: usize,
    pub height: usize,
    pub rgba: &'a [u8],
}

const MAX_WORK_SIDE: usize = 512;
const GRID_COLS: usize = 8;
const GRID_ROWS: usize = 6;
const TARGET_FEATURES: usize = 192;
const PER_CELL_CAP: usize = 4;
const MIN_SEPARATION_PX: f32 = 9.0;
const BORDER_PX: usize = 10;
const LK_PATCH_RADIUS: i32 = 5;
const FB_MAX_PX: f32 = 1.65;

/// Configuration for [`solve_2d_camera_space`].
#[derive(Clone, Debug)]
pub struct Camera2DOptions {
    pub stereo: StereoLayout,
    pub playhead: Tick,
    pub cut_sensitivity: f32,
}

impl Default for Camera2DOptions {
    fn default() -> Self {
        Self {
            stereo: StereoLayout::Mono,
            playhead: Tick(0),
            cut_sensitivity: 0.5,
        }
    }
}

/// Output of [`solve_2d_camera_space`].
#[derive(Clone, Debug)]
pub struct Camera2DSolve {
    pub samples: Vec<TrackSample>,
    pub shots_skipped: usize,
    pub mean_inlier_ratio: f32,
}

/// Failure modes of [`solve_2d_camera_space`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Camera2DError {
    Cancelled,
    TooShort,
    Decode(String),
}

#[derive(Clone, Copy, Debug)]
struct StepTransform {
    /// `a = scale * cos(dtheta)`
    a: f64,
    /// `b = scale * sin(dtheta)`
    b: f64,
    tx: f64,
    ty: f64,
    has_zoom: bool,
    has_rot: bool,
    confidence: f32,
}

impl StepTransform {
    fn identity(confidence: f32) -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            tx: 0.0,
            ty: 0.0,
            has_zoom: false,
            has_rot: false,
            confidence,
        }
    }
}

#[derive(Clone, Debug)]
struct FrameObservation {
    time: Tick,
    /// `(track_id, prev_px, prev_py, curr_px, curr_py)` in view-pixel coordinates.
    matches: Vec<(usize, f32, f32, f32, f32)>,
    /// Newly or currently alive points `(track_id, px, py)` at this frame.
    alive: Vec<(usize, f32, f32)>,
    survival: f32,
    scene_diff: f32,
}

#[derive(Clone, Copy, Debug)]
struct LivePoint {
    id: usize,
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
}

fn pyramid_levels_for(w: usize, h: usize) -> usize {
    let m = w.min(h);
    if m >= 320 {
        4
    } else if m >= 160 {
        3
    } else {
        2
    }
}

fn box_downsample_rgba(src: &[u8], sw: usize, sh: usize, dw: usize, dh: usize) -> Vec<u8> {
    let mut out = vec![0u8; dw * dh * 4];
    for dy in 0..dh {
        let y0 = (dy * sh) / dh;
        let y1 = (((dy + 1) * sh) / dh).max(y0 + 1).min(sh);
        for dx in 0..dw {
            let x0 = (dx * sw) / dw;
            let x1 = (((dx + 1) * sw) / dw).max(x0 + 1).min(sw);
            let (mut r, mut g, mut b, mut a, mut n) = (0u32, 0u32, 0u32, 0u32, 0u32);
            for sy in y0..y1 {
                let row = sy * sw * 4;
                for sx in x0..x1 {
                    let i = row + sx * 4;
                    if let Some(px) = src.get(i..i + 4) {
                        r += u32::from(px[0]);
                        g += u32::from(px[1]);
                        b += u32::from(px[2]);
                        a += u32::from(px[3]);
                        n += 1;
                    }
                }
            }
            let o = (dy * dw + dx) * 4;
            if n > 0
                && let Some(dst) = out.get_mut(o..o + 4)
            {
                dst[0] = (r / n) as u8;
                dst[1] = (g / n) as u8;
                dst[2] = (b / n) as u8;
                dst[3] = (a / n) as u8;
            }
        }
    }
    out
}

fn conform_rgba<'a>(rgba: &'a [u8], w: usize, h: usize, stereo: StereoLayout) -> (usize, usize, std::borrow::Cow<'a, [u8]>) {
    let max_side = match stereo {
        StereoLayout::Mono => MAX_WORK_SIDE,
        StereoLayout::SideBySide | StereoLayout::TopBottom => MAX_WORK_SIDE * 2,
    };
    let long = w.max(h);
    if long <= max_side {
        return (w, h, std::borrow::Cow::Borrowed(rgba));
    }
    let scale = max_side as f64 / long as f64;
    let dw = ((w as f64 * scale).round() as usize).max(32) & !1;
    let dh = ((h as f64 * scale).round() as usize).max(32) & !1;
    (dw, dh, std::borrow::Cow::Owned(box_downsample_rgba(rgba, w, h, dw, dh)))
}

fn grid_cell_index(x: f32, y: f32, vw: usize, vh: usize) -> usize {
    let gx = ((x.max(0.0) as usize) * GRID_COLS / vw.max(1)).min(GRID_COLS - 1);
    let gy = ((y.max(0.0) as usize) * GRID_ROWS / vh.max(1)).min(GRID_ROWS - 1);
    gy * GRID_COLS + gx
}

fn count_cells_mask(mask: u64) -> usize {
    mask.count_ones() as usize
}

fn seed_grid(
    view: &Gray,
    gpu: Option<(&dyn GpuTrackBackend, usize)>,
    live: &mut Vec<LivePoint>,
    next_id: &mut usize,
) {
    let (vw, vh) = (view.w, view.h);
    let min_sep2 = MIN_SEPARATION_PX * MIN_SEPARATION_PX;
    let border = BORDER_PX.min(vw / 10).min(vh / 10);
    let mut cell_counts = [0usize; GRID_COLS * GRID_ROWS];
    for p in live.iter() {
        cell_counts[grid_cell_index(p.x, p.y, vw, vh)] += 1;
    }

    if let Some((g, slot)) = gpu
        && vw > border * 2
        && vh > border * 2
        && let Some(mut cands) = g.detect_corners(slot, (border, border, vw - border, vh - border), 8)
    {
        cands.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        for (_, px, py) in cands {
            if live.len() >= TARGET_FEATURES {
                break;
            }
            if (px as usize) < border
                || (py as usize) < border
                || (px as usize) + border >= vw
                || (py as usize) + border >= vh
            {
                continue;
            }
            let cell = grid_cell_index(px, py, vw, vh);
            if cell_counts[cell] >= PER_CELL_CAP {
                continue;
            }
            if live.iter().any(|q| {
                let dx = q.x - px;
                let dy = q.y - py;
                dx * dx + dy * dy < min_sep2
            }) {
                continue;
            }
            let id = *next_id;
            *next_id += 1;
            cell_counts[cell] += 1;
            live.push(LivePoint { id, x: px, y: py, vx: 0.0, vy: 0.0 });
        }
        return;
    }

    let cell_w = vw as f32 / GRID_COLS as f32;
    let cell_h = vh as f32 / GRID_ROWS as f32;
    let reach = 0.62 * cell_w.hypot(cell_h);
    for gy in 0..GRID_ROWS {
        for gx in 0..GRID_COLS {
            if live.len() >= TARGET_FEATURES {
                return;
            }
            let cell = gy * GRID_COLS + gx;
            if cell_counts[cell] >= PER_CELL_CAP {
                continue;
            }
            let cx = (gx as f32 + 0.5) * cell_w;
            let cy = (gy as f32 + 0.5) * cell_h;
            for (px, py) in corners(view, cx, cy, reach, MIN_SEPARATION_PX, PER_CELL_CAP * 2) {
                if live.len() >= TARGET_FEATURES || cell_counts[cell] >= PER_CELL_CAP {
                    break;
                }
                if (px as usize) < border
                    || (py as usize) < border
                    || (px as usize) + border >= vw
                    || (py as usize) + border >= vh
                {
                    continue;
                }
                if live.iter().any(|q| {
                    let dx = q.x - px;
                    let dy = q.y - py;
                    dx * dx + dy * dy < min_sep2
                }) {
                    continue;
                }
                let id = *next_id;
                *next_id += 1;
                cell_counts[cell] += 1;
                live.push(LivePoint { id, x: px, y: py, vx: 0.0, vy: 0.0 });
            }
        }
    }
}

fn track_single_cpu(prev_pyr: &[Level], curr_pyr: &[Level], p: (f32, f32), guess: (f32, f32)) -> Option<(f32, f32)> {
    let fwd = track_point(prev_pyr, curr_pyr, p, LK_PATCH_RADIUS, guess)?;
    let back_guess = (p.0 - fwd.0, p.1 - fwd.1);
    let bwd = track_point(curr_pyr, prev_pyr, fwd, LK_PATCH_RADIUS, back_guess)?;
    if (bwd.0 - p.0).hypot(bwd.1 - p.1) <= FB_MAX_PX {
        Some(fwd)
    } else {
        None
    }
}

/// Estimate a coarse global shift `(dx, dy)` via ZNCC on up to 9 spread anchor points when fast
/// whip-pans exceed the normal pyramidal IC-LK basin.
fn estimate_whip_pan_guess(
    prev_pyr: Option<&[Level]>,
    curr_pyr: Option<&[Level]>,
    gpu: Option<(&dyn GpuTrackBackend, usize, usize)>,
    vw: usize,
    vh: usize,
    live: &[LivePoint],
) -> Option<(f32, f32)> {
    if live.len() < 4 {
        return None;
    }
    let reach = (vw.max(vh) as f32 * 0.24).clamp(28.0, 128.0);
    let step = (live.len() / 9).max(1);
    let mut shifts: Vec<(f32, f32)> = Vec::with_capacity(9);
    for p in live.iter().step_by(step).take(9) {
        let found = if let Some((g, prev_slot, curr_slot)) = gpu {
            g.coarse_search(prev_slot, curr_slot, (p.x, p.y), 3, reach)
        } else if let (Some(pp), Some(cp)) = (prev_pyr, curr_pyr) {
            search(pp, cp, (p.x, p.y), 3, reach)
        } else {
            None
        };
        if let Some(d) = found {
            shifts.push(d);
        }
    }
    if shifts.len() < 3 {
        return None;
    }
    let mut best_inliers = 0usize;
    let mut best = (0.0f32, 0.0f32);
    for &(dx, dy) in &shifts {
        let cnt = shifts
            .iter()
            .filter(|&&(ox, oy)| (ox - dx).hypot(oy - dy) <= 8.0)
            .count();
        if cnt > best_inliers {
            best_inliers = cnt;
            best = (dx, dy);
        }
    }
    (best_inliers >= 3).then_some(best)
}

fn median_f64(vals: &mut [f64]) -> f64 {
    if vals.is_empty() {
        return 0.0;
    }
    vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = vals.len() / 2;
    if vals.len() % 2 == 1 {
        vals[mid]
    } else {
        0.5 * (vals[mid - 1] + vals[mid])
    }
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Huber-IRLS refinement of pure 2D translation `(tx, ty)`.
fn refine_translation(pts: &[(f64, f64, f64, f64, usize)], init_tx: f64, init_ty: f64, huber: f64) -> (f64, f64) {
    let (mut tx, mut ty) = (init_tx, init_ty);
    for _ in 0..5 {
        let (mut ws, mut sx, mut sy) = (0.0, 0.0, 0.0);
        for &(x1, y1, x2, y2, _) in pts {
            let rx = (x2 - x1) - tx;
            let ry = (y2 - y1) - ty;
            let r = rx.hypot(ry);
            if r > huber * 3.5 {
                continue;
            }
            let w = if r <= huber { 1.0 } else { huber / r };
            ws += w;
            sx += w * (x2 - x1);
            sy += w * (y2 - y1);
        }
        if ws < 1e-6 {
            break;
        }
        tx = sx / ws;
        ty = sy / ws;
    }
    (tx, ty)
}

/// Huber-IRLS refinement of 3-DOF isotropic scale + pan `(s, tx, ty)` (`dtheta = 0`).
fn refine_scale_pan(pts: &[(f64, f64, f64, f64, usize)], init_s: f64, init_tx: f64, init_ty: f64, huber: f64) -> (f64, f64, f64) {
    let (mut s, mut tx, mut ty) = (init_s, init_tx, init_ty);
    for _ in 0..6 {
        let (mut ws, mut fx, mut fy, mut gx, mut gy) = (0.0, 0.0, 0.0, 0.0, 0.0);
        let mut weights = vec![0.0; pts.len()];
        for (i, &(x1, y1, x2, y2, _)) in pts.iter().enumerate() {
            let rx = x2 - (s * x1 + tx);
            let ry = y2 - (s * y1 + ty);
            let r = rx.hypot(ry);
            if r > huber * 3.2 {
                continue;
            }
            let w = if r <= huber { 1.0 } else { huber / r };
            weights[i] = w;
            ws += w;
            fx += w * x1;
            fy += w * y1;
            gx += w * x2;
            gy += w * y2;
        }
        if ws < 2.0 {
            break;
        }
        fx /= ws;
        fy /= ws;
        gx /= ws;
        gy /= ws;
        let (mut num, mut den) = (0.0, 0.0);
        for (i, &(x1, y1, x2, y2, _)) in pts.iter().enumerate() {
            let w = weights[i];
            if w <= 0.0 {
                continue;
            }
            let (ux, uy) = (x1 - fx, y1 - fy);
            let (vx, vy) = (x2 - gx, y2 - gy);
            num += w * (ux * vx + uy * vy);
            den += w * (ux * ux + uy * uy);
        }
        if den > 400.0 {
            s = (num / den).clamp(0.75, 1.33);
        }
        tx = gx - s * fx;
        ty = gy - s * fy;
    }
    (s, tx, ty)
}

/// Huber-IRLS refinement of 4-DOF 2D similarity `(a, b, tx, ty)`.
fn refine_similarity_4dof(
    pts: &[(f64, f64, f64, f64, usize)],
    init_a: f64,
    init_b: f64,
    init_tx: f64,
    init_ty: f64,
    huber: f64,
) -> (f64, f64, f64, f64) {
    let (mut a, mut b, mut tx, mut ty) = (init_a, init_b, init_tx, init_ty);
    for _ in 0..6 {
        let (mut ws, mut fx, mut fy, mut gx, mut gy) = (0.0, 0.0, 0.0, 0.0, 0.0);
        let mut weights = vec![0.0; pts.len()];
        for (i, &(x1, y1, x2, y2, _)) in pts.iter().enumerate() {
            let rx = x2 - (a * x1 - b * y1 + tx);
            let ry = y2 - (b * x1 + a * y1 + ty);
            let r = rx.hypot(ry);
            if r > huber * 3.2 {
                continue;
            }
            let w = if r <= huber { 1.0 } else { huber / r };
            weights[i] = w;
            ws += w;
            fx += w * x1;
            fy += w * y1;
            gx += w * x2;
            gy += w * y2;
        }
        if ws < 2.0 {
            break;
        }
        fx /= ws;
        fy /= ws;
        gx /= ws;
        gy /= ws;
        let (mut num_a, mut num_b, mut den) = (0.0, 0.0, 0.0);
        for (i, &(x1, y1, x2, y2, _)) in pts.iter().enumerate() {
            let w = weights[i];
            if w <= 0.0 {
                continue;
            }
            let (ux, uy) = (x1 - fx, y1 - fy);
            let (vx, vy) = (x2 - gx, y2 - gy);
            num_a += w * (ux * vx + uy * vy);
            num_b += w * (ux * vy - uy * vx);
            den += w * (ux * ux + uy * uy);
        }
        if den > 600.0 {
            let na = num_a / den;
            let nb = num_b / den;
            let sc = na.hypot(nb);
            if (0.75..=1.33).contains(&sc) {
                a = na;
                b = nb;
            }
        }
        tx = gx - (a * fx - b * fy);
        ty = gy - (b * fx + a * fy);
    }
    (a, b, tx, ty)
}

fn score_model(
    pts: &[(f64, f64, f64, f64, usize)],
    a: f64,
    b: f64,
    tx: f64,
    ty: f64,
    tol: f64,
) -> (usize, usize, f64, Vec<bool>) {
    let mut inliers = 0usize;
    let mut cell_mask = 0u64;
    let mut errs = Vec::with_capacity(pts.len());
    let mut mask = vec![false; pts.len()];
    for (i, &(x1, y1, x2, y2, cell)) in pts.iter().enumerate() {
        let rx = x2 - (a * x1 - b * y1 + tx);
        let ry = y2 - (b * x1 + a * y1 + ty);
        let r = rx.hypot(ry);
        if r <= tol {
            inliers += 1;
            cell_mask |= 1u64 << (cell.min(63));
            errs.push(r);
            mask[i] = true;
        }
    }
    let med = if errs.is_empty() { 999.0 } else { median_f64(&mut errs) };
    (inliers, count_cells_mask(cell_mask), med, mask)
}

/// Multi-model RANSAC step estimator for dominant 2D background camera motion:
/// 0. Stationary lock (anime on 2s/3s or static tripod with moving characters -> exact identity)
/// 1. 2-DOF pure pan `(tx, ty)`
/// 2. 3-DOF pan + isotropic zoom `(s, tx, ty)`
/// 3. 4-DOF similarity `(s, dtheta, tx, ty)`
fn estimate_2d_camera_step(
    matches: &[(usize, f32, f32, f32, f32)],
    vw: usize,
    vh: usize,
    rng: &mut u64,
) -> (StepTransform, Vec<usize>) {
    let n = matches.len();
    if n < 3 {
        if n == 0 {
            return (StepTransform::identity(0.0), Vec::new());
        }
        let mut dx = 0.0f64;
        let mut dy = 0.0f64;
        let mut ids = Vec::with_capacity(n);
        for &(id, x1, y1, x2, y2) in matches {
            dx += f64::from(x2 - x1);
            dy += f64::from(y2 - y1);
            ids.push(id);
        }
        return (
            StepTransform {
                a: 1.0,
                b: 0.0,
                tx: dx / n as f64,
                ty: dy / n as f64,
                has_zoom: false,
                has_rot: false,
                confidence: 0.35,
            },
            ids,
        );
    }

    let cx = vw as f64 * 0.5;
    let cy = vh as f64 * 0.5;
    let tol = (vw.max(vh) as f64 * 0.0038).clamp(1.35, 2.25);

    let mut pts: Vec<(f64, f64, f64, f64, usize)> = Vec::with_capacity(n);
    let mut static_count = 0usize;
    let mut static_cells = 0u64;
    for &(_, x1, y1, x2, y2) in matches {
        let cell = grid_cell_index(x1, y1, vw, vh);
        let (u1, v1) = (f64::from(x1) - cx, f64::from(y1) - cy);
        let (u2, v2) = (f64::from(x2) - cx, f64::from(y2) - cy);
        if (u2 - u1).hypot(v2 - v1) <= 0.22 {
            static_count += 1;
            static_cells |= 1u64 << cell.min(63);
        }
        pts.push((u1, v1, u2, v2, cell));
    }

    // 0. Stationary background lock: if >= 38% of features spanning >= 4 grid cells haven't moved,
    // the 2D camera is locked (even if a large character is moving in the foreground!).
    if static_count >= 10 && static_count * 100 >= n * 38 && count_cells_mask(static_cells) >= 4 {
        let inlier_ids = matches
            .iter()
            .filter(|&&(_, x1, y1, x2, y2)| (x2 - x1).hypot(y2 - y1) <= tol as f32)
            .map(|m| m.0)
            .collect::<Vec<_>>();
        let conf = (inlier_ids.len() as f32 / n as f32).clamp(0.4, 1.0);
        return (StepTransform::identity(conf), inlier_ids);
    }

    // 1. Model A: 1-point RANSAC pure 2D translation `(tx, ty)`, scored by inliers + spatial cell spread!
    let mut best_pan_score = 0usize;
    let mut init_tx = 0.0f64;
    let mut init_ty = 0.0f64;
    for &(x1, y1, x2, y2, _) in &pts {
        let (cand_tx, cand_ty) = (x2 - x1, y2 - y1);
        let mut inl = 0usize;
        let mut cells = 0u64;
        for &(ox1, oy1, ox2, oy2, ocell) in &pts {
            if ((ox2 - ox1) - cand_tx).hypot((oy2 - oy1) - cand_ty) <= tol {
                inl += 1;
                cells |= 1u64 << ocell.min(63);
            }
        }
        let score = inl * 10 + count_cells_mask(cells) * 7;
        if score > best_pan_score {
            best_pan_score = score;
            init_tx = cand_tx;
            init_ty = cand_ty;
        }
    }
    let (pan_tx, pan_ty) = refine_translation(&pts, init_tx, init_ty, tol * 0.85);
    let (pan_inl, pan_cells, pan_med, pan_mask) = score_model(&pts, 1.0, 0.0, pan_tx, pan_ty, tol);

    let mut chosen_a = 1.0;
    let mut chosen_b = 0.0;
    let mut chosen_tx = pan_tx;
    let mut chosen_ty = pan_ty;
    let mut chosen_inl = pan_inl;
    let mut chosen_cells = pan_cells;
    let mut chosen_med = pan_med;
    let mut chosen_mask = pan_mask;
    let mut has_zoom = false;
    let mut has_rot = false;

    // 2. Model B: 2-point RANSAC for 3-DOF pan + isotropic zoom `(s, tx, ty)` (`b = 0`).
    if n >= 6 {
        let mut best_zp_score = 0usize;
        let mut zp_init = (1.0f64, pan_tx, pan_ty);
        for _ in 0..260 {
            let i = (splitmix64(rng) as usize) % n;
            let j = (splitmix64(rng) as usize) % n;
            if i == j {
                continue;
            }
            let (x1a, y1a, x2a, y2a, _) = pts[i];
            let (x1b, y1b, x2b, y2b, _) = pts[j];
            let (du1, dv1) = (x1b - x1a, y1b - y1a);
            let den = du1 * du1 + dv1 * dv1;
            if den < 900.0 {
                continue;
            }
            let (du2, dv2) = (x2b - x2a, y2b - y2a);
            let s = (du1 * du2 + dv1 * dv2) / den;
            if !(0.80..=1.25).contains(&s) {
                continue;
            }
            let tx = x2a - s * x1a;
            let ty = y2a - s * y1a;
            let mut inl = 0usize;
            let mut cells = 0u64;
            for &(ox1, oy1, ox2, oy2, ocell) in &pts {
                if (ox2 - (s * ox1 + tx)).hypot(oy2 - (s * oy1 + ty)) <= tol {
                    inl += 1;
                    cells |= 1u64 << ocell.min(63);
                }
            }
            let score = inl * 10 + count_cells_mask(cells) * 7;
            if score > best_zp_score {
                best_zp_score = score;
                zp_init = (s, tx, ty);
            }
        }
        let (zp_s, zp_tx, zp_ty) = refine_scale_pan(&pts, zp_init.0, zp_init.1, zp_init.2, tol * 0.85);
        let (zp_inl, zp_cells, zp_med, zp_mask) = score_model(&pts, zp_s, 0.0, zp_tx, zp_ty, tol);
        let zoom_meaningful = (zp_s - 1.0).abs() >= 0.0022 && zp_cells >= 3;
        let zoom_wins = zp_inl >= chosen_inl + 3
            || (zp_inl + 1 >= chosen_inl && zp_med + 0.14 < chosen_med);
        if zoom_meaningful && zoom_wins {
            chosen_a = zp_s;
            chosen_b = 0.0;
            chosen_tx = zp_tx;
            chosen_ty = zp_ty;
            chosen_inl = zp_inl;
            chosen_cells = zp_cells;
            chosen_med = zp_med;
            chosen_mask = zp_mask;
            has_zoom = true;
        }
    }

    // 3. Model C: 2-point RANSAC for 4-DOF similarity `(a, b, tx, ty)`.
    if n >= 8 {
        let mut best_sim_score = 0usize;
        let mut sim_init = (chosen_a, chosen_b, chosen_tx, chosen_ty);
        for _ in 0..240 {
            let i = (splitmix64(rng) as usize) % n;
            let j = (splitmix64(rng) as usize) % n;
            if i == j {
                continue;
            }
            let (x1a, y1a, x2a, y2a, _) = pts[i];
            let (x1b, y1b, x2b, y2b, _) = pts[j];
            let (du1, dv1) = (x1b - x1a, y1b - y1a);
            let den = du1 * du1 + dv1 * dv1;
            if den < 1200.0 {
                continue;
            }
            let (du2, dv2) = (x2b - x2a, y2b - y2a);
            let a = (du1 * du2 + dv1 * dv2) / den;
            let b = (du1 * dv2 - dv1 * du2) / den;
            let sc = a.hypot(b);
            let dtheta_deg = b.atan2(a).to_degrees().abs();
            if !(0.80..=1.25).contains(&sc) || dtheta_deg > 18.0 {
                continue;
            }
            let tx = x2a - (a * x1a - b * y1a);
            let ty = y2a - (b * x1a + a * y1a);
            let mut inl = 0usize;
            let mut cells = 0u64;
            for &(ox1, oy1, ox2, oy2, ocell) in &pts {
                if (ox2 - (a * ox1 - b * oy1 + tx)).hypot(oy2 - (b * ox1 + a * oy1 + ty)) <= tol {
                    inl += 1;
                    cells |= 1u64 << ocell.min(63);
                }
            }
            let score = inl * 10 + count_cells_mask(cells) * 7;
            if score > best_sim_score {
                best_sim_score = score;
                sim_init = (a, b, tx, ty);
            }
        }
        let (sa, sb, stx, sty) = refine_similarity_4dof(&pts, sim_init.0, sim_init.1, sim_init.2, sim_init.3, tol * 0.85);
        let (s_inl, s_cells, s_med, s_mask) = score_model(&pts, sa, sb, stx, sty, tol);
        let dtheta_deg = sb.atan2(sa).to_degrees().abs();
        let rot_meaningful = dtheta_deg >= 0.08 && s_cells >= 4;
        let rot_wins = s_inl >= chosen_inl + 4
            || (s_inl + 1 >= chosen_inl && s_cells >= chosen_cells && s_med + 0.20 < chosen_med);
        if rot_meaningful && rot_wins {
            chosen_a = sa;
            chosen_b = sb;
            chosen_tx = stx;
            chosen_ty = sty;
            chosen_inl = s_inl;
            chosen_mask = s_mask;
            has_rot = true;
            if (sa.hypot(sb) - 1.0).abs() >= 0.0022 {
                has_zoom = true;
            }
        }
    }

    let inlier_ids = matches
        .iter()
        .enumerate()
        .filter_map(|(i, m)| (*chosen_mask.get(i).unwrap_or(&false)).then_some(m.0))
        .collect::<Vec<_>>();
    let conf = (chosen_inl as f32 / n.max(1) as f32).clamp(0.15, 1.0);
    (
        StepTransform {
            a: chosen_a,
            b: chosen_b,
            tx: chosen_tx,
            ty: chosen_ty,
            has_zoom,
            has_rot,
            confidence: conf,
        },
        inlier_ids,
    )
}

fn isolate_shot_around_playhead(obs: &[FrameObservation], playhead: Tick, cut_sensitivity: f32) -> (usize, usize, usize) {
    let n = obs.len();
    if n <= 12 {
        return (0, n, 0);
    }
    let sens = cut_sensitivity.clamp(0.0, 1.0);
    let cut_thr = 0.42 - 0.24 * sens;
    let mut cuts = Vec::new();
    let mut last_cut = 0usize;
    for (idx, f) in obs.iter().enumerate().skip(1) {
        let hard_cut = (f.scene_diff >= cut_thr && f.survival < 0.42)
            || (f.scene_diff >= cut_thr * 1.45 && f.survival < 0.62)
            || (idx >= 2 && f.survival < 0.08 && f.scene_diff >= cut_thr * 0.72);
        if hard_cut && idx.saturating_sub(last_cut) >= 6 {
            cuts.push(idx);
            last_cut = idx;
        }
    }
    if cuts.is_empty() {
        return (0, n, 0);
    }
    let playhead_idx = obs
        .iter()
        .enumerate()
        .min_by_key(|(_, f)| (f.time.0 - playhead.0).abs())
        .map(|(i, _)| i)
        .unwrap_or(0);
    let mut boundaries = Vec::with_capacity(cuts.len() + 2);
    boundaries.push(0usize);
    boundaries.extend_from_slice(&cuts);
    boundaries.push(n);
    for w in boundaries.windows(2) {
        let (s, e) = (w[0], w[1]);
        if playhead_idx >= s && playhead_idx < e {
            let skipped = boundaries.windows(2).count().saturating_sub(1);
            return (s, e, skipped);
        }
    }
    (0, n, cuts.len())
}

/// Solve global 2D background camera motion over streamed RGBA frames.
pub fn solve_2d_camera_space<F>(options: &Camera2DOptions, mut for_each_frame: F) -> Result<Camera2DSolve, Camera2DError>
where
    F: FnMut(&mut dyn FnMut(Frame2D<'_>) -> Result<(), Camera2DError>) -> Result<(), Camera2DError>,
{
    let gpu = gpu_backend();
    let mut detector = SceneDetector::new();
    let mut observations: Vec<FrameObservation> = Vec::new();
    let mut live: Vec<LivePoint> = Vec::new();
    let mut prev_pyr: Option<Vec<Level>> = None;
    let mut gpu_prev_slot: Option<usize> = None;
    let mut next_id = 0usize;
    let mut view_geom: Option<(usize, usize, usize, usize, usize, usize)> = None;
    let mut levels = 3usize;

    let mut frame_idx = 0usize;
    for_each_frame(&mut |input: Frame2D<'_>| {
        if filmcraft_media::cancel::cancelled() {
            return Err(Camera2DError::Cancelled);
        }
        if input.width < 16 || input.height < 16 || input.rgba.len() < input.width * input.height * 4 {
            return Err(Camera2DError::Decode("invalid frame dimensions".into()));
        }
        let (cw, ch, conformed) = conform_rgba(input.rgba, input.width, input.height, options.stereo);
        let (vx, vy, vw, vh) = view_rect(options.stereo, cw, ch);
        if view_geom.is_none() {
            levels = pyramid_levels_for(vw, vh);
            view_geom = Some((cw, ch, vx, vy, vw, vh));
        }
        let (_, _, vx, vy, vw, vh) = view_geom.unwrap_or((cw, ch, vx, vy, vw, vh));
        let scene_diff = detector.push_rgba(&conformed, cw, ch);
        let curr_slot = frame_idx % 2;

        let gpu_view = gpu.and_then(|g| {
            g.build_rgba_pyramid(curr_slot, &conformed, cw, ch, (vx, vy, vw, vh), (vw, vh), levels)
        });
        let gpu_ready = gpu_view.is_some();
        let view = match gpu_view {
            Some(gv) => gv,
            None => {
                let full_gray = Gray::from_rgba8(&conformed, cw, ch);
                crop_gray(&full_gray, vx, vy, vw, vh)
            }
        };
        let curr_pyr = if gpu_ready { None } else { Some(view.pyramid(levels)) };

        let mut matches = Vec::with_capacity(live.len());
        let prev_count = live.len();

        if frame_idx > 0 && !live.is_empty() {
            let mut survivors = Vec::with_capacity(live.len());
            if gpu_ready
                && let (Some(g), Some(prev_slot)) = (gpu, gpu_prev_slot)
            {
                let queries: Vec<GpuTrackQuery> = live
                    .iter()
                    .map(|p| GpuTrackQuery {
                        pt: (p.x, p.y),
                        guess: (p.vx, p.vy),
                    })
                    .collect();
                if let Some(res) = g.track_batch_fb(prev_slot, curr_slot, &queries, LK_PATCH_RADIUS, FB_MAX_PX) {
                    let mut failed_idx = Vec::new();
                    for (i, r) in res.into_iter().enumerate() {
                        let p = live[i];
                        if let Some((nx, ny)) = r {
                            matches.push((p.id, p.x, p.y, nx, ny));
                            survivors.push(LivePoint {
                                id: p.id,
                                x: nx,
                                y: ny,
                                vx: nx - p.x,
                                vy: ny - p.y,
                            });
                        } else if p.vx.hypot(p.vy) > 0.75 {
                            failed_idx.push(i);
                        }
                    }
                    if !failed_idx.is_empty() {
                        let retry_q: Vec<GpuTrackQuery> = failed_idx
                            .iter()
                            .map(|&i| GpuTrackQuery {
                                pt: (live[i].x, live[i].y),
                                guess: (0.0, 0.0),
                            })
                            .collect();
                        if let Some(retry_res) = g.track_batch_fb(prev_slot, curr_slot, &retry_q, LK_PATCH_RADIUS, FB_MAX_PX) {
                            for (&orig_i, r) in failed_idx.iter().zip(retry_res) {
                                if let Some((nx, ny)) = r {
                                    let p = live[orig_i];
                                    matches.push((p.id, p.x, p.y, nx, ny));
                                    survivors.push(LivePoint {
                                        id: p.id,
                                        x: nx,
                                        y: ny,
                                        vx: nx - p.x,
                                        vy: ny - p.y,
                                    });
                                }
                            }
                        }
                    }
                }
            } else if let (Some(pp), Some(cp)) = (prev_pyr.as_deref(), curr_pyr.as_deref()) {
                for &p in &live {
                    let tracked = track_single_cpu(pp, cp, (p.x, p.y), (p.vx, p.vy)).or_else(|| {
                        (p.vx.hypot(p.vy) > 0.75)
                            .then(|| track_single_cpu(pp, cp, (p.x, p.y), (0.0, 0.0)))
                            .flatten()
                    });
                    if let Some((nx, ny)) = tracked {
                        matches.push((p.id, p.x, p.y, nx, ny));
                        survivors.push(LivePoint {
                            id: p.id,
                            x: nx,
                            y: ny,
                            vx: nx - p.x,
                            vy: ny - p.y,
                        });
                    }
                }
            }

            // Whip-pan recovery: if < 35% of features survived and it is NOT a hard scene cut,
            // run coarse ZNCC global shift estimation and re-track with that global shift guess!
            let gpu_pair = if gpu_ready {
                gpu.zip(gpu_prev_slot).map(|(g, ps)| (g, ps, curr_slot))
            } else {
                None
            };
            if survivors.len() * 100 < prev_count * 35
                && scene_diff < 0.36
                && let Some((whip_dx, whip_dy)) = estimate_whip_pan_guess(
                    prev_pyr.as_deref(),
                    curr_pyr.as_deref(),
                    gpu_pair,
                    vw,
                    vh,
                    &live,
                )
            {
                let mut whip_matches = Vec::with_capacity(live.len());
                let mut whip_survivors = Vec::with_capacity(live.len());
                if let Some((g, prev_slot, _)) = gpu_pair {
                    let queries: Vec<GpuTrackQuery> = live
                        .iter()
                        .map(|p| GpuTrackQuery {
                            pt: (p.x, p.y),
                            guess: (whip_dx, whip_dy),
                        })
                        .collect();
                    if let Some(res) = g.track_batch_fb(prev_slot, curr_slot, &queries, LK_PATCH_RADIUS, FB_MAX_PX) {
                        for (i, r) in res.into_iter().enumerate() {
                            if let Some((nx, ny)) = r {
                                let p = live[i];
                                whip_matches.push((p.id, p.x, p.y, nx, ny));
                                whip_survivors.push(LivePoint {
                                    id: p.id,
                                    x: nx,
                                    y: ny,
                                    vx: nx - p.x,
                                    vy: ny - p.y,
                                });
                            }
                        }
                    }
                } else if let (Some(pp), Some(cp)) = (prev_pyr.as_deref(), curr_pyr.as_deref()) {
                    for &p in &live {
                        if let Some((nx, ny)) = track_single_cpu(pp, cp, (p.x, p.y), (whip_dx, whip_dy)) {
                            whip_matches.push((p.id, p.x, p.y, nx, ny));
                            whip_survivors.push(LivePoint {
                                id: p.id,
                                x: nx,
                                y: ny,
                                vx: nx - p.x,
                                vy: ny - p.y,
                            });
                        }
                    }
                }
                if whip_survivors.len() > survivors.len() {
                    matches = whip_matches;
                    survivors = whip_survivors;
                }
            }
            live = survivors;
        }

        let survival = if prev_count == 0 {
            1.0
        } else {
            matches.len() as f32 / prev_count as f32
        };

        if frame_idx == 0 || frame_idx % 2 == 0 || live.len() < (TARGET_FEATURES * 7) / 10 {
            seed_grid(
                &view,
                gpu_ready.then_some(gpu).flatten().map(|g| (g, curr_slot)),
                &mut live,
                &mut next_id,
            );
        }

        let alive = live.iter().map(|p| (p.id, p.x, p.y)).collect::<Vec<_>>();
        observations.push(FrameObservation {
            time: input.time,
            matches,
            alive,
            survival,
            scene_diff,
        });

        if gpu_ready {
            gpu_prev_slot = Some(curr_slot);
        } else {
            gpu_prev_slot = None;
            prev_pyr = curr_pyr;
        }
        frame_idx += 1;
        Ok(())
    })?;

    if observations.len() < 2 {
        return Err(Camera2DError::TooShort);
    }

    let (cw, ch, vx, vy, vw, vh) = view_geom.ok_or(Camera2DError::TooShort)?;
    let (shot_start, shot_end, shots_skipped) =
        isolate_shot_around_playhead(&observations, options.playhead, options.cut_sensitivity);
    let shot = &observations[shot_start..shot_end];
    if shot.len() < 2 {
        return Err(Camera2DError::TooShort);
    }

    let cx = vw as f64 * 0.5;
    let cy = vh as f64 * 0.5;
    let tol = (vw.max(vh) as f64 * 0.0042).clamp(1.5, 2.5);

    // World coordinate table for confirmed background features: `(u, v, birth_local_frame, inlier_hits)`.
    let mut world_pts: Vec<Option<(f64, f64, usize, u32)>> = vec![None; next_id + 1];
    for &(id, px, py) in &shot[0].alive {
        if let Some(slot) = world_pts.get_mut(id) {
            *slot = Some((f64::from(px) - cx, f64::from(py) - cy, 0, 2));
        }
    }

    // Global 2D world-to-screen pose at frame `k`:
    //   x_screen - cx = A_k * u - B_k * v + TX_k
    //   y_screen - cy = B_k * u + A_k * v + TY_k
    let (mut ga, mut gb, mut gtx, mut gty) = (1.0f64, 0.0f64, 0.0f64, 0.0f64);
    let mut shot_has_zoom = false;
    let mut shot_has_rot = false;
    let mut rng = 0x2D_CA_4E_5A_91_07_u64;
    let mut conf_sum = 1.0f32;

    let norm_sample = |a: f64, b: f64, tx: f64, ty: f64, time: Tick, confidence: f32| -> TrackSample {
        let scale = a.hypot(b).clamp(0.05, 20.0) as f32;
        let rotation = b.atan2(a).to_degrees() as f32;
        let x = ((vx as f64 + cx + tx) / cw.max(1) as f64) as f32;
        let y = ((vy as f64 + cy + ty) / ch.max(1) as f64) as f32;
        TrackSample {
            time,
            x,
            y,
            z: 0.0,
            scale,
            rotation,
            confidence,
        }
    };

    let mut samples = Vec::with_capacity(shot.len());
    samples.push(norm_sample(1.0, 0.0, 0.0, 0.0, shot[0].time, 1.0));

    for (k, f) in shot.iter().enumerate().skip(1) {
        let (step, inlier_ids) = estimate_2d_camera_step(&f.matches, vw, vh, &mut rng);
        shot_has_zoom |= step.has_zoom;
        shot_has_rot |= step.has_rot;

        // Compose incremental step with previous global pose.
        let mut na = step.a * ga - step.b * gb;
        let mut nb = step.b * ga + step.a * gb;
        let mut ntx = step.a * gtx - step.b * gty + step.tx;
        let mut nty = step.b * gtx + step.a * gty + step.ty;

        if !shot_has_rot {
            let sc = if shot_has_zoom { na.hypot(nb) } else { 1.0 };
            na = sc;
            nb = 0.0;
        } else if !shot_has_zoom {
            let sc = na.hypot(nb).max(1e-9);
            na /= sc;
            nb /= sc;
        }

        // Increment background confidence on RANSAC step inliers.
        let mut inlier_flag = vec![false; next_id + 1];
        for &id in &inlier_ids {
            if let Some(fl) = inlier_flag.get_mut(id) {
                *fl = true;
            }
            if let Some(Some((_, _, _, hits))) = world_pts.get_mut(id) {
                *hits = hits.saturating_add(1);
            }
        }

        // Direct World-Space Keyframe Refinement: eliminate cumulative drift over long pans by
        // fitting `(na, nb, ntx, nty)` directly against confirmed multi-frame world landmarks!
        let mut world_corrs: Vec<(f64, f64, f64, f64, usize)> = Vec::new();
        let mut world_cells = 0u64;
        for &(id, px, py) in &f.alive {
            if !*inlier_flag.get(id).unwrap_or(&false) {
                continue;
            }
            if let Some(Some((u, v, birth, hits))) = world_pts.get(id).copied()
                && hits >= 2
                && k.saturating_sub(birth) >= 2
            {
                let x2 = f64::from(px) - cx;
                let y2 = f64::from(py) - cy;
                let pred_x = na * u - nb * v + ntx;
                let pred_y = nb * u + na * v + nty;
                if (x2 - pred_x).hypot(y2 - pred_y) <= tol * 2.4 {
                    let cell = grid_cell_index(px, py, vw, vh);
                    world_cells |= 1u64 << cell.min(63);
                    world_corrs.push((u, v, x2, y2, cell));
                }
            }
        }

        if world_corrs.len() >= 10 && count_cells_mask(world_cells) >= 3 {
            if !shot_has_zoom && !shot_has_rot {
                let (rtx, rty) = refine_translation(&world_corrs, ntx, nty, tol);
                ntx = rtx;
                nty = rty;
            } else if shot_has_zoom && !shot_has_rot {
                let (rs, rtx, rty) = refine_scale_pan(&world_corrs, na, ntx, nty, tol);
                na = rs;
                nb = 0.0;
                ntx = rtx;
                nty = rty;
            } else {
                let (ra, rb, rtx, rty) = refine_similarity_4dof(&world_corrs, na, nb, ntx, nty, tol);
                if !shot_has_zoom {
                    let sc = ra.hypot(rb).max(1e-9);
                    na = ra / sc;
                    nb = rb / sc;
                } else {
                    na = ra;
                    nb = rb;
                }
                ntx = rtx;
                nty = rty;
            }
        }

        ga = na;
        gb = nb;
        gtx = ntx;
        gty = nty;

        // Register newly seeded background points into 2D world space via inverse transform.
        let det = (ga * ga + gb * gb).max(1e-9);
        for &(id, px, py) in &f.alive {
            if let Some(slot) = world_pts.get_mut(id)
                && slot.is_none()
            {
                let rx = (f64::from(px) - cx) - gtx;
                let ry = (f64::from(py) - cy) - gty;
                let u = (ga * rx + gb * ry) / det;
                let v = (-gb * rx + ga * ry) / det;
                let init_hits = u32::from(*inlier_flag.get(id).unwrap_or(&false));
                *slot = Some((u, v, k, init_hits));
            }
        }

        conf_sum += step.confidence;
        samples.push(norm_sample(ga, gb, gtx, gty, f.time, step.confidence));
    }

    smooth_track_samples(&mut samples);

    let mut unwrapped_rot = samples[0].rotation;
    for i in 1..samples.len() {
        let mut d = samples[i].rotation - samples[i - 1].rotation;
        while d > 180.0 {
            d -= 360.0;
        }
        while d < -180.0 {
            d += 360.0;
        }
        unwrapped_rot += d;
        samples[i].rotation = unwrapped_rot;
    }

    Ok(Camera2DSolve {
        mean_inlier_ratio: (conf_sum / shot.len() as f32).clamp(0.0, 1.0),
        samples,
        shots_skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash01(x: i32, y: i32) -> f32 {
        let h = (x.wrapping_mul(374_761_393).wrapping_add(y.wrapping_mul(668_265_263))) as u32;
        let h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
        ((h ^ (h >> 16)) & 0xff_ffff) as f32 / 0xff_ffff as f32
    }

    fn vnoise(x: f32, y: f32, cell: f32) -> f32 {
        let (gx, gy) = ((x / cell).floor(), (y / cell).floor());
        let (mut fx, mut fy) = (x / cell - gx, y / cell - gy);
        fx = fx * fx * (3.0 - 2.0 * fx);
        fy = fy * fy * (3.0 - 2.0 * fy);
        let (a, b) = (hash01(gx as i32, gy as i32), hash01(gx as i32 + 1, gy as i32));
        let (c, d) = (hash01(gx as i32, gy as i32 + 1), hash01(gx as i32 + 1, gy as i32 + 1));
        a * (1.0 - fx) * (1.0 - fy) + b * fx * (1.0 - fy) + c * (1.0 - fx) * fy + d * fx * fy
    }

    fn bg_luma(u: f32, v: f32) -> f32 {
        0.58 * vnoise(u, v, 14.0) + 0.28 * vnoise(u + 41.0, v - 23.0, 6.0) + 0.14 * vnoise(u - 73.0, v + 59.0, 3.0)
    }

    /// Render a 2D background transformed by `(tx, ty, scale, rot_deg)` PLUS an independent
    /// moving foreground character sprite covering a large chunk of the center!
    fn render_2d_frame_with_character(
        w: usize,
        h: usize,
        tx: f32,
        ty: f32,
        scale: f32,
        rot_deg: f32,
        char_cx: Option<(f32, f32)>,
    ) -> Vec<u8> {
        let mut rgba = vec![255u8; w * h * 4];
        let (cx, cy) = (w as f32 * 0.5, h as f32 * 0.5);
        let (c, s) = (rot_deg.to_radians().cos(), rot_deg.to_radians().sin());
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) * 4;
                if let Some(( sx_char, sy_char )) = char_cx {
                    let dx = x as f32 - sx_char;
                    let dy = y as f32 - sy_char;
                    // Moving foreground character (56x76 px textured sprite moving opposite to camera!)
                    if dx.abs() < 28.0 && dy.abs() < 38.0 {
                        let sp = vnoise(dx + 200.0, dy + 200.0, 4.0);
                        rgba[i] = (210.0 * sp + 40.0) as u8;
                        rgba[i + 1] = (60.0 * sp + 20.0) as u8;
                        rgba[i + 2] = (90.0 * sp + 30.0) as u8;
                        continue;
                    }
                }
                let rx = (x as f32 - cx) - tx;
                let ry = (y as f32 - cy) - ty;
                let u = (c * rx + s * ry) / scale;
                let v = (-s * rx + c * ry) / scale;
                let lum = (bg_luma(u, v).clamp(0.0, 1.0) * 235.0 + 10.0) as u8;
                rgba[i] = lum;
                rgba[i + 1] = lum.saturating_sub(8);
                rgba[i + 2] = lum.saturating_add(8);
            }
        }
        rgba
    }

    #[test]
    fn tracks_multi_screen_2d_pan_beyond_unit_bounds_while_ignoring_moving_character() {
        let (w, h) = (240usize, 144usize);
        // Pan 1.5x the entire screen width (-360 px) while a foreground character walks right!
        let frames: Vec<(Tick, Vec<u8>)> = (0..36)
            .map(|k| {
                let tx = -10.0 * k as f32;
                let ty = 2.0 * k as f32;
                let char_x = 50.0 + 4.0 * k as f32;
                let char_y = 72.0 + (k as f32 * 0.4).sin() * 14.0;
                (
                    Tick(k as i64 * 1000),
                    render_2d_frame_with_character(w, h, tx, ty, 1.0, 0.0, Some((char_x, char_y))),
                )
            })
            .collect();

        let solved = solve_2d_camera_space(&Camera2DOptions::default(), |sink| {
            for (t, rgba) in &frames {
                sink(Frame2D {
                    time: *t,
                    width: w,
                    height: h,
                    rgba,
                })?;
            }
            Ok(())
        })
        .expect("2D solve should succeed");

        assert_eq!(solved.samples.len(), 36);
        let last = solved.samples.last().unwrap();
        // Expected last.x = 0.5 + (-350.0 / 240.0) = -0.9583 (well below 0.0 — NOT clamped!).
        let expected_x = 0.5 - 350.0 / w as f32;
        let expected_y = 0.5 + 70.0 / h as f32;
        assert!(
            (last.x - expected_x).abs() < 0.05,
            "expected unclamped 2D world x ~ {expected_x:.3}, got {:.3}",
            last.x
        );
        assert!(
            (last.y - expected_y).abs() < 0.05,
            "expected 2D world y ~ {expected_y:.3}, got {:.3}",
            last.y
        );
        assert!((last.scale - 1.0).abs() < 0.02, "pure pan should keep scale ~ 1.0, got {}", last.scale);
        assert!(last.rotation.abs() < 0.5, "pure pan should keep rotation ~ 0.0, got {}", last.rotation);
    }

    #[test]
    fn static_background_with_moving_character_has_zero_drift() {
        let (w, h) = (240usize, 144usize);
        let frames: Vec<(Tick, Vec<u8>)> = (0..20)
            .map(|k| {
                let char_x = 40.0 + 8.0 * k as f32;
                (
                    Tick(k as i64 * 1000),
                    render_2d_frame_with_character(w, h, 0.0, 0.0, 1.0, 0.0, Some((char_x, 72.0))),
                )
            })
            .collect();

        let solved = solve_2d_camera_space(&Camera2DOptions::default(), |sink| {
            for (t, rgba) in &frames {
                sink(Frame2D {
                    time: *t,
                    width: w,
                    height: h,
                    rgba,
                })?;
            }
            Ok(())
        })
        .expect("static 2D solve should succeed");

        for s in &solved.samples {
            assert!((s.x - 0.5).abs() < 0.002, "static background drifted x={}", s.x);
            assert!((s.y - 0.5).abs() < 0.002, "static background drifted y={}", s.y);
            assert!((s.scale - 1.0).abs() < 0.002);
            assert!(s.rotation.abs() < 0.05);
        }
    }
}
