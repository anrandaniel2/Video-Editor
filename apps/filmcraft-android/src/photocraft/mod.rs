//! The photo engine the restored PhotoCraft studio drives.
//!
//! PhotoCraft's own Rust engine was a very large thing (1,332 files: a full document model, PSD
//! I/O, effects, filters, smart objects). The studio UI needs a specific, small part of that: the
//! 31 commands it actually sends, a layer tree to list, a composite to draw, and undo. This module
//! is that part, written for FilmCraft, and nothing else — no command exists here that the
//! interface does not call.
//!
//! The shape is PhotoCraft's because the UI is: [`PhotoSession`] holds one or more documents,
//! [`PhotoSession::execute`] runs a command id, and [`PhotoSession::render`] hands back the
//! composite. The Compose editor never sees the document model.
//!
//! Why a `photocraft` module rather than reusing `drawing_tiles`: the studio's model is a *tree* of
//! layers with groups, blend modes, adjustment layers and a selection mask, and it is addressed by
//! command ids. The drawing canvas's tiled document is a flat layer stack addressed by method. They
//! are different models, and bending one into the other would make both worse.

pub mod commands;
pub mod compose;
pub mod model;
pub mod paint;
pub mod select;
pub mod text;

use std::sync::Mutex;

use serde_json::{Value, json};

use model::{Document, Kind, LayerStyle};

/// What went wrong, as Kotlin shows it. Flat, so the bindings raise one exception with a message.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum PhotoError {
    #[error("{0}")]
    Command(String),
    #[error("{0}")]
    Import(String),
    #[error("{0}")]
    Export(String),
    #[error("{0}")]
    Render(String),
}

/// A composited image: 8-bit RGBA, row-major.
#[derive(uniffi::Record)]
pub struct PhotoFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// One open document, as the studio's document switcher lists it.
#[derive(uniffi::Record)]
pub struct PhotoDocumentInfo {
    pub index: u32,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub dirty: bool,
}

/// The session's state, so the UI can tell what changed.
#[derive(uniffi::Record)]
pub struct PhotoStatus {
    /// Bumped on every change; the canvas re-renders when it moves.
    pub revision: u64,
    pub dirty: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub undo_label: String,
    pub redo_label: String,
    pub active: Option<PhotoDocumentInfo>,
    pub documents: Vec<PhotoDocumentInfo>,
    pub active_layer: Option<u64>,
}

/// The tool state every paint command reads: what the panels' sliders set.
pub struct Tools {
    pub brush_size: f64,
    pub brush_hardness: f64,
    pub brush_opacity: f64,
    pub brush_preset: String,
    /// The preset's own tip, so picking a brush changes what it does. Hardness alone cannot say
    /// what charcoal is: these carry the grain that breaks the edge up and the wetness that makes
    /// a stroke build rather than lay flat.
    pub brush_grain: f64,
    pub brush_wet: f64,
    pub foreground: [u8; 4],
    pub background: [u8; 4],
}

impl Default for Tools {
    fn default() -> Self {
        Self {
            brush_size: 40.0,
            brush_hardness: 0.85,
            brush_opacity: 1.0,
            // A clean, hard, ungrainned tip until a preset is chosen: grain is the texture a
            // brush adds, not something a plain stroke should have to opt out of.
            brush_preset: "Fine Ink".into(),
            brush_grain: 0.0,
            brush_wet: 0.0,
            foreground: [0, 0, 0, 255],
            background: [255, 255, 255, 255],
        }
    }
}

/// One open document and its history.
pub struct OpenDocument {
    pub doc: Document,
    pub active_layer: Option<u64>,
    pub selected: Vec<u64>,
    /// Snapshots for undo, oldest first. Each is a whole document: the model is small enough that
    /// a snapshot is cheaper than tracking every edit's inverse, and far less error-prone.
    pub past: Vec<Snapshot>,
    pub future: Vec<Snapshot>,
    /// The gesture name of the undo step on top, when it was recorded as coalescing.
    coalescing: Option<String>,
    pub revision: u64,
    /// The revision the last save (or open) happened at; `dirty` compares against it.
    pub saved_revision: u64,
}

/// One undoable state.
pub struct Snapshot {
    pub doc: Document,
    pub active_layer: Option<u64>,
    pub label: String,
}

/// How many undo steps a document keeps.
///
/// Each snapshot is a clone of the tree, and a layer's pixels are behind an `Option<Vec<u8>>` that
/// is only copied when it changes, so an undeleted layer costs nothing to snapshot. 32 steps is
/// what every editor offers and what the History panel shows.
const HISTORY: usize = 32;

