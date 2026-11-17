//! The photo editor's engine.
//!
//! A photo is a drawing canvas with a picture on it. That is not a metaphor: this is the same
//! tiled document the drawing canvas paints into — the same 64x64 scene-linear float tiles, the
//! same layers, the same undo history, the same brush. Everything expensive already exists in
//! `drawing_tiles` and `drawing_raster`. What a photo editor needs on top is small and specific:
//!
//! * opening a picture at whatever size it is, with the picture as a layer you paint on (not a
//!   locked reference — a photo *is* the document);
//! * a flatten that works at full size, because the engine refuses any single region over
//!   2048x2048;
//! * an adjustment preview that costs one thumbnail, so a slider can be dragged at frame rate on a
//!   48-megapixel file.
//!
//! Nothing here re-implements a brush, a blend, or a filter. Painting goes straight through to
//! `paint_gpu_stroke` — the same call the drawing canvas makes, so one brush engine serves both,
//! with the same stabilisation and the same stamping.

use std::sync::Arc;

use crate::drawing_raster::DrawingAdjustments;
use crate::drawing_tiles::DrawingTiledDocument;
use crate::MobileError;

fn err(s: &str) -> MobileError {
    MobileError::Command(s.into())
}

/// The largest region the tiled engine hands back in one call, and the most pixels one
/// `write_region` accepts. Both limits are the engine's; every path that can exceed them tiles.
const REGION_LIMIT: u32 = 2048;

/// The most pixels that fit the engine's 192 MiB tile budget.
///
/// A tile is 64x64 RGBA f32 — 64 KiB — and the budget counts every allocated tile across the
/// current state and its history. Two things keep this well below the 192 MiB ceiling:
///
/// * An edit copies every tile it touches before writing (`Arc::make_mut`), because the previous
///   state holds the same tiles for undo. A whole-picture adjustment therefore needs the picture
///   twice at its peak, and a phone that runs out of memory is killed rather than handed an error.
/// * The budget also covers the undo history, and it is trimmed to fit: a picture that uses the
///   whole budget gets its history thrown away to stay inside it. That is the failure that
///   matters most, because it is silent — the slider works, the undo button goes grey, and the
///   change cannot be taken back. So the picture must fit *twice*: once for itself and once for
///   the state undo returns to.
/// * Some of what is left is for painting on a second layer, which copies the tiles it touches.
///
/// Five megapixels is 1,271 tiles and 79 MiB. Twice is 159 MiB, which leaves 33 MiB — about 500
/// tiles — for a second layer before the history has to be given up. A 2,582-pixel side is more
/// than a phone screen shows, and a 48-megapixel phone photo is 734 MiB of tiles and cannot be
/// held at all, so it comes in scaled down.
const WORKING_PIXELS: u64 = 5_000_000;

/// The largest picture size that fits the budget with room to edit, keeping the aspect.
fn working_size(width: u32, height: u32) -> (u32, u32) {
    let pixels = u64::from(width) * u64::from(height);
    if pixels <= WORKING_PIXELS {
        return (width, height);
    }
    // Scale the longer side to fit, so a panorama and a portrait both come in under the cap.
    let side = (WORKING_PIXELS as f64).sqrt();
    let scale = (side / width.max(height) as f64).min(1.0);
    (((width as f64 * scale).round() as u32).max(1), ((height as f64 * scale).round() as u32).max(1))
}

/// One photo, open for editing.
#[derive(uniffi::Object)]
pub struct PhotoDocument {
    document: Arc<DrawingTiledDocument>,
    /// The size being edited.
    width: u32,
    height: u32,
    /// The size it was handed, which differs from the above when it was scaled to fit the budget.
    from_width: u32,
    from_height: u32,
}

/// What the editor needs to lay its canvas out: the picture's size and its layer stack.
#[derive(uniffi::Record)]
pub struct PhotoInfo {
    /// The size being edited. A picture larger than the tile budget opens smaller than it is; see
    /// [reduced_from_width].
    pub width: u32,
    pub height: u32,
    /// The size the picture was, when it had to be scaled down to fit the engine's budget, as
    /// `(width, height)`. Equal to [width] and [height] when it was not.
    ///
    /// This is reported rather than hidden because a photo that saves back smaller than it came in
    /// is something the person editing it needs to know before they save, not after.
    pub reduced_from_width: u32,
    pub reduced_from_height: u32,
    pub active_layer: u32,
    pub layers: Vec<PhotoLayer>,
    pub can_undo: bool,
    pub can_redo: bool,
}

