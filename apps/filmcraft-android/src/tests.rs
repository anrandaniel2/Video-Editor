use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use filmcraft_engine::export as fx;
use filmcraft_media::generators::GeneratorSource;
use filmcraft_media::{DemoScene, Generator, MediaSource};
use filmcraft_project::{ItemId, ItemKind, Label, MediaClip, MediaRef, Project, SequenceSettings, TrackKind};
use filmcraft_time::{FrameRate, TimeRange};

use super::*;
use crate::model::{ClipKind, LaneKind};

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn tmp() -> PathBuf {
    let d = std::env::temp_dir().join(format!("filmcraft-android-test-{}-{}", std::process::id(), COUNTER.fetch_add(1, Ordering::Relaxed)));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A `frames`-frame 24 fps 320×180 H.264 + AAC movie of a demo scene (with its sound).
fn movie(dir: &Path, name: &str, scene: DemoScene, frames: i64) -> String {
    let path = dir.join(name);
    let (w, h, rate) = (320, 180, FrameRate::FPS_24);
    let dur = rate.tick_of(frames);
    let src = GeneratorSource::new(Generator::Demo(scene), w, h, rate, dur);
    let mut p = Project::new("fixture");
    let clip = MediaClip {
        media: MediaRef::Generator(Generator::Demo(scene)),
        info: src.info().clone(),
        interpret: Default::default(),
        mark_in: None,
        mark_out: None,
        markers: vec![],
        offline: false,
        proxy: None,
        identity: None,
    };
    let item = p.add_item("scene", Label::Iris, ItemKind::Media(Box::new(clip)), None);
    let seq = p.new_sequence("s", SequenceSettings { width: w, height: h, frame_rate: rate, ..Default::default() }, 1, 1, None);
    let r = TimeRange::new(Tick::ZERO, dur);
    let mut v = p.make_track_item(item, TrackKind::Video, Tick::ZERO, r, rate).unwrap();
    for e in &mut v.effects {
        filmcraft_project::resolve_auto_points(e, (w, h), (w, h));
    }
    let a = p.make_track_item(item, TrackKind::Audio, Tick::ZERO, r, rate).unwrap();
    let q = p.sequence_mut(seq).unwrap();
    q.video_tracks[0].items.push(v);
    q.audio_tracks[0].items.push(a);
    let settings = fx::ExportSettings { format: fx::Format::H264, path: path.to_string_lossy().into_owned(), bitrate_kbps: 800, ..Default::default() };
    let shared: filmcraft_media::SharedSource = std::sync::Arc::new(src);
    let provider = move |id: ItemId| (id == item).then(|| shared.clone());
    fx::export(&std::sync::Arc::new(p), seq, &settings, &provider, &Default::default()).unwrap();
    path.to_string_lossy().into_owned()
}

/// A 64×64 PNG still.
fn picture(dir: &Path) -> String {
    let path = dir.join("still.png");
    let rgba: Vec<u8> = (0..64 * 64).flat_map(|i| [(i % 64 * 4) as u8, 120, 200, 255]).collect();
    std::fs::write(&path, fx::encode_png(rgba, 64, 64).unwrap()).unwrap();
    path.to_string_lossy().into_owned()
}

/// A mono 16-bit WAV of a 440 Hz tone, `seconds` long.
fn tone(dir: &Path, seconds: f64) -> String {
    let path = dir.join("tone.wav");
    let rate = 48_000u32;
    let n = (seconds * f64::from(rate)) as u32;
    let mut b = Vec::new();
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + n * 2).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&(n * 2).to_le_bytes());
    for i in 0..n {
        let v = ((f64::from(i) * 440.0 * std::f64::consts::TAU / f64::from(rate)).sin() * 12000.0) as i16;
        b.extend_from_slice(&v.to_le_bytes());
    }
    std::fs::write(&path, b).unwrap();
    path.to_string_lossy().into_owned()
}

const SEC: i64 = TICKS_PER_SECOND;

/// A portrait 30 fps project with two clips (2 s and 3 s) on the main track.
fn two_clips() -> (Arc<MobileSession>, PathBuf, Vec<u64>) {
    let d = tmp();
    let a = movie(&d, "a.mp4", DemoScene::Aurora, 48);
    let b = movie(&d, "b.mp4", DemoScene::Dunes, 72);
    let m = MobileSession::new();
    m.new_project("Test".into(), 1080, 1920, 30).unwrap();
    let ids = m.add_media(vec![a, b]).unwrap();
    (m, d, ids)
}

#[test]
fn the_media_a_project_starts_with_cannot_be_undone_away() {
    let (m, _d, ids) = two_clips();
    m.forget_history();
    assert!(!m.poll().can_undo);
    assert_eq!(m.undo(), None);
    assert_eq!(main_clips(&m).len(), ids.len());
    m.set_playhead(SEC);
    m.split(ids[0]).unwrap();
    assert!(m.undo().is_some());
    assert_eq!(m.undo(), None, "only the split was undoable");
    assert_eq!(main_clips(&m).len(), ids.len());
}

fn lane(t: &TimelineInfo, kind: LaneKind) -> &model::LaneInfo {
    t.lanes.iter().find(|l| l.kind == kind).unwrap()
}

fn main_clips(m: &MobileSession) -> Vec<model::ClipInfo> {
    lane(&m.timeline().unwrap(), LaneKind::Main).clips.clone()
}

fn clip(m: &MobileSession, id: u64) -> model::ClipInfo {
    m.timeline().unwrap().lanes.iter().flat_map(|l| l.clips.iter()).find(|c| c.id == id).cloned().unwrap()
}

#[test]
fn a_new_project_is_an_empty_portrait_sequence() {
    let m = MobileSession::new();
    let t = m.new_project("Oct 13, 2026".into(), 1080, 1920, 30).unwrap();
    assert_eq!((t.width, t.height, t.rate_num, t.rate_den, t.duration), (1080, 1920, 30, 1, 0));
    assert_eq!(t.lanes.len(), 1);
    assert_eq!(t.lanes[0].kind, LaneKind::Main);
    assert!(!m.poll().can_undo, "a new project starts with nothing to undo");
}

#[test]
fn media_goes_onto_the_main_track_back_to_back_and_the_first_is_selected() {
    let (m, _d, ids) = two_clips();
    let t = m.timeline().unwrap();
    let main = lane(&t, LaneKind::Main);
    assert_eq!(main.clips.iter().map(|c| c.id).collect::<Vec<_>>(), ids);
    assert_eq!(main.clips[0].start, 0);
    assert_eq!(main.clips[1].start, main.clips[0].duration);
    assert_eq!(main.clips[0].duration, 2 * SEC);
    assert!(main.clips.iter().all(|c| c.kind == ClipKind::Video && c.has_audio && c.audio_clip.is_some()));
    // the clips' sound is inside them, not a lane of its own
    assert!(t.lanes.iter().all(|l| l.kind != LaneKind::Audio));
    assert_eq!(t.selected, Some(ids[0]));
    assert_eq!(m.poll().undo_label.as_deref(), Some("Add Media"));
}

#[test]
fn new_media_goes_after_the_clip_under_the_playhead_and_before_one_starting_there() {
    let (m, d, ids) = two_clips();
    let c = movie(&d, "c.mp4", DemoScene::Plasma, 24);
    m.set_playhead(SEC); // inside the first clip
    let added = m.add_media(vec![c.clone()]).unwrap();
    assert_eq!(main_clips(&m).iter().map(|c| c.id).collect::<Vec<_>>(), vec![ids[0], added[0], ids[1]]);
    m.set_playhead(0); // exactly at the first clip
    let first = m.add_media(vec![c]).unwrap();
    assert_eq!(main_clips(&m)[0].id, first[0]);
}

#[test]
fn pictures_last_four_seconds_and_sounds_go_to_an_audio_lane() {
    let (m, d, _) = two_clips();
    let pic = m.add_media(vec![picture(&d)]).unwrap();
    let p = clip(&m, pic[0]);
    assert_eq!((p.kind, p.duration, p.has_audio), (ClipKind::Image, 4 * SEC, false));
    m.set_playhead(SEC);
    let snd = m.add_audio(vec![tone(&d, 1.5)]).unwrap();
    let t = m.timeline().unwrap();
    let audio = lane(&t, LaneKind::Audio);
    assert_eq!(audio.index, -1);
    assert_eq!(audio.clips[0].id, snd[0]);
    assert_eq!((audio.clips[0].kind, audio.clips[0].start), (ClipKind::Audio, SEC));
    // lanes run overlays, main, audio from top to bottom
    assert_eq!(t.lanes.last().map(|l| l.kind), Some(LaneKind::Audio));
}

#[test]
fn split_cuts_at_the_playhead_and_refuses_near_the_ends() {
    let (m, _d, ids) = two_clips();
    m.set_playhead(SEC);
    let right = m.split(ids[0]).unwrap();
    let main = main_clips(&m);
    assert_eq!(main.len(), 3);
    assert_eq!((main[0].duration, main[1].id, main[1].start), (SEC, right, SEC));
    assert_eq!(m.timeline().unwrap().selected, Some(right));
    // one frame from the start is too close
    m.set_playhead(SEC + SEC / 30);
    assert!(matches!(m.split(right), Err(MobileError::Command(_))));
}

#[test]
fn deleting_on_the_main_track_closes_the_gap() {
    let (m, _d, ids) = two_clips();
    m.delete_clip(ids[0]).unwrap();
    let main = main_clips(&m);
    assert_eq!(main.len(), 1);
    assert_eq!((main[0].id, main[0].start), (ids[1], 0));
    m.undo();
    assert_eq!(main_clips(&m).len(), 2);
}

#[test]
fn duplicate_puts_the_copy_right_after_the_original() {
    let (m, _d, ids) = two_clips();
    let copy = m.duplicate(ids[0]).unwrap();
    let main = main_clips(&m);
    assert_eq!(main.iter().map(|c| c.id).collect::<Vec<_>>(), vec![ids[0], copy, ids[1]]);
    assert_eq!((main[1].start, main[2].start), (2 * SEC, 4 * SEC));
    assert!(main[1].audio_clip.is_some(), "the copy keeps its sound");
}

