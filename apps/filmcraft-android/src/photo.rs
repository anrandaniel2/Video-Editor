//! PhotoCraft: the photo editor's pixel pipeline.
//!
//! The Android photo editor keeps the decoded picture and the user's settings in Kotlin and calls
//! these functions for every pixel operation. The live preview renders a reduced copy of the
//! picture and the saved photo renders the full working copy, so both go through exactly the same
//! code and what the user sees is what they save.
//!
//! Buffers are straight (not premultiplied) sRGB RGBA8 at the bridge, matching `drawing_raster`:
//! four bytes a pixel, rows top to bottom, no padding, alpha untouched by the colour pipeline.

use crate::MobileError;
use crate::drawing_raster::byte;
use filmcraft_color::{linear_to_srgb, luma709, srgb_to_linear};

/// The largest picture the bridge processes, in pixels. The app decodes at most this far: a full
/// 8 MP render needs one output buffer besides the source, which keeps a phone's peak reachable.
const MAX_PIXELS: u64 = 8_000_000;

/// How many filters [`photo_render`] understands, including the unfiltered picture (0).
const FILTERS: u32 = 9;

fn validate(pixels: &[u8], width: u32, height: u32) -> Result<(), MobileError> {
    let count = u64::from(width) * u64::from(height);
    if width == 0 || height == 0 || count > MAX_PIXELS || pixels.len() as u64 != count * 4 {
        return Err(MobileError::Command("Invalid photo dimensions or pixel buffer".into()));
    }
    Ok(())
}

