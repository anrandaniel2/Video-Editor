//! The session the FilmCraft Android app drives: PhotoCraft's engine behind UniFFI.
//!
//! The Android app owns no document model of its own. It holds one [`PhotoSession`] (a
//! `photocraft_engine::Session` behind a mutex), sends it JSON commands through
//! [`PhotoSession::execute`], and asks it for the composited pixels through
//! [`PhotoSession::render`] and [`PhotoSession::render_region`]. The Compose UI is a thin
//! shell over that: every panel, tool and adjustment in the editor is a command id the engine
//! already implements, so the UI stays in step with the engine by construction.
//!
//! The engine's file-system commands (`file.open`, `file.save`, …) are not used: the shell
//! owns the file system (SAF pickers, MediaStore, the app's own project folder) and hands the
//! engine bytes. [`PhotoSession::open_bytes`] and [`PhotoSession::export_bytes`] are the two
//! doors between them.

use std::sync::Mutex;

use photocraft_engine::{EngineError, Session};
use serde_json::{Value, json};

uniffi::setup_scaffolding!();

/// What went wrong, as the Kotlin side shows it.
///
/// Flat (no per-variant data): the Kotlin bindings render it as a `sealed class` of exceptions
/// carrying the engine's message, and the shell catches `Exception` either way.
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

impl From<EngineError> for PhotoError {
    fn from(e: EngineError) -> Self {
        PhotoError::Command(e.to_string())
    }
}

/// A composited image: 8-bit RGBA, row-major, `width * height * 4` bytes.
#[derive(uniffi::Record)]
pub struct PhotoFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// One open document, as the document switcher lists it.
#[derive(uniffi::Record)]
pub struct PhotoDocumentInfo {
    pub index: u32,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub dirty: bool,
}

/// The session's state, for the shell to notice what changed.
#[derive(uniffi::Record)]
pub struct PhotoStatus {
    /// Bumped on every document change; the shell re-renders when it moves.
    pub revision: u64,
    pub dirty: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub undo_label: String,
    pub redo_label: String,
    /// The active document, if any.
    pub active: Option<PhotoDocumentInfo>,
    /// Every open document, in tab order.
    pub documents: Vec<PhotoDocumentInfo>,
    /// The active layer's id, if any.
    pub active_layer: Option<u64>,
}

/// The engine session the Compose editor drives. One per editor screen; the shell creates it,
/// hands it bytes, and drops it when the editor closes.
#[derive(uniffi::Object)]
pub struct PhotoSession {
    session: Mutex<Session>,
}

impl PhotoSession {
    fn lock(&self) -> std::sync::MutexGuard<'_, Session> {
        // A poisoned mutex means a command panicked mid-edit; the session is still usable
        // (the engine rolls the document back), so recover rather than refuse.
        self.session.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn doc_info(s: &Session, index: usize) -> Option<PhotoDocumentInfo> {
        let st = s.documents().get(index)?;
        Some(PhotoDocumentInfo {
            index: index as u32,
            name: st.doc.name.clone(),
            width: st.doc.size.width,
            height: st.doc.size.height,
            dirty: st.is_dirty(),
        })
    }
}

#[uniffi::export]
impl PhotoSession {
    /// A fresh session with no documents.
    #[uniffi::constructor]
    pub fn new() -> Self {
        start_log();
        PhotoSession { session: Mutex::new(Session::new()) }
    }