#[test]
fn speed_changes_the_length_and_ripples_the_main_track() {
    let (m, _d, ids) = two_clips();
    m.set_speed(ids[0], 2.0).unwrap();
    let main = main_clips(&m);
    assert_eq!((main[0].duration, main[0].speed), (SEC, 2.0));
    assert_eq!(main[1].start, SEC);
    let keeps = m.lock().active_sequence().and_then(|q| q.find_item(filmcraft_project::ClipId(ids[0]))).is_some_and(|(_, c)| c.maintain_pitch);
    assert!(keeps, "the sound keeps its pitch");
    // out-of-range speeds are clamped, not refused
    m.set_speed(ids[0], 50.0).unwrap();
    assert_eq!(main_clips(&m)[0].speed, 10.0);
    assert!(m.set_speed(ids[0], f64::NAN).is_err());
}

#[test]
fn replace_keeps_place_and_length_and_refuses_a_shorter_video() {
    let (m, d, ids) = two_clips();
    m.forget_history();
    let longer = movie(&d, "longer.mp4", DemoScene::Dunes, 96);
    m.replace(ids[0], longer).unwrap();
    let main = main_clips(&m);
    assert_eq!((main.len(), main[0].start, main[0].duration), (2, 0, 2 * SEC));
    // a shorter one is refused, and leaves nothing to redo
    let shorter = movie(&d, "shorter.mp4", DemoScene::Aurora, 24);
    let before: Vec<(u64, i64, i64)> = main_clips(&m).iter().map(|c| (c.id, c.start, c.duration)).collect();
    assert!(matches!(m.replace(ids[1], shorter), Err(MobileError::Command(_))));
    assert_eq!(main_clips(&m).iter().map(|c| (c.id, c.start, c.duration)).collect::<Vec<_>>(), before);
    assert!(!m.poll().can_redo);
    // a picture of another shape fits the frame as the video did, centred on its own middle
    let motion = |m: &MobileSession| {
        let s = m.lock();
        let c = s.active_sequence().and_then(|q| q.find_item(filmcraft_project::ClipId(ids[0]))).map(|(_, c)| c.clone()).unwrap();
        let e = c.effects.iter().find(|e| e.effect == "motion").unwrap().clone();
        (e.vec2_at("anchor", c.source_in), e.f64_at("scale", c.source_in))
    };
    let (_, scale) = motion(&m);
    m.replace(ids[0], picture(&d)).unwrap();
    let (anchor, now) = motion(&m);
    assert_eq!((anchor.x, anchor.y), (32.0, 32.0), "the middle of the 64 px picture");
    assert!((now / scale - 5.0).abs() < 1e-9, "320 px wide fitted 1080, 64 px fits it at five times the scale: {scale} → {now}");
    assert_eq!(main_clips(&m)[0].duration, 2 * SEC);
    m.undo();
    assert_eq!(motion(&m).1, scale, "one undo step");
}

#[test]
fn freeze_inserts_a_four_second_still() {
    let (m, _d, ids) = two_clips();
    m.set_playhead(SEC);
    let held = m.freeze(ids[0]).unwrap();
    let t = m.timeline().unwrap();
    assert_eq!(t.duration, 9 * SEC);
    let h = clip(&m, held);
    assert_eq!((h.start, h.duration), (SEC, 4 * SEC));
}

#[test]
fn volume_mute_and_fades_round_trip() {
    let (m, _d, ids) = two_clips();
    m.set_volume(ids[0], 1.5, true).unwrap();
    m.set_volume(ids[0], 0.5, true).unwrap();
    let c = clip(&m, ids[0]);
    assert!((c.volume - 0.5).abs() < 1e-6, "{}", c.volume);
    // a drag is one undo step
    m.undo();
    assert!((clip(&m, ids[0]).volume - 1.0).abs() < 1e-6);
    m.set_volume(ids[0], 0.0, false).unwrap();
    assert_eq!(clip(&m, ids[0]).volume, 0.0);
    m.set_muted(ids[0], true).unwrap();
    assert!(clip(&m, ids[0]).muted);
    m.set_fades(ids[1], SEC / 2, SEC).unwrap();
    let c = clip(&m, ids[1]);
    assert_eq!((c.fade_in, c.fade_out), (SEC / 2, SEC));
    // the neighbour's sound is not crossfaded
    assert_eq!((clip(&m, ids[0]).fade_in, clip(&m, ids[0]).fade_out), (0, 0));
    m.set_fades(ids[1], 0, 0).unwrap();
    assert_eq!(clip(&m, ids[1]).fade_out, 0);
}

#[test]
fn extract_audio_makes_a_standalone_clip_under_the_video() {
    let (m, _d, ids) = two_clips();
    let a = m.extract_audio(ids[1]).unwrap();
    let t = m.timeline().unwrap();
    let audio = lane(&t, LaneKind::Audio);
    let c = audio.clips.iter().find(|c| c.id == a).unwrap();
    assert_eq!((c.start, c.duration, c.kind), (2 * SEC, 3 * SEC, ClipKind::Audio));
    assert!(c.name.starts_with("Extracted"));
    // the video keeps its own sound
    assert!(clip(&m, ids[1]).audio_clip.is_some());
}

#[test]
fn every_filter_effect_and_transition_reads_back_as_itself() {
    let (m, _d, ids) = two_clips();
    for f in filter_presets() {
        m.set_filter(ids[0], f.id.clone(), 140.0, false, false).unwrap();
        let c = clip(&m, ids[0]);
        assert_eq!(c.filter, f.id);
        assert!((c.filter_amount - 140.0).abs() < 1e-6, "{} {}", f.id, c.filter_amount);
    }
    m.set_filter(ids[0], String::new(), 0.0, false, false).unwrap();
    assert_eq!(clip(&m, ids[0]).filter, "");
    for e in effect_presets() {
        m.set_effect(ids[0], e.id.clone(), 30.0, false, false).unwrap();
        let c = clip(&m, ids[0]);
        assert_eq!(c.effect, e.id);
        assert!((c.effect_amount - 30.0).abs() < 1e-6, "{} {}", e.id, c.effect_amount);
        assert_eq!(c.filter, "");
    }
    for tr in transition_presets() {
        let id = m.set_transition(Some(ids[0]), Some(ids[1]), tr.id.clone(), 0.4, false).unwrap().unwrap();
        let t = m.timeline().unwrap();
        let found = t.transitions.iter().find(|x| x.id == id).unwrap();
        assert_eq!(found.preset, tr.id);
        assert_eq!(t.transitions.len(), 1, "a new transition replaces the old one");
    }
}

#[test]
fn a_filter_drag_is_one_undo_step_and_apply_to_all_reaches_every_picture() {
    let (m, _d, ids) = two_clips();
    m.set_filter(ids[0], "warm".into(), 100.0, false, false).unwrap();
    m.set_filter(ids[0], "warm".into(), 120.0, true, false).unwrap();
    m.set_filter(ids[0], "warm".into(), 160.0, true, false).unwrap();
    m.undo();
    assert_eq!(clip(&m, ids[0]).filter_amount, 100.0);
    m.set_filter(ids[0], "cool".into(), 80.0, false, true).unwrap();
    assert!(main_clips(&m).iter().all(|c| c.filter == "cool"));
}

#[test]
fn transitions_go_between_clips_and_at_the_ends() {
    let (m, _d, ids) = two_clips();
    let cut = m.set_transition(Some(ids[0]), Some(ids[1]), "dissolve".into(), 0.4, false).unwrap().unwrap();
    let intro = m.set_transition(None, Some(ids[0]), "fade".into(), 0.5, false).unwrap().unwrap();
    let t = m.timeline().unwrap();
    let c = t.transitions.iter().find(|x| x.id == cut).unwrap();
    assert_eq!((c.from, c.to, c.duration), (Some(ids[0]), Some(ids[1]), 12 * SEC / 30));
    // centred on the cut, which moved: the clips gave up the footage they lacked
    let first = &main_clips(&m)[0];
    assert_eq!(c.start + c.duration / 2, first.start + first.duration, "centred on the cut");
    let i = t.transitions.iter().find(|x| x.id == intro).unwrap();
    assert_eq!((i.from, i.to, i.start), (None, Some(ids[0]), 0));
    assert_eq!(t.selected_transition, Some(intro));
    m.select(None).unwrap();
    assert_eq!(m.timeline().unwrap().selected_transition, None, "selecting nothing clears a transition too");
    m.set_transition_duration(cut, 0.6).unwrap();
    assert_eq!(m.timeline().unwrap().transitions.iter().find(|x| x.id == cut).unwrap().duration, 18 * SEC / 30);
    // too long: held to the shorter clip
    m.set_transition_duration(cut, 9.0).unwrap();
    assert!(m.timeline().unwrap().transitions.iter().find(|x| x.id == cut).unwrap().duration <= 2 * SEC);
    m.set_transition(Some(ids[0]), Some(ids[1]), String::new(), 0.4, false).unwrap();
    assert_eq!(m.timeline().unwrap().transitions.len(), 1);
}

#[test]
fn opacity_mirror_and_flip_round_trip() {
    let (m, _d, ids) = two_clips();
    m.set_opacity(ids[0], 0.25, false, false).unwrap();
    m.set_flip(ids[0], true, true).unwrap();
    m.set_flip(ids[0], false, true).unwrap();
    let c = clip(&m, ids[0]);
    assert_eq!((c.opacity, c.mirrored, c.flipped), (0.25, true, true));
    m.set_flip(ids[0], true, false).unwrap();
    assert!(!clip(&m, ids[0]).mirrored);
    m.set_opacity(ids[0], 0.5, false, true).unwrap();
    assert!(main_clips(&m).iter().all(|c| c.opacity == 0.5));
}