fn finite(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

/// How a filter moves the adjustment sliders: none of them means "the unfiltered picture".
#[derive(Clone, Copy, Debug, Default)]
struct Look {
    exposure: f32,
    contrast: f32,
    warmth: f32,
    tint: f32,
    saturation: f32,
    vibrance: f32,
    fade: f32,
    vignette: f32,
    grain: f32,
    mono: f32,
}

/// The filters, in the order the editor's chips show them (0 is the original picture).
fn filter_look(filter: u32) -> Look {
    match filter {
        1 => Look { contrast: 0.18, saturation: 0.14, warmth: 0.05, vignette: 0.10, ..Look::default() },
        2 => Look { warmth: 0.10, contrast: -0.06, fade: 0.12, saturation: -0.08, grain: 0.12, vignette: 0.12, ..Look::default() },
        3 => Look { mono: 1.0, contrast: 0.10, ..Look::default() },
        4 => Look { mono: 1.0, contrast: 0.30, vignette: 0.30, grain: 0.18, ..Look::default() },
        5 => Look { warmth: 0.24, saturation: 0.05, ..Look::default() },
        6 => Look { warmth: -0.22, tint: 0.04, ..Look::default() },
        7 => Look { fade: 0.28, contrast: -0.12, saturation: -0.14, ..Look::default() },
        8 => Look { saturation: 0.26, vibrance: 0.14, contrast: 0.10, ..Look::default() },
        _ => Look::default(),
    }
}

/// The photo's adjustments, mirroring the editor's sliders. Every value is 0 (no change) unless
/// noted: exposure, contrast, highlights, shadows, whites, blacks, temperature, tint, vibrance,
/// saturation, clarity and vignette run -1…1, sharpen, grain and fade run 0…1, and `filter` is an
/// index into the filters ([`photo_filter_count`]).
#[derive(Clone, Copy, Debug, uniffi::Record)]
pub struct PhotoSettings {
    pub exposure: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub whites: f32,
    pub blacks: f32,
    pub temperature: f32,
    pub tint: f32,
    pub vibrance: f32,
    pub saturation: f32,
    pub clarity: f32,
    pub sharpen: f32,
    pub vignette: f32,
    pub grain: f32,
    pub fade: f32,
    pub filter: u32,
    pub filter_amount: f32,
}

impl PhotoSettings {
    /// The unfiltered picture, exactly as it was decoded.
    fn untouched() -> PhotoSettings {
        PhotoSettings {
            exposure: 0.0,
            contrast: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            whites: 0.0,
            blacks: 0.0,
            temperature: 0.0,
            tint: 0.0,
            vibrance: 0.0,
            saturation: 0.0,
            clarity: 0.0,
            sharpen: 0.0,
            vignette: 0.0,
            grain: 0.0,
            fade: 0.0,
            filter: 0,
            filter_amount: 1.0,
        }
    }
}

/// A crop frame in the turned picture's own coordinates: 0…1, left < right, top < bottom.
#[derive(Clone, Copy, Debug, uniffi::Record)]
pub struct PhotoCrop {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl PhotoCrop {
    /// The whole picture, whatever a bad frame asks for.
    fn clamped(self) -> PhotoCrop {
        let left = finite(self.left, 0.0).clamp(0.0, 1.0);
        let right = finite(self.right, 1.0).clamp(0.0, 1.0);
        let top = finite(self.top, 0.0).clamp(0.0, 1.0);
        let bottom = finite(self.bottom, 1.0).clamp(0.0, 1.0);
        // A frame thinner than a percent would render an empty picture: fall back to the whole one.
        let (left, right) = if right - left >= 0.01 { (left, right) } else { (0.0, 1.0) };
        let (top, bottom) = if bottom - top >= 0.01 { (top, bottom) } else { (0.0, 1.0) };
        PhotoCrop { left, top, right, bottom }
    }

    fn whole(self) -> bool {
        self.left <= 0.0 && self.top <= 0.0 && self.right >= 1.0 && self.bottom >= 1.0
    }
}

/// A picture crossing the bridge: its size and its straight sRGB RGBA8 pixels.
#[derive(Debug, uniffi::Record)]
pub struct PhotoImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// What the editor draws beside the sliders: 256 buckets a channel, plus overall brightness.
#[derive(Debug, uniffi::Record)]
pub struct PhotoHistogram {
    pub red: Vec<u32>,
    pub green: Vec<u32>,
    pub blue: Vec<u32>,
    pub luma: Vec<u32>,
}

/// The settings a freshly opened picture starts with.
#[uniffi::export]
pub fn photo_default_settings() -> PhotoSettings {
    PhotoSettings::untouched()
}

/// The crop frame that keeps the whole picture.
#[uniffi::export]
pub fn photo_full_crop() -> PhotoCrop {
    PhotoCrop { left: 0.0, top: 0.0, right: 1.0, bottom: 1.0 }
}

/// How many filters the editor offers, the original included.
#[uniffi::export]
pub fn photo_filter_count() -> u32 {
    FILTERS
}

/// Turn (a quarter turn clockwise at a time), flip and crop, in that order: flips happen on the
/// picture as decoded, then the quarter turns, then the crop is measured in the turned picture.
#[uniffi::export]
pub fn photo_transform(
    pixels: Vec<u8>,
    width: u32,
    height: u32,
    quarters: u32,
    flip_horizontal: bool,
    flip_vertical: bool,
    crop: PhotoCrop,
) -> Result<PhotoImage, MobileError> {
    validate(&pixels, width, height)?;
    let w = width as usize;
    let h = height as usize;
    let quarters = quarters % 4;
    let turned = if quarters % 2 == 0 { (w, h) } else { (h, w) };

    let mut rotated = vec![0u8; pixels.len()];
    for y in 0..h {
        for x in 0..w {
            let source = (y * w + x) * 4;
            let fx = if flip_horizontal { w - 1 - x } else { x };
            let fy = if flip_vertical { h - 1 - y } else { y };
            let (dest_x, dest_y) = match quarters {
                1 => (h - 1 - fy, fx),
                2 => (w - 1 - fx, h - 1 - fy),
                3 => (fy, w - 1 - fx),
                _ => (fx, fy),
            };
            let dest = (dest_y * turned.0 + dest_x) * 4;
            rotated[dest..dest + 4].copy_from_slice(&pixels[source..source + 4]);
        }
    }

    let crop = crop.clamped();
    if crop.whole() {
        return Ok(PhotoImage { width: turned.0 as u32, height: turned.1 as u32, rgba: rotated });
    }
    let left = ((crop.left * turned.0 as f32).floor() as usize).min(turned.0 - 1);
    let top = ((crop.top * turned.1 as f32).floor() as usize).min(turned.1 - 1);
    let right = ((crop.right * turned.0 as f32).ceil() as usize).clamp(left + 1, turned.0);
    let bottom = ((crop.bottom * turned.1 as f32).ceil() as usize).clamp(top + 1, turned.1);
    let (cw, ch) = (right - left, bottom - top);
    let mut out = vec![0u8; cw * ch * 4];
    for row in 0..ch {
        let from = ((top + row) * turned.0 + left) * 4;
        out[row * cw * 4..(row + 1) * cw * 4].copy_from_slice(&rotated[from..from + cw * 4]);
    }
    Ok(PhotoImage { width: cw as u32, height: ch as u32, rgba: out })
}

/// The per-channel tone curve and the cross-channel work of one render, built once from the
/// settings: three 256-entry lookup tables cover exposure, white balance, contrast and the tone
/// lifts, so those steps cost a table read a channel a pixel.
struct Params {
    lut: [[u8; 256]; 3],
    shadows: f32,
    highlights: f32,
    vibrance: f32,
    saturation: f32,
    mono: f32,
    clarity: f32,
    sharpen: f32,
    vignette: f32,
    grain: f32,
    identity: bool,
}

impl Params {
    fn new(settings: &PhotoSettings) -> Params {
        let amount = finite(settings.filter_amount, 1.0).clamp(0.0, 1.0);
        let look = if settings.filter == 0 || amount <= 0.0 { Look::default() } else { filter_look(settings.filter) };
        let exposure = (finite(settings.exposure, 0.0).clamp(-1.5, 1.5) + look.exposure * amount).clamp(-2.0, 2.0);
        let contrast = (finite(settings.contrast, 0.0).clamp(-1.0, 1.0) + look.contrast * amount).clamp(-1.0, 1.0);
        let warmth = (finite(settings.temperature, 0.0).clamp(-1.0, 1.0) + look.warmth * amount).clamp(-1.0, 1.0);
        let tint = (finite(settings.tint, 0.0).clamp(-1.0, 1.0) + look.tint * amount).clamp(-1.0, 1.0);
        let blacks = finite(settings.blacks, 0.0).clamp(-1.0, 1.0);
        let whites = finite(settings.whites, 0.0).clamp(-1.0, 1.0);
        let fade = (finite(settings.fade, 0.0).clamp(0.0, 1.0) + look.fade * amount).clamp(0.0, 1.0);
        let shadows = finite(settings.shadows, 0.0).clamp(-1.0, 1.0);
        let highlights = finite(settings.highlights, 0.0).clamp(-1.0, 1.0);
        let vibrance = (finite(settings.vibrance, 0.0).clamp(-1.0, 1.0) + look.vibrance * amount).clamp(-1.0, 1.0);
        let saturation = (finite(settings.saturation, 0.0).clamp(-1.0, 1.0) + look.saturation * amount).clamp(-1.0, 1.0);
        let mono = (look.mono * amount).clamp(0.0, 1.0);
        let clarity = finite(settings.clarity, 0.0).clamp(-1.0, 1.0);
        let sharpen = finite(settings.sharpen, 0.0).clamp(0.0, 1.0);
        let vignette = (finite(settings.vignette, 0.0).clamp(-1.0, 1.0) + look.vignette * amount).clamp(-1.0, 1.0);
        let grain = (finite(settings.grain, 0.0).clamp(0.0, 1.0) + look.grain * amount).clamp(0.0, 1.0);

        let gain = 2.0f32.powf(exposure * 1.5);
        let factor = 1.0 + contrast * 0.85;
        // Positive tint is magenta: red and blue up, green down. Warmth is the blue/orange axis.
        let channels = [1.0 + warmth * 0.16 + tint * 0.04, 1.0 - tint * 0.10, 1.0 - warmth * 0.16 + tint * 0.04];
        let mut lut = [[0u8; 256]; 3];
        for (channel, table) in lut.iter_mut().enumerate() {
            let balance = channels[channel];
            for (value, slot) in table.iter_mut().enumerate() {
                let mut v = value as f32 / 255.0;
                v = linear_to_srgb(srgb_to_linear(v) * gain);
                v = (v * balance).clamp(0.0, 1.0);
                v = (v - 0.5) * factor + 0.5;
                v += blacks * 0.10 * (1.0 - v) * (1.0 - v);
                v += whites * 0.10 * v * v;
                v = v * (1.0 - 0.30 * fade) + 0.14 * fade;
                *slot = byte(v * 255.0);
            }
        }

        let identity = exposure == 0.0
            && contrast == 0.0
            && warmth == 0.0
            && tint == 0.0
            && blacks == 0.0
            && whites == 0.0
            && fade == 0.0
            && shadows == 0.0
            && highlights == 0.0
            && vibrance == 0.0
            && saturation == 0.0
            && mono == 0.0
            && clarity == 0.0
            && sharpen == 0.0
            && vignette == 0.0
            && grain == 0.0;

        Params { lut, shadows, highlights, vibrance, saturation, mono, clarity, sharpen, vignette, grain, identity }
    }

    fn spatial(&self) -> bool {
        self.clarity != 0.0 || self.sharpen != 0.0 || self.vignette != 0.0 || self.grain != 0.0
    }

    /// Tone and colour, in place: the lookup tables, then highlights/shadows, then chroma.
    fn tone(&self, pixels: &mut [u8]) {
        for pixel in pixels.chunks_exact_mut(4) {
            let mut rgb = [
                f32::from(self.lut[0][usize::from(pixel[0])]) / 255.0,
                f32::from(self.lut[1][usize::from(pixel[1])]) / 255.0,
                f32::from(self.lut[2][usize::from(pixel[2])]) / 255.0,
            ];
            if self.shadows != 0.0 || self.highlights != 0.0 {
                let luma = luma709(rgb[0], rgb[1], rgb[2]).clamp(0.0, 1.0);
                let lift = self.shadows * (1.0 - luma) * (1.0 - luma) * 0.55 + self.highlights * luma * luma * 0.55;
                for channel in &mut rgb {
                    *channel = (*channel + lift).clamp(0.0, 1.0);
                }
            }
            if self.saturation != 0.0 || self.vibrance != 0.0 || self.mono != 0.0 {
                let luma = luma709(rgb[0], rgb[1], rgb[2]);
                let highest = rgb[0].max(rgb[1]).max(rgb[2]);
                let lowest = rgb[0].min(rgb[1]).min(rgb[2]);
                let chroma = (highest - lowest).clamp(0.0, 1.0);
                let boost = self.vibrance * (1.0 - chroma) * 0.6;
                let factor = (1.0 + self.saturation + boost).max(0.0);
                for channel in &mut rgb {
                    *channel = (luma + (*channel - luma) * factor).clamp(0.0, 1.0);
                }
                if self.mono > 0.0 {
                    let grey = luma709(rgb[0], rgb[1], rgb[2]);
                    for channel in &mut rgb {
                        *channel += (grey - *channel) * self.mono;
                    }
                }
            }
            for channel in 0..3 {
                pixel[channel] = byte(rgb[channel] * 255.0);
            }
        }
    }

    /// The neighbourhood work: clarity and sharpening, then the vignette and the grain.
    fn texture(&self, source: &[u8], width: usize, height: usize) -> Vec<u8> {
        let radius = (width.min(height) / 140).clamp(2, 14).min((width.min(height).saturating_sub(1) / 2).max(1));
        let blur = if self.clarity != 0.0 { Some(luma_blur(source, width, height, radius)) } else { None };
        let mut out = vec![0u8; source.len()];
        let centre_x = (width as f32 - 1.0) * 0.5;
        let centre_y = (height as f32 - 1.0) * 0.5;
        let reach = (centre_x * centre_x + centre_y * centre_y).sqrt().max(1.0);
        let mut noise = 0x9e37_79b9_7f4a_7c15u64;
        for y in 0..height {
            let up = y.saturating_sub(1);
            let down = (y + 1).min(height - 1);
            for x in 0..width {
                let index = (y * width + x) * 4;
                let left = x.saturating_sub(1);
                let right = (x + 1).min(width - 1);
                let dx = (x as f32 - centre_x) / reach;
                let dy = (y as f32 - centre_y) / reach;
                let falloff = if self.vignette != 0.0 { 1.0 - self.vignette * (dx * dx + dy * dy) * 0.9 } else { 1.0 };
                noise = noise.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
                let grain = if self.grain != 0.0 { ((noise >> 40) as f32 / 16_777_216.0 - 0.5) * self.grain * 26.0 } else { 0.0 };
                for channel in 0..3 {
                    let mut value = f32::from(source[index + channel]);
                    if let Some(blur) = &blur {
                        let mean = f32::from(blur[y * width + x]);
                        value += (value - mean) * self.clarity * 0.9;
                    }
                    if self.sharpen != 0.0 {
                        let around = (f32::from(source[(y * width + left) * 4 + channel])
                            + f32::from(source[(y * width + right) * 4 + channel])
                            + f32::from(source[(up * width + x) * 4 + channel])
                            + f32::from(source[(down * width + x) * 4 + channel]))
                            * 0.25;
                        value += (value - around) * self.sharpen * 1.6;
                    }
                    out[index + channel] = byte(value * falloff + grain);
                }
                out[index + 3] = source[index + 3];
            }
        }
        out
    }
}

/// A box blur of the picture's brightness, separable and clamped at the edges: the local average
/// that clarity subtracts to bring out mid-scale contrast.
fn luma_blur(source: &[u8], width: usize, height: usize, radius: usize) -> Vec<u8> {
    let mut luma = vec![0u8; width * height];
    for (index, pixel) in source.chunks_exact(4).enumerate() {
        luma[index] = byte(luma709(f32::from(pixel[0]), f32::from(pixel[1]), f32::from(pixel[2])));
    }
    let window = (2 * radius + 1) as u32;
    let span = radius as isize;
    let mut horizontal = vec![0u8; width * height];
    for y in 0..height {
        let row = y * width;
        let mut sum: u32 = 0;
        for offset in -span..=span {
            sum += u32::from(luma[row + offset.clamp(0, width as isize - 1) as usize]);
        }
        for x in 0..width {
            horizontal[row + x] = (sum / window) as u8;
            let leaving = (x as isize - span).clamp(0, width as isize - 1) as usize;
            let entering = (x as isize + span + 1).clamp(0, width as isize - 1) as usize;
            sum = sum - u32::from(luma[row + leaving]) + u32::from(luma[row + entering]);
        }
    }
    let mut out = vec![0u8; width * height];
    for x in 0..width {
        let mut sum: u32 = 0;
        for offset in -span..=span {
            sum += u32::from(horizontal[offset.clamp(0, height as isize - 1) as usize * width + x]);
        }
        for y in 0..height {
            out[y * width + x] = (sum / window) as u8;
            let leaving = (y as isize - span).clamp(0, height as isize - 1) as usize;
            let entering = (y as isize + span + 1).clamp(0, height as isize - 1) as usize;
            sum = sum - u32::from(horizontal[leaving * width + x]) + u32::from(horizontal[entering * width + x]);
        }
    }
    out
}

/// Develop the picture: the adjustments and the chosen filter, alpha preserved.
#[uniffi::export]
pub fn photo_render(pixels: Vec<u8>, width: u32, height: u32, settings: PhotoSettings) -> Result<Vec<u8>, MobileError> {
    validate(&pixels, width, height)?;
    let params = Params::new(&settings);
    if params.identity {
        return Ok(pixels);
    }
    let mut pixels = pixels;
    params.tone(&mut pixels);
    if params.spatial() {
        Ok(params.texture(&pixels, width as usize, height as usize))
    } else {
        Ok(pixels)
    }
}

/// A histogram of the picture as it stands, for the editor's tone display.
#[uniffi::export]
pub fn photo_histogram(pixels: Vec<u8>, width: u32, height: u32) -> Result<PhotoHistogram, MobileError> {
    validate(&pixels, width, height)?;
    let mut histogram = PhotoHistogram {
        red: vec![0; 256],
        green: vec![0; 256],
        blue: vec![0; 256],
        luma: vec![0; 256],
    };
    for pixel in pixels.chunks_exact(4) {
        histogram.red[usize::from(pixel[0])] += 1;
        histogram.green[usize::from(pixel[1])] += 1;
        histogram.blue[usize::from(pixel[2])] += 1;
        let luma = byte(luma709(f32::from(pixel[0]), f32::from(pixel[1]), f32::from(pixel[2])));
        histogram.luma[usize::from(luma)] += 1;
    }
    Ok(histogram)
}

/// Where a fraction of the picture's brightness falls: the 5th percentile is its shadows, the 95th
/// its highlights.
fn percentile(histogram: &[u32], total: u32, fraction: f32) -> f32 {
    let target = (total as f64 * f64::from(fraction)).round() as u32;
    let mut running = 0u32;
    for (value, count) in histogram.iter().enumerate() {
        running += count;
        if running >= target.max(1) {
            return value as f32 / 255.0;
        }
    }
    1.0
}

/// Settings that even out an exposure and a colour cast: what the Auto button applies.
#[uniffi::export]
pub fn photo_auto_settings(pixels: Vec<u8>, width: u32, height: u32) -> Result<PhotoSettings, MobileError> {
    validate(&pixels, width, height)?;
    let count = width as usize * height as usize;
    let step = (count / 250_000).max(1);
    let mut histogram = [0u32; 256];
    let mut sums = [0f64; 3];
    let mut samples = 0u32;
    let mut index = 0;
    while index < count {
        let offset = index * 4;
        let r = f32::from(pixels[offset]);
        let g = f32::from(pixels[offset + 1]);
        let b = f32::from(pixels[offset + 2]);
        histogram[usize::from(byte(luma709(r, g, b)))] += 1;
        sums[0] += f64::from(r);
        sums[1] += f64::from(g);
        sums[2] += f64::from(b);
        samples += 1;
        index += step;
    }
    if samples == 0 {
        return Ok(PhotoSettings::untouched());
    }
    let mean = (0..256).map(|value| value as f64 * f64::from(histogram[value])).sum::<f64>() / f64::from(samples) / 255.0;
    let dark = percentile(&histogram, samples, 0.05);
    let bright = percentile(&histogram, samples, 0.95);
    let mean_of = |channel: usize| (sums[channel] / f64::from(samples) / 255.0) as f32;
    let cast = mean_of(2) - mean_of(0);

    let settings = PhotoSettings {
        exposure: ((0.46 / mean.max(0.03)).log2() * 0.7).clamp(-0.8, 0.8),
        contrast: ((0.72 - f64::from(bright - dark)) as f32 * 1.6).clamp(-0.35, 0.55),
        highlights: 0.0,
        shadows: 0.0,
        whites: ((0.95 - bright) * 3.0).clamp(-0.35, 0.35),
        blacks: ((0.05 - dark) * 3.0).clamp(-0.35, 0.35),
        temperature: (cast * 1.1).clamp(-0.35, 0.35),
        tint: 0.0,
        vibrance: 0.0,
        saturation: 0.0,
        clarity: 0.0,
        sharpen: 0.0,
        vignette: 0.0,
        grain: 0.0,
        fade: 0.0,
        filter: 0,
        filter_amount: 1.0,
    };
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture(width: u32, height: u32, colors: &[[u8; 4]]) -> Vec<u8> {
        let mut pixels = Vec::with_capacity(colors.len() * 4);
        for color in colors {
            pixels.extend_from_slice(color);
        }
        assert_eq!(pixels.len(), width as usize * height as usize * 4);
        pixels
    }

    fn settings(exposure: f32) -> PhotoSettings {
        let mut s = PhotoSettings::untouched();
        s.exposure = exposure;
        s
    }

    #[test]
    fn the_untouched_picture_is_exactly_unchanged() -> Result<(), MobileError> {
        let source = picture(2, 1, &[[10, 20, 30, 255], [250, 128, 0, 128]]);
        let out = photo_render(source.clone(), 2, 1, PhotoSettings::untouched())?;
        assert_eq!(out, source);
        Ok(())
    }

    #[test]
    fn exposure_brightens_darks_and_keeps_alpha() -> Result<(), MobileError> {
        let source = picture(1, 1, &[[40, 40, 40, 77]]);
        let out = photo_render(source, 1, 1, settings(0.8))?;
        assert!(out[0] > 40 && out[1] > 40 && out[2] > 40);
        assert_eq!(out[3], 77);
        let dark = photo_render(picture(1, 1, &[[200, 200, 200, 255]]), 1, 1, settings(-0.8))?;
        assert!(dark[0] < 200);
        Ok(())
    }

    #[test]
    fn full_desaturation_leaves_grey_pixels() -> Result<(), MobileError> {
        let mut s = PhotoSettings::untouched();
        s.saturation = -1.0;
        let out = photo_render(picture(2, 1, &[[200, 30, 10, 255], [10, 180, 50, 255]]), 2, 1, s)?;
        for pixel in out.chunks_exact(4) {
            assert!(pixel[0].abs_diff(pixel[1]) <= 2 && pixel[1].abs_diff(pixel[2]) <= 2);
        }
        Ok(())
    }

    #[test]
    fn the_mono_filter_removes_colour_and_the_noir_one_adds_contrast() -> Result<(), MobileError> {
        let source = picture(3, 1, &[[220, 20, 20, 255], [128, 128, 128, 255], [20, 20, 220, 255]]);
        let mut s = PhotoSettings::untouched();
        s.filter = 3;
        let mono = photo_render(source.clone(), 3, 1, s)?;
        for pixel in mono.chunks_exact(4) {
            assert!(pixel[0].abs_diff(pixel[1]) <= 2 && pixel[1].abs_diff(pixel[2]) <= 2);
        }
        s.filter = 4;
        let noir = photo_render(source, 3, 1, s)?;
        assert_ne!(mono, noir);
        Ok(())
    }

    #[test]
    fn a_quarter_turn_swaps_the_sides_and_moves_the_corners() -> Result<(), MobileError> {
        // 2x1: red | green. A clockwise quarter turn makes it 1x2 with red on top.
        let source = picture(2, 1, &[[255, 0, 0, 255], [0, 255, 0, 255]]);
        let turned = photo_transform(source.clone(), 2, 1, 1, false, false, photo_full_crop())?;
        assert_eq!((turned.width, turned.height), (1, 2));
        assert_eq!(&turned.rgba[0..4], &[255, 0, 0, 255]);
        assert_eq!(&turned.rgba[4..8], &[0, 255, 0, 255]);
        let full = photo_transform(source, 2, 1, 4, false, false, photo_full_crop())?;
        assert_eq!((full.width, full.height), (2, 1));
        Ok(())
    }

    #[test]
    fn flips_mirror_both_axes() -> Result<(), MobileError> {
        let source = picture(2, 2, &[[1, 0, 0, 255], [2, 0, 0, 255], [3, 0, 0, 255], [4, 0, 0, 255]]);
        let horizontal = photo_transform(source.clone(), 2, 2, 0, true, false, photo_full_crop())?;
        assert_eq!(horizontal.rgba[0], 2);
        assert_eq!(horizontal.rgba[4], 1);
        let vertical = photo_transform(source, 2, 2, 0, false, true, photo_full_crop())?;
        assert_eq!(vertical.rgba[0], 3);
        Ok(())
    }

    #[test]
    fn the_crop_selects_the_frame_in_the_turned_picture() -> Result<(), MobileError> {
        let source = picture(4, 1, &[[1, 0, 0, 255], [2, 0, 0, 255], [3, 0, 0, 255], [4, 0, 0, 255]]);
        let crop = PhotoCrop { left: 0.5, top: 0.0, right: 1.0, bottom: 1.0 };
        let out = photo_transform(source, 4, 1, 0, false, false, crop)?;
        assert_eq!((out.width, out.height), (2, 1));
        assert_eq!(out.rgba[0], 3);
        // Nonsense frames fall back to the whole picture rather than rendering nothing.
        let empty = photo_transform(vec![0; 16], 2, 2, 0, false, false, PhotoCrop { left: 0.7, top: 0.0, right: 0.7, bottom: 1.0 })?;
        assert_eq!((empty.width, empty.height), (2, 2));
        Ok(())
    }

    #[test]
    fn the_histogram_counts_every_pixel_and_channel() -> Result<(), MobileError> {
        let source = picture(2, 2, &[[0, 255, 0, 255], [0, 255, 0, 255], [255, 255, 255, 255], [0, 0, 0, 255]]);
        let histogram = photo_histogram(source, 2, 2)?;
        assert_eq!(histogram.red.iter().sum::<u32>(), 4);
        assert_eq!(histogram.red[0], 3);
        assert_eq!(histogram.green[255], 3);
        assert_eq!(histogram.blue[0], 3);
        assert_eq!(histogram.luma.iter().sum::<u32>(), 4);
        Ok(())
    }

    #[test]
    fn auto_settings_even_out_a_dark_picture_and_stay_in_range() -> Result<(), MobileError> {
        let source = picture(4, 1, &[[20, 18, 25, 255], [35, 30, 40, 255], [8, 6, 12, 255], [28, 24, 30, 255]]);
        let auto = photo_auto_settings(source, 4, 1)?;
        assert!(auto.exposure > 0.1);
        assert!(auto.exposure <= 0.8 && auto.contrast.abs() <= 0.55 && auto.temperature.abs() <= 0.35);
        Ok(())
    }

    #[test]
    fn vignette_darkens_the_corners_more_than_the_centre() -> Result<(), MobileError> {
        let source = vec![200u8; 9 * 9 * 4];
        let mut s = PhotoSettings::untouched();
        s.vignette = 0.8;
        let out = photo_render(source, 9, 9, s)?;
        let centre = (4 * 9 + 4) * 4;
        let corner = 0;
        assert!(out[corner] < out[centre]);
        Ok(())
    }

    #[test]
    fn grain_is_deterministic_and_bounded() -> Result<(), MobileError> {
        let source = vec![128u8; 32 * 32 * 4];
        let mut s = PhotoSettings::untouched();
        s.grain = 0.5;
        let a = photo_render(source.clone(), 32, 32, s)?;
        let b = photo_render(source, 32, 32, s)?;
        assert_eq!(a, b);
        assert!(a.chunks_exact(4).all(|p| p[0] >= 100 && p[0] <= 156));
        Ok(())
    }

    #[test]
    fn impossible_buffers_are_refused() {
        assert!(photo_render(vec![0; 15], 2, 2, PhotoSettings::untouched()).is_err());
        assert!(photo_render(vec![0; 4], 0, 1, PhotoSettings::untouched()).is_err());
        assert!(photo_transform(vec![0; 4], 1, 1, 0, false, false, photo_full_crop()).is_ok());
        assert!(photo_histogram(vec![0; 3], 1, 1).is_err());
    }

    #[test]
    fn filters_number_the_original_and_all_chips() {
        assert_eq!(photo_filter_count(), FILTERS);
        assert!(filter_look(0).mono == 0.0 && filter_look(0).contrast == 0.0);
        for filter in 1..FILTERS {
            let look = filter_look(filter);
            let touched = look.exposure != 0.0
                || look.contrast != 0.0
                || look.warmth != 0.0
                || look.tint != 0.0
                || look.saturation != 0.0
                || look.vibrance != 0.0
                || look.fade != 0.0
                || look.vignette != 0.0
                || look.grain != 0.0
                || look.mono != 0.0;
            assert!(touched, "filter {filter} does nothing");
        }
    }
}
