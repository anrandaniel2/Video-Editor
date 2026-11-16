//! The drawing studio's brushes, as PhotoCraft brush presets.
//!
//! The photo editor and the drawing app are one family: the studio's brush library is ProCraft's,
//! so the Brushes panel here lists the same sets and the same brushes, and a stroke drawn in
//! either app looks the same. ProCraft's brushes are [`BrushSettings`] values on the shared brush
//! model (they differ only in how their studio sliders are bucketed), so the port is a
//! transcription of its library into presets: the same shapes and grains (our own procedural stand-
//! ins for ProCraft's generated ones), the same spacing, flow, hardness and pressure response.
//!
//! Two things carry over as tool state rather than brush settings, exactly as ProCraft keeps them:
//! the *streamline* (the studio's Stabilisation — it smooths the stroke's path, which is
//! `Smoothing` here) and the side sliders. A preset keeps its stabilisation in `smoothing`, so
//! picking a brush brings the same feel to the stroke as in the drawing app.
//!
//! Every brush keeps the name ProCraft gives it and the set it is filed under, so a person moving
//! between the two apps finds the same brush in the same place.

use crate::brush::{
    BrushPreset, BrushSettings, ColorDynamics, Control, Dynamic, MaskMode, Pattern, PatternStyle, PresetCaptures, Smoothing, Texture, TipShape,
};
use crate::procedural::{
    bristle_tip, chalk_tip, charcoal_tip, cloud_tip, flat_tip, grain_tip, grass_tip, hair_tip, leaf_tip, rake_tip, smudge_tip, spatter_tip, sponge_tip, square_tip,
    star_tip,
};
use crate::tile::GrayTile;

/// The studio's brush sets, in the order its library shows them.
pub const SET_NAMES: [&str; 12] = [
    "Sketching",
    "Inking",
    "Painting",
    "Airbrushing",
    "Charcoals",
    "Textures",
    "Drawing",
    "Watercolour",
    "Calligraphy",
    "Spray Paint",
    "Nature",
    "Effects",
];

/// What a studio brush starts as: the round pen, with no stabilisation of its own.
fn pen() -> BrushSettings {
    BrushSettings {
        size: 24.0,
        hardness: 1.0,
        spacing: 0.05,
        smoothing: Smoothing::default(),
        ..BrushSettings::default()
    }
}

/// One preset in `set`, named `name`, built from `brush`.
fn preset(set: &str, name: &str, brush: BrushSettings) -> BrushPreset {
    BrushPreset {
        name: name.to_owned(),
        brush,
        builtin: true,
        group: set.to_owned(),
        folder: Vec::new(),
        // Picking a studio brush keeps the tool's size and opacity, as the drawing app's brush
        // picker does: the side sliders are the tool's, not the preset's.
        captures: PresetCaptures { size: false, tool_settings: false, color: false },
    }
}

/// `b` with the studio's Stabilisation (`streamline`) set: it smooths the path the stroke follows.
fn stabilised(mut b: BrushSettings, streamline: f32) -> BrushSettings {
    b.smoothing = Smoothing {
        enabled: streamline > 0.0,
        amount: streamline.clamp(0.0, 1.0),
        pulled_string: false,
        catch_up: true,
        catch_up_on_end: true,
        adjust_for_zoom: true,
    };
    b
}

/// `b` with a pressure-sized and pressure-faded response, as the studio's brushes mostly carry.
/// `size` and `opacity` are how far pressure takes each down at no pressure (0 = none).
fn pressured(mut b: BrushSettings, size: f32, opacity: f32) -> BrushSettings {
    if size > 0.0 {
        b.pressure_size = true;
        b.shape_dynamics.enabled = true;
        b.shape_dynamics.size = Dynamic { control: Control::PenPressure, minimum: 1.0 - size, ..Dynamic::default() };
    }
    if opacity > 0.0 {
        b.pressure_opacity = true;
        b.transfer.enabled = true;
        b.transfer.opacity = Dynamic { control: Control::PenPressure, minimum: 1.0 - opacity, ..Dynamic::default() };
    }
    b
}

/// A grain fixed to the canvas (ProCraft's `paper`): a procedural tooth at `scale` px.
fn paper(style: PatternStyle, seed: u32, scale: f32, depth: f32) -> Texture {
    Texture { enabled: true, pattern: Pattern::Procedural { style, size: 128, seed }, scale, depth, ..Texture::default() }
}

/// The weave a canvas leaves: it takes paint away where the cloth is low.
fn weave(scale: f32, depth: f32) -> Texture {
    Texture {
        enabled: true,
        pattern: Pattern::Procedural { style: PatternStyle::Canvas, size: 128, seed: 3 },
        mode: MaskMode::Subtract,
        invert: true,
        scale,
        depth,
        ..Texture::default()
    }
}

/// Stamp jitter: each dab a little smaller and fainter, as the studio's charcoals and textures are.
fn jittered(mut b: BrushSettings, size: f32, opacity: f32) -> BrushSettings {
    b.shape_dynamics.enabled = true;
    b.shape_dynamics.size = Dynamic { jitter: size, ..Dynamic::default() };
    b.transfer.enabled = true;
    b.transfer.opacity = Dynamic { jitter: opacity, ..Dynamic::default() };
    b
}

/// Stamps scattered off the line (`throw`, of the stamp's diameter) and turned at random.
fn thrown(mut b: BrushSettings, throw: f32, count: u32) -> BrushSettings {
    b.scattering.enabled = true;
    b.scattering.scatter = Dynamic { jitter: throw, ..Dynamic::default() };
    b.scattering.both_axes = true;
    b.scattering.count = count.clamp(1, 16);
    b.shape_dynamics.enabled = true;
    b.shape_dynamics.angle = Dynamic::jitter(1.0);
    b
}