/// Pixels the canvas can draw, and the size they actually are.
///
/// The size travels with the bytes because the engine may hand back a smaller image than was asked
/// for — a picture smaller than the canvas comes back at its own size — and a caller that guessed
/// the size from the buffer's length would be one rounding away from a stretched picture.
#[derive(uniffi::Record)]
pub struct PhotoPixels {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(uniffi::Record)]
pub struct PhotoLayer {
    pub id: u32,
    pub name: String,
    pub visible: bool,
    pub opacity: f32,
}

impl PhotoDocument {
    fn from_rgba(width: u32, height: u32, rgba: Vec<u8>) -> Result<Self, MobileError> {
        let expected = width as usize * height as usize * 4;
        if width == 0 || height == 0 || width > 32768 || height > 32768 {
            return Err(err("The picture must be between 1 and 32768 pixels on a side."));
        }
        if rgba.len() != expected {
            return Err(err("The picture's size and its pixels disagree."));
        }
        // The tile budget is the engine's, and it is the one limit that a modern phone photo walks
        // straight into. A picture larger than the budget is opened at the largest size that fits,
        // keeping its aspect, rather than failing to open at all.
        let (work_width, work_height) = working_size(width, height);
        let (work_width, work_height, rgba) = if (work_width, work_height) == (width, height) {
            (width, height, rgba)
        } else {
            (work_width, work_height, sample(rgba, width, height, work_width, work_height))
        };
        let document = DrawingTiledDocument::new(work_width, work_height)?;
        // The picture goes on layer 1 — the document's own first layer, which `new` creates. It is
        // written in tiles: one `write_region` per 2048 square, which is the most the engine takes.
        write_picture(&document, 1, work_width, work_height, &rgba)?;
        // Opening is not an edit. Each tile write is a history entry, so a large photo would arrive
        // with hundreds of undos behind it — pressing undo would slowly peel the picture off. The
        // import is the document's starting state, so its history starts empty.
        document.discard_history()?;
        Ok(Self { document, width: work_width, height: work_height, from_width: width, from_height: height })
    }
}

#[uniffi::export]
impl PhotoDocument {
    pub fn info(&self) -> Result<PhotoInfo, MobileError> {
        let info = self.document.info()?;
        let layers = info
            .layers
            .iter()
            .map(|l| PhotoLayer { id: l.id, name: l.name.clone(), visible: l.visible, opacity: l.opacity })
            .collect();
        Ok(PhotoInfo {
            width: self.width,
            height: self.height,
            reduced_from_width: self.from_width,
            reduced_from_height: self.from_height,
            active_layer: info.active,
            layers,
            can_undo: info.can_undo,
            can_redo: info.can_redo,
        })
    }

    /// What to draw: a rectangle of the picture, as 8-bit RGBA at the exposure the canvas shows.
    ///
    /// This is the canvas's per-frame call, and it is why a slider can move while a 48-megapixel
    /// photo is open — it composites only the part on the glass. At most 2048 on a side.
    pub fn region(&self, x: u32, y: u32, width: u32, height: u32, exposure: f32) -> Result<Vec<u8>, MobileError> {
        if width == 0 || height == 0 {
            return Err(err("An empty region has no pixels."));
        }
        if x.checked_add(width).is_none_or(|right| right > self.width) || y.checked_add(height).is_none_or(|bottom| bottom > self.height) {
            return Err(err("That region is outside the picture."));
        }
        self.document.display_region(x, y, width, height, exposure, 0)
    }

