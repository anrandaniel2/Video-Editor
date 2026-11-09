//! The phone editor's edits, each one undo step, built from the engine's commands.
//!
//! Main-track edits keep V1 free of gaps (inserts push later clips, deletes and trims pull them
//! back); overlays and audio lanes move freely, and items on them after a main-track insert move
//! along with it, whole.

use filmcraft_engine::{EngineError, Session};
use filmcraft_project::{ClipId, EffectInstance, ItemId, ParamValue, TrackItem, TrackKind, Transition, TransitionId};
use filmcraft_time::{FrameRate, Tick};
use serde_json::{Value, json};

use crate::model;
use crate::{MobileError, presets};

/// The Volume level that means silence (the engine's floor).
pub(crate) const SILENT_DB: f64 = -287.5;
/// Voice-over recordings are named "Voiceover 1", "Voiceover 2"…
pub(crate) const VOICEOVER_PREFIX: &str = "Voiceover";
/// Pictures, text and blanks last this long when added (seconds).
pub(crate) const STILL_SECONDS: f64 = 4.0;
/// Speed range (1.0 = normal).
pub(crate) const MIN_SPEED: f64 = 0.1;
pub(crate) const MAX_SPEED: f64 = 10.0;
/// Transition length range (seconds) and the length a new one starts at.
pub(crate) const MIN_TRANSITION: f64 = 0.1;
pub(crate) const MAX_TRANSITION: f64 = 5.0;
pub(crate) const DEFAULT_TRANSITION: f64 = 0.4;
/// A clip can't be split closer to either end than this many frames.
const SPLIT_MARGIN_FRAMES: i64 = 2;
/// Motion Scale's upper bound (the parameter's range, in %).
const MAX_SCALE: f64 = 10_000.0;

/// Run engine command `id` with `params`.
pub(crate) fn ex(s: &mut Session, id: &str, params: Value) -> Result<Value, MobileError> {
    Ok(s.execute(id, params)?)
}

fn other(msg: impl Into<String>) -> EngineError {
    EngineError::Other(msg.into())
}

/// After an edit made without `execute`: what `execute` does after every command, so playback and
/// auto-save see it.
pub(crate) fn settle(s: &mut Session) {
    s.sync_persistence();
    s.previews.live.publish_project(s.project.clone());
}

fn ticks(t: i64) -> Tick {
    Tick(t.max(0))
}

/// The clip `id` (a hidden audio partner counts as its video clip): its track kind and index.
fn locate(s: &Session, id: u64) -> Result<(TrackKind, usize, TrackItem), MobileError> {
    let (_, seq) = model::sequence(s)?;
    let cid = model::shown_clip(seq, ClipId(id));
    let f = model::find(seq, cid).ok_or_else(|| MobileError::Command("That clip is no longer on the timeline.".into()))?;
    Ok((f.kind, f.track, f.clip.clone()))
}

fn is_main(kind: TrackKind, track: usize) -> bool {
    kind == TrackKind::Video && track == 0
}

/// The clip that carries a clip's sound (a video clip's linked audio, or the audio clip).
fn sound_clip(s: &Session, id: u64) -> Result<TrackItem, MobileError> {
    let (_, seq) = model::sequence(s)?;
    let cid = model::shown_clip(seq, ClipId(id));
    let f = model::find(seq, cid).ok_or_else(|| MobileError::Command("That clip is no longer on the timeline.".into()))?;
    model::sound_of(seq, &f).map(|a| a.clip.clone()).ok_or_else(|| MobileError::Command("This clip has no sound.".into()))
}

/// Where new media goes on the main track: after the clip under the playhead, before a clip
/// starting exactly at it, else after the last clip.
pub(crate) fn insert_point(s: &Session) -> Result<Tick, MobileError> {
    let (_, seq) = model::sequence(s)?;
    let ph = s.playhead();
    let clips = model::main_clips(seq);
    for &(start, end, _) in &clips {
        if ph == start {
            return Ok(start);
        }
        if ph > start && ph < end {
            return Ok(end);
        }
    }
    Ok(clips.last().map(|c| c.1).unwrap_or(Tick::ZERO))
}

/// Whether `[start, end)` is free on a track (ignoring `except`).
fn free(items: &[TrackItem], start: Tick, end: Tick, except: Option<ClipId>) -> bool {
    items.iter().all(|c| Some(c.id) == except || c.end() <= start || c.start >= end)
}

/// A lane of `kind` (index ≥ `first`, 0-based) free over `[start, end)`, preferring `want`;
/// adds a track when none is. Returns the engine's 1-based track name ("V2", "A3").
fn free_lane(
    s: &mut Session,
    kind: TrackKind,
    first: usize,
    want: Option<usize>,
    start: Tick,
    end: Tick,
    except: Option<ClipId>,
) -> Result<String, MobileError> {
    let letter = if kind == TrackKind::Video { "V" } else { "A" };
    Ok(format!("{letter}{}", free_lane_index(s, kind, first, want, start, end, except)? + 1))
}

/// [`free_lane`] as a 0-based track index.
pub(crate) fn free_lane_index(
    s: &mut Session,
    kind: TrackKind,
    first: usize,
    want: Option<usize>,
    start: Tick,
    end: Tick,
    except: Option<ClipId>,
) -> Result<usize, MobileError> {
    let (_, seq) = model::sequence(s)?;
    let tracks = if kind == TrackKind::Video { &seq.video_tracks } else { &seq.audio_tracks };
    if let Some(w) = want.filter(|&w| w >= first)
        && tracks.get(w).is_some_and(|t| free(&t.items, start, end, except))
    {
        return Ok(w);
    }
    if let Some(i) = tracks.iter().enumerate().skip(first).find(|(_, t)| free(&t.items, start, end, except)).map(|(i, _)| i) {
        return Ok(i);
    }
    let missing = first.saturating_sub(tracks.len()) + 1;
    let n = tracks.len();
    let p = if kind == TrackKind::Video { json!({"video": missing, "audio": 0}) } else { json!({"video": 0, "audio": missing}) };
    ex(s, "sequence.addTracks", p)?;
    Ok(n.max(first))
}

fn clips_of(v: &Value, key: &str) -> Vec<u64> {
    v[key].as_array().map(|a| a.iter().filter_map(Value::as_u64).collect()).unwrap_or_default()
}

fn select_one(s: &mut Session, id: Option<u64>) -> Result<(), MobileError> {
    match id {
        Some(c) => ex(s, "timeline.select", json!({"clips": [c]}))?,
        // an empty clip selection also clears a transition's
        None => ex(s, "timeline.select", json!({"clips": []}))?,
    };
    Ok(())
}