/// A sampled tip.
fn tip(t: GrayTile) -> TipShape {
    TipShape::Sampled(t)
}

// ---------------------------------------------------------------- Sketching

fn sketching() -> Vec<BrushPreset> {
    let set = "Sketching";
    // Graphite: a grainy disc on paper, a soft pressure curve, a short taper.
    let graphite = pressured(
        BrushSettings {
            size: 18.0,
            hardness: 0.7,
            spacing: 0.06,
            flow: 0.85,
            tip: tip(grain_tip(64, 11)),
            texture: paper(PatternStyle::Paper, 21, 120.0, 0.6),
            ..pen()
        },
        0.3,
        0.9,
    );
    vec![
        preset(set, "Graphite", graphite.clone()),
        preset(
            set,
            "Soft Pencil",
            pressured(BrushSettings { size: 26.0, flow: 0.7, texture: paper(PatternStyle::Paper, 5, 200.0, 0.7), ..graphite.clone() }, 0.3, 0.9),
        ),
        preset(
            set,
            "Technical Pencil",
            pressured(
                BrushSettings {
                    size: 8.0,
                    hardness: 0.95,
                    flow: 1.0,
                    tip: TipShape::Round,
                    texture: paper(PatternStyle::Paper, 21, 120.0, 0.35),
                    ..pen()
                },
                0.1,
                0.6,
            ),
        ),
        preset(
            set,
            "Lead Holder",
            pressured(
                BrushSettings {
                    size: 6.0,
                    hardness: 0.9,
                    flow: 0.9,
                    tip: TipShape::Round,
                    texture: paper(PatternStyle::Paper, 21, 100.0, 0.25),
                    ..pen()
                },
                0.15,
                0.85,
            ),
        ),
        preset(
            set,
            "Coloured Pencil",
            pressured(
                BrushSettings {
                    size: 16.0,
                    hardness: 0.8,
                    flow: 0.8,
                    tip: tip(grain_tip(64, 11)),
                    texture: paper(PatternStyle::Canvas, 9, 90.0, 0.65),
                    ..pen()
                },
                0.3,
                0.95,
            ),
        ),
        preset(
            set,
            "Sketch Pen",
            pressured(BrushSettings { size: 12.0, hardness: 0.92, tip: TipShape::Round, texture: paper(PatternStyle::Paper, 21, 160.0, 0.2), ..pen() }, 0.5, 0.0),
        ),
        preset(
            set,
            "HB Pencil",
            pressured(
                BrushSettings {
                    size: 12.0,
                    hardness: 0.8,
                    flow: 0.75,
                    tip: tip(grain_tip(64, 11)),
                    texture: paper(PatternStyle::Paper, 21, 110.0, 0.5),
                    ..pen()
                },
                0.3,
                0.9,
            ),
        ),
        preset(
            set,
            "6B Pencil",
            pressured(
                BrushSettings {
                    size: 20.0,
                    hardness: 0.6,
                    flow: 0.8,
                    tip: tip(grain_tip(64, 23)),
                    texture: paper(PatternStyle::Paper, 33, 140.0, 0.85),
                    ..pen()
                },
                0.3,
                0.85,
            ),
        ),
        preset(
            set,
            "Graphite Stick",
            pressured(
                jittered(
                    BrushSettings {
                        size: 34.0,
                        hardness: 0.5,
                        spacing: 0.08,
                        flow: 0.7,
                        tip: tip(grain_tip(64, 41)),
                        texture: paper(PatternStyle::Paper, 21, 180.0, 0.7),
                        ..pen()
                    },
                    0.1,
                    0.15,
                ),
                0.4,
                0.8,
            ),
        ),
        preset(
            set,
            "Charcoal Pencil",
            pressured(
                jittered(
                    BrushSettings {
                        size: 16.0,
                        hardness: 0.4,
                        spacing: 0.08,
                        flow: 0.65,
                        tip: tip(charcoal_tip(64, 4)),
                        texture: paper(PatternStyle::Paper, 5, 160.0, 0.8),
                        ..pen()
                    },
                    0.12,
                    0.2,
                ),
                0.4,
                0.8,
            ),
        ),
        preset(
            set,
            "Shading Pencil",
            pressured(BrushSettings { size: 40.0, hardness: 0.2, flow: 0.35, tip: tip(grain_tip(64, 51)), ..pen() }, 0.2, 0.7),
        ),
        preset(
            set,
            "Ballpoint",
            pressured(
                BrushSettings { size: 5.0, hardness: 0.95, flow: 0.85, tip: TipShape::Round, texture: paper(PatternStyle::Paper, 21, 90.0, 0.15), ..pen() },
                0.2,
                0.6,
            ),
        ),
    ]
}

// ---------------------------------------------------------------- Inking

