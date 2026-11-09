//! One-click background removal with keep/remove seed points.
//!
//! The effect builds small foreground/background colour models from seed points (or an automatic
//! centre/border prior on the first click), keys each pixel by the nearest-model distance margin,
//! then smooths and feathers the matte. Points are normalised 0..1 so they survive resolution
//! changes; Threshold, Feather and Edge Smooth are keyframable.

use filmcraft_project::{EffectInstance, ParamValue};
use rayon::prelude::*;

use crate::effects::FxCtx;
use crate::image::Image;

use super::{blur_plane, bv, enc, fv};

fn points(e: &EffectInstance, id: &str) -> Vec<(f32, f32)> {
    match e.param(id).map(|p| &p.value) {
        Some(ParamValue::Curve(c)) => c.iter().filter(|p| p[0].is_finite() && p[1].is_finite()).map(|p| (p[0].clamp(0.0, 1.0), p[1].clamp(0.0, 1.0))).collect(),
        _ => Vec::new(),
    }
}

fn pixel_color(img: &Image, x: usize, y: usize) -> [f32; 3] {
    let i = (y * img.w + x) * 4;
    let Some(p) = img.px.get(i..i + 4) else { return [0.0; 3] };
    if p[3] <= 1e-6 {
        return [0.0; 3];
    }
    enc([p[0] / p[3], p[1] / p[3], p[2] / p[3]])
}

fn seed_colors(img: &Image, pts: &[(f32, f32)]) -> Vec<[f32; 3]> {
    let mut out = Vec::new();
    let r = 2isize;
    for &(nx, ny) in pts {
        let cx = (nx * img.w as f32).clamp(0.0, img.w as f32 - 1.0) as isize;
        let cy = (ny * img.h as f32).clamp(0.0, img.h as f32 - 1.0) as isize;
        for dy in -r..=r {
            for dx in -r..=r {
                let x = cx + dx;
                let y = cy + dy;
                if x >= 0 && y >= 0 && x < img.w as isize && y < img.h as isize {
                    out.push(pixel_color(img, x as usize, y as usize));
                }
            }
        }
    }
    out
}

fn clusters(samples: &[[f32; 3]], k: usize) -> Vec<[f32; 3]> {
    if samples.is_empty() {
        return Vec::new();
    }
    let k = k.min(samples.len()).max(1);
    let mut c: Vec<[f32; 3]> = (0..k).map(|i| samples[i * samples.len() / k]).collect();
    for _ in 0..5 {
        let mut sum = vec![[0.0f32; 3]; k];
        let mut n = vec![0usize; k];
        for s in samples {
            let mut best = 0;
            let mut bd = f32::MAX;
            for (i, m) in c.iter().enumerate() {
                let d = dist2(s, m);
                if d < bd {
                    bd = d;
                    best = i;
                }
            }
            n[best] += 1;
            for ch in 0..3 {
                sum[best][ch] += s[ch];
            }
        }
        for i in 0..k {
            if n[i] > 0 {
                for ch in 0..3 {
                    c[i][ch] = sum[i][ch] / n[i] as f32;
                }
            }
        }
    }
    c
}

fn dist2(a: &[f32; 3], b: &[f32; 3]) -> f32 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
}

fn nearest(c: &[[f32; 3]], x: &[f32; 3]) -> f32 {
    c.iter().map(|m| dist2(x, m)).fold(f32::MAX, f32::min)
}

const AUTO_KEEP: &[(f32, f32)] = &[(0.5, 0.5), (0.44, 0.46), (0.56, 0.54)];
const AUTO_REMOVE: &[(f32, f32)] = &[(0.02, 0.02), (0.98, 0.02), (0.02, 0.98), (0.98, 0.98), (0.5, 0.02), (0.5, 0.98), (0.02, 0.5), (0.98, 0.5)];

fn stamp(alpha: &mut [f32], w: usize, h: usize, nx: f32, ny: f32, radius: f32, strength: f32, keep: bool) {
    let cx = nx * w as f32;
    let cy = ny * h as f32;
    let x0 = ((cx - radius).floor().max(0.0) as usize).min(w.saturating_sub(1));
    let x1 = ((cx + radius).ceil().max(0.0) as usize).min(w.saturating_sub(1));
    let y0 = ((cy - radius).floor().max(0.0) as usize).min(h.saturating_sub(1));
    let y1 = ((cy + radius).ceil().max(0.0) as usize).min(h.saturating_sub(1));
    for y in y0..=y1 {
        for x in x0..=x1 {
            let d = ((x as f32 + 0.5 - cx).hypot(y as f32 + 0.5 - cy) / radius).clamp(0.0, 1.0);
            let cov = (1.0 - d) * (1.0 - d) * (3.0 - 2.0 * (1.0 - d));
            let k = cov * strength;
            let a = &mut alpha[y * w + x];
            *a = if keep { *a * (1.0 - k) + k } else { *a * (1.0 - k) };
        }
    }
}