/// Import files into the project; their item ids, or the first error.
pub(crate) fn import(s: &mut Session, paths: &[String]) -> Result<Vec<u64>, MobileError> {
    let r = s.execute("file.import", json!({ "paths": paths })).map_err(|e| MobileError::Import(e.to_string()))?;
    let items: Vec<u64> = r["items"].as_array().map(|a| a.iter().filter_map(Value::as_u64).collect()).unwrap_or_default();
    if items.is_empty() {
        let msg = r["errors"].as_array().and_then(|e| e.first()).and_then(Value::as_str).unwrap_or("The file holds no media.").to_string();
        return Err(MobileError::Import(msg));
    }
    Ok(items)
}

fn item_kind(s: &Session, item: u64) -> Option<filmcraft_media::MediaKind> {
    match &s.project.item(ItemId(item))?.kind {
        filmcraft_project::ItemKind::Media(m) => Some(m.info.kind),
        _ => None,
    }
}

/// Add pictures, videos and sounds: pictures and videos onto the main track at the insert point,
/// back to back; sounds onto audio lanes at the playhead. Returns the new clips (the first is
/// selected).
pub(crate) fn add_media(s: &mut Session, paths: &[String]) -> Result<Vec<u64>, MobileError> {
    let items = import(s, paths)?;
    let r = s.edit_group("Add Media", |s| {
        let mut out = Vec::new();
        let mut at = insert_point(s).map_err(|e| other(e.to_string()))?;
        for &item in &items {
            if item_kind(s, item) == Some(filmcraft_media::MediaKind::AudioOnly) {
                let ph = s.playhead();
                let len = item_duration(s, item);
                let lane = free_lane(s, TrackKind::Audio, 1, None, ph, ph + len, None).map_err(|e| other(e.to_string()))?;
                let r = s.execute("timeline.place", json!({"item": item, "time": ph.0, "track": lane}))?;
                out.extend(clips_of(&r, "clips"));
                continue;
            }
            let r =
                ripple_main(s, at, |s| s.execute("timeline.place", json!({"item": item, "time": at.0, "track": "V1", "audioTrack": "A1", "insert": true})))?;
            let placed = clips_of(&r, "clips");
            // the video clip (not its audio) is the one the phone shows
            let (_, seq) = model::sequence(s).map_err(|e| other(e.to_string()))?;
            if let Some(v) = placed.iter().filter_map(|&c| model::find(seq, ClipId(c))).find(|f| f.kind == TrackKind::Video) {
                at = v.clip.end();
                out.push(v.clip.id.0);
            }
        }
        Ok(out)
    })?;
    select_one(s, r.first().copied())?;
    Ok(r)
}

fn item_duration(s: &Session, item: u64) -> Tick {
    match s.project.item(ItemId(item)).map(|p| &p.kind) {
        Some(filmcraft_project::ItemKind::Media(m)) if m.info.duration > Tick::ZERO => m.info.duration,
        _ => Tick::from_seconds_f64(STILL_SECONDS),
    }
}

/// Add sound files (Files ▸ Audio): on a free audio lane at the playhead.
pub(crate) fn add_audio(s: &mut Session, paths: &[String]) -> Result<Vec<u64>, MobileError> {
    let items = import(s, paths)?;
    let r = s.edit_group("Add Audio", |s| {
        let mut out = Vec::new();
        let mut at = s.playhead();
        for &item in &items {
            let len = item_duration(s, item);
            let lane = free_lane(s, TrackKind::Audio, 1, None, at, at + len, None).map_err(|e| other(e.to_string()))?;
            let r = s.execute("timeline.place", json!({"item": item, "time": at.0, "track": lane, "video": false}))?;
            let placed = clips_of(&r, "clips");
            let (_, seq) = model::sequence(s).map_err(|e| other(e.to_string()))?;
            if let Some(f) = placed.iter().filter_map(|&c| model::find(seq, ClipId(c))).find(|f| f.kind == TrackKind::Audio) {
                at = f.clip.end();
                out.push(f.clip.id.0);
            }
        }
        Ok(out)
    })?;
    select_one(s, r.first().copied())?;
    Ok(r)
}

/// Add a blank (black) clip to the main track at the insert point.
pub(crate) fn add_blank(s: &mut Session) -> Result<u64, MobileError> {
    let r = s.edit_group("Add Blank", |s| {
        let item = s.execute("file.newBlackVideo", json!({"seconds": STILL_SECONDS}))?["item"].as_u64().ok_or_else(|| other("no blank was made"))?;
        let at = insert_point(s).map_err(|e| other(e.to_string()))?;
        let r = ripple_main(s, at, |s| s.execute("timeline.place", json!({"item": item, "time": at.0, "track": "V1", "insert": true, "audio": false})))?;
        clips_of(&r, "clips").first().copied().ok_or_else(|| other("the blank could not be placed"))
    })?;
    select_one(s, Some(r))?;
    s.set_playhead(locate(s, r)?.2.start);
    Ok(r)
}

/// Split the clip at the playhead; the part after the cut is selected.
pub(crate) fn split(s: &mut Session, id: u64) -> Result<u64, MobileError> {
    let (kind, track, c) = locate(s, id)?;
    let rate = s.sequence_rate();
    let ph = rate.snap(s.playhead());
    let margin = Tick(rate.frame_duration().0.saturating_mul(SPLIT_MARGIN_FRAMES));
    if ph < c.start + margin || ph > c.end() - margin {
        return Err(MobileError::Command("Move the playhead into the clip to split it.".into()));
    }
    ex(s, "timeline.razor", json!({"time": ph.0, "clip": c.id.0}))?;
    let (_, seq) = model::sequence(s)?;
    let tracks = if kind == TrackKind::Video { &seq.video_tracks } else { &seq.audio_tracks };
    let right = tracks.get(track).and_then(|t| t.items.iter().find(|i| i.start == ph)).map(|i| i.id.0).unwrap_or(id);
    select_one(s, Some(right))?;
    Ok(right)
}

/// Delete a clip: the main track closes the gap (and later items on other lanes move up with it);
/// on another lane the gap stays.
pub(crate) fn delete(s: &mut Session, id: u64) -> Result<(), MobileError> {
    let (kind, track, c) = locate(s, id)?;
    if !is_main(kind, track) {
        ex(s, "edit.clear", json!({"clips": [c.id.0]}))?;
        return Ok(());
    }
    s.edit_group("Delete", |s| ripple_main(s, c.end(), |s| s.execute("edit.rippleDelete", json!({"clips": [c.id.0]}))))?;
    Ok(())
}

