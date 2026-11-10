//! glTF 2.0 / GLB reader. Written against the Khronos specification, using the workspace's JSON
//! and image libraries rather than pulling a second scene graph and decoder stack into the app.
//!
//! Core geometry, interleaved/normalized/sparse accessors, scene hierarchies, metallic-roughness
//! PBR, PNG/JPEG/WebP textures, linear-blend skins, morph targets and STEP/LINEAR/CUBICSPLINE
//! animation are supported. KHR_texture_transform, KHR_materials_unlit, emissive strength and
//! quantized attributes are read too. Compressed geometry/textures (Draco, meshopt, KTX2) are
//! rejected with an actionable message instead of silently importing an empty model. There is no
//! file-size or model-complexity ceiling: any GLB the phone can hold imports, with only structural
//! validation (claims must match the real data) standing between a corrupt file and the parser.

use std::collections::HashMap;
use std::path::{Component, Path};

use serde_json::Value;

use super::math::{self, Mat4};
use super::{AlphaMode, Animation, Channel, Instance, Interp, Material, Mesh, Model, MorphTarget, Node, Path as AnimPath, Primitive, Skin, Texture, TextureRef, Vertex, Wrap};

/// An accessor with no buffer view holds invented zeros (optionally sparsely patched), so its
/// size is not backed by real file data; bound that phantom allocation so a tiny file cannot
/// claim gigabytes of zeros. Data-backed accessors are bounded by their actual buffer ranges —
/// the importer deliberately has no file-size, texture-size or mesh-size ceiling.
const MAX_ACCESSOR_FLOATS: usize = 24 * 1024 * 1024;

type Result<T> = std::result::Result<T, String>;

fn err<T>(msg: impl std::fmt::Display) -> Result<T> {
    Err(msg.to_string())
}

fn arr<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key).and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
}

fn obj<'a>(v: &'a Value, key: &str) -> &'a Value {
    v.get(key).unwrap_or(&Value::Null)
}

fn num(v: &Value, key: &str, default: f64) -> f64 {
    v.get(key).and_then(Value::as_f64).filter(|x| x.is_finite()).unwrap_or(default)
}

fn uint(v: &Value, key: &str, default: usize) -> usize {
    v.get(key).and_then(Value::as_u64).map(|x| x.min(usize::MAX as u64) as usize).unwrap_or(default)
}

fn index(v: &Value, key: &str, len: usize) -> Result<usize> {
    let Some(i) = v.get(key).and_then(Value::as_u64) else { return err(format!("missing or invalid {key} index")) };
    if i >= len as u64 { return err(format!("{key} index {i} is out of range")); }
    Ok(i as usize)
}

fn opt_index(v: &Value, key: &str, len: usize) -> Result<Option<usize>> {
    if v.get(key).is_none() { Ok(None) } else { index(v, key, len).map(Some) }
}

fn name(v: &Value, fallback: String) -> String {
    v.get("name").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_owned).unwrap_or(fallback)
}

fn floats<const N: usize>(v: &Value, key: &str, default: [f32; N]) -> Result<[f32; N]> {
    let Some(a) = v.get(key) else { return Ok(default) };
    let Some(a) = a.as_array() else { return err(format!("{key} must be an array")) };
    if a.len() != N { return err(format!("{key} needs {N} components")); }
    let mut out = [0.0; N];
    for (o, x) in out.iter_mut().zip(a) {
        let Some(x) = x.as_f64().filter(|x| x.is_finite()) else { return err(format!("{key} contains a non-finite number")) };
        *o = x as f32;
    }
    Ok(out)
}

fn strings(v: &Value) -> Vec<String> {
    arr(v, "targetNames").iter().map(|x| x.as_str().unwrap_or_default().to_owned()).collect()
}

/// GLB's 12-byte header and JSON/BIN chunks. Chunk payloads start aligned to four bytes.
fn container(bytes: &[u8]) -> Result<(Value, Option<&[u8]>)> {
    if !bytes.starts_with(b"glTF") {
        let json = serde_json::from_slice(bytes).map_err(|e| format!("invalid glTF JSON: {e}"))?;
        return Ok((json, None));
    }
    let word = |b: &[u8], at: usize| -> Result<usize> {
        let Some(s) = b.get(at..at + 4) else { return err("truncated GLB header") };
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]) as usize)
    };
    if word(bytes, 4)? != 2 { return err("only glTF 2.0 is supported"); }
    let total = word(bytes, 8)?;
    if total != bytes.len() || total < 20 { return err("GLB length does not match its header"); }
    let mut at = 12;
    let mut json = None;
    let mut bin = None;
    while at < total {
        if at + 8 > total { return err("truncated GLB chunk header"); }
        let len = word(bytes, at)?;
        let kind = &bytes[at + 4..at + 8];
        let start = at + 8;
        let end = start.checked_add(len).ok_or("GLB chunk length overflow")?;
        if end > total || len % 4 != 0 { return err("invalid GLB chunk length"); }
        let data = &bytes[start..end];
        if kind == b"JSON" {
            if json.is_some() || at != 12 { return err("GLB must start with one JSON chunk"); }
            // GLB permits trailing space padding in the JSON chunk.
            json = Some(serde_json::from_slice(data).map_err(|e| format!("invalid GLB JSON: {e}"))?);
        } else if kind == b"BIN\0" {
            if bin.is_some() { return err("GLB has more than one BIN chunk"); }
            bin = Some(data);
        }
        at = end;
    }
    let Some(json) = json else { return err("GLB has no JSON chunk") };
    Ok((json, bin))
}