fn inking() -> Vec<BrushPreset> {
    let set = "Inking";
    let round_pen = |size: f32, hardness: f32, flow: f32| BrushSettings { size, hardness, flow, ..pen() };
    vec![
        preset(set, "Round Pen", round_pen(24.0, 1.0, 1.0)),
        preset(set, "Fine Liner", stabilised(pressured(round_pen(20.0, 0.97, 1.0), 0.15, 0.0), 0.06)),
        preset(set, "Brush Pen", stabilised(pressured(round_pen(50.0, 0.95, 1.0), 0.95, 0.0), 0.67)),
        preset(set, "Dip Pen", stabilised(pressured(round_pen(30.0, 0.98, 1.0), 1.0, 0.0), 0.03)),
        preset(
            set,
            "Marker",
            // A chisel tip held at one angle; overlaps in a stroke don't darken.
            BrushSettings { size: 45.0, hardness: 0.9, spacing: 0.03, flow: 0.55, roundness: 0.35, angle: 45.0, ..pen() },
        ),
        preset(
            set,
            "Dry Ink",
            stabilised(pressured(BrushSettings { size: 32.0, hardness: 0.95, texture: paper(PatternStyle::Paper, 7, 220.0, 0.55), ..pen() }, 0.7, 0.0), 0.3),
        ),
        preset(set, "Studio Ink", stabilised(pressured(round_pen(38.0, 1.0, 1.0), 0.9, 0.0), 0.13)),
        preset(set, "Gel Pen", round_pen(9.0, 0.97, 1.0)),
        preset(set, "Thick Liner", stabilised(pressured(round_pen(60.0, 0.97, 1.0), 0.25, 0.0), 0.06)),
        preset(set, "Felt Tip", BrushSettings { size: 18.0, hardness: 0.85, spacing: 0.04, flow: 0.75, texture: paper(PatternStyle::Paper, 13, 120.0, 0.25), ..pen() }),
        preset(set, "Rough Ink", pressured(thrown(round_pen(22.0, 0.95, 1.0), 0.12, 1), 0.6, 0.0)),
        preset(set, "Ink Bleed", pressured(BrushSettings { size: 38.0, hardness: 0.7, flow: 0.95, texture: paper(PatternStyle::Noise, 17, 200.0, 0.3), ..pen() }, 0.7, 0.0)),
    ]
}

// ---------------------------------------------------------------- Painting

fn painting() -> Vec<BrushPreset> {
    let set = "Painting";
    // The Flat Brush: a broad flat stamp turned across the stroke, its weave taking paint away.
    let flat = pressured(
        BrushSettings { size: 110.0, hardness: 0.6, spacing: 0.03, flow: 0.9, tip: tip(flat_tip(96)), angle: 90.0, texture: weave(90.0, 0.35), ..pen() },
        0.4,
        0.4,
    );
    // The Bristle Brush: the flat's bristles, drawn along the line.
    let bristle = pressured(
        BrushSettings { size: 100.0, hardness: 0.55, spacing: 0.03, flow: 0.9, tip: tip(bristle_tip(96, 5)), angle: 90.0, texture: weave(120.0, 0.6), ..pen() },
        0.4,
        0.4,
    );
    vec![
        preset(set, "Round Brush", pressured(BrushSettings { size: 90.0, hardness: 0.6, spacing: 0.05, flow: 0.35, ..pen() }, 0.6, 0.5)),
        preset(set, "Flat Brush", flat.clone()),
        preset(set, "Bristle Brush", bristle.clone()),
        preset(
            set,
            "Dry Brush",
            pressured(
                BrushSettings { size: 90.0, flow: 0.85, tip: tip(flat_tip(96)), angle: 90.0, texture: paper(PatternStyle::Noise, 29, 140.0, 0.5), ..pen() },
                0.4,
                0.4,
            ),
        ),
        preset(
            set,
            "Gouache",
            pressured(
                jittered(
                    BrushSettings { size: 80.0, hardness: 0.75, spacing: 0.04, flow: 0.9, tip: tip(grain_tip(64, 61)), texture: paper(PatternStyle::Paper, 41, 160.0, 0.3), ..pen() },
                    0.08,
                    0.1,
                ),
                0.5,
                0.3,
            ),
        ),
        preset(set, "Oil Paint", pressured(jittered(BrushSettings { size: 110.0, flow: 1.0, ..bristle.clone() }, 0.05, 0.08), 0.4, 0.4)),
        preset(
            set,
            "Palette Knife",
            pressured(
                BrushSettings {
                    size: 90.0,
                    hardness: 1.0,
                    spacing: 0.03,
                    flow: 1.0,
                    tip: tip(square_tip(64)),
                    roundness: 0.25,
                    angle: 90.0,
                    texture: paper(PatternStyle::Noise, 37, 120.0, 0.35),
                    ..pen()
                },
                0.2,
                0.0,
            ),
        ),
        preset(
            set,
            "Filbert",
            pressured(BrushSettings { size: 80.0, hardness: 0.7, spacing: 0.03, flow: 0.9, tip: tip(flat_tip(96)), roundness: 0.6, angle: 90.0, ..pen() }, 0.4, 0.4),
        ),
        preset(
            set,
            "Fan Brush",
            pressured(BrushSettings { size: 90.0, hardness: 0.6, spacing: 0.02, flow: 0.9, tip: tip(hair_tip(96, 13)), angle: 90.0, ..pen() }, 0.4, 0.4),
        ),
        preset(
            set,
            "Rigger",
            stabilised(
                pressured(BrushSettings { size: 12.0, hardness: 0.85, flow: 0.9, texture: paper(PatternStyle::Noise, 43, 80.0, 0.2), ..pen() }, 0.9, 0.0),
                0.03,
            ),
        ),
        preset(set, "Sable", pressured(BrushSettings { size: 60.0, hardness: 0.75, spacing: 0.05, flow: 0.6, tip: tip(cloud_tip(96, 7)), ..pen() }, 0.8, 0.3)),
        preset(
            set,
            "Impasto",
            pressured(
                jittered(
                    BrushSettings { size: 80.0, hardness: 0.9, spacing: 0.04, flow: 1.0, tip: tip(grain_tip(64, 71)), texture: paper(PatternStyle::Noise, 53, 100.0, 0.5), ..pen() },
                    0.1,
                    0.12,
                ),
                0.5,
                0.3,
            ),
        ),
        preset(set, "Acrylic", pressured(jittered(BrushSettings { size: 100.0, flow: 0.95, texture: weave(100.0, 0.45), ..bristle }, 0.04, 0.06), 0.4, 0.3)),
        preset(
            set,
            "Tempera",
            pressured(BrushSettings { size: 30.0, hardness: 0.85, spacing: 0.05, flow: 0.8, texture: paper(PatternStyle::Paper, 23, 120.0, 0.4), ..pen() }, 0.6, 0.5),
        ),
        preset(
            set,
            "Scumble",
            pressured(
                thrown(
                    BrushSettings {
                        size: 120.0,
                        hardness: 0.5,
                        spacing: 0.06,
                        flow: 0.4,
                        tip: tip(grain_tip(64, 83)),
                        texture: paper(PatternStyle::Paper, 67, 200.0, 0.8),
                        ..pen()
                    },
                    0.15,
                    1,
                ),
                0.5,
                0.3,
            ),
        ),
    ]
}

