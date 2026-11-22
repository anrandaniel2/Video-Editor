//! Feature-based motion tracking for 3D layer follow.
//!
//! Pyramidal Lucas–Kanade in Bouguet's formulation (the algorithm behind OpenCV's
//! `calcOpticalFlowPyrLK`): the template's gradients and Hessian are computed once per point per
//! pyramid level, the search starts at the coarsest level and is refined downwards, so a feature
//! can move tens of pixels between frames instead of the fraction of a patch a single-level
//! solver manages. Three things make it hold on real footage:
//!
//! * every match is tracked home again and compared with where it started (the forward/backward
//!   check from Median-Flow), so points that slipped onto a similar-looking bit are dropped;
//! * the survivors vote on one similarity transform, with outlier rejection, instead of the
//!   centre point deciding on its own;
//! * the last frame's velocity seeds the search, and a correlation search at the coarsest level
//!   recovers when that guess is not enough. Corners are re-seeded around the target as they
//!   drift or leave the picture, which is what keeps a long track from accumulating error.
//!
//! [`Tracker`] is streaming: it keeps two pyramids, not the whole clip. An optional right
//! stereoscopic view adds relative depth by disparity search.

/// A single-channel tracking picture (0..1 luma).
#[derive(Clone)]
pub struct Gray {
    pub w: usize,
    pub h: usize,
    pub p: Vec<f32>,
}

/// Bilinear sample of a plane, `None` outside it.
fn sample(p: &[f32], w: usize, h: usize, x: f32, y: f32) -> Option<f32> {
    if !(x >= 0.0 && y >= 0.0 && x <= w as f32 - 1.0 && y <= h as f32 - 1.0) {
        return None;
    }
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(w - 1);
    let y1 = (y0 + 1).min(h - 1);
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    Some(p[y0 * w + x0] * (1.0 - fx) * (1.0 - fy) + p[y0 * w + x1] * fx * (1.0 - fy) + p[y1 * w + x0] * (1.0 - fx) * fy + p[y1 * w + x1] * fx * fy)
}

impl Gray {
    pub fn from_rgba8(rgba: &[u8], w: usize, h: usize) -> Self {
        let mut p = vec![0.0; w * h];
        for (i, px) in rgba.chunks_exact(4).enumerate().take(w * h) {
            p[i] = (0.2126 * px[0] as f32 + 0.7152 * px[1] as f32 + 0.0722 * px[2] as f32) / 255.0;
        }
        Self { w, h, p }
    }

    pub(crate) fn at(&self, x: f32, y: f32) -> Option<f32> {
        sample(&self.p, self.w, self.h, x, y)
    }

    /// Half size, low-passed first so the coarse levels do not alias (a 5-tap binomial).
    pub(crate) fn half(&self) -> Gray {
        let (w, h) = ((self.w / 2).max(1), (self.h / 2).max(1));
        let mut p = vec![0.0; w * h];
        let k = [1.0f32, 4.0, 6.0, 4.0, 1.0];
        for y in 0..h {
            for x in 0..w {
                let (cx, cy) = (x as i32 * 2, y as i32 * 2);
                let (mut sum, mut wsum) = (0.0f32, 0.0f32);
                for (j, ky) in k.iter().enumerate() {
                    for (i, kx) in k.iter().enumerate() {
                        let sx = (cx + i as i32 - 2).clamp(0, self.w as i32 - 1) as usize;
                        let sy = (cy + j as i32 - 2).clamp(0, self.h as i32 - 1) as usize;
                        let wt = kx * ky;
                        sum += self.p[sy * self.w + sx] * wt;
                        wsum += wt;
                    }
                }
                p[y * w + x] = sum / wsum;
            }
        }
        Gray { w, h, p }
    }

    /// Levels 0 (this picture) .. n-1, each half the one before, with their gradients. The
    /// gradients are computed once per frame and shared by every feature and both directions.
    pub(crate) fn pyramid(&self, levels: usize) -> Vec<Level> {
        let mut imgs = vec![self.clone()];
        while imgs.len() < levels {
            let Some(last) = imgs.last() else { break };
            if last.w < 24 || last.h < 24 {
                break;
            }
            imgs.push(last.half());
        }
        imgs.into_iter()
            .map(|img| {
                let (ix, iy) = img.gradients();
                Level { img, ix, iy }
            })
            .collect()
    }

    /// Rotation-invariant 3×3 Scharr gradients (`[3, 10, 3] / 32`), zero at the 1-px border.
    /// Eliminates directional sub-pixel bias on diagonal edges compared to raw 2-tap differences.
    pub(crate) fn gradients(&self) -> (Vec<f32>, Vec<f32>) {
        let (w, h) = (self.w, self.h);
        let (mut ix, mut iy) = (vec![0.0; w * h], vec![0.0; h * w]);
        if w < 3 || h < 3 {
            return (ix, iy);
        }
        for y in 1..h - 1 {
            let r0 = (y - 1) * w;
            let r1 = y * w;
            let r2 = (y + 1) * w;
            for x in 1..w - 1 {
                let p00 = self.p[r0 + x - 1];
                let p01 = self.p[r0 + x];
                let p02 = self.p[r0 + x + 1];
                let p10 = self.p[r1 + x - 1];
                let p12 = self.p[r1 + x + 1];
                let p20 = self.p[r2 + x - 1];
                let p21 = self.p[r2 + x];
                let p22 = self.p[r2 + x + 1];
                ix[r1 + x] = (3.0 * (p02 - p00) + 10.0 * (p12 - p10) + 3.0 * (p22 - p20)) * (1.0 / 32.0);
                iy[r1 + x] = (3.0 * (p20 - p00) + 10.0 * (p21 - p01) + 3.0 * (p22 - p02)) * (1.0 / 32.0);
            }
        }
        (ix, iy)
    }
}

/// One feature query for batched GPU pyramidal Lucas–Kanade + forward-backward verification.
#[derive(Clone, Copy, Debug)]
pub struct GpuTrackQuery {
    pub pt: (f32, f32),
    pub guess: (f32, f32),
}

/// Pluggable GPU compute backend (`filmcraft_gpu::tracker`) executing RGBA→luma, Gaussian
/// pyramids, Scharr gradients, Shi–Tomasi corner detection, illumination-invariant forward-backward
/// pyramidal Lucas–Kanade, coarse ZNCC search, and stereo disparity completely on the GPU.
pub trait GpuTrackBackend: Send + Sync {
    /// Upload RGBA8, crop to `crop = (vx, vy, vw, vh)`, conform to `dst_size = (dw, dh)`, and
    /// build `levels` Gaussian pyramid levels + Scharr gradients in GPU VRAM `slot`.
    fn build_rgba_pyramid(
        &self,
        slot: usize,
        rgba: &[u8],
        src_w: usize,
        src_h: usize,
        crop: (usize, usize, usize, usize),
        dst_size: (usize, usize),
        levels: usize,
    ) -> Option<Gray>;

    /// Upload an existing level-0 [`Gray`] plane and build its Gaussian pyramid + Scharr gradients
    /// on the GPU in `slot`.
    fn upload_gray_pyramid(&self, slot: usize, gray: &Gray, levels: usize) -> bool;