fn base64(s: &str) -> Result<Vec<u8>> {
    fn value(c: u8) -> Result<u32> {
        match c {
            b'A'..=b'Z' => Ok((c - b'A') as u32),
            b'a'..=b'z' => Ok((c - b'a' + 26) as u32),
            b'0'..=b'9' => Ok((c - b'0' + 52) as u32),
            b'+' => Ok(62),
            b'/' => Ok(63),
            _ => err("invalid base64 in glTF data URI"),
        }
    }
    let s: Vec<u8> = s.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if s.len() % 4 != 0 { return err("base64 data URI length is not a multiple of four"); }
    let mut out = Vec::new();
    out.try_reserve_exact(s.len() / 4 * 3).map_err(|_| "model data URI is too large")?;
    for chunk in s.chunks_exact(4) {
        let pad = chunk.iter().rev().take_while(|&&c| c == b'=').count();
        if pad > 2 || chunk[..4 - pad].contains(&b'=') { return err("invalid base64 padding"); }
        let mut bits = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            bits |= (if i >= 4 - pad { 0 } else { value(c)? }) << (18 - 6 * i);
        }
        out.push((bits >> 16) as u8);
        if pad < 2 { out.push((bits >> 8) as u8); }
        if pad < 1 { out.push(bits as u8); }
    }
    Ok(out)
}

fn percent_decode(s: &str) -> Result<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    let hex = |c: u8| match c { b'0'..=b'9' => Some(c - b'0'), b'a'..=b'f' => Some(c - b'a' + 10), b'A'..=b'F' => Some(c - b'A' + 10), _ => None };
    while i < b.len() {
        if b[i] == b'%' {
            let Some(h) = b.get(i + 1).copied().and_then(hex) else { return err("invalid percent encoding in glTF URI") };
            let Some(l) = b.get(i + 2).copied().and_then(hex) else { return err("invalid percent encoding in glTF URI") };
            out.push(h * 16 + l);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| "glTF URI is not UTF-8".into())
}

/// Resolve a relative resource without allowing an imported file to read arbitrary device paths.
fn resource(base: &Path, uri: &str) -> Result<Vec<u8>> {
    if let Some(data) = uri.strip_prefix("data:") {
        let Some((meta, payload)) = data.split_once(',') else { return err("malformed glTF data URI") };
        if !meta.split(';').any(|x| x.eq_ignore_ascii_case("base64")) { return err("glTF data URIs must be base64 encoded"); }
        let bytes = base64(payload)?;
        return Ok(bytes);
    }
    if uri.contains("://") || uri.starts_with('/') || uri.starts_with('\\') {
        return err("glTF resources must be embedded or relative files, not remote/absolute URLs");
    }
    let decoded = percent_decode(uri.split(['?', '#']).next().unwrap_or(uri))?;
    let rel = Path::new(&decoded);
    if rel.components().any(|c| !matches!(c, Component::Normal(_) | Component::CurDir)) {
        return err("glTF resource paths must stay inside the model's folder");
    }
    let path = base.join(rel);
    std::fs::read(&path).map_err(|e| format!("missing glTF resource {uri}: {e}"))
}

struct Accessor {
    components: usize,
    values: Vec<f32>,
}

impl Accessor {
    fn count(&self) -> usize {
        if self.components == 0 { 0 } else { self.values.len() / self.components }
    }
    fn get(&self, i: usize, c: usize) -> f32 {
        self.values.get(i * self.components + c).copied().unwrap_or(0.0)
    }
    fn vec3(&self, i: usize) -> [f32; 3] {
        [self.get(i, 0), self.get(i, 1), self.get(i, 2)]
    }
}

fn component_size(t: u64) -> Result<usize> {
    match t { 5120 | 5121 => Ok(1), 5122 | 5123 => Ok(2), 5125 | 5126 => Ok(4), _ => err(format!("unsupported accessor component type {t}")) }
}

fn dimensions(t: &str) -> Result<usize> {
    match t { "SCALAR" => Ok(1), "VEC2" => Ok(2), "VEC3" => Ok(3), "VEC4" => Ok(4), "MAT2" => Ok(4), "MAT3" => Ok(9), "MAT4" => Ok(16), _ => err(format!("unsupported accessor type {t}")) }
}

fn read_component(b: &[u8], at: usize, ty: u64, normalized: bool) -> Result<f32> {
    let Some(s) = b.get(at..) else { return err("accessor runs past its buffer") };
    let need = component_size(ty)?;
    if s.len() < need { return err("accessor runs past its buffer"); }
    let raw = match ty {
        5120 => s[0] as i8 as f64,
        5121 => s[0] as f64,
        5122 => i16::from_le_bytes([s[0], s[1]]) as f64,
        5123 => u16::from_le_bytes([s[0], s[1]]) as f64,
        5125 => u32::from_le_bytes([s[0], s[1], s[2], s[3]]) as f64,
        5126 => f32::from_le_bytes([s[0], s[1], s[2], s[3]]) as f64,
        _ => return err("unsupported accessor component type"),
    };
    let value = if normalized {
        match ty { 5120 => (raw / 127.0).max(-1.0), 5121 => raw / 255.0, 5122 => (raw / 32767.0).max(-1.0), 5123 => raw / 65535.0, _ => raw }
    } else { raw };
    if !value.is_finite() { return err("accessor contains a non-finite value"); }
    Ok(value as f32)
}