impl OpenDocument {
    pub fn new(doc: Document) -> Self {
        let active = doc.layers().last().map(|l| l.id);
        Self {
            doc,
            active_layer: active,
            selected: active.into_iter().collect(),
            past: Vec::new(),
            future: Vec::new(),
            // Equal, so a freshly opened or created picture is clean. A nonzero start would show
            // the unsaved-changes dot the moment the editor opened.
            revision: 1,
            saved_revision: 1,
            coalescing: None,
        }
    }

    /// Record an undo step under `label`.
    ///
    /// This must be called **before** the document is changed — the snapshot is the state to
    /// return to. A call after the edit would snapshot the already-changed document, and undo
    /// would appear to do nothing. (That is exactly what the first version of this engine did;
    /// [`crate::photo`]'s history tests are what catch it.)
    pub fn push_history(&mut self, label: &str) {
        self.push_history_coalescing(label, None);
    }

    /// Record an undo step, merging it into the previous one when `coalesce` names it.
    ///
    /// The adjustment panels re-send their whole value on every slider tick, so without this a
    /// single drag of the levels slider would leave forty history rows. The coalescing key is the
    /// panel's own name for the gesture (`photocraft.levels`); the same key twice in a row is one
    /// drag, and any other edit in between starts a new step.
    pub fn push_history_coalescing(&mut self, label: &str, coalesce: Option<&str>) {
        if let Some(key) = coalesce {
            if self.coalescing.as_deref() == Some(key) && !self.past.is_empty() {
                // Same gesture: the snapshot already on top is the state before the drag began,
                // which is exactly what undo should return to. Keeping it means one step.
                self.future.clear();
                return;
            }
        }
        self.past.push(Snapshot { doc: self.doc.clone(), active_layer: self.active_layer, label: label.to_string() });
        if self.past.len() > HISTORY {
            self.past.remove(0);
        }
        self.coalescing = coalesce.map(str::to_string);
        self.future.clear();
    }

    /// The document changed.
    pub fn touch(&mut self) {
        self.revision += 1;
    }

    pub fn is_dirty(&self) -> bool {
        self.revision != self.saved_revision
    }

    /// The History panel's rows, oldest first.
    pub fn history_labels(&self) -> Vec<String> {
        self.past.iter().map(|s| s.label.clone()).collect()
    }

    pub fn undo(&mut self) -> bool {
        let Some(prev) = self.past.pop() else { return false };
        self.future.push(Snapshot { doc: self.doc.clone(), active_layer: self.active_layer, label: prev.label.clone() });
        self.doc = prev.doc;
        self.active_layer = prev.active_layer;
        self.revision += 1;
        // Stepping through history ends any drag in progress: the next slider tick is a new edit.
        self.coalescing = None;
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.future.pop() else { return false };
        self.past.push(Snapshot { doc: self.doc.clone(), active_layer: self.active_layer, label: next.label.clone() });
        self.doc = next.doc;
        self.active_layer = next.active_layer;
        self.revision += 1;
        self.coalescing = None;
        true
    }
}

/// The session the Compose editor drives: one per editor screen.
#[derive(uniffi::Object)]
pub struct PhotoSession {
    inner: Mutex<Session>,
}

/// The unlocked state.
pub struct Session {
    pub documents: Vec<OpenDocument>,
    pub active: Option<usize>,
    pub tools: Tools,
}

impl Session {
    pub fn new() -> Self {
        Self { documents: Vec::new(), active: None, tools: Tools::default() }
    }

    pub fn active(&self) -> Option<&OpenDocument> {
        self.active.and_then(|i| self.documents.get(i))
    }

    pub fn active_mut(&mut self) -> Option<&mut OpenDocument> {
        self.active.and_then(|i| self.documents.get_mut(i))
    }

    /// Open `doc` and make it active; returns its index.
    pub fn push(&mut self, doc: Document) -> u32 {
        self.documents.push(OpenDocument::new(doc));
        (self.documents.len() - 1) as u32
    }

