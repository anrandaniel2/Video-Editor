//! Full-GPU optical-flow & corner-detection backend (`GpuTrackBackend`) for 2D Motion Tracking
//! and 3D Camera Match-Move.
//!
//! Keeps multi-scale Gaussian + 3x3 Scharr pyramids resident in VRAM across frames (`slots 0..=2`),
//! executing RGBA->luma crop/conform, pyramid construction, tiled Shi-Tomasi corner extraction,
//! batched illumination-normalized Inverse-Compositional Lucas-Kanade optical flow with
//! forward-backward verification, ZNCC coarse template recovery, and stereo disparity on the GPU.

use std::sync::{Mutex, OnceLock};

use filmcraft_render::track3d::{GpuTrackBackend, GpuTrackQuery, Gray, register_gpu_backend};
use wgpu::util::DeviceExt;

const MAX_SLOTS: usize = 4;
const MAX_LEVELS: usize = 4;

#[derive(Clone, Copy, Default)]
struct LevelMeta {
    offset: u32,
    width: u32,
    height: u32,
}

struct PyrSlot {
    levels: usize,
    meta: [LevelMeta; MAX_LEVELS],
    total_elems: usize,
    pyr_buf: wgpu::Buffer,
}

struct GpuTrackerEngine {
    device: wgpu::Device,
    queue: wgpu::Queue,
    bgl_build: wgpu::BindGroupLayout,
    bgl_corners: wgpu::BindGroupLayout,
    bgl_lk: wgpu::BindGroupLayout,
    pipe_rgba_to_luma: wgpu::ComputePipeline,
    pipe_pyr_down: wgpu::ComputePipeline,
    pipe_scharr: wgpu::ComputePipeline,
    pipe_corners: wgpu::ComputePipeline,
    pipe_pyrlk_fb: wgpu::ComputePipeline,
    pipe_zncc: wgpu::ComputePipeline,
    pipe_stereo: wgpu::ComputePipeline,
    slots: [Option<PyrSlot>; MAX_SLOTS],
    scratch_pyr: Option<(usize, wgpu::Buffer)>,
}

struct GpuTrackAdapter;

static ENGINE: OnceLock<Option<Mutex<GpuTrackerEngine>>> = OnceLock::new();
static ADAPTER: GpuTrackAdapter = GpuTrackAdapter;

/// Register the `wgpu` compute tracker backend with `filmcraft-render` (idempotent).
pub fn register() {
    register_gpu_backend(&ADAPTER);
}

fn engine() -> Option<&'static Mutex<GpuTrackerEngine>> {
    if crate::export_renderer::device_failed() {
        return None;
    }
    ENGINE.get_or_init(|| GpuTrackerEngine::new().map(Mutex::new)).as_ref()
}