#[test]
fn text_is_added_at_the_playhead_styled_and_blank_text_removes_it() {
    let (m, _d, _) = two_clips();
    m.set_playhead(SEC);
    assert_eq!(m.add_text("   ".into()).unwrap(), None);
    let id = m.add_text("Hello".into()).unwrap().unwrap();
    let t = m.timeline().unwrap();
    let overlay = lane(&t, LaneKind::Overlay);
    let c = overlay.clips.iter().find(|c| c.id == id).unwrap();
    assert_eq!((c.kind, c.start, c.duration, c.text.as_str()), (ClipKind::Text, SEC, 4 * SEC, "Hello"));
    assert_eq!(overlay.index, 1);
    let st = m.text_style(id).unwrap();
    assert_eq!((st.align, st.color, st.background), (text::TextAlign::Center, 0xffff_ffff, None));
    assert!((st.width - 1.0).abs() < 1e-6, "{}", st.width);
    let boxed = m.text_preset_style(id, "pop".into()).unwrap();
    m.set_text_style(id, boxed.clone(), false, false).unwrap();
    let back = m.text_style(id).unwrap();
    assert_eq!((back.background, back.font_style.as_str()), (boxed.background, "Bold"));
    m.set_text_style(id, text::TextStyle { width: 0.5, align: text::TextAlign::Left, ..back }, false, false).unwrap();
    let back = m.text_style(id).unwrap();
    assert_eq!(back.align, text::TextAlign::Left);
    assert!((back.width - 0.5).abs() < 0.01);
    m.set_text(id, "World".into()).unwrap();
    assert_eq!(clip(&m, id).text, "World");
    let p = m.placement(id).unwrap();
    assert!(p.width > 0.0 && p.height > 0.0, "{p:?}");
    m.set_text(id, "".into()).unwrap();
    assert!(m.timeline().unwrap().lanes.iter().all(|l| l.clips.iter().all(|c| c.id != id)));
    assert!(!m.fonts().is_empty());
}

#[test]
fn trimming_the_main_track_ripples_and_an_overlay_stops_at_its_neighbour() {
    let (m, _d, ids) = two_clips();
    let applied = m.trim(ids[0], false, -SEC / 2).unwrap();
    assert_eq!(applied, -SEC / 2);
    assert_eq!(main_clips(&m)[1].start, SEC + SEC / 2);
    let applied = m.trim(ids[1], true, SEC / 2).unwrap();
    assert_eq!(applied, SEC / 2);
    assert_eq!(main_clips(&m)[1].duration, 2 * SEC + SEC / 2);
    // overlays
    m.set_playhead(0);
    let a = m.add_text("A".into()).unwrap().unwrap();
    m.move_clip(a, 1, 0).unwrap();
    m.set_playhead(5 * SEC);
    let b = m.add_text("B".into()).unwrap().unwrap();
    m.move_clip(b, 1, 5 * SEC).unwrap();
    assert_eq!(clip(&m, b).start, 5 * SEC);
    let applied = m.trim(a, false, 3 * SEC).unwrap();
    assert_eq!(applied, SEC, "stops at the next text");
}

#[test]
fn reordering_moves_a_main_track_clip_and_keeps_the_track_gapless() {
    let (m, d, ids) = two_clips();
    m.set_playhead(5 * SEC);
    let c = m.add_media(vec![movie(&d, "c.mp4", DemoScene::Plasma, 24)]).unwrap()[0];
    m.reorder(ids[0], 2).unwrap();
    let main = main_clips(&m);
    assert_eq!(main.iter().map(|c| c.id).collect::<Vec<_>>(), vec![ids[1], c, ids[0]]);
    assert_eq!(main[0].start, 0);
    assert_eq!(main[1].start, main[0].duration);
    assert_eq!(main[2].start, main[1].start + main[1].duration);
    m.reorder(ids[0], 0).unwrap();
    assert_eq!(main_clips(&m)[0].id, ids[0]);
    assert_eq!(m.timeline().unwrap().duration, 6 * SEC);
    m.undo();
    assert_eq!(main_clips(&m)[2].id, ids[0], "a reorder is one undo step");
}

#[test]
fn blank_clips_and_aspect_ratios() {
    let (m, _d, ids) = two_clips();
    m.set_playhead(SEC);
    let blank = m.add_blank().unwrap();
    let b = clip(&m, blank);
    assert_eq!((b.kind, b.start, b.duration), (ClipKind::Blank, 2 * SEC, 4 * SEC));
    assert_eq!(main_clips(&m)[2].id, ids[1]);
    let t = m.set_aspect(16, 9).unwrap();
    assert_eq!((t.width, t.height), (1920, 1080));
    assert_eq!(frame_size_for(4, 5), vec![1080, 1350]);
    assert_eq!(frame_size_for(21, 9), vec![2520, 1080]);
}

#[test]
fn another_aspect_ratio_and_back_gives_the_same_pictures() {
    let (m, _d, ids) = two_clips();
    m.set_placement(ids[0], 300.0, 500.0, 2.0, 15.0, false).unwrap();
    let text = m.add_text("Hi".into()).unwrap().unwrap();
    let look =
        |m: &MobileSession| [ids[0], ids[1], text].map(|c| m.placement(c).unwrap()).map(|p| [p.x, p.y, p.width * p.scale, p.height * p.scale, p.rotation]);
    let before = look(&m);
    // a 16:9 picture fills a 16:9 frame
    m.set_aspect(16, 9).unwrap();
    let b = m.placement(ids[1]).unwrap();
    assert!((b.width * b.scale - 1920.0).abs() < 0.5 && (b.x, b.y) == (960.0, 540.0), "{b:?}");
    m.set_aspect(1, 1).unwrap();
    m.set_aspect(9, 16).unwrap();
    for (a, b) in before.iter().flatten().zip(look(&m).iter().flatten()) {
        assert!((a - b).abs() < 1e-6, "{before:?} became {:?}", look(&m));
    }
    m.undo();
    assert_eq!(m.timeline().unwrap().height, 1080, "each change is one undo step: back to 1:1");
}

#[test]
fn edits_across_a_text_never_fail_and_later_items_follow_the_main_track() {
    let (m, _d, ids) = two_clips();
    // a text across the cut at 2 s, and one later
    m.set_playhead(SEC);
    let across = m.add_text("across".into()).unwrap().unwrap();
    m.set_playhead(4 * SEC);
    let later = m.add_text("later".into()).unwrap().unwrap();
    let starts = |m: &MobileSession| (clip(m, across).start, clip(m, later).start);
    // the clips have no spare footage: the cut gives up 0.4 s
    m.set_transition(Some(ids[0]), Some(ids[1]), "dissolve".into(), 0.4, false).unwrap().unwrap();
    assert_eq!(starts(&m), (SEC, 4 * SEC - 12 * SEC / 30), "a transition");
    m.undo();
    m.set_speed(ids[0], 2.0).unwrap();
    assert_eq!(starts(&m), (SEC, 3 * SEC), "twice as fast");
    m.undo();
    m.trim(ids[0], false, -SEC / 2).unwrap();
    assert_eq!(starts(&m), (SEC, 4 * SEC - SEC / 2), "a trim");
    m.undo();
    m.delete_clip(ids[0]).unwrap();
    assert_eq!(starts(&m), (SEC, 2 * SEC), "a delete");
    assert_eq!(clip(&m, across).duration, 4 * SEC, "nothing is cut");
    m.undo();
    assert_eq!(starts(&m), (SEC, 4 * SEC));
}

#[test]
fn captions_follow_the_footage_they_were_made_from() {
    let (m, _d, ids) = two_clips();
    // a caption one second into the second clip (2 s to 5 s)
    m.execute("captions.add".into(), json!({"seconds": 3.0, "durationSeconds": 1.0}).to_string()).unwrap();
    let at = |m: &MobileSession| m.timeline().unwrap().captions.iter().map(|c| c.start).collect::<Vec<_>>();
    m.delete_clip(ids[0]).unwrap();
    assert_eq!(at(&m), vec![SEC], "the clip before it went");
    m.undo();
    assert_eq!((at(&m), main_clips(&m).len()), (vec![3 * SEC], 2), "one undo step");
    m.set_speed(ids[1], 2.0).unwrap();
    assert_eq!(at(&m), vec![2 * SEC + SEC / 2], "twice as fast");
    m.undo();
    m.trim(ids[1], true, SEC / 2).unwrap();
    assert_eq!(at(&m), vec![2 * SEC + SEC / 2], "half a second trimmed off its head");
    m.undo();
    m.reorder(ids[1], 0).unwrap();
    assert_eq!(at(&m), vec![SEC], "moved first");
    m.undo();
    m.delete_clip(ids[1]).unwrap();
    assert!(at(&m).is_empty(), "its footage is gone");
    m.undo();
    assert_eq!(at(&m), vec![3 * SEC]);
}

#[test]
fn a_main_track_insert_moves_later_items_whole_and_cuts_none() {
    let (m, _d, ids) = two_clips();
    // a text and a caption across the cut at 2 s, and one of each after it
    m.set_playhead(SEC);
    let across = m.add_text("across".into()).unwrap().unwrap();
    m.set_playhead(3 * SEC);
    let later = m.add_text("later".into()).unwrap().unwrap();
    m.execute("captions.add".into(), json!({"seconds": 1.0, "durationSeconds": 2.0}).to_string()).unwrap();
    m.execute("captions.add".into(), json!({"seconds": 3.0, "durationSeconds": 1.0}).to_string()).unwrap();
    let above = |m: &MobileSession| m.timeline().unwrap().lanes.iter().filter(|l| l.kind != LaneKind::Main).map(|l| l.clips.len()).sum::<usize>();
    let captions = |m: &MobileSession| m.timeline().unwrap().captions.iter().map(|c| (c.start, c.duration)).collect::<Vec<_>>();
    let texts = above(&m);
    let check = |m: &MobileSession, grew: i64, what: &str| {
        assert_eq!((clip(m, across).start, clip(m, across).duration), (SEC, 4 * SEC), "{what}: an item across the insert stays whole");
        assert_eq!(clip(m, later).start, 3 * SEC + grew, "{what}: a later item moves with the main track");
        assert_eq!(above(m), texts, "{what}: nothing is cut");
        assert_eq!(captions(m), vec![(SEC, 2 * SEC), (3 * SEC + grew, SEC)], "{what}: captions move the same way");
    };
    // the blank goes after the clip under the playhead: at 2 s
    m.set_playhead(SEC);
    m.add_blank().unwrap();
    check(&m, 4 * SEC, "blank");
    m.undo();
    check(&m, 0, "undone");
    m.select(Some(ids[0])).unwrap();
    m.duplicate(ids[0]).unwrap();
    check(&m, 2 * SEC, "duplicate");
    m.undo();
    // the still goes in at the playhead, after the start of the text across it
    m.set_playhead(SEC + SEC / 2);
    m.freeze(ids[0]).unwrap();
    check(&m, 4 * SEC, "freeze");
    m.undo();
    check(&m, 0, "each insert is one undo step");
}

