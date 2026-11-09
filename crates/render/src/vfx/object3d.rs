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
use filmcraft_time::Tick;

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
    }
}

struct Buf {
    w: usize,
    h: usize,
    px: Vec<[f32; 4]>,
    z: Vec<f32>,
}

impl Buf {
    fn new(w: usize, h: usize) -> Self {
        Self { w, h, px: vec![[0.0; 4]; w * h], z: vec![0.0; w * h] }
    }
    fn put(&mut self, x: f32, y: f32, inv_z: f32, c: [f32; 4]) {
        if inv_z <= 0.0 || c[3] <= 0.0 {
            return;
        }
        let xi = x.floor() as isize;
        let yi = y.floor() as isize;
        if xi < 0 || yi < 0 || xi >= self.w as isize || yi >= self.h as isize {
            return;
        }
        let i = yi as usize * self.w + xi as usize;
        if inv_z > self.z[i] {
            self.z[i] = inv_z;
            self.px[i] = c;
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

fn shade(n: V3, p: &Params, base: [f32; 3], alpha: f32) -> [f32; 4] {
    let n = n.norm();
    let diff = n.dot(p.light).max(0.0);
    let view = V3::new(0.0, 0.0, -1.0);
    let h = p.light.add(view).norm();
    let shin = 2.0 + (1.0 - p.rough) * 96.0;
    let spec = n.dot(h).max(0.0).powf(shin) * p.metal * 0.9;
    let mut c = [0.0; 3];
    for k in 0..3 {
        c[k] = (base[k] * (0.28 + 0.72 * diff) + spec).clamp(0.0, 1.0) * alpha;
    }
    [c[0], c[1], c[2], alpha]
}

fn tri(buf: &mut Buf, pts: [(f32, f32, f32); 3], c: [f32; 4]) {
    let [a, b, d] = pts;
    let area = (b.0 - a.0) * (d.1 - a.1) - (b.1 - a.1) * (d.0 - a.0);
    if area.abs() < 1e-6 {
        return;
    }
    let min_x = a.0.min(b.0).min(d.0).floor().max(0.0) as usize;
    let max_x = a.0.max(b.0).max(d.0).ceil().min(buf.w as f32 - 1.0).max(0.0) as usize;
    let min_y = a.1.min(b.1).min(d.1).floor().max(0.0) as usize;
    let max_y = a.1.max(b.1).max(d.1).ceil().min(buf.h as f32 - 1.0).max(0.0) as usize;
    if min_x > max_x || min_y > max_y {
        return;
    }
    for y in min_y..=max_y {
        let py = y as f32 + 0.5;
        for x in min_x..=max_x {
            let px = x as f32 + 0.5;
            let w0 = ((b.0 - a.0) * (py - a.1) - (b.1 - a.1) * (px - a.0)) / area;
            let w1 = ((d.0 - b.0) * (py - b.1) - (d.1 - b.1) * (px - b.0)) / area;
            let w2 = 1.0 - w0 - w1;
            if w0 < -0.001 || w1 < -0.001 || w2 < -0.001 {
                continue;
            }
            // w0/w1/w2 are the barycentric weights of d/a/b respectively.
            let iz = w0 * d.2 + w1 * a.2 + w2 * b.2;
            buf.put(px, py, iz, c);
        }
    }
}

fn draw_tri(buf: &mut Buf, v: [V3; 3], n: V3, p: &Params, base: [f32; 3], alpha: f32, cam: (f32, f32, f32)) {
    let (cx, cy, f) = cam;
    let Some(a) = project(v[0], cx, cy, f) else { return };
    let Some(b) = project(v[1], cx, cy, f) else { return };
    let Some(d) = project(v[2], cx, cy, f) else { return };
    // Two-sided lighting: parametric windings are not all outward, and the z-buffer already
    // keeps the nearest surface, so face the visible side toward the camera for shading.
    let n = rotate(n, p.rot);
    let n = if n.z > 0.0 { n.mul(-1.0) } else { n };
    let c = shade(n, p, base, alpha);
    tri(buf, [a, b, d], c);
}

fn quad(buf: &mut Buf, v: [V3; 4], n: V3, p: &Params, base: [f32; 3], alpha: f32, cam: (f32, f32, f32)) {
    draw_tri(buf, [v[0], v[1], v[2]], n, p, base, alpha, cam);
    draw_tri(buf, [v[0], v[2], v[3]], n, p, base, alpha, cam);
}

fn mesh_tris(mesh: u32) -> Vec<([V3; 3], V3)> {
    let mut t = Vec::new();
    let mut push = |a: V3, b: V3, c: V3| {
        let n = b.sub(a).cross(c.sub(a)).norm();
        t.push(([a, b, c], n));
    };
    match mesh {
        1 => {
            let (lat, lon) = (14usize, 28usize);
            for i in 0..lat {
                let v0 = std::f32::consts::PI * i as f32 / lat as f32;
                let v1 = std::f32::consts::PI * (i + 1) as f32 / lat as f32;
                for j in 0..lon {
                    let u0 = std::f32::consts::TAU * j as f32 / lon as f32;
                    let u1 = std::f32::consts::TAU * (j + 1) as f32 / lon as f32;
                    let s = |v: f32, u: f32| V3::new(v.sin() * u.cos(), v.cos(), v.sin() * u.sin());
                    let a = s(v0, u0);
                    let b = s(v1, u0);
                    let c = s(v1, u1);
                    let d = s(v0, u1);
                    push(a, b, c);
                    push(a, c, d);
                }
            }
        }
        2 | 3 => {
            let n = 28usize;
            let top = if mesh == 3 { 0.0 } else { 1.0 };
            for j in 0..n {
                let a0 = std::f32::consts::TAU * j as f32 / n as f32;
                let a1 = std::f32::consts::TAU * (j + 1) as f32 / n as f32;
                let b0 = V3::new(a0.cos(), -1.0, a0.sin());
                let b1 = V3::new(a1.cos(), -1.0, a1.sin());
                let t0 = V3::new(a0.cos() * top, 1.0, a0.sin() * top);
                let t1 = V3::new(a1.cos() * top, 1.0, a1.sin() * top);
                if mesh == 3 {
                    push(b0, b1, t0);
                } else {
                    push(b0, b1, t1);
                    push(b0, t1, t0);
                    push(V3::new(0.0, 1.0, 0.0), t0, t1);
                }
                push(V3::new(0.0, -1.0, 0.0), b1, b0);
            }
        }
        4 => {
            let (n, m) = (28usize, 12usize);
            let (r0, r1) = (0.72f32, 0.28f32);
            for i in 0..n {
                let a0 = std::f32::consts::TAU * i as f32 / n as f32;
                let a1 = std::f32::consts::TAU * (i + 1) as f32 / n as f32;
                for j in 0..m {
                    let b0 = std::f32::consts::TAU * j as f32 / m as f32;
                    let b1 = std::f32::consts::TAU * (j + 1) as f32 / m as f32;
                    let pt = |a: f32, b: f32| V3::new((r0 + r1 * b.cos()) * a.cos(), r1 * b.sin(), (r0 + r1 * b.cos()) * a.sin());
                    let a = pt(a0, b0);
                    let b = pt(a1, b0);
                    let c = pt(a1, b1);
                    let d = pt(a0, b1);
                    push(a, b, c);
                    push(a, c, d);
                }
            }
        }
        5 => {
            let a = V3::new(-1.0, -1.0, 0.0);
            let b = V3::new(1.0, -1.0, 0.0);
            let c = V3::new(1.0, 1.0, 0.0);
            let d = V3::new(-1.0, 1.0, 0.0);
            push(a, b, c);
            push(a, c, d);
        }
        _ => {
            let v = [
                V3::new(-1.0, -1.0, -1.0),
                V3::new(1.0, -1.0, -1.0),
                V3::new(1.0, 1.0, -1.0),
                V3::new(-1.0, 1.0, -1.0),
                V3::new(-1.0, -1.0, 1.0),
                V3::new(1.0, -1.0, 1.0),
                V3::new(1.0, 1.0, 1.0),
                V3::new(-1.0, 1.0, 1.0),
            ];
            let f = [[0, 1, 2, 3], [5, 4, 7, 6], [4, 0, 3, 7], [1, 5, 6, 2], [3, 2, 6, 7], [4, 5, 1, 0]];
            for q in f {
                push(v[q[0]], v[q[1]], v[q[2]]);
                push(v[q[0]], v[q[2]], v[q[3]]);
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
    let mut ymin = f32::MAX;
    let mut ymax = f32::MIN;
    for ct in contours {
        for &(_, y) in ct {
            ymin = ymin.min(y);
            ymax = ymax.max(y);
        }
    }
    let y0 = (ymin.floor().max(0.0) as usize).min(buf.h.saturating_sub(1));
    let y1 = (ymax.ceil().max(0.0) as usize).min(buf.h.saturating_sub(1));
    let (pa, pb, pc) = plane;
    let mut xs = Vec::new();
    for y in y0..=y1 {
        let py = y as f32 + 0.5;
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
            let xa = (pair[0].ceil().max(0.0) as usize).min(buf.w.saturating_sub(1));
            let xb = (pair[1].floor().max(0.0) as usize).min(buf.w.saturating_sub(1));
            if xa > xb {
                continue;
            }
            for x in xa..=xb {
                let px = x as f32 + 0.5;
                buf.put(px, py, pa * px + pb * py + pc, c);
            }
        }
    }
}

fn draw_text(buf: &mut Buf, p: &Params, cam: (f32, f32, f32)) {
    let cs = text_contours(p);
    if cs.is_empty() {
        return;
    }
    let (cx, cy, f) = cam;
    // The camera looks down +z, so the front face sits at negative z (nearer the viewer).
    let depth = p.depth.max(0.5);
    let bevel = p.bevel.min(depth * 0.45);
    let front = if bevel > 0.25 { cs.iter().map(|c| inset(c, 0.94)).collect::<Vec<_>>() } else { cs.clone() };
    let alpha = p.opacity * p.color[3].clamp(0.0, 1.0);
    let side_alpha = p.opacity * p.side[3].clamp(0.0, 1.0);
    let base = lin(p.color);
    let side = lin(p.side);
    let to_v = |x: f32, y: f32, z: f32| rotate(V3::new(x, y, z), p.rot).add(V3::new(p.pos.0, p.pos.1, p.z));
    let proj_ct = |ct: &[(f32, f32)], z: f32| -> Vec<(f32, f32)> {
        ct.iter().filter_map(|&(x, y)| project(to_v(x, y, z), cx, cy, f).map(|(sx, sy, _)| (sx, sy))).collect()
    };
    // Back and front faces (planar; even-odd scanline fill keeps glyph holes open).
    for (ct, z, n, col, a) in [(&cs, 0.0, V3::new(0.0, 0.0, 1.0), side, side_alpha), (&front, -depth, V3::new(0.0, 0.0, -1.0), base, alpha)] {
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
    for ct in &cs {
        let ins = inset(ct, 0.94);
        for i in 0..ct.len() {
            let j = (i + 1) % ct.len();
            let (x0, y0) = ct[i];
            let (x1, y1) = ct[j];
            let n = V3::new(y1 - y0, x0 - x1, 0.0).norm();
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
                let bn = V3::new(y1 - y0, x0 - x1, -(x1 - x0).hypot(y1 - y0) * 0.7).norm();
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

pub(super) fn apply(img: &mut Image, e: &EffectInstance, cx: &FxCtx) {
    if img.w == 0 || img.h == 0 {
        return;
    }
    let p = read_params(e, cx, img);
    if p.opacity <= 0.0 || p.scale <= 0.0 {
        return;
    }
    let mut buf = Buf::new(img.w, img.h);
    let cam = (img.w as f32 * 0.5, img.h as f32 * 0.5, (img.h as f32).max(1.0));
    if p.mesh == 6 {
        draw_text(&mut buf, &p, cam);
    } else {
        let k = p.size * p.scale * 0.5;
        let dz = if p.mesh == 5 { 0.0 } else if p.depth > 0.0 { p.depth * p.scale * 0.5 } else { k };
        let base = lin(p.color);
        let alpha = p.opacity * p.color[3].clamp(0.0, 1.0);
        for (v, n) in mesh_tris(p.mesh) {
            let v = v.map(|x| V3::new(x.x * k, x.y * k, x.z * dz));
            let n = V3::new(n.x / k.max(1e-4), n.y / k.max(1e-4), n.z / dz.max(1e-4)).norm();
            let v = v.map(|x| rotate(x, p.rot).add(V3::new(p.pos.0, p.pos.1, p.z)));
            draw_tri(&mut buf, v, n, &p, base, alpha, cam);
        }
    }
    for (dst, src) in img.px.chunks_mut(4).zip(buf.px) {
        if src[3] > 0.0 {
            super::over(dst, src);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use filmcraft_project::find_effect;

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
        assert!(img.px.chunks(4).any(|p| p[3] > 0.5));
    }

    #[test]
    fn extruded_text_draws_something() {
        let mut e = find_effect("object_3d").unwrap().instance();
        e.params.get_mut("mesh").unwrap().value = filmcraft_project::ParamValue::Choice(6);
        e.params.get_mut("text").unwrap().value = filmcraft_project::ParamValue::Text("Ag".into());
        e.params.get_mut("size").unwrap().value = filmcraft_project::ParamValue::Float(120.0);
        let mut img = Image::new(160, 96);
        apply(&mut img, &e, &ctx());
        assert!(img.px.chunks(4).any(|p| p[3] > 0.2));
    }
}