/// Switch sync lock off on every track but V1 and A1; returns the tracks it changed.
fn unlock_others(s: &mut Session) -> filmcraft_engine::Result<Vec<String>> {
    let Some(seq) = s.active_sequence() else { return Err(EngineError::NoSequence) };
    let mut names = Vec::new();
    for (i, t) in seq.video_tracks.iter().enumerate().skip(1) {
        if t.sync_lock {
            names.push(format!("V{}", i + 1));
        }
    }
    for (i, t) in seq.audio_tracks.iter().enumerate().skip(1) {
        if t.sync_lock {
            names.push(format!("A{}", i + 1));
        }
    }
    for n in &names {
        s.execute("timeline.setTrack", json!({"track": n, "syncLock": false}))?;
    }
    Ok(names)
}

fn relock(s: &mut Session, names: &[String]) -> filmcraft_engine::Result<()> {
    for n in names {
        s.execute("timeline.setTrack", json!({"track": n, "syncLock": true}))?;
    }
    Ok(())
}

fn main_end(s: &Session) -> filmcraft_engine::Result<Tick> {
    let seq = s.active_sequence().ok_or(EngineError::NoSequence)?;
    Ok(seq.video_tracks.first().map(|t| t.end()).unwrap_or(Tick::ZERO))
}

/// A main-track edit (`f`) that moves what comes after `at` (an insert, delete, trim or speed
/// change), made the phone's way. The engine ripples every sync-locked lane, cutting whatever lies
/// across the edit point or refusing the edit; on the phone, items on the other lanes move whole.
/// So the other lanes and the captions sit the edit out; then whatever on the lanes starts at or
/// after `at` moves by as much as the main track grew or shrank, never back over the item before it
/// on its lane. The captions follow their footage afterwards (`speech::follow_main`). Several undo
/// steps: call it in an edit group.
fn ripple_main<R>(s: &mut Session, at: Tick, f: impl FnOnce(&mut Session) -> filmcraft_engine::Result<R>) -> filmcraft_engine::Result<R> {
    let before = main_end(s)?;
    let (lanes, captions) = s.edit_sequence("Ripple", |q, _, _| {
        let mut lanes = Vec::new();
        for t in q.video_tracks.iter_mut().skip(1).chain(q.audio_tracks.iter_mut().skip(1)).filter(|t| t.sync_lock) {
            t.sync_lock = false;
            lanes.push(t.id);
        }
        let mut captions = Vec::new();
        for t in q.caption_tracks.iter_mut().filter(|t| t.sync_lock) {
            t.sync_lock = false;
            captions.push(t.id);
        }
        Ok((lanes, captions))
    })?;
    let r = f(s)?;
    let delta = main_end(s)? - before;
    s.edit_sequence("Ripple", |q, _, _| {
        for t in q.video_tracks.iter_mut().chain(q.audio_tracks.iter_mut()).filter(|t| lanes.contains(&t.id)) {
            t.sync_lock = true;
            if delta != Tick::ZERO {
                shift_lane(t, at, delta);
            }
        }
        for t in q.caption_tracks.iter_mut().filter(|t| captions.contains(&t.id)) {
            t.sync_lock = true;
        }
        Ok(())
    })?;
    Ok(r)
}

/// Move every item on a lane that starts at or after `at` by `delta`, keeping each one after the
/// end of the item before it (and the start of the timeline).
fn shift_lane(t: &mut filmcraft_project::Track, at: Tick, delta: Tick) {
    t.sort();
    let mut prev_end = Tick::ZERO;
    let mut moved = Vec::new();
    for it in &mut t.items {
        if it.start >= at {
            let to = (it.start + delta).max(prev_end);
            moved.push((it.id, to - it.start));
            it.start = to;
        }
        prev_end = prev_end.max(it.end());
    }
    // a transition moves with the item it belongs to
    for tr in &mut t.transitions {
        let by = moved.iter().find(|(id, _)| tr.from == Some(*id) || tr.to == Some(*id)).map(|m| m.1);
        if let Some(by) = by {
            tr.start += by;
        }
    }
    t.sort();
}

/// Duplicate a clip right after itself (the main track makes room; other lanes find a free lane).
pub(crate) fn duplicate(s: &mut Session, id: u64) -> Result<u64, MobileError> {
    let (kind, track, c) = locate(s, id)?;
    let copy = s.edit_group("Duplicate", |s| {
        let (lane, insert) = if is_main(kind, track) {
            ("V1".to_string(), true)
        } else {
            (free_lane(s, kind, 1, Some(track), c.end(), c.end() + c.duration, None).map_err(|e| other(e.to_string()))?, false)
        };
        let moves = json!({"moves": [{"clip": c.id.0, "track": lane, "time": c.end().0}], "copy": true, "insert": insert});
        let r = if insert { ripple_main(s, c.end(), |s| s.execute("timeline.move", moves))? } else { s.execute("timeline.move", moves)? };
        let (_, seq) = s.active_sequence().map(|q| ((), q)).ok_or(EngineError::NoSequence)?;
        let copies = clips_of(&r, "copied");
        copies.iter().copied().find(|&x| model::find(seq, ClipId(x)).is_some_and(|f| f.kind == kind)).ok_or_else(|| other("the copy could not be placed"))
    })?;
    select_one(s, Some(copy))?;
    Ok(copy)
}

/// Replace a clip's media, keeping its place, length and settings. A video shorter than the clip
/// is refused.
pub(crate) fn replace(s: &mut Session, id: u64, path: &str) -> Result<(), MobileError> {
    let (_, _, c) = locate(s, id)?;
    let old = shown_size(s, c.item);
    let short = std::cell::Cell::new(false);
    // one undo step; a refused replace leaves nothing behind
    let r = s.edit_group("Replace", |s| {
        let items = import(s, &[path.to_string()]).map_err(|e| other(e.to_string()))?;
        let item = items.first().copied().ok_or_else(|| other("The file holds no media."))?;
        let r = s.execute("clip.replaceFromBin", json!({"clips": [c.id.0], "item": item}))?;
        if r["short"].as_array().is_some_and(|a| !a.is_empty()) {
            short.set(true);
            return Err(other("short"));
        }
        refit(s, c.id, old)
    });
    match r {
        Err(_) if short.get() => Err(MobileError::Command("The new video is shorter than the clip. Choose a longer video or trim the clip first.".into())),
        r => Ok(r?),
    }
}

