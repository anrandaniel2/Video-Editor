//! The timeline as the phone editor shows it, read from the engine's sequence.
//!
//! The phone model is a simple one laid over the engine's tracks:
//!
//! - **V1 is the main track.** It has no gaps: inserts, deletes, trims and speed changes ripple
//!   the clips after them.
//! - **V2 and up are overlay lanes** (text, pictures over the main video), drawn above the main
//!   track, the highest on top.
//! - **A1 holds the sound of the main track's clips.** Each video clip's audio is its linked
//!   partner there, shown inside the video clip (its waveform lane), never as a clip of its own.
//!   The same goes for the linked audio of overlay clips, wherever it sits.
//! - **A2 and up are audio lanes** for sound that stands alone: music, voice-overs and extracted
//!   audio, drawn under the main track.

use filmcraft_engine::Session;
use filmcraft_project::graphic::TEXT_LAYER;
use filmcraft_project::{ClipId, ItemId, ItemKind, MediaRef, ParamValue, Sequence, Track, TrackItem, TrackKind};
use filmcraft_time::Tick;

use crate::MobileError;
use crate::presets;

/// What a lane holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum LaneKind {
    /// V1: the main video track.
    Main,
    /// V2 and up: overlays and text.
    Overlay,
    /// A2 and up: music, voice-overs, extracted audio.
    Audio,
}

/// What a clip is, for how it is drawn and which tools it offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ClipKind {
    Video,
    Image,
    Text,
    Audio,
    Blank,
    /// Its media file is gone.
    Missing,
}

/// One clip on the timeline. Times are in ticks; volumes and opacities are fractions (1.0 =
/// 100 %).
#[derive(Debug, Clone, uniffi::Record)]
pub struct ClipInfo {
    pub id: u64,
    /// The project item it plays.
    pub item: u64,
    pub kind: ClipKind,
    /// Where it sits on the timeline.
    pub start: i64,
    pub duration: i64,
    /// Where it starts in its media.
    pub source_in: i64,
    /// The media's own length; 0 for stills, text and blanks (which have none).
    pub media_duration: i64,
    /// Playback speed (1.0 = normal).
    pub speed: f64,
    pub name: String,
    /// A text clip's text, or what a speech clip says.
    pub text: String,
    /// The media file, empty for text and blanks.
    pub path: String,
    /// The picture's size, 0 when it has none.
    pub media_width: u32,
    pub media_height: u32,
    /// It plays sound (a video with an audio stream, or an audio clip).
    pub has_audio: bool,
    pub muted: bool,
    /// 0.0 to 2.0.
    pub volume: f64,
    pub fade_in: i64,
    pub fade_out: i64,
    /// 0.0 to 1.0.
    pub opacity: f64,
    pub mirrored: bool,
    pub flipped: bool,
    /// The filter's id, empty for none; its intensity 0 to 200.
    pub filter: String,
    pub filter_amount: f64,
    /// The effect's id, empty for none; its intensity 0 to 100.
    pub effect: String,
    pub effect_amount: f64,
    /// The clip carries a 3D object / extruded-text effect.
    pub has_3d: bool,
    /// A recorded voice-over.
    pub voiceover: bool,
    /// Speech made from text (it can be said again differently).
    pub speech: bool,
    /// The linked audio clip that carries a video clip's sound.
    pub audio_clip: Option<u64>,
}

/// A row of the timeline.
#[derive(Debug, Clone, uniffi::Record)]
pub struct LaneInfo {
    /// The engine track.
    pub track: u64,
    pub kind: LaneKind,
    /// 0 for the main track, 1, 2… for overlays (higher draws on top), -1, -2… for audio lanes.
    pub index: i32,
    pub clips: Vec<ClipInfo>,
}

/// A transition on the main track: between two clips, or at the very start (`from` empty) or
/// end (`to` empty) of the video.
#[derive(Debug, Clone, uniffi::Record)]
pub struct TransitionInfo {
    pub id: u64,
    /// The preset's id ([`presets::transitions`]), or the engine effect's id when it is not one
    /// of the phone's.
    pub preset: String,
    pub from: Option<u64>,
    pub to: Option<u64>,
    pub start: i64,
    pub duration: i64,
}

