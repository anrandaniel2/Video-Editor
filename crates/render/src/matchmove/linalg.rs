//! Self-contained deterministic PRNG, `SO(3)` / `SE(3)` Lie math, symmetric Jacobi eigensolver,
//! `3×3` SVD, Hartley normalisation, and partial-pivoted dense linear solver.

pub(crate) type Vec3 = [f64; 3];
pub(crate) type Mat3 = [[f64; 3]; 3];

pub(crate) const IDENT3: Mat3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// Deterministic `xorshift64` generator for RANSAC sampling (no wall-clock or OS entropy).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Rng(u64);

impl Rng {
    pub(crate) fn new(seed: u64) -> Self {
        Self(if seed == 0 { 0x5eed_cafe_d00d_f00du64 } else { seed })
    }

    pub(crate) fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// Sample `K` distinct indices uniformly from `0..n` (`n >= K`).
    pub(crate) fn sample_indices<const K: usize>(&mut self, n: usize) -> Option<[usize; K]> {
        if n < K || K == 0 {
            return None;
        }
        let mut picked = [0usize; K];
        let mut count = 0;
        while count < K {
            let cand = (self.next_u64() as usize) % n;
            if !picked[..count].contains(&cand) {
                picked[count] = cand;
                count += 1;
            }
        }
        Some(picked)
    }
}

