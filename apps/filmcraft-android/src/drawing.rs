//! Native drawing input dynamics. Android supplies pen samples and displays the result.
//! This is the first extracted part of the drawing engine, not a Krita compatibility layer.

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