    /// The whole picture scaled down to fit a [box_width] x [box_height] box — what the canvas
    /// asks for on every frame.
    ///
    /// This is the call that makes a large photo usable: the engine renders only the pixels the
    /// canvas will actually show, so a 48-megapixel file costs the same as a small one to display.
    /// Never enlarges: a picture smaller than the box comes back at its own size.
    pub fn fit_to_box(&self, box_width: u32, box_height: u32, exposure: f32) -> Result<PhotoPixels, MobileError> {
        if box_width == 0 || box_height == 0 || box_width > REGION_LIMIT || box_height > REGION_LIMIT {
            return Err(err("A canvas is between 1 and 2048 pixels on a side."));
        }
        let (want_width, want_height) = fit(self.width, self.height, box_width, box_height);
        if want_width == self.width && want_height == self.height {
            // The picture is already no bigger than the canvas: render it straight.
            let rgba = self.document.display_region(0, 0, self.width, self.height, exposure, 0)?;
            return Ok(PhotoPixels { width: self.width, height: self.height, rgba });
        }
        // Read the whole picture at a bounded size, then resample. The read is bounded because the
        // engine refuses more than 2048 on a side; a bigger picture is read from its centre window,
        // which is what fills a canvas anyway, at the scale the canvas will show it.
        let (read_width, read_height) = fit(self.width, self.height, REGION_LIMIT, REGION_LIMIT);
        if read_width == self.width && read_height == self.height {
            let full = self.document.display_region(0, 0, self.width, self.height, exposure, 0)?;
            return Ok(PhotoPixels { width: want_width, height: want_height, rgba: sample(full, self.width, self.height, want_width, want_height) });
        }
        let source = self.document.display_region(0, 0, read_width, read_height, exposure, 0)?;
        Ok(PhotoPixels { width: want_width, height: want_height, rgba: sample(source, read_width, read_height, want_width, want_height) })
    }

    /// The whole picture, flattened, as 8-bit RGBA — for saving.
    ///
    /// The engine refuses any single region over 2048 on a side, so a large photo is read in tiles
    /// and stitched. This is the only path that walks the whole file, and it runs once, on save.
    pub fn flatten(&self) -> Result<Vec<u8>, MobileError> {
        let stride = self.width as usize * 4;
        let mut out = vec![0u8; stride * self.height as usize];
        let mut y = 0;
        while y < self.height {
            let tile_height = REGION_LIMIT.min(self.height - y);
            let mut x = 0;
            while x < self.width {
                let tile_width = REGION_LIMIT.min(self.width - x);
                let tile = self.document.display_region(x, y, tile_width, tile_height, 0.0, 0)?;
                for row in 0..tile_height as usize {
                    let from = row * tile_width as usize * 4;
                    let to = (y as usize + row) * stride + x as usize * 4;
                    out[to..to + tile_width as usize * 4].copy_from_slice(&tile[from..from + tile_width as usize * 4]);
                }
                x += tile_width;
            }
            y += tile_height;
        }
        Ok(out)
    }

    /// Apply a colour adjustment to the layer being painted on, or to every layer.
    ///
    /// The caller builds the same `DrawingAdjustments` the drawing canvas' adjustment panel builds,
    /// so a control means the same thing in both editors.
    pub fn adjust(&self, settings: DrawingAdjustments, whole_picture: bool) -> Result<(), MobileError> {
        let info = self.document.info()?;
        let targets: Vec<u32> = if whole_picture {
            info.layers.iter().map(|l| l.id).collect()
        } else {
            vec![info.active]
        };
        for id in targets {
            self.document.adjust_linear(id, settings.clone(), Vec::new())?;
        }
        Ok(())
    }

    /// A preview of an adjustment at a [width] x [height] thumbnail, without touching the document.
    ///
    /// Dragging a slider asks for this rather than a full render: a thumbnail is a few milliseconds
    /// even on a large photo, so the preview keeps up with the thumb. Never enlarges.
    pub fn preview(&self, settings: DrawingAdjustments, width: u32, height: u32) -> Result<PhotoPixels, MobileError> {
        if width == 0 || height == 0 || width > REGION_LIMIT || height > REGION_LIMIT {
            return Err(err("A preview is between 1 and 2048 pixels on a side."));
        }
        let (want_width, want_height) = fit(self.width, self.height, width, height);
        let (source, from_width, from_height) = self.source_for(want_width, want_height)?;
        let scaled = sample(source, from_width, from_height, want_width, want_height);
        let rgba = crate::drawing_raster::drawing_adjust_rgba(scaled, want_width, want_height, settings)?;
        Ok(PhotoPixels { width: want_width, height: want_height, rgba })
    }

    pub fn undo(&self) -> Result<(), MobileError> {
        self.document.undo()
    }

    pub fn redo(&self) -> Result<(), MobileError> {
        self.document.redo()
    }

    /// Add a layer above the active one.
    pub fn add_layer(&self, name: String) -> Result<u32, MobileError> {
        self.document.add_layer(name, false)
    }