/// The size a picture shows at, in the sequence's pixels (its display aspect), if it has one.
fn shown_size(s: &Session, item: ItemId) -> Option<(f64, f64)> {
    let seq = s.active_sequence()?;
    let size = s.project.source_size(item).filter(|&(w, h)| w > 0 && h > 0)?;
    let par = s.project.source_par(item).unwrap_or(seq.settings.par);
    Some(filmcraft_project::conformed_size(size, par, seq.settings.par))
}

/// After a replace, the new picture fits the frame the way the old one did (the clip's scale
/// mode): its anchor moves to the same point of the new picture, and its scale changes by as
/// much as the fit does. Without both sizes the clip stays as it is.
fn refit(s: &mut Session, clip: ClipId, old: Option<(f64, f64)>) -> filmcraft_engine::Result<()> {
    let Some((w0, h0)) = old else { return Ok(()) };
    let seq = s.active_sequence().ok_or(EngineError::NoSequence)?;
    let (fw, fh) = (f64::from(seq.settings.width.max(1)), f64::from(seq.settings.height.max(1)));
    let Some(item) = seq.find_item(clip).map(|(_, c)| c.item) else { return Ok(()) };
    let Some((w1, h1)) = shown_size(s, item) else { return Ok(()) };
    let fit = |w: f64, h: f64| (fw / w).min(fh / h);
    let k = fit(w1, h1) / fit(w0, h0);
    if !k.is_finite() || k <= 0.0 {
        return Ok(());
    }
    s.edit_sequence("Replace", |q, _, _| {
        let Some((_, it)) = q.find_item_mut(clip) else { return Ok(()) };
        // Scale to Frame Size refits on its own
        let refits = it.scale_to_frame;
        let Some(m) = it.effect_mut("motion") else { return Ok(()) };
        if let Some(p) = m.param_mut("anchor") {
            p.map_values(|v| match v {
                // an automatic (NaN) anchor stays automatic
                ParamValue::Vec2(mut v) => {
                    v.x = v.x / w0 * w1;
                    v.y = v.y / h0 * h1;
                    ParamValue::Vec2(v)
                }
                other => other,
            });
        }
        if !refits {
            for id in ["scale", "scale_width"] {
                if let Some(p) = m.param_mut(id) {
                    p.map_values(|v| match v {
                        ParamValue::Float(x) => ParamValue::Float((x * k).clamp(0.0, MAX_SCALE)),
                        other => other,
                    });
                }
            }
        }
        Ok(())
    })
}

/// Change a clip's speed (0.1× to 10×), keeping the pitch of its sound. The main track makes room
/// or closes up.
pub(crate) fn set_speed(s: &mut Session, id: u64, speed: f64) -> Result<(), MobileError> {
    let (kind, track, c) = locate(s, id)?;
    if !speed.is_finite() {
        return Err(MobileError::Command("Speed must be a number.".into()));
    }
    let speed = speed.clamp(MIN_SPEED, MAX_SPEED);
    let params = json!({"clips": [c.id.0], "speed": speed * 100.0, "ripple": is_main(kind, track), "maintainPitch": true});
    if is_main(kind, track) {
        s.edit_group("Speed", |s| ripple_main(s, c.end(), |s| s.execute("clip.speedDuration", params)))?;
    } else {
        ex(s, "clip.speedDuration", params)?;
    }
    Ok(())
}

/// Insert a 4-second still of the frame at the playhead into the clip.
pub(crate) fn freeze(s: &mut Session, id: u64) -> Result<u64, MobileError> {
    let (kind, track, c) = locate(s, id)?;
    let rate = s.sequence_rate();
    let ph = rate.snap(s.playhead());
    if ph < c.start || ph >= c.end() - rate.frame_duration() {
        return Err(MobileError::Command("Move the playhead into the clip to freeze a frame.".into()));
    }
    let r = s.edit_group("Freeze Frame", |s| {
        ripple_main(s, ph, |s| s.execute("clip.insertFrameHoldSegment", json!({"clip": c.id.0, "time": ph.0, "seconds": STILL_SECONDS})))
    })?;
    let held = r["clip"].as_u64();
    let _ = (kind, track);
    if let Some(h) = held {
        select_one(s, Some(h))?;
    }
    Ok(held.unwrap_or(id))
}

fn db_of(volume: f64) -> f64 {
    if !volume.is_finite() || volume <= 0.001 { SILENT_DB } else { 20.0 * volume.min(2.0).log10() }
}

/// Set a clip's volume (0.0 to 2.0). `merge` folds a slider drag into one undo step.
pub(crate) fn set_volume(s: &mut Session, id: u64, volume: f64, merge: bool) -> Result<(), MobileError> {
    let a = sound_clip(s, id)?;
    let db = db_of(volume);
    let key = format!("volume:{}", a.id.0);
    s.edit_sequence_as("Volume", merge.then_some(key.as_str()), |q, _, _| {
        let c = q.find_item_mut(a.id).map(|x| x.1).ok_or_else(|| other("clip gone"))?;
        let e = c.effects.iter_mut().find(|e| e.effect == "volume").ok_or_else(|| other("no volume"))?;
        if let Some(p) = e.param_mut("level") {
            p.value = filmcraft_project::ParamValue::Float(db);
            p.keyframes.clear();
        }
        Ok(())
    })?;
    settle(s);
    Ok(())
}

/// Mute or unmute a clip's sound.
pub(crate) fn set_muted(s: &mut Session, id: u64, muted: bool) -> Result<(), MobileError> {
    let a = sound_clip(s, id)?;
    s.edit_sequence(if muted { "Mute" } else { "Unmute" }, |q, _, _| {
        q.find_item_mut(a.id).map(|x| x.1).ok_or_else(|| other("clip gone"))?.enabled = !muted;
        Ok(())
    })?;
    settle(s);
    Ok(())
}

