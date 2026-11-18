//! The brush: what a stroke's stamps look like, and how they are placed along the path.
//!
//! This is the photo engine's own version of the model Procraft carries in
//! `crates/engine/src/brush.rs`. The vocabulary is the same on purpose, because the panel is the
//! same panel: a brush is not "hardness and grain", it is a stamp shape (with roundness, rotation,
//! scattering and jitter), a grain that either sits on the canvas or travels with the stamp, a
//! spacing, a flow, and a taper at each end.
//!
//! The reason this exists at all: the engine used to stamp one round disc, varied only by hardness
//! and grain depth. Eight names fed those two numbers, so at a glance every brush drew the same
//! line -- which is exactly what the Brushes panel looked like it was doing, and was.

/// A stock tip shape. Procraft draws these from texture images; the radii here are drawn
/// procedurally, so the engine carries no image assets and the tips still differ in silhouette.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tip {
    /// A round disc, softened by `hardness`.
    Round,
    /// A chisel: the disc squashed across its rotation, so the stroke has a broad and a narrow
    /// direction and a turn of the wrist shows.
    Chisel,
    /// A flat brush: strongly squashed and following the stroke, so a change of direction draws a
    /// thin edge.
    Flat,
    /// A ragged disc: the rim is broken up by the grain, which is what charcoal and chalk are.
    Ragged,
    /// A cloud: several offset lobes, soft all over, for a smudging airbrush.
    Cloud,
}

impl Tip {
    pub fn as_str(self) -> &'static str {
        match self {
            Tip::Round => "round",
            Tip::Chisel => "chisel",
            Tip::Flat => "flat",
            Tip::Ragged => "ragged",
            Tip::Cloud => "cloud",
        }
    }

    /// The tip's name as the studio's panels show it.
    pub fn name(self) -> &'static str {
        match self {
            Tip::Round => "Round",
            Tip::Chisel => "Chisel",
            Tip::Flat => "Flat",
            Tip::Ragged => "Ragged",
            Tip::Cloud => "Cloud",
        }
    }

    pub fn parse(name: &str) -> Option<Tip> {
        Some(match name {
            "round" => Tip::Round,
            "chisel" => Tip::Chisel,
            "flat" => Tip::Flat,
            "ragged" => Tip::Ragged,
            "cloud" => Tip::Cloud,
            _ => return None,
        })
    }
}

/// How a stroke's stamps build up within the stroke.
///
/// This is the studio's "rendering mode", and it is the difference between a stroke that reads as
/// one ribbon of paint and one that reads as a chain of discs. Procraft has six named modes; these
/// are the four behaviours they are built from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Rendering {
    /// Each stamp adds to what is there, up to the stroke's opacity. The default, and what a soft
    /// airbrush wants.
    #[default]
    BuildUp,
    /// Overlapping stamps do not build up: the stroke keeps the strongest stamp at each pixel.
    /// This is "Light Glaze", and it is what stops a hard tip leaving dark blotches where its
    /// discs overlap -- which is most of why a stroke looked stamped rather than drawn.
    Strongest,
    /// Each stamp adds less the more is already there, by what is left squared: a build-up that
    /// approaches the stroke's opacity smoothly instead of in steps.
    Gradual,
}

impl Rendering {
    pub fn as_str(self) -> &'static str {
        match self {
            Rendering::BuildUp => "buildup",
            Rendering::Strongest => "glaze",
            Rendering::Gradual => "gradual",
        }
    }

    pub fn parse(name: &str) -> Option<Rendering> {
        Some(match name {
            "buildup" | "blending" | "intense blending" => Rendering::BuildUp,
            "glaze" | "light glaze" | "uniformed glaze" | "intense glaze" | "heavy glaze" => {
                Rendering::Strongest
            }
            "gradual" | "uniform blending" => Rendering::Gradual,
            _ => return None,
        })
    }
}

/// How a brush's grain is laid down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrainMode {
    /// Fixed to the canvas: dragging over the same place twice deepens the same texture.
    Canvas,
    /// Travelling with the stamp: the texture moves with the hand.
    Stamp,
}