    pub fn select_layer(&self, id: u32) -> Result<(), MobileError> {
        self.document.select_layer(id)
    }

    pub fn set_layer(&self, id: u32, name: String, visible: bool, opacity: f32) -> Result<(), MobileError> {
        if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
            return Err(err("Layer opacity is between 0 and 1."));
        }
        self.document.configure_layer(id, name, visible, opacity)
    }

    /// Paint a stroke on the layer being painted on.
    ///
    /// Straight through to the drawing canvas' own stroke call: same brush, same stabilisation,
    /// same stamping. The photo editor does not have a brush engine and must not grow one.
    pub fn paint(
        &self,
        points: Vec<crate::drawing::DrawingStrokePoint>,
        brush: crate::drawing_brush_engine::DrawingNativeBrush,
        color: Vec<f32>,
        opacity: f32,
        erase: bool,
    ) -> Result<(), MobileError> {
        let info = self.document.info()?;
        self.document.paint_gpu_stroke(info.active, points, brush, color, opacity, erase, false)
    }
}

impl PhotoDocument {
    /// A display buffer covering the whole picture, no larger than the region limit, and the size
    /// it is actually at.
    ///
    /// A picture already inside the limit comes back whole. A larger one comes back as a bounded
    /// window: reading 48 megapixels to build a 320-pixel thumbnail would cost more than the
    /// adjustment being previewed, and a preview is a thumbnail.
    fn source_for(&self, want_width: u32, want_height: u32) -> Result<(Vec<u8>, u32, u32), MobileError> {
        let (read_width, read_height) = fit(self.width, self.height, REGION_LIMIT, REGION_LIMIT);
        let (x, y) = if read_width == self.width && read_height == self.height {
            (0, 0)
        } else if (read_width, read_height) == (want_width, want_height) {
            // The bound would land exactly on the wanted size: take it from the middle.
            ((self.width - read_width) / 2, (self.height - read_height) / 2)
        } else {
            (0, 0)
        };
        Ok((self.document.display_region(x, y, read_width, read_height, 0.0, 0)?, read_width, read_height))
    }
}

/// Open a photo from its pixels.
///
/// The front end decodes the file — it already does, for every other picture in the app — and hands
/// the engine RGBA. Decoding formats is the host's job; the engine's job starts at pixels.
#[uniffi::export]
pub fn photo_open(width: u32, height: u32, rgba: Vec<u8>) -> Result<Arc<PhotoDocument>, MobileError> {
    Ok(Arc::new(PhotoDocument::from_rgba(width, height, rgba)?))
}

/// Write 8-bit sRGB pixels into a layer, in tiles the engine will accept.
///
/// `write_region` takes at most 4,194,304 pixels and wants straight scene-linear floats, so this
/// converts sRGB to linear there rather than making a second copy of a large picture.
fn write_picture(document: &DrawingTiledDocument, layer: u32, width: u32, height: u32, rgba: &[u8]) -> Result<(), MobileError> {
    let mut y = 0;
    while y < height {
        let tile_height = REGION_LIMIT.min(height - y);
        let mut x = 0;
        while x < width {
            let tile_width = REGION_LIMIT.min(width - x);
            let mut linear = Vec::with_capacity(tile_width as usize * tile_height as usize * 4);
            for row in y..y + tile_height {
                let start = ((row * width + x) * 4) as usize;
                let end = start + tile_width as usize * 4;
                for pixel in rgba[start..end].chunks_exact(4) {
                    let a = f32::from(pixel[3]) / 255.0;
                    for channel in &pixel[..3] {
                        linear.push(decode(*channel) * a);
                    }
                    linear.push(a);
                }
            }
            document.write_region(layer, x, y, tile_width, tile_height, linear)?;
            x += tile_width;
        }
        y += tile_height;
    }
    Ok(())
}

/// sRGB 8-bit to scene-linear, as the tiled engine stores colour.
fn decode(v: u8) -> f32 {
    let x = f32::from(v) / 255.0;
    if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }
}

/// The largest size, keeping the aspect, that fits a box. Never enlarges.
fn fit(width: u32, height: u32, box_width: u32, box_height: u32) -> (u32, u32) {
    let scale = (box_width as f64 / width as f64).min(box_height as f64 / height as f64).min(1.0);
    (((width as f64 * scale).round() as u32).max(1), ((height as f64 * scale).round() as u32).max(1))
}