fn storage_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn pack_u32s(words: &[u32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(words.len() * 4);
    for &w in words {
        out.extend_from_slice(&w.to_ne_bytes());
    }
    out
}

fn pack_vec4_f32(vecs: &[[f32; 4]]) -> Vec<u8> {
    let mut out = Vec::with_capacity(vecs.len() * 16);
    for v in vecs {
        for &c in v {
            out.extend_from_slice(&c.to_ne_bytes());
        }
    }
    out
}

fn unpack_f32s(bytes: &[u8]) -> Vec<f32> {
    let mut out = Vec::with_capacity(bytes.len() / 4);
    for ch in bytes.chunks_exact(4) {
        let arr = [ch[0], ch[1], ch[2], ch[3]];
        out.push(f32::from_ne_bytes(arr));
    }
    out
}

fn unpack_vec4_f32(bytes: &[u8]) -> Vec<[f32; 4]> {
    let mut out = Vec::with_capacity(bytes.len() / 16);
    for ch in bytes.chunks_exact(16) {
        let f0 = f32::from_ne_bytes([ch[0], ch[1], ch[2], ch[3]]);
        let f1 = f32::from_ne_bytes([ch[4], ch[5], ch[6], ch[7]]);
        let f2 = f32::from_ne_bytes([ch[8], ch[9], ch[10], ch[11]]);
        let f3 = f32::from_ne_bytes([ch[12], ch[13], ch[14], ch[15]]);
        out.push([f0, f1, f2, f3]);
    }
    out
}

fn compute_level_layout(w0: usize, h0: usize, max_levels: usize) -> (usize, [LevelMeta; MAX_LEVELS], usize) {
    let mut meta = [LevelMeta::default(); MAX_LEVELS];
    let cap = max_levels.clamp(1, MAX_LEVELS);
    let mut levels = 0usize;
    let mut offset = 0u32;
    let mut cw = w0;
    let mut ch = h0;
    while levels < cap {
        meta[levels] = LevelMeta { offset, width: cw as u32, height: ch as u32 };
        offset = offset.saturating_add((cw * ch) as u32);
        levels += 1;
        if cw < 24 || ch < 24 || levels == cap {
            break;
        }
        cw = cw.div_ceil(2);
        ch = ch.div_ceil(2);
    }
    (levels, meta, offset as usize)
}

impl GpuTrackerEngine {
    fn new() -> Option<Self> {
        let export = crate::ExportRenderer::new()?;
        let (device, queue) = export.graphics_device();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("GPU tracker shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("tracker.wgsl").into()),
        });

        let bgl_build = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("tracker-bgl-build"),
            entries: &[
                uniform_entry(0),
                storage_entry(1, true),
                storage_entry(2, true),
                storage_entry(3, false),
                storage_entry(4, false),
            ],
        });
        let pl_build = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("tracker-pl-build"),
            bind_group_layouts: &[Some(&bgl_build)],
            immediate_size: 0,
        });

        let bgl_corners = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("tracker-bgl-corners"),
            entries: &[uniform_entry(0), storage_entry(1, true), storage_entry(2, false)],
        });
        let pl_corners = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("tracker-pl-corners"),
            bind_group_layouts: &[Some(&bgl_corners)],
            immediate_size: 0,
        });

        let bgl_lk = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("tracker-bgl-lk"),
            entries: &[
                uniform_entry(0),
                storage_entry(1, true),
                storage_entry(2, true),
                storage_entry(3, true),
                storage_entry(4, false),
            ],
        });
        let pl_lk = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("tracker-pl-lk"),
            bind_group_layouts: &[Some(&bgl_lk)],
            immediate_size: 0,
        });

        let make_pipe = |pl: &wgpu::PipelineLayout, ep: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(ep),
                layout: Some(pl),
                module: &shader,
                entry_point: Some(ep),
                compilation_options: Default::default(),
                cache: None,
            })
        };

        let pipe_rgba_to_luma = make_pipe(&pl_build, "cs_rgba_to_luma");
        let pipe_pyr_down = make_pipe(&pl_build, "cs_pyr_down");
        let pipe_scharr = make_pipe(&pl_build, "cs_scharr");
        let pipe_corners = make_pipe(&pl_corners, "cs_corners");
        let pipe_pyrlk_fb = make_pipe(&pl_lk, "cs_pyrlk_fb");
        let pipe_zncc = make_pipe(&pl_lk, "cs_zncc_search");
        let pipe_stereo = make_pipe(&pl_lk, "cs_stereo_disp");

        Some(Self {
            device,
            queue,
            bgl_build,
            bgl_corners,
            bgl_lk,
            pipe_rgba_to_luma,
            pipe_pyr_down,
            pipe_scharr,
            pipe_corners,
            pipe_pyrlk_fb,
            pipe_zncc,
            pipe_stereo,
            slots: [None, None, None, None],
            scratch_pyr: None,
        })
    }

    fn init_buf(&self, label: &str, data: &[u8], usage: wgpu::BufferUsages) -> wgpu::Buffer {
        self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(label),
            contents: data,
            usage,
        })
    }

    fn readback_bytes(&self, src_buf: &wgpu::Buffer, byte_len: u64, mut enc: wgpu::CommandEncoder) -> Option<Vec<u8>> {
        if byte_len == 0 {
            self.queue.submit(Some(enc.finish()));
            return Some(Vec::new());
        }
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tracker-readback"),
            size: byte_len,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        enc.copy_buffer_to_buffer(src_buf, 0, &staging, 0, byte_len);
        self.queue.submit(Some(enc.finish()));

        let slice = staging.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |res| {
            let _ = tx.send(res);
        });
        super::wait_mapped(&self.device, &rx)?;
        let data = slice.get_mapped_range().ok()?;
        let mut out = vec![0u8; byte_len as usize];
        crate::copy_mapped(&data, &mut out);
        drop(data);
        staging.unmap();
        Some(out)
    }

    fn ensure_slot(&mut self, slot: usize, w0: usize, h0: usize, max_levels: usize) -> Option<([LevelMeta; MAX_LEVELS], usize, usize)> {
        if slot >= MAX_SLOTS || w0 == 0 || h0 == 0 {
            return None;
        }
        let (levels, meta, total_elems) = compute_level_layout(w0, h0, max_levels);
        let need_alloc = self.slots[slot].as_ref().is_none_or(|s| s.total_elems < total_elems);
        if need_alloc {
            let pyr_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("tracker-pyr-slot"),
                size: (total_elems.max(1) * 16) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            self.slots[slot] = Some(PyrSlot { levels, meta, total_elems, pyr_buf });
        } else if let Some(s) = self.slots[slot].as_mut() {
            s.levels = levels;
            s.meta = meta;
        }
        let need_scratch = self.scratch_pyr.as_ref().is_none_or(|(cap, _)| *cap < total_elems);
        if need_scratch {
            let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("tracker-pyr-scratch"),
                size: (total_elems.max(1) * 16) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.scratch_pyr = Some((total_elems, buf));
        }
        Some((meta, levels, total_elems))
    }

    #[allow(clippy::too_many_arguments)]
    fn build_rgba_pyramid(
        &mut self,
        slot: usize,
        rgba: &[u8],
        src_w: usize,
        src_h: usize,
        crop: (usize, usize, usize, usize),
        dst_dims: (usize, usize),
        levels_req: usize,
        readback_luma: bool,
    ) -> Option<Vec<f32>> {
        let (dst_w, dst_h) = dst_dims;
        if src_w == 0 || src_h == 0 || dst_w < 8 || dst_h < 8 || rgba.len() < src_w.saturating_mul(src_h).saturating_mul(4) {
            return None;
        }
        let (meta, levels, _) = self.ensure_slot(slot, dst_w, dst_h, levels_req)?;
        let slot_ref = self.slots[slot].as_ref()?;
        let (_, scratch_buf) = self.scratch_pyr.as_ref()?;

        let rgba_slice = &rgba[..src_w * src_h * 4];
        let rgba_buf = self.init_buf("tracker-rgba-in", rgba_slice, wgpu::BufferUsages::STORAGE);
        let luma_bytes = (dst_w * dst_h * 4) as u64;
        let luma_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tracker-luma-l0"),
            size: luma_bytes.max(16),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("tracker-build-pyr"),
        });

        // 1. RGBA8 -> Level 0 luma in `scratch_buf` (and `luma_buf`).
        let u0 = pack_u32s(&[
            src_w as u32,
            src_h as u32,
            crop.0 as u32,
            crop.1 as u32,
            crop.2.max(1) as u32,
            crop.3.max(1) as u32,
            dst_w as u32,
            dst_h as u32,
            0,
            meta[0].offset,
            0,
            0,
        ]);
        let u0_buf = self.init_buf("ub-rgba", &u0, wgpu::BufferUsages::UNIFORM);
        let bg0 = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bg-rgba"),
            layout: &self.bgl_build,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: u0_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: rgba_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: slot_ref.pyr_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: scratch_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: luma_buf.as_entire_binding() },
            ],
        });
        {
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("rgba->luma"), timestamp_writes: None });
            pass.set_pipeline(&self.pipe_rgba_to_luma);
            pass.set_bind_group(0, &bg0, &[]);
            pass.dispatch_workgroups((dst_w as u32).div_ceil(16), (dst_h as u32).div_ceil(16), 1);
        }
        enc.copy_buffer_to_buffer(
            scratch_buf,
            u64::from(meta[0].offset) * 16,
            &slot_ref.pyr_buf,
            u64::from(meta[0].offset) * 16,
            u64::from(meta[0].width) * u64::from(meta[0].height) * 16,
        );

        // 2. Gaussian 5x5 downsample for levels 1..levels (reading `slot_ref.pyr_buf`, writing `scratch_buf`,
        // then copying level `l` into `slot_ref.pyr_buf` so `src` and `dst` are never the same buffer).
        for l in 1..levels {
            let prev = meta[l - 1];
            let cur = meta[l];
            let ul = pack_u32s(&[prev.width, prev.height, 0, 0, 0, 0, cur.width, cur.height, prev.offset, cur.offset, 0, 0]);
            let ul_buf = self.init_buf("ub-down", &ul, wgpu::BufferUsages::UNIFORM);
            let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bg-down"),
                layout: &self.bgl_build,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: ul_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: rgba_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 2, resource: slot_ref.pyr_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 3, resource: scratch_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 4, resource: luma_buf.as_entire_binding() },
                ],
            });
            {
                let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("pyr-down"), timestamp_writes: None });
                pass.set_pipeline(&self.pipe_pyr_down);
                pass.set_bind_group(0, &bg, &[]);
                pass.dispatch_workgroups(cur.width.div_ceil(16), cur.height.div_ceil(16), 1);
            }
            enc.copy_buffer_to_buffer(
                scratch_buf,
                u64::from(cur.offset) * 16,
                &slot_ref.pyr_buf,
                u64::from(cur.offset) * 16,
                u64::from(cur.width) * u64::from(cur.height) * 16,
            );
        }

        // 3. 3x3 Scharr spatial gradients on every level (reading `scratch_buf`, writing `(luma, ix, iy, 0)`
        // directly into `slot_ref.pyr_buf`).
        for cur in meta.iter().take(levels) {
            let us = pack_u32s(&[cur.width, cur.height, 0, 0, 0, 0, cur.width, cur.height, cur.offset, cur.offset, 0, 0]);
            let us_buf = self.init_buf("ub-scharr", &us, wgpu::BufferUsages::UNIFORM);
            let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bg-scharr"),
                layout: &self.bgl_build,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: us_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: rgba_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 2, resource: scratch_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 3, resource: slot_ref.pyr_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 4, resource: luma_buf.as_entire_binding() },
                ],
            });
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("scharr"), timestamp_writes: None });
            pass.set_pipeline(&self.pipe_scharr);
            pass.set_bind_group(0, &bg, &[]);
            pass.dispatch_workgroups(cur.width.div_ceil(16), cur.height.div_ceil(16), 1);
        }

        if readback_luma {
            let bytes = self.readback_bytes(&luma_buf, luma_bytes, enc)?;
            Some(unpack_f32s(&bytes))
        } else {
            self.queue.submit(Some(enc.finish()));
            Some(Vec::new())
        }
    }

    fn build_luma_pyramid(&mut self, slot: usize, luma: &[f32], w: usize, h: usize, levels_req: usize) -> bool {
        if w < 8 || h < 8 || luma.len() < w.saturating_mul(h) {
            return false;
        }
        let Some((meta, levels, _)) = self.ensure_slot(slot, w, h, levels_req) else {
            return false;
        };
        let (Some(slot_ref), Some((_, scratch_buf))) = (self.slots[slot].as_ref(), self.scratch_pyr.as_ref()) else {
            return false;
        };
        let mut lvl0_vecs = Vec::with_capacity(w * h);
        for &v in &luma[..w * h] {
            lvl0_vecs.push([v, 0.0, 0.0, 0.0]);
        }
        let lvl0_bytes = pack_vec4_f32(&lvl0_vecs);
        self.queue.write_buffer(scratch_buf, 0, &lvl0_bytes);
        self.queue.write_buffer(&slot_ref.pyr_buf, 0, &lvl0_bytes);

        let dummy_rgba = self.init_buf("dummy-rgba", &[0u8; 16], wgpu::BufferUsages::STORAGE);
        let dummy_luma = self.init_buf("dummy-luma", &[0u8; 16], wgpu::BufferUsages::STORAGE);
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("tracker-luma-pyr") });

        for l in 1..levels {
            let prev = meta[l - 1];
            let cur = meta[l];
            let ul = pack_u32s(&[prev.width, prev.height, 0, 0, 0, 0, cur.width, cur.height, prev.offset, cur.offset, 0, 0]);
            let ul_buf = self.init_buf("ub-down-l", &ul, wgpu::BufferUsages::UNIFORM);
            let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bg-down-l"),
                layout: &self.bgl_build,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: ul_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: dummy_rgba.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 2, resource: slot_ref.pyr_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 3, resource: scratch_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 4, resource: dummy_luma.as_entire_binding() },
                ],
            });
            {
                let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("pyr-down-l"), timestamp_writes: None });
                pass.set_pipeline(&self.pipe_pyr_down);
                pass.set_bind_group(0, &bg, &[]);
                pass.dispatch_workgroups(cur.width.div_ceil(16), cur.height.div_ceil(16), 1);
            }
            enc.copy_buffer_to_buffer(
                scratch_buf,
                u64::from(cur.offset) * 16,
                &slot_ref.pyr_buf,
                u64::from(cur.offset) * 16,
                u64::from(cur.width) * u64::from(cur.height) * 16,
            );
        }

        for cur in meta.iter().take(levels) {
            let us = pack_u32s(&[cur.width, cur.height, 0, 0, 0, 0, cur.width, cur.height, cur.offset, cur.offset, 0, 0]);
            let us_buf = self.init_buf("ub-scharr-l", &us, wgpu::BufferUsages::UNIFORM);
            let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bg-scharr-l"),
                layout: &self.bgl_build,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: us_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: dummy_rgba.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 2, resource: scratch_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 3, resource: slot_ref.pyr_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 4, resource: dummy_luma.as_entire_binding() },
                ],
            });
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("scharr-l"), timestamp_writes: None });
            pass.set_pipeline(&self.pipe_scharr);
            pass.set_bind_group(0, &bg, &[]);
            pass.dispatch_workgroups(cur.width.div_ceil(16), cur.height.div_ceil(16), 1);
        }
        self.queue.submit(Some(enc.finish()));
        true
    }

    fn detect_corners(&self, slot: usize, roi: (usize, usize, usize, usize), cell: usize) -> Option<Vec<(f32, f32, f32)>> {
        let s = self.slots.get(slot)?.as_ref()?;
        let m0 = s.meta[0];
        let cell_u = cell.max(4);
        let x0 = roi.0.min(m0.width as usize);
        let y0 = roi.1.min(m0.height as usize);
        let x1 = roi.2.min(m0.width as usize);
        let y1 = roi.3.min(m0.height as usize);
        if x1 <= x0 || y1 <= y0 {
            return Some(Vec::new());
        }
        let tiles_x = (x1 - x0).div_ceil(cell_u);
        let tiles_y = (y1 - y0).div_ceil(cell_u);
        let total_tiles = tiles_x * tiles_y;
        if total_tiles == 0 {
            return Some(Vec::new());
        }

        let uc = pack_u32s(&[
            m0.width,
            m0.height,
            x0 as u32,
            y0 as u32,
            x1 as u32,
            y1 as u32,
            cell_u as u32,
            tiles_x as u32,
            tiles_y as u32,
            m0.offset,
            0,
            0,
        ]);
        let uc_buf = self.init_buf("uc-corners", &uc, wgpu::BufferUsages::UNIFORM);
        let out_bytes = (total_tiles * 16) as u64;
        let out_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("corners-out"),
            size: out_bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bg-corners"),
            layout: &self.bgl_corners,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uc_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: s.pyr_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: out_buf.as_entire_binding() },
            ],
        });
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("tracker-corners") });
        {
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("cs-corners"), timestamp_writes: None });
            pass.set_pipeline(&self.pipe_corners);
            pass.set_bind_group(0, &bg, &[]);
            pass.dispatch_workgroups((total_tiles as u32).div_ceil(64), 1, 1);
        }
        let raw = self.readback_bytes(&out_buf, out_bytes, enc)?;
        let vecs = unpack_vec4_f32(&raw);
        let mut candidates: Vec<(f32, f32, f32)> = vecs
            .into_iter()
            .filter(|v| v[3] > 0.5 && v[0] > 1e-3)
            .map(|v| (v[0], v[1], v[2]))
            .collect();
        candidates.sort_by(|a, b| b.0.total_cmp(&a.0));
        Some(candidates)
    }

    #[allow(clippy::too_many_arguments)]
    fn pack_lk_uniform(
        prev: &PyrSlot,
        levels: u32,
        count: u32,
        patch_r: i32,
        search_range: i32,
        fb_max_px: f32,
        max_disp: f32,
        pt: (f32, f32),
    ) -> Vec<u8> {
        let mut words = Vec::with_capacity(24);
        for m in &prev.meta {
            words.extend_from_slice(&[m.offset, m.width, m.height, 0]);
        }
        words.push(levels);
        words.push(count);
        words.push(patch_r as u32);
        words.push(search_range as u32);
        words.push(fb_max_px.to_bits());
        words.push(max_disp.to_bits());
        words.push(pt.0.to_bits());
        words.push(pt.1.to_bits());
        pack_u32s(&words)
    }

    fn track_batch_fb(
        &self,
        prev_slot: usize,
        curr_slot: usize,
        queries: &[GpuTrackQuery],
        patch_r: usize,
        fb_max_px: f32,
    ) -> Option<Vec<Option<(f32, f32)>>> {
        if queries.is_empty() {
            return Some(Vec::new());
        }
        let prev = self.slots.get(prev_slot)?.as_ref()?;
        let curr = self.slots.get(curr_slot)?.as_ref()?;
        let levels = prev.levels.min(curr.levels);
        if levels == 0 || prev.meta[0].width != curr.meta[0].width || prev.meta[0].height != curr.meta[0].height {
            return None;
        }

        let q_vecs: Vec<[f32; 4]> = queries.iter().map(|q| [q.pt.0, q.pt.1, q.guess.0, q.guess.1]).collect();
        let q_buf = self.init_buf("lk-queries", &pack_vec4_f32(&q_vecs), wgpu::BufferUsages::STORAGE);
        let out_bytes = (queries.len() * 16) as u64;
        let out_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("lk-results"),
            size: out_bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let ulk = Self::pack_lk_uniform(prev, levels as u32, queries.len() as u32, patch_r as i32, 0, fb_max_px, 0.0, (0.0, 0.0));
        let ulk_buf = self.init_buf("ulk-fb", &ulk, wgpu::BufferUsages::UNIFORM);
        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bg-lk-fb"),
            layout: &self.bgl_lk,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: ulk_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: prev.pyr_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: curr.pyr_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: q_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: out_buf.as_entire_binding() },
            ],
        });
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("tracker-lk-fb") });
        {
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("cs-pyrlk-fb"), timestamp_writes: None });
            pass.set_pipeline(&self.pipe_pyrlk_fb);
            pass.set_bind_group(0, &bg, &[]);
            pass.dispatch_workgroups((queries.len() as u32).div_ceil(64), 1, 1);
        }
        let raw = self.readback_bytes(&out_buf, out_bytes, enc)?;
        let vecs = unpack_vec4_f32(&raw);
        Some(vecs.into_iter().map(|v| (v[2] > 0.0).then_some((v[0], v[1]))).collect())
    }

    fn zncc_search(&self, prev_slot: usize, curr_slot: usize, pt: (f32, f32), patch_r: usize, range_top_px: i32) -> Option<(f32, f32)> {
        let prev = self.slots.get(prev_slot)?.as_ref()?;
        let curr = self.slots.get(curr_slot)?.as_ref()?;
        let levels = prev.levels.min(curr.levels);
        if levels == 0 || range_top_px <= 0 {
            return None;
        }
        let span = (2 * range_top_px + 1) as usize;
        let total = span * span;
        let dummy_q = self.init_buf("zncc-dummy-q", &[0u8; 16], wgpu::BufferUsages::STORAGE);
        let out_bytes = (total * 16) as u64;
        let out_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("zncc-results"),
            size: out_bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let ulk = Self::pack_lk_uniform(prev, levels as u32, total as u32, patch_r as i32, range_top_px, 0.0, 0.0, pt);
        let ulk_buf = self.init_buf("ulk-zncc", &ulk, wgpu::BufferUsages::UNIFORM);
        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bg-zncc"),
            layout: &self.bgl_lk,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: ulk_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: prev.pyr_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: curr.pyr_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: dummy_q.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: out_buf.as_entire_binding() },
            ],
        });
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("tracker-zncc") });
        {
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("cs-zncc"), timestamp_writes: None });
            pass.set_pipeline(&self.pipe_zncc);
            pass.set_bind_group(0, &bg, &[]);
            pass.dispatch_workgroups((total as u32).div_ceil(64), 1, 1);
        }
        let raw = self.readback_bytes(&out_buf, out_bytes, enc)?;
        let vecs = unpack_vec4_f32(&raw);
        let mut best = (0.55f32, None);
        for v in vecs {
            if v[3] > 0.5 && v[0] > best.0 {
                best = (v[0], Some((v[1], v[2])));
            }
        }
        best.1
    }

    fn stereo_disparity_batch(
        &self,
        left_slot: usize,
        right_slot: usize,
        pts: &[(f32, f32)],
        patch_r: usize,
        max_disp: f32,
    ) -> Option<Vec<Option<f32>>> {
        if pts.is_empty() {
            return Some(Vec::new());
        }
        let left = self.slots.get(left_slot)?.as_ref()?;
        let right = self.slots.get(right_slot)?.as_ref()?;
        if left.meta[0].width != right.meta[0].width || left.meta[0].height != right.meta[0].height {
            return None;
        }
        let q_vecs: Vec<[f32; 4]> = pts.iter().map(|&(x, y)| [x, y, 0.0, 0.0]).collect();
        let q_buf = self.init_buf("stereo-pts", &pack_vec4_f32(&q_vecs), wgpu::BufferUsages::STORAGE);
        let out_bytes = (pts.len() * 16) as u64;
        let out_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("stereo-results"),
            size: out_bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let ulk = Self::pack_lk_uniform(left, 1, pts.len() as u32, patch_r as i32, 0, 0.0, max_disp, (0.0, 0.0));
        let ulk_buf = self.init_buf("ulk-stereo", &ulk, wgpu::BufferUsages::UNIFORM);
        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bg-stereo"),
            layout: &self.bgl_lk,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: ulk_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: left.pyr_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: right.pyr_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: q_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: out_buf.as_entire_binding() },
            ],
        });
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("tracker-stereo") });
        {
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("cs-stereo"), timestamp_writes: None });
            pass.set_pipeline(&self.pipe_stereo);
            pass.set_bind_group(0, &bg, &[]);
            pass.dispatch_workgroups((pts.len() as u32).div_ceil(64), 1, 1);
        }
        let raw = self.readback_bytes(&out_buf, out_bytes, enc)?;
        let vecs = unpack_vec4_f32(&raw);
        Some(vecs.into_iter().map(|v| (v[1] > 0.0).then_some(v[0])).collect())
    }
}

