//! Pictures and sound of a moment of the project, made without holding the session: a
//! [`Renderer`] is a snapshot of the project (it costs a few pointer copies), so frames,
//! thumbnails and audio can be made on any number of threads while edits go on. Take a new one
//! when the timeline's revision changes.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use filmcraft_engine::Session;
use filmcraft_engine::media_pool::MediaPool;
use filmcraft_engine::previews::PreviewStore;
use filmcraft_project::{ItemId, Project};
use filmcraft_render::{RenderOptions, SourceProvider};
use filmcraft_time::{FrameRate, Tick};

use crate::{Frame, MobileError};

/// The largest side a picture may be asked for: a 4K frame's long side.
const MAX_SIDE: u32 = 4096;
/// The longest stretch of audio one call mixes (10 s at 48 kHz).
const MAX_MIX_FRAMES: u32 = 480_000;
/// The most waveform values one call returns.
const MAX_PEAKS: u32 = 200_000;

/// Lets the app abandon a picture it no longer needs (the playhead moved on).
#[derive(uniffi::Object, Default)]
pub struct CancelToken {
    flag: Arc<AtomicBool>,
}

#[uniffi::export]
impl CancelToken {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn cancel(&self) {
        self.flag.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::Relaxed)
    }
}

/// A snapshot of the open sequence for drawing and playing it.
#[derive(uniffi::Object)]
pub struct Renderer {
    project: Arc<Project>,
    seq: ItemId,
    media: Arc<MediaPool>,
    services: Arc<dyn filmcraft_engine::Services>,
    previews: Arc<PreviewStore>,
    revision: u64,
    width: u32,
    height: u32,
    rate: FrameRate,
    sample_rate: u32,
}

impl Renderer {
    pub(crate) fn of(s: &Session) -> Result<Self, MobileError> {
        let seq = s.state.active_sequence.ok_or(MobileError::NoSequence)?;
        let q = s.active_sequence().ok_or(MobileError::NoSequence)?;
        Ok(Self {
            project: s.project.clone(),
            seq,
            media: s.media.clone(),
            services: s.services.clone(),
            previews: s.previews.clone(),
            revision: s.revision,
            width: q.settings.width,
            height: q.settings.height,
            rate: s.sequence_rate(),
            sample_rate: q.settings.sample_rate.max(8000),
        })
    }

    fn provider(&self) -> filmcraft_engine::media_pool::PoolProvider {
        self.media.provider(self.project.clone(), self.services.clone())
    }
}

impl Renderer {
    fn draw_variant(&self, project: Project, time: i64, max_side: u32) -> Result<Frame, MobileError> {
        let project = Arc::new(project);
        let t = self.rate.snap(Tick(time.max(0)));
        let scale = scale_for(self.width.max(self.height), max_side);
        let provider = self.media.provider(project.clone(), self.services.clone());
        let img = filmcraft_media::cancel::with_background(true, || {
            filmcraft_render::render_sequence(&project, self.seq, t, RenderOptions { scale, ..Default::default() }, &provider)
        });
        Ok(frame_of(&img, t.0))
    }
}

/// The part of `f` from (`x`, `y`), `w` × `h` (clamped to it).
fn crop(f: &Frame, x: u32, y: u32, w: u32, h: u32) -> Frame {
    let x = x.min(f.width.saturating_sub(1));
    let y = y.min(f.height.saturating_sub(1));
    let w = w.clamp(1, f.width - x);
    let h = h.clamp(1, f.height - y);
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for row in y..y + h {
        let start = ((row * f.width + x) * 4) as usize;
        let end = start + (w * 4) as usize;
        if let Some(s) = f.rgba.get(start..end) {
            rgba.extend_from_slice(s);
        }
    }
    if rgba.len() != (w * h * 4) as usize {
        return f.clone();
    }
    Frame { width: w, height: h, time: f.time, rgba }
}

fn frame_of(img: &filmcraft_render::Image, time: i64) -> Frame {
    let (width, height) = (u32::try_from(img.w).unwrap_or(0), u32::try_from(img.h).unwrap_or(0));
    Frame { width, height, time, rgba: img.over_black_rgba8() }
}

fn scale_for(long_side: u32, max_side: u32) -> f32 {
    let side = max_side.clamp(16, MAX_SIDE);
    (side as f32 / long_side.max(1) as f32).min(1.0)
}

fn guarded<R>(cancel: &Option<Arc<CancelToken>>, f: impl FnOnce() -> R) -> R {
    match cancel {
        Some(c) => filmcraft_media::cancel::with_cancel(&c.flag, f),
        None => f(),
    }
}

