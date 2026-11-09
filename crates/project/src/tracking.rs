//! Motion tracks shared by a sequence.
//!
//! A track is a sparse list of samples in the tracked clip's own time. Samples carry a
//! normalised picture position, a relative depth (stereoscopic disparity when available),
//! scale and rotation so 3D layers can follow the footage without baking keyframes into every
//! object. Times are clip-relative ticks; renderers add the tracked clip's current timeline start.

use filmcraft_time::Tick;
use serde::{Deserialize, Serialize};

use crate::ClipId;

/// How a stereoscopic picture is packed. Mono tracks still produce scale/rotation; depth stays 0.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StereoLayout {
    #[default]
    Mono,
    SideBySide,
    TopBottom,
}

impl StereoLayout {
    pub fn label(self) -> &'static str {
        match self {
            Self::Mono => "Mono",
            Self::SideBySide => "Side by side",
            Self::TopBottom => "Top / bottom",
        }
    }
}

/// One tracked moment. `x`/`y` are fractions of the tracked view (0..1); `z` is relative depth
/// (0 for mono, larger = nearer for stereo); `scale` is relative to the first sample; `rotation`
/// is degrees clockwise.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrackSample {
    pub time: Tick,
    pub x: f32,
    pub y: f32,
    #[serde(default)]
    pub z: f32,
    #[serde(default = "one")]
    pub scale: f32,
    #[serde(default)]
    pub rotation: f32,
}

fn one() -> f32 {
    1.0
}

impl TrackSample {
    pub fn lerp(a: Self, b: Self, t: f32) -> Self {
        let l = |x: f32, y: f32| x + (y - x) * t;
        Self {
            time: Tick(a.time.0 + ((b.time.0 - a.time.0) as f32 * t) as i64),
            x: l(a.x, b.x),
            y: l(a.y, b.y),
            z: l(a.z, b.z),
            scale: l(a.scale, b.scale),
            rotation: l(a.rotation, b.rotation),
        }
    }
}

/// A named analysis of one clip. Samples are sorted by clip-relative time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MotionTrack {
    pub id: u64,
    pub clip: ClipId,
    pub name: String,
    #[serde(default)]
    pub layout: StereoLayout,
    pub samples: Vec<TrackSample>,
}

impl MotionTrack {
    /// Sample at clip-relative time `t`, holding outside the analysed range.
    pub fn sample_at(&self, t: Tick) -> Option<TrackSample> {
        let s = &self.samples;
        if s.is_empty() {
            return None;
        }
        if t <= s[0].time {
            return Some(s[0]);
        }
        if t >= s[s.len() - 1].time {
            return Some(s[s.len() - 1]);
        }
        let i = s.partition_point(|k| k.time <= t).saturating_sub(1);
        let a = s[i];
        let b = s.get(i + 1).copied().unwrap_or(a);
        let span = (b.time.0 - a.time.0).max(1) as f32;
        Some(TrackSample::lerp(a, b, ((t.0 - a.time.0) as f32 / span).clamp(0.0, 1.0)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track() -> MotionTrack {
        MotionTrack {
            id: 1,
            clip: ClipId(2),
            name: "Track".into(),
            layout: StereoLayout::Mono,
            samples: vec![
                TrackSample { time: Tick(0), x: 0.0, y: 0.0, z: 0.0, scale: 1.0, rotation: 0.0 },
                TrackSample { time: Tick(100), x: 1.0, y: 2.0, z: 0.5, scale: 2.0, rotation: 90.0 },
            ],
        }
    }

    #[test]
    fn samples_interpolate_and_hold() {
        let t = track();
        assert_eq!(t.sample_at(Tick(-10)).unwrap().x, 0.0);
        let mid = t.sample_at(Tick(50)).unwrap();
        assert_eq!(mid.x, 0.5);
        assert_eq!(mid.y, 1.0);
        assert_eq!(mid.scale, 1.5);
        assert_eq!(t.sample_at(Tick(200)).unwrap().rotation, 90.0);
    }

    #[test]
    fn empty_track_has_no_sample() {
        let mut t = track();
        t.samples.clear();
        assert!(t.sample_at(Tick::ZERO).is_none());
    }
}