    pub fn select_index(&mut self, index: u32) {
        if (index as usize) < self.documents.len() {
            self.active = Some(index as usize);
        }
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl PhotoSession {
    fn lock(&self) -> std::sync::MutexGuard<'_, Session> {
        // A poisoned mutex means a command panicked mid-edit. The document is either the old state
        // or the new one and both are valid, so recover rather than refusing to work.
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn info(session: &Session, index: usize) -> Option<PhotoDocumentInfo> {
        let d = session.documents.get(index)?;
        Some(PhotoDocumentInfo {
            index: index as u32,
            name: d.doc.name.clone(),
            width: d.doc.width(),
            height: d.doc.height(),
            dirty: d.is_dirty(),
        })
    }
}

#[uniffi::export]
impl PhotoSession {
    /// A fresh session with no documents.
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self { inner: Mutex::new(Session::new()) }
    }

    /// Run one engine command. `params` is a JSON object, or empty for none.
    pub fn execute(&self, id: String, params: String) -> Result<String, PhotoError> {
        let params: Value = if params.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(&params).map_err(|e| PhotoError::Command(format!("bad params for `{id}`: {e}")))?
        };
        let mut s = self.lock();
        let out = commands::execute(&mut s, &id, params).map_err(PhotoError::Command)?;
        Ok(out.to_string())
    }

    /// Whether the command can run right now.
    pub fn is_enabled(&self, id: String) -> bool {
        commands::enabled(&self.lock(), &id).is_ok()
    }

    /// Why the command cannot run, if it cannot.
    pub fn disabled_reason(&self, id: String) -> Option<String> {
        commands::enabled(&self.lock(), &id).err()
    }

    /// The session's state.
    pub fn status(&self) -> PhotoStatus {
        let s = self.lock();
        let documents: Vec<PhotoDocumentInfo> = (0..s.documents.len()).filter_map(|i| Self::info(&s, i)).collect();
        let active_info = s.active.and_then(|i| Self::info(&s, i));
        let (can_undo, can_redo, undo_label, redo_label, revision, dirty, active_layer) = match s.active() {
            Some(d) => (
                !d.past.is_empty(),
                !d.future.is_empty(),
                d.past.last().map(|s| s.label.clone()).unwrap_or_default(),
                d.future.last().map(|s| s.label.clone()).unwrap_or_default(),
                d.revision,
                d.is_dirty(),
                d.active_layer,
            ),
            None => (false, false, String::new(), String::new(), 0, false, None),
        };
        PhotoStatus {
            revision,
            dirty,
            can_undo,
            can_redo,
            undo_label,
            redo_label,
            active: active_info,
            documents,
            active_layer,
        }
    }

    /// Open a picture from its bytes; `name` picks the decoder.
    pub fn open_bytes(&self, name: String, bytes: Vec<u8>, as_ext: Option<String>) -> Result<u32, PhotoError> {
        let hint = as_ext.unwrap_or_else(|| name.clone());
        let (w, h, rgba) = commands::decode_picture(&hint, &bytes).map_err(PhotoError::Import)?;
        let mut doc = Document::new(picture_name(&name), w, h, "none").map_err(PhotoError::Import)?;
        // The picture is the first layer, named for its file.
        if let Some(first) = doc.layers_mut().first_mut() {
            first.name = picture_name(&name);
            first.kind = Kind::Pixels;
            first.content = model::Content::Pixels(Some(std::sync::Arc::new(rgba)));
        }
        let mut s = self.lock();
        let index = s.push(doc);
        s.select_index(index);
        Ok(index)
    }

    /// Place a picture as a new layer on the active document.
    pub fn place_bytes(&self, name: String, bytes: Vec<u8>, _params: String) -> Result<String, PhotoError> {
        let (w, h, rgba) = commands::decode_picture(&name, &bytes).map_err(PhotoError::Import)?;
        let mut s = self.lock();
        let doc = s.active_mut().ok_or_else(|| PhotoError::Import("Open a picture first.".into()))?;
        let id = doc.doc.take_id();
        // Placed at the document's size: the picture is scaled to fit the canvas's box, which is
        // what "place" means when the canvas is already decided.
        let (tw, th) = if (w, h) == (doc.doc.width(), doc.doc.height()) {
            (w, h)
        } else {
            commands::fit(w, h, doc.doc.width(), doc.doc.height())
        };
        let scaled = if (tw, th) == (w, h) { rgba } else { commands::reduce(&rgba, w, h, tw, th) };
        doc.push_history("Place picture");
        // A placed picture is a picture, not a pixel layer: it keeps its frame, so the Place Image
        // and Picture Frame tools can move and resize it afterwards without resampling it.
        doc.doc.layers_mut().push(model::Layer {
            id,
            name: picture_name(&name),
            kind: Kind::Picture,
            visible: true,
            locked: false,
            opacity: 1.0,
            blend: model::Blend::Normal,
            clipped: false,
            children: Vec::new(),
            content: model::Content::Picture(model::PictureRef {
                frame: model::Frame::new(0.0, 0.0, f64::from(tw), f64::from(th)),
                pixels: std::sync::Arc::new(scaled),
                width: tw,
                height: th,
                fit: model::PictureFit::Stretch,
            }),
            style: LayerStyle::default(),
        });
        doc.active_layer = Some(id);
        doc.selected = vec![id];
        doc.touch();
        Ok(json!({ "layer": id }).to_string())
    }