/// Set a clip's sound fades (ticks; 0 = none). Each is kept within the clip.
pub(crate) fn set_fades(s: &mut Session, id: u64, fade_in: i64, fade_out: i64) -> Result<(), MobileError> {
    let a = sound_clip(s, id)?;
    let rate = s.sequence_rate();
    let clamp = |t: i64| rate.snap(Tick(t.clamp(0, a.duration.0)));
    let (fi, fo) = (clamp(fade_in), clamp(fade_out));
    let (fi, fo) = if fi + fo > a.duration { (Tick(a.duration.0 / 2), Tick(a.duration.0 - a.duration.0 / 2)) } else { (fi, fo) };
    s.edit_sequence("Fade", |q, ctx, _| {
        let track = q.audio_tracks.iter().find(|t| t.items.iter().any(|c| c.id == a.id)).map(|t| t.id).ok_or_else(|| other("clip gone"))?;
        if let Some(t) = q.track_mut(track) {
            t.transitions.retain(|tr| !(tr.from.is_none() && tr.to == Some(a.id)) && !(tr.from == Some(a.id) && tr.to.is_none()));
        }
        let fade = |id: u64, start: Tick, dur: Tick, from, to| -> filmcraft_engine::Result<Transition> {
            let effect = filmcraft_project::find_effect("constant_power").ok_or_else(|| other("no fade"))?.instance();
            Ok(Transition { id: TransitionId(id), effect, start, duration: dur, from, to, align: Default::default(), reverse: false })
        };
        if fi > Tick::ZERO {
            let id = alloc(ctx);
            filmcraft_edit::add_transition(q, track, fade(id, a.start, fi, None, Some(a.id))?, ctx)?;
        }
        if fo > Tick::ZERO {
            let id = alloc(ctx);
            filmcraft_edit::add_transition(q, track, fade(id, a.end() - fo, fo, Some(a.id), None)?, ctx)?;
        }
        Ok(())
    })?;
    settle(s);
    Ok(())
}

fn alloc(ctx: &mut filmcraft_edit::EditCtx) -> u64 {
    let id = *ctx.next_id;
    *ctx.next_id += 1;
    id
}

/// Copy a video clip's sound to an audio lane of its own, under the clip. Returns the new clip.
pub(crate) fn extract_audio(s: &mut Session, id: u64) -> Result<u64, MobileError> {
    let (kind, _, c) = locate(s, id)?;
    if kind != TrackKind::Video {
        return Err(MobileError::Command("Only a video's sound can be extracted.".into()));
    }
    let a = sound_clip(s, id)?;
    let name = s.project.item(c.item).map(|p| p.name.clone()).unwrap_or_default();
    let copy = s.edit_group("Extract Audio", |s| {
        let lane = free_lane(s, TrackKind::Audio, 1, None, a.start, a.end(), None).map_err(|e| other(e.to_string()))?;
        let r = s.execute("timeline.move", json!({"moves": [{"clip": a.id.0, "track": lane, "time": a.start.0}], "copy": true, "linked": false}))?;
        let copy = clips_of(&r, "copied").first().copied().ok_or_else(|| other("the sound could not be copied"))?;
        s.edit_sequence("Extract Audio", |q, _, _| {
            let c = q.find_item_mut(ClipId(copy)).map(|x| x.1).ok_or_else(|| other("copy gone"))?;
            c.link = None;
            c.name = format!("Extracted {name}");
            Ok(())
        })?;
        Ok(copy)
    })?;
    settle(s);
    select_one(s, Some(copy))?;
    Ok(copy)
}

/// Where standard effects go in a clip's list: before the intrinsic Motion/Opacity.
fn effect_pos(c: &TrackItem) -> usize {
    c.effects.iter().position(|e| e.def().is_some_and(|d| d.intrinsic)).unwrap_or(c.effects.len())
}

fn resolve(inst: &mut [EffectInstance], w: u32, h: u32) {
    for e in inst {
        filmcraft_project::resolve_auto_points(e, (w, h), (w, h));
    }
}

/// Replace a clip's preset instances (`slots`) with `new`. Masks belong to the effect instance,
/// so carry masks across a replacement of the same effect (changing a filter/effect amount or
/// look must not silently delete them). Intrinsic Opacity masks are never in `slots`.
pub(crate) fn swap_instances(c: &mut TrackItem, slots: &[usize], new: Vec<EffectInstance>) {
    let at = slots.first().copied();
    let mut old_masks: Vec<(String, Vec<filmcraft_project::Mask>)> = Vec::new();
    let mut ordered = slots.to_vec();
    ordered.sort_unstable();
    ordered.dedup();
    for &i in &ordered {
        if let Some(old) = c.effects.get(i).filter(|e| !e.masks.is_empty()) {
            old_masks.push((old.effect.clone(), old.masks.clone()));
        }
    }
    for &i in ordered.iter().rev() {
        if i < c.effects.len() {
            c.effects.remove(i);
        }
    }
    let pos = at.filter(|&i| i <= c.effects.len()).unwrap_or_else(|| effect_pos(c));
    for (k, mut e) in new.into_iter().enumerate() {
        for (id, masks) in &mut old_masks {
            if *id == e.effect && !masks.is_empty() {
                e.masks.append(masks);
            }
        }
        c.effects.insert(pos + k, e);
    }
}

fn picture_clips(s: &Session, id: u64, all: bool) -> Result<Vec<ClipId>, MobileError> {
    if !all {
        let (kind, _, c) = locate(s, id)?;
        if kind != TrackKind::Video {
            return Err(MobileError::Command("Only pictures and videos take this.".into()));
        }
        return Ok(vec![c.id]);
    }
    let (_, seq) = model::sequence(s)?;
    let mut v = Vec::new();
    for t in &seq.video_tracks {
        for c in &t.items {
            let text = s.project.item(c.item).is_some_and(|p| matches!(p.kind, filmcraft_project::ItemKind::Graphic { .. }));
            if !text {
                v.push(c.id);
            }
        }
    }
    Ok(v)
}

/// Set a clip's filter (empty = none) at `amount` (0–200); `all` applies it to every picture.
pub(crate) fn set_filter(s: &mut Session, id: u64, filter: &str, amount: f64, merge: bool, all: bool) -> Result<(), MobileError> {
    let clips = picture_clips(s, id, all)?;
    let (_, seq) = model::sequence(s)?;
    let (w, h) = (seq.settings.width, seq.settings.height);
    let new = if filter.is_empty() {
        Vec::new()
    } else {
        presets::filter_instances(filter, amount).ok_or_else(|| MobileError::Command(format!("Unknown filter {filter}.")))?
    };
    let key = format!("filter:{id}");
    s.edit_sequence_as("Filter", (merge && !all).then_some(key.as_str()), |q, _, _| {
        for cid in &clips {
            let c = q.find_item_mut(*cid).map(|x| x.1).ok_or_else(|| other("clip gone"))?;
            let slots = presets::filter_slots(c);
            let mut inst = new.clone();
            resolve(&mut inst, w, h);
            swap_instances(c, &slots, inst);
        }
        Ok(())
    })?;
    settle(s);
    Ok(())
}