#[test]
fn the_renderer_draws_mixes_and_reads_waveforms_without_the_session() {
    let (m, _d, ids) = two_clips();
    let r = m.renderer().unwrap();
    let f = r.frame(SEC / 2, 480, true, None).unwrap();
    assert_eq!((f.width, f.height), (270, 480));
    assert_eq!(f.rgba.len(), 270 * 480 * 4);
    // an edit while the snapshot is held doesn't change it
    m.delete_clip(ids[0]).unwrap();
    assert!(r.revision() < m.poll().revision);
    let pcm = r.mix(0, 4800);
    assert_eq!(pcm.len(), 4800 * 8);
    assert!(pcm.chunks(4).any(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]).abs() > 1e-4), "the demo scene has sound");
    let c = clip(&m, ids[1]);
    let peaks = r.peaks(c.item, 0, SEC, 50);
    assert_eq!(peaks.len(), 50);
    assert!(peaks.iter().all(|p| (0.0..=1.0).contains(p)));
    let thumb = r.thumbnail(c.item, 0, 64, None).unwrap();
    assert!(thumb.width <= 128 && thumb.width > 0);
    let token = render::CancelToken::new();
    token.cancel();
    assert!(r.frame(0, 480, false, Some(token)).is_err());
}

#[test]
fn a_saved_project_opens_with_everything_in_place() {
    let (m, d, ids) = two_clips();
    m.set_filter(ids[0], "vivid".into(), 60.0, false, false).unwrap();
    m.set_transition(Some(ids[0]), Some(ids[1]), "push_left".into(), 0.4, false).unwrap();
    let path = d.join("project.fcproj").to_string_lossy().into_owned();
    m.save_project(path.clone()).unwrap();
    assert!(!m.poll().dirty);
    let n = MobileSession::new();
    let t = n.open_project(path).unwrap();
    assert_eq!(lane(&t, LaneKind::Main).clips.len(), 2);
    assert_eq!(lane(&t, LaneKind::Main).clips[0].filter, "vivid");
    assert_eq!(t.transitions[0].preset, "push_left");
}

#[test]
fn bad_input_is_an_error_not_a_crash() {
    let (m, _d, ids) = two_clips();
    assert!(m.split(999_999).is_err());
    assert!(m.set_filter(ids[0], "nope".into(), 50.0, false, false).is_err());
    assert!(m.set_effect(ids[0], "nope".into(), 50.0, false, false).is_err());
    assert!(m.set_transition(Some(ids[0]), Some(ids[1]), "nope".into(), 0.4, false).is_err());
    assert!(m.set_volume(ids[0], f64::INFINITY, false).is_ok());
    assert!(m.trim(ids[0], true, i64::MIN).is_ok());
    assert!(m.add_media(vec!["/no/such/file.mp4".into()]).is_err());
    assert!(m.execute("no.such.command".into(), String::new()).is_err());
    assert!(m.execute("timeline.razor".into(), "not json".into()).is_err());
    assert!(m.renderer().unwrap().frame(i64::MAX, 0, false, None).is_ok());
    assert!(m.renderer().unwrap().peaks(999_999, 0, SEC, 10).is_empty());
}

#[test]
fn the_export_plan_keeps_the_aspect_and_an_export_finishes() {
    let (m, d, _) = two_clips();
    let settings = |short, q| export::ExportSettings { short_side: short, fps: 30, quality: q };
    let p = m.export_plan(settings(1080, export::ExportQuality::High)).unwrap();
    assert_eq!((p.width, p.height), (1080, 1920));
    let low = m.export_plan(settings(1080, export::ExportQuality::Low)).unwrap();
    assert!(low.estimated_bytes < p.estimated_bytes);
    assert_eq!(m.export_plan(settings(2160, export::ExportQuality::High)).unwrap().height, 3840);
    let out = d.join("out.mp4").to_string_lossy().into_owned();
    let job = m.start_export(out.clone(), settings(720, export::ExportQuality::Low)).unwrap();
    let start = std::time::Instant::now();
    let status = loop {
        let st = m.export_status(job).unwrap();
        if st.finished || start.elapsed().as_secs() > 120 {
            break st;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    };
    assert!(status.finished && status.error.is_none(), "{status:?}");
    assert!(std::fs::metadata(&out).unwrap().len() > 1000);
}

/// Opens a real clip when `FILMCRAFT_TEST_CLIP` names one, checks the frame size and timing.
#[test]
fn a_real_clip_opens_and_plays_when_one_is_given() {
    let Ok(path) = std::env::var("FILMCRAFT_TEST_CLIP") else { return };
    let m = MobileSession::new();
    let info = m.open_clip(path).unwrap();
    assert!(info.width > 0 && info.duration > 0, "{info:?}");
    let frame = TICKS_PER_SECOND * info.rate_den / info.rate_num;
    let r = m.renderer().unwrap();
    let start = std::time::Instant::now();
    for i in 0..30 {
        let f = r.frame(i * frame, 1080, false, None).unwrap();
        assert_eq!(f.time, i * frame);
    }
    eprintln!("{}x{}: {:.1} ms a frame", info.width, info.height, start.elapsed().as_secs_f64() * 1000.0 / 30.0);
}

#[test]
fn clips_are_moved_scaled_and_turned_on_the_frame_as_one_undo_step() {
    let (m, _d, ids) = two_clips();
    let p = m.placement(ids[0]).unwrap();
    // a 16:9 clip fitted across a 9:16 frame, centred
    assert_eq!((p.x, p.y), (540.0, 960.0));
    assert!((p.width * p.scale - 1080.0).abs() < 2.0, "{p:?}");
    m.set_placement(ids[0], 300.0, 700.0, p.scale * 0.5, 15.0, true).unwrap();
    m.set_placement(ids[0], 320.0, 720.0, p.scale * 0.5, 30.0, true).unwrap();
    let q = m.placement(ids[0]).unwrap();
    assert_eq!((q.x, q.y, q.rotation), (320.0, 720.0, 30.0));
    assert!((q.scale - p.scale * 0.5).abs() < 1e-9);
    m.undo();
    assert_eq!(m.placement(ids[0]).unwrap().x, 540.0);
    assert!(m.set_placement(ids[0], f64::NAN, 0.0, 1.0, 0.0, false).is_err());
    // text moves by its centre too
    let t = m.add_text("Hi".into()).unwrap().unwrap();
    let before = m.placement(t).unwrap();
    m.set_placement(t, before.x + 100.0, before.y - 50.0, before.scale, 0.0, false).unwrap();
    let after = m.placement(t).unwrap();
    assert!((after.x - before.x - 100.0).abs() < 2.0 && (after.y - before.y + 50.0).abs() < 2.0, "{before:?} {after:?}");
}

/// A square, full-frame still whose pixels are easy to reason about in mask tests.
fn mask_still() -> (Arc<MobileSession>, PathBuf, u64) {
    let d = tmp();
    let path = picture(&d);
    let m = MobileSession::new();
    m.new_project("Masks".into(), 64, 64, 30).unwrap();
    let id = m.add_media(vec![path]).unwrap()[0];
    (m, d, id)
}

fn mask_settings(feather: f64) -> masks::MaskSettings {
    masks::MaskSettings { combine: masks::MaskCombine::Add, inverted: false, feather, opacity: 100.0, expansion: 0.0 }
}

fn mask_rect(x0: f64, y0: f64, x1: f64, y1: f64) -> masks::MaskPath {
    masks::MaskPath {
        closed: true,
        vertices: [(x0, y0), (x1, y0), (x1, y1), (x0, y1)].into_iter().map(|(x, y)| masks::MaskVertex {
            point: masks::MaskPoint { x, y },
            handle_in: masks::MaskPoint { x: 0.0, y: 0.0 },
            handle_out: masks::MaskPoint { x: 0.0, y: 0.0 },
        }).collect(),
    }
}

fn mask_pixel(f: &Frame, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * f.width + x) * 4) as usize;
    [f.rgba[i], f.rgba[i + 1], f.rgba[i + 2], f.rgba[i + 3]]
}

