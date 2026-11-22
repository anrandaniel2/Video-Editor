//! Stages 0–3: working-size conform (`<= 480` px long side), single-pass streaming shot detection
//! (`scene::SceneDetector`) + 2-pyramid Lucas–Kanade feature tracking, 4×3 grid Shi–Tomasi corner
//! seeding, and post-shot track filtering.

use filmcraft_project::{CameraSolve, StereoLayout};
use filmcraft_time::Tick;

use super::linalg::median_f64;
use super::stereo::{StereoObs, sample_stereo_disparities};
use super::{InputFrame, SolveError, SolveOptions};
use crate::scene::{SceneDetector, SceneOptions};
use crate::track3d::{GpuTrackBackend, GpuTrackQuery, Gray, Level, corners, crop_gray, gpu_backend, track_point, view_rect};

pub(crate) const MAX_WORKING_SIDE: usize = 480;
const GRID_COLS: usize = 4;
const GRID_ROWS: usize = 3;
const TARGET_FEATURES: usize = 180;
const PER_CELL_INITIAL: usize = 15;
const MIN_SEPARATION_PX: f32 = 8.0;
const BORDER_PX: usize = 12;
const LK_PATCH_RADIUS: i32 = 6;
const FWD_BWD_MAX_PX: f32 = 1.0;
const RESEED_CADENCE: usize = 8;
const MAX_MEDIAN_STEP_PX: f64 = 25.0;

fn pyramid_levels_for(w: usize, h: usize) -> usize {
    let min_dim = w.min(h);
    if min_dim >= 240 { 3 } else { 2 }
}

/// A contiguous 2D feature track restricted to the solved shot's frames `0..num_frames`.
#[derive(Clone, Debug)]
pub(crate) struct FeatureTrack {
    pub(crate) id: usize,
    pub(crate) start_frame: usize,
    /// Positions `(x, y)` for frames `start_frame .. start_frame + pts.len()`.
    pub(crate) pts: Vec<(f64, f64)>,
}

impl FeatureTrack {
    pub(crate) fn at_frame(&self, frame: usize) -> Option<(f64, f64)> {
        let idx = frame.checked_sub(self.start_frame)?;
        self.pts.get(idx).copied()
    }
}

/// Extracted tracks and metadata for the single shot containing the playhead.
pub(crate) struct TrackedShot {
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) times: Vec<Tick>,
    pub(crate) tracks: Vec<FeatureTrack>,
    pub(crate) stereo_obs: Vec<StereoObs>,
    pub(crate) shots_skipped: u32,
}

struct RawTrack {
    start_frame: usize,
    pts: Vec<(f32, f32)>,
    active: bool,
}

struct StreamState {
    prev_pyr: Option<Vec<Level>>,
    gpu_prev_slot: Option<usize>,
    tracks: Vec<RawTrack>,
    active_ids: Vec<usize>,
    since_seed: usize,
    view_w: usize,
    view_h: usize,
    stereo_raw: Vec<(usize, usize, f32)>,
}

fn conformed_dims(w: usize, h: usize) -> (usize, usize) {
    let mut cw = w.max(1);
    let mut ch = h.max(1);
    while cw.max(ch) > MAX_WORKING_SIDE * 2 {
        cw = (cw / 2).max(1);
        ch = (ch / 2).max(1);
    }
    let long = cw.max(ch);
    if long <= MAX_WORKING_SIDE {
        return (cw, ch);
    }
    let scale = MAX_WORKING_SIDE as f32 / long as f32;
    (((cw as f32 * scale).round() as usize).max(16), ((ch as f32 * scale).round() as usize).max(16))
}