/// Set a clip's effect (empty = none) at `amount` (0–100); `all` applies it to every picture.
pub(crate) fn set_effect(s: &mut Session, id: u64, effect: &str, amount: f64, merge: bool, all: bool) -> Result<(), MobileError> {
    let clips = picture_clips(s, id, all)?;
    let (_, seq) = model::sequence(s)?;
    let (w, h) = (seq.settings.width, seq.settings.height);
    let new = if effect.is_empty() {
        Vec::new()
    } else {
        presets::effect_instances(effect, amount).ok_or_else(|| MobileError::Command(format!("Unknown effect {effect}.")))?
    };
    let key = format!("effect:{id}");
    s.edit_sequence_as("Effect", (merge && !all).then_some(key.as_str()), |q, _, _| {
        for cid in &clips {
            let c = q.find_item_mut(*cid).map(|x| x.1).ok_or_else(|| other("clip gone"))?;
            let slots = presets::effect_slots(c);
            let mut inst = new.clone();
            resolve(&mut inst, w, h);
            swap_instances(c, &slots, inst);
        }
        Ok(())
    })?;
    settle(s);
    Ok(())
}

/// Set a clip's opacity (0.0 to 1.0); `all` sets every picture's.
pub(crate) fn set_opacity(s: &mut Session, id: u64, opacity: f64, merge: bool, all: bool) -> Result<(), MobileError> {
    let clips = picture_clips(s, id, all)?;
    let v = if opacity.is_finite() { opacity.clamp(0.0, 1.0) * 100.0 } else { 100.0 };
    let key = format!("opacity:{id}");
    s.edit_sequence_as("Opacity", (merge && !all).then_some(key.as_str()), |q, _, _| {
        for cid in &clips {
            let c = q.find_item_mut(*cid).map(|x| x.1).ok_or_else(|| other("clip gone"))?;
            if let Some(p) = c.effects.iter_mut().find(|e| e.effect == "opacity").and_then(|e| e.param_mut("opacity")) {
                p.value = filmcraft_project::ParamValue::Float(v);
                p.keyframes.clear();
            }
        }
        Ok(())
    })?;
    settle(s);
    Ok(())
}

/// Flip a clip's picture left-right (`horizontal`) or upside down.
pub(crate) fn set_flip(s: &mut Session, id: u64, horizontal: bool, on: bool) -> Result<(), MobileError> {
    let clips = picture_clips(s, id, false)?;
    let fx = if horizontal { "horizontal_flip" } else { "vertical_flip" };
    let inst = filmcraft_project::find_effect(fx).ok_or_else(|| MobileError::Command("flip is missing".into()))?.instance();
    s.edit_sequence(if horizontal { "Mirror" } else { "Flip" }, |q, _, _| {
        for cid in &clips {
            let c = q.find_item_mut(*cid).map(|x| x.1).ok_or_else(|| other("clip gone"))?;
            c.effects.retain(|e| e.effect != fx);
            if on {
                let pos = effect_pos(c);
                c.effects.insert(pos, inst.clone());
            }
        }
        Ok(())
    })?;
    settle(s);
    Ok(())
}

/// A transition's slot on the main track: between `from` and `to`, or at the start (`from`
/// empty) or end (`to` empty).
fn transition_at(s: &Session, from: Option<u64>, to: Option<u64>) -> Option<TransitionId> {
    let (_, seq) = model::sequence(s).ok()?;
    seq.video_tracks.first()?.transitions.iter().find(|t| t.from.map(|c| c.0) == from && t.to.map(|c| c.0) == to).map(|t| t.id)
}

/// Footage a clip can still show past one of its edges, in timeline time: before its in point
/// (`head`) or after its out point. Pictures, blanks and text never run out.
fn spare(s: &Session, c: &TrackItem, head: bool) -> Tick {
    let endless = Tick(i64::MAX / 4);
    let media = match s.project.item(c.item).map(|p| &p.kind) {
        Some(filmcraft_project::ItemKind::Media(m))
            if matches!(m.media, filmcraft_project::MediaRef::File { .. }) && m.info.kind != filmcraft_media::MediaKind::Still =>
        {
            m.info.duration
        }
        _ => return endless,
    };
    let speed = if c.speed.is_finite() && c.speed.abs() > 1e-6 { c.speed.abs() } else { 1.0 };
    let used = Tick::from_seconds_f64(c.duration.seconds() * speed);
    let left = if head { c.source_in } else { media - (c.source_in + used) };
    Tick::from_seconds_f64(left.max(Tick::ZERO).seconds() / speed)
}

/// The longest transition a slot takes (seconds). Each side of a cut gives up to a third of its
/// clip plus the footage it has to spare (at most half the clip), less what a transition at its
/// other end already uses; the cut takes twice the smaller side. A start or end takes half the clip.
pub(crate) fn max_transition(s: &Session, from: Option<u64>, to: Option<u64>) -> f64 {
    let Ok((_, seq)) = model::sequence(s) else { return 0.0 };
    let clip = |c: Option<u64>| c.and_then(|c| seq.find_item(ClipId(c))).map(|(_, c)| c.clone());
    let transitions = seq.video_tracks.first().map(|t| t.transitions.clone()).unwrap_or_default();
    // what the transition at a clip's far end takes from it
    let far = |c: &TrackItem, head: bool| -> f64 {
        transitions
            .iter()
            .find(|t| if head { t.from == Some(c.id) && t.to.is_some() } else { t.to == Some(c.id) && t.from.is_some() })
            .map_or(0.0, |t| t.duration.seconds() / 2.0)
    };
    let side = |c: &TrackItem, head: bool| {
        let len = c.duration.seconds();
        ((len + spare(s, c, head).seconds().min(len / 2.0)) / 3.0 - far(c, head)).max(0.0)
    };
    let max = match (clip(from), clip(to)) {
        (Some(a), Some(b)) => 2.0 * side(&a, false).min(side(&b, true)),
        (Some(a), None) | (None, Some(a)) => a.duration.seconds() / 2.0,
        (None, None) => 0.0,
    };
    max.min(MAX_TRANSITION)
}