/// A caption: words shown over the picture for a while.
#[derive(Debug, Clone, uniffi::Record)]
pub struct CaptionInfo {
    pub id: u64,
    pub start: i64,
    pub duration: i64,
    pub text: String,
}

/// The whole timeline.
#[derive(Debug, Clone, uniffi::Record)]
pub struct TimelineInfo {
    /// Changes with every edit, undo and redo.
    pub revision: u64,
    pub width: u32,
    pub height: u32,
    pub rate_num: i64,
    pub rate_den: i64,
    pub duration: i64,
    pub playhead: i64,
    /// Top to bottom: overlays (highest first), the main track, audio lanes.
    pub lanes: Vec<LaneInfo>,
    /// The main track's transitions.
    pub transitions: Vec<TransitionInfo>,
    /// The selected clip (a video clip rather than its hidden audio), if any.
    pub selected: Option<u64>,
    /// The selected transition, if any.
    pub selected_transition: Option<u64>,
    /// The captions, in time order (shown in their own lane above the others).
    pub captions: Vec<CaptionInfo>,
}

/// The open sequence.
pub(crate) fn sequence(s: &Session) -> Result<(ItemId, &Sequence), MobileError> {
    let id = s.state.active_sequence.ok_or(MobileError::NoSequence)?;
    let seq = s.active_sequence().ok_or(MobileError::NoSequence)?;
    Ok((id, seq))
}

/// Where a clip sits: its track's kind and index, and the clip.
pub(crate) struct Found<'a> {
    pub kind: TrackKind,
    pub track: usize,
    pub clip: &'a TrackItem,
}

pub(crate) fn find(seq: &Sequence, id: ClipId) -> Option<Found<'_>> {
    for (kind, tracks) in [(TrackKind::Video, &seq.video_tracks), (TrackKind::Audio, &seq.audio_tracks)] {
        for (i, t) in tracks.iter().enumerate() {
            if let Some(c) = t.items.iter().find(|c| c.id == id) {
                return Some(Found { kind, track: i, clip: c });
            }
        }
    }
    None
}

/// A clip's linked partner of the other kind (a video clip's audio, an audio clip's video).
pub(crate) fn partner<'a>(seq: &'a Sequence, clip: &TrackItem, kind: TrackKind) -> Option<Found<'a>> {
    let link = clip.link?;
    let tracks = if kind == TrackKind::Video { &seq.video_tracks } else { &seq.audio_tracks };
    for (i, t) in tracks.iter().enumerate() {
        if let Some(c) = t.items.iter().find(|c| c.link == Some(link) && c.id != clip.id) {
            return Some(Found { kind, track: i, clip: c });
        }
    }
    None
}

/// An audio clip that belongs to a video clip (and so is shown inside it, not as a clip).
pub(crate) fn is_hidden_audio(seq: &Sequence, clip: &TrackItem) -> bool {
    partner(seq, clip, TrackKind::Video).is_some()
}

/// The clip the phone means by `id`: an audio partner stands for its video clip.
pub(crate) fn shown_clip(seq: &Sequence, id: ClipId) -> ClipId {
    match find(seq, id) {
        Some(f) if f.kind == TrackKind::Audio => partner(seq, f.clip, TrackKind::Video).map(|p| p.clip.id).unwrap_or(id),
        _ => id,
    }
}

/// The clip that carries a clip's sound: a video clip's linked audio, or an audio clip itself.
pub(crate) fn sound_of<'a>(seq: &'a Sequence, f: &Found<'a>) -> Option<Found<'a>> {
    if f.kind == TrackKind::Audio {
        return Some(Found { kind: f.kind, track: f.track, clip: f.clip });
    }
    partner(seq, f.clip, TrackKind::Audio)
}

fn file_missing(path: &str) -> bool {
    !path.is_empty() && !std::path::Path::new(path).exists()
}