/// Stream up to `CameraSolve::MAX_FRAMES` frames, keeping at most two pyramids in RAM/VRAM at once,
/// isolate the shot containing `opts.playhead`, and return filtered multi-frame tracks.
pub(crate) fn stream_and_track<F>(opts: &SolveOptions, cancel: &dyn Fn() -> bool, mut next_frame: F) -> Result<TrackedShot, SolveError>
where
    F: FnMut() -> Result<Option<InputFrame>, SolveError>,
{
    let scene_opts = SceneOptions::default();
    let cut_thr = scene_opts.threshold();
    let mut detector = SceneDetector::new();
    let mut times: Vec<Tick> = Vec::new();
    let mut state: Option<StreamState> = None;

    while times.len() < CameraSolve::MAX_FRAMES {
        if cancel() {
            return Err(SolveError::Cancelled);
        }
        let Some(frame) = next_frame()? else { break };
        if frame.width == 0 || frame.height == 0 || frame.rgba.len() < frame.width.saturating_mul(frame.height).saturating_mul(4) {
            return Err(SolveError::Decode("Decoded frame has invalid dimensions.".into()));
        }
        let diff = detector.push_rgba(&frame.rgba, frame.width, frame.height);
        let frame_idx = times.len();
        times.push(frame.time);

        let (vx, vy, vw, vh) = view_rect(opts.layout, frame.width, frame.height);
        let dst = conformed_dims(vw, vh);
        let levels = pyramid_levels_for(dst.0, dst.1);
        let right_crop = match opts.layout {
            StereoLayout::Mono => None,
            StereoLayout::SideBySide => Some((vx + vw, vy, vw, vh)),
            StereoLayout::TopBottom => Some((vx, vy + vh, vw, vh)),
        };

        if let Some(gpu) = gpu_backend() {
            let slot = match state.as_ref().and_then(|st| st.gpu_prev_slot) {
                Some(prev_s) => 1 - (prev_s & 1),
                None => 0,
            };
            if let Some(left_gray) = gpu.build_rgba_pyramid(slot, &frame.rgba, frame.width, frame.height, (vx, vy, vw, vh), dst, levels) {
                let need_stereo = right_crop.is_some() && (frame_idx == 0 || frame_idx.is_multiple_of(4));
                let right_slot = if need_stereo {
                    right_crop.and_then(|rc| gpu.build_rgba_pyramid(2, &frame.rgba, frame.width, frame.height, rc, dst, 1).map(|_| 2usize))
                } else {
                    None
                };
                match &mut state {
                    None => state = Some(init_stream_gpu(gpu, slot, left_gray, right_slot, opts)),
                    Some(st) => {
                        if left_gray.w != st.view_w || left_gray.h != st.view_h {
                            return Err(SolveError::Decode("Frame dimensions changed mid-clip.".into()));
                        }
                        step_stream_gpu(st, gpu, slot, frame_idx, left_gray, right_slot, diff >= cut_thr, opts);
                    }
                }
                continue;
            }
        }

        let raw_gray = Gray::from_rgba8(&frame.rgba, frame.width, frame.height);
        let (raw_left, raw_right) = split_eyes(&raw_gray, opts.layout);
        let left = conform_gray(raw_left);
        let right = raw_right.map(conform_gray);
        match &mut state {
            None => state = Some(init_stream(left, right.as_ref(), opts)),
            Some(st) => {
                if left.w != st.view_w || left.h != st.view_h {
                    return Err(SolveError::Decode("Frame dimensions changed mid-clip.".into()));
                }
                step_stream(st, frame_idx, left, right.as_ref(), diff >= cut_thr, opts);
            }
        }
    }

    if times.len() < 8 {
        return Err(SolveError::TooShort);
    }
    let Some(st) = state else { return Err(SolveError::TooShort) };
    let (shot_range, shots_skipped) = select_playhead_shot(&times, &detector.cuts(&scene_opts), opts.playhead);
    build_shot_tracks(st, &times, shot_range, shots_skipped)
}

fn conform_gray(mut g: Gray) -> Gray {
    while g.w.max(g.h) > MAX_WORKING_SIDE * 2 {
        g = g.half();
    }
    let long = g.w.max(g.h);
    if long <= MAX_WORKING_SIDE {
        return g;
    }
    let scale = MAX_WORKING_SIDE as f32 / long as f32;
    let nw = ((g.w as f32 * scale).round() as usize).max(16);
    let nh = ((g.h as f32 * scale).round() as usize).max(16);
    resize_bilinear(&g, nw, nh)
}