/// Make the footage a transition of `frames` on the cut between `from` and `to` needs: each side
/// lends half from its spare footage, and what it lacks is trimmed off the clip itself (the main
/// track closes up, so the video gets shorter). Starts and ends need nothing.
fn make_room(s: &mut Session, rate: FrameRate, from: Option<u64>, to: Option<u64>, frames: i64) -> Result<(), MobileError> {
    let (Some(a), Some(b)) = (from, to) else { return Ok(()) };
    let out_half = rate.tick_of(frames / 2);
    let in_half = rate.tick_of(frames - frames / 2);
    let (_, _, ca) = locate(s, a)?;
    let short = out_half - spare(s, &ca, false);
    if short > Tick::ZERO {
        trim(s, a, false, -short.0)?;
    }
    let (_, _, cb) = locate(s, b)?;
    let short = in_half - spare(s, &cb, true);
    if short > Tick::ZERO {
        trim(s, b, true, short.0)?;
    }
    Ok(())
}

/// Put transition `preset` (empty = none) of `seconds` in a main-track slot. Returns its id.
pub(crate) fn set_transition(s: &mut Session, from: Option<u64>, to: Option<u64>, preset: &str, seconds: f64, all: bool) -> Result<Option<u64>, MobileError> {
    if all {
        return set_transition_everywhere(s, preset, seconds);
    }
    let rate = s.sequence_rate();
    let r = s.edit_group(if preset.is_empty() { "Remove Transition" } else { "Transition" }, |s| place_transition(s, rate, from, to, preset, seconds))?;
    if let Some(t) = r {
        ex(s, "timeline.select", json!({"clips": [], "transitions": [t]}))?;
    }
    Ok(r)
}

fn place_transition(s: &mut Session, rate: FrameRate, from: Option<u64>, to: Option<u64>, preset: &str, seconds: f64) -> filmcraft_engine::Result<Option<u64>> {
    if let Some(old) = transition_at(s, from, to) {
        s.execute("sequence.removeTransition", json!({"transitions": [old.0]}))?;
    }
    if preset.is_empty() {
        return Ok(None);
    }
    let (effect, params) = presets::transition_params(preset).ok_or_else(|| other(format!("unknown transition {preset}")))?;
    let max = max_transition(s, from, to);
    if max < MIN_TRANSITION {
        return Err(other("The clips are too short for a transition."));
    }
    let secs = if seconds.is_finite() { seconds.clamp(MIN_TRANSITION, max) } else { DEFAULT_TRANSITION.min(max) };
    let frames = (secs * rate.as_f64()).round().max(1.0) as i64;
    make_room(s, rate, from, to, frames).map_err(|e| other(e.to_string()))?;
    let (clip, edge) = match (from, to) {
        (Some(f), _) => (f, "out"),
        (None, Some(t)) => (t, "in"),
        (None, None) => return Err(other("no slot")),
    };
    let r = s.execute("sequence.applyVideoTransition", json!({"clip": clip, "edge": edge, "effect": effect, "frames": frames, "params": params}))?;
    Ok(r["transition"].as_u64())
}

fn set_transition_everywhere(s: &mut Session, preset: &str, seconds: f64) -> Result<Option<u64>, MobileError> {
    let rate = s.sequence_rate();
    let (_, seq) = model::sequence(s)?;
    let clips = model::main_clips(seq);
    let cuts: Vec<(u64, u64)> = clips.windows(2).filter_map(|w| Some((w.first()?.2.0, w.get(1)?.2.0))).collect();
    s.edit_group("Apply To All", |s| {
        for (a, b) in cuts {
            // a cut too short for a transition is skipped
            let _ = place_transition(s, rate, Some(a), Some(b), preset, seconds);
        }
        Ok(())
    })?;
    Ok(None)
}

/// Change a transition's length (seconds).
pub(crate) fn set_transition_duration(s: &mut Session, id: u64, seconds: f64) -> Result<(), MobileError> {
    let (_, seq) = model::sequence(s)?;
    let tr = seq
        .video_tracks
        .first()
        .and_then(|t| t.transitions.iter().find(|t| t.id.0 == id))
        .cloned()
        .ok_or_else(|| MobileError::Command("That transition is gone.".into()))?;
    let max = max_transition(s, tr.from.map(|c| c.0), tr.to.map(|c| c.0));
    let secs = if seconds.is_finite() { seconds.clamp(MIN_TRANSITION, max.max(MIN_TRANSITION)) } else { DEFAULT_TRANSITION };
    let rate = s.sequence_rate();
    let frames = (secs * rate.as_f64()).round().max(1.0) as i64;
    let (from, to) = (tr.from.map(|c| c.0), tr.to.map(|c| c.0));
    s.edit_group("Transition Duration", |s| {
        make_room(s, rate, from, to, frames).map_err(|e| other(e.to_string()))?;
        s.execute("sequence.setTransition", json!({"transition": id, "frames": frames}))
    })?;
    Ok(())
}

/// Trim a clip's head (`head`) or tail by `delta` ticks (positive moves the edge right). The main
/// track ripples. Returns the change actually made.
pub(crate) fn trim(s: &mut Session, id: u64, head: bool, delta: i64) -> Result<i64, MobileError> {
    let (kind, track, c) = locate(s, id)?;
    let main = is_main(kind, track);
    // an overlay edge stops at its neighbours
    let delta = if main {
        delta
    } else {
        let (_, seq) = model::sequence(s)?;
        let tracks = if kind == TrackKind::Video { &seq.video_tracks } else { &seq.audio_tracks };
        let items = tracks.get(track).map(|t| t.items.as_slice()).unwrap_or(&[]);
        if head {
            let prev_end = items.iter().filter(|i| i.end() <= c.start && i.id != c.id).map(|i| i.end()).max().unwrap_or(Tick::ZERO);
            delta.max((prev_end - c.start).0)
        } else {
            let next_start = items.iter().filter(|i| i.start >= c.end() && i.id != c.id).map(|i| i.start).min();
            next_start.map(|n| delta.min((n - c.end()).0)).unwrap_or(delta)
        }
    };
    let rate = s.sequence_rate();
    // no edge moves further than the whole timeline plus the clip
    let limit = seq_end(s).0.saturating_add(c.duration.0).max(1);
    let delta = delta.clamp(-limit, limit);
    let delta = rate.snap_nearest(Tick(delta.abs())).0 * delta.signum();
    if delta == 0 {
        return Ok(0);
    }
    let params = json!({"clip": c.id.0, "edge": if head { "in" } else { "out" }, "mode": if main { "ripple" } else { "regular" }, "delta": delta});
    // trimming a head moves the clip's own footage too, and what lies over it
    let at = if head { c.start } else { c.end() };
    let r = if main { s.edit_group("Trim", |s| ripple_main(s, at, |s| s.execute("timeline.trim", params)))? } else { ex(s, "timeline.trim", params)? };
    Ok(r["delta"].as_i64().unwrap_or(0))
}

