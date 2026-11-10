//! Text clips: one text layer on a graphic clip, styled the phone's way (a font, a size, a colour,
//! an optional box behind the text, alignment and a width).

use filmcraft_engine::Session;
use filmcraft_geom::Vec2;
use filmcraft_project::graphic::{LayerContent, TEXT_LAYER};
use filmcraft_project::{ClipId, ItemKind, ParamValue, TrackItem};
use filmcraft_render::graphic_clip;
use filmcraft_time::Tick;
use serde_json::{Value, json};

use crate::edits::{self, STILL_SECONDS, ex};
use crate::model;
use crate::{MobileError, presets::PresetInfo};

/// Text alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

/// A text clip's look. Colours are ARGB.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct TextStyle {
    pub font: String,
    pub font_style: String,
    /// Font size in frame pixels.
    pub size: f64,
    pub color: u32,
    /// The box behind the text, if any (its alpha is the box's opacity).
    pub background: Option<u32>,
    /// An outline, if any.
    pub stroke: Option<u32>,
    pub shadow: bool,
    pub align: TextAlign,
    /// The text's width as a fraction of the frame's (0.1 to 1.0).
    pub width: f64,
}

/// Where a selected item sits on the frame, in frame pixels, for drawing its outline and for
/// moving it with a finger.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Placement {
    /// Centre.
    pub x: f64,
    pub y: f64,
    /// Untransformed size of the content.
    pub width: f64,
    pub height: f64,
    /// Scale (1.0 = as placed) and rotation (degrees, clockwise).
    pub scale: f64,
    pub rotation: f64,
}

fn hex(argb: u32) -> String {
    format!("#{:06x}", argb & 0x00ff_ffff)
}

fn alpha(argb: u32) -> f64 {
    f64::from(argb >> 24) / 255.0 * 100.0
}

fn to_argb(c: &[f32; 4]) -> u32 {
    let b = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u32;
    (b(c[3]) << 24) | (b(c[0]) << 16) | (b(c[1]) << 8) | b(c[2])
}

/// The default size of new text: 13 % of the frame's short side, big enough to read on a phone.
fn default_size(w: u32, h: u32) -> f64 {
    f64::from(w.min(h)) * 0.13
}

/// Add a 4-second text clip at the playhead on the lowest free overlay lane. Blank text makes
/// nothing.
pub(crate) fn add_text(s: &mut Session, text: &str) -> Result<Option<u64>, MobileError> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    let (_, seq) = model::sequence(s)?;
    let (w, h) = (seq.settings.width, seq.settings.height);
    let rate = s.sequence_rate();
    let at = rate.snap(s.playhead());
    let end = at + rate.snap_nearest(filmcraft_time::Tick::from_seconds_f64(STILL_SECONDS));
    // the lowest overlay lane free there (never the main track)
    let track = edits::free_lane_index(s, filmcraft_project::TrackKind::Video, 1, None, at, end, None)?;
    let clip = s.edit_group("Add Text", |s| {
        let r = s.execute(
            "graphics.newText",
            json!({
                "text": text, "time": at.0, "seconds": STILL_SECONDS, "size": default_size(w, h),
                "font": "Inter", "fontStyle": "SemiBold", "box": [w, 1], "track": track,
            }),
        )?;
        let clip = r["clip"].as_u64().ok_or_else(|| filmcraft_engine::EngineError::Other("The text could not be added.".into()))?;
        // a box the frame's width, as tall as the text, centred in the frame
        s.execute("graphics.set", json!({"clip": clip, "layer": 0, "props": {"align": "Center", "box_height": 0}}))?;
        recenter(s, clip, f64::from(w) / 2.0, f64::from(h) / 2.0)?;
        Ok(clip)
    })?;
    ex(s, "timeline.select", json!({"clips": [clip]}))?;
    Ok(Some(clip))
}