// ---------------------------------------------------------------- Airbrushing

fn airbrushing() -> Vec<BrushPreset> {
    let set = "Airbrushing";
    // The Soft Airbrush: no hardness at all, a low flow, and dabs laid down over time.
    let soft = pressured(
        BrushSettings {
            size: 180.0,
            hardness: 0.0,
            spacing: 0.04,
            flow: 0.08,
            build_up: true,
            build_up_rate: 25.0,
            tip: tip(cloud_tip(128, 3)),
            ..pen()
        },
        0.0,
        0.8,
    );
    vec![
        preset(set, "Soft Airbrush", soft.clone()),
        preset(set, "Hard Airbrush", pressured(BrushSettings { size: 140.0, hardness: 0.85, flow: 0.25, tip: TipShape::Round, ..soft.clone() }, 0.0, 0.8)),
        preset(set, "Medium Airbrush", pressured(BrushSettings { size: 160.0, hardness: 0.5, flow: 0.15, tip: TipShape::Round, ..soft.clone() }, 0.0, 0.8)),
        preset(
            set,
            "Fine Mist",
            pressured(jittered(BrushSettings { size: 200.0, spacing: 0.08, flow: 0.5, tip: tip(spatter_tip(64, 11, 9)), ..soft.clone() }, 0.2, 0.0), 0.0, 0.8),
        ),
        preset(set, "Flat Airbrush", pressured(BrushSettings { size: 160.0, flow: 0.25, roundness: 0.3, angle: 90.0, ..soft.clone() }, 0.0, 0.8)),
        preset(set, "Grainy Airbrush", pressured(BrushSettings { size: 160.0, flow: 0.15, texture: paper(PatternStyle::Noise, 61, 64.0, 0.8), ..soft.clone() }, 0.0, 0.7)),
        preset(set, "Stipple Airbrush", pressured(BrushSettings { size: 160.0, flow: 0.3, texture: paper(PatternStyle::Dots, 71, 128.0, 1.0), ..soft.clone() }, 0.0, 0.7)),
        preset(set, "Feather Airbrush", pressured(BrushSettings { size: 200.0, flow: 0.12, ..soft.clone() }, 0.6, 0.3)),
        preset(set, "Tight Airbrush", pressured(BrushSettings { size: 120.0, hardness: 0.95, flow: 0.5, tip: TipShape::Round, ..soft }, 0.5, 0.3)),
    ]
}

// ---------------------------------------------------------------- Charcoals

fn charcoals() -> Vec<BrushPreset> {
    let set = "Charcoals";
    let stick = pressured(
        jittered(
            BrushSettings { size: 60.0, hardness: 0.35, spacing: 0.08, flow: 0.6, tip: tip(charcoal_tip(64, 4)), texture: paper(PatternStyle::Paper, 5, 260.0, 0.95), ..pen() },
            0.25,
            0.3,
        ),
        0.4,
        0.7,
    );
    vec![
        preset(set, "Charcoal Stick", stick.clone()),
        preset(set, "Willow Charcoal", pressured(BrushSettings { size: 36.0, flow: 0.8, texture: paper(PatternStyle::Paper, 21, 200.0, 0.7), ..stick.clone() }, 0.4, 0.7)),
        preset(set, "Compressed Charcoal", pressured(BrushSettings { size: 52.0, flow: 0.85, texture: paper(PatternStyle::Paper, 7, 200.0, 0.8), ..stick.clone() }, 0.4, 0.7)),
        preset(
            set,
            "Vine Charcoal",
            pressured(jittered(BrushSettings { size: 20.0, flow: 0.55, tip: tip(grain_tip(64, 11)), texture: paper(PatternStyle::Paper, 21, 160.0, 0.85), ..stick.clone() }, 0.15, 0.2), 0.4, 0.7),
        ),
        preset(
            set,
            "Charcoal Dust",
            pressured(
                jittered(
                    BrushSettings { size: 170.0, hardness: 0.0, spacing: 0.1, flow: 0.18, tip: tip(cloud_tip(96, 17)), texture: paper(PatternStyle::Paper, 7, 240.0, 0.9), ..stick.clone() },
                    0.3,
                    0.3,
                ),
                0.2,
                0.8,
            ),
        ),
        preset(
            set,
            "Charcoal Block",
            pressured(
                BrushSettings {
                    size: 100.0,
                    flow: 0.75,
                    tip: tip(square_tip(64)),
                    roundness: 0.35,
                    angle: 90.0,
                    texture: paper(PatternStyle::Paper, 7, 260.0, 0.95),
                    ..stick.clone()
                },
                0.4,
                0.7,
            ),
        ),
        preset(
            set,
            "Burnt Stick",
            pressured(jittered(BrushSettings { size: 44.0, flow: 0.9, tip: tip(grain_tip(64, 23)), texture: paper(PatternStyle::Noise, 31, 220.0, 0.8), ..stick.clone() }, 0.2, 0.25), 0.4, 0.7),
        ),
        preset(
            set,
            "Smudged Charcoal",
            pressured(
                BrushSettings { size: 150.0, hardness: 0.0, flow: 0.3, tip: tip(smudge_tip(96)), texture: paper(PatternStyle::Paper, 21, 200.0, 0.6), ..stick.clone() },
                0.4,
                0.7,
            ),
        ),
        preset(set, "Soft Charcoal", pressured(BrushSettings { size: 52.0, flow: 0.7, texture: paper(PatternStyle::Noise, 19, 160.0, 0.9), ..stick }, 0.4, 0.7)),
    ]
}

