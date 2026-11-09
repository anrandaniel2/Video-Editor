//! Feature-based motion tracking for 3D layer follow.
//!
//! A small Lucas–Kanade tracker follows a centre patch and four satellite patches. The satellites
//! give a similarity transform (translation, scale, rotation); an optional right stereoscopic view
//! gives relative depth by horizontal disparity search. The mobile bridge samples decoded frames
//! into [`Gray`] pictures and stores the result as a sequence [`filmcraft_project::MotionTrack`].

/// A single-channel tracking picture (0..1 luma).
#[derive(Clone)]
pub struct Gray {
    pub w: usize,
    pub h: usize,
    pub p: Vec<f32>,
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
        if x < 0.0 || y < 0.0 || x > self.w as f32 - 1.0 || y > self.h as f32 - 1.0 {
            return None;
        }
        let x0 = x.floor() as usize;
        let y0 = y.floor() as usize;
        let x1 = (x0 + 1).min(self.w - 1);
        let y1 = (y0 + 1).min(self.h - 1);
        let fx = x - x0 as f32;
        let fy = y - y0 as f32;
        let a = self.p[y0 * self.w + x0];
        let b = self.p[y0 * self.w + x1];
        let c = self.p[y1 * self.w + x0];
        let d = self.p[y1 * self.w + x1];
        Some(a * (1.0 - fx) * (1.0 - fy) + b * fx * (1.0 - fy) + c * (1.0 - fx) * fy + d * fx * fy)
    }
}

/// One tracked sample in view pixels, with relative depth/scale/rotation.
#[derive(Clone, Copy, Debug)]
pub struct TrackPoint {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub scale: f32,
    pub rotation: f32,
}

fn klt(prev: &Gray, next: &Gray, start: (f32, f32), radius: i32) -> Option<(f32, f32)> {
    let r = radius.max(3);
    let mut gx = 0.0f32;
    let mut gy = 0.0f32;
    let mut gxx = 0.0f32;
    let mut gyy = 0.0f32;
    let mut gxy = 0.0f32;
    for dy in -r..=r {
        for dx in -r..=r {
            let x = start.0 + dx as f32;
            let y = start.1 + dy as f32;
            let ix = (prev.at(x + 1.0, y)? - prev.at(x - 1.0, y)?) * 0.5;
            let iy = (prev.at(x, y + 1.0)? - prev.at(x, y - 1.0)?) * 0.5;
            gxx += ix * ix;
            gyy += iy * iy;
            gxy += ix * iy;
            gx += ix;
            gy += iy;
        }
    }
    let det = gxx * gyy - gxy * gxy;
    if det.abs() < 1e-4 {
        return None;
    }
    let mut tx = 0.0f32;
    let mut ty = 0.0f32;
    for _ in 0..10 {
        let mut bx = 0.0f32;
        let mut by = 0.0f32;
        for dy in -r..=r {
            for dx in -r..=r {
                let x = start.0 + dx as f32;
                let y = start.1 + dy as f32;
                let it = next.at(x + tx, y + ty)? - prev.at(x, y)?;
                let ix = (prev.at(x + 1.0, y)? - prev.at(x - 1.0, y)?) * 0.5;
                let iy = (prev.at(x, y + 1.0)? - prev.at(x, y - 1.0)?) * 0.5;
                bx += ix * it;
                by += iy * it;
            }
        }
        let dx = (-bx * gyy + by * gxy) / det;
        let dy = (-by * gxx + bx * gxy) / det;
        tx += dx.clamp(-4.0, 4.0);
        ty += dy.clamp(-4.0, 4.0);
        if dx.abs() + dy.abs() < 0.02 {
            break;
        }
    }
    let out = (start.0 + tx, start.1 + ty);
    if next.at(out.0, out.1).is_some() { Some(out) } else { None }
}

