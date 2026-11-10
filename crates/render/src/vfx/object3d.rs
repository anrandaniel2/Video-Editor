//! Software 3D objects for the `object_3d` effect.
//!
//! The effect is a small z-buffered renderer: parametric meshes (cube, sphere, cylinder, cone,
//! torus, plane) and extruded text are lit in linear light and composited over the clip. Text keeps
//! the picked font/style/size and colours; the front face is the glyph fill, the walls take the side
//! colour, and a chamfer bevel is built from inset contours. Motion tracks drive position, depth,
//! scale and Z rotation when Follow Motion Track is on.

use filmcraft_project::EffectInstance;
use filmcraft_project::graphic::TextProps;
use filmcraft_text::render;

use crate::effects::FxCtx;
use crate::image::Image;

use super::{bv, chv, cv, fv, lin, tv};

#[derive(Clone, Copy, Debug)]
struct V3 {
    x: f32,
    y: f32,
    z: f32,
}

impl V3 {
    const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
    fn add(self, o: Self) -> Self {
        Self::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
    fn sub(self, o: Self) -> Self {
        Self::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
    fn mul(self, k: f32) -> Self {
        Self::new(self.x * k, self.y * k, self.z * k)
    }
    fn dot(self, o: Self) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    fn cross(self, o: Self) -> Self {
        Self::new(self.y * o.z - self.z * o.y, self.z * o.x - self.x * o.z, self.x * o.y - self.y * o.x)
    }
    fn len(self) -> f32 {
        self.dot(self).sqrt()
    }
    fn norm(self) -> Self {
        let l = self.len();
        if l > 1e-8 { self.mul(1.0 / l) } else { Self::new(0.0, 0.0, 1.0) }
    }
}

struct Params {
    mesh: u32,
    text: String,
    font: String,
    font_style: String,
    align: u32,
    tracking: f32,
    kerning: bool,
    ligatures: bool,
    leading: f32,
    baseline_shift: f32,
    faux_bold: bool,
    faux_italic: bool,
    caps: u32,
    underline: bool,
    box_width: f32,
    vertical: bool,
    size: f32,
    depth: f32,
    bevel: f32,
    pos: (f32, f32),
    z: f32,
    rot: V3,
    scale: f32,
    color: [f32; 4],
    side: [f32; 4],
    metal: f32,
    rough: f32,
    light: V3,
    opacity: f32,
    model_path: String,
    model_anim: i32,
    model_time: f32,
    model_loop: bool,
    model_speed: f32,
    model_play: bool,
}

fn rotate(v: V3, r: V3) -> V3 {
    let (sx, cx) = r.x.to_radians().sin_cos();
    let (sy, cy) = r.y.to_radians().sin_cos();
    let (sz, cz) = r.z.to_radians().sin_cos();
    let y = v.y * cx - v.z * sx;
    let z = v.y * sx + v.z * cx;
    let x = v.x * cy + z * sy;
    let z = -v.x * sy + z * cy;
    V3::new(x * cz - y * sz, x * sz + y * cz, z)
}

fn read_params(e: &EffectInstance, cx: &FxCtx, img: &Image) -> Params {
    let px = cx.px_scale.max(1e-4);
    let mut pos = crate::effects::point(e, "position", cx, img);
    let mut z = fv(e, "z", cx) * px;
    let mut scale = fv(e, "scale", cx) / 100.0;
    let mut rz = fv(e, "rotation_z", cx);
    let track_id = fv(e, "track_id", cx).max(0.0) as u64;
    if bv(e, "follow_track") && track_id != 0 {
        if let Some(s) = cx.env.and_then(|env| env.motion_track(track_id, cx.t)) {
            let off = e.param("position").map(|p| p.vec2_at(cx.t)).unwrap_or_default();
            let ox = if off.x.is_nan() { 0.0 } else { off.x as f32 * px };
            let oy = if off.y.is_nan() { 0.0 } else { off.y as f32 * px };
            pos = filmcraft_geom::Vec2::new((s.x * img.w as f32 + ox) as f64, (s.y * img.h as f32 + oy) as f64);
            z += s.z * fv(e, "track_depth", cx) * px;
            scale *= s.scale.max(0.01);
            rz += s.rotation;
        }
    }
    let la = fv(e, "light_angle", cx).to_radians();
    let le = fv(e, "light_elevation", cx).to_radians();
    Params {
        mesh: chv(e, "mesh"),
        text: tv(e, "text"),
        font: tv(e, "font"),
        font_style: tv(e, "font_style"),
        align: chv(e, "align"),
        tracking: fv(e, "tracking", cx) * px,
        kerning: bv(e, "kerning"),
        ligatures: bv(e, "ligatures"),
        leading: fv(e, "leading", cx) * px,
        baseline_shift: fv(e, "baseline_shift", cx) * px,
        faux_bold: bv(e, "faux_bold"),
        faux_italic: bv(e, "faux_italic"),
        caps: chv(e, "caps"),
        underline: bv(e, "underline"),
        box_width: fv(e, "box_width", cx).max(0.0) * px,
        vertical: bv(e, "vertical"),
        size: fv(e, "size", cx).max(1.0) * px,
        depth: fv(e, "depth", cx).max(0.0) * px,
        bevel: fv(e, "bevel", cx).max(0.0) * px,
        pos: (pos.x as f32, pos.y as f32),
        z: z.max(8.0 * px),
        rot: V3::new(fv(e, "rotation_x", cx), fv(e, "rotation_y", cx), rz),
        scale: scale.max(0.0),
        color: cv(e, "color", cx),
        side: cv(e, "side_color", cx),
        metal: (fv(e, "metalness", cx) / 100.0).clamp(0.0, 1.0),
        rough: (fv(e, "roughness", cx) / 100.0).clamp(0.0, 1.0),
        light: V3::new(la.cos() * le.cos(), la.sin() * le.cos(), le.sin()).norm(),
        opacity: (fv(e, "opacity", cx) / 100.0).clamp(0.0, 1.0),
        model_path: tv(e, "model_path"),
        model_anim: fv(e, "model_anim", cx).round() as i32,
        model_time: fv(e, "model_time", cx),
        model_loop: bv(e, "model_loop"),
        model_speed: fv(e, "model_speed", cx) / 100.0,
        model_play: bv(e, "model_play"),
    }
}

/// The rasteriser's target: the object's bounding box inside the layer, optionally at 2x so
/// silhouettes and highlights are antialiased. `x0`/`y0` are whole layer pixels.
struct Buf {
    x0: i32,
    y0: i32,
    ss: usize,
    w: usize,
    h: usize,
    px: Vec<[f32; 4]>,
    z: Vec<f32>,
}

impl Buf {
    /// A buffer over the layer rectangle covering `bounds` (in layer pixels), supersampled when
    /// that stays affordable. Nothing is allocated for an object that is entirely off screen.
    fn new(img: &Image, bounds: (f32, f32, f32, f32)) -> Option<Self> {
        let (bx0, by0, bx1, by1) = bounds;
        if !bx0.is_finite() || !by0.is_finite() || !bx1.is_finite() || !by1.is_finite() {
            return None;
        }
        let x0 = (bx0.floor() as i32 - 2).max(0);
        let y0 = (by0.floor() as i32 - 2).max(0);
        let x1 = (bx1.ceil() as i32 + 2).min(img.w as i32);
        let y1 = (by1.ceil() as i32 + 2).min(img.h as i32);
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
        // 2x2 samples per pixel where the box is small enough; a full-frame object at 4K would
        // otherwise ask for hundreds of megabytes.
        let ss = if w * h <= 1_000_000 { 2 } else { 1 };
        let n = w * ss * h * ss;
        Some(Self { x0, y0, ss, w: w * ss, h: h * ss, px: vec![[0.0; 4]; n], z: vec![0.0; n] })
    }

    /// The layer-pixel centre of a buffer sample.
    fn centre(&self, x: usize, y: usize) -> (f32, f32) {
        let s = self.ss as f32;
        (self.x0 as f32 + (x as f32 + 0.5) / s, self.y0 as f32 + (y as f32 + 0.5) / s)
    }

    /// The buffer range covering a layer-space rectangle.
    fn range(&self, x_lo: f32, x_hi: f32, y_lo: f32, y_hi: f32) -> Option<(usize, usize, usize, usize)> {
        let s = self.ss as f32;
        let to = |v: f32, o: i32| ((v - o as f32) * s).floor() as i64;
        let x0 = to(x_lo, self.x0).max(0) as usize;
        let y0 = to(y_lo, self.y0).max(0) as usize;
        let x1 = to(x_hi, self.x0).min(self.w as i64 - 1);
        let y1 = to(y_hi, self.y0).min(self.h as i64 - 1);
        if x1 < x0 as i64 || y1 < y0 as i64 {
            return None;
        }
        Some((x0, x1 as usize, y0, y1 as usize))
    }

    fn put(&mut self, x: f32, y: f32, inv_z: f32, c: [f32; 4]) {
        if inv_z <= 0.0 || c[3] <= 0.0 {
            return;
        }
        let s = self.ss as f32;
        let xi = ((x - self.x0 as f32) * s).floor() as i64;
        let yi = ((y - self.y0 as f32) * s).floor() as i64;
        if xi < 0 || yi < 0 || xi >= self.w as i64 || yi >= self.h as i64 {
            return;
        }
        let i = yi as usize * self.w + xi as usize;
        if inv_z > self.z[i] {
            self.z[i] = inv_z;
            self.px[i] = c;
        }
    }

    /// Box-filter the samples down to layer pixels and mix them over the layer.
    fn composite(self, img: &mut Image) {
        let s = self.ss;
        let n = (s * s) as f32;
        for y in 0..self.h / s {
            for x in 0..self.w / s {
                let mut acc = [0.0f32; 4];
                for oy in 0..s {
                    for ox in 0..s {
                        let p = self.px[(y * s + oy) * self.w + x * s + ox];
                        for c in 0..4 {
                            acc[c] += p[c];
                        }
                    }
                }
                for c in &mut acc {
                    *c /= n;
                }
                if acc[3] <= 0.0 {
                    continue;
                }
                let (ix, iy) = (self.x0 as usize + x, self.y0 as usize + y);
                if ix >= img.w || iy >= img.h {
                    continue;
                }
                let i = (iy * img.w + ix) * 4;
                super::over(&mut img.px[i..i + 4], acc);
            }
        }
    }
}

fn project(v: V3, cx: f32, cy: f32, f: f32) -> Option<(f32, f32, f32)> {
    if !v.z.is_finite() || v.z <= 1.0 {
        return None;
    }
    let iz = 1.0 / v.z;
    Some((cx + f * v.x * iz, cy + f * v.y * iz, iz))
}

/// The layer rectangle a set of world points projects into.
fn bounds(pts: &[V3], cam: (f32, f32, f32)) -> Option<(f32, f32, f32, f32)> {
    let (cx, cy, f) = cam;
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    let mut any = false;
    for &v in pts {
        if let Some((x, y, _)) = project(v, cx, cy, f) {
            any = true;
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    any.then_some((x0, y0, x1, y1))
}

fn shade(n: V3, p: &Params, base: [f32; 3], alpha: f32) -> [f32; 4] {
    let n = n.norm();
    // The camera looks down +z, so a visible face has a normal pointing back toward -z.
    let n = if n.z > 0.0 { n.mul(-1.0) } else { n };
    // Light lives in camera space: elevation lifts it toward the viewer (-z), so a front face
    // is lit at the default 45° instead of being lit from behind and falling to ambient only.
    let light = V3::new(p.light.x, p.light.y, -p.light.z).norm();
    let diff = n.dot(light).max(0.0);
    let view = V3::new(0.0, 0.0, -1.0);
    let h = light.add(view).norm();
    let shin = 2.0 + (1.0 - p.rough) * 96.0;
    let spec = n.dot(h).max(0.0).powf(shin) * p.metal * 0.9;
    let mut c = [0.0; 3];
    for k in 0..3 {
        c[k] = (base[k] * (0.28 + 0.72 * diff) + spec).clamp(0.0, 1.0) * alpha;
    }
    [c[0], c[1], c[2], alpha]
}

/// A triangle with a colour per corner: shading is interpolated across the face (Gouraud), so
/// a curved surface reads as curved instead of as a fan of flat facets.
fn tri(buf: &mut Buf, pts: [(f32, f32, f32); 3], cols: [[f32; 4]; 3]) {
    let [a, b, d] = pts;
    let area = (b.0 - a.0) * (d.1 - a.1) - (b.1 - a.1) * (d.0 - a.0);
    if area.abs() < 1e-6 {
        return;
    }
    let Some((x0, x1, y0, y1)) = buf.range(a.0.min(b.0).min(d.0), a.0.max(b.0).max(d.0), a.1.min(b.1).min(d.1), a.1.max(b.1).max(d.1)) else {
        return;
    };
    let inv = 1.0 / area;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let (px, py) = buf.centre(x, y);
            let w0 = ((b.0 - a.0) * (py - a.1) - (b.1 - a.1) * (px - a.0)) * inv;
            let w1 = ((d.0 - b.0) * (py - b.1) - (d.1 - b.1) * (px - b.0)) * inv;
            let w2 = 1.0 - w0 - w1;
            if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                continue;
            }
            // w0/w1/w2 are the barycentric weights of d/a/b respectively.
            let iz = w0 * d.2 + w1 * a.2 + w2 * b.2;
            let mut c = [0.0f32; 4];
            for k in 0..4 {
                c[k] = w1 * cols[0][k] + w2 * cols[1][k] + w0 * cols[2][k];
            }
            let i = y * buf.w + x;
            if iz > buf.z[i] {
                buf.z[i] = iz;
                buf.px[i] = c;
            }
        }
    }
}

/// A triangle already in world space (rotated and placed), with a normal per corner.
fn draw_tri(buf: &mut Buf, v: [Vert; 3], p: &Params, base: [f32; 3], alpha: f32, cam: (f32, f32, f32)) {
    let (cx, cy, f) = cam;
    let Some(a) = project(v[0].p, cx, cy, f) else { return };
    let Some(b) = project(v[1].p, cx, cy, f) else { return };
    let Some(d) = project(v[2].p, cx, cy, f) else { return };
    // Two-sided lighting: parametric windings are not all outward, and the z-buffer already
    // keeps the nearest surface; `shade` faces the visible side toward the camera.
    let cols = [shade(v[0].n, p, base, alpha), shade(v[1].n, p, base, alpha), shade(v[2].n, p, base, alpha)];
    tri(buf, [a, b, d], cols);
}

/// A flat-shaded quad already in world space (text walls and bevels).
fn quad(buf: &mut Buf, v: [V3; 4], n: V3, p: &Params, base: [f32; 3], alpha: f32, cam: (f32, f32, f32)) {
    let t = [Vert { p: v[0], n }, Vert { p: v[1], n }, Vert { p: v[2], n }, Vert { p: v[3], n }];
    draw_tri(buf, [t[0], t[1], t[2]], p, base, alpha, cam);
    draw_tri(buf, [t[0], t[2], t[3]], p, base, alpha, cam);
}

/// A mesh vertex: where it is and which way its surface faces. Curved primitives carry the
/// analytic normal at each vertex so shading can be interpolated; the cube and plane carry the
/// face normal at all three corners, which keeps their edges crisp.
#[derive(Clone, Copy)]
struct Vert {
    p: V3,
    n: V3,
}

/// How many segments a circle of `radius` layer pixels needs for its facets to stay under half
/// a pixel: the sagitta of a chord is r(1-cos(θ/2)) ≈ rθ²/8.
fn segments(radius: f32) -> usize {
    ((std::f32::consts::PI * radius.max(1.0).sqrt()).ceil() as usize).clamp(24, 192)
}

/// The triangles of a primitive, scaled to `k` (half width) by `dz` (half depth). `radius` is
/// the object's approximate size on screen, which sets how finely curves are cut.
fn mesh_tris(mesh: u32, k: f32, dz: f32, radius: f32) -> Vec<[Vert; 3]> {
    let mut t: Vec<[Vert; 3]> = Vec::new();
    // Positions scale by (k, k, dz); normals by the inverse transpose, so a stretched sphere
    // still lights like a stretched sphere.
    let vert = |x: f32, y: f32, z: f32, nx: f32, ny: f32, nz: f32| Vert {
        p: V3::new(x * k, y * k, z * dz),
        n: V3::new(nx / k.max(1e-4), ny / k.max(1e-4), nz / dz.max(1e-4)).norm(),
    };
    let seg = segments(radius);
    match mesh {
        1 => {
            let (lat, lon) = ((seg / 2).max(12), seg);
            for i in 0..lat {
                let v0 = std::f32::consts::PI * i as f32 / lat as f32;
                let v1 = std::f32::consts::PI * (i + 1) as f32 / lat as f32;
                for j in 0..lon {
                    let u0 = std::f32::consts::TAU * j as f32 / lon as f32;
                    let u1 = std::f32::consts::TAU * (j + 1) as f32 / lon as f32;
                    let s = |v: f32, u: f32| (v.sin() * u.cos(), v.cos(), v.sin() * u.sin());
                    let (a, b, c, d) = (s(v0, u0), s(v1, u0), s(v1, u1), s(v0, u1));
                    let mv = |(x, y, z): (f32, f32, f32)| vert(x, y, z, x, y, z);
                    let (a, b, c, d) = (mv(a), mv(b), mv(c), mv(d));
                    t.push([a, b, c]);
                    t.push([a, c, d]);
                }
            }
        }
        2 | 3 => {
            let n = seg;
            let top = if mesh == 3 { 0.0 } else { 1.0 };
            // A cone's side leans: its normal tilts up by the slope of the wall.
            let lean = if mesh == 3 { 0.5 } else { 0.0 };
            for j in 0..n {
                let a0 = std::f32::consts::TAU * j as f32 / n as f32;
                let a1 = std::f32::consts::TAU * (j + 1) as f32 / n as f32;
                let (s0, c0, s1, c1) = (a0.sin(), a0.cos(), a1.sin(), a1.cos());
                let b0 = vert(c0, -1.0, s0, c0, lean, s0);
                let b1 = vert(c1, -1.0, s1, c1, lean, s1);
                let t0 = vert(c0 * top, 1.0, s0 * top, c0, lean, s0);
                let t1 = vert(c1 * top, 1.0, s1 * top, c1, lean, s1);
                if mesh == 3 {
                    // The apex is a point: its normal is the average of the wall around it.
                    let apex = vert(0.0, 1.0, 0.0, (c0 + c1) * 0.5, 1.0, (s0 + s1) * 0.5);
                    t.push([b0, b1, apex]);
                } else {
                    t.push([b0, b1, t1]);
                    t.push([b0, t1, t0]);
                    let cap = vert(0.0, 1.0, 0.0, 0.0, 1.0, 0.0);
                    t.push([cap, t0, t1]);
                }
                let base = vert(0.0, -1.0, 0.0, 0.0, -1.0, 0.0);
                t.push([base, b1, b0]);
            }
        }
        4 => {
            let (n, m) = (seg, (seg / 2).max(12));
            let (r0, r1) = (0.72f32, 0.28f32);
            for i in 0..n {
                let a0 = std::f32::consts::TAU * i as f32 / n as f32;
                let a1 = std::f32::consts::TAU * (i + 1) as f32 / n as f32;
                for j in 0..m {
                    let b0 = std::f32::consts::TAU * j as f32 / m as f32;
                    let b1 = std::f32::consts::TAU * (j + 1) as f32 / m as f32;
                    let s = |a: f32, b: f32| {
                        let (sb, cb) = (b.sin(), b.cos());
                        let (sa, ca) = (a.sin(), a.cos());
                        let r = r0 + r1 * cb;
                        vert(r * ca, r1 * sb, r * sa, cb * ca, sb, cb * sa)
                    };
                    let (a, b, c, d) = (s(a0, b0), s(a1, b0), s(a1, b1), s(a0, b1));
                    t.push([a, b, c]);
                    t.push([a, c, d]);
                }
            }
        }
        5 => {
            let v = [
                vert(-1.0, -1.0, 0.0, 0.0, 0.0, -1.0),
                vert(1.0, -1.0, 0.0, 0.0, 0.0, -1.0),
                vert(1.0, 1.0, 0.0, 0.0, 0.0, -1.0),
                vert(-1.0, 1.0, 0.0, 0.0, 0.0, -1.0),
            ];
            t.push([v[0], v[1], v[2]]);
            t.push([v[0], v[2], v[3]]);
        }
        _ => {
            let v = [
                (-1.0, -1.0, -1.0),
                (1.0, -1.0, -1.0),
                (1.0, 1.0, -1.0),
                (-1.0, 1.0, -1.0),
                (-1.0, -1.0, 1.0),
                (1.0, -1.0, 1.0),
                (1.0, 1.0, 1.0),
                (-1.0, 1.0, 1.0),
            ];
            let f = [[0, 1, 2, 3], [5, 4, 7, 6], [4, 0, 3, 7], [1, 5, 6, 2], [3, 2, 6, 7], [4, 5, 1, 0]];
            for q in f {
                let a = V3::new(v[q[0]].0, v[q[0]].1, v[q[0]].2);
                let b = V3::new(v[q[1]].0, v[q[1]].1, v[q[1]].2);
                let c = V3::new(v[q[2]].0, v[q[2]].1, v[q[2]].2);
                let n = b.sub(a).cross(c.sub(a)).norm();
                let mv = |i: usize| vert(v[i].0, v[i].1, v[i].2, n.x, n.y, n.z);
                let (a, b, c, d) = (mv(q[0]), mv(q[1]), mv(q[2]), mv(q[3]));
                t.push([a, b, c]);
                t.push([a, c, d]);
            }
        }
    }
    t
}

fn text_contours(p: &Params) -> Vec<Vec<(f32, f32)>> {
    if p.text.trim().is_empty() {
        return Vec::new();
    }
    let props = TextProps {
        text: p.text.clone(),
        font: p.font.clone(),
        style: p.font_style.clone(),
        size: p.size.max(1.0),
        align: p.align,
        tracking: p.tracking,
        kerning: p.kerning,
        ligatures: p.ligatures,
        leading: p.leading,
        baseline_shift: p.baseline_shift,
        faux_bold: p.faux_bold,
        faux_italic: p.faux_italic,
        caps: p.caps,
        underline: p.underline,
        box_width: p.box_width,
        box_height: 0.0,
        vertical: p.vertical,
        runs: Vec::new(),
    };
    let l = crate::graphic_clip::text_layout(&props);
    let path = render::layout_path(&l, &[1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    let Some((x0, y0, x1, y1)) = path.bounds() else { return Vec::new() };
    let (mx, my) = ((x0 + x1) * 0.5, (y0 + y1) * 0.5);
    let mut cs: Vec<Vec<(f32, f32)>> = path
        .contours
        .into_iter()
        .filter(|c| c.len() >= 3)
        .map(|c| c.into_iter().map(|(x, y)| ((x - mx) * p.scale, (y - my) * p.scale)).collect())
        .collect();
    let total: usize = cs.iter().map(|c| c.len()).sum();
    if total > 12_000 {
        let step = total.div_ceil(12_000).max(2);
        for c in &mut cs {
            let mut out = Vec::new();
            for (i, pt) in c.iter().enumerate() {
                if i % step == 0 || i + 1 == c.len() {
                    out.push(*pt);
                }
            }
            if out.len() >= 3 {
                *c = out;
            }
        }
    }
    cs
}

fn inset(c: &[(f32, f32)], k: f32) -> Vec<(f32, f32)> {
    if c.is_empty() {
        return Vec::new();
    }
    let (mut sx, mut sy) = (0.0, 0.0);
    for &(x, y) in c {
        sx += x;
        sy += y;
    }
    let (mx, my) = (sx / c.len() as f32, sy / c.len() as f32);
    c.iter().map(|&(x, y)| (mx + (x - mx) * k, my + (y - my) * k)).collect()
}

fn face_plane(pts: &[(f32, f32, f32)]) -> Option<(f32, f32, f32)> {
    let a = *pts.first()?;
    for i in 1..pts.len() {
        for j in i + 1..pts.len() {
            let b = pts[i];
            let c = pts[j];
            let d = (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
            if d.abs() > 1e-4 {
                let dx = (b.2 - a.2) * (c.1 - a.1) - (b.1 - a.1) * (c.2 - a.2);
                let dy = (b.0 - a.0) * (c.2 - a.2) - (b.2 - a.2) * (c.0 - a.0);
                let ia = dx / d;
                let ib = dy / d;
                return Some((ia, ib, a.2 - ia * a.0 - ib * a.1));
            }
        }
    }
    None
}

fn fill_face(buf: &mut Buf, contours: &[Vec<(f32, f32)>], plane: (f32, f32, f32), c: [f32; 4]) {
    if contours.is_empty() || c[3] <= 0.0 {
        return;
    }
    let (mut xmin, mut ymin, mut xmax, mut ymax) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for ct in contours {
        for &(x, y) in ct {
            xmin = xmin.min(x);
            xmax = xmax.max(x);
            ymin = ymin.min(y);
            ymax = ymax.max(y);
        }
    }
    let Some((bx0, bx1, by0, by1)) = buf.range(xmin, xmax, ymin, ymax) else { return };
    let (pa, pb, pc) = plane;
    let ss = buf.ss as f32;
    let mut xs = Vec::new();
    for by in by0..=by1 {
        let py = buf.y0 as f32 + (by as f32 + 0.5) / ss;
        xs.clear();
        for ct in contours {
            for i in 0..ct.len() {
                let (x0, ya) = ct[i];
                let (x1, yb) = ct[(i + 1) % ct.len()];
                if (ya <= py && yb > py) || (yb <= py && ya > py) {
                    xs.push(x0 + (py - ya) * (x1 - x0) / (yb - ya));
                }
            }
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        for pair in xs.chunks_exact(2) {
            let xa = (((pair[0] - buf.x0 as f32) * ss).ceil() as i64).max(bx0 as i64);
            let xb = (((pair[1] - buf.x0 as f32) * ss).floor() as i64).min(bx1 as i64);
            if xa > xb || xa >= buf.w as i64 {
                continue;
            }
            for bx in xa as usize..=xb as usize {
                let px = buf.x0 as f32 + (bx as f32 + 0.5) / ss;
                let iz = pa * px + pb * py + pc;
                let i = by * buf.w + bx;
                if iz > buf.z[i] {
                    buf.z[i] = iz;
                    buf.px[i] = c;
                }
            }
        }
    }
}

fn draw_text(buf: &mut Buf, p: &Params, cam: (f32, f32, f32), cs: &[Vec<(f32, f32)>]) {
    if cs.is_empty() {
        return;
    }
    let (cx, cy, f) = cam;
    // The camera looks down +z, so the front face sits at negative z (nearer the viewer).
    let depth = p.depth.max(0.5);
    let bevel = p.bevel.min(depth * 0.45);
    let front = if bevel > 0.25 { cs.iter().map(|c| inset(c, 0.94)).collect::<Vec<_>>() } else { cs.to_vec() };
    let alpha = p.opacity * p.color[3].clamp(0.0, 1.0);
    let side_alpha = p.opacity * p.side[3].clamp(0.0, 1.0);
    let base = lin(p.color);
    let side = lin(p.side);
    // Glyphs are centred on the origin; `cam` carries the position (see `apply`).
    let to_v = |x: f32, y: f32, z: f32| rotate(V3::new(x, y, z), p.rot).add(V3::new(0.0, 0.0, p.z));
    let proj_ct = |ct: &[(f32, f32)], z: f32| -> Vec<(f32, f32)> {
        ct.iter().filter_map(|&(x, y)| project(to_v(x, y, z), cx, cy, f).map(|(sx, sy, _)| (sx, sy))).collect()
    };
    // Back and front faces (planar; even-odd scanline fill keeps glyph holes open).
    for (ct, z, n, col, a) in [(cs, 0.0, V3::new(0.0, 0.0, 1.0), side, side_alpha), (front.as_slice(), -depth, V3::new(0.0, 0.0, -1.0), base, alpha)] {
        let pc: Vec<Vec<(f32, f32)>> = ct.iter().map(|c| proj_ct(c, z)).filter(|c| c.len() >= 3).collect();
        let flat: Vec<(f32, f32, f32)> = ct
            .iter()
            .flat_map(|c| c.iter().filter_map(move |&(x, y)| project(to_v(x, y, z), cx, cy, f)))
            .collect();
        if let Some(plane) = face_plane(&flat) {
            fill_face(buf, &pc, plane, shade(rotate(n, p.rot), p, col, a));
        }
    }
    // Walls and chamfer.
    for ct in cs {
        let ins = inset(ct, 0.94);
        for i in 0..ct.len() {
            let j = (i + 1) % ct.len();
            let (x0, y0) = ct[i];
            let (x1, y1) = ct[j];
            let n = rotate(V3::new(y1 - y0, x0 - x1, 0.0).norm(), p.rot);
            let z_top = -depth + bevel;
            quad(
                buf,
                [to_v(x0, y0, 0.0), to_v(x1, y1, 0.0), to_v(x1, y1, z_top), to_v(x0, y0, z_top)],
                n,
                p,
                side,
                side_alpha,
                cam,
            );
            if bevel > 0.25 && ins.len() == ct.len() {
                let (bx0, by0) = ins[i];
                let (bx1, by1) = ins[j];
                let bn = rotate(V3::new(y1 - y0, x0 - x1, -(x1 - x0).hypot(y1 - y0) * 0.7).norm(), p.rot);
                quad(
                    buf,
                    [to_v(x0, y0, z_top), to_v(x1, y1, z_top), to_v(bx1, by1, -depth), to_v(bx0, by0, -depth)],
                    bn,
                    p,
                    base,
                    alpha,
                    cam,
                );
            }
        }
    }
}

/// A model vertex after fitting, node/skin animation and the object's own transform.
#[derive(Clone, Copy)]
struct ModelVert {
    p: V3,
    n: V3,
    tangent: V3,
    tangent_w: f32,
    uv: [[f32; 2]; 2],
    color: [f32; 4],
}

impl ModelVert {
    fn lerp(a: Self, b: Self, t: f32) -> Self {
        let mix = |x: f32, y: f32| x + (y - x) * t;
        let mix3 = |x: V3, y: V3| V3::new(mix(x.x, y.x), mix(x.y, y.y), mix(x.z, y.z));
        Self {
            p: mix3(a.p, b.p), n: mix3(a.n, b.n), tangent: mix3(a.tangent, b.tangent),
            tangent_w: mix(a.tangent_w, b.tangent_w),
            uv: [std::array::from_fn(|k| mix(a.uv[0][k], b.uv[0][k])), std::array::from_fn(|k| mix(a.uv[1][k], b.uv[1][k]))],
            color: std::array::from_fn(|k| mix(a.color[k], b.color[k])),
        }
    }
}

/// Clip against z = 1 before projection, so moving a model through the camera does not make
/// triangles vanish or explode. Returns a triangle fan (at most two triangles from one input).
fn clip_near(v: [ModelVert; 3]) -> Vec<[ModelVert; 3]> {
    const NEAR: f32 = 1.01;
    let mut poly: Vec<ModelVert> = Vec::with_capacity(4);
    for i in 0..3 {
        let a = v[i];
        let b = v[(i + 1) % 3];
        let ain = a.p.z >= NEAR;
        let bin = b.p.z >= NEAR;
        if ain { poly.push(a); }
        if ain != bin {
            let t = (NEAR - a.p.z) / (b.p.z - a.p.z);
            poly.push(ModelVert::lerp(a, b, t));
        }
    }
    if poly.len() < 3 { return Vec::new(); }
    (1..poly.len() - 1).map(|i| [poly[0], poly[i], poly[i + 1]]).collect()
}

fn sample_map(model: &crate::model::Model, r: Option<crate::model::TextureRef>, uv: [[f32; 2]; 2]) -> Option<[f32; 4]> {
    let r = r?;
    let (u, v) = r.uv(uv);
    model.textures.get(r.index).map(|t| t.sample(u, v))
}

/// Metallic-roughness shading with a GGX specular lobe. The editor's current directional light
/// is used until the scene-light objects arrive; ambient is a cheap diffuse/specular environment.
fn shade_pbr(n: V3, view: V3, light: V3, base: [f32; 3], metallic: f32, roughness: f32, ao: f32, emissive: [f32; 3], alpha: f32, unlit: bool) -> [f32; 4] {
    if unlit {
        return [(base[0] + emissive[0]) * alpha, (base[1] + emissive[1]) * alpha, (base[2] + emissive[2]) * alpha, alpha];
    }
    let n = n.norm();
    let v = view.norm();
    let l = light.norm();
    let h = v.add(l).norm();
    let nl = n.dot(l).max(0.0);
    let nv = n.dot(v).max(1e-4);
    let nh = n.dot(h).max(0.0);
    let vh = v.dot(h).max(0.0);
    let rough = roughness.clamp(0.045, 1.0);
    let a = rough * rough;
    let a2 = a * a;
    let den = nh * nh * (a2 - 1.0) + 1.0;
    let d = a2 / (std::f32::consts::PI * den * den).max(1e-6);
    let k = (rough + 1.0) * (rough + 1.0) / 8.0;
    let g = (nv / (nv * (1.0 - k) + k)) * (nl / (nl * (1.0 - k) + k).max(1e-6));
    let f0 = std::array::from_fn::<_, 3, _>(|i| 0.04 + (base[i] - 0.04) * metallic);
    let f = std::array::from_fn::<_, 3, _>(|i| f0[i] + (1.0 - f0[i]) * (1.0 - vh).powi(5));
    let mut c = [0.0f32; 3];
    for i in 0..3 {
        let kd = (1.0 - f[i]) * (1.0 - metallic);
        let spec = d * g * f[i] / (4.0 * nv * nl).max(1e-5);
        let direct = (kd * base[i] / std::f32::consts::PI + spec) * nl * 3.0;
        let ambient_diffuse = base[i] * (1.0 - metallic) * 0.22 * ao;
        let ambient_spec = f0[i] * (0.08 + 0.12 * (1.0 - rough)) * ao;
        c[i] = ((direct + ambient_diffuse + ambient_spec + emissive[i]).max(0.0)) * alpha;
    }
    [c[0], c[1], c[2], alpha]
}

/// Perspective-correct textured triangle. `blend` draws back-to-front transparent surfaces over
/// the opaque z-buffer without writing depth, so glass does not hide surfaces behind it.
fn model_tri(buf: &mut Buf, pts: [(f32, f32, f32); 3], v: [ModelVert; 3], model: &crate::model::Model, mat: &crate::model::Material, p: &Params, light: V3, blend: bool) {
    let [a, b, c] = pts;
    let area = (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
    if area.abs() < 1e-7 { return; }
    let Some((x0, x1, y0, y1)) = buf.range(a.0.min(b.0).min(c.0), a.0.max(b.0).max(c.0), a.1.min(b.1).min(c.1), a.1.max(b.1).max(c.1)) else { return };
    let inv = 1.0 / area;
    // A tangent frame from UV deltas when the file has no TANGENT attribute.
    let e1 = v[1].p.sub(v[0].p);
    let e2 = v[2].p.sub(v[0].p);
    let set = mat.normal_texture.map(|r| r.uv_set).unwrap_or(0);
    let d1 = [v[1].uv[set][0] - v[0].uv[set][0], v[1].uv[set][1] - v[0].uv[set][1]];
    let d2 = [v[2].uv[set][0] - v[0].uv[set][0], v[2].uv[set][1] - v[0].uv[set][1]];
    let det = d1[0] * d2[1] - d1[1] * d2[0];
    let fallback = if det.abs() > 1e-10 { e1.mul(d2[1]).sub(e2.mul(d1[1])).mul(1.0 / det).norm() } else { V3::new(1.0, 0.0, 0.0) };
    for y in y0..=y1 {
        for x in x0..=x1 {
            let (px, py) = buf.centre(x, y);
            let w0 = ((c.0 - b.0) * (py - b.1) - (c.1 - b.1) * (px - b.0)) * inv;
            let w1 = ((a.0 - c.0) * (py - c.1) - (a.1 - c.1) * (px - c.0)) * inv;
            let w2 = 1.0 - w0 - w1;
            if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 { continue; }
            let iz = w0 * a.2 + w1 * b.2 + w2 * c.2;
            if iz <= 0.0 { continue; }
            let idx = y * buf.w + x;
            if iz <= buf.z[idx] { continue; }
            let q = [w0 * a.2 / iz, w1 * b.2 / iz, w2 * c.2 / iz];
            let mix3 = |get: &dyn Fn(&ModelVert) -> V3| get(&v[0]).mul(q[0]).add(get(&v[1]).mul(q[1])).add(get(&v[2]).mul(q[2]));
            let pos = mix3(&|v| v.p);
            let mut n = mix3(&|v| v.n).norm();
            let uv = [
                std::array::from_fn::<_, 2, _>(|k| v[0].uv[0][k] * q[0] + v[1].uv[0][k] * q[1] + v[2].uv[0][k] * q[2]),
                std::array::from_fn::<_, 2, _>(|k| v[0].uv[1][k] * q[0] + v[1].uv[1][k] * q[1] + v[2].uv[1][k] * q[2]),
            ];
            let vc = std::array::from_fn::<_, 4, _>(|k| v[0].color[k] * q[0] + v[1].color[k] * q[1] + v[2].color[k] * q[2]);
            let view = pos.mul(-1.0).norm();
            if n.dot(view) < 0.0 { n = n.mul(-1.0); }
            if let Some(nt) = sample_map(model, mat.normal_texture, uv) {
                let mut t = mix3(&|v| if v.tangent.len() > 1e-6 { v.tangent } else { fallback });
                t = t.sub(n.mul(t.dot(n))).norm();
                let tw = v[0].tangent_w * q[0] + v[1].tangent_w * q[1] + v[2].tangent_w * q[2];
                let bit = n.cross(t).mul(if tw < 0.0 { -1.0 } else { 1.0 });
                let tn = V3::new((nt[0] * 2.0 - 1.0) * mat.normal_scale, (nt[1] * 2.0 - 1.0) * mat.normal_scale, (nt[2] * 2.0 - 1.0).max(1e-4));
                n = t.mul(tn.x).add(bit.mul(tn.y)).add(n.mul(tn.z)).norm();
            }
            let tex = sample_map(model, mat.base_texture, uv).unwrap_or([1.0; 4]);
            let base = std::array::from_fn::<_, 3, _>(|i| (mat.base_color[i] * tex[i] * vc[i]).max(0.0));
            let mr = sample_map(model, mat.mr_texture, uv).unwrap_or([1.0; 4]);
            let metallic = (mat.metallic * mr[2]).clamp(0.0, 1.0);
            let rough = (mat.roughness * mr[1]).clamp(0.0, 1.0);
            let occ = sample_map(model, mat.occlusion_texture, uv).map(|x| 1.0 + mat.occlusion_strength * (x[0] - 1.0)).unwrap_or(1.0);
            let et = sample_map(model, mat.emissive_texture, uv).unwrap_or([1.0; 4]);
            let emissive = std::array::from_fn::<_, 3, _>(|i| mat.emissive[i] * et[i]);
            let mut alpha = mat.base_color[3] * tex[3] * vc[3] * p.opacity;
            match mat.alpha_mode {
                crate::model::AlphaMode::Opaque => alpha = p.opacity,
                crate::model::AlphaMode::Mask => { if alpha < mat.alpha_cutoff { continue; } alpha = p.opacity; }
                crate::model::AlphaMode::Blend => {}
            }
            if alpha <= 0.0 { continue; }
            let col = shade_pbr(n, view, light, base, metallic, rough, occ, emissive, alpha.clamp(0.0, 1.0), mat.unlit);
            if blend {
                let dst = &mut buf.px[idx];
                super::over(dst, col);
            } else {
                buf.z[idx] = iz;
                buf.px[idx] = col;
            }
        }
    }
}

fn draw_model(img: &mut Image, _e: &EffectInstance, p: &Params, cx: &FxCtx, cam: (f32, f32, f32)) {
    if p.model_path.is_empty() { return; }
    let Ok(model) = crate::model::load_cached(std::path::Path::new(&p.model_path)) else { return };
    let anim = if p.model_anim >= 0 { Some(p.model_anim as usize) } else { None };
    let time = p.model_time + if p.model_play { cx.seconds as f32 * p.model_speed } else { 0.0 };
    let pose = model.pose(anim, time, p.model_loop);
    let center = std::array::from_fn::<_, 3, _>(|i| (model.bounds.0[i] + model.bounds.1[i]) * 0.5);
    let extent = (0..3).map(|i| model.bounds.1[i] - model.bounds.0[i]).fold(0.0f32, f32::max).max(1e-6);
    let fit = p.size * p.scale / extent;
    let off = V3::new(0.0, 0.0, p.z);
    // glTF is right-handed Y-up, +Z toward the viewer. The object renderer looks down +Z with
    // Y down, so rotate 180° about X: (x, y, z) -> (x, -y, -z), preserving winding.
    let place = |v: crate::model::Vertex| {
        let local = V3::new((v.pos[0] - center[0]) * fit, -(v.pos[1] - center[1]) * fit, -(v.pos[2] - center[2]) * fit);
        let n = V3::new(v.normal[0], -v.normal[1], -v.normal[2]);
        let t = V3::new(v.tangent[0], -v.tangent[1], -v.tangent[2]);
        ModelVert { p: rotate(local, p.rot).add(off), n: rotate(n, p.rot), tangent: rotate(t, p.rot), tangent_w: v.tangent[3], uv: v.uv, color: v.color }
    };
    let mut prepared: Vec<(&crate::model::Primitive, Vec<ModelVert>)> = Vec::new();
    let (mut bx0, mut by0, mut bx1, mut by1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    let mut any = false;
    for inst in &model.instances {
        for prim in &model.meshes[inst.mesh].prims {
            let verts: Vec<ModelVert> = (0..prim.verts.len()).map(|i| place(model.skin_vertex(&pose, inst, prim, i))).collect();
            for t in prim.indices.chunks_exact(3) {
                let tri = [verts[t[0] as usize], verts[t[1] as usize], verts[t[2] as usize]];
                for clipped in clip_near(tri) {
                    for v in clipped {
                        if let Some((x, y, _)) = project(v.p, cam.0, cam.1, cam.2) {
                            any = true; bx0 = bx0.min(x); by0 = by0.min(y); bx1 = bx1.max(x); by1 = by1.max(y);
                        }
                    }
                }
            }
            prepared.push((prim, verts));
        }
    }
    if !any { return; }
    let Some(mut buf) = Buf::new(img, (bx0, by0, bx1, by1)) else { return };
    let light = V3::new(p.light.x, p.light.y, -p.light.z).norm();
    // Opaque/mask first. Transparent triangles are sorted far-to-near and depth-tested against
    // the opaque buffer, but do not write depth themselves.
    let mut transparent: Vec<(usize, usize, f32)> = Vec::new();
    let draw_list: Vec<(&crate::model::Primitive, &[ModelVert])> = prepared.iter().map(|(prim, verts)| (*prim, verts.as_slice())).collect();
    for (ti, (prim, verts)) in draw_list.iter().enumerate() {
        let mat = model.material(prim.material);
        for (ii, t) in prim.indices.chunks_exact(3).enumerate() {
            let tri = [verts[t[0] as usize], verts[t[1] as usize], verts[t[2] as usize]];
            let depth = (tri[0].p.z + tri[1].p.z + tri[2].p.z) / 3.0;
            if mat.alpha_mode == crate::model::AlphaMode::Blend {
                transparent.push((ti, ii, depth));
                continue;
            }
            for clipped in clip_near(tri) {
                let Some(a) = project(clipped[0].p, cam.0, cam.1, cam.2) else { continue };
                let Some(b) = project(clipped[1].p, cam.0, cam.1, cam.2) else { continue };
                let Some(c) = project(clipped[2].p, cam.0, cam.1, cam.2) else { continue };
                model_tri(&mut buf, [a, b, c], clipped, &model, mat, p, light, false);
            }
        }
    }
    transparent.sort_by(|a, b| b.2.total_cmp(&a.2));
    for (ti, ii, _) in transparent {
        let (prim, verts) = draw_list[ti];
        let mat = model.material(prim.material);
        let t = &prim.indices[ii * 3..ii * 3 + 3];
        for clipped in clip_near([verts[t[0] as usize], verts[t[1] as usize], verts[t[2] as usize]]) {
            let Some(a) = project(clipped[0].p, cam.0, cam.1, cam.2) else { continue };
            let Some(b) = project(clipped[1].p, cam.0, cam.1, cam.2) else { continue };
            let Some(c) = project(clipped[2].p, cam.0, cam.1, cam.2) else { continue };
            model_tri(&mut buf, [a, b, c], clipped, &model, mat, p, light, true);
        }
    }
    buf.composite(img);
}

pub(super) fn apply(img: &mut Image, e: &EffectInstance, cx: &FxCtx) {
    if img.w == 0 || img.h == 0 {
        return;
    }
    let p = read_params(e, cx, img);
    if p.opacity <= 0.0 || p.scale <= 0.0 {
        return;
    }
    // `position` is where the object sits in the layer, in layer pixels (the frame centre by
    // default), so it is the camera's principal point. It must not be added to the vertices as
    // well: that would move the object by f/z times its position — a few pixels in a thumbnail,
    // but well over a thousand at 1080p, which put every object outside the frame.
    let cam = (p.pos.0, p.pos.1, (img.h as f32).max(1.0));
    let off = V3::new(0.0, 0.0, p.z);
    if p.mesh == 7 {
        draw_model(img, e, &p, cx, cam);
        return;
    }
    let place = |v: Vert| Vert { p: rotate(v.p, p.rot).add(off), n: rotate(v.n, p.rot) };
    if p.mesh == 6 {
        let cs = text_contours(&p);
        if cs.is_empty() {
            return;
        }
        let depth = p.depth.max(0.5);
        let pts: Vec<V3> = cs
            .iter()
            .flat_map(|c| c.iter())
            .flat_map(|&(x, y)| [rotate(V3::new(x, y, 0.0), p.rot).add(off), rotate(V3::new(x, y, -depth), p.rot).add(off)])
            .collect();
        let Some(b) = bounds(&pts, cam) else { return };
        let Some(mut buf) = Buf::new(img, b) else { return };
        draw_text(&mut buf, &p, cam, &cs);
        buf.composite(img);
    } else {
        let k = p.size * p.scale * 0.5;
        let dz = if p.mesh == 5 { 0.0 } else if p.depth > 0.0 { p.depth * p.scale * 0.5 } else { k };
        let base = lin(p.color);
        let alpha = p.opacity * p.color[3].clamp(0.0, 1.0);
        // Cut curves to the size they actually appear at, so a big sphere is round and a small
        // one does not cost thousands of triangles.
        let radius = k * cam.2 / p.z.max(1.0);
        let tris: Vec<[Vert; 3]> = mesh_tris(p.mesh, k, dz, radius).into_iter().map(|t| [place(t[0]), place(t[1]), place(t[2])]).collect();
        let pts: Vec<V3> = tris.iter().flat_map(|t| t.iter().map(|v| v.p)).collect();
        let Some(b) = bounds(&pts, cam) else { return };
        let Some(mut buf) = Buf::new(img, b) else { return };
        for t in tris {
            draw_tri(&mut buf, t, &p, base, alpha, cam);
        }
        buf.composite(img);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use filmcraft_project::find_effect;
    use filmcraft_time::Tick;

    fn ctx() -> FxCtx<'static> {
        FxCtx {
            t: Tick::ZERO,
            px_scale: 1.0,
            seconds: 0.0,
            timecode: "",
            clip_name: "",
            project: None,
            env: None,
            frame_rate: 30.0,
            working: filmcraft_color::WorkingSpace::Rec709,
        }
    }

    #[test]
    fn cube_draws_something() {
        let e = find_effect("object_3d").unwrap().instance();
        let mut img = Image::new(96, 96);
        apply(&mut img, &e, &ctx());
        let lit = img.px.chunks(4).filter(|p| p[3] > 0.5).count();
        assert!(lit > 20, "cube covered only {lit} pixels");
        assert!(img.px.chunks(4).any(|p| p[3] > 0.5 && p[0] + p[1] + p[2] > 0.05), "cube was black");
    }

    /// Centre, size and pixel count of what was drawn, in pixels of `img`.
    fn drawn(img: &Image) -> (f32, f32, f32, f32, usize) {
        let (mut sx, mut sy, mut n) = (0.0f64, 0.0f64, 0usize);
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0usize, 0usize);
        for (i, p) in img.px.chunks(4).enumerate() {
            if p[3] <= 0.5 {
                continue;
            }
            let (x, y) = (i % img.w, i / img.w);
            sx += x as f64;
            sy += y as f64;
            n += 1;
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        if n == 0 {
            return (f32::NAN, f32::NAN, 0.0, 0.0, 0);
        }
        ((sx / n as f64) as f32, (sy / n as f64) as f32, (x1 - x0 + 1) as f32, (y1 - y0 + 1) as f32, n)
    }

    /// The middle of the bounding box of what was drawn. Unlike the ink centroid this is exactly
    /// the object's position: glyph contours are centred on their bounds before extrusion, and
    /// every face projects symmetrically about the principal point.
    fn box_centre(img: &Image) -> (f32, f32) {
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0usize, 0usize);
        for (i, p) in img.px.chunks(4).enumerate() {
            if p[3] <= 0.5 {
                continue;
            }
            let (x, y) = (i % img.w, i / img.w);
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        ((x0 + x1) as f32 / 2.0, (y0 + y1) as f32 / 2.0)
    }

    /// The regression that mattered on device: at 1080 x 1920 the default position (the frame
    /// centre) used to be added to the vertices before projecting, which threw the object more
    /// than a frame away. Small test images hid it — the error scales with the frame height.
    #[test]
    fn a_default_object_sits_in_the_middle_of_a_full_size_frame() {
        let e = find_effect("object_3d").unwrap().instance();
        for (w, h) in [(1080usize, 1920usize), (1920, 1080)] {
            let mut img = Image::new(w, h);
            apply(&mut img, &e, &ctx());
            let (cx, cy, bw, bh, n) = drawn(&img);
            assert!(n > 50_000, "{w}x{h}: the cube covered only {n} pixels");
            assert!((cx - w as f32 / 2.0).abs() < w as f32 * 0.01, "{w}x{h}: centre x was {cx}");
            assert!((cy - h as f32 / 2.0).abs() < h as f32 * 0.01, "{w}x{h}: centre y was {cy}");
            assert!(bw < w as f32 && bh < h as f32, "{w}x{h}: the cube filled the frame ({bw}x{bh})");
        }
    }

    /// The monitor renders at a fraction of the sequence size; the object must keep its place
    /// and its relative size, not shrink away or drift out of the picture.
    #[test]
    fn preview_scale_keeps_the_object_in_the_same_place() {
        let e = find_effect("object_3d").unwrap().instance();
        let mut full = Image::new(1080, 1920);
        apply(&mut full, &e, &ctx());
        let (fx, fy, fw, fh, _) = drawn(&full);
        let mut small = Image::new(405, 720);
        apply(&mut small, &e, &FxCtx { px_scale: 0.375, ..ctx() });
        let (sx, sy, sw, sh, n) = drawn(&small);
        assert!(n > 5_000, "the preview covered only {n} pixels");
        assert!((sx / 405.0 - fx / 1080.0).abs() < 0.01, "x moved: {sx} vs {fx}");
        assert!((sy / 720.0 - fy / 1920.0).abs() < 0.01, "y moved: {sy} vs {fy}");
        assert!((sw / 405.0 - fw / 1080.0).abs() < 0.02, "width changed: {sw} vs {fw}");
        assert!((sh / 720.0 - fh / 1920.0).abs() < 0.02, "height changed: {sh} vs {fh}");
    }

    /// X and Y are frame pixels, as the sliders and the motion tracks write them.
    #[test]
    fn position_moves_the_object_in_frame_pixels() {
        let mut e = find_effect("object_3d").unwrap().instance();
        e.params.get_mut("size").unwrap().value = filmcraft_project::ParamValue::Float(120.0);
        e.params.get_mut("position").unwrap().value = filmcraft_project::ParamValue::Vec2(filmcraft_geom::Vec2::new(300.0, 500.0));
        let mut img = Image::new(1080, 1920);
        apply(&mut img, &e, &ctx());
        let (cx, cy, _, _, n) = drawn(&img);
        assert!(n > 10_000, "the moved cube covered only {n} pixels");
        assert!((cx - 300.0).abs() < 3.0, "x was {cx}, wanted 300");
        assert!((cy - 500.0).abs() < 3.0, "y was {cy}, wanted 500");
        let mut small = Image::new(405, 720);
        apply(&mut small, &e, &FxCtx { px_scale: 0.375, ..ctx() });
        let (sx, sy, _, _, _) = drawn(&small);
        assert!((sx - 112.5).abs() < 2.0 && (sy - 187.5).abs() < 2.0, "scaled position was ({sx}, {sy})");
    }

    #[test]
    fn extruded_text_sits_in_the_middle_of_a_full_size_frame() {
        let mut e = find_effect("object_3d").unwrap().instance();
        e.params.get_mut("mesh").unwrap().value = filmcraft_project::ParamValue::Choice(6);
        e.params.get_mut("text").unwrap().value = filmcraft_project::ParamValue::Text("Ag".into());
        let mut img = Image::new(1080, 1920);
        apply(&mut img, &e, &ctx());
        let (_, _, _, _, n) = drawn(&img);
        assert!(n > 20_000, "3D text covered only {n} pixels at 1080x1920");
        let (cx, cy) = box_centre(&img);
        assert!((cx - 540.0).abs() < 3.0 && (cy - 960.0).abs() < 3.0, "3D text sat at ({cx}, {cy})");
    }

    /// A curved surface must shade as a gradient, not as a fan of flat facets: a scan across
    /// the middle of a sphere should pass through many distinct brightnesses.
    #[test]
    fn a_sphere_shades_smoothly() {
        let mut e = find_effect("object_3d").unwrap().instance();
        e.params.get_mut("mesh").unwrap().value = filmcraft_project::ParamValue::Choice(1);
        e.params.get_mut("size").unwrap().value = filmcraft_project::ParamValue::Float(300.0);
        let mut img = Image::new(512, 512);
        apply(&mut img, &e, &ctx());
        let row = &img.px[(256 * 512) * 4..(257 * 512) * 4];
        let mut seen = std::collections::HashSet::new();
        for p in row.chunks(4) {
            if p[3] > 0.9 {
                seen.insert(((p[0] * 255.0) as u16, (p[1] * 255.0) as u16, (p[2] * 255.0) as u16));
            }
        }
        assert!(seen.len() > 40, "the sphere had only {} shades across its middle: still faceted", seen.len());
    }

    /// Silhouettes are supersampled, so edges carry partial coverage instead of stair-stepping.
    #[test]
    fn edges_are_antialiased() {
        let mut e = find_effect("object_3d").unwrap().instance();
        e.params.get_mut("mesh").unwrap().value = filmcraft_project::ParamValue::Choice(1);
        let mut img = Image::new(256, 256);
        apply(&mut img, &e, &ctx());
        let partial = img.px.chunks(4).filter(|p| p[3] > 0.02 && p[3] < 0.98).count();
        assert!(partial > 100, "only {partial} partially covered pixels: the edge is not antialiased");
    }

    /// Curves are cut to the size they are drawn at, so a large sphere is rounder than a small one.
    #[test]
    fn tessellation_follows_the_size_on_screen() {
        assert!(segments(10.0) >= 24);
        assert!(segments(400.0) > segments(40.0));
        assert!(segments(100_000.0) <= 192, "tessellation must stay bounded");
    }

    #[test]
    fn extruded_text_draws_something() {
        let mut e = find_effect("object_3d").unwrap().instance();
        e.params.get_mut("mesh").unwrap().value = filmcraft_project::ParamValue::Choice(6);
        e.params.get_mut("text").unwrap().value = filmcraft_project::ParamValue::Text("Ag".into());
        e.params.get_mut("size").unwrap().value = filmcraft_project::ParamValue::Float(120.0);
        let mut img = Image::new(160, 96);
        apply(&mut img, &e, &ctx());
        assert!(img.px.chunks(4).any(|p| p[3] > 0.2 && p[0] + p[1] + p[2] > 0.03), "text was invisible or black");
    }
}