/// The view must fit its buffer; each consumer separately checks its own range inside the view
/// (adding a consumer offset here would wrongly reject tightly packed views that end exactly
/// with their buffer).
fn view_range(view: &Value, buffers: &[Vec<u8>]) -> Result<(usize, usize)> {
    let buffer = index(view, "buffer", buffers.len())?;
    let start = uint(view, "byteOffset", 0);
    let len = uint(view, "byteLength", 0);
    let end = start.checked_add(len).ok_or("buffer view range overflow")?;
    if len == 0 || end > buffers[buffer].len() { return err("buffer view runs past its buffer"); }
    Ok((buffer, start))
}

fn read_accessor(a: &Value, views: &[Value], buffers: &[Vec<u8>]) -> Result<Accessor> {
    let components = dimensions(a.get("type").and_then(Value::as_str).unwrap_or_default())?;
    let ty = a.get("componentType").and_then(Value::as_u64).ok_or("accessor has no componentType")?;
    let size = component_size(ty)?;
    let normalized = obj(a, "normalized").as_bool().unwrap_or(false);
    if normalized && (ty == 5125 || ty == 5126) { return err("float/uint32 accessors cannot be normalized"); }
    let count = uint(a, "count", 0);
    let n = count.checked_mul(components).ok_or("accessor size overflow")?;
    if count == 0 { return err("accessor is empty"); }
    let element = size * components;
    // Check any backing data before allocating, so a lying count fails cheaply. A data-backed
    // accessor's size is bounded by its real buffer; only a data-less one needs the phantom cap.
    let mut backing = None;
    if let Some(vi) = opt_index(a, "bufferView", views.len())? {
        let view = &views[vi];
        let offset = uint(a, "byteOffset", 0);
        let stride = uint(view, "byteStride", element);
        if stride < element || stride > 255 || offset % size != 0 { return err("invalid accessor stride/offset"); }
        let last = offset.checked_add((count - 1).checked_mul(stride).ok_or("accessor stride overflow")?).and_then(|x| x.checked_add(element)).ok_or("accessor range overflow")?;
        let (buffer, start) = view_range(view, buffers)?;
        let view_len = uint(view, "byteLength", 0);
        if last > view_len { return err("accessor runs past its buffer view"); }
        backing = Some((buffer, start + offset, stride));
    } else if n > MAX_ACCESSOR_FLOATS {
        return err("data-less accessor exceeds the model memory limit");
    }
    let mut values = Vec::new();
    values.try_reserve_exact(n).map_err(|_| "model accessor allocation failed")?;
    values.resize(n, 0.0f32);
    if let Some((buffer, start, stride)) = backing {
        let data = &buffers[buffer];
        for i in 0..count {
            for c in 0..components {
                values[i * components + c] = read_component(data, start + i * stride + c * size, ty, normalized)?;
            }
        }
    }
    if let Some(sparse) = a.get("sparse").filter(|v| !v.is_null()) {
        let nreplace = uint(sparse, "count", 0);
        if nreplace == 0 || nreplace > count { return err("invalid sparse accessor count"); }
        let si = obj(sparse, "indices");
        let sv = obj(sparse, "values");
        let it = si.get("componentType").and_then(Value::as_u64).ok_or("sparse indices need componentType")?;
        if !matches!(it, 5121 | 5123 | 5125) { return err("sparse indices must be unsigned integers"); }
        let isz = component_size(it)?;
        let iv = index(si, "bufferView", views.len())?;
        let vv = index(sv, "bufferView", views.len())?;
        if views[iv].get("byteStride").is_some() || views[vv].get("byteStride").is_some() { return err("sparse buffer views cannot be interleaved"); }
        let io = uint(si, "byteOffset", 0);
        let vo = uint(sv, "byteOffset", 0);
        let (ib, istart) = view_range(&views[iv], buffers)?;
        let (vb, vstart) = view_range(&views[vv], buffers)?;
        if io % isz != 0 || vo % size != 0 { return err("misaligned sparse accessor") };
        if io + nreplace * isz > uint(&views[iv], "byteLength", 0) || vo + nreplace * element > uint(&views[vv], "byteLength", 0) { return err("sparse data runs past its buffer view"); }
        for j in 0..nreplace {
            let idx = read_component(&buffers[ib], istart + io + j * isz, it, false)? as usize;
            if idx >= count { return err("sparse index is out of range"); }
            for c in 0..components {
                values[idx * components + c] = read_component(&buffers[vb], vstart + vo + j * element + c * size, ty, normalized)?;
            }
        }
    }
    Ok(Accessor { components, values })
}

fn attribute<'a, 'v>(attrs: &'v Value, semantic: &str, accessors: &'a [Accessor]) -> Result<Option<&'a Accessor>> {
    let Some(v) = attrs.get(semantic) else { return Ok(None) };
    let i = v.as_u64().ok_or("invalid attribute index")? as usize;
    accessors.get(i).map(Some).ok_or_else(|| format!("attribute {semantic} index is out of range"))
}