#[test]
fn clip_masks_cut_out_the_picture_and_every_shape_and_control_round_trips() {
    let (m, _d, id) = mask_still();
    assert!(m.clip_masks(id).unwrap().masks.is_empty());
    let st = m.add_clip_mask(id, masks::MaskShape::Rectangle, vec![]).unwrap();
    assert_eq!(st.masks.len(), 1);
    assert_eq!(st.masks[0].path.vertices.len(), 4);
    m.set_clip_mask(id, 0, mask_settings(0.0), None).unwrap();
    m.set_clip_mask_path(id, 0, mask_rect(16.0, 16.0, 48.0, 48.0), None).unwrap();
    let f = m.renderer().unwrap().frame(0, 64, false, None).unwrap();
    assert!(mask_pixel(&f, 2, 2)[..3].iter().all(|&v| v < 3), "outside is cut out: {:?}", mask_pixel(&f, 2, 2));
    assert!(mask_pixel(&f, 32, 32)[0] > 80, "inside keeps the picture");

    let mut settings = mask_settings(0.0);
    settings.inverted = true;
    m.set_clip_mask(id, 0, settings, None).unwrap();
    let f = m.renderer().unwrap().frame(0, 64, false, None).unwrap();
    assert!(mask_pixel(&f, 2, 2)[0] > 0 && mask_pixel(&f, 32, 32)[0] < 3, "inversion swaps inside and outside");
    settings.inverted = false;
    settings.combine = masks::MaskCombine::None;
    m.set_clip_mask(id, 0, settings, None).unwrap();
    let f = m.renderer().unwrap().frame(0, 64, false, None).unwrap();
    assert!(mask_pixel(&f, 2, 2)[0] > 0, "None bypasses the mask without deleting it");

    // Multiple masks and the engine's combine modes remain ordinary project data.
    settings.combine = masks::MaskCombine::Add;
    m.set_clip_mask(id, 0, settings, None).unwrap();
    let st = m.add_clip_mask(id, masks::MaskShape::Ellipse, vec![]).unwrap();
    assert_eq!(st.masks.len(), 2);
    assert_eq!(st.masks[1].path.vertices.len(), 4);
    let st = m.add_clip_mask(id, masks::MaskShape::Polygon, vec![]).unwrap();
    assert_eq!(st.masks.last().unwrap().path.vertices.len(), 5);
    let triangle = vec![masks::MaskPoint { x: 4.0, y: 4.0 }, masks::MaskPoint { x: 20.0, y: 6.0 }, masks::MaskPoint { x: 10.0, y: 22.0 }];
    let st = m.add_clip_mask(id, masks::MaskShape::Polygon, triangle).unwrap();
    assert_eq!(st.masks.len(), 4);
    for combine in [masks::MaskCombine::Subtract, masks::MaskCombine::Intersect, masks::MaskCombine::Lighten, masks::MaskCombine::Darken, masks::MaskCombine::Difference] {
        settings.combine = combine;
        m.set_clip_mask(id, 3, settings, None).unwrap();
        assert_eq!(m.clip_masks(id).unwrap().masks[3].settings.combine, combine);
    }
    assert!(m.set_clip_mask(id, 99, settings, None).is_err());
    assert!(m.set_clip_mask_path(id, 0, mask_rect(0.0, 0.0, f64::NAN, 4.0), None).is_err());
    m.delete_clip_mask(id, 3).unwrap();
    assert_eq!(m.clip_masks(id).unwrap().masks.len(), 3);
    m.clear_clip_masks(id).unwrap();
    assert!(m.clip_masks(id).unwrap().masks.is_empty());
    m.undo();
    assert_eq!(m.clip_masks(id).unwrap().masks.len(), 3, "clear is one undo step");
}

#[test]
fn mask_gestures_merge_and_animated_paths_save_and_reopen() {
    let (m, d, id) = mask_still();
    m.add_clip_mask(id, masks::MaskShape::Rectangle, vec![]).unwrap();
    let original = mask_rect(10.0, 10.0, 30.0, 30.0);
    m.set_clip_mask_path(id, 0, original.clone(), None).unwrap();
    let before = m.poll().revision;
    for step in 1..=5 {
        m.set_clip_mask_path(id, 0, mask_rect(10.0 + f64::from(step), 10.0, 30.0 + f64::from(step), 30.0), Some("drag-one".into())).unwrap();
    }
    assert_eq!(m.poll().revision, before + 5);
    m.undo();
    assert_eq!(m.clip_masks(id).unwrap().masks[0].path, original, "the whole drag is one undo step");
    m.redo();
    assert!((m.clip_masks(id).unwrap().masks[0].path.vertices[0].point.x - 15.0).abs() < 1e-9);
    // A separate token is a separate edit even when no other command came between.
    m.set_clip_mask_path(id, 0, mask_rect(20.0, 10.0, 40.0, 30.0), Some("drag-two".into())).unwrap();
    m.undo();
    assert!((m.clip_masks(id).unwrap().masks[0].path.vertices[0].point.x - 15.0).abs() < 1e-9);

    m.set_playhead(0);
    m.set_clip_mask_path(id, 0, mask_rect(10.0, 10.0, 30.0, 30.0), None).unwrap();
    m.execute("effects.toggleAnimation".into(), json!({"clip": id, "effect": "opacity", "mask": 0, "param": "path"}).to_string()).unwrap();
    m.set_playhead(SEC);
    m.move_clip_mask(id, 0, 20.0, 0.0, None).unwrap();
    let st = m.clip_masks(id).unwrap();
    assert!(st.masks[0].animated);
    assert_eq!(st.masks[0].path_keyframes.len(), 2);
    m.set_playhead(SEC / 2);
    let mid = m.clip_masks(id).unwrap();
    assert!((mid.masks[0].path.vertices[0].point.x - 20.0).abs() < 1e-6, "path interpolates at the playhead");

    let path = d.join("masks.fcproj").to_string_lossy().into_owned();
    m.save_project(path.clone()).unwrap();
    let opened = MobileSession::new();
    opened.open_project(path).unwrap();
    let c = main_clips(&opened)[0].id;
    opened.set_playhead(SEC / 2);
    assert_eq!(opened.clip_masks(c).unwrap().masks, mid.masks);
    assert_eq!(opened.clip_masks(c).unwrap().transform, mid.transform);
}

#[test]
fn mask_overlay_transform_follows_placement_and_mirror() {
    let (m, _d, ids) = two_clips();
    let id = ids[0];
    m.add_clip_mask(id, masks::MaskShape::Ellipse, vec![]).unwrap();
    let p = m.placement(id).unwrap();
    m.set_placement(id, 300.0, 700.0, p.scale * 0.5, 30.0, false).unwrap();
    let st = m.clip_masks(id).unwrap();
    let center = filmcraft_geom::Vec2::new(f64::from(st.source_width) / 2.0, f64::from(st.source_height) / 2.0);
    let mapped = filmcraft_geom::Affine { a: st.transform.a, b: st.transform.b, c: st.transform.c, d: st.transform.d, e: st.transform.e, f: st.transform.f }.apply(center);
    assert!((mapped.x - 300.0).abs() < 1e-6 && (mapped.y - 700.0).abs() < 1e-6);
    m.set_flip(id, true, true).unwrap();
    let flipped = m.clip_masks(id).unwrap();
    assert!(flipped.transform.a * st.transform.a + flipped.transform.b * st.transform.b < 0.0, "mirror reverses the source x axis");
    assert!(m.renderer().unwrap().frame(SEC / 2, 180, false, None).is_ok());
}

#[test]
fn mask_vertex_tools_keep_a_closed_shape_and_reject_other_clip_kinds() {
    let (m, d, id) = mask_still();
    m.add_clip_mask(id, masks::MaskShape::Polygon, vec![]).unwrap();
    let st = m.add_clip_mask_vertex(id, 0, 0, masks::MaskPoint { x: 32.0, y: 4.0 }).unwrap();
    assert_eq!(st.masks[0].path.vertices.len(), 6);
    let st = m.remove_clip_mask_vertex(id, 0, 1).unwrap();
    assert_eq!(st.masks[0].path.vertices.len(), 5);
    for i in (3..5).rev() { m.remove_clip_mask_vertex(id, 0, i).unwrap(); }
    assert!(m.remove_clip_mask_vertex(id, 0, 0).is_err(), "a mask keeps at least three points");
    let text = m.add_text("hello".into()).unwrap().unwrap();
    assert!(m.clip_masks(text).is_err());
    let sound = m.add_audio(vec![tone(&d, 0.2)]).unwrap()[0];
    assert!(m.add_clip_mask(sound, masks::MaskShape::Ellipse, vec![]).is_err());
}

#[test]
fn changing_filters_and_effects_keeps_clip_and_effect_masks() {
    let (m, _d, id) = mask_still();
    m.add_clip_mask(id, masks::MaskShape::Rectangle, vec![]).unwrap();
    m.set_clip_mask_path(id, 0, mask_rect(12.0, 12.0, 52.0, 52.0), None).unwrap();
    let before = m.clip_masks(id).unwrap().masks;
    m.set_filter(id, "vivid".into(), 80.0, false, false).unwrap();
    m.set_effect(id, "gaussian".into(), 60.0, false, false).unwrap();
    assert_eq!(m.clip_masks(id).unwrap().masks, before, "preset swaps leave Opacity masks alone");
    // A mask on the replaced standard effect itself is carried to the new instance of that effect.
    m.execute("masks.add".into(), json!({"clip": id, "effect": "gaussian_blur", "shape": "ellipse"}).to_string()).unwrap();
    m.set_effect(id, "gaussian".into(), 30.0, false, false).unwrap();
    let listed: serde_json::Value = serde_json::from_str(&m.execute("masks.list".into(), json!({"clip": id}).to_string()).unwrap()).unwrap();
    let arr = listed["masks"].as_array().unwrap();
    assert!(arr.iter().any(|v| v["effectId"].as_str() == Some("gaussian_blur")), "effect mask survives amount change: {arr:?}");
    assert_eq!(m.clip_masks(id).unwrap().masks, before, "the phone snapshot still exposes only clip masks");
    let preview = m.renderer().unwrap().look_preview(id, "noir".into(), 100.0, String::new(), 0.0, 0, 64).unwrap();
    assert!(mask_pixel(&preview, 2, 2)[..3].iter().all(|&v| v < 3), "look preview keeps the cut-out");
    assert!(mask_pixel(&preview, 32, 32)[3] > 0);
}