#[uniffi::export]
impl Renderer {
    /// The timeline revision this snapshot shows.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Sample rate of [`Renderer::mix`].
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// The program at `time` (ticks, snapped to a frame) with its long side at most `max_side`.
    /// `draft` decodes faster at a little less quality (for playback and scrubbing).
    pub fn frame(&self, time: i64, max_side: u32, draft: bool, cancel: Option<Arc<CancelToken>>) -> Result<Frame, MobileError> {
        let t = self.rate.snap(Tick(time.max(0)));
        let scale = scale_for(self.width.max(self.height), max_side);
        let provider = self.provider();
        let img = guarded(&cancel, || {
            filmcraft_media::cancel::with_draft(draft, || {
                filmcraft_render::render_sequence(&self.project, self.seq, t, RenderOptions { scale, captions: true, ..Default::default() }, &provider)
            })
        });
        if cancel.as_ref().is_some_and(|c| c.is_cancelled()) {
            return Err(MobileError::Render("cancelled".into()));
        }
        Ok(frame_of(&img, t.0))
    }

    /// A still of project item `item` at media time `time`, long side at most `max_side` (for
    /// the timeline's filmstrips and pickers).
    pub fn thumbnail(&self, item: u64, time: i64, max_side: u32, cancel: Option<Arc<CancelToken>>) -> Result<Frame, MobileError> {
        let provider = self.provider();
        let long = match self.project.item(ItemId(item)).map(|p| &p.kind) {
            Some(filmcraft_project::ItemKind::Media(m)) => m.info.video.as_ref().map(|v| v.width.max(v.height)).unwrap_or(self.width.max(self.height)),
            _ => self.width.max(self.height),
        };
        let scale = scale_for(long, max_side);
        let img = guarded(&cancel, || {
            filmcraft_media::cancel::with_background(true, || filmcraft_render::render_item(&self.project, ItemId(item), Tick(time.max(0)), scale, &provider))
        })
        .ok_or_else(|| MobileError::Render("That picture could not be drawn.".into()))?;
        Ok(frame_of(&img, time))
    }

    /// `frames` frames of the mixed sound from sample `start` (at [`Renderer::sample_rate`]),
    /// stereo, interleaved 32-bit floats, little-endian bytes (what Android's AudioTrack takes).
    /// Reading on from where the last call ended keeps effects continuous.
    pub fn mix(&self, start: i64, frames: u32) -> Vec<u8> {
        let frames = frames.min(MAX_MIX_FRAMES) as usize;
        let provider = self.provider();
        let buf = self.previews.mix(&self.project, self.seq, start.max(0), frames, &provider);
        let left = buf.channels.first();
        let right = buf.channels.get(1).or(left);
        let mut out = Vec::with_capacity(frames * 8);
        for i in 0..frames {
            let l = left.and_then(|c| c.get(i)).copied().unwrap_or(0.0);
            let r = right.and_then(|c| c.get(i)).copied().unwrap_or(0.0);
            out.extend_from_slice(&l.to_le_bytes());
            out.extend_from_slice(&r.to_le_bytes());
        }
        out
    }

    /// The frame at `time` with clip `clip` showing filter `filter` at `filter_amount` and effect
    /// `effect` at `effect_amount` instead of its own (empty = none): the pickers' previews.
    #[allow(clippy::too_many_arguments)]
    pub fn look_preview(
        &self,
        clip: u64,
        filter: String,
        filter_amount: f64,
        effect: String,
        effect_amount: f64,
        time: i64,
        max_side: u32,
    ) -> Result<Frame, MobileError> {
        let mut project = (*self.project).clone();
        let seq = project.sequence_mut(self.seq).ok_or(MobileError::NoSequence)?;
        let (w, h) = (seq.settings.width, seq.settings.height);
        let c = seq.find_item_mut(filmcraft_project::ClipId(clip)).map(|x| x.1).ok_or_else(|| MobileError::Command("That clip is gone.".into()))?;
        let swap = |c: &mut filmcraft_project::TrackItem, slots: Vec<usize>, mut new: Vec<filmcraft_project::EffectInstance>| {
            for e in &mut new {
                filmcraft_project::resolve_auto_points(e, (w, h), (w, h));
            }
            crate::edits::swap_instances(c, &slots, new);
        };
        let filter_new = if filter.is_empty() { Vec::new() } else { crate::presets::filter_instances(&filter, filter_amount).unwrap_or_default() };
        let slots = crate::presets::filter_slots(c);
        swap(c, slots, filter_new);
        let effect_new = if effect.is_empty() { Vec::new() } else { crate::presets::effect_instances(&effect, effect_amount).unwrap_or_default() };
        let slots = crate::presets::effect_slots(c);
        swap(c, slots, effect_new);
        // drawn larger, then cut to the clip's own picture
        let rect = project.sequence(self.seq).and_then(|q| crate::placement::clip_rect(&project, q, clip));
        let side = self.width.max(self.height) as f64;
        let f = self.draw_variant(project, time, (max_side.saturating_mul(2)).min(MAX_SIDE))?;
        let Some((l, t, r, b)) = rect else { return Ok(f) };
        let k = f64::from(f.width.max(f.height)) / side.max(1.0);
        Ok(crop(&f, (l * k) as u32, (t * k) as u32, ((r - l) * k) as u32, ((b - t) * k) as u32))
    }