pub(crate) fn dot3(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(crate) fn cross3(a: Vec3, b: Vec3) -> Vec3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

pub(crate) fn norm3(v: Vec3) -> f64 {
    dot3(v, v).sqrt()
}

pub(crate) fn normalize3(v: Vec3) -> Option<Vec3> {
    let n = norm3(v);
    (n.is_finite() && n > 1e-12).then(|| [v[0] / n, v[1] / n, v[2] / n])
}

pub(crate) fn add3(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub(crate) fn sub3(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(crate) fn scale3(v: Vec3, s: f64) -> Vec3 {
    [v[0] * s, v[1] * s, v[2] * s]
}

pub(crate) fn skew3(v: Vec3) -> Mat3 {
    [[0.0, -v[2], v[1]], [v[2], 0.0, -v[0]], [-v[1], v[0], 0.0]]
}

pub(crate) fn mat3_transpose(m: Mat3) -> Mat3 {
    [[m[0][0], m[1][0], m[2][0]], [m[0][1], m[1][1], m[2][1]], [m[0][2], m[1][2], m[2][2]]]
}

pub(crate) fn mat3_mul(a: Mat3, b: Mat3) -> Mat3 {
    let mut out = [[0.0; 3]; 3];
    for r in 0..3 {
        for c in 0..3 {
            out[r][c] = a[r][0] * b[0][c] + a[r][1] * b[1][c] + a[r][2] * b[2][c];
        }
    }
    out
}

pub(crate) fn mat3_mul_vec(m: Mat3, v: Vec3) -> Vec3 {
    [dot3(m[0], v), dot3(m[1], v), dot3(m[2], v)]
}

pub(crate) fn mat3_t_mul_vec(m: Mat3, v: Vec3) -> Vec3 {
    [m[0][0] * v[0] + m[1][0] * v[1] + m[2][0] * v[2], m[0][1] * v[0] + m[1][1] * v[1] + m[2][1] * v[2], m[0][2] * v[0] + m[1][2] * v[1] + m[2][2] * v[2]]
}

pub(crate) fn mat3_det(m: Mat3) -> f64 {
    m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0]) + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}

pub(crate) fn mat3_inv(m: Mat3) -> Option<Mat3> {
    let c00 = m[1][1] * m[2][2] - m[1][2] * m[2][1];
    let c01 = -(m[1][0] * m[2][2] - m[1][2] * m[2][0]);
    let c02 = m[1][0] * m[2][1] - m[1][1] * m[2][0];
    let det = m[0][0] * c00 + m[0][1] * c01 + m[0][2] * c02;
    if !det.is_finite() || det.abs() < 1e-14 {
        return None;
    }
    let inv = 1.0 / det;
    let c10 = -(m[0][1] * m[2][2] - m[0][2] * m[2][1]);
    let c11 = m[0][0] * m[2][2] - m[0][2] * m[2][0];
    let c12 = -(m[0][0] * m[2][1] - m[0][1] * m[2][0]);
    let c20 = m[0][1] * m[1][2] - m[0][2] * m[1][1];
    let c21 = -(m[0][0] * m[1][2] - m[0][2] * m[1][0]);
    let c22 = m[0][0] * m[1][1] - m[0][1] * m[1][0];
    Some([[c00 * inv, c10 * inv, c20 * inv], [c01 * inv, c11 * inv, c21 * inv], [c02 * inv, c12 * inv, c22 * inv]])
}

/// Exponential map `exp([w]_×)` on `SO(3)` via Rodrigues' formula.
pub(crate) fn rodrigues_exp(w: Vec3) -> Mat3 {
    let theta2 = dot3(w, w);
    if !theta2.is_finite() {
        return IDENT3;
    }
    let k = skew3(w);
    let k2 = mat3_mul(k, k);
    let (a, b) = if theta2 < 1e-10 {
        (1.0 - theta2 / 6.0, 0.5 - theta2 / 24.0)
    } else {
        let theta = theta2.sqrt();
        (theta.sin() / theta, (1.0 - theta.cos()) / theta2)
    };
    let mut r = IDENT3;
    for i in 0..3 {
        for j in 0..3 {
            r[i][j] += a * k[i][j] + b * k2[i][j];
        }
    }
    r
}

/// Geodesic angle in radians between two rotations `R_a` and `R_b`.
#[cfg(test)]
pub(crate) fn rotation_angle_rad(ra: Mat3, rb: Mat3) -> f64 {
    let rel = mat3_mul(ra, mat3_transpose(rb));
    let tr = rel[0][0] + rel[1][1] + rel[2][2];
    ((tr - 1.0) * 0.5).clamp(-1.0, 1.0).acos()
}

/// Cyclic Jacobi eigenvalue decomposition for a symmetric `N×N` matrix `a`.
/// Returns `(eigenvalues, eigenvectors_columns)` where column `j` of `v` corresponds to `vals[j]`.
pub(crate) fn jacobi_sym<const N: usize>(mut a: [[f64; N]; N]) -> ([f64; N], [[f64; N]; N]) {
    let mut v = [[0.0; N]; N];
    for i in 0..N {
        v[i][i] = 1.0;
    }
    for _sweep in 0..24 {
        let mut max_off = 0.0f64;
        for p in 0..N {
            for q in (p + 1)..N {
                max_off = max_off.max(a[p][q].abs());
                jacobi_rotate(&mut a, &mut v, p, q);
            }
        }
        if max_off < 1e-15 {
            break;
        }
    }
    let mut vals = [0.0; N];
    for i in 0..N {
        vals[i] = a[i][i];
    }
    (vals, v)
}

fn jacobi_rotate<const N: usize>(a: &mut [[f64; N]; N], v: &mut [[f64; N]; N], p: usize, q: usize) {
    let apq = a[p][q];
    if apq.abs() < 1e-18 {
        return;
    }
    let tau = (a[q][q] - a[p][p]) / (2.0 * apq);
    let t = if tau >= 0.0 { 1.0 / (tau + (1.0 + tau * tau).sqrt()) } else { -1.0 / (-tau + (1.0 + tau * tau).sqrt()) };
    let c = 1.0 / (1.0 + t * t).sqrt();
    let s = t * c;
    let app = a[p][p];
    let aqq = a[q][q];
    a[p][p] = app - t * apq;
    a[q][q] = aqq + t * apq;
    a[p][q] = 0.0;
    a[q][p] = 0.0;
    for r in 0..N {
        if r != p && r != q {
            let arp = a[r][p];
            let arq = a[r][q];
            a[r][p] = c * arp - s * arq;
            a[p][r] = a[r][p];
            a[r][q] = s * arp + c * arq;
            a[q][r] = a[r][q];
        }
    }
    for r in 0..N {
        let vrp = v[r][p];
        let vrq = v[r][q];
        v[r][p] = c * vrp - s * vrq;
        v[r][q] = s * vrp + c * vrq;
    }
}

/// Right singular vector corresponding to the smallest singular value of `A` given `m = A^T A`.
pub(crate) fn smallest_eigvec<const N: usize>(ata: [[f64; N]; N]) -> [f64; N] {
    let (vals, vecs) = jacobi_sym(ata);
    let mut best = 0usize;
    for i in 1..N {
        if vals[i] < vals[best] {
            best = i;
        }
    }
    let mut out = [0.0; N];
    for r in 0..N {
        out[r] = vecs[r][best];
    }
    out
}

/// `3×3` Singular Value Decomposition `m = U · diag(s) · V^T` with `s[0] >= s[1] >= s[2] >= 0`.
pub(crate) fn svd3(m: Mat3) -> (Mat3, [f64; 3], Mat3) {
    let mt = mat3_transpose(m);
    let mtm = mat3_mul(mt, m);
    let (evals, evecs) = jacobi_sym(mtm);
    let mut order = [0usize, 1, 2];
    order.sort_by(|&a, &b| evals[b].total_cmp(&evals[a]));
    let mut v = [[0.0; 3]; 3];
    let mut s = [0.0; 3];
    for col in 0..3 {
        let src = order[col];
        s[col] = evals[src].max(0.0).sqrt();
        for row in 0..3 {
            v[row][col] = evecs[row][src];
        }
    }
    if mat3_det(v) < 0.0 {
        for row in 0..3 {
            v[row][2] = -v[row][2];
        }
    }
    let v0 = [v[0][0], v[1][0], v[2][0]];
    let v1 = [v[0][1], v[1][1], v[2][1]];
    let v2 = [v[0][2], v[1][2], v[2][2]];
    let u0 = normalize3(mat3_mul_vec(m, v0)).unwrap_or([1.0, 0.0, 0.0]);
    let mv1 = mat3_mul_vec(m, v1);
    let u1_raw = sub3(mv1, scale3(u0, dot3(u0, mv1)));
    let u1 = normalize3(u1_raw).unwrap_or_else(|| orthogonal_unit(u0));
    let mut u2 = cross3(u0, u1);
    let mv2 = mat3_mul_vec(m, v2);
    if s[2] > 1e-12 && dot3(mv2, u2) < 0.0 {
        u2 = scale3(u2, -1.0);
    }
    let u = [[u0[0], u1[0], u2[0]], [u0[1], u1[1], u2[1]], [u0[2], u1[2], u2[2]]];
    let s_exact = [dot3(u0, mat3_mul_vec(m, v0)).max(0.0), dot3(u1, mv1).max(0.0), dot3(u2, mv2).max(0.0)];
    (u, s_exact, v)
}

fn orthogonal_unit(u: Vec3) -> Vec3 {
    let axis = if u[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    normalize3(cross3(u, axis)).unwrap_or([0.0, 0.0, 1.0])
}

/// Project an arbitrary `3×3` matrix onto the closest proper rotation `R ∈ SO(3)`.
pub(crate) fn project_to_so3(m: Mat3) -> Mat3 {
    let (mut u, _, v) = svd3(m);
    let vt = mat3_transpose(v);
    let cand = mat3_mul(u, vt);
    if mat3_det(cand) < 0.0 {
        for r in 0..3 {
            u[r][2] = -u[r][2];
        }
        mat3_mul(u, vt)
    } else {
        cand
    }
}

/// Hartley isotropic 2D point normalisation (`mean = 0`, mean distance from origin `= sqrt(2)`).
/// Returns `(normalised_points, T)` such that `[x', y', 1]^T = T · [x, y, 1]^T`.
pub(crate) fn hartley_normalize(pts: &[(f64, f64)]) -> Option<(Vec<(f64, f64)>, Mat3)> {
    if pts.is_empty() {
        return None;
    }
    let n = pts.len() as f64;
    let mx = pts.iter().map(|p| p.0).sum::<f64>() / n;
    let my = pts.iter().map(|p| p.1).sum::<f64>() / n;
    let mean_dist = pts.iter().map(|p| (p.0 - mx).hypot(p.1 - my)).sum::<f64>() / n;
    if !mean_dist.is_finite() || mean_dist < 1e-12 {
        return None;
    }
    let s = std::f64::consts::SQRT_2 / mean_dist;
    let norm = pts.iter().map(|p| ((p.0 - mx) * s, (p.1 - my) * s)).collect();
    let t = [[s, 0.0, -s * mx], [0.0, s, -s * my], [0.0, 0.0, 1.0]];
    Some((norm, t))
}

/// Solve dense `n×n` linear system `a * x = b` via Gaussian elimination with partial pivoting.
pub(crate) fn solve_linear(a: &mut [f64], b: &mut [f64], n: usize) -> Option<Vec<f64>> {
    if n == 0 || a.len() < n * n || b.len() < n {
        return None;
    }
    for k in 0..n {
        let mut piv = k;
        let mut max_val = a[k * n + k].abs();
        for r in (k + 1)..n {
            let val = a[r * n + k].abs();
            if val > max_val {
                max_val = val;
                piv = r;
            }
        }
        if !max_val.is_finite() || max_val < 1e-12 {
            return None;
        }
        if piv != k {
            for c in k..n {
                a.swap(k * n + c, piv * n + c);
            }
            b.swap(k, piv);
        }
        let inv_piv = 1.0 / a[k * n + k];
        for r in (k + 1)..n {
            let factor = a[r * n + k] * inv_piv;
            if factor == 0.0 {
                continue;
            }
            a[r * n + k] = 0.0;
            for c in (k + 1)..n {
                a[r * n + c] -= factor * a[k * n + c];
            }
            b[r] -= factor * b[k];
        }
    }
    let mut x = vec![0.0; n];
    for k in (0..n).rev() {
        let mut sum = b[k];
        for c in (k + 1)..n {
            sum -= a[k * n + c] * x[c];
        }
        let diag = a[k * n + k];
        if !diag.is_finite() || diag.abs() < 1e-14 {
            return None;
        }
        x[k] = sum / diag;
    }
    x.iter().all(|v| v.is_finite()).then_some(x)
}

/// Median of a slice of `f64` values (`0.0` when empty).
pub(crate) fn median_f64(values: &mut [f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f64::total_cmp);
    let mid = values.len() / 2;
    if values.len() % 2 == 1 { values[mid] } else { 0.5 * (values[mid - 1] + values[mid]) }
}