impl GrainMode {
    pub fn as_str(self) -> &'static str {
        match self {
            GrainMode::Canvas => "canvas",
            GrainMode::Stamp => "stamp",
        }
    }

    pub fn parse(name: &str) -> Option<GrainMode> {
        Some(match name {
            "canvas" => GrainMode::Canvas,
            "stamp" => GrainMode::Stamp,
            _ => return None,
        })
    }
}

/// One brush, with everything a stroke needs to look like itself.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Brush {
    pub tip: Tip,
    /// The disc's edge: 1 is hard, 0 soft all the way to the centre.
    pub hardness: f64,
    /// How much the grain breaks the tip up, 0..1.
    pub grain: f64,
    /// The grain's coarseness: 0 is fine paper tooth, 1 is a coarse wash.
    pub grain_scale: f64,
    pub grain_mode: GrainMode,
    /// Distance between stamps as a fraction of the stamp's diameter. Small is a smooth line;
    /// large is a dotted or beaded one.
    pub spacing: f64,
    /// How much each stamp lays down, 0..1. Low builds slowly over a drag; 1 is opaque at once.
    pub flow: f64,
    /// Squash across the stamp's rotation, 1 = circular.
    pub roundness: f64,
    /// Rotation in radians.
    pub rotation: f64,
    /// How far the stamp turns with the stroke's direction: 1 follows it, 0 holds still.
    pub follow: f64,
    /// Random per-stamp rotation, in radians.
    pub scatter: f64,
    /// Random per-stamp size and opacity reduction, 0..1.
    pub jitter: f64,
    /// How many stamps at each step: more than one thickens and roughens a stroke.
    pub count: u32,
    /// Length of the taper at each end, as a fraction of the stroke.
    pub taper: f64,
    /// Whether the brush is a smudge: it mixes what is under it rather than covering it.
    pub wet: bool,
    /// How the stamps of one stroke build up, which is the studio's rendering mode.
    pub rendering: Rendering,
    /// How far the stroke's path is smoothed towards a spline, 0..1. Procraft calls this
    /// StreamLine; it is what turns a jittery drag into a confident line.
    pub streamline: f64,
}

impl Default for Brush {
    fn default() -> Self {
        // A clean round pen. A brush that did nothing surprising until one was chosen is what the
        // engine had before; it is now the fallback, not the only option.
        Brush {
            tip: Tip::Round,
            hardness: 0.85,
            grain: 0.0,
            grain_scale: 0.35,
            grain_mode: GrainMode::Canvas,
            spacing: 0.08,
            flow: 1.0,
            roundness: 1.0,
            rotation: 0.0,
            follow: 0.0,
            scatter: 0.0,
            jitter: 0.0,
            count: 1,
            taper: 0.0,
            wet: false,
            rendering: Rendering::BuildUp,
            streamline: 0.0,
        }
    }
}

impl Brush {
    /// The brush as the panel reads it.
    pub fn to_json(self) -> serde_json::Value {
        self.to_json_named("")
    }

    /// The brush with the name it is listed under, so a panel editing it can say which brush it
    /// is editing rather than only what its settings are.
    pub fn to_json_named(self, name: &str) -> serde_json::Value {
        serde_json::json!({
            "name": name,
            "tip": self.tip.as_str(),
            "hardness": self.hardness,
            "grain": self.grain,
            "grainScale": self.grain_scale,
            "grainMode": self.grain_mode.as_str(),
            "spacing": self.spacing,
            "flow": self.flow,
            "roundness": self.roundness,
            "rotation": self.rotation,
            "follow": self.follow,
            "scatter": self.scatter,
            "jitter": self.jitter,
            "count": self.count,
            "taper": self.taper,
            "wet": self.wet,
            "rendering": self.rendering.as_str(),
            "streamline": self.streamline,
        })
    }

