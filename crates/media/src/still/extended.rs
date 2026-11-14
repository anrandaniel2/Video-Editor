//! Additional native still readers. SVG never fetches external resources or executes scripts.
use std::io::Cursor;
use image::ImageDecoder;
use super::Raster;
use crate::{MediaError, Result};

fn is_jxl(b: &[u8]) -> bool {
    b.starts_with(&[0xff, 0x0a]) || b.starts_with(&[0, 0, 0, 12, b'J', b'X', b'L', b' ', 13, 10, 135, 10])
}
fn is_svg(name: &str, b: &[u8]) -> bool {
    let name = name.to_ascii_lowercase();
    name.ends_with(".svg") || name.ends_with(".svgz") || b[..b.len().min(4096)].windows(4).any(|w| w == b"<svg")
}
pub(super) fn sniff(name: &str, b: &[u8]) -> bool {
    is_jxl(b) || is_svg(name, b) || b.starts_with(b"qoif") || b.starts_with(b"farbfeld")
        || b.starts_with(b"DDS ") || b.starts_with(&[0, 0, 1, 0])
        || (b.len() > 2 && b[0] == b'P' && (b'1'..=b'7').contains(&b[1]) && b[2].is_ascii_whitespace())
}
pub(super) fn decode(name: &str, b: &[u8], max_pixels: usize) -> Option<Result<Raster>> {
    let error = |e: String| MediaError::Decode(format!("{name}: {e}"));
    let limit = max_pixels.min(16_777_216);
    if is_jxl(b) {
        return Some((|| {
            let mut decoder = jxl_oxide::integration::JxlDecoder::new(Cursor::new(b)).map_err(|e| error(e.to_string()))?;
            let (w, h) = decoder.dimensions();
            if u64::from(w) * u64::from(h) > limit as u64 { return Err(error("JPEG XL exceeds the 16-megapixel decode limit".into())); }
            let mut limits = image::Limits::default();
            limits.max_alloc = Some(256 * 1024 * 1024);
            limits.max_image_width = Some(16_384);
            limits.max_image_height = Some(16_384);
            decoder.set_limits(limits).map_err(|e| error(e.to_string()))?;
            let image = image::DynamicImage::from_decoder(decoder).map_err(|e| error(e.to_string()))?;
            let has_alpha = image.color().has_alpha();
            Ok(Raster { width: w, height: h, rgba: image.to_rgba8().into_raw(), has_alpha })
        })());
    }
    if is_svg(name, b) {
        return Some((|| {
            let mut options = resvg::usvg::Options::default();
            options.image_href_resolver.resolve_string = Box::new(|_, _| None);
            options.fontdb_mut().load_system_fonts();
            let tree = resvg::usvg::Tree::from_data(b, &options).map_err(|e| error(e.to_string()))?;
            let size = tree.size().to_int_size();
            if u64::from(size.width()) * u64::from(size.height()) > limit as u64 {
                return Err(error("SVG exceeds the 16-megapixel rasterization limit".into()));
            }
            let mut pixmap = resvg::tiny_skia::Pixmap::new(size.width(), size.height()).ok_or_else(|| error("SVG canvas allocation failed".into()))?;
            resvg::render(&tree, resvg::tiny_skia::Transform::identity(), &mut pixmap.as_mut());
            let mut rgba = pixmap.take();
            // tiny-skia is premultiplied; the rest of the still importer is straight RGBA.
            for pixel in rgba.chunks_exact_mut(4) {
                let alpha = u32::from(pixel[3]);
                for channel in &mut pixel[..3] {
                    *channel = if alpha == 0 { 0 } else { ((u32::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8 };
                }
            }
            Ok(Raster { width: size.width(), height: size.height(), rgba, has_alpha: true })
        })());
    }
    None // ICO/PNM/QOI/farbfeld/DDS use the image crate's bounded reader.
}