// ---------------------------------------------------------------- Textures

fn textures() -> Vec<BrushPreset> {
    let set = "Textures";
    // Every texture brush is the Canvas Texture: a broad soft stamp laying a grain down.
    let canvas = |grain: PatternStyle, seed: u32, scale: f32| {
        pressured(
            BrushSettings {
                size: 220.0,
                hardness: 0.6,
                spacing: 0.05,
                flow: 0.9,
                tip: tip(cloud_tip(96, 5)),
                texture: Texture {
                    enabled: true,
                    pattern: Pattern::Procedural { style: grain, size: 128, seed: seed ^ 0x5 },
                    mode: MaskMode::Subtract,
                    invert: true,
                    scale,
                    depth: 1.0,
                    ..Texture::default()
                },
                ..pen()
            },
            0.0,
            0.6,
        )
    };
    vec![
        preset(set, "Canvas Texture", canvas(PatternStyle::Canvas, 3, 64.0)),
        preset(set, "Speckles", thrown(jittered(BrushSettings { size: 70.0, spacing: 0.6, flow: 1.0, tip: tip(spatter_tip(96, 8, 24)), ..pen() }, 0.6, 0.4), 0.6, 2)),
        preset(set, "Dot Screen", canvas(PatternStyle::Dots, 11, 96.0)),
        preset(set, "Crosshatch", canvas(PatternStyle::Canvas, 23, 160.0)),
        preset(set, "Rough Wall", canvas(PatternStyle::Noise, 31, 256.0)),
        preset(set, "Grit", canvas(PatternStyle::Noise, 41, 128.0)),
        preset(set, "Stipple", canvas(PatternStyle::Dots, 53, 200.0)),
        preset(set, "Wood Grain", canvas(PatternStyle::Paper, 61, 400.0)),
        preset(set, "Brickwork", canvas(PatternStyle::Canvas, 71, 512.0)),
        preset(set, "Fish Scales", canvas(PatternStyle::Dots, 83, 256.0)),
        preset(set, "Cell Wall", canvas(PatternStyle::Canvas, 97, 300.0)),
        preset(set, "Marble", canvas(PatternStyle::Noise, 101, 600.0)),
        preset(set, "Pinstripe", canvas(PatternStyle::Canvas, 103, 160.0)),
        preset(set, "Honeycomb", canvas(PatternStyle::Dots, 107, 400.0)),
        preset(set, "Leather", canvas(PatternStyle::Noise, 109, 300.0)),
        preset(set, "Sandpaper", canvas(PatternStyle::Noise, 113, 128.0)),
        preset(set, "Checkerboard", canvas(PatternStyle::Canvas, 127, 128.0)),
    ]
}

// ---------------------------------------------------------------- Drawing

fn drawing() -> Vec<BrushPreset> {
    let set = "Drawing";
    // Chalk: a pastel stamp on a concrete-toothed paper.
    let chalk = pressured(
        jittered(
            BrushSettings { size: 52.0, hardness: 0.4, spacing: 0.06, flow: 0.75, tip: tip(chalk_tip(64, 7)), texture: paper(PatternStyle::Noise, 7, 220.0, 0.85), ..pen() },
            0.12,
            0.2,
        ),
        0.3,
        0.7,
    );
    vec![
        // The studio's Chalk carries a firm stabilisation of its own.
        preset(set, "Chalk", stabilised(chalk.clone(), 0.43)),
        preset(
            set,
            "Soft Pastel",
            pressured(jittered(BrushSettings { size: 80.0, flow: 0.6, tip: tip(chalk_tip(96, 13)), texture: paper(PatternStyle::Paper, 21, 200.0, 0.75), ..chalk.clone() }, 0.15, 0.15), 0.3, 0.7),
        ),
        preset(
            set,
            "Oil Pastel",
            pressured(jittered(BrushSettings { size: 64.0, flow: 0.95, texture: paper(PatternStyle::Noise, 37, 180.0, 0.55), ..chalk.clone() }, 0.1, 0.15), 0.3, 0.7),
        ),
        preset(
            set,
            "Wax Crayon",
            pressured(BrushSettings { size: 34.0, hardness: 0.85, flow: 0.9, tip: TipShape::Round, texture: paper(PatternStyle::Canvas, 9, 140.0, 0.9), ..chalk.clone() }, 0.3, 0.7),
        ),
        preset(
            set,
            "Conté",
            pressured(BrushSettings { size: 42.0, flow: 0.8, tip: tip(square_tip(64)), roundness: 0.5, angle: 30.0, texture: paper(PatternStyle::Paper, 21, 150.0, 0.7), ..chalk.clone() }, 0.3, 0.7),
        ),
        preset(
            set,
            "Hard Pastel",
            pressured(BrushSettings { size: 46.0, flow: 0.8, tip: tip(square_tip(64)), roundness: 0.6, texture: paper(PatternStyle::Noise, 19, 180.0, 0.8), ..chalk.clone() }, 0.3, 0.7),
        ),
        preset(
            set,
            "Chalk Pencil",
            pressured(BrushSettings { size: 14.0, hardness: 0.8, flow: 0.85, texture: paper(PatternStyle::Paper, 21, 120.0, 0.8), ..chalk.clone() }, 0.3, 0.7),
        ),
        preset(
            set,
            "Pastel Pencil",
            pressured(jittered(BrushSettings { size: 20.0, flow: 0.8, tip: tip(grain_tip(64, 11)), texture: paper(PatternStyle::Canvas, 9, 110.0, 0.7), ..chalk.clone() }, 0.1, 0.15), 0.3, 0.7),
        ),
        preset(
            set,
            "Litho Crayon",
            pressured(BrushSettings { size: 30.0, hardness: 0.9, flow: 1.0, texture: paper(PatternStyle::Paper, 7, 160.0, 0.75), ..chalk }, 0.3, 0.7),
        ),
    ]
}