/// Anchor a text layer at its box's centre and put that centre at (`cx`, `cy`): so it stays
/// put when the text or its size changes, and turns and scales about its middle.
fn recenter(s: &mut Session, clip: u64, cx: f64, cy: f64) -> filmcraft_engine::Result<()> {
    let v = s.execute("graphics.list", json!({"clip": clip}))?;
    let l = v["layers"].as_array().and_then(|a| a.first()).cloned().unwrap_or_default();
    let num = |x: &Value| x.as_f64().filter(|v| v.is_finite()).unwrap_or(0.0);
    let lb = &l["localBounds"];
    let (bw, bh) = (num(&lb[2]) - num(&lb[0]), num(&lb[3]) - num(&lb[1]));
    s.execute("graphics.set", json!({"clip": clip, "layer": 0, "props": {"anchor": [num(&lb[0]) + bw / 2.0, num(&lb[1]) + bh / 2.0], "position": [cx, cy]}}))?;
    Ok(())
}

/// The centre of a text clip's box on the frame.
fn center(s: &mut Session, clip: u64) -> Option<(f64, f64)> {
    placement(s, clip).ok().map(|p| (p.x, p.y))
}

/// Change a text clip's text; blank text deletes the clip.
pub(crate) fn set_text(s: &mut Session, id: u64, text: &str) -> Result<(), MobileError> {
    if text.trim().is_empty() {
        return edits::delete(s, id);
    }
    let (cx, cy) = center(s, id).unwrap_or((0.0, 0.0));
    s.edit_group("Edit Text", |s| {
        s.execute("graphics.setText", json!({"clip": id, "layer": 0, "text": text}))?;
        recenter(s, id, cx, cy)
    })?;
    Ok(())
}

fn layer(c: &TrackItem) -> Option<&filmcraft_project::EffectInstance> {
    c.effects.iter().find(|e| e.effect == TEXT_LAYER)
}

fn p<'a>(e: &'a filmcraft_project::EffectInstance, id: &str) -> Option<&'a ParamValue> {
    e.param(id).map(|p| &p.value)
}

fn f(e: &filmcraft_project::EffectInstance, id: &str, d: f64) -> f64 {
    match p(e, id) {
        Some(ParamValue::Float(v)) => *v,
        _ => d,
    }
}

fn b(e: &filmcraft_project::EffectInstance, id: &str) -> bool {
    matches!(p(e, id), Some(ParamValue::Bool(true)))
}

fn color(e: &filmcraft_project::EffectInstance, id: &str) -> [f32; 4] {
    match p(e, id) {
        Some(ParamValue::Color(c)) => *c,
        _ => [1.0, 1.0, 1.0, 1.0],
    }
}

fn text_p(e: &filmcraft_project::EffectInstance, id: &str) -> String {
    match p(e, id) {
        Some(ParamValue::Text(t)) => t.clone(),
        _ => String::new(),
    }
}

/// A text clip's style.
pub(crate) fn style(s: &Session, id: u64) -> Result<TextStyle, MobileError> {
    let (_, seq) = model::sequence(s)?;
    let c = seq.find_item(ClipId(id)).map(|x| x.1).ok_or_else(|| MobileError::Command("That text is gone.".into()))?;
    let e = layer(c).ok_or_else(|| MobileError::Command("That clip has no text.".into()))?;
    let w = f64::from(seq.settings.width.max(1));
    let align = match p(e, "align") {
        Some(ParamValue::Choice(1)) => TextAlign::Center,
        Some(ParamValue::Choice(2)) => TextAlign::Right,
        _ => TextAlign::Left,
    };
    let bg = b(e, "background").then(|| {
        let mut c = color(e, "background_color");
        c[3] = (f(e, "background_opacity", 100.0) / 100.0) as f32;
        to_argb(&c)
    });
    let box_w = f(e, "box_width", 0.0);
    Ok(TextStyle {
        font: text_p(e, "font"),
        font_style: text_p(e, "font_style"),
        size: f(e, "size", 100.0),
        color: to_argb(&color(e, "fill_color")),
        background: bg,
        stroke: b(e, "stroke").then(|| to_argb(&color(e, "stroke_color"))),
        shadow: b(e, "shadow"),
        align,
        width: if box_w > 0.0 { (box_w / w).clamp(0.1, 1.0) } else { 1.0 },
    })
}

