//! Export to an MP4 (H.264 + AAC) at a chosen resolution, frame rate and quality.
//!
//! Quality is a bit budget per pixel, the engine's adaptive presets: high 0.2, medium 0.1, low
//! 0.05 bits per pixel per frame.

use filmcraft_engine::Session;
use serde_json::{Value, json};

use crate::edits::ex;
use crate::{MobileError, model};

/// Export quality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ExportQuality {
    Low,
    Medium,
    High,
}

/// What to export: the short side (720, 1080 or 2160 pixels; the long side follows the frame's
/// aspect), frames per second and quality.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ExportSettings {
    pub short_side: u32,
    pub fps: u32,
    pub quality: ExportQuality,
}

/// An export's size and expected file size.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ExportPlan {
    pub width: u32,
    pub height: u32,
    pub bitrate_kbps: u32,
    pub estimated_bytes: u64,
}

/// A running export.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ExportStatus {
    /// 0.0 to 1.0.
    pub progress: f64,
    pub finished: bool,
    /// Set when it failed or was cancelled.
    pub error: Option<String>,
    pub path: String,
}

fn bits_per_pixel(q: ExportQuality) -> f64 {
    match q {
        ExportQuality::High => 0.2,
        ExportQuality::Medium => 0.1,
        ExportQuality::Low => 0.05,
    }
}

fn even(x: f64) -> u32 {
    ((x / 2.0).round() * 2.0).clamp(2.0, 7680.0) as u32
}

/// The plan for `settings` on the open sequence.
pub(crate) fn plan(s: &Session, settings: &ExportSettings) -> Result<ExportPlan, MobileError> {
    let (_, seq) = model::sequence(s)?;
    let (w, h) = (f64::from(seq.settings.width.max(1)), f64::from(seq.settings.height.max(1)));
    let short = f64::from(settings.short_side.clamp(144, 4320));
    let (width, height) = if w <= h { (even(short), even(short * h / w)) } else { (even(short * w / h), even(short)) };
    let fps = f64::from(settings.fps.clamp(1, 120));
    let kbps = (f64::from(width) * f64::from(height) * fps * bits_per_pixel(settings.quality) / 1000.0).max(500.0);
    let seconds = seq.duration().seconds();
    // video plus 320 kb/s of sound, and a little for the container
    let estimated = ((kbps + 320.0) * 1000.0 / 8.0 * seconds * 1.02) as u64;
    Ok(ExportPlan { width, height, bitrate_kbps: kbps.round() as u32, estimated_bytes: estimated })
}

/// Start exporting to `path`; returns the job to watch.
pub(crate) fn start(s: &mut Session, path: &str, settings: &ExportSettings) -> Result<u64, MobileError> {
    let p = plan(s, settings)?;
    let (_, seq) = model::sequence(s)?;
    if seq.duration() <= filmcraft_time::Tick::ZERO {
        return Err(MobileError::Command("Add something to the timeline before exporting.".into()));
    }
    let r = ex(
        s,
        "file.exportMedia",
        json!({
            "path": path, "format": "h264", "width": p.width, "height": p.height, "fps": settings.fps,
            "bitrateKbps": p.bitrate_kbps, "maxBitrateKbps": p.bitrate_kbps * 3 / 2, "bitrateMode": "vbr1Pass",
            "audio": true, "range": "entire",
            // captions are part of the picture, as in the preview
            "burnCaptions": true,
            // composite on the GPU (registered at startup); the CPU renders as the fallback
            "gpuRendering": "auto",
            "hardwareEncoding": "auto",
        }),
    )?;
    r["job"].as_u64().ok_or_else(|| MobileError::Command("The export did not start.".into()))
}

/// How export `job` is doing.
pub(crate) fn status(s: &mut Session, job: u64) -> Result<ExportStatus, MobileError> {
    let v = ex(s, "jobs.list", json!({}))?;
    let j = v.as_array().and_then(|a| a.iter().find(|j| j["id"].as_u64() == Some(job))).ok_or_else(|| MobileError::Command("That export is gone.".into()))?;
    let result = &j["result"];
    let error = result.get("error").and_then(Value::as_str).map(str::to_string);
    let path = result.get("path").and_then(Value::as_str).unwrap_or_default().to_string();
    Ok(ExportStatus { progress: j["progress"].as_f64().unwrap_or(0.0).clamp(0.0, 1.0), finished: j["finished"].as_bool().unwrap_or(false), error, path })
}

/// Stop export `job`.
pub(crate) fn cancel(s: &mut Session, job: u64) -> Result<(), MobileError> {
    ex(s, "jobs.cancel", json!({"job": job}))?;
    Ok(())
}
