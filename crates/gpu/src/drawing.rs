//! Hardware 4x-MSAA coverage for native drawing strokes. The resolved alpha tile is consumed
//! by the Android layer surface; this is a real multisampled GPU attachment, not supersampling.
use std::sync::{Mutex, OnceLock, atomic::{AtomicBool, Ordering}};
use wgpu::util::DeviceExt;

struct Targets { w: u32, h: u32, msaa: wgpu::Texture, resolved: wgpu::Texture, readback: wgpu::Buffer }
struct Painter { _instance: wgpu::Instance, device: wgpu::Device, queue: wgpu::Queue, pipeline: wgpu::RenderPipeline, dab_pipeline: wgpu::RenderPipeline, targets: Option<Targets> }
static PAINTER: OnceLock<Option<Mutex<Painter>>> = OnceLock::new();
static FAILED: AtomicBool = AtomicBool::new(false);
#[derive(Clone, Debug)]
pub struct DrawingGpuStatus {pub adapter:String,pub backend:String,pub sample_rate:bool,pub max_dimension:u32,pub verified:bool,pub detail:String}
static STATUS:Mutex<DrawingGpuStatus>=Mutex::new(DrawingGpuStatus {adapter:String::new(),backend:String::new(),sample_rate:false,max_dimension:0,verified:false,detail:String::new()});
fn fail(message:&str) {FAILED.store(true,Ordering::Relaxed);if let Ok(mut status)=STATUS.lock() {status.verified=false;status.detail=message.to_owned();}log::error!("Drawing GPU: {message}");}
fn painter() -> Option<&'static Mutex<Painter>> { PAINTER.get_or_init(|| Painter::new().map(Mutex::new)).as_ref() }
pub fn available() -> bool { painter().is_some() && !FAILED.load(Ordering::Relaxed) }
pub fn status()->DrawingGpuStatus {
    let _=painter();
    STATUS.lock().map(|s|s.clone()).unwrap_or_else(|_|DrawingGpuStatus {adapter:String::new(),backend:String::new(),sample_rate:false,max_dimension:0,verified:false,detail:"Drawing GPU status unavailable".into()})
}