fn float(e: &filmcraft_project::EffectInstance, id: &str) -> Option<f64> {
    match &e.param(id)?.value {
        ParamValue::Float(v) => Some(*v),
        _ => None,
    }
}

/// The level of a clip's Volume effect, as a fraction (0 dB = 1.0).
pub(crate) fn volume_of(c: &TrackItem) -> f64 {
    let db = c.effects.iter().find(|e| e.effect == "volume").and_then(|e| float(e, "level")).unwrap_or(0.0);
    if db <= crate::edits::SILENT_DB + 0.5 { 0.0 } else { 10f64.powf(db / 20.0) }
}

/// The text of a text clip's first text layer.
pub(crate) fn text_of(c: &TrackItem) -> String {
    c.effects
        .iter()
        .filter(|e| e.effect == TEXT_LAYER || e.effect == "object_3d")
        .find_map(|e| match &e.param("text")?.value {
            ParamValue::Text(t) if !t.is_empty() => Some(t.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

/// The fade-in and fade-out lengths on a clip's own track: one-sided transitions at its edges.
fn fades(track: &Track, id: ClipId) -> (i64, i64) {
    let mut fade_in = 0;
    let mut fade_out = 0;
    for t in &track.transitions {
        match (t.from, t.to) {
            (None, Some(to)) if to == id => fade_in = t.duration.0,
            (Some(from), None) if from == id => fade_out = t.duration.0,
            _ => {}
        }
    }
    (fade_in, fade_out)
}

fn clip_info(s: &Session, seq: &Sequence, f: &Found<'_>) -> ClipInfo {
    let c = f.clip;
    let pi = s.project.item(c.item);
    let mut kind = if f.kind == TrackKind::Audio { ClipKind::Audio } else { ClipKind::Video };
    let (mut path, mut media_duration, mut w, mut h, mut media_audio) = (String::new(), 0, 0, 0, false);
    match pi.map(|p| &p.kind) {
        Some(ItemKind::Media(m)) => {
            if let MediaRef::File { path: p } = &m.media {
                path.clone_from(p);
            }
            match &m.media {
                MediaRef::Generator(filmcraft_media::Generator::BlackVideo) => kind = ClipKind::Blank,
                MediaRef::Generator(_) => {}
                MediaRef::File { .. } => {
                    if f.kind == TrackKind::Video && m.info.kind == filmcraft_media::MediaKind::Still {
                        kind = ClipKind::Image;
                    }
                }
            }
            if m.info.kind != filmcraft_media::MediaKind::Still {
                media_duration = m.info.duration.0;
            }
            if let Some(v) = &m.info.video {
                (w, h) = (v.width, v.height);
            }
            media_audio = m.info.has_audio();
            if m.offline || file_missing(&path) {
                kind = ClipKind::Missing;
            }
        }
        Some(ItemKind::Graphic { .. }) => kind = ClipKind::Text,
        _ => {}
    }
    let sound = sound_of(seq, f);
    let has_audio = sound.is_some() && (media_audio || f.kind == TrackKind::Audio);
    let (volume, muted, (fade_in, fade_out)) = match &sound {
        Some(a) => {
            let track = seq.audio_tracks.get(a.track);
            (volume_of(a.clip), !a.clip.enabled, track.map(|t| fades(t, a.clip.id)).unwrap_or((0, 0)))
        }
        None => (1.0, false, (0, 0)),
    };
    let opacity = c.effects.iter().find(|e| e.effect == "opacity").and_then(|e| float(e, "opacity")).unwrap_or(100.0) / 100.0;
    let has = |id: &str| c.effects.iter().any(|e| e.effect == id && e.enabled);
    let (filter, filter_amount) = presets::filter_of(c).unwrap_or_default();
    let (effect, effect_amount) = presets::effect_of(c).unwrap_or_default();
    let name = if c.name.is_empty() { pi.map(|p| p.name.clone()).unwrap_or_default() } else { c.name.clone() };
    ClipInfo {
        id: c.id.0,
        item: c.item.0,
        kind,
        start: c.start.0,
        duration: c.duration.0,
        source_in: c.source_in.0,
        media_duration,
        speed: c.speed.abs(),
        voiceover: f.kind == TrackKind::Audio && name.starts_with(crate::edits::VOICEOVER_PREFIX),
        speech: f.kind == TrackKind::Audio && s.project.narrations.contains_key(&c.item),
        name,
        text: match kind {
            ClipKind::Text => text_of(c),
            // what a speech clip says
            ClipKind::Audio => s.project.narrations.get(&c.item).map(|n| n.text.clone()).unwrap_or_default(),
            _ => String::new(),
        },
        path,
        media_width: w,
        media_height: h,
        has_audio,
        muted,
        volume,
        fade_in,
        fade_out,
        opacity,
        mirrored: has("horizontal_flip"),
        flipped: has("vertical_flip"),
        filter,
        filter_amount,
        effect,
        effect_amount,
        has_3d: c.effects.iter().any(|e| e.effect == "object_3d" && e.enabled),
        audio_clip: if f.kind == TrackKind::Video { sound.map(|a| a.clip.id.0) } else { None },
    }
}

/// The timeline of the open sequence.
pub(crate) fn timeline(s: &Session) -> Result<TimelineInfo, MobileError> {
    let (_, seq) = sequence(s)?;
    let rate = s.sequence_rate();
    let mut lanes = Vec::new();
    for (i, t) in seq.video_tracks.iter().enumerate().rev() {
        let clips = t.items.iter().map(|c| clip_info(s, seq, &Found { kind: TrackKind::Video, track: i, clip: c })).collect();
        let kind = if i == 0 { LaneKind::Main } else { LaneKind::Overlay };
        lanes.push(LaneInfo { track: t.id.0, kind, index: i32::try_from(i).unwrap_or(i32::MAX), clips });
    }
    for (i, t) in seq.audio_tracks.iter().enumerate().skip(1) {
        let clips: Vec<ClipInfo> =
            t.items.iter().filter(|c| !is_hidden_audio(seq, c)).map(|c| clip_info(s, seq, &Found { kind: TrackKind::Audio, track: i, clip: c })).collect();
        // a lane only once something stands alone on it
        if clips.is_empty() {
            continue;
        }
        let index = -i32::try_from(i).unwrap_or(i32::MAX);
        lanes.push(LaneInfo { track: t.id.0, kind: LaneKind::Audio, index, clips });
    }
    let transitions = seq
        .video_tracks
        .first()
        .map(|t| {
            t.transitions
                .iter()
                .map(|tr| TransitionInfo {
                    id: tr.id.0,
                    preset: presets::transition_of(&tr.effect),
                    from: tr.from.map(|c| c.0),
                    to: tr.to.map(|c| c.0),
                    start: tr.start.0,
                    duration: tr.duration.0,
                })
                .collect()
        })
        .unwrap_or_default();
    let selected = s.state.selection.first().map(|&c| shown_clip(seq, c).0);
    Ok(TimelineInfo {
        revision: s.revision,
        width: seq.settings.width,
        height: seq.settings.height,
        rate_num: rate.num,
        rate_den: rate.den,
        duration: seq.duration().0,
        playhead: s.playhead().0,
        lanes,
        transitions,
        selected,
        selected_transition: s.state.transition_selection.first().map(|t| t.0),
        captions: seq
            .caption_tracks
            .first()
            .map(|t| {
                t.captions.iter().map(|c| CaptionInfo { id: c.id.0, start: c.start.0, duration: c.duration.0, text: c.plain_lines().join("\n") }).collect()
            })
            .unwrap_or_default(),
    })
}

/// The main track's clips in order (start, end, id).
pub(crate) fn main_clips(seq: &Sequence) -> Vec<(Tick, Tick, ClipId)> {
    seq.video_tracks.first().map(|t| t.items.iter().map(|c| (c.start, c.end(), c.id)).collect()).unwrap_or_default()
}
