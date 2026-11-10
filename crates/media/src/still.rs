//! Still images (PNG, JPEG, GIF, WebP, TIFF, BMP, Targa, DPX, OpenEXR, Radiance HDR, DNG camera raw) as media
//! sources. Animated PNG and GIF files open as movies instead ([`crate::animated`]); here they would
//! show their first frame.

use std::sync::Arc;

use filmcraft_frame::VideoFrame;
use filmcraft_time::{FrameRate, TICKS_PER_SECOND, Tick};

use crate::{FrameRequest, MediaError, MediaInfo, MediaKind, MediaSource, Result, VideoStreamInfo};

pub mod dng;
pub mod exr;
pub mod hdr;
mod ycbcr_tiff;

/// Default still duration (Premiere's preference default is 5 seconds).
pub const DEFAULT_STILL_DURATION: Tick = Tick(5 * TICKS_PER_SECOND);

/// Whether `b` (the contents of the file `name`) is a still image we decode.
pub fn sniff(name: &str, b: &[u8]) -> bool {
    crate::dpx::sniff(b)
        || hdr::sniff(b)
        || crate::tga::sniff(name, b)
        || b.starts_with(b"\x89PNG")
        || b.starts_with(&[0xFF, 0xD8, 0xFF])
        || b.starts_with(b"GIF8")
        || (b.len() > 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP")
        || b.starts_with(b"II*\0")
        || b.starts_with(b"MM\0*")
        || b.starts_with(b"BM")
}

/// A decoded picture: straight-alpha RGBA8, top row first.
pub struct Raster {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// Whether any pixel is not fully opaque.
    pub has_alpha: bool,
}

/// Decode a still image file (`name` picks Targa, which has no magic number; the content picks
/// every other format).
pub fn decode_raster(name: &str, bytes: &[u8]) -> Result<Raster> {
    decode_raster_with_limit(name, bytes, usize::MAX)
}

/// Decode a raster with a pixel-count ceiling, for embedded assets whose dimensions come from
/// an untrusted file. The image reader's allocation limit is applied before decoding, not after.
pub fn decode_raster_with_limit(name: &str, bytes: &[u8], max_pixels: usize) -> Result<Raster> {
    let check = |w: u32, h: u32| {
        if (w as usize).saturating_mul(h as usize) > max_pixels {
            Err(MediaError::Decode(format!("{name}: image exceeds the {max_pixels} pixel limit")))
        } else {
            Ok(())
        }
    };
    if crate::dpx::sniff(bytes) {
        let r = crate::dpx::decode(name, bytes)?;
        check(r.width, r.height)?;
        return Ok(r);
    }
    if crate::tga::sniff(name, bytes) {
        let r = crate::tga::decode(name, bytes)?;
        check(r.width, r.height)?;
        return Ok(r);
    }
    if ycbcr_tiff::sniff(bytes) {
        let r = ycbcr_tiff::decode(name, bytes)?;
        check(r.width, r.height)?;
        return Ok(r);
    }
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| MediaError::Decode(format!("{name}: {e}")))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16_384);
    limits.max_image_height = Some(16_384);
    limits.max_alloc = Some(max_pixels.saturating_mul(8).min(512 * 1024 * 1024) as u64);
    reader.limits(limits);
    let img = reader.decode().map_err(|e| MediaError::Decode(format!("{name}: {e}")))?;
    let (width, height) = img.dimensions();
    check(width, height)?;
    let has_alpha = img.color().has_alpha();
    let rgba = img.to_rgba8();
    Ok(Raster { width, height, rgba: rgba.into_raw(), has_alpha })
}

pub struct StillSource {
    info: MediaInfo,
    frame: Arc<VideoFrame>,
}

impl StillSource {
    pub fn decode(name: &str, bytes: &[u8]) -> Result<Self> {
        // a DNG is a TIFF too: develop its raw image instead of reading its preview
        if dng::sniff(bytes) {
            return dng::still(name, bytes);
        }
        if hdr::sniff(bytes) {
            return hdr::still(name, bytes);
        }
        let r = decode_raster(name, bytes)?;
        Ok(Self::from_rgba(name, r.width, r.height, r.rgba, r.has_alpha, bytes.len() as u64))
    }

    pub fn from_rgba(name: &str, w: u32, h: u32, rgba: Vec<u8>, has_alpha: bool, file_size: u64) -> Self {
        let codec = name.rsplit('.').next().unwrap_or("image").to_ascii_uppercase();
        let info = MediaInfo {
            name: name.to_string(),
            kind: MediaKind::Still,
            duration: DEFAULT_STILL_DURATION,
            video: Some(VideoStreamInfo {
                width: w,
                height: h,
                frame_rate: FrameRate::FPS_29_97,
                par: (1, 1),
                codec,
                pixel_format: if has_alpha { "RGBA 8-bit".into() } else { "RGB 8-bit".into() },
                color: filmcraft_color::ColorInfo::SRGB_FULL,
                has_alpha,
                bitrate: None,
                hdr: None,
            }),
            audio_streams: Vec::new(),
            container: "Image".into(),
            start_timecode: None,
            file_size: Some(file_size),
        };
        Self { info, frame: Arc::new(VideoFrame::rgba8(w, h, rgba)) }
    }
}

impl MediaSource for StillSource {
    fn info(&self) -> &MediaInfo {
        &self.info
    }
    fn video_frame(&self, _req: FrameRequest) -> Result<Arc<VideoFrame>> {
        Ok(self.frame.clone())
    }
    fn audio(&self, _start: i64, _frames: usize, _sample_rate: u32) -> Result<filmcraft_frame::AudioBuffer> {
        Err(MediaError::NoStream("audio"))
    }
}