pub fn coverage(points: &[[f32; 3]], left: u32, top: u32, width: u32, height: u32) -> Option<Vec<u8>> {
    if points.is_empty() || points.len()>8192 || points.iter().any(|p|p.iter().any(|v|!v.is_finite()) || p[2]<=0.0 || p[2]>2048.0) || !available() || width == 0 || height == 0 || u64::from(width) * u64::from(height) > 4_194_304 { return None; }
    painter()?.lock().ok()?.draw(points, left, top, width, height)
}
/// Each dab is [x,y,radius_x,radius_y,rotation_radians,hardness,flow,tip,seed].
pub fn dab_coverage(dabs: &[[f32; 9]], left:u32, top:u32, width:u32, height:u32) -> Option<Vec<u8>> {
    if !available() || dabs.is_empty() || dabs.len()>16_384 || width==0 || height==0 || u64::from(width)*u64::from(height)>4_194_304 || dabs.iter().any(|d|d.iter().any(|v|!v.is_finite()) || d[2]<=0.0 || d[3]<=0.0 || d[2]>2048.0 || d[3]>2048.0 || !(0.0..=1.0).contains(&d[5]) || !(0.0..=1.0).contains(&d[6]) || !(0.0..=7.0).contains(&d[7])) {return None;}
    painter()?.lock().ok()?.draw_dabs(dabs,left,top,width,height)
}
impl Painter {
    fn new() -> Option<Self> {
        // Drawing owns its device: a brush failure must not disable video export/compositing.
        if wgpu::Instance::enabled_backend_features().is_empty() {fail("No GPU backend was compiled for this platform");return None;}
        let instance=crate::adapter::native_instance();
        let mut adapters=pollster::block_on(instance.enumerate_adapters(wgpu::Backends::all()));
        adapters.retain(|adapter| {
            let info=adapter.get_info();let features=adapter.get_texture_format_features(wgpu::TextureFormat::Rgba8Unorm);
            // Never label llvmpipe/SwiftShader/another CPU adapter as hardware drawing.
            info.device_type!=wgpu::DeviceType::Cpu && !info.name.to_ascii_lowercase().contains("swiftshader") && !info.name.to_ascii_lowercase().contains("llvmpipe") && !info.name.to_ascii_lowercase().contains("lavapipe") && features.allowed_usages.contains(wgpu::TextureUsages::RENDER_ATTACHMENT|wgpu::TextureUsages::COPY_SRC) && features.flags.contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_X4|wgpu::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE|wgpu::TextureFormatFeatureFlags::BLENDABLE)
        });
        let mut selected=None;
        while !adapters.is_empty() {
            let adapter=match crate::adapter::pick(&adapters,None,wgpu::PowerPreference::HighPerformance) {Ok(a)=>a,Err(_)=>break};
            let info=adapter.get_info();let sample_rate=adapter.get_downlevel_capabilities().flags.contains(wgpu::DownlevelFlags::MULTISAMPLED_SHADING);
            let descriptor=wgpu::DeviceDescriptor {required_limits:wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),..Default::default()};
            match pollster::block_on(adapter.request_device(&descriptor)) {
                Ok((device,queue))=>{selected=Some((info,sample_rate,device,queue));break;}
                Err(error)=>{
                    log::warn!("Drawing device {} ({:?}) unavailable: {error}; trying another hardware backend",info.name,info.backend);
                    adapters.retain(|candidate| {let other=candidate.get_info();other.name!=info.name || other.backend!=info.backend || other.device!=info.device;});
                }
            }
        }
        let Some((info,sample_rate,device,queue))=selected else {fail("No usable hardware 4x MSAA drawing device; adapter discovery or device creation failed");return None;};
        device.on_uncaptured_error(std::sync::Arc::new(|e:wgpu::Error|fail(&format!("GPU error: {e}"))));
        device.set_device_lost_callback(|reason,message|fail(&format!("Device lost ({reason:?}): {message}")));
        if let Ok(mut status)=STATUS.lock() {*status=DrawingGpuStatus {adapter:info.name,backend:format!("{:?}",info.backend),sample_rate,max_dimension:device.limits().max_texture_dimension_2d,verified:false,detail:"Validating drawing pipelines".into()};}
        let validation=device.push_error_scope(wgpu::ErrorFilter::Validation);
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
        let source=include_str!("drawing_dabs.wgsl");
        // GLES adapters without sample-rate shading still use a real 4x attachment and resolve;
        // only procedural opacity evaluation is per-fragment instead of per-sample.
        let compatible=source.replace("@interpolate(perspective, sample)","@interpolate(perspective, center)").replace(", @builtin(sample_index) sample: u32", "").replace("if sample >= 4u { discard; }", "");
        let dab_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("Native brush dab shader"), source: wgpu::ShaderSource::Wgsl(if sample_rate {source.into()} else {compatible.into()}) });
        const DAB_ATTRS: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4];
        let dab_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Native brush sample-rate 4x MSAA"), layout: Some(&layout),
            vertex: wgpu::VertexState { module: &dab_shader, entry_point: Some("vs_dab"), compilation_options: Default::default(), buffers: &[Some(wgpu::VertexBufferLayout { array_stride: 32, step_mode: wgpu::VertexStepMode::Vertex, attributes: &DAB_ATTRS })] },
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() }, depth_stencil: None,
            multisample: wgpu::MultisampleState { count: 4, mask: !0, alpha_to_coverage_enabled: false },
            fragment: Some(wgpu::FragmentState { module: &dab_shader, entry_point: Some("fs_dab"), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format: wgpu::TextureFormat::Rgba8Unorm, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }),
            multiview_mask: None, cache: None,
        });
        if let Some(error)=pollster::block_on(validation.pop()) {fail(&format!("Pipeline validation failed: {error}"));return None;}
        if FAILED.load(Ordering::Relaxed) {return None;}
        let mut painter=Self {_instance:instance,device,queue,pipeline,dab_pipeline,targets:None};
        // Verify actual render, MSAA resolve and readback before reporting support.
        let probe=painter.draw_dabs(&[[16.25,16.25,8.0,8.0,0.0,1.0,0.5,0.0,1.0]],0,0,32,32);
        let good=probe.as_ref().is_some_and(|p|p.len()==1024 && p[0]==0 && (i32::from(p[16*32+16])-128).abs()<=2 && p.iter().any(|v|*v>0 && *v<120));
        if !good {fail("Drawing self-test failed (render, 4x resolve, flow or readback)");return None;}
        if let Ok(mut status)=STATUS.lock() {status.verified=true;status.detail=if sample_rate {"Hardware 4x MSAA render/resolve/readback verified; sample-rate brush shading"} else {"Hardware 4x MSAA render/resolve/readback verified; per-fragment brush shading"}.into();}
        Some(painter)
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
        self.render(&vertices, count, w, h, false)
    }
    fn draw_dabs(&mut self, dabs: &[[f32; 9]], left: u32, top: u32, w: u32, h: u32) -> Option<Vec<u8>> {
        let mut vertices = Vec::with_capacity(dabs.len() * 6 * 32);
        for dab in dabs {
            let cos = dab[4].cos(); let sin = dab[4].sin();
            for [u, v] in [[-1.0,-1.0],[1.0,-1.0],[-1.0,1.0],[-1.0,1.0],[1.0,-1.0],[1.0,1.0]] {
                let x = dab[0] + u * dab[2] * cos - v * dab[3] * sin;
                let y = dab[1] + u * dab[2] * sin + v * dab[3] * cos;
                for value in [(x-left as f32)/w as f32*2.0-1.0,1.0-(y-top as f32)/h as f32*2.0,u,v,dab[5],dab[6],dab[7],dab[8]] { vertices.extend_from_slice(&value.to_le_bytes()); }
            }
        }
        self.render(&vertices, (dabs.len()*6) as u32, w, h, true)
    }
    fn render(&mut self, vertices: &[u8], count: u32, w: u32, h: u32, dabs: bool) -> Option<Vec<u8>> {
        let limit = self.device.limits().max_texture_dimension_2d;
        if w == 0 || h == 0 || w > limit || h > limit { return None; }
        let row = (w * 4).div_ceil(256) * 256;
        let targets = match self.targets.take() {
            Some(t) if t.w == w && t.h == h => t,
            _ => {
                let texture = |samples, usage| self.device.create_texture(&wgpu::TextureDescriptor { label: Some("Drawing coverage target"), size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: samples, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba8Unorm, usage, view_formats: &[] });
                Targets { w, h, msaa: texture(4, wgpu::TextureUsages::RENDER_ATTACHMENT), resolved: texture(1, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC), readback: self.device.create_buffer(&wgpu::BufferDescriptor { label: Some("Drawing coverage readback"), size: u64::from(row) * u64::from(h), usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false }) }
            }
        };
        let vertex = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("Drawing stroke triangles"), contents: vertices, usage: wgpu::BufferUsages::VERTEX });
        let msaa = targets.msaa.create_view(&Default::default()); let resolved = targets.resolved.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor { label: Some("Drawing 4x MSAA resolve"), color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &msaa, depth_slice: None, resolve_target: Some(&resolved), ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Discard } })], depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None });
            pass.set_pipeline(if dabs { &self.dab_pipeline } else { &self.pipeline }); pass.set_vertex_buffer(0, vertex.slice(..)); pass.draw(0..count, 0..1);
        }
        encoder.copy_texture_to_buffer(wgpu::TexelCopyTextureInfo { texture: &targets.resolved, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All }, wgpu::TexelCopyBufferInfo { buffer: &targets.readback, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) } }, wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 });
        self.queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        targets.readback.slice(..).map_async(wgpu::MapMode::Read, move |result| { let _ = tx.send(result); });
        if crate::wait_mapped(&self.device, &rx).is_none() {fail("GPU readback timed out or mapping failed");return None;}
        let mapped = match targets.readback.slice(..).get_mapped_range() {Ok(data)=>data,Err(e)=>{fail(&format!("GPU mapped range unavailable: {e}"));return None;}};
        let mut alpha = Vec::with_capacity(w as usize * h as usize);
        let mut line = Vec::new();
        for y in 0..h as usize {
            crate::copy_mapped(&mapped[y * row as usize..y * row as usize + w as usize * 4], &mut line);
            alpha.extend(line.chunks_exact(4).map(|pixel| pixel[3]));
        }
        drop(mapped); targets.readback.unmap(); self.targets = Some(targets);
        if FAILED.load(Ordering::Relaxed) { None } else { Some(alpha) }
    }
}
