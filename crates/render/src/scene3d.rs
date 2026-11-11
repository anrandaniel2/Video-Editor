//! Backend-neutral 3D draw packets. Geometry/animation evaluation stays independent of the
//! graphics API; rasterization, depth, material sampling and lighting belong to the backend.
use std::sync::{Arc, OnceLock};
use crate::{Image, model};

#[derive(Clone, Copy)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub tangent: [f32; 4],
    pub uv: [[f32; 2]; 2],
    pub color: [f32; 4],
    pub joints: [f32; 4],
    pub weights: [f32; 4],
    pub morph_index: f32,
}

pub struct Draw {
    pub vertices: Vec<Vertex>,
    pub material: model::Material,
    pub model: Option<Arc<model::Model>>,
    /// Legacy primitive material versus glTF metallic-roughness.
    pub pbr: bool,
    /// Packed GPU vertex deformation: object/node transforms, joint palettes and morph deltas.
    pub deformation: Vec<f32>,
}

pub struct Light {
    pub kind: u8,
    pub color: [f32; 3],
    pub direction: [f32; 3],
    pub position: [f32; 3],
    pub range: f32,
    pub cos_outer: f32,
    pub cos_inner: f32,
}

pub struct Scene {
    pub width: usize,
    pub height: usize,
    pub camera: [f32; 3],
    pub ambient: [f32; 3],
    pub opacity: f32,
    pub lights: Vec<Light>,
    pub draws: Vec<Draw>,
}

type Backend = fn(&Scene) -> Option<Image>;
static BACKEND: OnceLock<Backend> = OnceLock::new();

/// Register at application startup. Headless hosts keep the CPU reference renderer.
pub fn register_backend(backend: Backend) {
    let _ = BACKEND.set(backend);
}

pub(crate) fn available() -> bool { BACKEND.get().is_some() }
pub(crate) fn render(scene: &Scene) -> Option<Image> { BACKEND.get()?(scene) }