fn resize_bilinear(src: &Gray, nw: usize, nh: usize) -> Gray {
    let mut p = vec![0.0f32; nw * nh];
    let sx_scale = (src.w.saturating_sub(1)) as f32 / (nw.saturating_sub(1).max(1)) as f32;
    let sy_scale = (src.h.saturating_sub(1)) as f32 / (nh.saturating_sub(1).max(1)) as f32;
    for y in 0..nh {
        let sy = y as f32 * sy_scale;
        for x in 0..nw {
            let sx = x as f32 * sx_scale;
            p[y * nw + x] = src.at(sx, sy).unwrap_or(0.0);
        }
    }
    Gray { w: nw, h: nh, p }
}

fn split_eyes(full: &Gray, layout: StereoLayout) -> (Gray, Option<Gray>) {
    let (vx, vy, vw, vh) = view_rect(layout, full.w, full.h);
    let left = crop_gray(full, vx, vy, vw, vh);
    let right = match layout {
        StereoLayout::Mono => None,
        StereoLayout::SideBySide => Some(crop_gray(full, vx + vw, vy, vw, vh)),
        StereoLayout::TopBottom => Some(crop_gray(full, vx, vy + vh, vw, vh)),
    };
    (left, right)
}

fn init_stream(left: Gray, right: Option<&Gray>, opts: &SolveOptions) -> StreamState {
    let (view_w, view_h) = (left.w, left.h);
    let seeded = seed_grid_corners(&left, &[]);
    let mut tracks = Vec::with_capacity(seeded.len());
    let mut active_ids = Vec::with_capacity(seeded.len());
    for pt in seeded {
        let id = tracks.len();
        tracks.push(RawTrack { start_frame: 0, pts: vec![pt], active: true });
        active_ids.push(id);
    }
    let mut stereo_raw = Vec::new();
    if let Some(r_img) = right {
        let pairs: Vec<(usize, (f32, f32))> = active_ids.iter().filter_map(|&tid| Some((tid, *tracks[tid].pts.last()?))).collect();
        sample_stereo_disparities(&left, r_img, 0, &pairs, opts.stereo_baseline, &mut stereo_raw);
    }
    let prev_pyr = Some(left.pyramid(pyramid_levels_for(view_w, view_h)));
    StreamState { prev_pyr, gpu_prev_slot: None, tracks, active_ids, since_seed: 0, view_w, view_h, stereo_raw }
}

fn init_stream_gpu(
    gpu: &'static dyn GpuTrackBackend,
    slot: usize,
    left: Gray,
    right_slot: Option<usize>,
    opts: &SolveOptions,
) -> StreamState {
    let (view_w, view_h) = (left.w, left.h);
    let seeded = seed_grid_corners_gpu(gpu, slot, &left, &[]);
    let mut tracks = Vec::with_capacity(seeded.len());
    let mut active_ids = Vec::with_capacity(seeded.len());
    for pt in seeded {
        let id = tracks.len();
        tracks.push(RawTrack { start_frame: 0, pts: vec![pt], active: true });
        active_ids.push(id);
    }
    let mut stereo_raw = Vec::new();
    if let Some(rs) = right_slot {
        sample_stereo_disparities_gpu(gpu, slot, rs, 0, &active_ids, &tracks, opts.stereo_baseline, &mut stereo_raw);
    }
    StreamState { prev_pyr: None, gpu_prev_slot: Some(slot), tracks, active_ids, since_seed: 0, view_w, view_h, stereo_raw }
}