    /// Compute dense Shi–Tomasi minimum eigenvalues on GPU `slot` level 0 inside
    /// `roi = (x0, y0, x1, y1)`, returning `(min_eig, x, y)` tile maxima per `cell`×`cell` block.
    fn detect_corners(
        &self,
        slot: usize,
        roi: (usize, usize, usize, usize),
        cell: usize,
    ) -> Option<Vec<(f32, f32, f32)>>;

    /// Track a batch of points from `prev_slot` to `curr_slot` on the GPU using mean-normalized
    /// Inverse-Compositional Pyramidal LK + `curr -> prev` backward verification (`<= fb_max_px`).
    fn track_batch_fb(
        &self,
        prev_slot: usize,
        curr_slot: usize,
        queries: &[GpuTrackQuery],
        patch_radius: i32,
        fb_max_px: f32,
    ) -> Option<Vec<Option<(f32, f32)>>>;

    /// Coarse-level ZNCC search on the GPU around `pt` between `prev_slot` and `curr_slot`.
    fn coarse_search(
        &self,
        prev_slot: usize,
        curr_slot: usize,
        pt: (f32, f32),
        patch_radius: i32,
        reach_px: f32,
    ) -> Option<(f32, f32)>;

    /// Batched horizontal ZNCC stereo disparity (`0..=max_disp`) on the GPU between `left_slot`
    /// and `right_slot`, returning normalized disparity `d / max_disp` (`0..=1`) or `None`.
    fn stereo_disparity_batch(
        &self,
        left_slot: usize,
        right_slot: usize,
        pts: &[(f32, f32)],
        patch_radius: i32,
        max_disp: f32,
    ) -> Option<Vec<Option<f32>>>;
}

static GPU_BACKEND: std::sync::OnceLock<&'static dyn GpuTrackBackend> = std::sync::OnceLock::new();

/// Register the `wgpu` compute tracking backend at startup.
pub fn register_gpu_backend(backend: &'static dyn GpuTrackBackend) {
    let _ = GPU_BACKEND.set(backend);
}

/// Active GPU tracking backend, if registered and supported by the device.
pub fn gpu_backend() -> Option<&'static dyn GpuTrackBackend> {
    GPU_BACKEND.get().copied()
}

/// Active primary-eye rectangle `(x0, y0, width, height)` for a stereoscopic packing layout.
pub fn view_rect(layout: filmcraft_project::StereoLayout, w: usize, h: usize) -> (usize, usize, usize, usize) {
    match layout {
        filmcraft_project::StereoLayout::Mono => (0, 0, w, h),
        filmcraft_project::StereoLayout::SideBySide => (0, 0, w / 2, h),
        filmcraft_project::StereoLayout::TopBottom => (0, 0, w, h / 2),
    }
}

/// Extract a sub-rectangle from a [`Gray`] plane, clamping out-of-range coordinates.
pub fn crop_gray(g: &Gray, x0: usize, y0: usize, w: usize, h: usize) -> Gray {
    let w = w.max(1);
    let h = h.max(1);
    let mut p = vec![0.0; w * h];
    if g.p.is_empty() || g.w == 0 || g.h == 0 {
        return Gray { w, h, p };
    }
    for y in 0..h {
        for x in 0..w {
            let sx = (x0 + x).min(g.w.saturating_sub(1));
            let sy = (y0 + y).min(g.h.saturating_sub(1));
            p[y * w + x] = g.p[sy * g.w + sx];
        }
    }
    Gray { w, h, p }
}

/// One pyramid level: the picture and its gradients.
pub(crate) struct Level {
    pub(crate) img: Gray,
    pub(crate) ix: Vec<f32>,
    pub(crate) iy: Vec<f32>,
}

/// One tracked sample in view pixels, with relative depth/scale/rotation.
#[derive(Clone, Copy, Debug)]
pub struct TrackPoint {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub scale: f32,
    pub rotation: f32,
    /// 0..1: how much of the feature set survived the forward/backward check. 0 means the
    /// target was lost and the sample only holds the last known place.
    pub confidence: f32,
}

/// Mean-subtracted template of one feature at one level: zero-mean patch values, Scharr gradients
/// sampled at the point's exact fractional position, and the inverse Hessian of the IC-LK system.
/// Subtracting patch means makes sub-pixel tracking invariant to DC brightness/exposure flicker.
#[derive(Clone)]
struct Template {
    dx: Vec<f32>,
    dy: Vec<f32>,
    v_zero: Vec<f32>,
    ix: Vec<f32>,
    iy: Vec<f32>,
    /// Inverse of [[Σix², Σixiy], [Σixiy, Σiy²]].
    h: [f32; 4],
}

fn template(l: &Level, x: f32, y: f32, r: i32) -> Option<Template> {
    let img = &l.img;
    let rf = r as f32;
    if x - rf < 1.0 || y - rf < 1.0 || x + rf >= img.w as f32 - 1.0 || y + rf >= img.h as f32 - 1.0 {
        return None;
    }
    let count = ((2 * r + 1) * (2 * r + 1)) as f32;
    let (mut t, mut gxx, mut gyy, mut gxy, mut sum_v) = (
        Template { dx: Vec::new(), dy: Vec::new(), v_zero: Vec::new(), ix: Vec::new(), iy: Vec::new(), h: [0.0; 4] },
        0.0f32,
        0.0f32,
        0.0f32,
        0.0f32,
    );
    for oy in -r..=r {
        for ox in -r..=r {
            let (sx, sy) = (x + ox as f32, y + oy as f32);
            let a = sample(&l.ix, img.w, img.h, sx, sy)?;
            let b = sample(&l.iy, img.w, img.h, sx, sy)?;
            let v = img.at(sx, sy)?;
            t.dx.push(ox as f32);
            t.dy.push(oy as f32);
            t.v_zero.push(v);
            t.ix.push(a);
            t.iy.push(b);
            sum_v += v;
            gxx += a * a;
            gyy += b * b;
            gxy += a * b;
        }
    }
    let mean = sum_v / count;
    for v in &mut t.v_zero {
        *v -= mean;
    }
    let det = gxx * gyy - gxy * gxy;
    let tr = gxx + gyy;
    let min_eig = (tr - (tr * tr - 4.0 * det).max(0.0).sqrt()) * 0.5 / count;
    if det.abs() < 1e-11 || min_eig < 6e-6 {
        return None;
    }
    t.h = [gyy / det, -gxy / det, -gxy / det, gxx / det];
    Some(t)
}