fn disparity(left: &Gray, right: &Gray, pt: (f32, f32), radius: i32, max_disp: f32) -> f32 {
    let r = radius.max(3);
    let mut best = 0.0f32;
    let mut best_score = f32::MAX;
    let mut d = 0.0f32;
    while d <= max_disp {
        let mut score = 0.0f32;
        let mut n = 0usize;
        for dy in -r..=r {
            for dx in -r..=r {
                let x = pt.0 + dx as f32;
                let y = pt.1 + dy as f32;
                if let (Some(a), Some(b)) = (left.at(x, y), right.at(x - d, y)) {
                    let e = a - b;
                    score += e * e;
                    n += 1;
                }
            }
        }
        if n > 0 {
            score /= n as f32;
            if score < best_score {
                best_score = score;
                best = d;
            }
        }
        d += 1.0;
    }
    (best / max_disp.max(1.0)).clamp(0.0, 1.0)
}

/// Track `start` through `left` frames. `right` (same size, rectified) enables stereo depth.
/// The first sample is the anchor: scale 1, rotation 0.
pub fn track_sequence(left: &[Gray], right: Option<&[Gray]>, start: (f32, f32), patch: usize, max_disp: f32) -> Vec<TrackPoint> {
    if left.is_empty() {
        return Vec::new();
    }
    let patch = patch.clamp(7, 31) as i32;
    let r = patch / 2;
    // Satellites sit two patch radii out so scale/rotation have a usable baseline.
    let spread = (r * 2) as f32;
    let offsets = [(0.0, 0.0), (spread, 0.0), (-spread, 0.0), (0.0, spread), (0.0, -spread)];
    let mut pts: Vec<(f32, f32)> = offsets.iter().map(|&(dx, dy)| (start.0 + dx, start.1 + dy)).collect();
    let mut out = Vec::new();
    let mut anchor = start;
    for (i, frame) in left.iter().enumerate() {
        if i > 0 {
            let prev = &left[i - 1];
            for p in &mut pts {
                if let Some(q) = klt(prev, frame, *p, r) {
                    *p = q;
                }
            }
        }
        let center = pts[0];
        if frame.at(center.0, center.1).is_none() {
            out.push(TrackPoint { x: anchor.0, y: anchor.1, z: 0.0, scale: 1.0, rotation: 0.0 });
            continue;
        }
        anchor = center;
        let (mut a, mut b, mut den) = (0.0f32, 0.0f32, 0.0f32);
        for k in 1..pts.len() {
            let v0 = offsets[k];
            let v = (pts[k].0 - center.0, pts[k].1 - center.1);
            a += v0.0 * v.0 + v0.1 * v.1;
            b += v0.0 * v.1 - v0.1 * v.0;
            den += v0.0 * v0.0 + v0.1 * v0.1;
        }
        let (scale, rotation) = if den > 1e-6 {
            a /= den;
            b /= den;
            ((a * a + b * b).sqrt().clamp(0.05, 20.0), b.atan2(a).to_degrees())
        } else {
            (1.0, 0.0)
        };
        let z = right
            .and_then(|r| r.get(i))
            .map(|r| disparity(frame, r, center, r.min(7), max_disp.max(1.0)))
            .unwrap_or(0.0);
        out.push(TrackPoint { x: center.0, y: center.1, z, scale, rotation });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shifted(dx: f32, dy: f32) -> Gray {
        let mut g = Gray { w: 64, h: 64, p: vec![0.0; 64 * 64] };
        for y in 0..64 {
            for x in 0..64 {
                let v = ((x as f32 + dx) * 0.35).sin() * ((y as f32 + dy) * 0.35).cos();
                g.p[y * 64 + x] = (v * 0.5 + 0.5).clamp(0.0, 1.0);
            }
        }
        g
    }

    #[test]
    fn follows_a_translating_texture() {
        let frames = vec![shifted(0.0, 0.0), shifted(2.0, 1.0), shifted(4.0, 2.0)];
        let t = track_sequence(&frames, None, (32.0, 32.0), 15, 16.0);
        assert_eq!(t.len(), 3);
        assert!((t[2].x - 28.0).abs() < 2.5, "{t:?}");
        assert!((t[2].y - 30.0).abs() < 2.5, "{t:?}");
    }
}