fn topology(mode: u64, indices: &[u32]) -> Result<Vec<u32>> {
    let mut out = Vec::new();
    match mode {
        4 => out.extend_from_slice(indices),
        5 => {
            for i in 0..indices.len().saturating_sub(2) {
                let t = if i % 2 == 0 { [indices[i], indices[i + 1], indices[i + 2]] } else { [indices[i + 1], indices[i], indices[i + 2]] };
                if t[0] != t[1] && t[1] != t[2] && t[0] != t[2] { out.extend_from_slice(&t); }
            }
        }
        6 => {
            for i in 1..indices.len().saturating_sub(1) {
                let t = [indices[0], indices[i], indices[i + 1]];
                if t[0] != t[1] && t[1] != t[2] && t[0] != t[2] { out.extend_from_slice(&t); }
            }
        }
        _ => return err(format!("primitive mode {mode} is not a triangle surface")),
    }
    if out.len() % 3 != 0 { return err("triangle index count is not a multiple of three"); }
    Ok(out)
}

/// Missing NORMAL means flat shading, not smooth shading with invented vertex normals.
fn flat_normals(verts: &mut Vec<Vertex>, indices: &mut Vec<u32>) {
    let mut nv = Vec::with_capacity(indices.len());
    for t in indices.chunks_exact(3) {
        let (a, b, c) = (verts[t[0] as usize].pos, verts[t[1] as usize].pos, verts[t[2] as usize].pos);
        let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let n = math::norm([ab[1] * ac[2] - ab[2] * ac[1], ab[2] * ac[0] - ab[0] * ac[2], ab[0] * ac[1] - ab[1] * ac[0]]);
        for &i in t {
            let mut v = verts[i as usize];
            v.normal = n;
            nv.push(v);
        }
    }
    *indices = (0..nv.len() as u32).collect();
    *verts = nv;
}

fn read_primitive(p: &Value, accessors: &[Accessor], materials: usize) -> Result<Option<Primitive>> {
    let mode = uint(p, "mode", 4) as u64;
    if mode < 4 { return Ok(None); } // point/line primitives are not solid surfaces
    for ext in ["KHR_draco_mesh_compression", "EXT_meshopt_compression"] {
        if obj(p, "extensions").get(ext).is_some() { return err(format!("{ext} is not supported; export an uncompressed GLB")); }
    }
    let attrs = obj(p, "attributes");
    let Some(pos) = attribute(attrs, "POSITION", accessors)? else { return Ok(None) };
    if pos.components != 3 { return err("POSITION must be VEC3"); }
    let count = pos.count();
    let normal = attribute(attrs, "NORMAL", accessors)?;
    let tangent = attribute(attrs, "TANGENT", accessors)?;
    let uv0 = attribute(attrs, "TEXCOORD_0", accessors)?;
    let uv1 = attribute(attrs, "TEXCOORD_1", accessors)?;
    let color = attribute(attrs, "COLOR_0", accessors)?;
    for (label, a, components) in [("NORMAL", normal, 3), ("TANGENT", tangent, 4), ("TEXCOORD_0", uv0, 2), ("TEXCOORD_1", uv1, 2)] {
        if let Some(a) = a {
            if a.count() != count || a.components != components { return err(format!("{label} does not match POSITION")); }
        }
    }
    if let Some(a) = color {
        if a.count() != count || !matches!(a.components, 3 | 4) { return err("COLOR_0 must be VEC3/VEC4 per vertex"); }
    }
    let mut influences: Vec<Vec<(u16, f32)>> = vec![Vec::new(); count];
    for set in 0..4 {
        let joints = attribute(attrs, &format!("JOINTS_{set}"), accessors)?;
        let weights = attribute(attrs, &format!("WEIGHTS_{set}"), accessors)?;
        match (joints, weights) {
            (Some(j), Some(w)) => {
                if j.components != 4 || w.components != 4 || j.count() != count || w.count() != count { return err("joint/weight attributes do not match POSITION"); }
                for i in 0..count {
                    for k in 0..4 {
                        let weight = w.get(i, k);
                        let jv = j.get(i, k);
                        if jv < 0.0 || jv > 65535.0 || jv.fract() != 0.0 { return err("joint index outside u16 range"); }
                        if weight > 0.0 { influences[i].push((jv as u16, weight)); }
                    }
                }
            }
            (None, None) => {}
            _ => return err("JOINTS and WEIGHTS must appear together"),
        }
    }
    let mut verts = Vec::new();
    verts.try_reserve_exact(count).map_err(|_| "model vertex allocation failed")?;
    for i in 0..count {
        let mut v = Vertex { pos: pos.vec3(i), ..Vertex::default() };
        if let Some(a) = normal { v.normal = a.vec3(i); }
        if let Some(a) = tangent { v.tangent = [a.get(i, 0), a.get(i, 1), a.get(i, 2), a.get(i, 3)]; }
        if let Some(a) = uv0 { v.uv[0] = [a.get(i, 0), a.get(i, 1)]; }
        if let Some(a) = uv1 { v.uv[1] = [a.get(i, 0), a.get(i, 1)]; }
        if let Some(a) = color {
            v.color = [a.get(i, 0), a.get(i, 1), a.get(i, 2), if a.components == 4 { a.get(i, 3) } else { 1.0 }];
        }
        influences[i].sort_by(|a, b| b.1.total_cmp(&a.1));
        let total: f32 = influences[i].iter().take(4).map(|x| x.1).sum();
        for (k, &(j, w)) in influences[i].iter().take(4).enumerate() {
            v.joints[k] = j;
            v.weights[k] = if total > 0.0 { w / total } else { 0.0 };
        }
        verts.push(v);
    }
    let raw: Vec<u32> = if let Some(a) = opt_index(p, "indices", accessors.len())?.map(|i| &accessors[i]) {
        if a.components != 1 { return err("primitive indices must be SCALAR"); }
        a.values.iter().map(|&x| {
            if x < 0.0 || x >= count as f32 || x.fract() != 0.0 { u32::MAX } else { x as u32 }
        }).collect()
    } else {
        (0..count as u32).collect()
    };
    if raw.contains(&u32::MAX) { return err("primitive index is out of range or uses primitive restart"); }
    let mut indices = topology(mode, &raw)?;
    let triangles = indices.len() / 3;
    let has_normals = normal.is_some();
    if !has_normals {
        flat_normals(&mut verts, &mut indices);
    }
    let mut morphs = Vec::new();
    for target in arr(p, "targets") {
        let mut m = MorphTarget::default();
        for (semantic, out) in [("POSITION", &mut m.dpos), ("NORMAL", &mut m.dnormal), ("TANGENT", &mut m.dtangent)] {
            if let Some(a) = attribute(target, semantic, accessors)? {
                if a.components != 3 || a.count() != count { return err(format!("morph {semantic} does not match POSITION")); }
                *out = (0..count).map(|i| a.vec3(i)).collect();
            }
        }
        morphs.push(m);
    }
    let material = opt_index(p, "material", materials)?.unwrap_or(0);
    Ok(Some(Primitive { verts, indices, material, morphs, has_normals }))
}