    /// Apply the parameters a command or the panel sent. Anything absent keeps its value, so a
    /// panel that only changes the size does not reset the grain.
    pub fn merge(&mut self, params: &serde_json::Value) -> Result<(), String> {
        use serde_json::Value;
        let num = |k: &str| params.get(k).and_then(Value::as_f64);
        if let Some(v) = params.get("tip").and_then(Value::as_str) {
            self.tip = Tip::parse(v).ok_or_else(|| format!("`{v}` is not a brush tip."))?;
        }
        if let Some(v) = params.get("grainMode").and_then(Value::as_str) {
            self.grain_mode =
                GrainMode::parse(v).ok_or_else(|| format!("`{v}` is not a grain mode."))?;
        }
        if let Some(v) = num("hardness") { self.hardness = v.clamp(0.0, 1.0); }
        if let Some(v) = num("grain") { self.grain = v.clamp(0.0, 1.0); }
        if let Some(v) = num("grainScale") { self.grain_scale = v.clamp(0.0, 1.0); }
        // A spacing of zero would stamp the same pixel forever.
        if let Some(v) = num("spacing") { self.spacing = v.clamp(0.02, 2.0); }
        if let Some(v) = num("flow") { self.flow = v.clamp(0.0, 1.0); }
        if let Some(v) = num("roundness") { self.roundness = v.clamp(0.05, 1.0); }
        if let Some(v) = num("rotation") { self.rotation = v; }
        if let Some(v) = num("follow") { self.follow = v.clamp(-1.0, 1.0); }
        if let Some(v) = num("scatter") { self.scatter = v.clamp(0.0, 2.0); }
        if let Some(v) = num("jitter") { self.jitter = v.clamp(0.0, 1.0); }
        if let Some(v) = num("count") { self.count = (v.round() as u32).clamp(1, 8); }
        if let Some(v) = num("taper") { self.taper = v.clamp(0.0, 0.5); }
        if let Some(v) = params.get("wet").and_then(Value::as_bool) { self.wet = v; }
        if let Some(v) = params.get("rendering").and_then(Value::as_str) {
            self.rendering = Rendering::parse(v).ok_or_else(|| format!("`{v}` is not a rendering mode."))?;
        }
        if let Some(v) = num("streamline") { self.streamline = v.clamp(0.0, 1.0); }
        Ok(())
    }
}

