//! Hardware 3D rasterizer shared by preview and export. The reference renderer submits draw
//! packets through a registered hook, so 3D effects nested inside masks/sequences use it too.
use std::sync::{Mutex, OnceLock};
use filmcraft_render::{Image, scene3d::{Scene, Draw}};
use filmcraft_render::model::{AlphaMode, TextureRef, Wrap};
use wgpu::util::DeviceExt;

pub fn register() { filmcraft_render::scene3d::register_backend(render); }

fn render(scene: &Scene) -> Option<Image> {
    static GPU: OnceLock<Option<Mutex<Renderer>>> = OnceLock::new();
    if crate::export_renderer::device_failed() { return None; }
    let mut gpu = GPU.get_or_init(|| Renderer::new().map(Mutex::new)).as_ref()?.lock().ok()?;
    let result = gpu.render(scene);
    if crate::export_renderer::device_failed() { None } else { result }
}

fn bytes(values: impl IntoIterator<Item=f32>) -> Vec<u8> {
    values.into_iter().flat_map(f32::to_le_bytes).collect()
}

struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    opaque: wgpu::RenderPipeline,
    transparent: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
}

impl Renderer {
    fn new() -> Option<Self> {
        let export = crate::ExportRenderer::new()?;
        let (device,queue) = export.graphics_device();
        let mut entries = vec![
            wgpu::BindGroupLayoutEntry { binding:0, visibility:wgpu::ShaderStages::VERTEX_FRAGMENT, ty:wgpu::BindingType::Buffer { ty:wgpu::BufferBindingType::Uniform, has_dynamic_offset:false, min_binding_size:None },count:None },
            wgpu::BindGroupLayoutEntry { binding:1, visibility:wgpu::ShaderStages::FRAGMENT, ty:wgpu::BindingType::Buffer { ty:wgpu::BufferBindingType::Storage { read_only:true }, has_dynamic_offset:false, min_binding_size:None },count:None },
            wgpu::BindGroupLayoutEntry { binding:2, visibility:wgpu::ShaderStages::FRAGMENT, ty:wgpu::BindingType::Buffer { ty:wgpu::BufferBindingType::Uniform, has_dynamic_offset:false, min_binding_size:None },count:None },
        ];
        for binding in 3..8 {
            entries.push(wgpu::BindGroupLayoutEntry { binding, visibility:wgpu::ShaderStages::FRAGMENT, ty:wgpu::BindingType::Texture { sample_type:wgpu::TextureSampleType::Float { filterable:true }, view_dimension:wgpu::TextureViewDimension::D2, multisampled:false },count:None });
        }
        for binding in 8..13 {
            entries.push(wgpu::BindGroupLayoutEntry { binding, visibility:wgpu::ShaderStages::FRAGMENT, ty:wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),count:None });
        }
        entries.push(wgpu::BindGroupLayoutEntry { binding:13, visibility:wgpu::ShaderStages::VERTEX, ty:wgpu::BindingType::Buffer {ty:wgpu::BufferBindingType::Storage {read_only:true},has_dynamic_offset:false,min_binding_size:None},count:None });
        let layout=device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label:Some("3D materials"), entries:&entries });
        let pipeline_layout=device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label:Some("3D"), bind_group_layouts:&[Some(&layout)], immediate_size:0 });
        let shader=device.create_shader_module(wgpu::ShaderModuleDescriptor { label:Some("3D lighting"), source:wgpu::ShaderSource::Wgsl(include_str!("scene3d.wgsl").into()) });
        const ATTRS: [wgpu::VertexAttribute;12]=wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x3,2=>Float32x4,3=>Float32x4,4=>Float32x2,5=>Float32x2,6=>Float32x2,7=>Float32x2,8=>Float32x2,9=>Float32x4,10=>Float32x4,11=>Float32];
        let make_pipeline=|blend:bool| device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:Some(if blend {"3D transparent"} else {"3D opaque"}), layout:Some(&pipeline_layout),
            vertex:wgpu::VertexState { module:&shader,entry_point:Some("vs"),compilation_options:Default::default(),buffers:&[Some(wgpu::VertexBufferLayout { array_stride:132,step_mode:wgpu::VertexStepMode::Vertex,attributes:&ATTRS })] },
            primitive:wgpu::PrimitiveState { cull_mode:None,..Default::default() },
            depth_stencil:Some(wgpu::DepthStencilState { format:wgpu::TextureFormat::Depth32Float,depth_write_enabled:Some(!blend),depth_compare:Some(wgpu::CompareFunction::LessEqual),stencil:Default::default(),bias:Default::default() }),
            multisample:wgpu::MultisampleState {count:4,..Default::default()},
            fragment:Some(wgpu::FragmentState { module:&shader,entry_point:Some("fs"),compilation_options:Default::default(),targets:&[Some(wgpu::ColorTargetState { format:wgpu::TextureFormat::Rgba16Float,blend:if blend {Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING)} else {None},write_mask:wgpu::ColorWrites::ALL })] }),
            multiview_mask:None,cache:None,
        });
        let opaque=make_pipeline(false); let transparent=make_pipeline(true);
        Some(Self {device,queue,opaque,transparent,layout})
    }

    fn buffer(&self, label:&str, data:&[u8], usage:wgpu::BufferUsages) -> wgpu::Buffer {
        self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label:Some(label),contents:data,usage })
    }

    fn texture(&self, width:u32, height:u32, format:wgpu::TextureFormat, usage:wgpu::TextureUsages) -> wgpu::Texture {
        self.texture_samples(width,height,format,usage,1)
    }
    fn texture_samples(&self, width:u32, height:u32, format:wgpu::TextureFormat, usage:wgpu::TextureUsages, samples:u32) -> wgpu::Texture {
        self.device.create_texture(&wgpu::TextureDescriptor { label:Some("3D texture"),size:wgpu::Extent3d {width,height,depth_or_array_layers:1},mip_level_count:1,sample_count:samples,dimension:wgpu::TextureDimension::D2,format,usage,view_formats:&[] })
    }

    fn prepare(&self, scene:&Scene, draw:&Draw, lights:&wgpu::Buffer) -> Option<(wgpu::Buffer,wgpu::BindGroup,u32)> {
        let m=&draw.material;
        let refs=[m.base_texture,m.mr_texture,m.normal_texture,m.occlusion_texture,m.emissive_texture];
        let mut packed=Vec::with_capacity(draw.vertices.len()*24);
        for v in &draw.vertices {
            packed.extend(v.position); packed.extend(v.normal); packed.extend(v.tangent); packed.extend(v.color);
            for r in refs {
                let uv=r.map(|r| r.uv(v.uv)).unwrap_or((0.0,0.0));
                packed.extend([uv.0,uv.1]);
            }
            packed.extend(v.joints); packed.extend(v.weights); packed.push(v.morph_index);
        }
        let deformation=if draw.deformation.is_empty() {vec![0.0;68]} else {draw.deformation.clone()};
        if deformation.len() as u64*4>self.device.limits().max_storage_buffer_binding_size as u64 {return None;}
        let deformation=self.buffer("3D deformation",&bytes(deformation),wgpu::BufferUsages::STORAGE);
        let vertex=self.buffer("3D vertices",&bytes(packed),wgpu::BufferUsages::VERTEX);
        let alpha=match m.alpha_mode {AlphaMode::Opaque=>0.0,AlphaMode::Mask=>1.0,AlphaMode::Blend=>2.0};
        let flags=|b:bool| if b {1.0} else {0.0};
        let scene_buffer=self.buffer("3D camera",&bytes([
            scene.camera[0],scene.camera[1],scene.camera[2],scene.opacity,
            scene.width as f32,scene.height as f32,m.alpha_cutoff,0.0,
            scene.ambient[0],scene.ambient[1],scene.ambient[2],scene.lights.len() as f32,
        ]),wgpu::BufferUsages::UNIFORM);
        let material=self.buffer("3D material",&bytes([
            m.base_color[0],m.base_color[1],m.base_color[2],m.base_color[3],
            m.metallic,m.roughness,m.normal_scale,m.occlusion_strength,
            m.emissive[0],m.emissive[1],m.emissive[2],flags(m.normal_texture.is_some()),
            alpha,flags(m.double_sided),flags(m.unlit),flags(draw.pbr),
        ]),wgpu::BufferUsages::UNIFORM);
        let mut views=Vec::new(); let mut samplers=Vec::new();
        for (i,r) in refs.iter().enumerate() {
            let source=draw.model.as_ref().and_then(|model| r.and_then(|r:TextureRef| model.textures.get(r.index)));
            let fallback=if i==2 { [128,128,255,255] } else { [255;4] };
            let (w,h,data,srgb,wrap)=source.map(|s| (s.w as u32,s.h as u32,s.rgba.as_slice(),s.srgb,s.wrap)).unwrap_or((1,1,&fallback,false,(Wrap::Clamp,Wrap::Clamp)));
            if w==0 || h==0 || w>self.device.limits().max_texture_dimension_2d || h>self.device.limits().max_texture_dimension_2d {return None;}
            let format=if srgb {wgpu::TextureFormat::Rgba8UnormSrgb} else {wgpu::TextureFormat::Rgba8Unorm};
            let texture=self.texture(w,h,format,wgpu::TextureUsages::TEXTURE_BINDING|wgpu::TextureUsages::COPY_DST);
            self.queue.write_texture(wgpu::TexelCopyTextureInfo {texture:&texture,mip_level:0,origin:wgpu::Origin3d::ZERO,aspect:wgpu::TextureAspect::All},data,wgpu::TexelCopyBufferLayout {offset:0,bytes_per_row:Some(w*4),rows_per_image:Some(h)},wgpu::Extent3d {width:w,height:h,depth_or_array_layers:1});
            views.push(texture.create_view(&Default::default()));
            let address=|w| match w {Wrap::Repeat=>wgpu::AddressMode::Repeat,Wrap::Mirror=>wgpu::AddressMode::MirrorRepeat,Wrap::Clamp=>wgpu::AddressMode::ClampToEdge};
            samplers.push(self.device.create_sampler(&wgpu::SamplerDescriptor {address_mode_u:address(wrap.0),address_mode_v:address(wrap.1),mag_filter:wgpu::FilterMode::Linear,min_filter:wgpu::FilterMode::Linear,..Default::default()}));
        }
        let mut entries=vec![wgpu::BindGroupEntry {binding:0,resource:scene_buffer.as_entire_binding()},wgpu::BindGroupEntry {binding:1,resource:lights.as_entire_binding()},wgpu::BindGroupEntry {binding:2,resource:material.as_entire_binding()}];
        for (i,view) in views.iter().enumerate() {entries.push(wgpu::BindGroupEntry {binding:3+i as u32,resource:wgpu::BindingResource::TextureView(view)});}
        for (i,sampler) in samplers.iter().enumerate() {entries.push(wgpu::BindGroupEntry {binding:8+i as u32,resource:wgpu::BindingResource::Sampler(sampler)});}
        entries.push(wgpu::BindGroupEntry {binding:13,resource:deformation.as_entire_binding()});
        let group=self.device.create_bind_group(&wgpu::BindGroupDescriptor {label:Some("3D draw"),layout:&self.layout,entries:&entries});
        Some((vertex,group,draw.vertices.len() as u32))
    }

    fn render(&mut self, scene:&Scene) -> Option<Image> {
        let (w,h)=(scene.width as u32,scene.height as u32);
        if w==0 || h==0 || w>self.device.limits().max_texture_dimension_2d || h>self.device.limits().max_texture_dimension_2d {return None;}
        let color=self.texture(w,h,wgpu::TextureFormat::Rgba16Float,wgpu::TextureUsages::RENDER_ATTACHMENT|wgpu::TextureUsages::COPY_SRC);
        let depth=self.texture_samples(w,h,wgpu::TextureFormat::Depth32Float,wgpu::TextureUsages::RENDER_ATTACHMENT,4);
        let multisample=self.texture_samples(w,h,wgpu::TextureFormat::Rgba16Float,wgpu::TextureUsages::RENDER_ATTACHMENT,4);
        let multisample_view=multisample.create_view(&Default::default());
        let view=color.create_view(&Default::default()); let depth_view=depth.create_view(&Default::default());
        let mut data=Vec::new();
        for l in &scene.lights {
            data.extend([l.color[0],l.color[1],l.color[2],l.kind as f32,l.direction[0],l.direction[1],l.direction[2],0.0,l.position[0],l.position[1],l.position[2],l.range,l.cos_outer,l.cos_inner,0.0,0.0]);
        }
        if data.is_empty() {data.resize(16,0.0);}
        let lights=self.buffer("3D lights",&bytes(data),wgpu::BufferUsages::STORAGE);
        if lights.size()>self.device.limits().max_storage_buffer_binding_size as u64 {return None;}
        let mut prepared=Vec::new();
        let mut opaque=Vec::new(); let mut transparent=Vec::new();
        for d in &scene.draws {
            if d.vertices.is_empty() {continue;}
            let index=prepared.len();
            prepared.push(self.prepare(scene,d,&lights)?);
            if d.material.alpha_mode==AlphaMode::Blend {
                for (i,t) in d.vertices.chunks_exact(3).enumerate() {
                    transparent.push((triangle_depth(d,t),index,(i*3) as u32));
                }
            } else {opaque.push(index);}
        }
        transparent.sort_by(|a,b| b.0.total_cmp(&a.0));
        let mut encoder=self.device.create_command_encoder(&Default::default());
        {
            let mut pass=encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label:Some("3D scene"),color_attachments:&[Some(wgpu::RenderPassColorAttachment {view:&multisample_view,depth_slice:None,resolve_target:Some(&view),ops:wgpu::Operations {load:wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),store:wgpu::StoreOp::Store}})],
                depth_stencil_attachment:Some(wgpu::RenderPassDepthStencilAttachment {view:&depth_view,depth_ops:Some(wgpu::Operations {load:wgpu::LoadOp::Clear(1.0),store:wgpu::StoreOp::Store}),stencil_ops:None}),timestamp_writes:None,occlusion_query_set:None,multiview_mask:None,
            });
            pass.set_pipeline(&self.opaque);
            for index in opaque {
                let (vertex,group,count)=&prepared[index];
                pass.set_bind_group(0,group,&[]);pass.set_vertex_buffer(0,vertex.slice(..));pass.draw(0..*count,0..1);
            }
            pass.set_pipeline(&self.transparent);
            for (_,index,start) in transparent {
                let (vertex,group,_)=&prepared[index];
                pass.set_bind_group(0,group,&[]);pass.set_vertex_buffer(0,vertex.slice(..));pass.draw(start..start+3,0..1);
            }
        }
        let row=(w*8).div_ceil(256)*256;
        let readback=self.device.create_buffer(&wgpu::BufferDescriptor {label:Some("3D readback"),size:row as u64*h as u64,usage:wgpu::BufferUsages::COPY_DST|wgpu::BufferUsages::MAP_READ,mapped_at_creation:false});
        encoder.copy_texture_to_buffer(wgpu::TexelCopyTextureInfo {texture:&color,mip_level:0,origin:wgpu::Origin3d::ZERO,aspect:wgpu::TextureAspect::All},wgpu::TexelCopyBufferInfo {buffer:&readback,layout:wgpu::TexelCopyBufferLayout {offset:0,bytes_per_row:Some(row),rows_per_image:Some(h)}},wgpu::Extent3d {width:w,height:h,depth_or_array_layers:1});
        self.queue.submit([encoder.finish()]);
        let (tx,rx)=std::sync::mpsc::channel();
        readback.slice(..).map_async(wgpu::MapMode::Read,move |r| {let _=tx.send(r);});
        crate::wait_mapped(&self.device,&rx)?;
        let mapped=readback.slice(..).get_mapped_range().ok()?;
        let mut px=Vec::with_capacity(w as usize*h as usize*4);
        for line in mapped.chunks_exact(row as usize) {
            let line=line[..w as usize*8].to_vec();
            px.extend(line.chunks_exact(2).map(|b| crate::f16_to_f32(u16::from_le_bytes([b[0],b[1]]))));
        }
        drop(mapped); readback.unmap();
        Some(Image {w:w as usize,h:h as usize,px})
    }
}

