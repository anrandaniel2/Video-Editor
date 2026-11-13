//! Native drawing input dynamics. Android supplies pen samples and displays the result.
//! This is the first extracted part of the drawing engine, not a Krita compatibility layer.

/// Reports the real GPU painter, rather than presenting software coverage as hardware MSAA.
#[uniffi::export]
pub fn drawing_hardware_msaa_available() -> bool { filmcraft_gpu::drawing::available() }

#[derive(Clone, Copy, Debug, uniffi::Record)]
pub struct DrawingInput {
    pub x: f32,
    pub y: f32,
    pub pressure: f32,
}

fn finite(value: f32, default: f32) -> f32 {
    if value.is_finite() { value } else { default }
}

/// Distance-adaptive stabilization in canvas pixels, and a configurable pen-pressure curve.
/// Zero stabilization is exactly passthrough; every pointer-down starts a new sample stream.
#[uniffi::export]
pub fn drawing_stroke_samples(
    previous_x: f32,
    previous_y: f32,
    inputs: Vec<DrawingInput>,
    stabilization: f32,
    pressure_curve: f32,
) -> Vec<DrawingInput> {
    let radius = finite(stabilization, 0.0).clamp(0.0, 100.0);
    let curve = finite(pressure_curve, 1.0).clamp(0.25, 4.0);
    let mut x = finite(previous_x, 0.0);
    let mut y = finite(previous_y, 0.0);
    inputs.into_iter().take(512).map(|input| {
        let target_x = finite(input.x, x);
        let target_y = finite(input.y, y);
        let distance = (target_x - x).hypot(target_y - y);
        let weight = if radius == 0.0 { 1.0 } else { (distance / (distance + radius)).clamp(0.05, 1.0) };
        x += (target_x - x) * weight;
        y += (target_y - y) * weight;
        DrawingInput { x, y, pressure: finite(input.pressure, 1.0).clamp(0.0, 1.0).powf(curve) }
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn off_is_passthrough_and_pressure_is_clamped() {
        let out = drawing_stroke_samples(0.0, 0.0, vec![DrawingInput { x: 10.0, y: 20.0, pressure: 2.0 }], 0.0, 1.0);
        assert_eq!((out[0].x, out[0].y, out[0].pressure), (10.0, 20.0, 1.0));
    }
    #[test]
    fn stabilizer_reduces_motion_without_overshooting() {
        let out = drawing_stroke_samples(0.0, 0.0, vec![DrawingInput { x: 10.0, y: 0.0, pressure: 0.5 }], 20.0, 2.0);
        assert!(out[0].x > 0.0 && out[0].x < 10.0);
        assert_eq!(out[0].pressure, 0.25);
    }
    #[test]
    fn invalid_samples_remain_finite() {
        let out = drawing_stroke_samples(f32::NAN, 0.0, vec![DrawingInput { x: f32::INFINITY, y: f32::NAN, pressure: f32::NAN }], f32::NAN, f32::NAN);
        assert!(out[0].x.is_finite() && out[0].y.is_finite() && out[0].pressure.is_finite());
    }
}

#[derive(Clone, Copy, Debug, uniffi::Record)]
pub struct DrawingStrokePoint {
    pub x: f32,
    pub y: f32,
    pub radius: f32,
}

#[derive(Debug, uniffi::Record)]
pub struct DrawingCoverage {
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
    pub alpha: Vec<u8>,
}

/// Hardware four-sample rasterization when available, with software coverage fallback.
/// Capsules union sample masks before resolve, avoiding dark seams at segment joins.
/// The GPU path uses an actual 4x multisampled attachment and hardware resolve.
#[uniffi::export]
pub fn drawing_stroke_coverage(points: Vec<DrawingStrokePoint>, width: u32, height: u32) -> Option<DrawingCoverage> {
    if points.is_empty() || points.len() > 8192 || width == 0 || height == 0 || u64::from(width) * u64::from(height) > 4_194_304 { return None; }
    if points.iter().any(|p| !p.x.is_finite() || !p.y.is_finite() || !p.radius.is_finite() || p.radius < 0.0 || p.radius > 2048.0) { return None; }
    let left = points.iter().map(|p| p.x - p.radius - 1.0).fold(f32::INFINITY, f32::min).floor().clamp(0.0, width as f32) as u32;
    let top = points.iter().map(|p| p.y - p.radius - 1.0).fold(f32::INFINITY, f32::min).floor().clamp(0.0, height as f32) as u32;
    let right = points.iter().map(|p| p.x + p.radius + 1.0).fold(f32::NEG_INFINITY, f32::max).ceil().clamp(0.0, width as f32) as u32;
    let bottom = points.iter().map(|p| p.y + p.radius + 1.0).fold(f32::NEG_INFINITY, f32::max).ceil().clamp(0.0, height as f32) as u32;
    if right <= left || bottom <= top { return None; }
    let w = (right - left) as usize;
    let h = (bottom - top) as usize;
    let gpu_points: Vec<[f32; 3]> = points.iter().map(|p| [p.x, p.y, p.radius]).collect();
    if let Some(alpha) = filmcraft_gpu::drawing::coverage(&gpu_points, left, top, w as u32, h as u32) {
        return Some(DrawingCoverage { left, top, width: w as u32, height: h as u32, alpha });
    }
    let mut coverage = vec![0u8; w * h];
    let samples = [(0.25f32, 0.25f32), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)];
    let mut work = 0u64;
    for (i, b) in points.iter().enumerate() {
        let a = if i == 0 { b } else { &points[i - 1] };
        let radius = a.radius.max(b.radius);
        let x0 = (a.x.min(b.x) - radius - 1.0).floor().clamp(left as f32, right as f32) as u32;
        let x1 = (a.x.max(b.x) + radius + 1.0).ceil().clamp(left as f32, right as f32) as u32;
        let y0 = (a.y.min(b.y) - radius - 1.0).floor().clamp(top as f32, bottom as f32) as u32;
        let y1 = (a.y.max(b.y) + radius + 1.0).ceil().clamp(top as f32, bottom as f32) as u32;
        work = work.saturating_add(u64::from(x1 - x0) * u64::from(y1 - y0));
        if work > 32_000_000 { return None; } // Keep very large strokes on the existing bounded renderer.
        let dx = b.x - a.x; let dy = b.y - a.y;
        let length2 = dx * dx + dy * dy;
        for y in y0..y1 { for x in x0..x1 {
            let pixel = (y - top) as usize * w + (x - left) as usize;
            for (bit, (sx, sy)) in samples.iter().enumerate() {
                if coverage[pixel] & (1 << bit) != 0 { continue; }
                let px = x as f32 + sx; let py = y as f32 + sy;
                let t = if length2 > 0.00001 { (((px - a.x) * dx + (py - a.y) * dy) / length2).clamp(0.0, 1.0) } else { 0.0 };
                let r = a.radius + (b.radius - a.radius) * t;
                let ex = px - (a.x + t * dx); let ey = py - (a.y + t * dy);
                if ex * ex + ey * ey <= r * r { coverage[pixel] |= 1 << bit; }
            }
        } }
    }
    for pixel in &mut coverage { *pixel = ((pixel.count_ones() * 255 + 2) / 4) as u8; }
    Some(DrawingCoverage { left, top, width: w as u32, height: h as u32, alpha: coverage })
}

#[cfg(test)]
mod coverage_tests {
    use super::*;
    #[test]
    fn edges_have_fractional_coverage_and_joins_do_not_darken() {
        let p = DrawingStrokePoint { x: 3.5, y: 3.5, radius: 1.25 };
        let one = drawing_stroke_coverage(vec![p], 8, 8);
        let twice = drawing_stroke_coverage(vec![p, p], 8, 8);
        assert!(one.is_some() && twice.is_some());
        if let (Some(a), Some(b)) = (one, twice) {
            assert_eq!(a.alpha, b.alpha);
            assert!(a.alpha.iter().any(|v| *v > 0 && *v < 255));
        }
    }
    #[test]
    fn invalid_and_oversized_inputs_are_rejected() {
        assert!(drawing_stroke_coverage(vec![], 8, 8).is_none());
        assert!(drawing_stroke_coverage(vec![DrawingStrokePoint { x: f32::NAN, y: 0.0, radius: 1.0 }], 8, 8).is_none());
    }
}
