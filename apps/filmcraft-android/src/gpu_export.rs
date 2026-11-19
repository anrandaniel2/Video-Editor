//! The GPU compositor for exports, registered at startup.
//!
//! Frames composite on the phone's GPU (Vulkan) through `filmcraft-gpu`'s WGSL compositor — the
//! same plan and the same maths as the CPU reference renderer. On an effects-heavy edit this is
//! what keeps export off the CPU's cores. The CPU is not a fallback: `export::start` asks for the
//! GPU only when the device has one (and says so when it has not), and a GPU that fails mid-export
//! fails the export with its reason.

use filmcraft_export::FrameRenderer;
use filmcraft_project::{ItemId, Project};
use filmcraft_render::{Image, RenderOptions, SourceProvider};
use filmcraft_time::Tick;

/// One export worker's GPU compositor (the export builds a small pool of them).
struct GpuFrames(filmcraft_gpu::ExportRenderer);

impl FrameRenderer for GpuFrames {
    fn render(&mut self, project: &Project, seq: ItemId, t: Tick, opts: RenderOptions, sources: &dyn SourceProvider) -> Option<Image> {
        self.0.render(project, seq, t, opts, sources)
    }
}

/// Register the GPU frame renderer once. A device without an adapter offers nothing, and the
/// export that asks for the GPU then says the device has none rather than rendering on the CPU.
pub(crate) fn register() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        filmcraft_gpu::scene3d::register();
        filmcraft_export::register_frame_renderer(|| {
            let r = filmcraft_gpu::ExportRenderer::new()?;
            Some(Box::new(GpuFrames(r)) as Box<dyn FrameRenderer>)
        });
    });
}