/// Transparent draw ordering is command preparation, not rasterization. Only centroids/depth
/// are needed on the host; vertex deformation and interpolation still run in the vertex stage.
fn triangle_depth(draw:&Draw, triangle:&[filmcraft_render::scene3d::Vertex])->f32 {
    use filmcraft_render::model::math;
    let data=&draw.deformation;
    if data.len()<68 {return triangle.iter().map(|v|v.position[2]).sum::<f32>();}
    let matrix=|offset:usize| -> math::Mat4 {
        std::array::from_fn(|i| data.get(offset+i).copied().unwrap_or(0.0))
    };
    let joints=data[65] as usize; let morphs=data[66] as usize; let vertices=data[67] as usize;
    triangle.iter().map(|v| {
        let mut pos=v.position;
        for target in 0..morphs {
            let weight=data.get(68+joints*32+target*4).copied().unwrap_or(0.0);
            let offset=68+joints*32+morphs*4+(target*vertices+v.morph_index as usize)*12;
            for c in 0..3 {pos[c]+=weight*data.get(offset+c).copied().unwrap_or(0.0);}
        }
        let mut p=[0.0;3]; let mut total=0.0;
        for k in 0..4 {
            if v.weights[k]<=0.0 || v.joints[k] as usize>=joints {continue;}
            let transformed=math::point(&matrix(68+v.joints[k] as usize*32),pos);
            for c in 0..3 {p[c]+=v.weights[k]*transformed[c];}
            total+=v.weights[k];
        }
        if total<=1e-6 {p=math::point(&matrix(32),pos);}
        math::point(&matrix(0),p)[2]
    }).sum()
}