// ---------------------------------------------------------------- Watercolour

fn watercolour() -> Vec<BrushPreset> {
    let set = "Watercolour";
    // The Wet Wash: thin paint that pools at the rims (wet edges) and mixes with what is under it.
    let wash = pressured(
        BrushSettings {
            size: 220.0,
            hardness: 0.3,
            spacing: 0.05,
            flow: 0.85,
            tip: tip(cloud_tip(96, 13)),
            texture: paper(PatternStyle::Noise, 71, 400.0, 0.45),
            wet_edges: true,
            ..pen()
        },
        0.3,
        0.6,
    );
    vec![
        preset(set, "Wet Wash", wash.clone()),
        preset(set, "Round Wash", pressured(BrushSettings { size: 120.0, hardness: 0.6, flow: 0.9, tip: TipShape::Round, ..wash.clone() }, 0.8, 0.6)),
        preset(set, "Granulating Wash", pressured(BrushSettings { size: 180.0, flow: 0.9, texture: paper(PatternStyle::Noise, 73, 220.0, 0.8), ..wash.clone() }, 0.3, 0.6)),
        preset(
            set,
            "Blot",
            pressured(thrown(jittered(BrushSettings { size: 260.0, spacing: 0.6, flow: 1.0, tip: tip(sponge_tip(96, 31)), ..wash.clone() }, 0.5, 0.3), 0.4, 1), 0.3, 0.6),
        ),
        preset(set, "Bleeding Wash", pressured(BrushSettings { size: 200.0, flow: 0.8, ..wash.clone() }, 0.3, 0.6)),
        preset(set, "Flat Wash", pressured(BrushSettings { size: 160.0, tip: tip(flat_tip(96)), angle: 90.0, ..wash.clone() }, 0.3, 0.6)),
        preset(set, "Dry Wash", pressured(BrushSettings { size: 180.0, flow: 0.7, texture: paper(PatternStyle::Noise, 7, 300.0, 0.9), ..wash.clone() }, 0.3, 0.6)),
        preset(
            set,
            "Salt",
            pressured(thrown(jittered(BrushSettings { size: 160.0, spacing: 0.14, flow: 1.0, tip: tip(sponge_tip(64, 19)), ..wash.clone() }, 0.4, 0.3), 0.2, 1), 0.3, 0.6),
        ),
        preset(
            set,
            "Splash",
            thrown(jittered(BrushSettings { size: 200.0, spacing: 0.3, flow: 1.0, tip: tip(spatter_tip(64, 11, 14)), build_up: true, ..pen() }, 0.6, 0.4), 0.8, 2),
        ),
        preset(set, "Mop Brush", pressured(BrushSettings { size: 200.0, hardness: 0.1, flow: 0.55, tip: tip(cloud_tip(128, 3)), ..wash }, 0.4, 0.6)),
    ]
}

// ---------------------------------------------------------------- Calligraphy

fn calligraphy() -> Vec<BrushPreset> {
    let set = "Calligraphy";
    // A nib held at one angle: broad one way, thin the other.
    let nib = |roundness: f32, angle: f32| {
        stabilised(pressured(BrushSettings { size: 40.0, hardness: 1.0, spacing: 0.02, flow: 1.0, roundness, angle, ..pen() }, 0.35, 0.0), 0.04)
    };
    vec![
        preset(set, "Chisel Nib", nib(0.18, 45.0)),
        preset(set, "Script Brush", stabilised(pressured(BrushSettings { size: 46.0, hardness: 0.95, flow: 1.0, ..pen() }, 1.0, 0.0), 0.09)),
        preset(set, "Monoline", stabilised(BrushSettings { size: 20.0, ..pen() }, 0.66)),
        preset(set, "Ribbon", pressured(BrushSettings { size: 70.0, ..nib(0.08, 30.0) }, 0.1, 0.0)),
        preset(set, "Pointed Nib", stabilised(pressured(BrushSettings { size: 28.0, hardness: 0.98, ..pen() }, 1.0, 0.0), 0.06)),
        preset(set, "Broad Pen", nib(0.2, 60.0)),
        preset(set, "Italic Nib", nib(0.25, 70.0)),
        preset(set, "Felt Brush", pressured(BrushSettings { size: 26.0, hardness: 0.9, flow: 0.95, roundness: 0.4, angle: 50.0, ..pen() }, 0.3, 0.2)),
        preset(set, "Copperplate", stabilised(pressured(BrushSettings { size: 22.0, hardness: 1.0, spacing: 0.03, ..pen() }, 0.9, 0.0), 0.2)),
    ]
}

// ---------------------------------------------------------------- Spray Paint

