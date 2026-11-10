//! The little bit of 3D maths a model loader needs: column-major 4x4 matrices, quaternions and
//! the transforms glTF nodes are built from.

/// Column-major 4x4, as glTF stores them.
pub type Mat4 = [f32; 16];

pub const IDENTITY: Mat4 = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

pub fn mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut out = [0.0f32; 16];
    for c in 0..4 {
        for r in 0..4 {
            let mut s = 0.0;
            for k in 0..4 {
                s += a[k * 4 + r] * b[c * 4 + k];
            }
            out[c * 4 + r] = s;
        }
    }
    out
}

/// Transform a point (w = 1).
pub fn point(m: &Mat4, p: [f32; 3]) -> [f32; 3] {
    [
        m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12],
        m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13],
        m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14],
    ]
}

/// Transform a direction (w = 0).
pub fn vector(m: &Mat4, v: [f32; 3]) -> [f32; 3] {
    [m[0] * v[0] + m[4] * v[1] + m[8] * v[2], m[1] * v[0] + m[5] * v[1] + m[9] * v[2], m[2] * v[0] + m[6] * v[1] + m[10] * v[2]]
}

pub fn trs(t: [f32; 3], r: [f32; 4], s: [f32; 3]) -> Mat4 {
    let [x, y, z, w] = r;
    let (x2, y2, z2) = (x + x, y + y, z + z);
    let (xx, xy, xz) = (x * x2, x * y2, x * z2);
    let (yy, yz, zz) = (y * y2, y * z2, z * z2);
    let (wx, wy, wz) = (w * x2, w * y2, w * z2);
    [
        (1.0 - (yy + zz)) * s[0],
        (xy + wz) * s[0],
        (xz - wy) * s[0],
        0.0,
        (xy - wz) * s[1],
        (1.0 - (xx + zz)) * s[1],
        (yz + wx) * s[1],
        0.0,
        (xz + wy) * s[2],
        (yz - wx) * s[2],
        (1.0 - (xx + yy)) * s[2],
        0.0,
        t[0],
        t[1],
        t[2],
        1.0,
    ]
}

/// Normalised quaternion linear interpolation, the cheap half of slerp that glTF's LINEAR
/// rotation tracks are specified to use.
pub fn quat_nlerp(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    let dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
    let sign = if dot < 0.0 { -1.0 } else { 1.0 };
    let q = [
        a[0] + (b[0] * sign - a[0]) * t,
        a[1] + (b[1] * sign - a[1]) * t,
        a[2] + (b[2] * sign - a[2]) * t,
        a[3] + (b[3] * sign - a[3]) * t,
    ];
    let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if len > 1e-8 { [q[0] / len, q[1] / len, q[2] / len, q[3] / len] } else { a }
}

/// The inverse transpose of the upper 3x3, for normals under non-uniform scale. Falls back to
/// the linear part itself when the matrix is singular.
pub fn normal_matrix(m: &Mat4) -> Mat4 {
    let (a, b, c) = (m[0], m[1], m[2]);
    let (d, e, f) = (m[4], m[5], m[6]);
    let (g, h, i) = (m[8], m[9], m[10]);
    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    if det.abs() < 1e-12 {
        return *m;
    }
    let k = 1.0 / det;
    // Cofactors, transposed: the inverse transpose's columns are the cofactor columns.
    [
        (e * i - f * h) * k,
        (c * h - b * i) * k,
        (b * f - c * e) * k,
        0.0,
        (f * g - d * i) * k,
        (a * i - c * g) * k,
        (c * d - a * f) * k,
        0.0,
        (d * h - e * g) * k,
        (b * g - a * h) * k,
        (a * e - b * d) * k,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
    ]
}

pub fn norm(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if l > 1e-12 { [v[0] / l, v[1] / l, v[2] / l] } else { [0.0, 0.0, 1.0] }
}

pub fn norm_quat(q: [f32; 4]) -> [f32; 4] {
    let l = q.iter().map(|v| v * v).sum::<f32>().sqrt();
    if l > 1e-12 && l.is_finite() { q.map(|v| v / l) } else { [0.0, 0.0, 0.0, 1.0] }
}