fn props(st: &TextStyle, frame_w: u32) -> Value {
    let width = if st.width.is_finite() { st.width.clamp(0.1, 1.0) } else { 1.0 };
    let size = if st.size.is_finite() { st.size.clamp(4.0, 2000.0) } else { 100.0 };
    json!({
        "font": st.font, "font_style": st.font_style, "size": size,
        "fill": true, "fill_color": hex(st.color),
        "background": st.background.is_some(),
        "background_color": hex(st.background.unwrap_or(0)),
        "background_opacity": st.background.map(alpha).unwrap_or(100.0),
        "background_size": size * 0.25, "background_radius": size * 0.1,
        "stroke": st.stroke.is_some(), "stroke_color": hex(st.stroke.unwrap_or(0xff00_0000)), "stroke_width": (size * 0.06).max(1.0),
        "shadow": st.shadow, "shadow_opacity": 60.0, "shadow_distance": size * 0.06, "shadow_blur": size * 0.2,
        "align": match st.align { TextAlign::Left => "Left", TextAlign::Center => "Center", TextAlign::Right => "Right" },
        "box_width": (f64::from(frame_w) * width).round(),
        "box_height": 0,
    })
}

/// Restyle a text clip; `merge` folds a drag into one undo step.
pub(crate) fn set_style(s: &mut Session, id: u64, st: &TextStyle, merge: bool) -> Result<(), MobileError> {
    let (_, seq) = model::sequence(s)?;
    let w = seq.settings.width;
    let (cx, cy) = center(s, id).unwrap_or((0.0, 0.0));
    let key = format!("style:{id}");
    // a drag (merge) is one undo step: keep the centre in the same merged edit
    let mut q = json!({"clip": id, "layer": 0, "props": props(st, w)});
    if merge {
        q["merge"] = json!(key);
    }
    ex(s, "graphics.set", q)?;
    let v = ex(s, "graphics.list", json!({"clip": id}))?;
    let l = v["layers"].as_array().and_then(|a| a.first()).cloned().unwrap_or_default();
    let num = |x: &Value| x.as_f64().filter(|v| v.is_finite()).unwrap_or(0.0);
    let lb = &l["localBounds"];
    let (bw, bh) = (num(&lb[2]) - num(&lb[0]), num(&lb[3]) - num(&lb[1]));
    let mut r = json!({"clip": id, "layer": 0, "props": {"anchor": [num(&lb[0]) + bw / 2.0, num(&lb[1]) + bh / 2.0], "position": [cx, cy]}});
    if merge {
        r["merge"] = json!(key);
    }
    ex(s, "graphics.set", r)?;
    Ok(())
}

/// Give every text clip this style (keeping each one's size).
pub(crate) fn set_style_everywhere(s: &mut Session, st: &TextStyle) -> Result<(), MobileError> {
    let (_, seq) = model::sequence(s)?;
    let ids: Vec<u64> = seq.video_tracks.iter().flat_map(|t| t.items.iter()).filter(|c| layer(c).is_some()).map(|c| c.id.0).collect();
    s.edit_group("Apply To All", |s| {
        for id in ids {
            let own = style(s, id).map_err(|e| filmcraft_engine::EngineError::Other(e.to_string()))?;
            let st = TextStyle { size: own.size, ..st.clone() };
            let w = s.active_sequence().map(|q| q.settings.width).unwrap_or(1080);
            s.execute("graphics.set", json!({"clip": id, "layer": 0, "props": props(&st, w)}))?;
        }
        Ok(())
    })?;
    Ok(())
}

struct Preset {
    id: &'static str,
    name: &'static str,
    font: &'static str,
    font_style: &'static str,
    color: u32,
    background: Option<u32>,
    stroke: Option<u32>,
    shadow: bool,
}