    /// Run one engine command. `params` is JSON (an object, or empty); the result is the
    /// command's JSON value, as a string. This is the whole command surface: every tool,
    /// adjustment, filter, layer operation and selection in the editor is one of these ids.
    pub fn execute(&self, id: String, params: String) -> Result<String, PhotoError> {
        let params: Value = if params.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(&params).map_err(|e| PhotoError::Command(format!("bad params for `{id}`: {e}")))?
        };
        let mut s = self.lock();
        let out = s.execute(&id, params).map_err(PhotoError::from)?;
        Ok(out.to_string())
    }

    /// Whether the command can run right now (the shell greys its menu items with this).
    pub fn is_enabled(&self, id: String) -> bool {
        self.lock().is_enabled(&id)
    }

    /// Why the command cannot run, if it cannot.
    pub fn disabled_reason(&self, id: String) -> Option<String> {
        self.lock().disabled_reason(&id)
    }

    /// The session's state: revision, undo/redo, the open documents and the active layer.
    pub fn status(&self) -> PhotoStatus {
        let s = self.lock();
        let active = s.active_index();
        let documents: Vec<PhotoDocumentInfo> = (0..s.documents().len())
            .filter_map(|i| Self::doc_info(&s, i))
            .collect();
        let active_info = active.and_then(|i| Self::doc_info(&s, i));
        let (can_undo, can_redo, undo_label, redo_label) = match s.active() {
            Some(st) => {
                let h = &st.history;
                (
                    h.can_undo(),
                    h.can_redo(),
                    h.undo_label().unwrap_or_default().to_string(),
                    h.redo_label().unwrap_or_default().to_string(),
                )
            }
            None => (false, false, String::new(), String::new()),
        };
        PhotoStatus {
            revision: s.active().map(|st| st.revision).unwrap_or(0),
            dirty: s.active().map(|st| st.is_dirty()).unwrap_or(false),
            can_undo,
            can_redo,
            undo_label,
            redo_label,
            active: active_info,
            documents,
            active_layer: s.active().and_then(|st| st.active_layer.map(|l| l.0)),
        }
    }

    /// Open the given bytes as a new document and make it active. `name` is the file's name
    /// (its extension picks the decoder); `as_ext` overrides it, as Open As does. Returns the
    /// new document's index.
    pub fn open_bytes(&self, name: String, bytes: Vec<u8>, as_ext: Option<String>) -> Result<u32, PhotoError> {
        let mut s = self.lock();
        let out = photocraft_engine::file_cmds::open_bytes_as(&mut s, &name, &bytes, as_ext.as_deref(), None)
            .map_err(|e| PhotoError::Import(e.to_string()))?;
        Ok(out.get("document").and_then(Value::as_u64).unwrap_or(0) as u32)
    }

    /// Place the given bytes as a new layer on the active document. `params` is the place
    /// command's JSON (position, scale, …), or empty for the defaults.
    pub fn place_bytes(&self, name: String, bytes: Vec<u8>, params: String) -> Result<String, PhotoError> {
        let params: Value = if params.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(&params).map_err(|e| PhotoError::Import(format!("bad place params: {e}")))?
        };
        let mut s = self.lock();
        let out = photocraft_engine::file_cmds::place_bytes(&mut s, &name, bytes, None, &params)
            .map_err(|e| PhotoError::Import(e.to_string()))?;
        Ok(out.to_string())
    }

    /// Export the active document to `format` (`"png"`, `"jpeg"`, `"webp"`, `"psd"`, …) and
    /// return the file's bytes. `quality` is the JPEG/WebP quality (1–100), or 0 for the
    /// format's default. Layered formats (PSD, layered TIFF) keep the document's layers.
    pub fn export_bytes(&self, format: String, quality: u8) -> Result<Vec<u8>, PhotoError> {
        let s = self.lock();
        let st = s.active().ok_or_else(|| PhotoError::Export("no document open".into()))?;
        let mut opts = photocraft_io::ExportOptions::default();
        if quality > 0 {
            // Both are u8 percentages in the codec options (1–100); 0 leaves the defaults.
            opts.encode.jpeg_quality = quality.clamp(1, 100);
            opts.encode.webp_quality = quality.clamp(1, 100);
        }
        let result = photocraft_io::export(&st.doc, &format, &opts).map_err(|e| PhotoError::Export(e.to_string()))?;
        Ok(result.bytes)
    }

    /// Composite the active document at the given size, fitting the whole canvas into it.
    /// The result is the canvas scaled to fit, with the same aspect ratio.
    pub fn render(&self, max_width: u32, max_height: u32) -> Result<PhotoFrame, PhotoError> {
        let s = self.lock();
        let st = s.active().ok_or_else(|| PhotoError::Render("no document open".into()))?;
        let bounds = st.doc.bounds();
        let (w, h) = fit(bounds.width(), bounds.height(), max_width, max_height);
        // The engine's own area-averaged reduction: a fit-to-screen preview must not read the
        // whole-document composite (a 24 MP document would cost 380 MB here for a thumbnail).
        Ok(frame_from_buffer(photocraft_compose::render_reduced(&st.doc, w, h)))
    }

    /// Composite one region of the active document, in document pixels. For the canvas
    /// viewport: the shell asks for the visible region at the zoom it shows, not the whole
    /// document.
    pub fn render_region(&self, x: i32, y: i32, width: u32, height: u32) -> Result<PhotoFrame, PhotoError> {
        let s = self.lock();
        let st = s.active().ok_or_else(|| PhotoError::Render("no document open".into()))?;
        let rect = photocraft_geom::Rect::from_xywh(x, y, width, height);
        let buf = photocraft_compose::render(&st.doc, rect);
        Ok(frame_from_buffer(buf))
    }

    /// A thumbnail of the active document, at most `max_side` pixels on its longer side.
    pub fn thumbnail(&self, max_side: u32) -> Result<PhotoFrame, PhotoError> {
        self.render(max_side, max_side)
    }

    /// Make the document at `index` active. Returns false when there is no such document.
    pub fn set_active(&self, index: u32) -> bool {
        self.lock().set_active(index as usize)
    }

    /// Close the document at `index`, discarding its changes. Returns false when there is no
    /// such document.
    pub fn close(&self, index: u32) -> bool {
        self.lock().close(index as usize).is_some()
    }

    /// Undo one step. Returns false when there is nothing to undo.
    pub fn undo(&self) -> bool {
        self.lock().undo()
    }

    /// Redo one step. Returns false when there is nothing to redo.
    pub fn redo(&self) -> bool {
        self.lock().redo()
    }

    /// The active document's size, in pixels.
    pub fn document_size(&self) -> Option<PhotoDocumentInfo> {
        let s = self.lock();
        s.active_index().and_then(|i| Self::doc_info(&s, i))
    }
}