fn step_stream_gpu(
    st: &mut StreamState,
    gpu: &'static dyn GpuTrackBackend,
    curr_slot: usize,
    frame_idx: usize,
    left: Gray,
    right_slot: Option<usize>,
    candidate_cut: bool,
    opts: &SolveOptions,
) {
    let Some(prev_slot) = st.gpu_prev_slot else {
        st.prev_pyr = Some(left.pyramid(pyramid_levels_for(st.view_w, st.view_h)));
        return;
    };
    let mut surviving = Vec::with_capacity(st.active_ids.len());
    if !st.active_ids.is_empty() {
        let queries: Vec<GpuTrackQuery> = st
            .active_ids
            .iter()
            .map(|&tid| {
                let tr = &st.tracks[tid];
                let p = *tr.pts.last().unwrap_or(&(0.0, 0.0));
                let vel = if tr.pts.len() >= 2 {
                    let prev = tr.pts[tr.pts.len() - 2];
                    (p.0 - prev.0, p.1 - prev.1)
                } else {
                    (0.0, 0.0)
                };
                GpuTrackQuery { pt: p, guess: vel }
            })
            .collect();

        if let Some(mut results) = gpu.track_batch_fb(prev_slot, curr_slot, &queries, LK_PATCH_RADIUS, FWD_BWD_MAX_PX) {
            // Retry features whose non-zero velocity prediction missed with zero-velocity guess.
            let mut retry_indices = Vec::new();
            let mut retry_queries = Vec::new();
            for (idx, (&q, res)) in queries.iter().zip(&results).enumerate() {
                if res.is_none() && q.guess != (0.0, 0.0) {
                    retry_indices.push(idx);
                    retry_queries.push(GpuTrackQuery { pt: q.pt, guess: (0.0, 0.0) });
                }
            }
            if !retry_queries.is_empty()
                && let Some(retried) = gpu.track_batch_fb(prev_slot, curr_slot, &retry_queries, LK_PATCH_RADIUS, FWD_BWD_MAX_PX)
            {
                for (orig_i, r) in retry_indices.into_iter().zip(retried) {
                    results[orig_i] = r;
                }
            }
            for (&tid, res) in st.active_ids.iter().zip(results) {
                if let Some(q) = res
                    && q.0 >= 2.0
                    && q.1 >= 2.0
                    && q.0 <= st.view_w as f32 - 3.0
                    && q.1 <= st.view_h as f32 - 3.0
                {
                    st.tracks[tid].pts.push(q);
                    surviving.push(tid);
                } else {
                    st.tracks[tid].active = false;
                }
            }
        }
    }
    st.active_ids = surviving;
    st.since_seed += 1;

    let low_count = st.active_ids.len() * 100 < TARGET_FEATURES * 60;
    if low_count || st.since_seed >= RESEED_CADENCE || candidate_cut {
        let existing: Vec<(f32, f32)> = st.active_ids.iter().filter_map(|&tid| st.tracks[tid].pts.last().copied()).collect();
        let fresh = seed_grid_corners_gpu(gpu, curr_slot, &left, &existing);
        for pt in fresh {
            if st.active_ids.len() >= TARGET_FEATURES {
                break;
            }
            let id = st.tracks.len();
            st.tracks.push(RawTrack { start_frame: frame_idx, pts: vec![pt], active: true });
            st.active_ids.push(id);
        }
        st.since_seed = 0;
    }

    if let Some(rs) = right_slot
        && frame_idx.is_multiple_of(4)
    {
        sample_stereo_disparities_gpu(gpu, curr_slot, rs, frame_idx, &st.active_ids, &st.tracks, opts.stereo_baseline, &mut st.stereo_raw);
    }
    st.gpu_prev_slot = Some(curr_slot);
}

fn sample_stereo_disparities_gpu(
    gpu: &'static dyn GpuTrackBackend,
    left_slot: usize,
    right_slot: usize,
    frame_idx: usize,
    active_ids: &[usize],
    tracks: &[RawTrack],
    max_disp: f32,
    out: &mut Vec<(usize, usize, f32)>,
) {
    let pairs: Vec<(usize, (f32, f32))> = active_ids.iter().filter_map(|&tid| Some((tid, *tracks[tid].pts.last()?))).collect();
    if pairs.is_empty() {
        return;
    }
    let pts: Vec<(f32, f32)> = pairs.iter().map(|&(_, p)| p).collect();
    if let Some(disps) = gpu.stereo_disparity_batch(left_slot, right_slot, &pts, 5, max_disp.max(1.0)) {
        let scale = max_disp.max(1.0);
        for ((tid, _), d_norm) in pairs.into_iter().zip(disps) {
            if let Some(dn) = d_norm {
                let d_px = dn * scale;
                if d_px >= 0.35 && d_px <= max_disp * 0.96 {
                    out.push((tid, frame_idx, d_px));
                }
            }
        }
    }
}

