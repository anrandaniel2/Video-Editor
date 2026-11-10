//! Rigging accuracy, checked against an independent implementation of the glTF 2.0 skinning
//! rules: jointMatrix = jointGlobalTransform × inverseBindMatrix, skinned = Σ w · jointMatrix · v,
//! JOINTS index the skin's joints array, and a skinned mesh's own node transform is ignored.
//! The semantics were cross-checked against the Khronos glTF-Tutorial "Skins" chapter and Bevy's
//! `bevy_pbr`/`bevy_mesh` skinning (joint_transform.affine() * inverse_bindpose).
//!
//! The reference here deliberately shares no code with the loader: it re-reads the GLB container,
//! accessors, node hierarchy and animation channels from the raw bytes and evaluates them itself.
use std::path::Path;

use filmcraft_render::model::Model;

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

// ---- raw GLB access -----------------------------------------------------------------------------

struct Raw {
    json: serde_json::Value,
    bin: Vec<u8>,
}

fn raw(path: &Path) -> Raw {
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(&bytes[..4], b"glTF", "{path:?} is not a GLB");
    let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
    let json_len = word(12);
    let json: serde_json::Value = serde_json::from_slice(&bytes[20..20 + json_len]).unwrap();
    let mut bin = Vec::new();
    let mut at = 20 + json_len;
    while at + 8 <= bytes.len() {
        let len = word(at);
        if &bytes[at + 4..at + 8] == b"BIN\0" {
            bin = bytes[at + 8..at + 8 + len].to_vec();
        }
        at += 8 + len;
    }
    Raw { json, bin }
}

fn arr(v: &serde_json::Value, key: &str) -> Vec<serde_json::Value> {
    v.get(key).and_then(|x| x.as_array()).cloned().unwrap_or_default()
}

/// Decode one accessor into floats (f32 for every component type, normalized where asked).
fn accessor(r: &Raw, idx: usize) -> Vec<f32> {
    let a = &r.json["accessors"][idx];
    let count = a["count"].as_u64().unwrap() as usize;
    let comps = match a["type"].as_str().unwrap() {
        "SCALAR" => 1, "VEC2" => 2, "VEC3" => 3, "VEC4" => 4, "MAT4" => 16,
        other => panic!("reference cannot read accessor type {other}"),
    };
    let ct = a["componentType"].as_u64().unwrap();
    let size = match ct { 5120 | 5121 => 1, 5122 | 5123 => 2, 5125 | 5126 => 4, _ => panic!("component type {ct}") };
    let normalized = a["normalized"].as_bool().unwrap_or(false);
    let mut out = Vec::with_capacity(count * comps);
    if let Some(vi) = a.get("bufferView").and_then(|x| x.as_u64()) {
        let v = &r.json["bufferViews"][vi as usize];
        let base = v["byteOffset"].as_u64().unwrap_or(0) as usize;
        let stride = v["byteStride"].as_u64().map(|s| s as usize).unwrap_or(size * comps);
        let off = a["byteOffset"].as_u64().unwrap_or(0) as usize;
        for i in 0..count {
            for c in 0..comps {
                let at = base + off + i * stride + c * size;
                let raw = match ct {
                    5120 => r.bin[at] as i8 as f64,
                    5121 => r.bin[at] as f64,
                    5122 => i16::from_le_bytes([r.bin[at], r.bin[at + 1]]) as f64,
                    5123 => u16::from_le_bytes([r.bin[at], r.bin[at + 1]]) as f64,
                    5125 => u32::from_le_bytes(r.bin[at..at + 4].try_into().unwrap()) as f64,
                    5126 => f32::from_le_bytes(r.bin[at..at + 4].try_into().unwrap()) as f64,
                    _ => unreachable!(),
                };
                let val = if normalized {
                    match ct {
                        5120 => (raw / 127.0).max(-1.0), 5121 => raw / 255.0,
                        5122 => (raw / 32767.0).max(-1.0), 5123 => raw / 65535.0,
                        _ => raw,
                    }
                } else {
                    raw
                };
                out.push(val as f32);
            }
        }
    } else {
        out.resize(count * comps, 0.0);
    }
    out
}

// ---- small column-major matrix library (glTF's convention) --------------------------------------

type M4 = [f32; 16];

fn mat_mul(a: &M4, b: &M4) -> M4 {
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

fn mat_point(m: &M4, p: [f32; 3]) -> [f32; 3] {
    [
        m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12],
        m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13],
        m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14],
    ]
}

