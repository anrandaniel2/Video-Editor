//! Real Khronos sample assets exercise the GLB container, PBR textures, skins and morph animation.
use std::path::Path;
use filmcraft_render::model::{Model, math};

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

#[test]
fn textured_glb_keeps_pbr_materials_and_embedded_images() {
    let m = Model::load(&fixture("BoxTextured.glb")).unwrap();
    assert_eq!(m.textures.len(), 1);
    assert!(m.textures[0].w > 0 && m.textures[0].h > 0);
    assert!(m.materials.iter().any(|m| m.base_texture.is_some()));
    assert!(m.materials[m.meshes[0].prims[0].material].base_texture.unwrap().index < m.textures.len());
    assert!(m.triangles >= 12);
    assert!(m.bounds.1[0] > m.bounds.0[0]);
}

#[test]
fn rigged_glb_evaluates_joint_animation() {
    let m = Model::load(&fixture("RiggedSimple.glb")).unwrap();
    assert!(!m.skins.is_empty() && !m.skins[0].joints.is_empty());
    assert!(!m.animations.is_empty() && m.animations[0].duration > 0.0);
    assert!(m.instances.iter().any(|i| i.skin.is_some()));
    let a = m.pose(Some(0), 0.0, false);
    let b = m.pose(Some(0), m.animations[0].duration * 0.4, false);
    assert!(a.skins[0].iter().zip(&b.skins[0]).any(|(x, y)| x.iter().zip(y).any(|(x, y)| (x - y).abs() > 1e-4)), "joint matrices did not animate");
    let inst = m.instances.iter().find(|i| i.skin.is_some()).unwrap();
    let prim = &m.meshes[inst.mesh].prims[0];
    let moved = (0..prim.verts.len()).filter(|&i| {
        let va = m.skin_vertex(&a, inst, prim, i);
        let vb = m.skin_vertex(&b, inst, prim, i);
        va.normal.iter().all(|x| x.is_finite()) && va.pos.iter().zip(vb.pos).any(|(a, b)| (a - b).abs() > 1e-4)
    }).count();
    assert!(moved > prim.verts.len() / 2, "only {moved} skinned vertices moved");
}

#[test]
fn morph_animation_changes_vertices_and_loops() {
    let m = Model::load(&fixture("AnimatedMorphCube.glb")).unwrap();
    assert!(m.morph_targets > 0);
    let a = m.pose(Some(0), 0.0, false);
    let b = m.pose(Some(0), m.animations[0].duration * 0.5, false);
    assert!(a.weights.iter().zip(&b.weights).any(|(a, b)| a.iter().zip(b).any(|(a, b)| (a - b).abs() > 0.01)));
    let inst = &m.instances[0];
    let prim = &m.meshes[inst.mesh].prims[0];
    assert!((0..prim.verts.len()).any(|i| {
        m.skin_vertex(&a, inst, prim, i).pos != m.skin_vertex(&b, inst, prim, i).pos
    }), "morph target did not change any vertex");
    let wrapped = m.pose(Some(0), m.animations[0].duration, true);
    let start = m.pose(Some(0), 0.0, false);
    assert_eq!(wrapped.world, start.world);
}