fn seed_grid_corners_gpu(
    gpu: &'static dyn GpuTrackBackend,
    slot: usize,
    img: &Gray,
    existing: &[(f32, f32)],
) -> Vec<(f32, f32)> {
    let border = BORDER_PX.min(img.w / 6).min(img.h / 6).max(2);
    if img.w <= border * 2 + 4 || img.h <= border * 2 + 4 {
        return Vec::new();
    }
    let (x_min, y_min) = (border, border);
    let (x_max, y_max) = (img.w - border, img.h - border);
    let Some(mut all_cands) = gpu.detect_corners(slot, (x_min, y_min, x_max, y_max), 6) else {
        return seed_grid_corners(img, existing);
    };
    all_cands.sort_by(|a, b| b.0.total_cmp(&a.0));
    let cell_w = (x_max - x_min) as f32 / GRID_COLS as f32;
    let cell_h = (y_max - y_min) as f32 / GRID_ROWS as f32;
    let mut cell_counts = [[0usize; GRID_COLS]; GRID_ROWS];
    let mut accepted: Vec<(f32, f32)> = existing.to_vec();
    let mut added: Vec<(f32, f32)> = Vec::new();
    let mut overflow: Vec<(f32, f32)> = Vec::new();

    for &(_, px, py) in &all_cands {
        if accepted.iter().any(|&(qx, qy)| (qx - px).hypot(qy - py) < MIN_SEPARATION_PX) {
            continue;
        }
        let gx = (((px - x_min as f32) / cell_w).floor() as isize).clamp(0, GRID_COLS as isize - 1) as usize;
        let gy = (((py - y_min as f32) / cell_h).floor() as isize).clamp(0, GRID_ROWS as isize - 1) as usize;
        if cell_counts[gy][gx] < PER_CELL_INITIAL && accepted.len() < TARGET_FEATURES {
            cell_counts[gy][gx] += 1;
            accepted.push((px, py));
            added.push((px, py));
        } else {
            overflow.push((px, py));
        }
    }
    for (px, py) in overflow {
        if accepted.len() >= TARGET_FEATURES {
            break;
        }
        if accepted.iter().all(|&(qx, qy)| (qx - px).hypot(qy - py) >= MIN_SEPARATION_PX) {
            accepted.push((px, py));
            added.push((px, py));
        }
    }
    added
}

fn step_stream(st: &mut StreamState, frame_idx: usize, left: Gray, right: Option<&Gray>, candidate_cut: bool, opts: &SolveOptions) {
    let curr_pyr = left.pyramid(pyramid_levels_for(st.view_w, st.view_h));
    let mut surviving = Vec::with_capacity(st.active_ids.len());
    if let Some(prev_pyr) = st.prev_pyr.as_ref() {
        for &tid in &st.active_ids {
            if let Some(next_pt) = advance_track(prev_pyr, &curr_pyr, &st.tracks[tid], st.view_w, st.view_h) {
                st.tracks[tid].pts.push(next_pt);
                surviving.push(tid);
            } else {
                st.tracks[tid].active = false;
            }
        }
    }
    st.active_ids = surviving;
    st.since_seed += 1;

    let low_count = st.active_ids.len() * 100 < TARGET_FEATURES * 60;
    if low_count || st.since_seed >= RESEED_CADENCE || candidate_cut {
        let existing: Vec<(f32, f32)> = st.active_ids.iter().filter_map(|&tid| st.tracks[tid].pts.last().copied()).collect();
        let fresh = seed_grid_corners(&left, &existing);
        for pt in fresh {
            if st.active_ids.len() >= TARGET_FEATURES {
                break;
            }
            let id = st.tracks.len();
            st.tracks.push(RawTrack { start_frame: frame_idx, pts: vec![pt], active: true });
            st.active_ids.push(id);
        }
        st.since_seed = 0;
    }

    if let Some(r_img) = right
        && frame_idx.is_multiple_of(4)
    {
        let pairs: Vec<(usize, (f32, f32))> = st.active_ids.iter().filter_map(|&tid| Some((tid, *st.tracks[tid].pts.last()?))).collect();
        sample_stereo_disparities(&left, r_img, frame_idx, &pairs, opts.stereo_baseline, &mut st.stereo_raw);
    }
    // Replace previous pyramid so at most two pyramids ever coexist during `step_stream`.
    st.prev_pyr = Some(curr_pyr);
}