/// FilmCraft's text styles.
const PRESETS: &[Preset] = &[
    Preset { id: "classic", name: "Classic", font: "Inter", font_style: "SemiBold", color: 0xffff_ffff, background: None, stroke: None, shadow: true },
    Preset {
        id: "serif",
        name: "Serif",
        font: "Noto Serif",
        font_style: "Regular",
        color: 0xffff_ffff,
        background: Some(0xcc12_1212),
        stroke: None,
        shadow: false,
    },
    Preset {
        id: "paper",
        name: "Paper",
        font: "Noto Serif",
        font_style: "Regular",
        color: 0xff11_1111,
        background: Some(0xffff_ffff),
        stroke: None,
        shadow: false,
    },
    Preset { id: "marker", name: "Marker", font: "Inter", font_style: "Bold", color: 0xffff_ffff, background: Some(0xffe8_a400), stroke: None, shadow: false },
    Preset {
        id: "terminal",
        name: "Terminal",
        font: "JetBrains Mono",
        font_style: "Regular",
        color: 0xfff2_f2f2,
        background: Some(0xff11_1111),
        stroke: None,
        shadow: false,
    },
    Preset { id: "pop", name: "Pop", font: "Inter", font_style: "Bold", color: 0xffff_ffff, background: Some(0xffe0_2a6a), stroke: None, shadow: false },
    Preset {
        id: "night",
        name: "Night",
        font: "Inter",
        font_style: "SemiBold",
        color: 0xff8f_b4ff,
        background: Some(0xff16_1a40),
        stroke: None,
        shadow: false,
    },
    Preset {
        id: "outline",
        name: "Outline",
        font: "Inter",
        font_style: "Bold",
        color: 0xffff_ffff,
        background: None,
        stroke: Some(0xff00_0000),
        shadow: false,
    },
    Preset { id: "sunny", name: "Sunny", font: "Inter", font_style: "Bold", color: 0xffff_d60a, background: None, stroke: Some(0xff1a_1a1a), shadow: true },
    Preset { id: "soft", name: "Soft", font: "Inter", font_style: "Medium", color: 0xff1b_1b1b, background: Some(0xccff_f4e6), stroke: None, shadow: false },
];

pub(crate) fn presets() -> Vec<PresetInfo> {
    PRESETS.iter().map(|p| PresetInfo { id: p.id.into(), name: p.name.into() }).collect()
}

/// The style preset `id` would give a clip now styled `own` (size, alignment and width kept).
pub(crate) fn preset_style(id: &str, own: &TextStyle) -> Option<TextStyle> {
    let p = PRESETS.iter().find(|p| p.id == id)?;
    Some(TextStyle {
        font: p.font.into(),
        font_style: p.font_style.into(),
        color: p.color,
        background: p.background,
        stroke: p.stroke,
        shadow: p.shadow,
        ..own.clone()
    })
}

/// The style a text clip had before any preset: the phone's plain default.
pub(crate) fn plain_style(own: &TextStyle) -> TextStyle {
    TextStyle { font: "Inter".into(), font_style: "SemiBold".into(), color: 0xffff_ffff, background: None, stroke: None, shadow: false, ..own.clone() }
}

/// A font family and the styles it actually ships, for the pickers.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FontFamily {
    pub name: String,
    pub styles: Vec<String>,
}

/// The families with their styles: the same curated selection text creation ships with (the
/// app's own fonts — Inter, JetBrains Mono, Noto Serif), not the phone's every system font.
/// A picker that offers "Bold" for a family that only has Regular silently draws the wrong face,
/// so the style list comes from the font itself.
pub(crate) fn font_families(_s: &mut Session) -> Vec<FontFamily> {
    let mut out: Vec<FontFamily> = filmcraft_text::bundled_families()
        .into_iter()
        .map(|(name, styles)| FontFamily { name, styles })
        .collect();
    if out.is_empty() {
        out.push(FontFamily { name: "Inter".into(), styles: vec!["Regular".into(), "Medium".into(), "SemiBold".into(), "Bold".into()] });
    }
    out
}