/// Mean-normalized Inverse-Compositional Lucas–Kanade at one pyramid level.
fn lk_level(next: &Gray, t: &Template, px: f32, py: f32, guess: (f32, f32)) -> Option<((f32, f32), f32)> {
    let (mut gx, mut gy) = guess;
    let n = t.v_zero.len();
    let inv_n = 1.0 / (n.max(1) as f32);
    let mut cur = vec![0.0f32; n];
    for _ in 0..20 {
        let mut sum = 0.0f32;
        for k in 0..n {
            let v = next.at(px + t.dx[k] + gx, py + t.dy[k] + gy)?;
            cur[k] = v;
            sum += v;
        }
        let mean = sum * inv_n;
        let (mut bx, mut by) = (0.0f32, 0.0f32);
        for k in 0..n {
            let it = (cur[k] - mean) - t.v_zero[k];
            bx += t.ix[k] * it;
            by += t.iy[k] * it;
        }
        let vx = -(t.h[0] * bx + t.h[1] * by);
        let vy = -(t.h[2] * bx + t.h[3] * by);
        if !vx.is_finite() || !vy.is_finite() || vx.abs() > 16.0 || vy.abs() > 16.0 {
            return None;
        }
        gx += vx;
        gy += vy;
        if vx.abs() + vy.abs() < 0.004 {
            break;
        }
    }
    let out = (px + gx, py + gy);
    next.at(out.0, out.1)?;
    let mut sum = 0.0f32;
    for k in 0..n {
        let v = next.at(px + t.dx[k] + gx, py + t.dy[k] + gy)?;
        cur[k] = v;
        sum += v;
    }
    let mean = sum * inv_n;
    let mut err = 0.0f32;
    for k in 0..n {
        err += ((cur[k] - mean) - t.v_zero[k]).abs();
    }
    Some((out, err * inv_n))
}

/// Pyramidal track of one point from `prev` to `next`, coarse to fine, starting from `initial`
/// (a velocity or a correlation match, in level-0 pixels).
pub(crate) fn track_point(prev: &[Level], next: &[Level], p: (f32, f32), r: i32, initial: (f32, f32)) -> Option<(f32, f32)> {
    let levels = prev.len().min(next.len());
    if levels == 0 {
        return None;
    }
    let top = (1usize << (levels - 1)) as f32;
    let mut guess = (initial.0 / top, initial.1 / top);
    let mut point = p;
    for l in (0..levels).rev() {
        let is_coarse = l > 0;
        let r_lvl = (r - l as i32).max(3).min(r.max(2));
        let s = (1usize << l) as f32;
        let (px, py) = (p.0 / s, p.1 / s);
        let Some(t) = template(&prev[l], px, py, r_lvl) else {
            if is_coarse {
                guess = (guess.0 * 2.0, guess.1 * 2.0);
                continue;
            }
            return None;
        };
        let Some(((nx, ny), err)) = lk_level(&next[l].img, &t, px, py, guess) else {
            if is_coarse {
                guess = (guess.0 * 2.0, guess.1 * 2.0);
                continue;
            }
            return None;
        };
        // A patch that no longer looks like itself is not a match, however well it converged.
        if err > 0.28 {
            if is_coarse {
                guess = (guess.0 * 2.0, guess.1 * 2.0);
                continue;
            }
            return None;
        }
        point = (nx * s, ny * s);
        guess = ((nx - px) * 2.0, (ny - py) * 2.0);
    }
    Some(point)
}

/// Zero-mean normalised correlation over a small patch at the coarsest level: a starting guess
/// when the motion is larger than the window, or when the last guess lost the target.
fn search(prev: &[Level], next: &[Level], p: (f32, f32), r: i32, reach: f32) -> Option<(f32, f32)> {
    let l = prev.len().min(next.len()).checked_sub(1)?;
    let s = (1usize << l) as f32;
    let (a, b) = (&prev[l].img, &next[l].img);
    let (cx, cy) = (p.0 / s, p.1 / s);
    let r = r.clamp(2, 4);
    let rf = r as f32;
    if cx - rf < 0.0 || cy - rf < 0.0 || cx + rf >= a.w as f32 || cy + rf >= a.h as f32 {
        return None;
    }
    let mut tpl = Vec::new();
    for oy in -r..=r {
        for ox in -r..=r {
            tpl.push(a.at(cx + ox as f32, cy + oy as f32)?);
        }
    }
    let mean = tpl.iter().sum::<f32>() / tpl.len() as f32;
    for v in &mut tpl {
        *v -= mean;
    }
    let tn = tpl.iter().map(|v| v * v).sum::<f32>().sqrt();
    if tn < 1e-6 {
        return None;
    }
    let range = (reach / s).clamp(2.0, 24.0) as i32;
    let (mut best, mut at) = (0.5f32, None);
    let mut win = Vec::with_capacity(tpl.len());
    for dy in -range..=range {
        for dx in -range..=range {
            let (x, y) = (cx + dx as f32, cy + dy as f32);
            if x - rf < 0.0 || y - rf < 0.0 || x + rf >= b.w as f32 || y + rf >= b.h as f32 {
                continue;
            }
            win.clear();
            let mut ok = true;
            for oy in -r..=r {
                for ox in -r..=r {
                    match b.at(x + ox as f32, y + oy as f32) {
                        Some(v) => win.push(v),
                        None => ok = false,
                    }
                }
            }
            if !ok {
                continue;
            }
            let wm = win.iter().sum::<f32>() / win.len() as f32;
            let mut dot = 0.0f32;
            let mut wn = 0.0f32;
            for (i, v) in win.iter().enumerate() {
                let c = v - wm;
                dot += tpl[i] * c;
                wn += c * c;
            }
            let wn = wn.sqrt();
            if wn < 1e-6 {
                continue;
            }
            let score = dot / (tn * wn);
            if score > best {
                (best, at) = (score, Some((dx as f32 * s, dy as f32 * s)));
            }
        }
    }
    at
}

/// Shi–Tomasi corners near (`x`, `y`), best first, at least `apart` pixels from each other.
pub(crate) fn corners(img: &Gray, x: f32, y: f32, reach: f32, apart: f32, want: usize) -> Vec<(f32, f32)> {
    let r = (reach as i32).max(4);
    let (cx, cy) = (x as i32, y as i32);
    let win = 3i32;
    let mut scored: Vec<(f32, f32, f32)> = Vec::new();
    for py in (cy - r..=cy + r).step_by(2) {
        for px in (cx - r..=cx + r).step_by(2) {
            if px - win < 1 || py - win < 1 || px + win >= img.w as i32 - 1 || py + win >= img.h as i32 - 1 {
                continue;
            }
            if ((px - cx) as f32).hypot((py - cy) as f32) > reach {
                continue;
            }
            let (mut gxx, mut gyy, mut gxy) = (0.0f32, 0.0f32, 0.0f32);
            for oy in -win..=win {
                for ox in -win..=win {
                    let i = ((py + oy) as usize) * img.w + (px + ox) as usize;
                    let a = (img.p[i + 1] - img.p[i - 1]) * 0.5;
                    let b = (img.p[i + img.w] - img.p[i - img.w]) * 0.5;
                    gxx += a * a;
                    gyy += b * b;
                    gxy += a * b;
                }
            }
            let tr = gxx + gyy;
            let det = gxx * gyy - gxy * gxy;
            let min_eig = (tr - (tr * tr - 4.0 * det).max(0.0).sqrt()) * 0.5;
            if min_eig > 8e-5 {
                scored.push((min_eig, px as f32, py as f32));
            }
        }
    }
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut out: Vec<(f32, f32)> = Vec::new();
    for (_, px, py) in scored {
        if out.iter().all(|&(qx, qy)| (qx - px).hypot(qy - py) >= apart) {
            out.push((px, py));
            if out.len() >= want {
                break;
            }
        }
    }
    out
}