fn spray_paint() -> Vec<BrushPreset> {
    let set = "Spray Paint";
    let can = pressured(
        jittered(BrushSettings { size: 180.0, hardness: 0.5, spacing: 0.12, flow: 0.85, tip: tip(spatter_tip(96, 11, 30)), ..pen() }, 0.1, 0.1),
        0.4,
        0.5,
    );
    vec![
        preset(set, "Spray Can", thrown(can.clone(), 0.05, 1)),
        preset(set, "Fat Cap", thrown(BrushSettings { size: 320.0, flow: 0.7, ..can.clone() }, 0.1, 2)),
        preset(
            set,
            "Flick",
            thrown(jittered(BrushSettings { size: 220.0, spacing: 1.2, flow: 1.0, tip: tip(spatter_tip(96, 29, 18)), ..can.clone() }, 0.7, 0.2), 1.0, 1),
        ),
        preset(
            set,
            "Mist",
            pressured(
                BrushSettings { size: 380.0, hardness: 0.0, spacing: 0.06, flow: 0.2, tip: tip(cloud_tip(128, 3)), texture: paper(PatternStyle::Noise, 61, 96.0, 0.7), ..can.clone() },
                0.4,
                0.5,
            ),
        ),
        preset(
            set,
            "Drips",
            thrown(jittered(BrushSettings { size: 120.0, spacing: 0.5, flow: 1.0, tip: tip(spatter_tip(64, 43, 8)), ..can.clone() }, 0.4, 0.2), 0.3, 1),
        ),
        preset(set, "Hard Spray", pressured(BrushSettings { size: 160.0, hardness: 0.8, spacing: 0.08, flow: 0.9, tip: tip(spatter_tip(96, 61, 40)), ..can.clone() }, 0.3, 0.4)),
        preset(
            set,
            "Splatter Can",
            thrown(jittered(BrushSettings { size: 240.0, spacing: 0.4, flow: 1.0, tip: tip(spatter_tip(96, 77, 40)), ..can.clone() }, 0.6, 0.2), 0.6, 1),
        ),
        preset(set, "Skinny Cap", pressured(BrushSettings { size: 60.0, hardness: 0.6, spacing: 0.08, flow: 0.7, tip: tip(spatter_tip(64, 11, 10)), ..can }, 0.3, 0.4)),
    ]
}

// ---------------------------------------------------------------- Nature

fn nature() -> Vec<BrushPreset> {
    let set = "Nature";
    // Colour that varies a little from stamp to stamp, as leaves and blades do.
    let natural = |b: BrushSettings| {
        let mut b = jittered(b, 0.25, 0.2);
        b.color_dynamics = ColorDynamics { enabled: true, per_tip: true, hue_jitter: 0.06, brightness_jitter: 0.3, ..ColorDynamics::default() };
        b
    };
    vec![
        preset(set, "Leaves", natural(thrown(BrushSettings { size: 60.0, spacing: 0.5, flow: 1.0, tip: tip(leaf_tip(64)), ..pen() }, 0.4, 1))),
        preset(set, "Grass", natural(thrown(BrushSettings { size: 50.0, spacing: 0.6, flow: 1.0, tip: tip(grass_tip(96, 6, 11)), ..pen() }, 0.3, 2))),
        preset(set, "Clouds", jittered(BrushSettings { size: 220.0, hardness: 0.0, spacing: 0.08, flow: 0.3, tip: tip(cloud_tip(128, 3)), ..pen() }, 0.2, 0.2)),
        preset(
            set,
            "Bark",
            natural(BrushSettings { size: 90.0, flow: 0.9, tip: tip(rake_tip(96, 3, 9)), angle: 90.0, texture: paper(PatternStyle::Paper, 41, 200.0, 0.6), ..pen() }),
        ),
        preset(set, "Foliage", natural(thrown(jittered(BrushSettings { size: 80.0, spacing: 0.5, flow: 0.9, tip: tip(leaf_tip(64)), ..pen() }, 0.3, 0.2), 0.35, 2))),
        preset(set, "Pine", natural(thrown(BrushSettings { size: 70.0, spacing: 0.5, flow: 1.0, tip: tip(grass_tip(64, 17, 9)), ..pen() }, 0.4, 1))),
        preset(set, "Twigs", natural(thrown(BrushSettings { size: 60.0, spacing: 0.5, flow: 0.9, tip: tip(hair_tip(64, 7)), ..pen() }, 0.5, 2))),
        preset(set, "Moss", natural(thrown(jittered(BrushSettings { size: 90.0, spacing: 0.2, flow: 0.8, tip: tip(sponge_tip(96, 41)), ..pen() }, 0.4, 0.2), 0.3, 2))),
        preset(set, "Flowers", natural(thrown(BrushSettings { size: 60.0, spacing: 0.6, flow: 1.0, tip: tip(star_tip(64)), ..pen() }, 0.5, 1))),
        preset(set, "Fur", natural(thrown(BrushSettings { size: 50.0, spacing: 0.3, flow: 0.9, tip: tip(hair_tip(96, 23)), ..pen() }, 0.2, 2))),
        preset(set, "Pebbles", natural(thrown(jittered(BrushSettings { size: 80.0, spacing: 0.5, flow: 1.0, tip: tip(sponge_tip(64, 53)), ..pen() }, 0.4, 0.2), 0.5, 2))),
        preset(set, "Feathers", natural(thrown(BrushSettings { size: 70.0, spacing: 0.4, flow: 0.95, tip: tip(leaf_tip(96)), ..pen() }, 0.3, 1))),
        preset(set, "Reeds", natural(thrown(BrushSettings { size: 70.0, spacing: 0.5, flow: 1.0, tip: tip(grass_tip(96, 29, 7)), ..pen() }, 0.35, 1))),
    ]
}

