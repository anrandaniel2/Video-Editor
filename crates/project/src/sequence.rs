use super::track::{AudioChannels, Marker, Track, TrackItem, TrackKind};
use super::{
    Caption, CaptionTrack, ClipId, Deserialize, EffectInstance, FrameRate, MergedClip, MixerStrip, MulticamSource, Serialize, TICKS_PER_SECOND, Tick,
    TimeRange, TrackId,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SequenceSettings {
    pub width: u32,
    pub height: u32,
    pub frame_rate: FrameRate,
    pub par: (u32, u32),
    pub sample_rate: u32,
    pub drop_frame: bool,
    /// Editing mode / preset name shown in the settings dialog.
    pub preset: String,
    pub audio_master: AudioChannels,
    /// Preview file codec name.
    pub preview_codec: String,
    pub max_bit_depth: bool,
    pub max_render_quality: bool,
    /// Working colour space (display label; [`SequenceSettings::color`] is authoritative).
    pub working_space: String,
    /// Colour pipeline: working space (Rec. 709 / Rec. 2100 PQ / HLG), wide-gamut compositing,
    /// Auto Tone Map Media.
    #[serde(default = "default_color_pipeline")]
    pub color: filmcraft_color::ColorPipeline,
}

fn default_color_pipeline() -> filmcraft_color::ColorPipeline {
    filmcraft_color::ColorPipeline::REC709
}

impl Default for SequenceSettings {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            frame_rate: FrameRate::FPS_23_976,
            par: (1, 1),
            sample_rate: 48_000,
            drop_frame: false,
            preset: "HD 1080p 23.976".into(),
            audio_master: AudioChannels::Stereo,
            preview_codec: "ProRes 422".into(),
            max_bit_depth: false,
            max_render_quality: false,
            working_space: "Rec. 709".into(),
            color: filmcraft_color::ColorPipeline::REC709,
        }
    }
}

/// Largest frame side, in pixels.
pub const MAX_FRAME_SIDE: u32 = 32_768;

/// Largest frame area, in pixels (16384 x 16384: 16K and 16384 x 8192 panoramas fit).
pub const MAX_FRAME_PIXELS: u64 = 268_435_456;

/// Bound working-frame allocations while retaining 16K and wide panoramic frames.
pub fn validate_frame_size(width: u32, height: u32) -> Result<(), String> {
    if width == 0 || height == 0 || width > MAX_FRAME_SIDE || height > MAX_FRAME_SIDE || u64::from(width) * u64::from(height) > MAX_FRAME_PIXELS {
        return Err(format!("frame size must be positive, at most {MAX_FRAME_SIDE} pixels per side and {MAX_FRAME_PIXELS} pixels total"));
    }
    Ok(())
}

/// The most a pixel aspect ratio may stretch a picture either way (8:1 or 1:8). Real footage stays
/// well inside (2:1 for 2x anamorphic lenses, 4:3 for HDV, 0.9 for NTSC DV); a ratio beyond it is
/// garbage from a damaged file and is treated as square.
pub const MAX_PIXEL_ASPECT: f64 = 8.0;

/// A pixel aspect ratio fit to compute with, in lowest terms, or `None` when it is unset (a zero
/// term, as decoders report "not signalled") or stretches more than [`MAX_PIXEL_ASPECT`] either
/// way. Ratios come from media, project and interchange files, so they are untrusted.
pub fn checked_par(par: (u32, u32)) -> Option<(u32, u32)> {
    let (n, d) = par;
    if n == 0 || d == 0 {
        return None;
    }
    let r = f64::from(n) / f64::from(d);
    if !(1.0 / MAX_PIXEL_ASPECT..=MAX_PIXEL_ASPECT).contains(&r) {
        return None;
    }
    let (mut a, mut b) = (n, d);
    while b != 0 {
        (a, b) = (b, a % b);
    }
    let g = a.max(1);
    Some((n / g, d / g))
}

/// [`checked_par`], with square pixels for a ratio that is unset or makes no sense.
pub fn sane_par(par: (u32, u32)) -> (u32, u32) {
    checked_par(par).unwrap_or((1, 1))
}