/// Huber-IRLS similarity (no reflection) taking `from` to `to` with variance-aware scale/rotation
/// regularization so narrow clusters or borderline points never inject sudden jitter into `target`.
fn similarity(from: &[(f32, f32)], to: &[(f32, f32)]) -> Option<(f32, f32, f32, f32)> {
    if from.len() < 2 || from.len() != to.len() {
        return None;
    }
    let fit_weighted = |w: &[f64]| -> Option<(f64, f64, f64, f64, f64)> {
        let w_sum: f64 = w.iter().copied().sum();
        if w_sum < 1.2 {
            return None;
        }
        let (mut fx, mut fy, mut tx, mut ty) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
        for i in 0..from.len() {
            let wi = w[i];
            if wi <= 0.0 {
                continue;
            }
            fx += wi * f64::from(from[i].0);
            fy += wi * f64::from(from[i].1);
            tx += wi * f64::from(to[i].0);
            ty += wi * f64::from(to[i].1);
        }
        let (fx, fy, tx, ty) = (fx / w_sum, fy / w_sum, tx / w_sum, ty / w_sum);
        let (mut num_a, mut num_b, mut den) = (0.0f64, 0.0f64, 0.0f64);
        for i in 0..from.len() {
            let wi = w[i];
            if wi <= 0.0 {
                continue;
            }
            let (ux, uy) = (f64::from(from[i].0) - fx, f64::from(from[i].1) - fy);
            let (vx, vy) = (f64::from(to[i].0) - tx, f64::from(to[i].1) - ty);
            num_a += wi * (ux * vx + uy * vy);
            num_b += wi * (ux * vy - uy * vx);
            den += wi * (ux * ux + uy * uy);
        }
        let (a, b) = if den < 36.0 {
            (1.0, 0.0)
        } else {
            let raw_a = num_a / den;
            let raw_b = num_b / den;
            let shrink = den / (den + 60.0);
            (1.0 + shrink * (raw_a - 1.0), shrink * raw_b)
        };
        Some((a, b, tx - (a * fx - b * fy), ty - (b * fx + a * fy), den))
    };

    let mut weights = vec![1.0f64; from.len()];
    let mut est = fit_weighted(&weights)?;
    for pass in 0..3 {
        let (a, b, tx, ty, _) = est;
        let res: Vec<f64> = from
            .iter()
            .zip(to)
            .map(|(f, t)| {
                let px = a * f64::from(f.0) - b * f64::from(f.1) + tx - f64::from(t.0);
                let py = b * f64::from(f.0) + a * f64::from(f.1) + ty - f64::from(t.1);
                px.hypot(py)
            })
            .collect();
        let mut sorted = res.clone();
        sorted.sort_by(|x, y| x.total_cmp(y));
        let med = sorted[sorted.len() / 2];
        let huber = (med * 1.8).clamp(0.45, 2.5);
        let hard_cut = if pass == 2 { (med * 2.8).max(1.2) } else { (med * 3.5).max(1.8) };
        for (wi, &r) in weights.iter_mut().zip(&res) {
            *wi = if r > hard_cut {
                0.0
            } else if r <= huber {
                1.0
            } else {
                huber / r
            };
        }
        if let Some(next_est) = fit_weighted(&weights) {
            est = next_est;
        } else {
            break;
        }
    }
    let (a, b, tx, ty, _) = est;
    Some((a as f32, b as f32, tx as f32, ty as f32))
}

pub(crate) fn disparity(left: &Gray, right: &Gray, pt: (f32, f32), radius: i32, max_disp: f32) -> f32 {
    let r = radius.clamp(3, 9);
    let score = |d: f32| -> Option<f32> {
        let (mut sum, mut n) = (0.0f32, 0usize);
        for dy in -r..=r {
            for dx in -r..=r {
                let (x, y) = (pt.0 + dx as f32, pt.1 + dy as f32);
                if let (Some(a), Some(b)) = (left.at(x, y), right.at(x - d, y)) {
                    let e = a - b;
                    sum += e * e;
                    n += 1;
                }
            }
        }
        (n > 0).then(|| sum / n as f32)
    };
    let (mut best, mut best_score) = (0.0f32, f32::MAX);
    let mut d = 0.0;
    while d <= max_disp {
        if let Some(s) = score(d) {
            if s < best_score {
                (best_score, best) = (s, d);
            }
        }
        d += 1.0;
    }
    // Sub-pixel: the parabola through the best match and its neighbours.
    let refined = match (score((best - 1.0).max(0.0)), score(best), score((best + 1.0).min(max_disp))) {
        (Some(l), Some(c), Some(r)) if l + r - 2.0 * c > 1e-9 => best + (0.5 * (l - r) / (l + r - 2.0 * c)).clamp(-0.5, 0.5),
        _ => best,
    };
    (refined / max_disp.max(1.0)).clamp(0.0, 1.0)
}

/// Maximum active feature count maintained around the tracked target.
const MAX_TRACK_FEATS: usize = 24;
const TOPUP_BELOW_FEATS: usize = 16;

/// A streaming tracker: build it on the first frame, then [`Tracker::push`] (or
/// [`Tracker::push_rgba`] for zero-CPU-copy GPU pyramid construction) each next frame.
/// When [`gpu_backend`] is registered, pyramid building, corner detection, forward-backward
/// pyramidal IC-LK, coarse ZNCC recovery, and stereo disparity execute completely on the GPU.
pub struct Tracker {
    prev: Vec<Level>,
    gpu_prev_slot: Option<usize>,
    feats: Vec<(f32, f32)>,
    anchor_pos: (f32, f32),
    anchor_tpl: Option<Template>,
    /// The target in level-0 view pixels.
    target: (f32, f32),
    radius: i32,
    reach: f32,
    levels: usize,
    scale: f32,
    rotation: f32,
    velocity: (f32, f32),
    max_disp: f32,
    lost: usize,
}

impl Tracker {
    /// Initialize directly from an RGBA8 frame on the GPU when a GPU backend is registered.
    pub fn new_rgba(
        rgba: &[u8],
        src_w: usize,
        src_h: usize,
        crop: (usize, usize, usize, usize),
        target: (f32, f32),
        patch: usize,
        max_disp: f32,
    ) -> Self {
        let vw = crop.2.max(1);
        let vh = crop.3.max(1);
        let radius = (patch.clamp(7, 41) as i32) / 2;
        let levels = ((vw.min(vh) as f32 / patch.clamp(7, 41) as f32).log2().floor().max(0.0) as usize).clamp(1, 4);
        let reach = (radius as f32 * 4.5).max(28.0);
        if let Some(gpu) = gpu_backend()
            && let Some(first_gray) = gpu.build_rgba_pyramid(0, rgba, src_w, src_h, crop, (vw, vh), levels)
        {
            let feats = Self::seed_with_gpu(Some((gpu, 0)), &first_gray, target, reach, radius, &[], MAX_TRACK_FEATS);
            let anchor_tpl = Self::extract_anchor_template(&first_gray, target, radius);
            return Self {
                prev: Vec::new(),
                gpu_prev_slot: Some(0),
                feats,
                anchor_pos: target,
                anchor_tpl,
                target,
                radius,
                reach,
                levels,
                scale: 1.0,
                rotation: 0.0,
                velocity: (0.0, 0.0),
                max_disp: max_disp.max(1.0),
                lost: 0,
            };
        }
        let g = Gray::from_rgba8(rgba, src_w, src_h);
        let left = crop_gray(&g, crop.0, crop.1, vw, vh);
        Self::new(&left, target, patch, max_disp)
    }