fn quat_norm(q: [f32; 4]) -> [f32; 4] {
    let l = q.iter().map(|v| v * v).sum::<f32>().sqrt();
    if l > 1e-12 { q.map(|v| v / l) } else { [0.0, 0.0, 0.0, 1.0] }
}

fn quat_nlerp(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    let dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
    let sign = if dot < 0.0 { -1.0 } else { 1.0 };
    quat_norm([
        a[0] + (b[0] * sign - a[0]) * t,
        a[1] + (b[1] * sign - a[1]) * t,
        a[2] + (b[2] * sign - a[2]) * t,
        a[3] + (b[3] * sign - a[3]) * t,
    ])
}

fn trs(t: [f32; 3], q: [f32; 4], s: [f32; 3]) -> M4 {
    let [x, y, z, w] = quat_norm(q);
    let (x2, y2, z2) = (x + x, y + y, z + z);
    let (xx, xy, xz) = (x * x2, x * y2, x * z2);
    let (yy, yz, zz) = (y * y2, y * z2, z * z2);
    let (wx, wy, wz) = (w * x2, w * y2, w * z2);
    [
        (1.0 - (yy + zz)) * s[0], (xy + wz) * s[0], (xz - wy) * s[0], 0.0,
        (xy - wz) * s[1], (1.0 - (xx + zz)) * s[1], (yz + wx) * s[1], 0.0,
        (xz + wy) * s[2], (yz - wx) * s[2], (1.0 - (xx + yy)) * s[2], 0.0,
        t[0], t[1], t[2], 1.0,
    ]
}

// ---- the reference rig evaluation ---------------------------------------------------------------

#[derive(Clone, Copy)]
struct Trs {
    t: [f32; 3],
    r: [f32; 4],
    s: [f32; 3],
}

fn node_trs(n: &serde_json::Value) -> Trs {
    let f3s = |key: &str, d: [f32; 3]| {
        n.get(key).and_then(|v| v.as_array()).map(|a| {
            [a[0].as_f64().unwrap() as f32, a[1].as_f64().unwrap() as f32, a[2].as_f64().unwrap() as f32]
        }).unwrap_or(d)
    };
    let rot = n.get("rotation").and_then(|v| v.as_array()).map(|a| {
        [a[0].as_f64().unwrap() as f32, a[1].as_f64().unwrap() as f32, a[2].as_f64().unwrap() as f32, a[3].as_f64().unwrap() as f32]
    }).unwrap_or([0.0, 0.0, 0.0, 1.0]);
    Trs { t: f3s("translation", [0.0; 3]), r: quat_norm(rot), s: f3s("scale", [1.0; 3]) }
}

fn node_matrix(n: &serde_json::Value) -> Option<M4> {
    let m = n.get("matrix")?.as_array()?;
    let mut out = [0.0f32; 16];
    for (o, x) in out.iter_mut().zip(m) {
        *o = x.as_f64().unwrap() as f32;
    }
    Some(out)
}