#[test]
fn exported_video_keeps_the_mask_from_the_preview() {
    let (m, d, id) = mask_still();
    m.add_clip_mask(id, masks::MaskShape::Rectangle, vec![]).unwrap();
    m.set_clip_mask(id, 0, mask_settings(0.0), None).unwrap();
    m.set_clip_mask_path(id, 0, mask_rect(16.0, 16.0, 48.0, 48.0), None).unwrap();
    let preview = m.renderer().unwrap().frame(0, 64, false, None).unwrap();
    let out = d.join("masked.mp4").to_string_lossy().into_owned();
    // The phone's own export path (not a separate renderer): 144 px is its smallest short side.
    let job = m.start_export(out.clone(), export::ExportSettings { short_side: 144, fps: 30, quality: export::ExportQuality::High }).unwrap();
    let start = std::time::Instant::now();
    let status = loop {
        let st = m.export_status(job).unwrap();
        if st.finished || start.elapsed().as_secs() > 120 { break st; }
        std::thread::sleep(std::time::Duration::from_millis(100));
    };
    assert!(status.finished && status.error.is_none(), "{status:?}");
    let src = filmcraft_media::open_path(Path::new(&out), &[]).unwrap();
    let frame = src.video_frame(filmcraft_media::FrameRequest { time: Tick::ZERO, scale: 1.0 }).unwrap();
    assert_eq!((frame.width, frame.height), (144, 144));
    let rgba = frame.to_rgba8();
    let at = |x: usize, y: usize| rgba[(y * frame.width as usize + x) * 4];
    assert!(at(4, 4) < 20, "exported corner is black");
    assert!(at(72, 72) > 80, "exported center keeps the picture");
    assert!(mask_pixel(&preview, 32, 32)[0] > 80);
}

#[test]
fn look_and_transition_previews_draw_the_clip_with_the_candidate() {
    let (m, _d, ids) = two_clips();
    let r = m.renderer().unwrap();
    let plain = r.frame(SEC / 2, 120, false, None).unwrap();
    let noir = r.look_preview(ids[0], "noir".into(), 100.0, String::new(), 0.0, SEC / 2, 120).unwrap();
    // cut to the clip's own 16:9 picture, not the portrait frame around it
    assert!(noir.width > noir.height && (f64::from(noir.width) / f64::from(noir.height) - 16.0 / 9.0).abs() < 0.1, "{}x{}", noir.width, noir.height);
    // grey: red, green and blue close together
    let grey =
        noir.rgba.chunks(4).filter(|p| p[3] > 0).all(|p| (i32::from(p[0]) - i32::from(p[1])).abs() < 12 && (i32::from(p[1]) - i32::from(p[2])).abs() < 12);
    assert!(grey);
    // the project itself is untouched
    assert_eq!(clip(&m, ids[0]).filter, "");
    let fx = r.look_preview(ids[0], String::new(), 0.0, "gaussian".into(), 80.0, SEC / 2, 120).unwrap();
    assert_ne!(fx.rgba, plain.rgba);
    for tr in ["dissolve", "push_left", "fade"] {
        let f = r.transition_preview(Some(ids[0]), Some(ids[1]), tr.into(), 0.8, 120).unwrap();
        assert!(f.width > 0);
    }
    assert!(r.transition_preview(None, Some(ids[0]), "fade".into(), 0.4, 120).is_ok());
    assert!(r.transition_preview(None, None, "fade".into(), 0.4, 120).is_err());
}

#[test]
fn picker_samples_draw_every_effect_and_transition() {
    for e in effect_presets() {
        let f = samples::effect_sample(e.id.clone(), 50.0, 96).unwrap();
        assert_eq!((f.width, f.height), (96, 96), "{}", e.id);
    }
    let plain = samples::transition_sample(String::new(), 0.4, 96).unwrap();
    let mut differ = 0;
    for t in transition_presets() {
        let f = samples::transition_sample(t.id.clone(), 0.4, 96).unwrap();
        if f.rgba != plain.rgba {
            differ += 1;
        }
    }
    // nearly every transition shows something different from a plain cut
    assert!(differ >= transition_presets().len() - 3, "{differ}");
    // the tiles play: the same transition differs along the way, and odd progress is clamped
    let first = transition_presets().into_iter().next().unwrap();
    let early = samples::transition_sample(first.id.clone(), 0.1, 96).unwrap();
    let late = samples::transition_sample(first.id.clone(), 0.9, 96).unwrap();
    assert_ne!(early.rgba, late.rgba);
    assert!(samples::transition_sample(first.id.clone(), f64::NAN, 96).is_ok());
    assert!(samples::transition_sample(first.id, 7.0, 96).is_ok());
    assert!(samples::effect_sample("nope".into(), 50.0, 96).is_err());
}

#[test]
fn new_text_sits_in_the_middle_of_the_frame_and_stays_centred_when_edited() {
    let m = MobileSession::new();
    m.new_project("P".into(), 1080, 1920, 30).unwrap();
    let id = m.add_text("Hello".into()).unwrap().unwrap();
    let p = m.placement(id).unwrap();
    assert!((p.x - 540.0).abs() < 1.0 && (p.y - 960.0).abs() < 1.0, "{p:?}");
    // the outline hugs the word, not the box it wraps in
    assert!(p.width > 150.0 && p.width < 900.0 && p.height > 50.0 && p.height < 300.0, "{p:?}");
    m.set_text(id, "Hello\nsecond line".into()).unwrap();
    let q = m.placement(id).unwrap();
    assert!((q.y - 960.0).abs() < 1.0 && q.height > p.height * 1.5, "{q:?}");
    let st = m.text_style(id).unwrap();
    m.set_text_style(id, text::TextStyle { width: 0.5, ..st }, false, false).unwrap();
    let r = m.placement(id).unwrap();
    assert!((r.x - 540.0).abs() < 1.0 && (r.y - 960.0).abs() < 1.0 && r.width <= 540.0, "{r:?}");
    // left-aligned, the letters start at the box's left edge (a quarter of the way in)
    m.set_text_style(id, text::TextStyle { width: 0.5, align: text::TextAlign::Left, ..m.text_style(id).unwrap() }, false, false).unwrap();
    let l = m.placement(id).unwrap();
    assert!((l.x - l.width / 2.0 - 270.0).abs() < 2.0, "{l:?}");
    // the text is drawn inside the frame
    let f = m.renderer().unwrap().frame(0, 270, false, None).unwrap();
    let lit = f.rgba.chunks(4).filter(|p| p[0] > 200 && p[1] > 200 && p[2] > 200).count();
    assert!(lit > 50, "{lit}");
}

fn say(text: &str) -> speech::SpeechSettings {
    speech::SpeechSettings { text: text.into(), voice: "basic-female".into(), pace: 1.0, pitch: 0 }
}

#[test]
fn speech_is_heard_first_added_on_an_audio_lane_and_said_again_in_its_place() {
    let (m, d, ids) = two_clips();
    let dir = d.to_string_lossy().into_owned();
    let heard = m.preview_speech(say("Hello there")).unwrap();
    assert!(heard.sample_rate > 0 && heard.pcm.len() > 4000, "{}", heard.pcm.len());
    m.set_playhead(SEC);
    let first = m.add_speech(say("Hello there"), dir.clone()).unwrap();
    let t = m.timeline().unwrap();
    let l = t.lanes.iter().find(|l| l.clips.iter().any(|c| c.id == first)).unwrap();
    // never the hidden lane that carries the videos' sound
    assert_eq!((l.kind, l.index), (LaneKind::Audio, -1));
    let c = clip(&m, first);
    assert!(c.speech && !c.voiceover && c.start == SEC && c.duration > SEC / 2, "{c:?}");
    assert_eq!(c.text, "Hello there");
    assert_eq!(t.selected, Some(first));
    assert_eq!(m.speech_settings(first).unwrap(), say("Hello there"));
    assert!(m.speech_settings(ids[0]).is_none() && !clip(&m, ids[0]).speech);
    // more words: a longer clip where the first was
    let longer = m.edit_speech(first, speech::SpeechSettings { pace: 0.8, ..say("Hello there, this sentence is a good deal longer") }, dir.clone()).unwrap();
    let c2 = clip(&m, longer);
    assert!(c2.start == SEC && c2.duration > c.duration, "{c2:?}");
    assert!(m.timeline().unwrap().lanes.iter().all(|l| l.clips.iter().all(|x| x.id != first)));
    assert_eq!(m.speech_settings(longer).unwrap().pace, 0.8);
    // one undo step back
    m.undo();
    assert_eq!(clip(&m, first).duration, c.duration);
    // a second one at the same time goes on another lane
    let other = m.add_speech(say("Again"), dir).unwrap();
    let t = m.timeline().unwrap();
    let lane_of = |id: u64| t.lanes.iter().find(|l| l.clips.iter().any(|c| c.id == id)).unwrap().index;
    assert_ne!(lane_of(other), lane_of(first));
    assert!(m.preview_speech(say("   ")).is_err());
}

