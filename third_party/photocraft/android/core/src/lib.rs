//! The Android bridge to PhotoCraft's engine.
//!
//! This is the vendored PhotoCraft (see `third_party/photocraft/FILMCRAFT-INTEGRATION.md`): the
//! real document, the real layer stack and the real CPU compositor that the desktop app uses, with
//! a Kotlin surface over it. It is deliberately thin — it owns a [`Document`] and answers what a
//! host screen asks — because the panels, tools and dialogs belong to the Kotlin UI and the
//! commands that change the document belong to `photocraft-engine` as they arrive.
//!
//! Buffers cross the bridge as straight-alpha RGBA8, a byte a channel, rows top to bottom, which is
//! Android's `ARGB_8888` memory order: the compositor's float output is written once, here, and the
//! Kotlin side hands it straight to a bitmap.

use std::sync::Arc;

use photocraft_compose::Buffer;
use photocraft_doc::{Document, Layer, LayerContent};
use photocraft_io::ImportOptions;

uniffi::setup_scaffolding!();

/// What went wrong, in words the editor can show.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum PcError {
    /// The file could not be read as a picture.
    #[error("{message}")]
    Open { message: String },
    /// The document could not be composited.
    #[error("{message}")]
    Render { message: String },
}

/// One layer, as the layer panel lists it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct PcLayer {
    /// The layer's name (`Layer 3` when it has none).
    pub name: String,
    /// Pixel, Group, Adjustment, Fill, Type, Shape or Smart Object.
    pub kind: String,
    /// How far inside groups the layer sits: 0 at the root.
    pub depth: u32,
    pub visible: bool,
    /// 0…1.
    pub opacity: f32,
    /// 0…1.
    pub fill_opacity: f32,
    /// The blend mode's name, as Photoshop spells it (`Linear Burn`).
    pub blend: String,
    /// Clipped to the layer below (a clipping mask).
    pub clipped: bool,
    /// Carries a pixel mask, a vector mask, or both.
    pub masked: bool,
}

/// An opened document, without its pixels.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct PcDocument {
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// RGB, CMYK, Lab, Grayscale, Indexed, Bitmap, Multichannel or Duotone.
    pub color_mode: String,
    /// 8, 16 or 32 bits a channel.
    pub bits_per_channel: u32,
    pub resolution_dpi: f32,
    /// Bottom to top, the way the layer panel reads.
    pub layers: Vec<PcLayer>,
}

/// A composited picture: straight sRGB RGBA8, a byte a channel.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct PcPixels {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// One open document: what PhotoCraft's engine decoded, ready to be drawn and edited.
#[derive(uniffi::Object)]
pub struct PcSession {
    document: Document,
    /// Notes about anything the import approximated or dropped, in the order it found them.
    warnings: Vec<String>,
}

#[uniffi::export]
impl PcSession {
    /// Open a document from the bytes of any format PhotoCraft reads: PSD and PSB, layered TIFF,
    /// PDN, OpenRaster, XCF, KRA, SVG, PNG, JPEG, TIFF, WebP, GIF, BMP, TGA, ICO, QOI, PNM, EXR,
    /// HDR, FITS, camera raw and the native `.pcraft` bundle. `name` carries the extension; the
    /// bytes are what get decoded.
    #[uniffi::constructor]
    pub fn open(name: String, bytes: Vec<u8>) -> Result<Arc<PcSession>, PcError> {
        let imported = photocraft_io::import_with_options(&name, &bytes, &ImportOptions::default())
            .map_err(|error| PcError::Open { message: error.to_string() })?;
        Ok(Arc::new(PcSession { document: imported.document, warnings: imported.warnings }))
    }

    /// The document's size, mode, depth and layer stack.
    pub fn document(&self) -> PcDocument {
        PcDocument {
            name: self.document.name.clone(),
            width: self.document.size.width,
            height: self.document.size.height,
            color_mode: format!("{:?}", self.document.mode),
            bits_per_channel: self.document.depth.bits() as u32,
            resolution_dpi: self.document.resolution_dpi,
            layers: layers_bottom_to_top(&self.document),
        }
    }

    /// What the import approximated or dropped.
    pub fn warnings(&self) -> Vec<String> {
        self.warnings.clone()
    }

    /// How many layers the document holds, groups included.
    pub fn layer_count(&self) -> u32 {
        self.document.layers.len() as u32
    }

    /// Composite the document, longest side at most `max_side`, as straight sRGB RGBA8.
    ///
    /// The composite runs in bands of the document's own rows and each band is box-filtered down to
    /// the asked size as it arrives, so a 50 MP document previews without a 50 MP float buffer ever
    /// existing — the difference between a phone that draws it and one that is killed for memory.
    pub fn render(&self, max_side: u32) -> Result<PcPixels, PcError> {
        let rect = self.document.bounds();
        let (width, height) = (rect.width(), rect.height());
        if width == 0 || height == 0 {
            return Err(PcError::Render { message: "This document has no pixels to draw.".to_owned() });
        }
        let mut out = Downsampler::new(width, height, max_side);
        // A band of float RGBA stays around 32 MB, whatever the document's width.
        let band_rows = (2_000_000u32 / width).max(1).min(height);
        photocraft_compose::render_bands(&self.document, rect, band_rows as i32, |band| -> Result<(), PcError> {
            out.band(&band);
            Ok(())
        })?;
        out.finish();
        Ok(out.into_pixels())
    }
}