fn advance_track(prev_pyr: &[Level], curr_pyr: &[Level], tr: &RawTrack, w: usize, h: usize) -> Option<(f32, f32)> {
    let &p = tr.pts.last()?;
    let vel = if tr.pts.len() >= 2 {
        let prev = tr.pts[tr.pts.len() - 2];
        (p.0 - prev.0, p.1 - prev.1)
    } else {
        (0.0, 0.0)
    };
    let q = track_point(prev_pyr, curr_pyr, p, LK_PATCH_RADIUS, vel)
        .or_else(|| if vel != (0.0, 0.0) { track_point(prev_pyr, curr_pyr, p, LK_PATCH_RADIUS, (0.0, 0.0)) } else { None })?;
    if q.0 < 2.0 || q.1 < 2.0 || q.0 > w as f32 - 3.0 || q.1 > h as f32 - 3.0 {
        return None;
    }
    let back_guess = (p.0 - q.0, p.1 - q.1);
    let back = track_point(curr_pyr, prev_pyr, q, LK_PATCH_RADIUS, back_guess)?;
    ((back.0 - p.0).hypot(back.1 - p.1) <= FWD_BWD_MAX_PX).then_some(q)
}

/// Deterministic 4×3 grid Shi–Tomasi corner seeding reusing `track3d::corners`.
pub(crate) fn seed_grid_corners(img: &Gray, existing: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let border = BORDER_PX.min(img.w / 6).min(img.h / 6).max(2);
    if img.w <= border * 2 + 4 || img.h <= border * 2 + 4 {
        return Vec::new();
    }
    let x_min = border as f32;
    let y_min = border as f32;
    let x_max = (img.w - border) as f32;
    let y_max = (img.h - border) as f32;
    let cell_w = (x_max - x_min) / GRID_COLS as f32;
    let cell_h = (y_max - y_min) / GRID_ROWS as f32;
    let reach = 0.5 * cell_w.hypot(cell_h) + 2.0;

    let mut accepted: Vec<(f32, f32)> = existing.to_vec();
    let mut added: Vec<(f32, f32)> = Vec::new();
    let mut overflow: Vec<(f32, f32)> = Vec::new();

    for gy in 0..GRID_ROWS {
        let cy0 = y_min + gy as f32 * cell_h;
        let cy1 = if gy + 1 == GRID_ROWS { y_max } else { cy0 + cell_h };
        for gx in 0..GRID_COLS {
            let cx0 = x_min + gx as f32 * cell_w;
            let cx1 = if gx + 1 == GRID_COLS { x_max } else { cx0 + cell_w };
            let cands = corners(img, 0.5 * (cx0 + cx1), 0.5 * (cy0 + cy1), reach, MIN_SEPARATION_PX, 28);
            let mut cell_taken = 0usize;
            for (px, py) in cands {
                if px < cx0 || px > cx1 || py < cy0 || py > cy1 {
                    continue;
                }
                if accepted.iter().any(|&(qx, qy)| (qx - px).hypot(qy - py) < MIN_SEPARATION_PX) {
                    continue;
                }
                if cell_taken < PER_CELL_INITIAL && accepted.len() < TARGET_FEATURES {
                    accepted.push((px, py));
                    added.push((px, py));
                    cell_taken += 1;
                } else {
                    overflow.push((px, py));
                }
            }
        }
    }

    for (px, py) in overflow {
        if accepted.len() >= TARGET_FEATURES {
            break;
        }
        if accepted.iter().all(|&(qx, qy)| (qx - px).hypot(qy - py) >= MIN_SEPARATION_PX) {
            accepted.push((px, py));
            added.push((px, py));
        }
    }
    added
}

fn select_playhead_shot(times: &[Tick], cuts: &[usize], playhead: Tick) -> (std::ops::Range<usize>, u32) {
    let n = times.len();
    let mut bounds = Vec::with_capacity(cuts.len() + 2);
    bounds.push(0usize);
    for &c in cuts {
        if c > 0 && c < n && bounds.last() != Some(&c) {
            bounds.push(c);
        }
    }
    bounds.push(n);

    // Locate frame nearest to `playhead`.
    let mut playhead_idx = 0usize;
    let mut best_dist = i128::MAX;
    for (i, t) in times.iter().enumerate() {
        let dist = (i128::from(t.0) - i128::from(playhead.0)).abs();
        if dist < best_dist {
            best_dist = dist;
            playhead_idx = i;
        }
    }

    let total_shots = bounds.len().saturating_sub(1);
    let shots_skipped = total_shots.saturating_sub(1) as u32;
    for w in bounds.windows(2) {
        if playhead_idx >= w[0] && playhead_idx < w[1] {
            return (w[0]..w[1], shots_skipped);
        }
    }
    (0..n, shots_skipped)
}