#[test]
fn captions_come_from_what_is_said_fit_the_frame_and_can_be_edited() {
    use filmcraft_project::{Speaker, Transcript, Word};
    let (m, _d, ids) = two_clips();
    assert!(m.timeline().unwrap().captions.is_empty() && m.caption_look().is_none());
    let item = clip(&m, ids[0]).item;
    let w = |text: &str, a: f64, b: f64| Word {
        text: text.into(),
        start: Tick::from_seconds_f64(a),
        end: Tick::from_seconds_f64(b),
        speaker: None,
        confidence: 1.0,
    };
    let heard = Transcript {
        language: "en".into(),
        source: "whisper-tiny".into(),
        speakers: vec![Speaker { name: "Speaker 1".into() }],
        words: vec![w("Hello", 0.1, 0.4), w("phone", 0.45, 0.8), w("world.", 0.85, 1.3)],
    };
    let plain = m.renderer().unwrap().frame(SEC / 2, 270, false, None).unwrap();
    let n = speech::make_captions(&mut m.lock(), vec![(ItemId(item), heard)]).unwrap();
    assert_eq!(n, 1);
    let t = m.timeline().unwrap();
    assert_eq!(t.captions.len(), 1);
    assert_eq!(t.captions[0].text, "Hello phone world.");
    let look = m.caption_look().unwrap();
    assert!((look.scale - 1.0).abs() < 0.01 && look.position == speech::CaptionPosition::Bottom && look.background, "{look:?}");
    // drawn on the picture
    let shown = m.renderer().unwrap().frame(SEC / 2, 270, false, None).unwrap();
    assert_ne!(plain.rgba, shown.rgba);
    let undo_before = m.poll().undo_label;
    for k in 1..=5 {
        let scale = 1.0 + 0.1 * f64::from(k);
        m.set_caption_look(speech::CaptionLook { scale, position: speech::CaptionPosition::Top, background: false }, true).unwrap();
    }
    m.undo();
    // a drag is one undo step
    assert_eq!(m.poll().undo_label, undo_before);
    m.set_caption_look(speech::CaptionLook { scale: 1.5, position: speech::CaptionPosition::Top, background: false }, false).unwrap();
    let look = m.caption_look().unwrap();
    assert!((look.scale - 1.5).abs() < 0.01 && look.position == speech::CaptionPosition::Top && !look.background, "{look:?}");
    let id = t.captions[0].id;
    m.set_caption_text(id, "Hi world".into()).unwrap();
    assert_eq!(m.timeline().unwrap().captions[0].text, "Hi world");
    // without the box the words get a dark edge
    assert!(m.lock().active_sequence().unwrap().caption_tracks[0].style.outline > 0.0);
    // making them again replaces them, on one track, from the kept transcript, in the same look
    assert_eq!(speech::make_captions(&mut m.lock(), vec![]).unwrap(), 1);
    assert_eq!(m.lock().active_sequence().unwrap().caption_tracks.len(), 1);
    assert_eq!(m.caption_look().unwrap(), look);
    m.set_caption_text(m.timeline().unwrap().captions[0].id, " ".into()).unwrap();
    assert!(m.timeline().unwrap().captions.is_empty());
    m.undo();
    assert_eq!(m.timeline().unwrap().captions.len(), 1);
    m.delete_captions().unwrap();
    assert!(m.timeline().unwrap().captions.is_empty());
    m.undo();
    assert_eq!(m.timeline().unwrap().captions.len(), 1);
}