fn texture_ref(v: &Value, textures: usize) -> Result<Option<TextureRef>> {
    if v.is_null() { return Ok(None); }
    let index = index(v, "index", textures)?;
    let ext = obj(obj(v, "extensions"), "KHR_texture_transform");
    Ok(Some(TextureRef {
        index,
        uv_set: uint(ext, "texCoord", uint(v, "texCoord", 0)).min(1),
        offset: floats(ext, "offset", [0.0; 2])?,
        scale: floats(ext, "scale", [1.0; 2])?,
        rotation: num(ext, "rotation", 0.0) as f32,
    }))
}

fn read_material(m: &Value, textures: usize) -> Result<Material> {
    let pbr = obj(m, "pbrMetallicRoughness");
    let normal = obj(m, "normalTexture");
    let occlusion = obj(m, "occlusionTexture");
    let ext = obj(m, "extensions");
    let alpha_mode = match m.get("alphaMode").and_then(Value::as_str).unwrap_or("OPAQUE") {
        "OPAQUE" => AlphaMode::Opaque, "MASK" => AlphaMode::Mask, "BLEND" => AlphaMode::Blend, other => return err(format!("unknown alpha mode {other}")),
    };
    let strength = num(obj(ext, "KHR_materials_emissive_strength"), "emissiveStrength", 1.0).max(0.0) as f32;
    Ok(Material {
        name: name(m, String::new()),
        base_color: floats(pbr, "baseColorFactor", [1.0; 4])?,
        base_texture: texture_ref(obj(pbr, "baseColorTexture"), textures)?,
        metallic: num(pbr, "metallicFactor", 1.0).clamp(0.0, 1.0) as f32,
        roughness: num(pbr, "roughnessFactor", 1.0).clamp(0.0, 1.0) as f32,
        mr_texture: texture_ref(obj(pbr, "metallicRoughnessTexture"), textures)?,
        normal_texture: texture_ref(normal, textures)?,
        normal_scale: num(normal, "scale", 1.0) as f32,
        occlusion_texture: texture_ref(occlusion, textures)?,
        occlusion_strength: num(occlusion, "strength", 1.0).clamp(0.0, 1.0) as f32,
        emissive: floats(m, "emissiveFactor", [0.0; 3])?.map(|x| x * strength),
        emissive_texture: texture_ref(obj(m, "emissiveTexture"), textures)?,
        alpha_mode,
        alpha_cutoff: num(m, "alphaCutoff", 0.5).max(0.0) as f32,
        double_sided: obj(m, "doubleSided").as_bool().unwrap_or(false),
        unlit: ext.get("KHR_materials_unlit").is_some(),
    })
}

fn texture_source(t: &Value, images: usize) -> Result<usize> {
    opt_index(t, "source", images)?
        .or_else(|| opt_index(obj(obj(t, "extensions"), "EXT_texture_webp"), "source", images).ok().flatten())
        .ok_or_else(|| "texture has no supported image source (KTX2/Basis textures must be exported as PNG/JPEG)".into())
}

