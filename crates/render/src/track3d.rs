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
    Some(
        p[y0 * w + x0] * (1.0 - fx) * (1.0 - fy) + p[y0 * w + x1] * fx * (1.0 - fy) + p[y1 * w + x0] * (1.0 - fx) * fy + p[y1 * w + x1] * fx * fy,
    )
}

impl Gray {
    pub fn from_rgba8(rgba: &[u8], w: usize, h: usize) -> Self {
        let mut p = vec![0.0; w * h];
        for (i, px) in rgba.chunks_exact(4).enumerate().take(w * h) {
            p[i] = (0.2126 * px[0] as f32 + 0.7152 * px[1] as f32 + 0.0722 * px[2] as f32) / 255.0;
        }
        Self { w, h, p }
    }

    fn at(&self, x: f32, y: f32) -> Option<f32> {
        sample(&self.p, self.w, self.h, x, y)
    }

    /// Half size, low-passed first so the coarse levels do not alias (a 5-tap binomial).
    fn half(&self) -> Gray {
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
    fn pyramid(&self, levels: usize) -> Vec<Level> {
        let mut imgs = vec![self.clone()];
        while imgs.len() < levels {
            let last = imgs.last().unwrap();
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

    /// Central differences, zero at the border.
    fn gradients(&self) -> (Vec<f32>, Vec<f32>) {
        let (mut ix, mut iy) = (vec![0.0; self.w * self.h], vec![0.0; self.w * self.h]);
        for y in 1..self.h.saturating_sub(1) {
            for x in 1..self.w.saturating_sub(1) {
                let i = y * self.w + x;
                ix[i] = (self.p[i + 1] - self.p[i - 1]) * 0.5;
                iy[i] = (self.p[i + self.w] - self.p[i - self.w]) * 0.5;
            }
        }
        (ix, iy)
    }
}

/// One pyramid level: the picture and its gradients.
struct Level {
    img: Gray,
    ix: Vec<f32>,
    iy: Vec<f32>,
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

/// The template of one feature at one level: values and gradients sampled once at the point's
/// exact (fractional) place, and the inverse Hessian of the Lucas–Kanade system. Snapping the
/// template to whole pixels while the point stays fractional biases every frame by up to half a
/// pixel, which a long track turns into a visible slide.
struct Template {
    dx: Vec<f32>,
    dy: Vec<f32>,
    v: Vec<f32>,
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
    let (mut t, mut gxx, mut gyy, mut gxy) = (
        Template { dx: Vec::new(), dy: Vec::new(), v: Vec::new(), ix: Vec::new(), iy: Vec::new(), h: [0.0; 4] },
        0.0f32,
        0.0f32,
        0.0f32,
    );
    for oy in -r..=r {
        for ox in -r..=r {
            let (sx, sy) = (x + ox as f32, y + oy as f32);
            let a = sample(&l.ix, img.w, img.h, sx, sy)?;
            let b = sample(&l.iy, img.w, img.h, sx, sy)?;
            t.dx.push(ox as f32);
            t.dy.push(oy as f32);
            t.v.push(img.at(sx, sy)?);
            t.ix.push(a);
            t.iy.push(b);
            gxx += a * a;
            gyy += b * b;
            gxy += a * b;
        }
    }
    let det = gxx * gyy - gxy * gxy;
    // Shi–Tomasi: the smaller eigenvalue of the structure tensor, per pixel. OpenCV's
    // minEigThreshold is 1e-4 on the same quantity; flat and edge-only patches are unusable.
    let tr = gxx + gyy;
    let min_eig = (tr - (tr * tr - 4.0 * det).max(0.0).sqrt()) * 0.5 / count;
    if det.abs() < 1e-8 || min_eig < 1e-4 {
        return None;
    }
    t.h = [gyy / det, -gxy / det, -gxy / det, gxx / det];
    Some(t)
}

/// One pyramid level of Lucas–Kanade: where does the template at (`px`, `py`) in `prev` land in
/// `next`, starting from `guess`? Returns the new point and the mean absolute patch error.
fn lk_level(next: &Gray, t: &Template, px: f32, py: f32, guess: (f32, f32)) -> Option<((f32, f32), f32)> {
    let (mut gx, mut gy) = guess;
    for _ in 0..20 {
        let (mut bx, mut by) = (0.0f32, 0.0f32);
        for k in 0..t.v.len() {
            let it = next.at(px + t.dx[k] + gx, py + t.dy[k] + gy)? - t.v[k];
            bx += t.ix[k] * it;
            by += t.iy[k] * it;
        }
        let vx = -(t.h[0] * bx + t.h[1] * by);
        let vy = -(t.h[2] * bx + t.h[3] * by);
        // A step this large means the linearisation has stopped meaning anything.
        if !vx.is_finite() || !vy.is_finite() || vx.abs() > 16.0 || vy.abs() > 16.0 {
            return None;
        }
        gx += vx;
        gy += vy;
        if vx.abs() + vy.abs() < 0.01 {
            break;
        }
    }
    let out = (px + gx, py + gy);
    next.at(out.0, out.1)?;
    let mut err = 0.0f32;
    for k in 0..t.v.len() {
        err += (next.at(px + t.dx[k] + gx, py + t.dy[k] + gy)? - t.v[k]).abs();
    }
    Some((out, err / t.v.len() as f32))
}

/// Pyramidal track of one point from `prev` to `next`, coarse to fine, starting from `initial`
/// (a velocity or a correlation match, in level-0 pixels).
fn track_point(prev: &[Level], next: &[Level], p: (f32, f32), r: i32, initial: (f32, f32)) -> Option<(f32, f32)> {
    let levels = prev.len().min(next.len());
    if levels == 0 {
        return None;
    }
    let top = (1usize << (levels - 1)) as f32;
    let mut guess = (initial.0 / top, initial.1 / top);
    let mut point = p;
    for l in (0..levels).rev() {
        let s = (1usize << l) as f32;
        let (px, py) = (p.0 / s, p.1 / s);
        let t = template(&prev[l], px, py, r.max(2))?;
        let ((nx, ny), err) = lk_level(&next[l].img, &t, px, py, guess)?;
        // A patch that no longer looks like itself is not a match, however well it converged.
        if err > 0.22 {
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
fn corners(img: &Gray, x: f32, y: f32, reach: f32, apart: f32, want: usize) -> Vec<(f32, f32)> {
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
            if min_eig > 1e-3 {
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

/// Least-squares similarity (no reflection) taking `from` to `to`: the complex scale-rotation
/// a+bi and the translation that goes with it, in the same coordinates. One round of outlier
/// rejection drops points that disagree with the majority, so a corner that slipped onto the
/// background cannot pull the target with it.
fn similarity(from: &[(f32, f32)], to: &[(f32, f32)]) -> Option<(f32, f32, f32, f32)> {
    if from.len() < 2 || from.len() != to.len() {
        return None;
    }
    let fit = |keep: &[bool]| -> Option<(f32, f32, f32, f32)> {
        let n = keep.iter().filter(|k| **k).count();
        if n < 2 {
            return None;
        }
        let (mut fx, mut fy, mut tx, mut ty) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
        for i in 0..from.len() {
            if keep[i] {
                fx += f64::from(from[i].0);
                fy += f64::from(from[i].1);
                tx += f64::from(to[i].0);
                ty += f64::from(to[i].1);
            }
        }
        let (fx, fy, tx, ty) = (fx / n as f64, fy / n as f64, tx / n as f64, ty / n as f64);
        let (mut a, mut b, mut den) = (0.0f64, 0.0f64, 0.0f64);
        for i in 0..from.len() {
            if !keep[i] {
                continue;
            }
            let (ux, uy) = (f64::from(from[i].0) - fx, f64::from(from[i].1) - fy);
            let (vx, vy) = (f64::from(to[i].0) - tx, f64::from(to[i].1) - ty);
            a += ux * vx + uy * vy;
            b += ux * vy - uy * vx;
            den += ux * ux + uy * uy;
        }
        // Everything on top of itself: only a translation is observable.
        let (a, b) = if den < 1e-6 { (1.0, 0.0) } else { (a / den, b / den) };
        Some((a as f32, b as f32, (tx - (a * fx - b * fy)) as f32, (ty - (b * fx + a * fy)) as f32))
    };
    let all = vec![true; from.len()];
    let first = fit(&all)?;
    let (a, b, tx, ty) = first;
    let mut res: Vec<(usize, f32)> = from
        .iter()
        .zip(to)
        .enumerate()
        .map(|(i, (f, t))| (i, (a * f.0 - b * f.1 + tx - t.0).hypot(b * f.0 + a * f.1 + ty - t.1)))
        .collect();
    res.sort_by(|x, y| x.1.partial_cmp(&y.1).unwrap_or(std::cmp::Ordering::Equal));
    let cut = (res[res.len() / 2].1 * 2.5).max(1.0);
    let mut keep = vec![false; from.len()];
    for (i, r) in res {
        keep[i] = r <= cut;
    }
    fit(&keep).or(Some(first))
}

fn disparity(left: &Gray, right: &Gray, pt: (f32, f32), radius: i32, max_disp: f32) -> f32 {
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

/// How often the corner set is refreshed, in frames. Corners drift and leave the picture; a
/// fresh set around the current target keeps a long track honest.
const RESEED_EVERY: usize = 8;

/// A streaming tracker: build it on the first frame, then [`Tracker::push`] each next frame.
pub struct Tracker {
    prev: Vec<Level>,
    feats: Vec<(f32, f32)>,
    /// The target in level-0 view pixels.
    target: (f32, f32),
    radius: i32,
    reach: f32,
    levels: usize,
    since_seed: usize,
    scale: f32,
    rotation: f32,
    velocity: (f32, f32),
    max_disp: f32,
    lost: usize,
}

impl Tracker {
    /// `patch` is the full window width in view pixels; `max_disp` only matters for stereo.
    pub fn new(first: &Gray, target: (f32, f32), patch: usize, max_disp: f32) -> Self {
        let radius = (patch.clamp(7, 41) as i32) / 2;
        let levels = ((first.w.min(first.h) as f32 / patch.clamp(7, 41) as f32).log2().floor().max(0.0) as usize).clamp(1, 4);
        let prev = first.pyramid(levels);
        let reach = (radius as f32 * 4.0).max(24.0);
        let feats = Self::seed(first, target, reach, radius);
        Self {
            prev,
            feats,
            target,
            radius,
            reach,
            levels,
            since_seed: 0,
            scale: 1.0,
            rotation: 0.0,
            velocity: (0.0, 0.0),
            max_disp: max_disp.max(1.0),
            lost: 0,
        }
    }

    fn seed(img: &Gray, target: (f32, f32), reach: f32, radius: i32) -> Vec<(f32, f32)> {
        let apart = (radius as f32).max(4.0);
        let mut pts = corners(img, target.0, target.1, reach, apart, 16);
        // The tapped point itself, if it carries any texture at all: the user asked for it.
        let (ix, iy) = img.gradients();
        let level = Level { img: img.clone(), ix, iy };
        if template(&level, target.0, target.1, radius).is_some() && pts.iter().all(|p| (p.0 - target.0).hypot(p.1 - target.1) >= apart * 0.5) {
            pts.insert(0, target);
        }
        pts
    }

    /// The first sample: the anchor everything else is relative to.
    pub fn anchor(&self) -> TrackPoint {
        TrackPoint { x: self.target.0, y: self.target.1, z: 0.0, scale: 1.0, rotation: 0.0, confidence: 1.0 }
    }

    /// Track every feature with a starting guess, keeping the ones that track home again.
    fn match_all(&self, next: &[Level], guess: (f32, f32)) -> (Vec<(f32, f32)>, Vec<(f32, f32)>) {
        let (mut from, mut to) = (Vec::new(), Vec::new());
        for &p in &self.feats {
            let Some(q) = track_point(&self.prev, next, p, self.radius, guess) else { continue };
            // Forward/backward error: track the match home again and see if it lands where it
            // started. This throws away points that slipped onto a similar-looking bit of the
            // picture (Median-Flow's check, the same test the OpenCV samples use).
            let Some(back) = track_point(next, &self.prev, q, self.radius, (-guess.0, -guess.1)) else { continue };
            if (back.0 - p.0).hypot(back.1 - p.1) > 1.0 {
                continue;
            }
            from.push(p);
            to.push(q);
        }
        (from, to)
    }

    /// Track into `frame` (and its rectified `right` view, for depth).
    pub fn push(&mut self, frame: &Gray, right: Option<&Gray>) -> TrackPoint {
        let next = frame.pyramid(self.levels);
        let (mut from, mut to) = self.match_all(&next, self.velocity);
        // Too few survived: the motion was bigger than the window, or the guess was wrong.
        // Find the target patch by correlation at the coarsest level and try again from there.
        if to.len() < 3.max(self.feats.len() / 4) {
            if let Some(g) = search(&self.prev, &next, self.target, self.radius, self.reach * 1.5) {
                let (f2, t2) = self.match_all(&next, g);
                if t2.len() > to.len() {
                    (from, to) = (f2, t2);
                }
            }
        }
        let confidence: f32 = if self.feats.is_empty() { 0.0 } else { to.len() as f32 / self.feats.len() as f32 };
        if let Some((a, b, tx, ty)) = similarity(&from, &to) {
            let (x, y) = self.target;
            self.target = (a * x - b * y + tx, b * x + a * y + ty);
            self.velocity = (self.target.0 - x, self.target.1 - y);
            let s = (a * a + b * b).sqrt();
            if s.is_finite() && s > 0.05 && s < 20.0 {
                self.scale *= s;
                self.rotation += b.atan2(a).to_degrees();
            }
            self.lost = 0;
        } else {
            self.lost += 1;
        }
        self.feats = to;
        self.since_seed += 1;
        // Re-seed when the set thins out or on a fixed cadence, so drift cannot accumulate.
        if self.feats.len() < 6 || self.since_seed >= RESEED_EVERY {
            let fresh = Self::seed(frame, self.target, self.reach, self.radius);
            if fresh.len() >= self.feats.len() {
                self.feats = fresh;
                self.since_seed = 0;
            }
        }
        self.prev = next;
        let inside = self.target.0 >= 0.0 && self.target.1 >= 0.0 && self.target.0 < frame.w as f32 && self.target.1 < frame.h as f32;
        let conf = if inside && self.lost == 0 { confidence.clamp(0.0, 1.0) } else { 0.0 };
        let z = right.map(|r| disparity(frame, r, self.target, self.radius.min(7), self.max_disp)).unwrap_or(0.0);
        TrackPoint { x: self.target.0, y: self.target.1, z, scale: self.scale, rotation: self.rotation, confidence: conf }
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
}