/// Decompose a glTF node matrix into translation, rotation and scale. glTF node matrices must
/// be TRS (no shear); column lengths give scale and the remaining orthogonal matrix gives quat.
pub fn decompose(m: &Mat4) -> ([f32; 3], [f32; 4], [f32; 3]) {
    let t = [m[12], m[13], m[14]];
    let col = |c: usize| [m[c * 4], m[c * 4 + 1], m[c * 4 + 2]];
    let (a, b, c) = (col(0), col(1), col(2));
    let len = |v: [f32; 3]| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    let det = a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0]);
    let mut s = [len(a), len(b), len(c)];
    if det < 0.0 { s[0] = -s[0]; }
    let div = |v: [f32; 3], k: f32| if k.abs() > 1e-12 { v.map(|x| x / k) } else { [0.0; 3] };
    let (x, y, z) = (div(a, s[0]), div(b, s[1]), div(c, s[2]));
    // Row-major rotation rows are (x0,y0,z0), (x1,y1,z1), (x2,y2,z2).
    let (m00, m01, m02) = (x[0], y[0], z[0]);
    let (m10, m11, m12) = (x[1], y[1], z[1]);
    let (m20, m21, m22) = (x[2], y[2], z[2]);
    let trace = m00 + m11 + m22;
    if !trace.is_finite() || s.iter().any(|x| !x.is_finite()) { return (t, [0.0, 0.0, 0.0, 1.0], [0.0; 3]); }
    let q = if trace > 0.0 {
        let s = ((trace + 1.0).sqrt() * 2.0).max(1e-12);
        [(m21 - m12) / s, (m02 - m20) / s, (m10 - m01) / s, 0.25 * s]
    } else if m00 > m11 && m00 > m22 {
        let s = ((1.0 + m00 - m11 - m22).max(0.0).sqrt() * 2.0).max(1e-12);
        [0.25 * s, (m01 + m10) / s, (m02 + m20) / s, (m21 - m12) / s]
    } else if m11 > m22 {
        let s = ((1.0 + m11 - m00 - m22).max(0.0).sqrt() * 2.0).max(1e-12);
        [(m01 + m10) / s, 0.25 * s, (m12 + m21) / s, (m02 - m20) / s]
    } else {
        let s = ((1.0 + m22 - m00 - m11).max(0.0).sqrt() * 2.0).max(1e-12);
        [(m02 + m20) / s, (m12 + m21) / s, 0.25 * s, (m10 - m01) / s]
    };
    (t, norm_quat(q), s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trs_matches_a_hand_built_matrix() {
        // A quarter turn about Y, doubled, moved right by 3.
        let q = [0.0, (std::f32::consts::FRAC_PI_4).sin(), 0.0, (std::f32::consts::FRAC_PI_4).cos()];
        let m = trs([3.0, 0.0, 0.0], q, [2.0, 2.0, 2.0]);
        let p = point(&m, [1.0, 0.0, 0.0]);
        assert!((p[0] - 3.0).abs() < 1e-5 && (p[2] + 2.0).abs() < 1e-5, "{p:?}");
    }

    #[test]
    fn multiply_is_associative_and_identity_is_neutral() {
        let a = trs([1.0, 2.0, 3.0], [0.0, 0.0, 0.0, 1.0], [1.0, 2.0, 1.0]);
        let b = trs([-1.0, 0.5, 0.0], [0.0, 0.7071068, 0.0, 0.7071068], [2.0; 3]);
        let p = [0.3, -1.2, 4.0];
        let ab = point(&mul(&a, &b), p);
        let step = point(&a, point(&b, p));
        for i in 0..3 {
            assert!((ab[i] - step[i]).abs() < 1e-4, "{ab:?} vs {step:?}");
        }
        assert_eq!(mul(&a, &IDENTITY), a);
    }

    #[test]
    fn normals_stay_perpendicular_under_stretch() {
        // Stretch x by 4: a 45° surface normal must lean the other way, not with the stretch.
        let m = trs([0.0; 3], [0.0, 0.0, 0.0, 1.0], [4.0, 1.0, 1.0]);
        let n = norm(vector(&normal_matrix(&m), norm([1.0, 1.0, 0.0])));
        let t = norm(vector(&m, [1.0, -1.0, 0.0]));
        let dot = n[0] * t[0] + n[1] * t[1] + n[2] * t[2];
        assert!(dot.abs() < 1e-5, "normal {n:?} is not perpendicular to {t:?}");
    }

    #[test]
    fn nlerp_keeps_quaternions_unit_and_takes_the_short_way() {
        let a = [0.0, 0.0, 0.0, 1.0];
        let b = [0.0, 1.0, 0.0, 0.0];
        let q = quat_nlerp(a, b, 0.5);
        let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
        assert!((len - 1.0).abs() < 1e-6);
        let neg = quat_nlerp(a, [0.0, -1.0, 0.0, 0.0], 0.5);
        assert!((q[1] - neg[1]).abs() < 1e-6, "the short arc should match: {q:?} vs {neg:?}");
    }
}