/// Independent channel evaluation: STEP / LINEAR (shortest-path nlerp) / CUBICSPLINE (Hermite,
/// tangents scaled by the key interval), exactly as the glTF 2.0 specification describes.
fn anim_trs(r: &Raw, anim: usize, t: f32, out: &mut Vec<(Trs, Option<M4>)>) {
    for ch in arr(&r.json["animations"][anim], "channels") {
        let target = &ch["target"];
        let node = target["node"].as_u64().unwrap() as usize;
        let path = target["path"].as_str().unwrap();
        if path == "weights" {
            continue; // fixtures do not morph; the loader's morph path is covered elsewhere
        }
        let sampler = &r.json["animations"][anim]["samplers"][ch["sampler"].as_u64().unwrap() as usize];
        let interp = sampler.get("interpolation").and_then(|x| x.as_str()).unwrap_or("LINEAR");
        let times = accessor(r, sampler["input"].as_u64().unwrap() as usize);
        let vals = accessor(r, sampler["output"].as_u64().unwrap() as usize);
        let comps = if path == "rotation" { 4 } else { 3 };
        let stride = if interp == "CUBICSPLINE" { comps * 3 } else { comps };
        let n = times.len();
        let value = |i: usize| -> Vec<f32> {
            let base = i * stride + if interp == "CUBICSPLINE" { comps } else { 0 };
            vals[base..base + comps].to_vec()
        };
        let mut v = value(0);
        if n > 1 && t > times[0] {
            if t >= times[n - 1] {
                v = value(n - 1);
            } else {
                let i = times.partition_point(|&x| x <= t).saturating_sub(1).min(n - 2);
                let dt = times[i + 1] - times[i];
                let u = if dt > 0.0 { ((t - times[i]) / dt).clamp(0.0, 1.0) } else { 0.0 };
                match interp {
                    "STEP" => v = value(i),
                    "CUBICSPLINE" => {
                        let (u2, u3) = (u * u, u * u * u);
                        let (h00, h10, h01, h11) = (2.0 * u3 - 3.0 * u2 + 1.0, u3 - 2.0 * u2 + u, -2.0 * u3 + 3.0 * u2, u3 - u2);
                        v = Vec::with_capacity(comps);
                        for k in 0..comps {
                            let p0 = vals[i * stride + comps + k];
                            let m0 = vals[i * stride + 2 * comps + k] * dt;
                            let p1 = vals[(i + 1) * stride + comps + k];
                            let m1 = vals[(i + 1) * stride + k] * dt;
                            v.push(h00 * p0 + h10 * m0 + h01 * p1 + h11 * m1);
                        }
                        if comps == 4 {
                            v = quat_norm([v[0], v[1], v[2], v[3]]).to_vec();
                        }
                    }
                    _ => {
                        if comps == 4 {
                            let q = quat_nlerp([value(i)[0], value(i)[1], value(i)[2], value(i)[3]], [value(i + 1)[0], value(i + 1)[1], value(i + 1)[2], value(i + 1)[3]], u);
                            v = q.to_vec();
                        } else {
                            let (a, b) = (value(i), value(i + 1));
                            v = (0..comps).map(|k| a[k] + (b[k] - a[k]) * u).collect();
                        }
                    }
                }
            }
        }
        let slot = &mut out[node];
        match path {
            "translation" => slot.0.t = [v[0], v[1], v[2]],
            "rotation" => slot.0.r = quat_norm([v[0], v[1], v[2], v[3]]),
            "scale" => slot.0.s = [v[0], v[1], v[2]],
            _ => {}
        }
    }
}

/// Node world matrices with independent animation evaluation (None = rest pose).
fn reference_world(r: &Raw, anim: Option<(usize, f32)>) -> Vec<M4> {
    let nodes = arr(&r.json, "nodes");
    let mut local: Vec<(Trs, Option<M4>)> = nodes.iter().map(|n| (node_trs(n), node_matrix(n))).collect();
    if let Some((a, t)) = anim {
        anim_trs(r, a, t, &mut local);
    }
    let mut parent = vec![usize::MAX; nodes.len()];
    for (i, n) in nodes.iter().enumerate() {
        for c in arr(n, "children") {
            parent[c.as_u64().unwrap() as usize] = i;
        }
    }
    let mut world = vec![[0.0f32; 16]; nodes.len()];
    let mut done = vec![false; nodes.len()];
    fn resolve(i: usize, parent: &[usize], local: &[(Trs, Option<M4>)], world: &mut [M4], done: &mut [bool]) -> M4 {
        if done[i] {
            return world[i];
        }
        let l = match &local[i] {
            (_, Some(m)) => *m,
            (tr, None) => trs(tr.t, tr.r, tr.s),
        };
        world[i] = if parent[i] != usize::MAX {
            let p = resolve(parent[i], parent, local, world, done);
            mat_mul(&p, &l)
        } else {
            l
        };
        done[i] = true;
        world[i]
    }
    for i in 0..nodes.len() {
        resolve(i, &parent, &local, &mut world, &mut done);
    }
    world
}

/// The skinned positions of primitive `prim` of mesh `mesh`, per the spec formula.
fn reference_skinned(r: &Raw, anim: Option<(usize, f32)>, mesh: usize, prim: usize) -> Vec<[f32; 3]> {
    let world = reference_world(r, anim);
    let nodes = arr(&r.json, "nodes");
    let mesh_node = nodes.iter().position(|n| n["mesh"].as_u64().map(|m| m as usize) == Some(mesh)).unwrap();
    let skin = &r.json["skins"][0];
    let joints: Vec<usize> = arr(skin, "joints").iter().map(|j| j.as_u64().unwrap() as usize).collect();
    let ibm: Vec<M4> = if let Some(i) = skin.get("inverseBindMatrices").and_then(|x| x.as_u64()) {
        let f = accessor(r, i as usize);
        f.chunks_exact(16).map(|c| { let mut m = [0.0f32; 16]; m.copy_from_slice(c); m }).collect()
    } else {
        vec![[1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]; joints.len()]
    };
    let joint_mats: Vec<M4> = joints.iter().zip(&ibm).map(|(&n, b)| mat_mul(&world[n], b)).collect();
    let p = &r.json["meshes"][mesh]["primitives"][prim];
    let pos = accessor(r, p["attributes"]["POSITION"].as_u64().unwrap() as usize);
    let joints0 = accessor(r, p["attributes"]["JOINTS_0"].as_u64().unwrap() as usize);
    let weights0 = accessor(r, p["attributes"]["WEIGHTS_0"].as_u64().unwrap() as usize);
    let mut out = Vec::new();
    for i in 0..pos.len() / 3 {
        let v = [pos[i * 3], pos[i * 3 + 1], pos[i * 3 + 2]];
        // Same convention as the engine: positive weights, strongest four, renormalized.
        let mut infl: Vec<(usize, f32)> = (0..4)
            .filter_map(|k| {
                let w = weights0[i * 4 + k];
                (w > 0.0).then(|| (joints0[i * 4 + k] as usize, w))
            })
            .collect();
        infl.sort_by(|a, b| b.1.total_cmp(&a.1));
        infl.truncate(4);
        let total: f32 = infl.iter().map(|x| x.1).sum();
        if total <= 1e-6 {
            out.push(mat_point(&world[0], v));
            continue;
        }
        let mut acc = [0.0f32; 3];
        for (j, w) in &infl {
            let p = mat_point(&joint_mats[*j], v);
            for c in 0..3 {
                acc[c] += w / total * p[c];
            }
        }
        out.push(acc);
    }
    out
}