fn build_shot_tracks(st: StreamState, all_times: &[Tick], shot: std::ops::Range<usize>, shots_skipped: u32) -> Result<TrackedShot, SolveError> {
    let shot_times: Vec<Tick> = all_times[shot.clone()].to_vec();
    let num_frames = shot_times.len();
    if num_frames < 8 {
        return Err(SolveError::TooShort);
    }
    let min_obs = 12.min(num_frames);
    let mut tracks = Vec::new();
    let mut raw_to_clean: Vec<Option<usize>> = vec![None; st.tracks.len()];

    for (raw_id, rt) in st.tracks.iter().enumerate() {
        let raw_end = rt.start_frame + rt.pts.len();
        let seg_start = rt.start_frame.max(shot.start);
        let seg_end = raw_end.min(shot.end);
        if seg_end <= seg_start || seg_end - seg_start < min_obs {
            continue;
        }
        let slice = &rt.pts[(seg_start - rt.start_frame)..(seg_end - rt.start_frame)];
        if !passes_median_step(slice) {
            continue;
        }
        let clean_id = tracks.len();
        raw_to_clean[raw_id] = Some(clean_id);
        let mut pts: Vec<(f64, f64)> = slice.iter().map(|&(x, y)| (f64::from(x), f64::from(y))).collect();
        smooth_2d_feature_track(&mut pts);
        tracks.push(FeatureTrack {
            id: clean_id,
            start_frame: seg_start - shot.start,
            pts,
        });
    }

    if tracks.len() < 12 {
        return Err(SolveError::InsufficientMotion);
    }

    let mut stereo_obs = Vec::new();
    for (raw_tid, frame_idx, disp_px) in st.stereo_raw {
        if frame_idx >= shot.start
            && frame_idx < shot.end
            && let Some(clean_tid) = raw_to_clean.get(raw_tid).copied().flatten()
        {
            stereo_obs.push(StereoObs { track_id: clean_tid, frame: frame_idx - shot.start, disparity_px: f64::from(disp_px) });
        }
    }

    Ok(TrackedShot { width: st.view_w, height: st.view_h, times: shot_times, tracks, stereo_obs, shots_skipped })
}

fn passes_median_step(pts: &[(f32, f32)]) -> bool {
    if pts.len() < 2 {
        return false;
    }
    let mut steps: Vec<f64> = pts.windows(2).map(|w| f64::from((w[1].0 - w[0].0).hypot(w[1].1 - w[0].1))).collect();
    median_f64(&mut steps) <= MAX_MEDIAN_STEP_PX
}

/// Sub-pixel velocity-preserving bilateral filter on individual 2D feature trajectories: suppresses
/// H.264 macroblock / sub-pixel LK noise (`<= 0.7 px`) while leaving linear velocities, parabolic
/// camera arcs, and perspective foreshortening untouched.
fn smooth_2d_feature_track(pts: &mut [(f64, f64)]) {
    let n = pts.len();
    if n < 5 {
        return;
    }
    let orig = pts.to_vec();
    for i in 1..n - 1 {
        let lin_x = 0.5 * (orig[i - 1].0 + orig[i + 1].0);
        let lin_y = 0.5 * (orig[i - 1].1 + orig[i + 1].1);
        let dev = (orig[i].0 - lin_x).hypot(orig[i].1 - lin_y);
        // Only damp sub-pixel micro-oscillations (`dev < 1.2 px`); preserve genuine accelerations!
        if dev < 1.2 {
            let alpha = 0.36 * (1.0 - dev / 1.2);
            pts[i].0 = (1.0 - alpha) * orig[i].0 + alpha * lin_x;
            pts[i].1 = (1.0 - alpha) * orig[i].1 + alpha * lin_y;
        }
    }
}