/// Move a main-track clip to position `index` among the main track's clips.
pub(crate) fn reorder(s: &mut Session, id: u64, index: u32) -> Result<(), MobileError> {
    let (kind, track, c) = locate(s, id)?;
    if !is_main(kind, track) {
        return Err(MobileError::Command("Only main-track clips are reordered.".into()));
    }
    let (_, seq) = model::sequence(s)?;
    let order: Vec<ClipId> = model::main_clips(seq).iter().map(|x| x.2).collect();
    let from = order.iter().position(|&x| x == c.id).ok_or_else(|| MobileError::Command("clip gone".into()))?;
    let to = (index as usize).min(order.len().saturating_sub(1));
    if from == to {
        return Ok(());
    }
    s.edit_group("Reorder", |s| {
        let locked = unlock_others(s)?;
        let r = (|| {
            // park the clip past the end, close the gap it left, then insert it at its new place
            let park = seq_end(s) + Tick::from_seconds_f64(1.0);
            s.execute("timeline.move", json!({"moves": [{"clip": c.id.0, "track": "V1", "time": park.0}], "insert": false}))?;
            s.execute("sequence.closeGap", json!({"track": "V1", "time": c.start.0}))?;
            let seq = s.active_sequence().ok_or(EngineError::NoSequence)?;
            let rest: Vec<(Tick, Tick)> = model::main_clips(seq).iter().filter(|x| x.2 != c.id).map(|x| (x.0, x.1)).collect();
            let at = rest.get(to).map(|x| x.0).or_else(|| rest.last().map(|x| x.1)).unwrap_or(Tick::ZERO);
            s.execute("timeline.move", json!({"moves": [{"clip": c.id.0, "track": "V1", "time": at.0}], "insert": true}))?;
            Ok(())
        })();
        relock(s, &locked)?;
        r
    })?;
    Ok(())
}

fn seq_end(s: &Session) -> Tick {
    s.active_sequence().map(|q| q.duration()).unwrap_or(Tick::ZERO)
}

/// Move an overlay or audio clip to `start` on lane `index` (1, 2… overlays; -1, -2… audio). A
/// lane that is taken there sends it to the nearest free one.
pub(crate) fn move_clip(s: &mut Session, id: u64, index: i32, start: i64) -> Result<(), MobileError> {
    let (kind, track, c) = locate(s, id)?;
    if is_main(kind, track) {
        return Err(MobileError::Command("Main-track clips are reordered instead.".into()));
    }
    let start = s.sequence_rate().snap_nearest(ticks(start));
    let want = usize::try_from(index.unsigned_abs()).ok().filter(|&w| w >= 1);
    let lane = free_lane(s, kind, 1, want, start, start + c.duration, Some(c.id))?;
    ex(s, "timeline.move", json!({"moves": [{"clip": c.id.0, "track": lane, "time": start.0}], "insert": false}))?;
    Ok(())
}

/// Change the frame's aspect: the new size keeps 1080 pixels on the short side.
/// Change the frame's shape. Every picture is sized to fit the frame when it's added (and a finger
/// scales it from there), so each one scales by how much its own fit changes, about the frame's
/// centre: a picture of the new shape fills the frame, and going to another shape and back gives
/// the same pictures.
pub(crate) fn set_aspect(s: &mut Session, num: u32, den: u32) -> Result<(), MobileError> {
    let (w, h) = frame_size(num, den);
    let (_, seq) = model::sequence(s)?;
    let (w0, h0) = (seq.settings.width, seq.settings.height);
    if (w0, h0) == (w, h) {
        return Ok(());
    }
    let (fw0, fh0, fw1, fh1) = (f64::from(w0.max(1)), f64::from(h0.max(1)), f64::from(w), f64::from(h));
    // a picture's fit in a frame, from its size at its display aspect
    let fit = |sw: f64, sh: f64, fw: f64, fh: f64| (fw / sw).min(fh / sh);
    let mut factors = std::collections::HashMap::new();
    for t in &seq.video_tracks {
        for it in &t.items {
            let k = match shown_size(s, it.item) {
                Some((sw, sh)) => fit(sw, sh, fw1, fh1) / fit(sw, sh, fw0, fh0),
                None => fit(fw0, fh0, fw1, fh1),
            };
            if k.is_finite() && k > 0.0 {
                factors.insert(it.id, k);
            }
        }
    }
    s.edit_group("Aspect Ratio", |s| {
        s.execute("sequence.settings", json!({"width": w, "height": h}))?;
        s.edit_sequence("Aspect Ratio", |q, _, _| {
            let (c0, c1) = ((fw0 / 2.0, fh0 / 2.0), (fw1 / 2.0, fh1 / 2.0));
            for it in q.video_tracks.iter_mut().flat_map(|t| t.items.iter_mut()) {
                let Some(&k) = factors.get(&it.id) else { continue };
                // Scale to Frame Size refits on its own
                let refits = it.scale_to_frame;
                let Some(m) = it.effect_mut("motion") else { continue };
                if let Some(p) = m.param_mut("position") {
                    p.map_values(|v| match v {
                        // an automatic (NaN) position follows the centre by itself
                        ParamValue::Vec2(mut v) => {
                            v.x = c1.0 + k * (v.x - c0.0);
                            v.y = c1.1 + k * (v.y - c0.1);
                            ParamValue::Vec2(v)
                        }
                        other => other,
                    });
                }
                if !refits {
                    for id in ["scale", "scale_width"] {
                        if let Some(p) = m.param_mut(id) {
                            p.map_values(|v| match v {
                                ParamValue::Float(x) => ParamValue::Float((x * k).clamp(0.0, MAX_SCALE)),
                                other => other,
                            });
                        }
                    }
                }
            }
            Ok(())
        })
    })?;
    Ok(())
}

/// The frame size for an aspect ratio: 1080 on the short side, even numbers.
pub(crate) fn frame_size(num: u32, den: u32) -> (u32, u32) {
    let (n, d) = (num.max(1) as f64, den.max(1) as f64);
    let even = |x: f64| ((x / 2.0).round() * 2.0).clamp(2.0, 7680.0) as u32;
    if n >= d { (even(1080.0 * n / d), 1080) } else { (1080, even(1080.0 * d / n)) }
}
