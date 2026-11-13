//! Hardware 4x-MSAA coverage for native drawing strokes. The resolved alpha tile is consumed
//! by the Android layer surface; this is a real multisampled GPU attachment, not supersampling.
use std::sync::{Mutex, OnceLock};
use wgpu::util::DeviceExt;

struct Targets { w: u32, h: u32, msaa: wgpu::Texture, resolved: wgpu::Texture, readback: wgpu::Buffer }
struct Painter { device: wgpu::Device, queue: wgpu::Queue, pipeline: wgpu::RenderPipeline, targets: Option<Targets> }
static PAINTER: OnceLock<Option<Mutex<Painter>>> = OnceLock::new();
fn painter() -> Option<&'static Mutex<Painter>> { PAINTER.get_or_init(|| Painter::new().map(Mutex::new)).as_ref() }
pub fn available() -> bool { !crate::export_renderer::device_failed() && painter().is_some() }

pub fn coverage(points: &[[f32; 3]], left: u32, top: u32, width: u32, height: u32) -> Option<Vec<u8>> {
    if !available() || points.is_empty() || width == 0 || height == 0 || u64::from(width) * u64::from(height) > 4_194_304 { return None; }
    painter()?.lock().ok()?.draw(points, left, top, width, height)
}
impl Painter {
    fn new() -> Option<Self> {
        let renderer = crate::ExportRenderer::new()?;
        let (device, queue) = renderer.graphics_device();
        // RGBA8 unorm with 4 samples is a baseline WebGPU render attachment capability.
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("Drawing MSAA coverage"), source: wgpu::ShaderSource::Wgsl(include_str!("drawing.wgsl").into()) });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("Drawing coverage"), bind_group_layouts: &[], immediate_size: 0 });
        const ATTRS: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x2];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Drawing hardware 4x MSAA"), layout: Some(&layout),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[Some(wgpu::VertexBufferLayout { array_stride: 8, step_mode: wgpu::VertexStepMode::Vertex, attributes: &ATTRS })] },
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() }, depth_stencil: None,
            multisample: wgpu::MultisampleState { count: 4, mask: !0, alpha_to_coverage_enabled: false },
            fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some("fs"), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format: wgpu::TextureFormat::Rgba8Unorm, blend: None, write_mask: wgpu::ColorWrites::ALL })] }),
            multiview_mask: None, cache: None,
        });
        Some(Self { device, queue, pipeline, targets: None })
    }
    fn draw(&mut self, points: &[[f32; 3]], left: u32, top: u32, w: u32, h: u32) -> Option<Vec<u8>> {
        let mut vertices = Vec::<u8>::new();
        let mut count = 0u32;
        let mut triangle = |a: [f32; 2], b: [f32; 2], c: [f32; 2]| {
            for p in [a, b, c] {
                let x = (p[0] - left as f32) / w as f32 * 2.0 - 1.0;
                let y = 1.0 - (p[1] - top as f32) / h as f32 * 2.0;
                vertices.extend_from_slice(&x.to_le_bytes()); vertices.extend_from_slice(&y.to_le_bytes());
            }
            count += 3;
        };
        for (i, b) in points.iter().enumerate() {
            let n = (b[2].sqrt() * 16.0).ceil().clamp(24.0, 256.0) as usize;
            for j in 0..n {
                let a0 = j as f32 * std::f32::consts::TAU / n as f32;
                let a1 = (j + 1) as f32 * std::f32::consts::TAU / n as f32;
                triangle([b[0], b[1]], [b[0] + b[2] * a0.cos(), b[1] + b[2] * a0.sin()], [b[0] + b[2] * a1.cos(), b[1] + b[2] * a1.sin()]);
            }
            if i > 0 {
                let a = points[i - 1]; let dx = b[0] - a[0]; let dy = b[1] - a[1];
                let length = dx.hypot(dy);
                if length > 0.0001 {
                    let nx = -dy / length; let ny = dx / length;
                    let p = [a[0] + nx * a[2], a[1] + ny * a[2]];
                    let q = [a[0] - nx * a[2], a[1] - ny * a[2]];
                    let r = [b[0] + nx * b[2], b[1] + ny * b[2]];
                    let s = [b[0] - nx * b[2], b[1] - ny * b[2]];
                    triangle(p, q, r); triangle(r, q, s);
                }
            }
        }
        let row = (w * 4).div_ceil(256) * 256;
        let targets = match self.targets.take() {
            Some(t) if t.w == w && t.h == h => t,
            _ => {
                let texture = |samples, usage| self.device.create_texture(&wgpu::TextureDescriptor { label: Some("Drawing coverage target"), size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: samples, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba8Unorm, usage, view_formats: &[] });
                Targets { w, h, msaa: texture(4, wgpu::TextureUsages::RENDER_ATTACHMENT), resolved: texture(1, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC), readback: self.device.create_buffer(&wgpu::BufferDescriptor { label: Some("Drawing coverage readback"), size: u64::from(row) * u64::from(h), usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false }) }
            }
        };
        let vertex = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("Drawing stroke triangles"), contents: &vertices, usage: wgpu::BufferUsages::VERTEX });
        let msaa = targets.msaa.create_view(&Default::default()); let resolved = targets.resolved.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor { label: Some("Drawing 4x MSAA resolve"), color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &msaa, depth_slice: None, resolve_target: Some(&resolved), ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Discard } })], depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None });
            pass.set_pipeline(&self.pipeline); pass.set_vertex_buffer(0, vertex.slice(..)); pass.draw(0..count, 0..1);
        }
        encoder.copy_texture_to_buffer(wgpu::TexelCopyTextureInfo { texture: &targets.resolved, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All }, wgpu::TexelCopyBufferInfo { buffer: &targets.readback, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) } }, wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 });
        self.queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        targets.readback.slice(..).map_async(wgpu::MapMode::Read, move |result| { let _ = tx.send(result); });
        crate::wait_mapped(&self.device, &rx)?;
        let mapped = targets.readback.slice(..).get_mapped_range().ok()?;
        let mut alpha = Vec::with_capacity(w as usize * h as usize);
        let mut line = Vec::new();
        for y in 0..h as usize {
            crate::copy_mapped(&mapped[y * row as usize..y * row as usize + w as usize * 4], &mut line);
            alpha.extend(line.chunks_exact(4).map(|pixel| pixel[3]));
        }
        drop(mapped); targets.readback.unmap(); self.targets = Some(targets);
        if crate::export_renderer::device_failed() { None } else { Some(alpha) }
    }
}