pub(super) fn apply(img: &mut Image, e: &EffectInstance, cx: &FxCtx) {
    if img.w == 0 || img.h == 0 {
        return;
    }
    let mut keep_pts = points(e, "keep_points");
    let mut remove_pts = points(e, "remove_points");
    if keep_pts.is_empty() && remove_pts.is_empty() {
        keep_pts = AUTO_KEEP.to_vec();
        remove_pts = AUTO_REMOVE.to_vec();
    } else {
        if keep_pts.is_empty() {
            keep_pts = AUTO_KEEP.to_vec();
        }
        if remove_pts.is_empty() {
            remove_pts = AUTO_REMOVE.to_vec();
        }
    }
    let keep = clusters(&seed_colors(img, &keep_pts), 5);
    let remove = clusters(&seed_colors(img, &remove_pts), 7);
    if keep.is_empty() || remove.is_empty() {
        return;
    }
    let threshold = (fv(e, "threshold", cx) / 100.0).clamp(0.01, 1.0);
    let invert = bv(e, "invert");
    let w = img.w;
    let h = img.h;
    let mut alpha = vec![0.0f32; w * h];
    alpha.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for (x, a) in row.iter_mut().enumerate() {
            let c = pixel_color(img, x, y);
            let fg = nearest(&keep, &c);
            let bg = nearest(&remove, &c);
            let mut v = (0.5 + (bg - fg) / (threshold * 0.7 + 1e-4)).clamp(0.0, 1.0);
            if invert {
                v = 1.0 - v;
            }
            *a = v;
        }
    });
    // Seed points also stamp a soft local keep/remove disk, so a tap reliably protects or cuts the
    // area under the finger even when its colour resembles the rest of the picture.
    let radius = (fv(e, "point_radius", cx) / 100.0).clamp(0.0, 1.0) * w.min(h) as f32;
    let strength = (fv(e, "point_strength", cx) / 100.0).clamp(0.0, 1.0);
    if radius > 0.5 && strength > 0.0 {
        for &(nx, ny) in &keep_pts {
            stamp(&mut alpha, w, h, nx, ny, radius, strength, true);
        }
        for &(nx, ny) in &remove_pts {
            stamp(&mut alpha, w, h, nx, ny, radius, strength, false);
        }
    }
    let smooth = fv(e, "smooth", cx).max(0.0) * cx.px_scale.max(0.0);
    if smooth > 0.25 {
        alpha = blur_plane(&alpha, w, h, smooth * 0.45);
    }
    let feather = fv(e, "feather", cx).max(0.0) * cx.px_scale.max(0.0);
    if feather > 0.25 {
        alpha = blur_plane(&alpha, w, h, feather * 0.35);
    }
    for (p, a) in img.px.chunks_mut(4).zip(alpha) {
        for v in p.iter_mut() {
            *v *= a;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use filmcraft_project::find_effect;
    use filmcraft_time::Tick;

    #[test]
    fn auto_seed_keeps_centre_and_removes_border() {
        let e = find_effect("background_removal").unwrap().instance();
        let mut img = Image::new(32, 32);
        for y in 0..32 {
            for x in 0..32 {
                let i = (y * 32 + x) * 4;
                let center = x > 10 && x < 22 && y > 10 && y < 22;
                let c = if center { [0.9, 0.1, 0.1, 1.0] } else { [0.05, 0.05, 0.9, 1.0] };
                img.px[i..i + 4].copy_from_slice(&c);
            }
        }
        let cx = FxCtx {
            t: Tick::ZERO,
            px_scale: 1.0,
            seconds: 0.0,
            timecode: "",
            clip_name: "",
            project: None,
            env: None,
            frame_rate: 30.0,
            working: filmcraft_color::WorkingSpace::Rec709,
        };
        apply(&mut img, &e, &cx);
        let center = img.px[(16 * 32 + 16) * 4 + 3];
        let corner = img.px[3];
        assert!(center > 0.6, "centre kept: {center}");
        assert!(corner < 0.4, "corner removed: {corner}");
    }
}