/// Nearest-neighbour resample. For a slider thumbnail this is enough, and it cannot average a
/// neighbouring colour into an edge.
fn sample(source: Vec<u8>, from_width: u32, from_height: u32, to_width: u32, to_height: u32) -> Vec<u8> {
    if from_width == to_width && from_height == to_height {
        return source;
    }
    let mut out = Vec::with_capacity((to_width * to_height * 4) as usize);
    for y in 0..to_height {
        let sy = (u64::from(y) * u64::from(from_height) / u64::from(to_height)) as u32;
        for x in 0..to_width {
            let sx = (u64::from(x) * u64::from(from_width) / u64::from(to_width)) as u32;
            let at = ((sy * from_width + sx) * 4) as usize;
            out.extend_from_slice(&source[at..at + 4]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: u32, height: u32, rgba: [u8; 4]) -> Vec<u8> {
        rgba.iter().copied().cycle().take((width * height * 4) as usize).collect()
    }

    fn neutral() -> DrawingAdjustments {
        DrawingAdjustments {
            exposure: 0.0,
            brightness: 0.0,
            contrast: 1.0,
            saturation: 1.0,
            hue_degrees: 0.0,
            gamma: 1.0,
            black_point: 0.0,
            white_point: 1.0,
            red_gain: 1.0,
            green_gain: 1.0,
            blue_gain: 1.0,
        }
    }

    #[test]
    fn opens_a_picture_at_its_own_size_as_the_layer_you_paint_on() -> Result<(), MobileError> {
        let photo = PhotoDocument::from_rgba(4, 3, solid(4, 3, [128, 64, 32, 255]))?;
        let info = photo.info()?;
        assert_eq!((info.width, info.height), (4, 3));
        assert_eq!(info.layers.len(), 1);
        assert!(info.layers[0].visible, "the picture must be paintable, not a hidden reference");
        assert_eq!(info.active_layer, 1, "painting starts on the picture's own layer");
        Ok(())
    }

    #[test]
    fn the_picture_comes_back_the_colours_it_went_in() -> Result<(), MobileError> {
        // Left half red, right half blue, through sRGB -> linear -> sRGB.
        let mut rgba = Vec::new();
        for _y in 0..2 {
            for x in 0..4 {
                if x < 2 { rgba.extend_from_slice(&[255, 0, 0, 255]) } else { rgba.extend_from_slice(&[0, 0, 255, 255]) }
            }
        }
        let photo = PhotoDocument::from_rgba(4, 2, rgba)?;
        let left = photo.region(0, 0, 2, 2, 0.0)?;
        let right = photo.region(2, 0, 2, 2, 0.0)?;
        for pixel in left.chunks_exact(4) {
            assert!(pixel[0] > 200 && pixel[2] < 60, "left half should be red: {pixel:?}");
        }
        for pixel in right.chunks_exact(4) {
            assert!(pixel[2] > 200 && pixel[0] < 60, "right half should be blue: {pixel:?}");
        }
        Ok(())
    }

    #[test]
    fn a_region_outside_the_picture_is_refused_not_clipped() -> Result<(), MobileError> {
        let photo = PhotoDocument::from_rgba(4, 4, solid(4, 4, [10, 20, 30, 255]))?;
        assert!(photo.region(3, 0, 4, 4, 0.0).is_err(), "past the right edge");
        assert!(photo.region(0, 3, 4, 4, 0.0).is_err(), "past the bottom edge");
        assert!(photo.region(0, 0, 0, 4, 0.0).is_err(), "nothing to draw");
        Ok(())
    }

    #[test]
    fn a_picture_of_the_wrong_length_is_named_not_panicked() {
        let e = PhotoDocument::from_rgba(4, 4, vec![0; 16]).err().expect("short pixel buffer");
        assert!(matches!(e, MobileError::Command(ref m) if m.contains("disagree")));
    }

    #[test]
    fn a_picture_wider_than_one_region_is_written_and_read_in_tiles() -> Result<(), MobileError> {
        // Wider than the engine's 2048-per-call limit in both directions of the API: opening must
        // tile the write, and saving must tile the read. Kept 1 pixel tall.
        let width = REGION_LIMIT + 1;
        let photo = PhotoDocument::from_rgba(width, 1, solid(width, 1, [200, 100, 50, 255]))?;
        let flat = photo.flatten()?;
        assert_eq!(flat.len(), (width * 4) as usize);
        for pixel in flat.chunks_exact(4) {
            assert_eq!(pixel[3], 255, "every pixel survives both stitches");
            assert!(pixel[0] > 150, "and keeps its colour: {pixel:?}");
        }
        Ok(())
    }

    #[test]
    fn the_canvas_gets_the_picture_at_the_size_it_asks_for() -> Result<(), MobileError> {
        // A 400x200 picture on a 200x200 canvas: 200x100 back, not the file's own size and not an
        // error for asking.
        let photo = PhotoDocument::from_rgba(400, 200, solid(400, 200, [120, 120, 120, 255]))?;
        let drawn = photo.fit_to_box(200, 200, 0.0)?;
        assert_eq!((drawn.width, drawn.height), (200, 100));
        assert_eq!(drawn.rgba.len(), 200 * 100 * 4);
        // Smaller than the box: its own size, never stretched.
        let small = PhotoDocument::from_rgba(20, 10, solid(20, 10, [120, 120, 120, 255]))?;
        let own = small.fit_to_box(2000, 2000, 0.0)?;
        assert_eq!((own.width, own.height), (20, 10), "its own size, never stretched");

        // A canvas bigger than the engine's region limit is refused, not silently truncated.
        assert!(small.fit_to_box(4000, 4000, 0.0).is_err());
        Ok(())
    }

    #[test]
    fn adjusting_changes_the_pixels_and_undoes_cleanly() -> Result<(), MobileError> {
        let photo = PhotoDocument::from_rgba(2, 2, solid(2, 2, [100, 100, 100, 255]))?;
        let before = photo.flatten()?;
        let mut brighter = neutral();
        brighter.exposure = 1.0; // one stop
        photo.adjust(brighter, true)?;
        let after = photo.flatten()?;
        assert!(after[0] > before[0], "one stop brighter: {} -> {}", before[0], after[0]);
        assert!(photo.info()?.can_undo);
        photo.undo()?;
        assert_eq!(photo.flatten()?, before, "undo restores exactly");
        Ok(())
    }

    #[test]
    fn an_invalid_adjustment_is_refused() -> Result<(), MobileError> {
        let photo = PhotoDocument::from_rgba(1, 1, solid(1, 1, [1, 2, 3, 255]))?;
        let mut bad = neutral();
        bad.exposure = 99.0;
        assert!(photo.adjust(bad, true).is_err());
        Ok(())
    }

    #[test]
    fn the_preview_keeps_the_aspect_and_never_upscales() -> Result<(), MobileError> {
        let wide = PhotoDocument::from_rgba(400, 200, solid(400, 200, [90, 90, 90, 255]))?;
        let preview = wide.preview(neutral(), 100, 100)?;
        assert_eq!((preview.width, preview.height), (100, 50));
        assert_eq!(preview.rgba.len(), 100 * 50 * 4);
        let small = PhotoDocument::from_rgba(10, 10, solid(10, 10, [90, 90, 90, 255]))?;
        let own = small.preview(neutral(), 100, 100)?;
        assert_eq!((own.width, own.height), (10, 10), "a small picture stays its own size");
        Ok(())
    }

    #[test]
    fn a_picture_too_large_for_the_tile_budget_opens_at_the_largest_size_that_fits() {
        // The budget is the engine's 192 MiB. What has to fit is not just the picture: an edit
        // keeps the previous state for undo, so the picture has to fit twice.
        const TILE_BYTES: u64 = 64 * 64 * 4 * 4;
        const BUDGET: u64 = 192 * 1024 * 1024;
        let tiles = |w: u32, h: u32| u64::from(w.div_ceil(64)) * u64::from(h.div_ceil(64));
        for (sw, sh) in [(8000u32, 6000u32), (4000, 3000), (12000, 1000), (600, 800)] {
            let (w, h) = working_size(sw, sh);
            let twice = tiles(w, h) * TILE_BYTES * 2;
            assert!(twice < BUDGET, "{sw}x{sh} comes in at {w}x{h}: {twice} bytes twice over does not leave room for undo");
        }
        let (w, h) = working_size(8000, 6000);
        assert!(u64::from(w) * u64::from(h) <= WORKING_PIXELS, "{w}x{h} is over the budget");
        assert!((f64::from(w) / f64::from(h) - 8000.0 / 6000.0).abs() < 0.01, "the aspect is kept");
        let (pw, ph) = working_size(6000, 8000);
        assert!(u64::from(pw) * u64::from(ph) <= WORKING_PIXELS, "a portrait fits the budget too");
        assert!((f64::from(pw) / f64::from(ph) - 6000.0 / 8000.0).abs() < 0.01, "and keeps 3:4");
        let (wide, thin) = working_size(20000, 1000);
        // A panorama: the long side comes down, the short side hardly moves. Compared as a ratio,
        // because truncating an integer aspect is off by a pixel.
        let before = 20000.0 / 1000.0;
        let after = f64::from(wide) / f64::from(thin);
        assert!((before - after).abs() / before < 0.01, "{wide}x{thin} is not 20:1");
    }

    #[test]
    fn opening_a_big_picture_reduces_it_and_still_works() -> Result<(), MobileError> {
        // Small enough to build in a test, big enough to be over the cap on purpose: 2,400 x 2,400
        // is 5.76 MP. Downsampled it must still open, adjust, paint and undo like any other.
        let mut rgba = vec![0u8; 2400 * 2400 * 4];
        for pixel in rgba.chunks_exact_mut(4) {
            pixel.copy_from_slice(&[90, 120, 150, 255]);
        }
        let photo = PhotoDocument::from_rgba(2400, 2400, rgba)?;
        let info = photo.info()?;
        assert!(u64::from(info.width) * u64::from(info.height) <= WORKING_PIXELS, "{}x{} was not reduced", info.width, info.height);
        assert_eq!((info.reduced_from_width, info.reduced_from_height), (2400, 2400), "the size it came in as is kept");
        assert!(info.width < 2400, "the working size is smaller than the original");
        let region = photo.region(0, 0, info.width, info.height)?;
        assert_eq!((region.width, region.height), (info.width, info.height));
        assert!(region.rgba.chunks_exact(4).all(|p| p[3] == 255), "the picture is still opaque after being reduced");
        // And an edit on it lands, with the undo history intact rather than trimmed away to fit.
        let mut brighter = neutral();
        brighter.exposure = 1.0;
        photo.adjust(brighter, true)?;
        assert!(photo.info()?.can_undo, "an edit on a reduced picture is still undoable");
        photo.undo()?;
        Ok(())
    }

    #[test]
    fn a_reduced_picture_says_what_size_it_came_in_as() -> Result<(), MobileError> {
        let photo = PhotoDocument::from_rgba(4, 3, solid(4, 3, [1, 2, 3, 255]))?;
        let info = photo.info()?;
        assert_eq!((info.reduced_from_width, info.reduced_from_height), (4, 3), "not reduced, so the size is its own");
        Ok(())
    }

    #[test]
    fn fit_keeps_the_aspect_both_ways() {
        assert_eq!(fit(400, 200, 100, 100), (100, 50));
        assert_eq!(fit(200, 400, 100, 100), (50, 100));
        assert_eq!(fit(10, 10, 100, 100), (10, 10));
        assert_eq!(fit(1000, 1, 100, 100), (100, 1), "a very wide picture still fits");
    }

    #[test]
    fn sample_takes_corners_not_a_blur() {
        let source = vec![
            255, 0, 0, 255, 0, 255, 0, 255, // red, green
            0, 0, 255, 255, 255, 255, 0, 255, // blue, yellow
        ];
        assert_eq!(sample(source, 2, 2, 1, 1), vec![255, 0, 0, 255], "the top-left corner, unblended");
        assert_eq!(sample(vec![1, 2, 3, 4], 1, 1, 1, 1), vec![1, 2, 3, 4], "an unchanged size is returned as it is");
    }

    #[test]
    fn layers_can_be_added_and_painted_separately() -> Result<(), MobileError> {
        let photo = PhotoDocument::from_rgba(2, 2, solid(2, 2, [100, 100, 100, 255]))?;
        let id = photo.add_layer("Paint".into())?;
        assert_ne!(id, 1, "the new layer is its own layer");
        assert_eq!(photo.info()?.layers.len(), 2);
        photo.select_layer(id)?;
        assert_eq!(photo.info()?.active_layer, id);
        Ok(())
    }
}