impl GpuTrackBackend for GpuTrackAdapter {
    #[allow(clippy::too_many_arguments)]
    fn build_rgba_pyramid(
        &self,
        slot: usize,
        rgba: &[u8],
        src_w: usize,
        src_h: usize,
        crop: (usize, usize, usize, usize),
        dst_size: (usize, usize),
        levels: usize,
    ) -> Option<Gray> {
        let mut eng = engine()?.lock().ok()?;
        let p = eng.build_rgba_pyramid(slot, rgba, src_w, src_h, crop, dst_size, levels, true)?;
        Some(Gray { w: dst_size.0, h: dst_size.1, p })
    }

    fn upload_gray_pyramid(&self, slot: usize, gray: &Gray, levels: usize) -> bool {
        let Some(mtx) = engine() else { return false };
        let Ok(mut eng) = mtx.lock() else { return false };
        eng.build_luma_pyramid(slot, &gray.p, gray.w, gray.h, levels)
    }

    fn detect_corners(&self, slot: usize, roi: (usize, usize, usize, usize), cell: usize) -> Option<Vec<(f32, f32, f32)>> {
        let eng = engine()?.lock().ok()?;
        eng.detect_corners(slot, roi, cell)
    }

    fn track_batch_fb(
        &self,
        prev_slot: usize,
        curr_slot: usize,
        queries: &[GpuTrackQuery],
        patch_radius: i32,
        fb_max_px: f32,
    ) -> Option<Vec<Option<(f32, f32)>>> {
        let eng = engine()?.lock().ok()?;
        eng.track_batch_fb(prev_slot, curr_slot, queries, patch_radius.max(1) as usize, fb_max_px)
    }

    fn coarse_search(
        &self,
        prev_slot: usize,
        curr_slot: usize,
        pt: (f32, f32),
        patch_radius: i32,
        reach_px: f32,
    ) -> Option<(f32, f32)> {
        let eng = engine()?.lock().ok()?;
        let prev = eng.slots.get(prev_slot)?.as_ref()?;
        let curr = eng.slots.get(curr_slot)?.as_ref()?;
        let levels = prev.levels.min(curr.levels);
        if levels == 0 {
            return None;
        }
        let s = (1u32 << (levels - 1)) as f32;
        let range_top = (reach_px / s).ceil().clamp(2.0, 20.0) as i32;
        eng.zncc_search(prev_slot, curr_slot, pt, patch_radius.max(2) as usize, range_top)
    }

    fn stereo_disparity_batch(
        &self,
        left_slot: usize,
        right_slot: usize,
        pts: &[(f32, f32)],
        patch_radius: i32,
        max_disp: f32,
    ) -> Option<Vec<Option<f32>>> {
        let eng = engine()?.lock().ok()?;
        eng.stereo_disparity_batch(left_slot, right_slot, pts, patch_radius.max(2) as usize, max_disp)
    }
}