// ---- the checks ---------------------------------------------------------------------------------

fn close(a: [f32; 3], b: [f32; 3]) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() < 5e-3)
}

/// Every skinned vertex must equal the independent reference at rest and mid-animation.
fn check_rig(name: &str, sample: usize) {
    let r = raw(&fixture(name));
    let anims = arr(&r.json, "animations");
    assert!(!anims.is_empty(), "{name} has no animation");
    let duration = {
        let times = accessor(&r, anims[0]["samplers"][0]["input"].as_u64().unwrap() as usize);
        *times.last().unwrap()
    };
    let m = Model::load(&fixture(name)).unwrap();
    assert!(m.meshes[0].prims[0].morphs.is_empty(), "reference does not cover morph targets");
    let inst = m.instances.iter().find(|i| i.skin.is_some()).copied().unwrap();
    let prim = &m.meshes[inst.mesh].prims[inst.prim];
    for (label, anim) in [("rest", None), ("first keys", Some((0, 0.0))), ("mid", Some((0, duration * 0.5))), ("late", Some((0, duration * 0.93)))] {
        let want = reference_skinned(&r, anim, inst.mesh, inst.prim);
        let pose = m.pose(anim.map(|(a, _)| a), anim.map(|(_, t)| t).unwrap_or(0.0), false);
        for i in (0..prim.verts.len()).step_by(sample) {
            let v = m.skin_vertex(&pose, &inst, prim, i);
            assert!(close(v.pos, want[i]), "{name} {label}: vertex {i} at {:?}, reference says {:?}", v.pos, want[i]);
        }
    }
}

#[test]
fn rigged_simple_matches_the_reference_skinning() {
    check_rig("RiggedSimple.glb", 1);
}

#[test]
fn rigged_figure_matches_the_reference_skinning() {
    check_rig("RiggedFigure.glb", 1);
}

#[test]
fn cesium_man_matches_the_reference_skinning() {
    check_rig("CesiumMan.glb", 11);
}