    /// `patch` is the full window width in view pixels; `max_disp` only matters for stereo.
    pub fn new(first: &Gray, target: (f32, f32), patch: usize, max_disp: f32) -> Self {
        let radius = (patch.clamp(7, 41) as i32) / 2;
        let levels = ((first.w.min(first.h) as f32 / patch.clamp(7, 41) as f32).log2().floor().max(0.0) as usize).clamp(1, 4);
        let reach = (radius as f32 * 4.5).max(28.0);
        let gpu_prev_slot = gpu_backend().and_then(|gpu| gpu.upload_gray_pyramid(0, first, levels).then_some(0));
        let prev = if gpu_prev_slot.is_some() { Vec::new() } else { first.pyramid(levels) };
        let gpu_ctx = gpu_backend().zip(gpu_prev_slot);
        let feats = Self::seed_with_gpu(gpu_ctx, first, target, reach, radius, &[], MAX_TRACK_FEATS);
        let anchor_tpl = Self::extract_anchor_template(first, target, radius);
        Self {
            prev,
            gpu_prev_slot,
            feats,
            anchor_pos: target,
            anchor_tpl,
            target,
            radius,
            reach,
            levels,
            scale: 1.0,
            rotation: 0.0,
            velocity: (0.0, 0.0),
            max_disp: max_disp.max(1.0),
            lost: 0,
        }
    }

    fn extract_anchor_template(img: &Gray, target: (f32, f32), radius: i32) -> Option<Template> {
        let r_win = (radius + 2) as usize;
        let x0 = (target.0.floor() as isize - r_win as isize).max(0) as usize;
        let y0 = (target.1.floor() as isize - r_win as isize).max(0) as usize;
        let x1 = ((target.0.ceil() as usize) + r_win + 1).min(img.w);
        let y1 = ((target.1.ceil() as usize) + r_win + 1).min(img.h);
        if x1 <= x0 + 4 || y1 <= y0 + 4 {
            return None;
        }
        let sub = crop_gray(img, x0, y0, x1 - x0, y1 - y0);
        let (ix, iy) = sub.gradients();
        let lvl = Level { img: sub, ix, iy };
        template(&lvl, target.0 - x0 as f32, target.1 - y0 as f32, radius)
    }