/// The brushes the panel lists, with their own characters.
///
/// These are Procraft's own names and, near enough, its settings: the pencils are hard and fine
/// with a paper tooth, the charcoals are ragged and coarse, the chalk is soft and heavily grained,
/// the bristle brush scatters, and the ink is a flat, hard, ungrainned line that follows the hand.
/// The point is that a stroke from one does not look like a stroke from another.
/// The studio's brushes, ported from Procraft's own library.
///
/// These are not looks made up to be distinguishable: each is the parameter set Procraft's
/// `crates/engine/src/library.rs` gives that brush, mapped onto the controls this engine draws.
/// Where Procraft works from a texture image the tip is a procedural stamp with the same
/// silhouette (its `GRAINY_DISC` is this `Ragged`, its `FLAT` this `Flat`), and where Procraft's
/// parameter has no equivalent here -- tilt, pressure, a material channel -- the value closest to
/// what that parameter does to the mark is used.
///
/// The names, the set they belong to, and their order are Procraft's: Sketching, then Inking.
pub const LIBRARY: &[(&str, Brush)] = &[
    // --- Sketching ---------------------------------------------------------
    /// Procraft's `graphite()`: a grainy disc on paper, its 0.7 hardness, spaced 0.06.
    (
        "Graphite",
        Brush {
            tip: Tip::Ragged,
            hardness: 0.7,
            grain: 0.6,
            grain_scale: 0.45,
            grain_mode: GrainMode::Stamp,
            spacing: 0.06,
            flow: 0.85,
            roundness: 1.0,
            rotation: 0.0,
            follow: 0.0,
            scatter: 0.35,
            jitter: 0.25,
            count: 1,
            taper: 0.08,
            wet: false,
            rendering: Rendering::Strongest,
            streamline: 0.0,
    },
    ),
    /// Procraft's `soft_pencil()`: the same brush wider and softer, on coarse paper.
    (
        "Soft Pencil",
        Brush {
            tip: Tip::Round,
            hardness: 0.55,
            grain: 0.7,
            grain_scale: 0.6,
            grain_mode: GrainMode::Stamp,
            spacing: 0.06,
            flow: 0.7,
            roundness: 1.0,
            rotation: 0.0,
            follow: 0.0,
            scatter: 0.2,
            jitter: 0.2,
            count: 1,
            taper: 0.08,
            wet: false,
            rendering: Rendering::Strongest,
            streamline: 0.0,
    },
    ),
    /// Procraft's `technical_pencil()`: hard, tight, nearly opaque, a drafting lead.
    (
        "Technical Pencil",
        Brush {
            tip: Tip::Round,
            hardness: 0.95,
            grain: 0.35,
            grain_scale: 0.35,
            grain_mode: GrainMode::Stamp,
            spacing: 0.05,
            flow: 1.0,
            roundness: 1.0,
            rotation: 0.0,
            follow: 0.0,
            scatter: 0.0,
            jitter: 0.0,
            count: 1,
            taper: 0.0,
            wet: false,
            rendering: Rendering::Strongest,
            streamline: 0.0,
    },
    ),
    /// Procraft's `hb_pencil()`: the everyday pencil, a little less grain than 6B.
    (
        "HB Pencil",
        Brush {
            tip: Tip::Ragged,
            hardness: 0.8,
            grain: 0.5,
            grain_scale: 0.4,
            grain_mode: GrainMode::Stamp,
            spacing: 0.06,
            flow: 0.75,
            roundness: 1.0,
            rotation: 0.0,
            follow: 0.0,
            scatter: 0.3,
            jitter: 0.2,
            count: 1,
            taper: 0.08,
            wet: false,
            rendering: Rendering::Strongest,
            streamline: 0.0,
    },
    ),
    /// Procraft's `six_b_pencil()`: dense and dark, the softest lead in the set.
    (
        "6B Pencil",
        Brush {
            tip: Tip::Ragged,
            hardness: 0.7,
            grain: 0.75,
            grain_scale: 0.65,
            grain_mode: GrainMode::Stamp,
            spacing: 0.06,
            flow: 0.95,
            roundness: 1.0,
            rotation: 0.0,
            follow: 0.0,
            scatter: 0.4,
            jitter: 0.3,
            count: 1,
            taper: 0.08,
            wet: false,
            rendering: Rendering::Strongest,
            streamline: 0.0,
    },
    ),
    /// Procraft's `charcoal_pencil()`: coarse, dark and broken up.
    (
        "Charcoal Pencil",
        Brush {
            tip: Tip::Ragged,
            hardness: 0.75,
            grain: 0.85,
            grain_scale: 0.7,
            grain_mode: GrainMode::Stamp,
            spacing: 0.06,
            flow: 0.9,
            roundness: 1.0,
            rotation: 0.0,
            follow: 0.0,
            scatter: 0.5,
            jitter: 0.35,
            count: 1,
            taper: 0.08,
            wet: false,
            rendering: Rendering::Strongest,
            streamline: 0.0,
    },
    ),
    // --- Inking ------------------------------------------------------------
    /// Procraft's `fine_liner()`: a hard even line, its 0.062 streamline keeping it straight.
    (
        "Fine Liner",
        Brush {
            tip: Tip::Round,
            hardness: 0.97,
            grain: 0.0,
            grain_scale: 0.0,
            grain_mode: GrainMode::Stamp,
            spacing: 0.08,
            flow: 1.0,
            roundness: 1.0,
            rotation: 0.0,
            follow: 0.0,
            scatter: 0.0,
            jitter: 0.0,
            count: 1,
            taper: 0.0,
            wet: false,
            rendering: Rendering::Strongest,
            streamline: 0.062,
    },
    ),
    /// Procraft's `brush_pen()`: heavily streamlined, tapering hard at both ends.
    (
        "Brush Pen",
        Brush {
            tip: Tip::Round,
            hardness: 0.92,
            grain: 0.0,
            grain_scale: 0.0,
            grain_mode: GrainMode::Stamp,
            spacing: 0.08,
            flow: 1.0,
            roundness: 1.0,
            rotation: 0.0,
            follow: 0.0,
            scatter: 0.0,
            jitter: 0.0,
            count: 1,
            taper: 0.3,
            wet: false,
            rendering: Rendering::Strongest,
            streamline: 0.67,
    },
    ),
    /// Procraft's `Brush::round_pen()`: the parent of the inking set, barely tapered.
    (
        "Round Pen",
        Brush {
            tip: Tip::Round,
            hardness: 0.92,
            grain: 0.0,
            grain_scale: 0.0,
            grain_mode: GrainMode::Stamp,
            spacing: 0.08,
            flow: 1.0,
            roundness: 1.0,
            rotation: 0.0,
            follow: 0.0,
            scatter: 0.0,
            jitter: 0.0,
            count: 1,
            taper: 0.15,
            wet: false,
            rendering: Rendering::BuildUp,
            streamline: 0.015,
    },
    ),
    /// Procraft's `flat_brush()`: a chisel that follows the stroke, on a woven grain.
    (
        "Flat Brush",
        Brush {
            tip: Tip::Flat,
            hardness: 0.92,
            grain: 0.35,
            grain_scale: 0.5,
            grain_mode: GrainMode::Stamp,
            spacing: 0.03,
            flow: 0.9,
            roundness: 0.4,
            rotation: std::f64::consts::FRAC_PI_2,
            follow: 1.0,
            scatter: 0.0,
            jitter: 0.0,
            count: 1,
            taper: 0.0,
            wet: false,
            rendering: Rendering::Strongest,
            streamline: 0.0,
    },
    ),
    /// Procraft's `soft_airbrush()`: wide, soft, spaced so its stamps blend into a wash.
    (
        "Soft Airbrush",
        Brush {
            tip: Tip::Cloud,
            hardness: 0.0,
            grain: 0.0,
            grain_scale: 0.0,
            grain_mode: GrainMode::Stamp,
            spacing: 0.04,
            flow: 0.25,
            roundness: 1.0,
            rotation: 0.0,
            follow: 0.0,
            scatter: 0.0,
            jitter: 0.1,
            count: 1,
            taper: 0.0,
            wet: false,
            rendering: Rendering::BuildUp,
            streamline: 0.0,
    },
    ),
];