// ---------------------------------------------------------------- Effects

fn effects() -> Vec<BrushPreset> {
    let set = "Effects";
    let glow = |size: f32, flow: f32| jittered(BrushSettings { size, hardness: 0.0, spacing: 0.06, flow, tip: tip(cloud_tip(96, 3)), ..pen() }, 0.15, 0.15);
    vec![
        preset(set, "Sparkle", thrown(jittered(BrushSettings { size: 60.0, spacing: 0.4, flow: 1.0, tip: tip(star_tip(64)), ..pen() }, 0.5, 0.3), 0.5, 1)),
        preset(set, "Bokeh", thrown(jittered(BrushSettings { size: 160.0, spacing: 0.8, flow: 0.5, tip: tip(cloud_tip(64, 7)), ..pen() }, 0.4, 0.4), 0.6, 1)),
        preset(set, "Smoke", jittered(BrushSettings { size: 240.0, hardness: 0.0, spacing: 0.1, flow: 0.2, tip: tip(cloud_tip(128, 3)), ..pen() }, 0.3, 0.3)),
        preset(set, "Snow", thrown(jittered(BrushSettings { size: 50.0, spacing: 0.5, flow: 0.9, tip: tip(star_tip(48)), ..pen() }, 0.5, 0.3), 0.7, 2)),
        preset(set, "Light Trail", jittered(BrushSettings { size: 80.0, hardness: 0.0, spacing: 0.03, flow: 0.4, tip: tip(cloud_tip(96, 3)), ..pen() }, 0.1, 0.1)),
        preset(set, "Glow", glow(120.0, 0.25)),
        preset(set, "Stars", thrown(jittered(BrushSettings { size: 40.0, spacing: 0.6, flow: 1.0, tip: tip(star_tip(48)), ..pen() }, 0.5, 0.3), 0.8, 1)),
        preset(set, "Bubbles", thrown(jittered(BrushSettings { size: 90.0, spacing: 0.5, flow: 0.6, tip: tip(cloud_tip(64, 11)), ..pen() }, 0.4, 0.3), 0.5, 1)),
        preset(set, "Embers", thrown(jittered(BrushSettings { size: 40.0, spacing: 0.4, flow: 1.0, tip: tip(spatter_tip(64, 23, 6)), ..pen() }, 0.5, 0.4), 0.9, 2)),
        preset(
            set,
            "Rain",
            jittered(BrushSettings { size: 60.0, hardness: 0.6, spacing: 0.4, flow: 0.8, tip: tip(hair_tip(64, 3)), roundness: 0.15, angle: 70.0, ..pen() }, 0.2, 0.2),
        ),
        preset(set, "Snowflakes", thrown(jittered(BrushSettings { size: 60.0, spacing: 0.6, flow: 1.0, tip: tip(star_tip(64)), ..pen() }, 0.5, 0.3), 0.8, 1)),
    ]
}

/// Every set, in the order the drawing app's library shows them.
pub fn sets() -> Vec<(&'static str, Vec<BrushPreset>)> {
    vec![
        ("Sketching", sketching()),
        ("Inking", inking()),
        ("Painting", painting()),
        ("Airbrushing", airbrushing()),
        ("Charcoals", charcoals()),
        ("Textures", textures()),
        ("Drawing", drawing()),
        ("Watercolour", watercolour()),
        ("Calligraphy", calligraphy()),
        ("Spray Paint", spray_paint()),
        ("Nature", nature()),
        ("Effects", effects()),
    ]
}

/// The studio's whole brush library: every set's presets, in the library's order.
///
/// These *replace* the generic preset set ([`crate::presets::builtin`]) in the studio's session:
/// the Brushes panel is the drawing app's Brushes panel, and every stroke a finger draws uses the
/// same brush model as the drawing app's.
pub fn library() -> Vec<BrushPreset> {
    sets().into_iter().flat_map(|(_, brushes)| brushes).collect()
}

/// A brush the library ships, by name, if one does.
pub fn find(name: &str) -> Option<BrushPreset> {
    library().into_iter().find(|p| p.name.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_set_has_brushes_and_each_has_a_unique_name() {
        let mut names = std::collections::HashSet::new();
        for (set, brushes) in sets() {
            assert!(!brushes.is_empty(), "{set} has no brushes");
            for p in brushes {
                assert_eq!(p.group, set, "{} is filed under the wrong set", p.name);
                assert!(names.insert(p.name.clone()), "{} is in the library twice", p.name);
                assert!(p.brush.size > 0.0, "{} has no size", p.name);
                assert!(p.brush.spacing > 0.0, "{} has no spacing", p.name);
            }
        }
        assert!(names.len() >= 100, "the library is the drawing app's, not a handful");
    }

    #[test]
    fn the_library_is_the_brush_sets_in_order() {
        let lib = library();
        assert_eq!(lib.first().map(|p| p.name.as_str()), Some("Graphite"));
        assert!(lib.iter().any(|p| p.name == "Soft Airbrush"));
        assert!(lib.iter().all(|p| p.builtin));
        // Picking a studio brush keeps the tool's size: the side sliders are the tool's.
        assert!(lib.iter().all(|p| !p.captures.size));
    }

    #[test]
    fn stabilisation_carries_a_brushs_streamline() {
        assert!(!find("Soft Pencil").unwrap().brush.smoothing.enabled, "no streamline, no smoothing");
        let brush_pen = find("Brush Pen").unwrap();
        assert!(brush_pen.brush.smoothing.enabled);
        assert!((brush_pen.brush.smoothing.amount - 0.67).abs() < 1e-6);
    }
}