    /// Export the active document; `quality` is 1–100, or 0 for the format's default.
    pub fn export_bytes(&self, format: String, quality: u8) -> Result<Vec<u8>, PhotoError> {
        let s = self.lock();
        let d = s.active().ok_or_else(|| PhotoError::Export("Open a picture first.".into()))?;
        commands::encode_picture(&d.doc, &format, quality).map_err(PhotoError::Export)
    }

    /// Composite the active document, fitted into `max_width` × `max_height`.
    pub fn render(&self, max_width: u32, max_height: u32) -> Result<PhotoFrame, PhotoError> {
        let s = self.lock();
        let d = s.active().ok_or_else(|| PhotoError::Render("Open a picture first.".into()))?;
        let (w, h, rgba) = commands::render_fitted(&d.doc, max_width, max_height);
        Ok(PhotoFrame { width: w, height: h, rgba })
    }

    /// Composite one region of the active document, in document pixels.
    pub fn render_region(&self, x: i32, y: i32, width: u32, height: u32) -> Result<PhotoFrame, PhotoError> {
        let s = self.lock();
        let d = s.active().ok_or_else(|| PhotoError::Render("Open a picture first.".into()))?;
        if x < 0 || y < 0 {
            return Err(PhotoError::Render("That region is outside the canvas.".into()));
        }
        let (x, y) = (x as u32, y as u32);
        // The studio asks for the visible viewport, which can be larger than the engine's
        // per-call budget on a big canvas; split it rather than refusing.
        const LIMIT: u32 = 2048;
        if width == 0 || height == 0 {
            return Err(PhotoError::Render("An empty region has no pixels.".into()));
        }
        if width <= LIMIT && height <= LIMIT {
            return Ok(PhotoFrame { width, height, rgba: compose::render_rect(&d.doc, x, y, width, height) });
        }
        let mut rgba = vec![0u8; (width as usize) * (height as usize) * 4];
        let mut ty = 0;
        while ty < height {
            let th = LIMIT.min(height - ty);
            let mut tx = 0;
            while tx < width {
                let tw = LIMIT.min(width - tx);
                let part = compose::render_rect(&d.doc, x + tx, y + ty, tw, th);
                // A part hanging off the canvas comes back shorter; copy what exists.
                for row in 0..th as usize {
                    let dst = ((ty as usize + row) * width as usize + tx as usize) * 4;
                    let src = row * tw as usize * 4;
                    let len = (tw as usize * 4).min(part.len().saturating_sub(src));
                    if len > 0 && dst + len <= rgba.len() {
                        rgba[dst..dst + len].copy_from_slice(&part[src..src + len]);
                    }
                }
                tx += tw;
            }
            ty += th;
        }
        Ok(PhotoFrame { width, height, rgba })
    }

    /// A thumbnail of the active document.
    pub fn thumbnail(&self, max_side: u32) -> Result<PhotoFrame, PhotoError> {
        self.render(max_side, max_side)
    }

    /// Make the document at `index` active.
    pub fn set_active(&self, index: u32) -> bool {
        let mut s = self.lock();
        if (index as usize) >= s.documents.len() {
            return false;
        }
        s.active = Some(index as usize);
        true
    }

    /// Close the document at `index`, discarding its changes.
    pub fn close(&self, index: u32) -> bool {
        let mut s = self.lock();
        if (index as usize) >= s.documents.len() {
            return false;
        }
        s.documents.remove(index as usize);
        s.active = match s.active {
            Some(a) if a >= s.documents.len() => s.documents.len().checked_sub(1),
            Some(a) if a > index as usize => Some(a - 1),
            other => other,
        };
        true
    }

    pub fn undo(&self) -> bool {
        self.lock().active_mut().map(OpenDocument::undo).unwrap_or(false)
    }

    pub fn redo(&self) -> bool {
        self.lock().active_mut().map(OpenDocument::redo).unwrap_or(false)
    }

    /// The active document's size and name.
    pub fn document_size(&self) -> Option<PhotoDocumentInfo> {
        let s = self.lock();
        s.active.and_then(|i| Self::info(&s, i))
    }
}

impl Default for PhotoSession {
    fn default() -> Self {
        Self::new()
    }
}

/// A file's name without its extension, for the document and layer names the UI lists.
fn picture_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    match base.rfind('.') {
        Some(i) if i > 0 => base[..i].to_string(),
        _ => base.to_string(),
    }
}

#[cfg(test)]
mod tests;