/// The font families text can use, sorted.
pub(crate) fn fonts(s: &mut Session) -> Vec<String> {
    let Ok(v) = s.execute("fonts.list", json!({"system": true})) else { return vec!["Inter".into()] };
    let mut names: Vec<String> = v.as_array().map(|a| a.iter().filter_map(|f| f["family"].as_str().map(str::to_string)).collect()).unwrap_or_default();
    // the phone's per-script Noto families (hundreds) are not title fonts
    names.retain(|n| {
        let script = (n.starts_with("Noto Sans ") || n.starts_with("Noto Serif ")) && !n.ends_with(" Display");
        !script && !n.is_empty()
    });
    names.sort_by_key(|n| n.to_lowercase());
    names.dedup();
    names
}

/// Where a text clip's layer sits on the frame.
pub(crate) fn placement(s: &mut Session, id: u64) -> Result<Placement, MobileError> {
    let v = ex(s, "graphics.list", json!({"clip": id}))?;
    let l = v["layers"].as_array().and_then(|a| a.first()).ok_or_else(|| MobileError::Command("That clip has no text.".into()))?;
    let num = |x: &Value, d: f64| x.as_f64().filter(|v| v.is_finite()).unwrap_or(d);
    let quad: Vec<(f64, f64)> =
        l["quad"].as_array().map(|q| q.iter().filter_map(|p| Some((p.get(0)?.as_f64()?, p.get(1)?.as_f64()?))).collect()).unwrap_or_default();
    let (cx, cy) = if quad.len() == 4 {
        (quad.iter().map(|p| p.0).sum::<f64>() / 4.0, quad.iter().map(|p| p.1).sum::<f64>() / 4.0)
    } else {
        (num(&l["position"][0], 0.0), num(&l["position"][1], 0.0))
    };
    let lb = &l["localBounds"];
    let (scale, rotation) = (num(&l["scale"], 100.0) / 100.0, num(&l["rotation"], 0.0));
    // the letters themselves when they can be measured, so the outline hugs them
    if let Some((x, y, width, height)) = ink(s, id) {
        return Ok(Placement { x, y, width, height, scale, rotation });
    }
    Ok(Placement { x: cx, y: cy, width: num(&lb[2], 0.0) - num(&lb[0], 0.0), height: num(&lb[3], 0.0) - num(&lb[1], 0.0), scale, rotation })
}

/// The text's lines as laid out (each from its first letter to its last, ascent to descent),
/// rather than the box they wrap in: centre on the frame and unscaled size.
fn ink(s: &Session, id: u64) -> Option<(f64, f64, f64, f64)> {
    let (_, seq) = model::sequence(s).ok()?;
    let it = model::find(seq, ClipId(id))?.clip;
    let size = match s.project.item(it.item).map(|p| &p.kind) {
        Some(ItemKind::Graphic { width, height, .. }) => (*width, *height),
        _ => (seq.settings.width, seq.settings.height),
    };
    let last = (it.end() - Tick(1)).max(it.start);
    let mt = it.source_time_at(s.playhead().max(it.start).min(last));
    let specs = graphic_clip::item_layer_specs(it, mt, size);
    let spec = specs.iter().map(|(_, sp)| sp).find(|sp| matches!(&sp.content, LayerContent::Text(t) if !t.vertical))?;
    let LayerContent::Text(t) = &spec.content else { return None };
    let layout = graphic_clip::text_layout(t);
    let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for line in layout.lines.iter().filter(|l| l.width > 0.0) {
        b = [b[0].min(line.x), b[1].min(line.baseline - line.ascent), b[2].max(line.x + line.width), b[3].max(line.baseline + line.descent)];
    }
    if !(b[2] > b[0] && b[3] > b[1]) {
        return None;
    }
    let mid = Vec2::new(f64::from(b[0] + b[2]) / 2.0, f64::from(b[1] + b[3]) / 2.0);
    let c = graphic_clip::layer_matrix(spec).apply(mid);
    let (w, h) = (f64::from(b[2] - b[0]), f64::from(b[3] - b[1]));
    [c.x, c.y, w, h].iter().all(|v| v.is_finite()).then_some((c.x, c.y, w, h))
}