/// The brush called `name`, or the default if the library has no such brush.
pub fn by_name(name: &str) -> Option<Brush> {
    LIBRARY.iter().find(|(n, _)| *n == name).map(|(_, b)| *b)
}

/// The library as `brush.presets.list` reports it.
pub fn library_json() -> Vec<serde_json::Value> {
    LIBRARY
        .iter()
        .map(|(name, brush)| {
            let mut v = brush.to_json();
            if let serde_json::Value::Object(ref mut o) = v {
                o.insert("name".into(), serde_json::Value::String((*name).to_string()));
            }
            v
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_brush_in_the_library_differs_from_every_other() {
        // The complaint this exists for: eight names that drew the same line. Comparing the whole
        // parameter set is what stops that returning by accident.
        for (i, (a_name, a)) in LIBRARY.iter().enumerate() {
            for (b_name, b) in LIBRARY.iter().skip(i + 1) {
                assert_ne!(a, b, "`{a_name}` and `{b_name}` are the same brush");
            }
        }
    }

    #[test]
    fn a_stroke_from_two_brushes_is_not_the_same_stroke() {
        // Differing parameters are not the same as differing paint. These two are as far apart as
        // the library gets: a hard fine line and a soft grainy one.
        let ink = by_name("Fine Liner").expect("ink");
        let chalk = by_name("6B Pencil").expect("chalk");
        assert!(ink.hardness > chalk.hardness + 0.5);
        assert!(chalk.grain > ink.grain + 0.5);
        assert!(chalk.spacing > ink.spacing);
        assert_ne!(ink.tip, chalk.tip);
        assert_ne!(chalk.count, ink.count);
    }
}