/// Parse a GLB or glTF file. `base` is the directory external resources resolve against.
pub fn parse(bytes: &[u8], base: &Path, model_name: String) -> Result<Model> {
    let (doc, bin) = container(bytes)?;
    if obj(&doc, "asset").get("version").and_then(Value::as_str) != Some("2.0") { return err("only glTF 2.0 is supported"); }
    let required = arr(&doc, "extensionsRequired");
    for ext in required.iter().filter_map(Value::as_str) {
        if matches!(ext, "KHR_draco_mesh_compression" | "EXT_meshopt_compression" | "KHR_texture_basisu") {
            return err(format!("{ext} is not supported; export an uncompressed GLB with PNG/JPEG textures"));
        }
    }
    if obj(&doc, "extensions").get("EXT_meshopt_compression").is_some() { return err("meshopt compression is not supported; export an uncompressed GLB"); }
    let buffer_defs = arr(&doc, "buffers");
    let views = arr(&doc, "bufferViews");
    let mut buffers = Vec::new();
    for (i, b) in buffer_defs.iter().enumerate() {
        let len = uint(b, "byteLength", 0);
        if len == 0 { return err("buffer byteLength must be positive"); }
        let data = match b.get("uri").and_then(Value::as_str) {
            Some(uri) => resource(base, uri)?,
            None if i == 0 => bin.ok_or("GLB buffer has no BIN chunk")?.to_vec(),
            None => return err("only the first GLB buffer may omit its URI"),
        };
        if data.len() < len { return err("buffer is shorter than its byteLength"); }
        buffers.push(data);
    }
    for v in views {
        view_range(v, &buffers)?;
        if obj(v, "extensions").get("EXT_meshopt_compression").is_some() { return err("meshopt compression is not supported; export an uncompressed GLB"); }
    }
    let accessors: Vec<Accessor> = arr(&doc, "accessors").iter().map(|a| read_accessor(a, views, &buffers)).collect::<Result<_>>()?;

    let image_defs = arr(&doc, "images");
    let texture_defs = arr(&doc, "textures");
    let sampler_defs = arr(&doc, "samplers");
    let mut images: HashMap<usize, Texture> = HashMap::new();
    let mut textures = Vec::new();
    for (i, t) in texture_defs.iter().enumerate() {
        let source = texture_source(t, image_defs.len())?;
        if images.contains_key(&source) { continue; }
        let image = &image_defs[source];
        let (texture_name, data) = if let Some(uri) = image.get("uri").and_then(Value::as_str) {
            (name(image, format!("Texture {}", i + 1)), resource(base, uri)?)
        } else {
            let vi = index(image, "bufferView", views.len())?;
            let (b, start) = view_range(&views[vi], &buffers)?;
            let end = start + uint(&views[vi], "byteLength", 0);
            (name(image, format!("Texture {}", i + 1)), buffers[b][start..end].to_vec())
        };
        let ext = match image.get("mimeType").and_then(Value::as_str).unwrap_or("") {
            "image/png" => "png", "image/jpeg" => "jpg", "image/webp" => "webp", _ => "img",
        };
        let raster = filmcraft_media::still::decode_raster(&format!("texture.{ext}"), &data).map_err(|e| e.to_string())?;
        images.insert(source, Texture { name: texture_name, w: raster.width as usize, h: raster.height as usize, rgba: raster.rgba, srgb: false, wrap: (Wrap::Repeat, Wrap::Repeat) });
    }
    for t in texture_defs {
        let source = texture_source(t, image_defs.len())?;
        textures.push(source);
        if let Some(s) = opt_index(t, "sampler", sampler_defs.len())? {
            let wrap = |key: &str| match uint(&sampler_defs[s], key, 10497) { 33071 => Wrap::Clamp, 33648 => Wrap::Mirror, _ => Wrap::Repeat };
            if let Some(img) = images.get_mut(&source) { img.wrap = (wrap("wrapS"), wrap("wrapT")); }
        }
    }
    let mut ordered_images: Vec<(usize, Texture)> = images.into_iter().collect();
    ordered_images.sort_by_key(|x| x.0);
    let mut image_map = HashMap::new();
    let mut images = Vec::new();
    for (source, image) in ordered_images { image_map.insert(source, images.len()); images.push(image); }
    let texture_sources: Vec<usize> = textures.iter().map(|s| image_map[s]).collect();
    let mut materials = arr(&doc, "materials").iter().map(|m| read_material(m, texture_sources.len())).collect::<Result<Vec<_>>>()?;
    if materials.is_empty() { materials.push(Material::default()); }
    for m in &materials {
        for r in [m.base_texture, m.emissive_texture].into_iter().flatten() { images[texture_sources[r.index]].srgb = true; }
    }
    for m in &mut materials {
        for r in [&mut m.base_texture, &mut m.mr_texture, &mut m.normal_texture, &mut m.occlusion_texture, &mut m.emissive_texture].into_iter().flatten() {
            r.index = texture_sources[r.index];
        }
    }

    let mesh_defs = arr(&doc, "meshes");
    let mut meshes = Vec::new();
    for (i, m) in mesh_defs.iter().enumerate() {
        let mut prims = Vec::new();
        for p in arr(m, "primitives") {
            if let Some(p) = read_primitive(p, &accessors, materials.len())? { prims.push(p); }
        }
        let targets = prims.iter().map(|p| p.morphs.len()).max().unwrap_or(0);
        let mut weights = arr(m, "weights").iter().map(|x| x.as_f64().filter(|x| x.is_finite()).unwrap_or(0.0) as f32).collect::<Vec<_>>();
        weights.resize(targets, 0.0);
        let target_names = strings(obj(m, "extras"));
        for p in &mut prims {
            for (k, morph) in p.morphs.iter_mut().enumerate() { morph.name = target_names.get(k).cloned().unwrap_or_default(); }
        }
        meshes.push(Mesh { name: name(m, format!("Mesh {}", i + 1)), prims, weights, target_names });
    }

    let node_defs = arr(&doc, "nodes");
    let mut nodes = Vec::new();
    for (i, n) in node_defs.iter().enumerate() {
        let mut node = Node {
            name: name(n, format!("Node {}", i + 1)),
            mesh: opt_index(n, "mesh", meshes.len())?,
            skin: opt_index(n, "skin", arr(&doc, "skins").len())?,
            translation: floats(n, "translation", [0.0; 3])?,
            rotation: math::norm_quat(floats(n, "rotation", [0.0, 0.0, 0.0, 1.0])?),
            scale: floats(n, "scale", [1.0; 3])?,
            ..Node::default()
        };
        if let Some(matrix) = n.get("matrix") {
            let a = matrix.as_array().ok_or("node matrix must be an array")?;
            if a.len() != 16 { return err("node matrix needs 16 values"); }
            let mut m: Mat4 = [0.0; 16];
            for (o, x) in m.iter_mut().zip(a) {
                let Some(x) = x.as_f64().filter(|x| x.is_finite()) else { return err("node matrix contains a non-finite value") };
                *o = x as f32;
            }
            if m[3] != 0.0 || m[7] != 0.0 || m[11] != 0.0 || m[15] != 1.0 { return err("node matrix must be affine"); }
            (node.translation, node.rotation, node.scale) = math::decompose(&m);
        }
        if let Some(w) = n.get("weights") {
            let a = w.as_array().ok_or("node weights must be an array")?;
            node.weights = Some(a.iter().map(|x| x.as_f64().filter(|x| x.is_finite()).unwrap_or(0.0) as f32).collect());
        }
        for c in arr(n, "children") {
            let c = c.as_u64().ok_or("invalid child node index")? as usize;
            if c >= node_defs.len() { return err("child node index is out of range"); }
            node.children.push(c);
        }
        nodes.push(node);
    }
    for i in 0..nodes.len() {
        for k in 0..nodes[i].children.len() {
            let c = nodes[i].children[k];
            if nodes[c].parent.is_some() { return err("a node cannot have two parents"); }
            nodes[c].parent = Some(i);
        }
    }
    // Reject cycles before recursive transform evaluation; a malformed file must not overflow.
    let mut state = vec![0u8; nodes.len()];
    for i in 0..nodes.len() {
        let mut path = Vec::new();
        let mut cur = Some(i);
        while let Some(n) = cur {
            if state[n] == 2 { break; }
            if state[n] == 1 { return err("node hierarchy contains a cycle"); }
            state[n] = 1;
            path.push(n);
            cur = nodes[n].parent;
        }
        for n in path { state[n] = 2; }
    }
    let scenes = arr(&doc, "scenes");
    let roots: Vec<usize> = if !scenes.is_empty() {
        let scene = opt_index(&doc, "scene", scenes.len())?.unwrap_or(0);
        arr(&scenes[scene], "nodes").iter().map(|n| {
            let i = n.as_u64().ok_or("invalid scene node index")? as usize;
            if i >= nodes.len() { err("scene node index is out of range") } else { Ok(i) }
        }).collect::<Result<_>>()?
    } else {
        (0..nodes.len()).filter(|&i| nodes[i].parent.is_none()).collect()
    };
    let mut skins = Vec::new();
    for (i, s) in arr(&doc, "skins").iter().enumerate() {
        let joints: Vec<usize> = arr(s, "joints").iter().map(|n| {
            let i = n.as_u64().ok_or("invalid joint node index")? as usize;
            if i >= nodes.len() { err("joint node index is out of range") } else { Ok(i) }
        }).collect::<Result<_>>()?;
        if joints.is_empty() { return err("skin has no joints"); }
        let mut inverse_bind = vec![math::IDENTITY; joints.len()];
        if let Some(a) = opt_index(s, "inverseBindMatrices", accessors.len())? {
            let a = &accessors[a];
            if a.components != 16 || a.count() < joints.len() { return err("inverseBindMatrices must be MAT4 per joint"); }
            for (j, m) in inverse_bind.iter_mut().enumerate() {
                for k in 0..16 { m[k] = a.get(j, k); }
                if m[3] != 0.0 || m[7] != 0.0 || m[11] != 0.0 || m[15] != 1.0 { return err("inverse bind matrix must be affine"); }
            }
        }
        skins.push(Skin { name: name(s, format!("Rig {}", i + 1)), joints, inverse_bind });
    }
    let mut instances = Vec::new();
    let mut stack: Vec<(usize, usize)> = roots.iter().rev().map(|&r| (r, 0)).collect();
    while let Some((i, depth)) = stack.pop() {
        if depth > 256 { return err("node hierarchy is deeper than 256 levels"); }
        let n = &nodes[i];
        if let Some(mesh) = n.mesh {
            for p in &meshes[mesh].prims {
                let skinned = p.verts.iter().any(|v| v.weights.iter().any(|&w| w > 0.0));
                let skin = if skinned { n.skin } else { None };
                if skinned && skin.is_none() { return err("skinned mesh node has no skin"); }
                if let Some(s) = skin {
                    let joints = skins[s].joints.len();
                    if p.verts.iter().any(|v| v.weights.iter().any(|&w| w > 0.0) && v.joints.iter().zip(v.weights.iter()).any(|(&j, &w)| w > 0.0 && j as usize >= joints)) {
                        return err("vertex joint index is outside its skin");
                    }
                }
                instances.push(Instance { node: i, mesh, skin });
            }
        }
        for &c in n.children.iter().rev() { stack.push((c, depth + 1)); }
    }
    if instances.is_empty() { return err("the selected scene contains no triangle meshes"); }
    let animations = arr(&doc, "animations").iter().enumerate()
        .map(|(i, a)| read_animation(a, i, &nodes, &meshes, &accessors)).collect::<Result<Vec<_>>>()?;

    let triangles: usize = instances.iter().map(|i| meshes[i.mesh].prims.iter().map(|p| p.indices.len() / 3).sum::<usize>()).sum();
    let vertices: usize = instances.iter().map(|i| meshes[i.mesh].prims.iter().map(|p| p.verts.len()).sum::<usize>()).sum();
    let morph_targets = meshes.iter().flat_map(|m| &m.prims).map(|p| p.morphs.len()).max().unwrap_or(0);
    let mut model = Model { name: model_name, nodes, roots, meshes, materials, textures: images, skins, animations, instances, bounds: ([f32::MAX; 3], [f32::MIN; 3]), triangles, vertices, morph_targets };
    let pose = model.pose(None, 0.0, false);
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for inst in &model.instances {
        for prim in &model.meshes[inst.mesh].prims {
            for i in 0..prim.verts.len() {
                let p = model.skin_vertex(&pose, inst, prim, i).pos;
                if !p.iter().all(|x| x.is_finite()) { return err("model transform produced a non-finite position"); }
                for k in 0..3 { lo[k] = lo[k].min(p[k]); hi[k] = hi[k].max(p[k]); }
            }
        }
    }
    model.bounds = (lo, hi);
    Ok(model)
}