    /// The frame in the middle of a transition `preset` of `seconds` put between `from` and `to`
    /// on the main track (either empty at the ends): the transition picker's previews.
    pub fn transition_preview(&self, from: Option<u64>, to: Option<u64>, preset: String, seconds: f64, max_side: u32) -> Result<Frame, MobileError> {
        let mut project = (*self.project).clone();
        let seq = project.sequence_mut(self.seq).ok_or(MobileError::NoSequence)?;
        let track = seq.video_tracks.first_mut().ok_or(MobileError::NoSequence)?;
        let clip = |id: Option<u64>| id.and_then(|c| track.items.iter().find(|i| i.id.0 == c)).map(|i| (i.id, i.start, i.end()));
        let (a, b) = (clip(from), clip(to));
        let secs = if seconds.is_finite() { seconds.clamp(0.1, 5.0) } else { 0.4 };
        let dur = self.rate.snap_nearest(Tick::from_seconds_f64(secs)).max(self.rate.frame_duration());
        let (start, mid) = match (a, b) {
            (Some(_), Some(b)) => (b.1 - Tick(dur.0 / 2), b.1),
            (None, Some(b)) => (b.1, b.1 + Tick(dur.0 / 2)),
            (Some(a), None) => (a.2 - dur, a.2 - Tick(dur.0 / 2)),
            (None, None) => return Err(MobileError::Command("no transition slot".into())),
        };
        track.transitions.retain(|t| !(t.from.map(|c| c.0) == from && t.to.map(|c| c.0) == to));
        if !preset.is_empty() {
            let effect = crate::presets::transition_instance(&preset).ok_or_else(|| MobileError::Command(format!("Unknown transition {preset}.")))?;
            let id = filmcraft_project::TransitionId(project.next_id);
            let seq = project.sequence_mut(self.seq).ok_or(MobileError::NoSequence)?;
            if let Some(track) = seq.video_tracks.first_mut() {
                track.transitions.push(filmcraft_project::Transition {
                    id,
                    effect,
                    start: start.max(Tick::ZERO),
                    duration: dur,
                    from: a.map(|x| x.0),
                    to: b.map(|x| x.0),
                    align: Default::default(),
                    reverse: false,
                });
            }
        }
        self.draw_variant(project, mid.0, max_side)
    }

    /// The loudness of project item `item` between media times `from` and `to`, as `buckets`
    /// peak values from 0 to 1 (for drawing waveforms). Empty when it has no sound.
    pub fn peaks(&self, item: u64, from: i64, to: i64, buckets: u32) -> Vec<f32> {
        let buckets = buckets.clamp(1, MAX_PEAKS) as usize;
        let Some(src) = self.provider().source(ItemId(item)) else { return Vec::new() };
        let Some(info) = src.info().audio().cloned() else { return Vec::new() };
        let rate = info.sample_rate.clamp(8000, 192_000);
        let (from, to) = (Tick(from.max(0)), Tick(to.max(from.max(0))));
        let start = from.to_units_floor(i64::from(rate));
        let total = (to - from).to_units_floor(i64::from(rate)).max(0) as usize;
        if total == 0 {
            return vec![0.0; buckets];
        }
        let per = total.div_ceil(buckets).max(1);
        let mut out = vec![0.0f32; buckets];
        // read in pieces of about four seconds
        let chunk = (per * (16_384 / per).max(1)).max(per);
        let mut done = 0usize;
        while done < total {
            let n = chunk.min(total - done);
            let Ok(buf) = filmcraft_media::cancel::with_background(true, || src.audio(start + done as i64, n, rate)) else { break };
            for ch in &buf.channels {
                for (i, v) in ch.iter().enumerate() {
                    let b = (done + i) / per;
                    if let Some(slot) = out.get_mut(b) {
                        *slot = slot.max(v.abs());
                    }
                }
            }
            done += n;
        }
        for v in &mut out {
            *v = v.min(1.0);
        }
        out
    }
}