/// The version of the vendored engine, for the About screen and for reports.
#[uniffi::export]
pub fn pc_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// The document's layers, groups expanded in place, bottom to top — the layer panel's order.
fn layers_bottom_to_top(document: &Document) -> Vec<PcLayer> {
    fn walk(layers: &[Layer], depth: u32, out: &mut Vec<PcLayer>) {
        for layer in layers {
            out.push(PcLayer {
                name: layer.name.clone(),
                kind: layer.content.kind_name().to_owned(),
                depth,
                visible: layer.visible,
                opacity: layer.opacity,
                fill_opacity: layer.fill_opacity,
                blend: blend_name(layer),
                clipped: layer.clipped,
                masked: layer.mask.is_some() || layer.vector_mask.is_some(),
            });
            if let LayerContent::Group(group) = &layer.content {
                walk(&group.children, depth + 1, out);
            }
        }
    }
    let mut out = Vec::with_capacity(document.layers.len());
    walk(&document.layers, 0, &mut out);
    out
}

/// `LinearBurn` as `Linear Burn`: the names the panel shows.
fn blend_name(layer: &Layer) -> String {
    let raw = format!("{:?}", layer.blend);
    let mut out = String::with_capacity(raw.len() + 4);
    for (index, ch) in raw.chars().enumerate() {
        if ch.is_uppercase() && index > 0 {
            out.push(' ');
        }
        out.push(ch);
    }
    out
}

/// One composite, band by band, into the asked size: a box filter over the source pixels each
/// output pixel covers.
struct Downsampler {
    width: u32,
    height: u32,
    target_width: u32,
    target_height: u32,
    /// The output row being accumulated.
    row: u32,
    /// The source rows gathered so far for `row`.
    rows: u32,
    /// Per output column, RGBA sums over the source pixels assigned to that output row.
    sum: Vec<f32>,
    rgba: Vec<u8>,
}

impl Downsampler {
    fn new(width: u32, height: u32, max_side: u32) -> Downsampler {
        let longest = width.max(height);
        let scale = (f64::from(max_side.max(1)) / f64::from(longest)).min(1.0);
        let target_width = ((f64::from(width) * scale).round() as u32).max(1);
        let target_height = ((f64::from(height) * scale).round() as u32).max(1);
        Downsampler {
            width,
            height,
            target_width,
            target_height,
            row: 0,
            rows: 0,
            sum: vec![0.0; target_width as usize * 4],
            rgba: vec![0u8; target_width as usize * target_height as usize * 4],
        }
    }

    /// Add one band of the composite, in document coordinates.
    fn band(&mut self, band: &Buffer) {
        let band_width = band.rect.width() as usize;
        let band_height = band.rect.height() as usize;
        if band_width == 0 || band_height == 0 {
            return;
        }
        for local in 0..band_height {
            let source_row = band.rect.y0 as i64 + local as i64;
            if source_row < 0 {
                continue;
            }
            let target_row = (u64::from(source_row as u32) * u64::from(self.target_height) / u64::from(self.height)) as u32;
            // Bands arrive bottom-row first within their own extent, in ascending document order.
            if target_row < self.row {
                continue;
            }
            while self.row < target_row {
                self.flush_row();
            }
            self.add_row(&band.px[local * band_width..(local + 1) * band_width]);
        }
    }

    /// The last rows, which no later band will finish.
    fn finish(&mut self) {
        while self.row < self.target_height {
            self.flush_row();
        }
    }

    /// Average one source row into the running sums, box-filtered horizontally.
    fn add_row(&mut self, pixels: &[[f32; 4]]) {
        let source_width = pixels.len() as u32;
        if source_width == 0 || source_width != self.width {
            return;
        }
        for column in 0..self.target_width {
            let from = (u64::from(column) * u64::from(self.width) / u64::from(self.target_width)) as usize;
            let to = ((u64::from(column + 1) * u64::from(self.width) / u64::from(self.target_width)) as usize).max(from + 1).min(pixels.len());
            let count = (to - from) as f32;
            let mut sums = [0.0f32; 4];
            for pixel in &pixels[from..to] {
                for (channel, value) in pixel.iter().enumerate() {
                    sums[channel] += value;
                }
            }
            let at = column as usize * 4;
            for (channel, value) in sums.iter().enumerate() {
                self.sum[at + channel] += value / count;
            }
        }
        self.rows += 1;
    }

    /// Write the accumulated row out and start the next one.
    fn flush_row(&mut self) {
        if self.row < self.target_height && self.rows > 0 {
            let weight = self.rows as f32;
            let start = self.row as usize * self.target_width as usize * 4;
            for (index, value) in self.sum.iter().enumerate() {
                self.rgba[start + index] = byte(value / weight);
            }
        }
        self.sum.fill(0.0);
        self.rows = 0;
        self.row += 1;
    }

    fn into_pixels(self) -> PcPixels {
        PcPixels { width: self.target_width, height: self.target_height, rgba: self.rgba }
    }
}

/// 0…1 to a byte, rounded rather than truncated: the compositor's float output is already sRGB.
fn byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}
