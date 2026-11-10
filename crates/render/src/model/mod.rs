//! Imported 3D models: glTF 2.0 / GLB with metallic-roughness materials, textures, skins,
//! morph targets and animation.
//!
//! The loader uses `serde_json` and the editor's image decoders, so models bring no new
//! dependency. [`Model`] keeps the file's node/mesh/material structure; [`Model::pose`] evaluates
//! animation at a time, including joint matrices and morph weights. The renderer transforms each
//! vertex once, then shades and rasterises its triangles through the 3D object's z-buffer.
//! Unchanged files are cached by path, size and modification time for timeline scrubbing.

pub mod gltf;
pub mod math;

use std::collections::HashMap;
use std::path::{Path as FilePath, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

use math::Mat4;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AlphaMode {
    #[default]
    Opaque,
    Mask,
    Blend,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Wrap {
    #[default]
    Repeat,
    Clamp,
    Mirror,
}

/// A texture reference: its UV set and the KHR_texture_transform mapping, if present.
#[derive(Clone, Copy, Debug)]
pub struct TextureRef {
    pub index: usize,
    pub uv_set: usize,
    pub offset: [f32; 2],
    pub scale: [f32; 2],
    pub rotation: f32,
}

impl TextureRef {
    pub fn uv(&self, uv: [[f32; 2]; 2]) -> (f32, f32) {
        let p = uv[self.uv_set.min(1)];
        let (s, c) = self.rotation.sin_cos();
        let (x, y) = (p[0] * self.scale[0], p[1] * self.scale[1]);
        // The extension's column-major 2x2 rotation/scale matrix.
        (c * x - s * y + self.offset[0], s * x + c * y + self.offset[1])
    }
}

/// Straight-alpha RGBA8, top row first. Colour maps are decoded from sRGB *before* filtering;
/// normal, metallic/roughness and occlusion maps are linear data and must not be decoded.
#[derive(Clone)]
pub struct Texture {
    pub name: String,
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
    pub srgb: bool,
    pub wrap: (Wrap, Wrap),
}

impl Texture {
    pub fn sample(&self, u: f32, v: f32) -> [f32; 4] {
        if self.w == 0 || self.h == 0 || !u.is_finite() || !v.is_finite() {
            return [1.0; 4];
        }
        let coordinate = |x: f32, n: usize, wrap: Wrap| {
            let t = match wrap {
                Wrap::Repeat => x.rem_euclid(1.0),
                Wrap::Clamp => x.clamp(0.0, 1.0),
                Wrap::Mirror => {
                    let t = x.rem_euclid(2.0);
                    if t > 1.0 { 2.0 - t } else { t }
                }
            };
            (t * n as f32 - 0.5).clamp(0.0, n as f32 - 1.0)
        };
        let x = coordinate(u, self.w, self.wrap.0);
        let y = coordinate(v, self.h, self.wrap.1);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.w - 1), (y0 + 1).min(self.h - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let table = filmcraft_color::srgb_u8_to_linear_table();
        let px = |xx: usize, yy: usize| {
            let i = (yy * self.w + xx) * 4;
            let b = &self.rgba[i..i + 4];
            if self.srgb {
                [table[b[0] as usize], table[b[1] as usize], table[b[2] as usize], b[3] as f32 / 255.0]
            } else {
                [b[0] as f32 / 255.0, b[1] as f32 / 255.0, b[2] as f32 / 255.0, b[3] as f32 / 255.0]
            }
        };
        let (a, b, c, d) = (px(x0, y0), px(x1, y0), px(x0, y1), px(x1, y1));
        std::array::from_fn(|k| {
            let top = a[k] + (b[k] - a[k]) * fx;
            let bot = c[k] + (d[k] - c[k]) * fx;
            top + (bot - top) * fy
        })
    }
}

/// A flattened glTF metallic-roughness material. Factors and vertex colours are linear.
#[derive(Clone)]
pub struct Material {
    pub name: String,
    pub base_color: [f32; 4],
    pub base_texture: Option<TextureRef>,
    pub metallic: f32,
    pub roughness: f32,
    /// Green = roughness, blue = metallic.
    pub mr_texture: Option<TextureRef>,
    pub normal_texture: Option<TextureRef>,
    pub normal_scale: f32,
    /// Red channel; affects indirect light only.
    pub occlusion_texture: Option<TextureRef>,
    pub occlusion_strength: f32,
    pub emissive: [f32; 3],
    pub emissive_texture: Option<TextureRef>,
    pub alpha_mode: AlphaMode,
    pub alpha_cutoff: f32,
    pub double_sided: bool,
    pub unlit: bool,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            name: String::new(),
            base_color: [1.0; 4],
            base_texture: None,
            metallic: 1.0,
            roughness: 1.0,
            mr_texture: None,
            normal_texture: None,
            normal_scale: 1.0,
            occlusion_texture: None,
            occlusion_strength: 1.0,
            emissive: [0.0; 3],
            emissive_texture: None,
            alpha_mode: AlphaMode::Opaque,
            alpha_cutoff: 0.5,
            double_sided: false,
            unlit: false,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    /// XYZ direction and bitangent sign. Zero XYZ means "derive from the triangle's UVs".
    pub tangent: [f32; 4],
    pub uv: [[f32; 2]; 2],
    pub color: [f32; 4],
    pub joints: [u16; 4],
    pub weights: [f32; 4],
}

impl Default for Vertex {
    fn default() -> Self {
        Self {
            pos: [0.0; 3], normal: [0.0, 0.0, 1.0], tangent: [0.0, 0.0, 0.0, 1.0],
            uv: [[0.0; 2]; 2], color: [1.0; 4], joints: [0; 4], weights: [0.0; 4],
        }
    }
}

impl Vertex {
    /// Interpolation used when a triangle is clipped at the camera's near plane.
    pub fn lerp(a: &Self, b: &Self, t: f32) -> Self {
        let mix3 = |x: [f32; 3], y: [f32; 3]| std::array::from_fn(|k| x[k] + (y[k] - x[k]) * t);
        Self {
            pos: mix3(a.pos, b.pos), normal: mix3(a.normal, b.normal),
            tangent: std::array::from_fn(|k| a.tangent[k] + (b.tangent[k] - a.tangent[k]) * t),
            uv: [std::array::from_fn(|k| a.uv[0][k] + (b.uv[0][k] - a.uv[0][k]) * t), std::array::from_fn(|k| a.uv[1][k] + (b.uv[1][k] - a.uv[1][k]) * t)],
            color: std::array::from_fn(|k| a.color[k] + (b.color[k] - a.color[k]) * t),
            joints: if t < 0.5 { a.joints } else { b.joints },
            weights: if t < 0.5 { a.weights } else { b.weights },
        }
    }
}

#[derive(Clone, Default)]
pub struct MorphTarget {
    pub name: String,
    pub dpos: Vec<[f32; 3]>,
    pub dnormal: Vec<[f32; 3]>,
    pub dtangent: Vec<[f32; 3]>,
}

#[derive(Clone)]
pub struct Primitive {
    pub verts: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub material: usize,
    pub morphs: Vec<MorphTarget>,
    pub has_normals: bool,
}

#[derive(Clone, Default)]
pub struct Mesh {
    pub name: String,
    pub prims: Vec<Primitive>,
    pub weights: Vec<f32>,
    pub target_names: Vec<String>,
}

#[derive(Clone)]
pub struct Node {
    pub name: String,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub mesh: Option<usize>,
    pub skin: Option<usize>,
    pub translation: [f32; 3],
    /// x, y, z, w.
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
    pub weights: Option<Vec<f32>>,
}

impl Default for Node {
    fn default() -> Self {
        Self {
            name: String::new(), parent: None, children: Vec::new(), mesh: None, skin: None,
            translation: [0.0; 3], rotation: [0.0, 0.0, 0.0, 1.0], scale: [1.0; 3], weights: None,
        }
    }
}

#[derive(Clone)]
pub struct Skin {
    pub name: String,
    pub joints: Vec<usize>,
    pub inverse_bind: Vec<Mat4>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Path {
    Translation,
    Rotation,
    Scale,
    Weights,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interp {
    Linear,
    Step,
    CubicSpline,
}

#[derive(Clone)]
pub struct Channel {
    pub node: usize,
    pub path: Path,
    pub times: Vec<f32>,
    /// Components per key: 3 TRS, 4 rotation, one per morph target for weights. Cubic splines
    /// store incoming tangent, value, outgoing tangent, in that order, for every key.
    pub values: Vec<f32>,
    pub components: usize,
    pub interp: Interp,
}

impl Channel {
    pub fn duration(&self) -> f32 {
        self.times.last().copied().unwrap_or(0.0)
    }

    pub fn eval(&self, t: f32, out: &mut [f32]) {
        let n = self.times.len();
        let c = self.components;
        if n == 0 || c == 0 || out.len() < c || self.values.len() < n * c * if self.interp == Interp::CubicSpline { 3 } else { 1 } {
            return;
        }
        let cubic = self.interp == Interp::CubicSpline;
        let stride = c * if cubic { 3 } else { 1 };
        let value = |i: usize| &self.values[i * stride + if cubic { c } else { 0 }..i * stride + if cubic { 2 * c } else { c }];
        if t <= self.times[0] || n == 1 {
            out[..c].copy_from_slice(value(0));
            return;
        }
        if t >= self.times[n - 1] {
            out[..c].copy_from_slice(value(n - 1));
            return;
        }
        let i = self.times.partition_point(|&x| x <= t).saturating_sub(1).min(n - 2);
        let (t0, t1) = (self.times[i], self.times[i + 1]);
        let dt = t1 - t0;
        let u = if dt > 0.0 { ((t - t0) / dt).clamp(0.0, 1.0) } else { 0.0 };
        match self.interp {
            Interp::Step => out[..c].copy_from_slice(value(i)),
            Interp::Linear => {
                let (a, b) = (value(i), value(i + 1));
                if self.path == Path::Rotation && c == 4 {
                    out[..4].copy_from_slice(&math::quat_nlerp([a[0], a[1], a[2], a[3]], [b[0], b[1], b[2], b[3]], u));
                } else {
                    for k in 0..c { out[k] = a[k] + (b[k] - a[k]) * u; }
                }
            }
            Interp::CubicSpline => {
                let (u2, u3) = (u * u, u * u * u);
                let (h00, h10, h01, h11) = (2.0 * u3 - 3.0 * u2 + 1.0, u3 - 2.0 * u2 + u, -2.0 * u3 + 3.0 * u2, u3 - u2);
                for k in 0..c {
                    let p0 = self.values[i * stride + c + k];
                    let m0 = self.values[i * stride + 2 * c + k] * dt;
                    let p1 = self.values[(i + 1) * stride + c + k];
                    let m1 = self.values[(i + 1) * stride + k] * dt;
                    out[k] = h00 * p0 + h10 * m0 + h01 * p1 + h11 * m1;
                }
                if self.path == Path::Rotation && c == 4 {
                    let l = out[..4].iter().map(|v| v * v).sum::<f32>().sqrt();
                    if l > 1e-8 { for k in 0..4 { out[k] /= l; } }
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct Animation {
    pub name: String,
    pub channels: Vec<Channel>,
    pub duration: f32,
}

#[derive(Clone, Copy)]
pub struct Instance {
    pub node: usize,
    pub mesh: usize,
    /// Only set when the primitive actually has joint weights.
    pub skin: Option<usize>,
}

pub struct Model {
    pub name: String,
    pub nodes: Vec<Node>,
    pub roots: Vec<usize>,
    pub meshes: Vec<Mesh>,
    pub materials: Vec<Material>,
    pub textures: Vec<Texture>,
    pub skins: Vec<Skin>,
    pub animations: Vec<Animation>,
    /// Mesh instances reachable from the selected scene, not unused library nodes.
    pub instances: Vec<Instance>,
    /// Rest-pose bounds in glTF coordinates, including skinned vertices.
    pub bounds: ([f32; 3], [f32; 3]),
    pub triangles: usize,
    pub vertices: usize,
    pub morph_targets: usize,
}

impl Model {
    pub fn load(path: &FilePath) -> Result<Arc<Model>, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Model".into());
        Ok(Arc::new(gltf::parse(&bytes, path.parent().unwrap_or(FilePath::new(".")), name)?))
    }

    pub fn animation(&self, index: Option<usize>) -> Option<&Animation> {
        index.and_then(|i| self.animations.get(i))
    }

    /// Evaluate a clip. Looping wraps into its duration; otherwise the end pose is held.
    /// Morph weights are per *node*, so two instances of one mesh can pose differently.
    pub fn pose(&self, anim: Option<usize>, time: f32, looping: bool) -> Pose {
        let mut tr: Vec<[f32; 3]> = self.nodes.iter().map(|n| n.translation).collect();
        let mut rot: Vec<[f32; 4]> = self.nodes.iter().map(|n| n.rotation).collect();
        let mut scale: Vec<[f32; 3]> = self.nodes.iter().map(|n| n.scale).collect();
        let mut weights: Vec<Vec<f32>> = self.nodes.iter().map(|n| {
            n.weights.clone().unwrap_or_else(|| n.mesh.and_then(|i| self.meshes.get(i)).map(|m| m.weights.clone()).unwrap_or_default())
        }).collect();
        if let Some(a) = self.animation(anim) {
            let time = if time.is_finite() { time } else { 0.0 };
            let t = if looping && a.duration > 0.0 { time.rem_euclid(a.duration) } else { time.clamp(0.0, a.duration) };
            let mut buf = Vec::new();
            for ch in &a.channels {
                if ch.node >= self.nodes.len() { continue; }
                buf.clear();
                buf.resize(ch.components, 0.0);
                ch.eval(t, &mut buf);
                match ch.path {
                    Path::Translation if buf.len() >= 3 => tr[ch.node] = [buf[0], buf[1], buf[2]],
                    Path::Rotation if buf.len() >= 4 => rot[ch.node] = math::norm_quat([buf[0], buf[1], buf[2], buf[3]]),
                    Path::Scale if buf.len() >= 3 => scale[ch.node] = [buf[0], buf[1], buf[2]],
                    Path::Weights => weights[ch.node].clone_from(&buf),
                    _ => {}
                }
            }
        }
        let mut world = vec![math::IDENTITY; self.nodes.len()];
        let mut done = vec![false; self.nodes.len()];
        for i in 0..self.nodes.len() { self.resolve(i, &tr, &rot, &scale, &mut world, &mut done, 0); }
        let skins = self.skins.iter().map(|s| {
            s.joints.iter().enumerate().map(|(j, &node)| {
                let ibm = s.inverse_bind.get(j).copied().unwrap_or(math::IDENTITY);
                math::mul(world.get(node).unwrap_or(&math::IDENTITY), &ibm)
            }).collect()
        }).collect();
        Pose { world, skins, weights }
    }

    fn resolve(&self, i: usize, tr: &[[f32; 3]], rot: &[[f32; 4]], scale: &[[f32; 3]], world: &mut [Mat4], done: &mut [bool], depth: usize) {
        if done[i] || depth > 256 { return; }
        let local = math::trs(tr[i], rot[i], scale[i]);
        world[i] = match self.nodes[i].parent {
            Some(p) if p != i && p < self.nodes.len() => {
                self.resolve(p, tr, rot, scale, world, done, depth + 1);
                math::mul(&world[p], &local)
            }
            _ => local,
        };
        done[i] = true;
    }

    /// Vertex `index` after morphing and skinning, in model space. Skinned geometry ignores its
    /// instance node's transform, as required by glTF 2.0; the joint matrices already include it.
    pub fn skin_vertex(&self, pose: &Pose, inst: &Instance, prim: &Primitive, index: usize) -> Vertex {
        let Some(base) = prim.verts.get(index) else { return Vertex::default() };
        let mut v = *base;
        if let Some(w) = pose.weights.get(inst.node) {
            for (k, target) in prim.morphs.iter().enumerate() {
                let weight = w.get(k).copied().unwrap_or(0.0);
                if weight == 0.0 || !weight.is_finite() { continue; }
                if let Some(d) = target.dpos.get(index) { for c in 0..3 { v.pos[c] += weight * d[c]; } }
                if let Some(d) = target.dnormal.get(index) { for c in 0..3 { v.normal[c] += weight * d[c]; } }
                if let Some(d) = target.dtangent.get(index) { for c in 0..3 { v.tangent[c] += weight * d[c]; } }
            }
        }
        if let Some(mats) = inst.skin.and_then(|s| pose.skins.get(s)) {
            let (mut v0, v1, v2, v3) = ([0.0f32; 3], [0.0f32; 3], [0.0f32; 3], [0.0f32; 3]);
            let mut total = 0.0f32;
            let mut normal_mat = None;
            for k in 0..4 {
                let weight = v.weights[k];
                if weight <= 0.0 || !weight.is_finite() { continue; }
                let Some(m) = mats.get(v.joints[k] as usize) else { continue };
                let p = math::point(m, v.pos);
                let n = math::vector(m, v.normal);
                let t = math::vector(m, [v.tangent[0], v.tangent[1], v.tangent[2]]);
                for c in 0..3 { v0[c] += weight * p[c]; v1[c] += weight * n[c]; v2[c] += weight * t[c]; }
                v3[0] += weight;
                total += weight;
                if normal_mat.is_none() { normal_mat = Some(math::normal_matrix(m)); }
            }
            if total > 1e-6 {
                v.pos = v0;
                // Linear-blend normals are approximate under non-uniform joint scale. Use the
                // dominant joint's inverse transpose rather than stretching normals with joints.
                v.normal = math::norm(math::vector(&normal_mat.unwrap_or(math::IDENTITY), v1));
                v.tangent[..3].copy_from_slice(&math::norm(v2));
                return v;
            }
        }
        let m = pose.world.get(inst.node).unwrap_or(&math::IDENTITY);
        v.pos = math::point(m, v.pos);
        v.normal = math::norm(math::vector(&math::normal_matrix(m), v.normal));
        v.tangent[..3].copy_from_slice(&math::norm(math::vector(m, [v.tangent[0], v.tangent[1], v.tangent[2]])));
        v
    }

    pub fn material(&self, index: usize) -> &Material {
        static DEFAULT: OnceLock<Material> = OnceLock::new();
        self.materials.get(index).unwrap_or_else(|| DEFAULT.get_or_init(Material::default))
    }
}

pub struct Pose {
    pub world: Vec<Mat4>,
    pub skins: Vec<Vec<Mat4>>,
    /// One weight list per node (not per mesh).
    pub weights: Vec<Vec<f32>>,
}

struct Cached {
    modified: Option<SystemTime>,
    len: u64,
    model: Arc<Model>,
}

fn cache() -> &'static Mutex<HashMap<PathBuf, Cached>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Cached>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn load_cached(path: &FilePath) -> Result<Arc<Model>, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let (modified, len) = (meta.modified().ok(), meta.len());
    if let Ok(map) = cache().lock() {
        if let Some(c) = map.get(path) {
            if c.modified == modified && c.len == len { return Ok(c.model.clone()); }
        }
    }
    let model = Model::load(path)?;
    if let Ok(mut map) = cache().lock() {
        // Bound decoded model memory on a phone. Arc keeps models currently rendering alive.
        if map.len() >= 4 { map.clear(); }
        map.insert(path.to_path_buf(), Cached { modified, len, model: model.clone() });
    }
    Ok(model)
}

pub fn invalidate(path: &FilePath) {
    if let Ok(mut map) = cache().lock() { map.remove(path); }
}

pub fn clear_cache() {
    if let Ok(mut map) = cache().lock() { map.clear(); }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel(path: Path, times: Vec<f32>, values: Vec<f32>, components: usize, interp: Interp) -> Channel {
        Channel { node: 0, path, times, values, components, interp }
    }

    #[test]
    fn linear_tracks_interpolate_and_hold_outside() {
        let c = channel(Path::Translation, vec![0.0, 1.0, 2.0], vec![0.0, 0.0, 0.0, 10.0, 0.0, 0.0, 10.0, 5.0, 0.0], 3, Interp::Linear);
        let mut out = [0.0; 3];
        c.eval(-1.0, &mut out); assert_eq!(out, [0.0; 3]);
        c.eval(0.5, &mut out); assert_eq!(out, [5.0, 0.0, 0.0]);
        c.eval(1.5, &mut out); assert_eq!(out, [10.0, 2.5, 0.0]);
        c.eval(9.0, &mut out); assert_eq!(out, [10.0, 5.0, 0.0]);
    }

    #[test]
    fn step_tracks_jump_at_the_next_key() {
        let c = channel(Path::Scale, vec![0.0, 1.0], vec![1.0, 1.0, 1.0, 2.0, 2.0, 2.0], 3, Interp::Step);
        let mut out = [0.0; 3];
        c.eval(0.99, &mut out); assert_eq!(out, [1.0; 3]);
        c.eval(1.0, &mut out); assert_eq!(out, [2.0; 3]);
    }

    #[test]
    fn cubic_splines_pass_through_their_keys() {
        let c = channel(Path::Translation, vec![0.0, 1.0], vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 4.0, 2.0, 0.0, 0.0, 0.0, 0.0], 3, Interp::CubicSpline);
        let mut out = [0.0; 3];
        c.eval(0.0, &mut out); assert_eq!(out, [0.0; 3]);
        c.eval(1.0, &mut out); assert!((out[0] - 4.0).abs() < 1e-5 && (out[1] - 2.0).abs() < 1e-5, "{out:?}");
        c.eval(0.5, &mut out); assert!(out[0] > 0.0 && out[0] < 4.0, "{out:?}");
    }

    #[test]
    fn rotation_tracks_take_the_short_arc() {
        let c = channel(Path::Rotation, vec![0.0, 1.0], vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, -1.0], 4, Interp::Linear);
        let mut out = [0.0; 4];
        c.eval(0.5, &mut out);
        let len = out.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!((len - 1.0).abs() < 1e-5, "{out:?}");
    }

    #[test]
    fn textures_wrap_mirror_and_clamp() {
        let t = Texture { name: String::new(), w: 2, h: 1, rgba: vec![255, 0, 0, 255, 0, 0, 255, 255], srgb: false, wrap: (Wrap::Repeat, Wrap::Repeat) };
        let a = t.sample(0.25, 0.5);
        assert!(a[0] > 0.9 && a[2] < 0.1, "{a:?}");
        let b = t.sample(1.25, 0.5); assert!((a[0] - b[0]).abs() < 1e-5);
        let clamped = Texture { wrap: (Wrap::Clamp, Wrap::Clamp), ..t.clone() };
        assert!(clamped.sample(1.25, 0.5)[2] > 0.9);
        let mirrored = Texture { wrap: (Wrap::Mirror, Wrap::Mirror), ..t };
        assert!(mirrored.sample(1.25, 0.5)[2] > 0.9);
    }

    #[test]
    fn colour_maps_decode_before_filtering_but_data_maps_do_not() {
        let mut t = Texture { name: String::new(), w: 2, h: 1, rgba: vec![0, 0, 0, 255, 255, 255, 255, 255], srgb: true, wrap: (Wrap::Clamp, Wrap::Clamp) };
        assert!((t.sample(0.5, 0.5)[0] - 0.5).abs() < 1e-5);
        t.srgb = false;
        assert!((t.sample(0.5, 0.5)[0] - 127.5 / 255.0).abs() < 1e-5);
    }

    #[test]
    fn texture_transform_uses_the_selected_uv_set() {
        let r = TextureRef { index: 0, uv_set: 1, offset: [0.25, 0.5], scale: [2.0, 3.0], rotation: 0.0 };
        assert_eq!(r.uv([[0.0; 2], [0.5, 0.25]]), (1.25, 1.25));
    }
}