#[test]
fn caption_jobs_need_sound_and_a_known_model_and_report_how_they_end() {
    let m = MobileSession::new();
    m.new_project("P".into(), 1080, 1920, 30).unwrap();
    assert!(m.start_captions("whisper-tiny".into(), "auto".into()).is_err(), "nothing to listen to");
    let (m, _d, _) = two_clips();
    assert!(m.start_captions("whisper-huge".into(), "auto".into()).is_err());
    let job = m.start_captions("whisper-tiny".into(), "en".into()).unwrap();
    let mut st = m.caption_status(job).unwrap();
    for _ in 0..1200 {
        if st.finished {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
        st = m.caption_status(job).unwrap();
    }
    // without the model downloaded it says so; with it, the demo scenes' sound has no words
    assert!(st.finished, "{st:?}");
    assert!(st.error.is_some() || st.captions == 0, "{st:?}");
    assert!(m.caption_status(job).is_err(), "a finished job is gone");
    let job = m.start_captions("whisper-tiny".into(), "en".into()).unwrap();
    m.cancel_captions(job);
    for _ in 0..1200 {
        if m.caption_status(job).map(|s| s.finished).unwrap_or(true) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// Makes captions with the real speech model when `FILMCRAFT_TEST_SPEECH` names a clip of clear
/// English speech (and `FILMCRAFT_DATA_DIR` holds `models/whisper-tiny`).
#[test]
fn captions_from_real_speech() {
    let Ok(path) = std::env::var("FILMCRAFT_TEST_SPEECH") else { return };
    let m = MobileSession::new();
    m.new_project("P".into(), 1080, 1920, 30).unwrap();
    m.add_media(vec![path]).unwrap();
    let t0 = std::time::Instant::now();
    let job = m.start_captions("whisper-tiny".into(), "auto".into()).unwrap();
    let mut st = m.caption_status(job).unwrap();
    while !st.finished {
        std::thread::sleep(std::time::Duration::from_millis(200));
        st = m.caption_status(job).unwrap();
    }
    let t = m.timeline().unwrap();
    let words: Vec<String> = t.captions.iter().map(|c| c.text.replace('\n', " / ")).collect();
    eprintln!("{st:?} in {:.1} s: {words:#?}", t0.elapsed().as_secs_f64());
    assert!(st.error.is_none() && st.captions > 0, "{st:?}");
    assert!(words.join(" ").to_lowercase().contains("fox"));
    // a picture to look at: FILMCRAFT_TEST_SPEECH_PNG names where
    if let Ok(out) = std::env::var("FILMCRAFT_TEST_SPEECH_PNG") {
        let c = &t.captions[1];
        let f = m.renderer().unwrap().frame(c.start + c.duration / 2, 960, false, None).unwrap();
        std::fs::write(out, fx::encode_png(f.rgba, f.width, f.height).unwrap()).unwrap();
    }
}

/// A natural voice says a sentence and the speech model hears it back, when `FILMCRAFT_TEST_VOICES`
/// is set (and `FILMCRAFT_DATA_DIR` holds `models/kokoro-82m` and `models/whisper-tiny`).
#[test]
fn natural_speech_is_heard_back_as_captions() {
    if std::env::var("FILMCRAFT_TEST_VOICES").is_err() {
        return;
    }
    assert!(speech::voice_package().installed);
    let v = speech::voices();
    assert!(v.iter().any(|v| v.natural && v.installed && v.id == "kokoro-heart"), "{v:?}");
    let (m, d, _) = two_clips();
    let t0 = std::time::Instant::now();
    let st = speech::SpeechSettings { text: "Captions and voices are made right on the phone.".into(), voice: "kokoro-heart".into(), pace: 1.0, pitch: 0 };
    let clip = m.add_speech(st, d.to_string_lossy().into_owned()).unwrap();
    eprintln!("speech in {:.1} s: {:?}", t0.elapsed().as_secs_f64(), self::clip(&m, clip).duration as f64 / SEC as f64);
    let job = m.start_captions("whisper-tiny".into(), "en".into()).unwrap();
    let mut s = m.caption_status(job).unwrap();
    while !s.finished {
        std::thread::sleep(std::time::Duration::from_millis(200));
        s = m.caption_status(job).unwrap();
    }
    let words: Vec<String> = m.timeline().unwrap().captions.iter().map(|c| c.text.replace('\n', " ")).collect();
    eprintln!("{s:?}: {words:?}");
    let all = words.join(" ").to_lowercase();
    assert!(all.contains("captions") && all.contains("phone"), "{all}");
}

#[test]
fn words_over_a_video_with_no_words_are_captioned_and_talking_over_is_heard_once() {
    use filmcraft_project::{Transcript, Word};
    let (m, d, ids) = two_clips();
    m.set_playhead(SEC / 2);
    let over = m.add_audio(vec![tone(&d, 1.5)]).unwrap()[0];
    let w = |text: &str, a: f64, b: f64| Word {
        text: text.into(),
        start: Tick::from_seconds_f64(a),
        end: Tick::from_seconds_f64(b),
        speaker: None,
        confidence: 1.0,
    };
    let t = |words: Vec<Word>| Transcript { language: "en".into(), source: "whisper-tiny".into(), speakers: vec![], words };
    let video = clip(&m, ids[0]).item;
    let voice = clip(&m, over).item;
    // the video's own sound has no words: the voice-over above it is captioned
    let n =
        speech::make_captions(&mut m.lock(), vec![(ItemId(video), t(vec![])), (ItemId(voice), t(vec![w("Over", 0.1, 0.4), w("here.", 0.5, 0.9)]))]).unwrap();
    assert_eq!(n, 1);
    let c = &m.timeline().unwrap().captions[0];
    assert_eq!(c.text, "Over here.");
    // the voice-over starts half a second in: its words do too
    assert!(c.start >= SEC / 2, "{c:?}");
    // when the video's sound says something at the same moment, that is what is captioned
    let n = speech::make_captions(&mut m.lock(), vec![(ItemId(video), t(vec![w("Hello", 0.6, 0.9), w("there.", 1.0, 1.3)]))]).unwrap();
    assert_eq!(n, 1);
    assert_eq!(m.timeline().unwrap().captions[0].text, "Hello there.");
}

#[test]
fn a_transition_on_a_cut_without_spare_footage_shortens_the_video_by_its_length() {
    let (m, _d, ids) = two_clips();
    let before = m.timeline().unwrap().duration;
    assert_eq!(before, 5 * SEC);
    // each 2 s / 3 s clip lends a third of itself: the cut takes at most 2 × 2/3 s
    let max = m.max_transition(Some(ids[0]), Some(ids[1]));
    assert!((max - 4.0 / 3.0).abs() < 0.01, "{max}");
    let t = m.set_transition(Some(ids[0]), Some(ids[1]), "dissolve".into(), 0.4, false).unwrap().unwrap();
    let tl = m.timeline().unwrap();
    // 0.2 s off the end of the first clip and the start of the second
    assert_eq!(tl.duration, before - 2 * SEC / 5, "{}", tl.duration);
    let main = main_clips(&m);
    assert_eq!(main[0].duration, 2 * SEC - SEC / 5);
    assert_eq!(main[1].source_in, SEC / 5);
    assert_eq!(tl.transitions.iter().find(|x| x.id == t).unwrap().duration, 2 * SEC / 5);
    // longer: the clips give up a little more; taking it away gives nothing back
    m.set_transition_duration(t, 0.8).unwrap();
    assert_eq!(m.timeline().unwrap().duration, before - 4 * SEC / 5);
    m.set_transition(Some(ids[0]), Some(ids[1]), String::new(), 0.0, false).unwrap();
    assert_eq!(m.timeline().unwrap().duration, before - 4 * SEC / 5);
    // one undo step each
    m.undo();
    m.undo();
    assert_eq!(m.timeline().unwrap().duration, before - 2 * SEC / 5);
    m.undo();
    assert_eq!(m.timeline().unwrap().duration, before);
}

#[test]
fn advanced_object3d_rotation_keyframes_interpolate_and_toggle_off() {
    let m = MobileSession::new();
    m.new_project("3D".into(), 320, 240, 30).unwrap();
    let id = m.add_object3d(advanced::Object3DMesh::Cube).unwrap();
    let state = m.object3d_state(id).unwrap();
    assert!(state.has_object && !state.animated);
    assert_eq!(state.settings.mesh, advanced::Object3DMesh::Cube);
    assert!((state.settings.depth - 220.0).abs() < 1e-6);
    assert!(m.toggle_object3d_animation(id).unwrap());
    let mut a = m.object3d_state(id).unwrap().settings;
    a.rotation_y = 0.0;
    m.set_object3d(id, a, None).unwrap();
    m.set_playhead(SEC);
    let mut b = m.object3d_state(id).unwrap().settings;
    b.rotation_y = 90.0;
    m.set_object3d(id, b, None).unwrap();
    m.set_playhead(SEC / 2);
    let mid = m.object3d_state(id).unwrap();
    assert!(mid.animated);
    assert!((mid.settings.rotation_y - 45.0).abs() < 1.0, "{:?}", mid.settings.rotation_y);
    assert!(!m.toggle_object3d_animation(id).unwrap());
    assert!(!m.object3d_state(id).unwrap().animated);
}

#[test]
fn advanced_object3d_preview_renders_lit_pixels_not_black() {
    let m = MobileSession::new();
    m.new_project("3D preview".into(), 320, 240, 30).unwrap();
    let id = m.add_object3d(advanced::Object3DMesh::Cube).unwrap();
    let f = m.renderer().unwrap().frame(0, 320, false, None).unwrap();
    let lit = f.rgba.chunks_exact(4).filter(|p| p[0] as u32 + p[1] as u32 + p[2] as u32 > 60).count();
    assert!(lit > 200, "3D preview had only {lit} lit pixels");
    assert!(f.rgba.chunks_exact(4).any(|p| p[0] > 120 && p[1] > 40 && p[2] < 120), "cube did not render in its front colour");
    let mut st = m.object3d_state(id).unwrap().settings;
    st.mesh = advanced::Object3DMesh::Text;
    st.text = "Ag".into();
    st.size = 120.0;
    m.set_object3d(id, st, None).unwrap();
    let f = m.renderer().unwrap().frame(0, 320, false, None).unwrap();
    let lit = f.rgba.chunks_exact(4).filter(|p| p[0] as u32 + p[1] as u32 + p[2] as u32 > 60).count();
    assert!(lit > 80, "3D text preview had only {lit} lit pixels");
}

/// Pixels frame `b` changed against `a`: how many, their centre of mass and their bounding box.
fn changed_pixels(a: &[u8], b: &[u8], w: usize) -> (usize, f64, f64, (usize, usize, usize, usize)) {
    let (mut n, mut sx, mut sy) = (0usize, 0f64, 0f64);
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0usize, 0usize);
    for (i, (p, q)) in a.chunks_exact(4).zip(b.chunks_exact(4)).enumerate() {
        if p[..3].iter().zip(&q[..3]).all(|(u, v)| u.abs_diff(*v) <= 12) {
            continue;
        }
        let (x, y) = (i % w, i / w);
        n += 1;
        sx += x as f64;
        sy += y as f64;
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    if n == 0 {
        return (0, f64::NAN, f64::NAN, (0, 0, 0, 0));
    }
    (n, sx / n as f64, sy / n as f64, (x0, y0, x1, y1))
}

/// The case the empty 320x240 project could not show: a 1080x1920 sequence with video on V1, the
/// playhead inside that clip, and the monitor rendering at 720 px. The object has to land on top
/// of the video, in the middle of the picture, at that scale — a position applied before the
/// perspective divide threw it out of the frame by a distance proportional to the frame height.
#[test]
fn advanced_object3d_previews_over_video_at_monitor_scale() {
    let (m, _d, _ids) = two_clips();
    let t = SEC / 2;
    m.set_playhead(t);
    let before = m.renderer().unwrap().frame(t, 720, false, None).unwrap();
    let id = m.add_object3d(advanced::Object3DMesh::Cube).unwrap();
    let tl = m.timeline().unwrap();
    let over = lane(&tl, LaneKind::Overlay).clips.iter().find(|c| c.id == id).cloned();
    assert!(over.is_some(), "the 3D clip did not land on an overlay lane");
    let after = m.renderer().unwrap().frame(t, 720, false, None).unwrap();
    assert_eq!((after.width, after.height), (405, 720));
    let (n, cx, cy, bbox) = changed_pixels(&before.rgba, &after.rgba, 405);
    let msg = format!("{n} changed pixels, centre ({cx:.0}, {cy:.0}), box {bbox:?}, clip {over:?}");
    assert!(n > 10_000, "the 3D object was invisible over the video: {msg}");
    assert!(n < 405 * 720 / 2, "the 3D object covered the whole picture: {msg}");
    assert!((cx - 202.5).abs() < 12.0 && (cy - 360.0).abs() < 20.0, "the 3D object was not centred: {msg}");
    // It must be lit, not a black silhouette: the default front colour is a warm orange.
    let (mut sum, mut k) = ([0u64; 3], 0u64);
    for (p, q) in before.rgba.chunks_exact(4).zip(after.rgba.chunks_exact(4)) {
        if p[..3].iter().zip(&q[..3]).all(|(u, v)| u.abs_diff(*v) <= 12) {
            continue;
        }
        for c in 0..3 {
            sum[c] += u64::from(q[c]);
        }
        k += 1;
    }
    let mean = [sum[0] / k.max(1), sum[1] / k.max(1), sum[2] / k.max(1)];
    assert!(mean[0] > 60 && mean[0] > mean[2], "the 3D object rendered black or colourless: {mean:?} ({msg})");

    let mut st = m.object3d_state(id).unwrap().settings;
    st.mesh = advanced::Object3DMesh::Text;
    st.text = "3D".into();
    st.size = 140.0;
    m.set_object3d(id, st, None).unwrap();
    let text = m.renderer().unwrap().frame(t, 720, false, None).unwrap();
    let (n, cx, cy, bbox) = changed_pixels(&before.rgba, &text.rgba, 405);
    let msg = format!("{n} changed pixels, centre ({cx:.0}, {cy:.0}), box {bbox:?}");
    assert!(n > 1_500, "3D text was invisible over the video: {msg}");
    // The box, not the ink centroid: a glyph run carries its weight unevenly.
    let (bx, by) = ((bbox.0 + bbox.2) as f64 / 2.0, (bbox.1 + bbox.3) as f64 / 2.0);
    assert!((bx - 202.5).abs() < 8.0 && (by - 360.0).abs() < 8.0, "3D text was not centred: {msg}");
}

#[test]
fn advanced_extruding_text_carries_the_picked_style() {
    let m = MobileSession::new();
    m.new_project("Text".into(), 320, 240, 30).unwrap();
    let id = m.add_text("Style".into()).unwrap().unwrap();
    let st = m.text_style(id).unwrap();
    m.set_text_style(
        id,
        text::TextStyle { font: "Inter".into(), font_style: "Bold".into(), color: 0xff11_2233, stroke: Some(0xff44_5566), ..st },
        false,
        false,
    )
    .unwrap();
    assert_eq!(m.extrude_text(id, 60.0, 6.0).unwrap(), id);
    let o = m.object3d_state(id).unwrap();
    assert!(o.has_object);
    assert_eq!(o.settings.mesh, advanced::Object3DMesh::Text);
    assert_eq!(o.settings.text, "Style");
    assert_eq!(o.settings.font_style, "Bold");
    assert_eq!(o.settings.color, 0xff11_2233);
    assert_eq!(o.settings.side_color, 0xff44_5566);
    assert!((o.settings.depth - 60.0).abs() < 1e-6);
    assert!((o.settings.bevel - 6.0).abs() < 1e-6);
    assert!(clip(&m, id).has_3d);
    assert_eq!(clip(&m, id).text, "Style");
}

#[test]
fn advanced_background_removal_keeps_custom_points_and_controls() {
    let (m, d, _) = two_clips();
    let pic = m.add_media(vec![picture(&d)]).unwrap()[0];
    let auto = m.remove_background(pic).unwrap();
    assert!(auto.enabled);
    let mut st = auto.settings;
    st.keep_points.push(advanced::NormPoint { x: 0.25, y: 0.75 });
    st.remove_points.push(advanced::NormPoint { x: 0.8, y: 0.2 });
    st.threshold = 55.0;
    st.point_radius = 20.0;
    m.set_background(pic, st, None).unwrap();
    let back = m.background_state(pic).unwrap();
    assert!(back.enabled);
    assert_eq!(back.settings.keep_points.len(), 1);
    assert_eq!(back.settings.remove_points.len(), 1);
    assert!((back.settings.threshold - 55.0).abs() < 1e-6);
    assert!((back.settings.point_radius - 20.0).abs() < 1e-6);
}

#[test]
fn advanced_motion_tracking_stores_a_track_a_3d_object_can_follow() {
    let (m, _d, ids) = two_clips();
    let info = m
        .track_motion(
            ids[0],
            advanced::TrackOptions { target_x: 0.5, target_y: 0.5, patch_size: 15, stereo: advanced::StereoLayout::Mono, baseline: 32.0, max_seconds: 0.5 },
        )
        .unwrap();
    assert!(info.samples >= 2, "{info:?}");
    assert_eq!(m.motion_tracks().unwrap().len(), 1);
    let obj = m.add_object3d(advanced::Object3DMesh::Sphere).unwrap();
    let mut st = m.object3d_state(obj).unwrap().settings;
    st.follow_track = true;
    st.track_id = info.id;
    m.set_object3d(obj, st, None).unwrap();
    let followed = m.object3d_state(obj).unwrap().settings;
    assert!(followed.follow_track);
    assert_eq!(followed.track_id, info.id);
    m.delete_motion_track(info.id).unwrap();
    assert!(m.motion_tracks().unwrap().is_empty());
}