#[test]
fn absent_and_invalid_models_return_errors() {
    assert!(Model::load(Path::new("/does/not/exist.glb")).is_err());
    let dir = std::env::temp_dir().join(format!("filmcraft-bad-glb-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("bad.glb");
    std::fs::write(&p, b"glTF\x02\0\0\0\x0c\0\0\0").unwrap();
    assert!(Model::load(&p).is_err());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn normal_matrix_handles_rotation_with_nonuniform_scale() {
    let q = math::norm_quat([0.2, 0.3, 0.1, 0.9]);
    let m = math::trs([1.0, 2.0, 3.0], q, [3.0, 0.5, 2.0]);
    let nm = math::normal_matrix(&m);
    // For a TRS matrix the normal matrix is the same rotation with reciprocal scale.
    let expected = math::trs([0.0; 3], q, [1.0 / 3.0, 2.0, 0.5]);
    for i in 0..16 {
        assert!((nm[i] - expected[i]).abs() < 1e-5, "normal matrix element {i}: {} vs {}", nm[i], expected[i]);
    }
    let n = math::norm([0.4, -0.2, 0.8]);
    let t = math::norm([n[1], -n[0], 0.0]);
    let transformed_n = math::norm(math::vector(&nm, n));
    let transformed_t = math::norm(math::vector(&m, t));
    let dot = transformed_n.iter().zip(transformed_t).map(|(a, b)| a * b).sum::<f32>();
    assert!(dot.abs() < 1e-4, "normal lost perpendicularity: {dot}");
}

/// A scratch directory per test (tests share the runner's process id).
fn tmp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("filmcraft-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// The importer has no file-size, texture-size or mesh-complexity ceiling (a mid-size GLB with 4K
// PBR textures used to hit pixel caps, high-poly meshes a vertex ceiling, heavy rigs a shared
// accessor budget). These regression tests build models the old caps refused.

#[test]
fn large_textures_import_without_a_pixel_ceiling() {
    // Three 4096×4097 textures: 16.8 MP each (over the old 16 MP per-image cap) and 50.3 MP in
    // total (over the old 48 MP texture budget).
    let dir = tmp_dir("gltf-textures");
    std::fs::copy(fixture("solid_4096x4097.png"), dir.join("solid.png")).unwrap();
    let json = r#"{
      "asset": {"version": "2.0"},
      "images": [{"uri": "solid.png"}, {"uri": "solid.png"}, {"uri": "solid.png"}],
      "textures": [{"source": 0}, {"source": 1}, {"source": 2}],
      "materials": [{"pbrMetallicRoughness": {"baseColorTexture": {"index": 0}, "metallicRoughnessTexture": {"index": 1}}, "normalTexture": {"index": 2}}],
      "meshes": [{"primitives": [{"attributes": {"POSITION": 0, "NORMAL": 1}}]}],
      "accessors": [{"type": "VEC3", "componentType": 5126, "count": 3}, {"type": "VEC3", "componentType": 5126, "count": 3}],
      "nodes": [{"mesh": 0}]
    }"#;
    std::fs::write(dir.join("model.gltf"), json).unwrap();
    let m = Model::load(&dir.join("model.gltf")).unwrap();
    assert_eq!(m.textures.len(), 3);
    assert!(m.textures.iter().all(|t| t.w == 4096 && t.h == 4097));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn high_poly_meshes_import_without_a_vertex_ceiling() {
    // 1,500,003 vertices — just over the old 1.5 M ceiling — as one non-indexed triangle soup.
    let dir = tmp_dir("gltf-dense");
    let count = 1_500_003usize;
    let stride = 24usize; // interleaved POSITION + NORMAL: two f32 vec3s per vertex
    let mut bin = vec![0u8; count * stride];
    for i in 0..count {
        let f = [(i % 4096) as f32 * 0.01, (i / 4096) as f32 * 0.01, 0.0, 0.0, 0.0, 1.0];
        for (k, v) in f.iter().enumerate() {
            bin[i * stride + k * 4..i * stride + k * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
    }
    std::fs::write(dir.join("mesh.bin"), &bin).unwrap();
    let json = format!(
        r#"{{"asset": {{"version": "2.0"}},
        "buffers": [{{"uri": "mesh.bin", "byteLength": {}}}],
        "bufferViews": [{{"buffer": 0, "byteOffset": 0, "byteLength": {}, "byteStride": 24}}],
        "accessors": [
          {{"bufferView": 0, "type": "VEC3", "componentType": 5126, "count": {count}}},
          {{"bufferView": 0, "byteOffset": 12, "type": "VEC3", "componentType": 5126, "count": {count}}}],
        "meshes": [{{"primitives": [{{"attributes": {{"POSITION": 0, "NORMAL": 1}}}}]}}],
        "nodes": [{{"mesh": 0}}]}}"#,
        count * stride, count * stride
    );
    std::fs::write(dir.join("model.gltf"), json).unwrap();
    let m = Model::load(&dir.join("model.gltf")).unwrap();
    assert_eq!(m.vertices, count);
    assert_eq!(m.triangles, count / 3);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn many_accessors_import_without_a_shared_float_budget() {
    // 26 × 1 M-float accessors = 26 M floats, over the old shared 24 M-float budget. The large
    // accessors are data-less (zeros), which the loader bounds per accessor instead.
    let dir = tmp_dir("gltf-accessors");
    let fat = r#"{"type": "SCALAR", "componentType": 5126, "count": 1000000},"#.repeat(26);
    let json = format!(
        r#"{{"asset": {{"version": "2.0"}},
        "accessors": [{fat}{{"type": "VEC3", "componentType": 5126, "count": 3}}, {{"type": "VEC3", "componentType": 5126, "count": 3}}],
        "meshes": [{{"primitives": [{{"attributes": {{"POSITION": 26, "NORMAL": 27}}}}]}}],
        "nodes": [{{"mesh": 0}}]}}"#
    );
    std::fs::write(dir.join("model.gltf"), json).unwrap();
    assert!(Model::load(&dir.join("model.gltf")).is_ok());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn many_nodes_import_without_a_node_ceiling() {
    // 70,000 nodes — over the old 65,536-node ceiling.
    let dir = tmp_dir("gltf-nodes");
    let mut nodes = String::from(r#"{"mesh": 0},"#);
    nodes.push_str(&vec!["{}"; 69_999].join(","));
    let json = format!(
        r#"{{"asset": {{"version": "2.0"}},
        "nodes": [{nodes}],
        "meshes": [{{"primitives": [{{"attributes": {{"POSITION": 0, "NORMAL": 1}}}}]}}],
        "accessors": [{{"type": "VEC3", "componentType": 5126, "count": 3}}, {{"type": "VEC3", "componentType": 5126, "count": 3}}]}}"#
    );
    std::fs::write(dir.join("model.gltf"), json).unwrap();
    let m = Model::load(&dir.join("model.gltf")).unwrap();
    assert_eq!(m.nodes.len(), 70_000);
    std::fs::remove_dir_all(&dir).ok();
}