/// A mesh whose primitives mix a weighted (skinned) prim and a rigid one must expose exactly one
/// instance per primitive, each with its own skin decision. (A previous version drew every prim
/// once per instance, leaving a frozen bind-pose copy of skinned prims next to the animated one.)
#[test]
fn mixed_skinned_and_rigid_primitives_get_one_instance_each() {
    // Bin layout: pos0|nrm|joints(u16)|weights|pos1|nrm|identity-MAT4.
    let mut bin: Vec<u8> = Vec::new();
    let push = |bin: &mut Vec<u8>, f: &[f32]| {
        while bin.len() % 4 != 0 { bin.push(0); }
        for v in f { bin.extend_from_slice(&v.to_le_bytes()); }
    };
    let pos0 = [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    let nrm = [0.0f32, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0];
    let weights0 = [1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0];
    let pos1 = [5.0f32, 0.0, 0.0, 6.0, 0.0, 0.0, 5.0, 1.0, 0.0];
    let ibm = [1.0f32, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
    push(&mut bin, &pos0); // [0,36)
    push(&mut bin, &nrm); // [36,72)
    for v in [0u16; 12].iter() { bin.extend_from_slice(&v.to_le_bytes()); } // [72,96)
    push(&mut bin, &weights0); // [96,144)
    push(&mut bin, &pos1); // [144,180)
    push(&mut bin, &nrm); // [180,216)
    push(&mut bin, &ibm); // [216,280)
    assert_eq!(bin.len(), 280);
    let json = format!(
        r#"{{"asset": {{"version": "2.0"}},
        "buffers": [{{"byteLength": {}}}],
        "bufferViews": [
          {{"buffer": 0, "byteOffset": 0, "byteLength": 36}},
          {{"buffer": 0, "byteOffset": 36, "byteLength": 36}},
          {{"buffer": 0, "byteOffset": 72, "byteLength": 24}},
          {{"buffer": 0, "byteOffset": 96, "byteLength": 48}},
          {{"buffer": 0, "byteOffset": 144, "byteLength": 36}},
          {{"buffer": 0, "byteOffset": 180, "byteLength": 36}},
          {{"buffer": 0, "byteOffset": 216, "byteLength": 64}}],
        "accessors": [
          {{"bufferView": 0, "type": "VEC3", "componentType": 5126, "count": 3}},
          {{"bufferView": 1, "type": "VEC3", "componentType": 5126, "count": 3}},
          {{"bufferView": 2, "type": "VEC4", "componentType": 5123, "count": 3}},
          {{"bufferView": 3, "type": "VEC4", "componentType": 5126, "count": 3}},
          {{"bufferView": 4, "type": "VEC3", "componentType": 5126, "count": 3}},
          {{"bufferView": 5, "type": "VEC3", "componentType": 5126, "count": 3}},
          {{"bufferView": 6, "type": "MAT4", "componentType": 5126, "count": 1}},
          {{"type": "VEC3", "componentType": 5126, "count": 3}}],
        "meshes": [{{"primitives": [
          {{"attributes": {{"POSITION": 0, "NORMAL": 1, "JOINTS_0": 2, "WEIGHTS_0": 3}}}},
          {{"attributes": {{"POSITION": 4, "NORMAL": 5, "COLOR_0": 7}}}}]}}],
        "skins": [{{"joints": [1], "inverseBindMatrices": 6}}],
        "nodes": [{{"mesh": 0, "skin": 0}}, {{"translation": [0.0, 2.0, 0.0]}}],
        "scenes": [{{"nodes": [0]}}]}}"#,
        bin.len()
    );
    let mut json_bytes = json.into_bytes();
    while json_bytes.len() % 4 != 0 { json_bytes.push(b' '); }
    while bin.len() % 4 != 0 { bin.push(0); }
    let mut glb = Vec::new();
    glb.extend_from_slice(b"glTF");
    glb.extend_from_slice(&2u32.to_le_bytes());
    let total = (12 + 8 + json_bytes.len() + 8 + bin.len()) as u32;
    glb.extend_from_slice(&total.to_le_bytes());
    glb.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
    glb.extend_from_slice(b"JSON");
    glb.extend_from_slice(&json_bytes);
    glb.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    glb.extend_from_slice(b"BIN\0");
    glb.extend_from_slice(&bin);
    let dir = std::env::temp_dir().join(format!("filmcraft-mixed-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("mixed.glb"), &glb).unwrap();

    let m = Model::load(&dir.join("mixed.glb")).unwrap();
    assert_eq!(m.instances.len(), 2, "one instance per primitive, no duplicates");
    let mut prims: Vec<usize> = m.instances.iter().map(|i| i.prim).collect();
    prims.sort_unstable();
    assert_eq!(prims, vec![0, 1]);
    let skinned = m.instances.iter().find(|i| i.prim == 0).copied().unwrap();
    let rigid = m.instances.iter().find(|i| i.prim == 1).copied().unwrap();
    assert!(skinned.skin.is_some(), "the weighted prim must skin");
    assert!(rigid.skin.is_none(), "the rigid prim must not pretend to skin");
    // The weighted prim rides the joint (translated up by 2); the rigid one keeps its node space.
    let pose = m.pose(None, 0.0, false);
    let p0 = &m.meshes[skinned.mesh].prims[skinned.prim];
    let p1 = &m.meshes[rigid.mesh].prims[rigid.prim];
    let skinned_pos = m.skin_vertex(&pose, &skinned, p0, 0).pos;
    let rigid_pos = m.skin_vertex(&pose, &rigid, p1, 0).pos;
    assert!(close(skinned_pos, [0.0, 2.0, 0.0]), "skinned vertex should ride the joint: {skinned_pos:?}");
    assert!(close(rigid_pos, [5.0, 0.0, 0.0]), "rigid vertex must keep the node transform: {rigid_pos:?}");
    std::fs::remove_dir_all(&dir).ok();
}