/// The width of one pixel over its height ([`sane_par`] first): 1.333… for 4:3 pixels.
pub fn par_ratio(par: (u32, u32)) -> f64 {
    let (n, d) = sane_par(par);
    f64::from(n) / f64::from(d)
}

/// The size of a `size` picture with `par` pixels in the pixels of a frame whose pixel aspect is
/// `frame_par`: the width scales by par / frame_par, the height stays. A 1440 x 1080 picture with
/// 4:3 pixels covers 1920 x 1080 square pixels.
pub fn conformed_size(size: (u32, u32), par: (u32, u32), frame_par: (u32, u32)) -> (f64, f64) {
    let (par, frame_par) = (sane_par(par), sane_par(frame_par));
    let w = if par == frame_par { f64::from(size.0) } else { f64::from(size.0) * par_ratio(par) / par_ratio(frame_par) };
    (w, f64::from(size.1))
}

impl SequenceSettings {
    pub fn validate(&self) -> Result<(), String> {
        validate_frame_size(self.width, self.height)?;
        if self.frame_rate.num <= 0
            || self.frame_rate.den <= 0
            || self.frame_rate.as_f64() > 1000.0
            || self.frame_rate.num > i64::from(u32::MAX)
            || self.frame_rate.den > i64::from(u32::MAX)
            || (i128::from(TICKS_PER_SECOND) * i128::from(self.frame_rate.den) / i128::from(self.frame_rate.num.max(1))) > i128::from(Tick::MAX.0)
        {
            return Err("frame rate must be positive, at most 1000 fps, with unsigned 32-bit rational components".into());
        }
        if self.sample_rate == 0 || self.sample_rate > 384_000 {
            return Err("sample rate must be between 1 and 384000 Hz".into());
        }
        if self.par.0 == 0 || self.par.1 == 0 {
            return Err("pixel aspect ratio must be positive".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sequence {
    pub settings: SequenceSettings,
    pub video_tracks: Vec<Track>,
    pub audio_tracks: Vec<Track>,
    pub markers: Vec<Marker>,
    pub mark_in: Option<Tick>,
    pub mark_out: Option<Tick>,
    /// Work area bar (optional; Premiere hides it by default now).
    pub work_area: Option<TimeRange>,
    /// Timecode of the first frame (frames), "Start Time" in Sequence settings.
    pub start_timecode: i64,
    /// Master audio volume (dB).
    #[serde(default)]
    pub master_volume_db: f64,
    /// Mix track inserts (`EffectInstance::post_fader` picks the side).
    #[serde(default)]
    pub master_effects: Vec<EffectInstance>,
    /// Mix track automation (`volume` lane) and mode.
    #[serde(default)]
    pub master_mixer: MixerStrip,
    /// Audio submix tracks (no clips); tracks and sends route into them, they route to the Mix
    /// or to a submix after them.
    #[serde(default)]
    pub submix_tracks: Vec<Track>,
    /// Caption tracks (drawn above the video tracks; first = top). Older files have none (serde default).
    #[serde(default)]
    pub caption_tracks: Vec<CaptionTrack>,
    /// Set on multi-camera source sequences (cameras, audio mode).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multicam: Option<MulticamSource>,
    /// Set on merged clips (the merged video and audio items).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merged: Option<MergedClip>,
    /// Motion analyses shared by 3D layers (schema v16). Older files have none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub motion_tracks: Vec<crate::tracking::MotionTrack>,
    /// Offline 3D camera match-move solves on clips in this sequence (schema v17).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub camera_solves: Vec<crate::matchmove::CameraSolve>,
    /// Active camera match-move binding driving 3D layers when enabled (schema v17).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matchmove: Option<crate::matchmove::SequenceMatchMove>,
    /// Scene lights shared by the 3D layers. Empty = the classic single light each object
    /// carries from its `light_angle`/`light_elevation`.
    #[serde(default, skip_serializing_if = "crate::lighting::SceneLighting::is_default")]
    pub scene_lighting: crate::lighting::SceneLighting,
    /// The camera the sequence's 3D layers render through. Absent = the classic fixed camera.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene_camera: Option<crate::lighting::SceneCamera3D>,
    /// Sequence-timeline keyframe tracks for `scene_camera` (`x`, `y`, `z`, `yaw`, `pitch`, `roll`, `fov`).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub scene_camera_tracks: std::collections::BTreeMap<String, crate::keyframe::Param>,
    /// Split In/Out points (Markers ▸ Mark Split): per-channel overrides of `mark_in`/`mark_out`.
    #[serde(default, skip_serializing_if = "SplitMarks::is_empty")]
    pub split: SplitMarks,
}

/// Split edit points: separate video and audio In/Out points (Markers ▸ Mark Split). `None` means
/// the channel uses the ordinary In/Out point.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SplitMarks {
    pub video_in: Option<Tick>,
    pub video_out: Option<Tick>,
    pub audio_in: Option<Tick>,
    pub audio_out: Option<Tick>,
}

impl SplitMarks {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
    /// The video In point given the ordinary In point.
    pub fn video_in_or(&self, mark_in: Option<Tick>) -> Option<Tick> {
        self.video_in.or(mark_in)
    }
    pub fn video_out_or(&self, mark_out: Option<Tick>) -> Option<Tick> {
        self.video_out.or(mark_out)
    }
    pub fn audio_in_or(&self, mark_in: Option<Tick>) -> Option<Tick> {
        self.audio_in.or(mark_in)
    }
    pub fn audio_out_or(&self, mark_out: Option<Tick>) -> Option<Tick> {
        self.audio_out.or(mark_out)
    }
}

impl Sequence {
    pub fn duration(&self) -> Tick {
        let media = self.video_tracks.iter().chain(&self.audio_tracks).map(Track::end).max().unwrap_or(Tick::ZERO);
        media.max(self.caption_tracks.iter().map(CaptionTrack::end).max().unwrap_or(Tick::ZERO))
    }
    pub fn caption_track(&self, id: TrackId) -> Option<&CaptionTrack> {
        self.caption_tracks.iter().find(|t| t.id == id)
    }
    pub fn caption_track_mut(&mut self, id: TrackId) -> Option<&mut CaptionTrack> {
        self.caption_tracks.iter_mut().find(|t| t.id == id)
    }
    /// Find a caption anywhere: (caption track id, caption).
    pub fn find_caption(&self, id: ClipId) -> Option<(TrackId, &Caption)> {
        self.caption_tracks.iter().find_map(|t| t.caption(id).map(|c| (t.id, c)))
    }
    pub fn tracks(&self, kind: TrackKind) -> &Vec<Track> {
        match kind {
            TrackKind::Video => &self.video_tracks,
            TrackKind::Audio => &self.audio_tracks,
        }
    }
    pub fn tracks_mut(&mut self, kind: TrackKind) -> &mut Vec<Track> {
        match kind {
            TrackKind::Video => &mut self.video_tracks,
            TrackKind::Audio => &mut self.audio_tracks,
        }
    }
    pub fn all_tracks(&self) -> impl Iterator<Item = &Track> {
        self.video_tracks.iter().chain(self.audio_tracks.iter())
    }
    pub fn all_tracks_mut(&mut self) -> impl Iterator<Item = &mut Track> {
        self.video_tracks.iter_mut().chain(self.audio_tracks.iter_mut())
    }
    pub fn track(&self, id: TrackId) -> Option<&Track> {
        self.all_tracks().find(|t| t.id == id)
    }
    pub fn track_mut(&mut self, id: TrackId) -> Option<&mut Track> {
        self.all_tracks_mut().find(|t| t.id == id)
    }
    /// Find a track item anywhere: (track id, item).
    pub fn find_item(&self, id: ClipId) -> Option<(TrackId, &TrackItem)> {
        self.all_tracks().find_map(|t| t.item(id).map(|i| (t.id, i)))
    }
    pub fn find_item_mut(&mut self, id: ClipId) -> Option<(TrackId, &mut TrackItem)> {
        self.all_tracks_mut().find_map(|t| {
            let tid = t.id;
            t.item_mut(id).map(|i| (tid, i))
        })
    }
    pub fn frame_rate(&self) -> FrameRate {
        self.settings.frame_rate
    }
    /// The solved 3D camera at sequence `timeline` time when [`Self::matchmove`] is enabled and
    /// points to a non-empty [`crate::matchmove::CameraSolve`]. Holds the last confident pose
    /// whenever a frame's confidence drops below [`crate::matchmove::CameraSolve::LOST_CONFIDENCE`].
    pub fn matchmove_camera_at(&self, timeline: Tick) -> Option<crate::lighting::SceneCamera3D> {
        let mm = self.matchmove.as_ref().filter(|m| m.enabled)?;
        let solve = self.camera_solves.iter().find(|s| s.id == mm.solve)?;
        let start = self.find_item(solve.clip).map(|(_, c)| c.start).unwrap_or(Tick::ZERO);
        let sample = solve.confident_sample_at(Tick(timeline.0.saturating_sub(start.0)))?;
        Some(mm.apply_to_sample(&sample))
    }
    /// Evaluate the sequence's manual 3D scene camera at timeline tick `t`, applying any active
    /// camera keyframe tracks.
    pub fn scene_camera_at(&self, t: Tick) -> Option<crate::lighting::SceneCamera3D> {
        self.scene_camera.map(|c| c.evaluated_with_tracks(&self.scene_camera_tracks, t))
    }
    /// Update the manual 3D scene camera at timeline tick `t`. When camera keyframe tracks are
    /// active, stamps/updates the changed parameters at `t`.
    pub fn update_scene_camera_at(&mut self, cam: crate::lighting::SceneCamera3D, t: Tick) {
        let prev_eval = self.scene_camera_at(t).unwrap_or(cam);
        self.scene_camera = Some(cam);
        if self.scene_camera_tracks.values().any(crate::keyframe::Param::is_animated) {
            for &id in crate::lighting::SceneCamera3D::ANIMATED_PARAMS {
                let Some(next_v) = cam.static_param_value(id) else { continue };
                let prev_v = prev_eval.static_param_value(id);
                let changed = match (&prev_v, &next_v) {
                    (Some(crate::keyframe::ParamValue::Float(a)), crate::keyframe::ParamValue::Float(b)) => (a - b).abs() > 1e-5,
                    _ => prev_v.as_ref() != Some(&next_v),
                };
                if let Some(track) = self.scene_camera_tracks.get_mut(id) {
                    track.value = next_v.clone();
                    if track.is_animated() && (changed || track.keyframes.iter().any(|k| k.time == t)) {
                        track.put_keyframe(t, next_v);
                    }
                } else if changed {
                    let mut p = crate::keyframe::Param::new(next_v.clone());
                    p.put_keyframe(t, next_v);
                    self.scene_camera_tracks.insert(id.to_string(), p);
                }
            }
        }
    }
    /// Route the active 3D match-move camera solve directly onto [`Self::scene_camera`] and
    /// [`Self::scene_camera_tracks`], interpolating smoothly across all intermediate timeline
    /// frames from the sampled COLMAP / GLOMAP keyframes while keeping 3D objects stationary in
    /// world space. When match-move is disabled or cleared, resets the routed camera.
    pub fn route_matchmove_to_scene_camera(&mut self, playhead: Tick) -> Option<crate::lighting::SceneCamera3D> {
        let Some(mm) = self.matchmove.filter(|m| m.enabled) else {
            self.scene_camera = None;
            self.scene_camera_tracks.clear();
            return None;
        };
        let Some((clip_id, first_rel, last_rel)) = self.camera_solves.iter().find(|s| s.id == mm.solve).and_then(|s| {
            let f0 = s.frames.first()?.time;
            let f1 = s.frames.last()?.time;
            Some((s.clip, f0, f1))
        }) else {
            self.scene_camera = None;
            self.scene_camera_tracks.clear();
            return None;
        };

        // Ensure 3D objects remain stationary in world space (camera moves, NOT the 3D objects).
        for track in &mut self.video_tracks {
            for item in &mut track.items {
                for eff in &mut item.effects {
                    if eff.effect == "object_3d"
                        && let Some(ft) = eff.params.get_mut("follow_track")
                    {
                        ft.value = crate::keyframe::ParamValue::Bool(false);
                        ft.keyframes.clear();
                    }
                }
            }
        }

        let clip_start = self.find_item(clip_id).map(|(_, c)| c.start).unwrap_or(Tick::ZERO);
        let t_start = Tick(clip_start.0.saturating_add(first_rel.0));
        let t_end = Tick(clip_start.0.saturating_add(last_rel.0)).max(t_start);
        let span = t_end.0.saturating_sub(t_start.0);
        let min_step = Tick::from_seconds_f64(1.0 / 60.0).max(Tick(1));
        let mut step = self.settings.frame_rate.frame_duration().max(min_step);
        let max_kf = crate::matchmove::CameraSolve::MAX_FRAMES as i64;
        if span > 0 && span / step.0 > max_kf {
            step = Tick((span / max_kf).max(1));
        }

        let unwrap_deg = |prev: f64, next: f64| -> f64 {
            let d = (next - prev + 180.0).rem_euclid(360.0) - 180.0;
            prev + d
        };

        let est = ((span / step.0.max(1)) as usize).saturating_add(2).min(crate::matchmove::CameraSolve::MAX_FRAMES + 2);
        let mut kf_x = Vec::with_capacity(est);
        let mut kf_y = Vec::with_capacity(est);
        let mut kf_z = Vec::with_capacity(est);
        let mut kf_yaw = Vec::with_capacity(est);
        let mut kf_pitch = Vec::with_capacity(est);
        let mut kf_roll = Vec::with_capacity(est);
        let mut kf_fov = Vec::with_capacity(est);

        let mut prev_angles: Option<(f64, f64, f64)> = None;
        let mut first_cam: Option<crate::lighting::SceneCamera3D> = None;

        let mut push_sample = |t: Tick, cam: crate::lighting::SceneCamera3D| {
            if first_cam.is_none() {
                first_cam = Some(cam);
            }
            let raw_yaw = f64::from(cam.yaw);
            let raw_pitch = f64::from(cam.pitch);
            let raw_roll = f64::from(cam.roll);
            let (yaw, pitch, roll) = match prev_angles {
                Some((py, pp, pr)) => (
                    unwrap_deg(py, raw_yaw),
                    unwrap_deg(pp, raw_pitch),
                    unwrap_deg(pr, raw_roll),
                ),
                None => (raw_yaw, raw_pitch, raw_roll),
            };
            prev_angles = Some((yaw, pitch, roll));
            kf_x.push(crate::keyframe::Keyframe::new(t, crate::keyframe::ParamValue::Float(f64::from(cam.position[0]))));
            kf_y.push(crate::keyframe::Keyframe::new(t, crate::keyframe::ParamValue::Float(f64::from(cam.position[1]))));
            kf_z.push(crate::keyframe::Keyframe::new(t, crate::keyframe::ParamValue::Float(f64::from(cam.position[2]))));
            kf_yaw.push(crate::keyframe::Keyframe::new(t, crate::keyframe::ParamValue::Float(yaw)));
            kf_pitch.push(crate::keyframe::Keyframe::new(t, crate::keyframe::ParamValue::Float(pitch)));
            kf_roll.push(crate::keyframe::Keyframe::new(t, crate::keyframe::ParamValue::Float(roll)));
            kf_fov.push(crate::keyframe::Keyframe::new(t, crate::keyframe::ParamValue::Float(f64::from(cam.fov))));
        };

        let mut t = t_start;
        loop {
            if let Some(cam) = self.matchmove_camera_at(t) {
                push_sample(t, cam);
            }
            if t >= t_end {
                break;
            }
            let next_t = Tick(t.0.saturating_add(step.0));
            t = if next_t > t_end { t_end } else { next_t };
        }

        let base = first_cam?;
        let make_param = |fallback: f32, keyframes: Vec<crate::keyframe::Keyframe>| crate::keyframe::Param {
            value: keyframes.first().map(|k| k.value.clone()).unwrap_or(crate::keyframe::ParamValue::Float(f64::from(fallback))),
            keyframes,
        };
        let mut tracks = std::collections::BTreeMap::new();
        tracks.insert("x".into(), make_param(base.position[0], kf_x));
        tracks.insert("y".into(), make_param(base.position[1], kf_y));
        tracks.insert("z".into(), make_param(base.position[2], kf_z));
        tracks.insert("yaw".into(), make_param(base.yaw, kf_yaw));
        tracks.insert("pitch".into(), make_param(base.pitch, kf_pitch));
        tracks.insert("roll".into(), make_param(base.roll, kf_roll));
        tracks.insert("fov".into(), make_param(base.fov, kf_fov));

        let cam_now = base.evaluated_with_tracks(&tracks, playhead);
        self.scene_camera_tracks = tracks;
        self.scene_camera = Some(cam_now);
        Some(cam_now)
    }

    /// Every sample of the active match-move solve mapped onto sequence timeline ticks and
    /// calibrated world coordinates, holding the last confident pose across lost frames.
    pub fn evaluated_matchmove_frames(&self) -> Option<Vec<crate::matchmove::EvaluatedMatchMoveFrame>> {
        let mm = self.matchmove.as_ref().filter(|m| m.enabled)?;
        let solve = self.camera_solves.iter().find(|s| s.id == mm.solve)?;
        if solve.frames.is_empty() {
            return None;
        }
        let base = self.find_item(solve.clip).map(|(_, c)| c.start).unwrap_or(Tick::ZERO);
        let first_confident = solve.frames.iter().find(|f| f.is_confident()).copied().unwrap_or(solve.frames[0]);
        let mut held = first_confident;
        let mut out = Vec::with_capacity(solve.frames.len());
        for f in &solve.frames {
            if f.is_confident() {
                held = *f;
            }
            out.push(crate::matchmove::EvaluatedMatchMoveFrame {
                timeline: Tick(base.0.saturating_add(f.time.0)),
                camera: mm.apply_to_sample(&held),
                rms_px: f.rms_px,
                confidence: f.confidence,
                lost: !f.is_confident(),
            });
        }
        Some(out)
    }
    /// Sorted, de-duplicated edit points (clip boundaries) on all tracks.
    pub fn edit_points(&self) -> Vec<Tick> {
        let mut v: Vec<Tick> = self.all_tracks().flat_map(|t| t.items.iter().flat_map(|i| [i.start, i.end()])).collect();
        v.sort_unstable();
        v.dedup();
        v
    }
    pub fn check(&self) -> Result<(), String> {
        self.check_bounds()?;
        for t in self.all_tracks() {
            t.check()?;
        }
        for t in &self.caption_tracks {
            t.check()?;
        }
        Ok(())
    }
    /// The hostile-value bounds alone (settings, marks and every time range), without the
    /// structural invariants of [`Sequence::check`]: a project that breaks only those (e.g. an
    /// overlap written by an older build) still opens, but nothing in it can overflow.
    pub fn check_bounds(&self) -> Result<(), String> {
        self.settings.validate()?;
        if self.mark_in.into_iter().chain(self.mark_out).any(|t| t < Tick::MIN || t > Tick::MAX)
            || self.work_area.is_some_and(|r| !bounded_time_range(r.start, r.duration))
        {
            return Err("sequence marks or work area are outside supported time bounds".into());
        }
        for t in self.all_tracks() {
            t.check_bounds()?;
        }
        for t in &self.caption_tracks {
            t.check_bounds()?;
        }
        if !self.camera_solves.iter().all(crate::matchmove::CameraSolve::is_finite) || self.matchmove.is_some_and(|m| !m.is_finite()) {
            return Err("sequence camera match-move data contains non-finite values or exceeds bounds".into());
        }
        Ok(())
    }
}

/// Bound persisted timeline arithmetic before any start + duration operation: start, duration and
/// end all stay within `Tick::MIN..=Tick::MAX`, leaving headroom for later edit math. Negative
/// positions remain supported; the sign of the duration is a structural rule (track items and
/// captions require a positive one in their own invariant checks), not a bound.
pub(crate) fn bounded_time_range(start: Tick, duration: Tick) -> bool {
    let within = |t: i64| (Tick::MIN.0..=Tick::MAX.0).contains(&t);
    within(start.0) && within(duration.0) && start.0.checked_add(duration.0).is_some_and(within)
}