    fn seed_with_gpu(
        gpu_ctx: Option<(&'static dyn GpuTrackBackend, usize)>,
        img: &Gray,
        target: (f32, f32),
        reach: f32,
        radius: i32,
        existing: &[(f32, f32)],
        want_total: usize,
    ) -> Vec<(f32, f32)> {
        let apart = (radius as f32 * 0.85).max(4.0);
        let mut out: Vec<(f32, f32)> = existing.to_vec();
        if out.is_empty()
            && Self::extract_anchor_template(img, target, radius).is_some()
        {
            out.push(target);
        }
        if out.len() >= want_total {
            return out;
        }
        let cand: Vec<(f32, f32)> = if let Some((gpu, slot)) = gpu_ctx {
            let x0 = (target.0 - reach).floor().max(2.0) as usize;
            let y0 = (target.1 - reach).floor().max(2.0) as usize;
            let x1 = ((target.0 + reach).ceil() as usize).min(img.w.saturating_sub(2));
            let y1 = ((target.1 + reach).ceil() as usize).min(img.h.saturating_sub(2));
            if x1 > x0 + 4 && y1 > y0 + 4 {
                if let Some(mut gpu_cands) = gpu.detect_corners(slot, (x0, y0, x1, y1), 4) {
                    gpu_cands.retain(|&(_, px, py)| (px - target.0).hypot(py - target.1) <= reach);
                    gpu_cands.sort_by(|a, b| b.0.total_cmp(&a.0));
                    gpu_cands.into_iter().map(|(_, px, py)| (px, py)).collect()
                } else {
                    corners(img, target.0, target.1, reach, apart, want_total)
                }
            } else {
                Vec::new()
            }
        } else {
            corners(img, target.0, target.1, reach, apart, want_total)
        };

        for (px, py) in cand {
            if out.len() >= want_total {
                break;
            }
            if out.iter().all(|&(qx, qy)| (qx - px).hypot(qy - py) >= apart) {
                out.push((px, py));
            }
        }
        out
    }

    /// The first sample: the anchor everything else is relative to.
    pub fn anchor(&self) -> TrackPoint {
        TrackPoint { x: self.target.0, y: self.target.1, z: 0.0, scale: 1.0, rotation: 0.0, confidence: 1.0 }
    }

    /// True when the initial region contains trackable spatial gradients/corners.
    pub fn has_features(&self) -> bool {
        !self.feats.is_empty() || self.anchor_tpl.is_some()
    }

    /// Track every feature on the CPU with a starting guess, keeping forward-backward consistent matches.
    fn match_all_cpu(&self, next: &[Level], guess: (f32, f32)) -> (Vec<(f32, f32)>, Vec<(f32, f32)>) {
        let (mut from, mut to) = (Vec::new(), Vec::new());
        for &p in &self.feats {
            let Some(q) = track_point(&self.prev, next, p, self.radius, guess) else { continue };
            let Some(back) = track_point(next, &self.prev, q, self.radius, (p.0 - q.0, p.1 - q.1)) else { continue };
            if (back.0 - p.0).hypot(back.1 - p.1) > 1.0 {
                continue;
            }
            from.push(p);
            to.push(q);
        }
        (from, to)
    }

    /// Track every feature on the GPU between `prev_slot` and `curr_slot`.
    fn match_all_gpu(
        &self,
        gpu: &'static dyn GpuTrackBackend,
        prev_slot: usize,
        curr_slot: usize,
        guess: (f32, f32),
    ) -> Option<(Vec<(f32, f32)>, Vec<(f32, f32)>)> {
        if self.feats.is_empty() {
            return Some((Vec::new(), Vec::new()));
        }
        let queries: Vec<GpuTrackQuery> = self.feats.iter().map(|&pt| GpuTrackQuery { pt, guess }).collect();
        let tracked = gpu.track_batch_fb(prev_slot, curr_slot, &queries, self.radius, 1.0)?;
        let (mut from, mut to) = (Vec::with_capacity(self.feats.len()), Vec::with_capacity(self.feats.len()));
        for (&p, res) in self.feats.iter().zip(tracked) {
            if let Some(q) = res {
                from.push(p);
                to.push(q);
            }
        }
        Some((from, to))
    }

    /// Stream an RGBA8 frame directly into the GPU tracker (with optional stereo right-eye crop).
    pub fn push_rgba(
        &mut self,
        rgba: &[u8],
        src_w: usize,
        src_h: usize,
        left_crop: (usize, usize, usize, usize),
        right_crop: Option<(usize, usize, usize, usize)>,
    ) -> TrackPoint {
        let (vw, vh) = (left_crop.2.max(1), left_crop.3.max(1));
        if let (Some(gpu), Some(prev_slot)) = (gpu_backend(), self.gpu_prev_slot) {
            let curr_slot = 1 - (prev_slot & 1);
            if let Some(left_gray) = gpu.build_rgba_pyramid(curr_slot, rgba, src_w, src_h, left_crop, (vw, vh), self.levels) {
                let right_slot = right_crop.and_then(|rc| {
                    gpu.build_rgba_pyramid(2, rgba, src_w, src_h, rc, (rc.2.max(1), rc.3.max(1)), 1).map(|_| 2usize)
                });
                return self.finish_step_gpu(gpu, prev_slot, curr_slot, &left_gray, right_slot);
            }
        }
        let g = Gray::from_rgba8(rgba, src_w, src_h);
        let left = crop_gray(&g, left_crop.0, left_crop.1, vw, vh);
        let right = right_crop.map(|rc| crop_gray(&g, rc.0, rc.1, rc.2.max(1), rc.3.max(1)));
        self.push(&left, right.as_ref())
    }

    fn apply_motion_update(&mut self, frame: &Gray, from: &[(f32, f32)], to: &[(f32, f32)]) -> Vec<(f32, f32)> {
        if let Some((a, b, tx, ty)) = similarity(from, to) {
            let (x, y) = self.target;
            let mut nx = a * x - b * y + tx;
            let mut ny = b * x + a * y + ty;
            let s = (a * a + b * b).sqrt();
            if s.is_finite() && s > 0.05 && s < 20.0 {
                if (s - 1.0).abs() >= 0.0012 {
                    self.scale *= s;
                }
                let dtheta = b.atan2(a).to_degrees();
                if dtheta.abs() >= 0.035 {
                    self.rotation += dtheta;
                }
            }
            // Gentle sub-pixel anchor lock against Frame-0 template when scale & rotation remain near
            // unity, preventing cumulative multi-second random-walk drift.
            if (self.scale - 1.0).abs() < 0.09
                && self.rotation.abs() < 6.5
                && let Some(tpl) = &self.anchor_tpl
                && let Some((refined, err)) = lk_level(frame, tpl, self.anchor_pos.0, self.anchor_pos.1, (nx - self.anchor_pos.0, ny - self.anchor_pos.1))
                && err < 0.075
                && (refined.0 - nx).hypot(refined.1 - ny) < 2.2
            {
                nx = 0.65 * nx + 0.35 * refined.0;
                ny = 0.65 * ny + 0.35 * refined.1;
            }
            self.target = (nx, ny);
            self.velocity = (nx - x, ny - y);
            self.lost = 0;
        } else if from.len() == 1 && to.len() == 1 {
            let dx = to[0].0 - from[0].0;
            let dy = to[0].1 - from[0].1;
            self.target = (self.target.0 + dx, self.target.1 + dy);
            self.velocity = (dx, dy);
            self.lost = 0;
        } else {
            self.lost += 1;
        }
        let max_keep_r = self.reach * 1.35;
        to.iter()
            .copied()
            .filter(|&(px, py)| {
                px >= 2.0
                    && py >= 2.0
                    && px < frame.w as f32 - 2.0
                    && py < frame.h as f32 - 2.0
                    && (px - self.target.0).hypot(py - self.target.1) <= max_keep_r
            })
            .collect()
    }

    fn finish_step_gpu(
        &mut self,
        gpu: &'static dyn GpuTrackBackend,
        prev_slot: usize,
        curr_slot: usize,
        frame: &Gray,
        right_slot: Option<usize>,
    ) -> TrackPoint {
        let prev_count = self.feats.len();
        let (mut from, mut to) = self.match_all_gpu(gpu, prev_slot, curr_slot, self.velocity).unwrap_or_default();
        if to.len() * 2 < prev_count
            && self.velocity.0.hypot(self.velocity.1) > 1.5
            && let Some((fz, tz)) = self.match_all_gpu(gpu, prev_slot, curr_slot, (0.0, 0.0))
            && tz.len() > to.len()
        {
            (from, to) = (fz, tz);
        }
        if to.len() < 3.max(prev_count / 4)
            && let Some(g) = gpu.coarse_search(prev_slot, curr_slot, self.target, self.radius, self.reach * 1.6)
            && let Some((f2, t2)) = self.match_all_gpu(gpu, prev_slot, curr_slot, g)
            && t2.len() > to.len()
        {
            (from, to) = (f2, t2);
        }
        let confidence: f32 = if prev_count == 0 { 0.0 } else { to.len() as f32 / prev_count as f32 };
        let kept = self.apply_motion_update(frame, &from, &to);
        self.feats = if kept.len() < TOPUP_BELOW_FEATS {
            Self::seed_with_gpu(Some((gpu, curr_slot)), frame, self.target, self.reach, self.radius, &kept, MAX_TRACK_FEATS)
        } else {
            kept
        };
        self.gpu_prev_slot = Some(curr_slot);
        let inside = self.target.0 >= 0.0 && self.target.1 >= 0.0 && self.target.0 < frame.w as f32 && self.target.1 < frame.h as f32;
        let conf = if inside && self.lost == 0 { confidence.clamp(0.0, 1.0) } else { 0.0 };
        let z = right_slot
            .and_then(|rs| gpu.stereo_disparity_batch(curr_slot, rs, &[self.target], self.radius.min(7), self.max_disp))
            .and_then(|v| v.into_iter().next().flatten())
            .unwrap_or(0.0);
        TrackPoint { x: self.target.0, y: self.target.1, z, scale: self.scale, rotation: self.rotation, confidence: conf }
    }

    /// Track into `frame` (and its rectified `right` view, for depth).
    pub fn push(&mut self, frame: &Gray, right: Option<&Gray>) -> TrackPoint {
        if let (Some(gpu), Some(prev_slot)) = (gpu_backend(), self.gpu_prev_slot) {
            let curr_slot = 1 - (prev_slot & 1);
            if gpu.upload_gray_pyramid(curr_slot, frame, self.levels) {
                let right_slot = right.and_then(|r| gpu.upload_gray_pyramid(2, r, 1).then_some(2usize));
                return self.finish_step_gpu(gpu, prev_slot, curr_slot, frame, right_slot);
            }
            // Fallback seamlessly to CPU if GPU device lost mid-stream
            self.gpu_prev_slot = None;
        }
        if self.prev.is_empty() {
            self.prev = frame.pyramid(self.levels);
        }
        let prev_count = self.feats.len();
        let next = frame.pyramid(self.levels);
        let (mut from, mut to) = self.match_all_cpu(&next, self.velocity);
        if to.len() * 2 < prev_count && self.velocity.0.hypot(self.velocity.1) > 1.5 {
            let (fz, tz) = self.match_all_cpu(&next, (0.0, 0.0));
            if tz.len() > to.len() {
                (from, to) = (fz, tz);
            }
        }
        if to.len() < 3.max(prev_count / 4)
            && let Some(g) = search(&self.prev, &next, self.target, self.radius, self.reach * 1.6)
        {
            let (f2, t2) = self.match_all_cpu(&next, g);
            if t2.len() > to.len() {
                (from, to) = (f2, t2);
            }
        }
        let confidence: f32 = if prev_count == 0 { 0.0 } else { to.len() as f32 / prev_count as f32 };
        let kept = self.apply_motion_update(frame, &from, &to);
        self.feats = if kept.len() < TOPUP_BELOW_FEATS {
            Self::seed_with_gpu(None, frame, self.target, self.reach, self.radius, &kept, MAX_TRACK_FEATS)
        } else {
            kept
        };
        self.prev = next;
        let inside = self.target.0 >= 0.0 && self.target.1 >= 0.0 && self.target.0 < frame.w as f32 && self.target.1 < frame.h as f32;
        let conf = if inside && self.lost == 0 { confidence.clamp(0.0, 1.0) } else { 0.0 };
        let z = right.map(|r| disparity(frame, r, self.target, self.radius.min(7), self.max_disp)).unwrap_or(0.0);
        TrackPoint { x: self.target.0, y: self.target.1, z, scale: self.scale, rotation: self.rotation, confidence: conf }
    }
}

/// Zero-phase, confidence-weighted, motion-adaptive temporal filter for [`filmcraft_project::TrackSample`]
/// trajectories. Eliminates high-frequency sub-pixel / macroblock shake without introducing phase
/// lag or smearing intentional fast pans and stops.
pub fn smooth_track_samples(samples: &mut [filmcraft_project::TrackSample]) {
    let n = samples.len();
    if n < 3 {
        return;
    }
    let orig = samples.to_vec();
    // Estimate per-frame velocity magnitude to adapt smoothing strength: steady/slow segments get
    // strong jitter suppression while rapid whip-pans preserve crisp responsiveness.
    let mut speed = vec![0.0f32; n];
    for i in 0..n {
        let i0 = i.saturating_sub(1);
        let i1 = (i + 1).min(n - 1);
        let dt = (i1 - i0).max(1) as f32;
        speed[i] = ((orig[i1].x - orig[i0].x).hypot(orig[i1].y - orig[i0].y)) / dt;
    }

    for (i, out) in samples.iter_mut().enumerate() {
        if i == 0 || orig[i].confidence <= 0.0 {
            // Preserve the exact Frame-0 anchor so relative follow_track delta starts at (0, 0).
            continue;
        }
        // Bilateral temporal kernel over [i - 3 ..= i + 3] for position, [i - 4 ..= i + 4] for
        // scale/rotation/depth (which are far more visually sensitive to micro-oscillation).
        let sp = speed[i];
        let sigma_t_xy: f32 = if sp > 0.035 { 0.85 } else if sp > 0.012 { 1.25 } else { 1.75 };
        let (mut wx_sum, mut sx, mut sy) = (0.0f32, 0.0f32, 0.0f32);
        let j0 = i.saturating_sub(3);
        let j1 = (i + 3).min(n - 1);
        for (j, sj) in orig.iter().enumerate().take(j1 + 1).skip(j0) {
            if sj.confidence <= 0.0 && j != i {
                continue;
            }
            let dt = (j as f32 - i as f32).abs();
            let wt = (-0.5 * (dt / sigma_t_xy).powi(2)).exp();
            // Linear prediction from j to i using local velocity prevents curvature bias on constant-speed pans!
            let vel_x = if j > 0 && j + 1 < n { (orig[j + 1].x - orig[j - 1].x) * 0.5 } else { 0.0 };
            let vel_y = if j > 0 && j + 1 < n { (orig[j + 1].y - orig[j - 1].y) * 0.5 } else { 0.0 };
            let pred_x = sj.x + vel_x * (i as f32 - j as f32);
            let pred_y = sj.y + vel_y * (i as f32 - j as f32);
            let dev = (pred_x - orig[i].x).hypot(pred_y - orig[i].y);
            // Bilateral range gate: reject across sharp motion discontinuities / direction reversals.
            let wr = (-0.5 * (dev / 0.012f32).powi(2)).exp();
            let w = wt * (0.25 + 0.75 * wr) * sj.confidence.max(0.15);
            wx_sum += w;
            sx += w * pred_x;
            sy += w * pred_y;
        }
        if wx_sum > 1e-6 {
            out.x = (sx / wx_sum).clamp(0.0, 1.0);
            out.y = (sy / wx_sum).clamp(0.0, 1.0);
        }

        let (mut wz_sum, mut sz, mut ssc, mut srot_sin, mut srot_cos) = (0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32);
        let k0 = i.saturating_sub(4);
        let k1 = (i + 4).min(n - 1);
        for (k, sk) in orig.iter().enumerate().take(k1 + 1).skip(k0) {
            if sk.confidence <= 0.0 && k != i {
                continue;
            }
            let dt = (k as f32 - i as f32).abs();
            let w = (-0.5 * (dt / 2.1f32).powi(2)).exp() * sk.confidence.max(0.15);
            wz_sum += w;
            sz += w * sk.z;
            ssc += w * sk.scale.max(0.01).ln();
            let rad = sk.rotation.to_radians();
            srot_sin += w * rad.sin();
            srot_cos += w * rad.cos();
        }
        if wz_sum > 1e-6 {
            out.z = sz / wz_sum;
            out.scale = (ssc / wz_sum).exp();
            // Unwrap smoothed circular angle around orig[i].rotation to preserve >180 deg multi-turn spins.
            let base_rad = orig[i].rotation.to_radians();
            let mean_ang = srot_sin.atan2(srot_cos);
            let mut diff = mean_ang - base_rad;
            while diff > std::f32::consts::PI {
                diff -= 2.0 * std::f32::consts::PI;
            }
            while diff < -std::f32::consts::PI {
                diff += 2.0 * std::f32::consts::PI;
            }
            out.rotation = (base_rad + diff).to_degrees();
        }
    }
}

/// Track `start` through `left` frames. `right` (same size, rectified) enables stereo depth.
/// The first sample is the anchor: scale 1, rotation 0. A wrapper over [`Tracker`] for callers
/// that already hold every frame; the mobile bridge streams instead.
pub fn track_sequence(left: &[Gray], right: Option<&[Gray]>, start: (f32, f32), patch: usize, max_disp: f32) -> Vec<TrackPoint> {
    let Some(first) = left.first() else { return Vec::new() };
    let mut t = Tracker::new(first, start, patch, max_disp);
    let mut out = vec![t.anchor()];
    for (i, frame) in left.iter().enumerate().skip(1) {
        out.push(t.push(frame, right.and_then(|r| r.get(i))));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash01(x: i32, y: i32) -> f32 {
        let h = (x.wrapping_mul(374_761_393).wrapping_add(y.wrapping_mul(668_265_263))) as u32;
        let h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
        ((h ^ (h >> 16)) & 0xff_ffff) as f32 / 0xff_ffff as f32
    }

    /// Smooth value noise: texture with corners everywhere and no repeating period, which is
    /// what footage looks like. A sinusoid grid would be ambiguous half a cell away.
    fn vnoise(x: f32, y: f32, cell: f32) -> f32 {
        let (gx, gy) = ((x / cell).floor(), (y / cell).floor());
        let (mut fx, mut fy) = (x / cell - gx, y / cell - gy);
        fx = fx * fx * (3.0 - 2.0 * fx);
        fy = fy * fy * (3.0 - 2.0 * fy);
        let (a, b) = (hash01(gx as i32, gy as i32), hash01(gx as i32 + 1, gy as i32));
        let (c, d) = (hash01(gx as i32, gy as i32 + 1), hash01(gx as i32 + 1, gy as i32 + 1));
        a * (1.0 - fx) * (1.0 - fy) + b * fx * (1.0 - fy) + c * (1.0 - fx) * fy + d * fx * fy
    }

    fn pattern(x: f32, y: f32) -> f32 {
        0.62 * vnoise(x, y, 11.0) + 0.28 * vnoise(x + 37.0, y - 19.0, 5.0) + 0.10 * vnoise(x - 71.0, y + 53.0, 2.5)
    }

    /// The pattern moved by (dx, dy), grown by `scale` and turned by `rot` about the centre.
    fn scene(dx: f32, dy: f32, scale: f32, rot: f32) -> Gray {
        let (w, h) = (160usize, 120usize);
        let mut g = Gray { w, h, p: vec![0.0; w * h] };
        let (c, s) = (rot.to_radians().cos(), rot.to_radians().sin());
        for y in 0..h {
            for x in 0..w {
                let (ux, uy) = (x as f32 - 80.0 - dx, y as f32 - 60.0 - dy);
                let (px, py) = ((c * ux + s * uy) / scale + 80.0, (-s * ux + c * uy) / scale + 60.0);
                g.p[y * w + x] = pattern(px, py).clamp(0.0, 1.0);
            }
        }
        g
    }

    #[test]
    fn follows_a_translating_texture() {
        let frames: Vec<Gray> = (0..3).map(|i| scene(i as f32 * 2.0, i as f32, 1.0, 0.0)).collect();
        let t = track_sequence(&frames, None, (80.0, 60.0), 15, 16.0);
        assert_eq!(t.len(), 3);
        assert!((t[2].x - 84.0).abs() < 1.5, "{t:?}");
        assert!((t[2].y - 62.0).abs() < 1.5, "{t:?}");
        assert!(t[2].confidence > 0.3, "{t:?}");
    }

    /// The case a single-level solver cannot do: more than half a patch of motion per frame.
    #[test]
    fn follows_fast_motion_across_the_frame() {
        let frames: Vec<Gray> = (0..6).map(|i| scene(i as f32 * 9.0, i as f32 * -5.0, 1.0, 0.0)).collect();
        let t = track_sequence(&frames, None, (60.0, 80.0), 15, 16.0);
        let last = t.last().unwrap();
        assert!((last.x - 105.0).abs() < 3.0, "x drifted: {t:?}");
        assert!((last.y - 55.0).abs() < 3.0, "y drifted: {t:?}");
    }

    #[test]
    fn recovers_scale_and_rotation() {
        let frames: Vec<Gray> = (0..5).map(|i| scene(0.0, 0.0, 1.0 + i as f32 * 0.04, i as f32 * 2.0)).collect();
        let t = track_sequence(&frames, None, (80.0, 60.0), 15, 16.0);
        let last = t.last().unwrap();
        assert!((last.scale - 1.16).abs() < 0.08, "scale was {}: {t:?}", last.scale);
        assert!((last.rotation - 8.0).abs() < 3.0, "rotation was {}: {t:?}", last.rotation);
    }

    /// A flat wall has nothing to follow: say so instead of inventing a path.
    #[test]
    fn reports_no_confidence_on_a_featureless_picture() {
        let flat = Gray { w: 96, h: 96, p: vec![0.5; 96 * 96] };
        let t = track_sequence(&[flat.clone(), flat.clone(), flat], None, (48.0, 48.0), 15, 16.0);
        assert!(t.iter().skip(1).all(|p| p.confidence == 0.0), "{t:?}");
    }

    /// 40 frames of changing motion: re-seeding and the fractional template must keep the error
    /// from accumulating. The anchor is a pixel in frame 0, which is itself already offset.
    #[test]
    fn a_long_track_does_not_drift_away() {
        let frames: Vec<Gray> = (0..40).map(|i| scene((i as f32 * 1.5).sin() * 12.0, (i as f32 * 0.9).cos() * 8.0, 1.0, 0.0)).collect();
        let t = track_sequence(&frames, None, (80.0, 60.0), 15, 16.0);
        let last = t.last().unwrap();
        let want = (80.0 + (39.0f32 * 1.5).sin() * 12.0, 60.0 + (39.0f32 * 0.9).cos() * 8.0 - 8.0);
        assert!((last.x - want.0).abs() < 2.0 && (last.y - want.1).abs() < 2.0, "ended at ({}, {}), wanted {want:?}", last.x, last.y);
    }

    #[test]
    fn suppresses_flicker_and_sensor_shake_without_lag() {
        let mut rng: u64 = 0x5EA1_E001;
        let frames: Vec<Gray> = (0..24)
            .map(|i| {
                let mut g = scene(i as f32 * 1.5, i as f32 * -0.75, 1.0, 0.0);
                let gain = 1.0 + 0.16 * (i as f32 * 2.3).sin();
                let bias = 0.06 * (i as f32 * 1.7).cos();
                for px in &mut g.p {
                    rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
                    let noise = (((rng >> 40) as f32 / ((1u32 << 24) as f32)) - 0.5) * 0.03;
                    *px = (*px * gain + bias + noise).clamp(0.0, 1.0);
                }
                g
            })
            .collect();
        let raw = track_sequence(&frames, None, (80.0, 60.0), 17, 16.0);
        let mut samples: Vec<filmcraft_project::TrackSample> = raw
            .iter()
            .enumerate()
            .map(|(i, p)| filmcraft_project::TrackSample {
                time: filmcraft_time::Tick(i as i64 * 1000),
                x: p.x / 160.0,
                y: p.y / 120.0,
                z: p.z,
                scale: p.scale,
                rotation: p.rotation,
                confidence: p.confidence,
            })
            .collect();
        smooth_track_samples(&mut samples);
        for (i, s) in samples.iter().enumerate() {
            let want_x = (80.0 + i as f32 * 1.5) / 160.0;
            let want_y = (60.0 - i as f32 * 0.75) / 120.0;
            assert!((s.x - want_x).abs() * 160.0 < 0.75, "frame {i} x error={} px", (s.x - want_x) * 160.0);
            assert!((s.y - want_y).abs() * 120.0 < 0.75, "frame {i} y error={} px", (s.y - want_y) * 120.0);
            assert!((s.scale - 1.0).abs() < 0.02, "spurious scale shake on frame {i}: {}", s.scale);
            assert!(s.rotation.abs() < 0.6, "spurious rotation shake on frame {i}: {}", s.rotation);
        }
    }
}