/// The largest size with `w`/`h`'s aspect ratio that fits in `max_w` × `max_h`, at least 1×1.
fn fit(w: u32, h: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    let (w, h, max_w, max_h) = (w.max(1) as f64, h.max(1) as f64, max_w.max(1) as f64, max_h.max(1) as f64);
    let scale = (max_w / w).min(max_h / h).min(1.0);
    ((w * scale).round().max(1.0) as u32, (h * scale).round().max(1.0) as u32)
}

/// Turn a composited buffer into 8-bit RGBA for the shell. The buffer is already the size the
/// caller asked for: [`PhotoSession::render`] reduces at the engine's own resampler, and
/// [`PhotoSession::render_region`] is rendered 1:1.
fn frame_from_buffer(buf: photocraft_compose::Buffer) -> PhotoFrame {
    let img = buf.to_rgba8();
    PhotoFrame { width: img.width, height: img.height, rgba: img.pixels }
}

/// The log, to Android's (tag "photocraft-android"), with a panic's message and place in it;
/// once. Called from the constructor, so the engine's own logs reach logcat from the first
/// command.
fn start_log() {
    #[cfg(target_os = "android")]
    {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            android_logger::init_once(
                android_logger::Config::default().with_max_level(log::LevelFilter::Info).with_tag("photocraft-android"),
            );
            let before = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                log::error!("{info}");
                before(info);
            }));
        });
    }
}