fn read_animation(a: &Value, i: usize, nodes: &[Node], meshes: &[Mesh], accessors: &[Accessor]) -> Result<Animation> {
    let samplers = arr(a, "samplers");
    let mut channels = Vec::new();
    let mut duration = 0.0f32;
    for c in arr(a, "channels") {
        let si = index(c, "sampler", samplers.len())?;
        let sampler = &samplers[si];
        let input = index(sampler, "input", accessors.len())?;
        let output = index(sampler, "output", accessors.len())?;
        let target = obj(c, "target");
        let node = index(target, "node", nodes.len())?;
        let path = match target.get("path").and_then(Value::as_str).unwrap_or_default() {
            "translation" => AnimPath::Translation, "rotation" => AnimPath::Rotation, "scale" => AnimPath::Scale, "weights" => AnimPath::Weights,
            other => return err(format!("unsupported animation path {other}")),
        };
        let interp = match sampler.get("interpolation").and_then(Value::as_str).unwrap_or("LINEAR") {
            "LINEAR" => Interp::Linear, "STEP" => Interp::Step, "CUBICSPLINE" => Interp::CubicSpline, other => return err(format!("unsupported animation interpolation {other}")),
        };
        let (input, output) = (&accessors[input], &accessors[output]);
        if input.components != 1 || input.values.is_empty() { return err("animation times must be a nonempty SCALAR accessor"); }
        if input.values.windows(2).any(|w| w[1] < w[0]) || input.values.iter().any(|&t| t < 0.0) { return err("animation times must be nonnegative and sorted"); }
        let components = match path {
            AnimPath::Translation | AnimPath::Scale => 3,
            AnimPath::Rotation => 4,
            AnimPath::Weights => nodes[node].mesh.map(|m| meshes[m].weights.len()).unwrap_or(0),
        };
        if components == 0 { continue; }
        let per_key = components * if interp == Interp::CubicSpline { 3 } else { 1 };
        if output.values.len() != input.values.len().checked_mul(per_key).ok_or("animation size overflow")? { return err("animation output count does not match its keys/path"); }
        if interp == Interp::CubicSpline && input.values.len() < 2 { return err("cubic animation needs at least two keys"); }
        duration = duration.max(input.values.last().copied().unwrap_or(0.0));
        channels.push(Channel { node, path, times: input.values.clone(), values: output.values.clone(), components, interp });
    }
    Ok(Animation { name: name(a, format!("Animation {}", i + 1)), channels, duration })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_and_percent_decoding() {
        assert_eq!(base64("Zg==").unwrap(), b"f");
        assert_eq!(base64("Zm8=").unwrap(), b"fo");
        assert_eq!(base64("Zm9v").unwrap(), b"foo");
        assert!(base64("Z===").is_err());
        assert_eq!(percent_decode("a%20b%2Fc").unwrap(), "a b/c");
        assert!(percent_decode("%gg").is_err());
    }

    #[test]
    fn strips_and_fans_keep_winding() {
        assert_eq!(topology(5, &[0, 1, 2, 3]).unwrap(), vec![0, 1, 2, 2, 1, 3]);
        assert_eq!(topology(6, &[0, 1, 2, 3]).unwrap(), vec![0, 1, 2, 0, 2, 3]);
        assert_eq!(topology(5, &[0, 1, 2, 2, 3, 4]).unwrap(), vec![0, 1, 2, 3, 2, 4]);
    }

    #[test]
    fn glb_header_is_validated() {
        assert!(container(b"glTF").is_err());
        let mut b = Vec::new();
        b.extend_from_slice(b"glTF"); b.extend_from_slice(&2u32.to_le_bytes()); b.extend_from_slice(&24u32.to_le_bytes());
        b.extend_from_slice(&4u32.to_le_bytes()); b.extend_from_slice(b"JSON"); b.extend_from_slice(b"{}  ");
        let (j, bin) = container(&b).unwrap();
        assert_eq!(j, serde_json::json!({}));
        assert!(bin.is_none());
        b[8] = 25;
        assert!(container(&b).is_err());
    }

    #[test]
    fn decompose_round_trips_node_matrices() {
        let t = [1.0, -2.0, 3.0];
        let r = [0.1f32, 0.2, 0.3, 0.9];
        let r = math::norm_quat(r);
        let s = [2.0, 0.5, 3.0];
        let m = math::trs(t, r, s);
        let (t2, r2, s2) = math::decompose(&m);
        for k in 0..3 { assert!((t[k] - t2[k]).abs() < 1e-5 && (s[k] - s2[k]).abs() < 1e-5); }
        for k in 0..4 { assert!((r[k].abs() - r2[k].abs()).abs() < 1e-5); }
    }
}
