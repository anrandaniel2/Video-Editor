//! The document model: a canvas, a tree of layers, and the tools' state.
//!
//! This is the model the restored PhotoCraft studio drives. Its shape is set by the UI, not by
//! what would be tidy here: the studio reads a layer **tree** (groups with children, top first),
//! a list of history labels, and the active/selected layers, and it reads them from one
//! `document.inspect` call. Everything a panel can change is a command that edits this model and
//! returns to the caller through [`crate::photocraft::commands`].
//!
//! What a layer holds is deliberately narrow: 8-bit RGBA pixels at canvas size, or — for an
//! adjustment layer — the settings it applies to what is beneath it. The studio offers three
//! adjustments (curves, levels, brightness/contrast) and five blend modes, so those are what exist.
//! A type layer stores its text and draws it with the layout engine the rest of the app already
//! has; a group holds children.

use serde_json::{Value, json};

/// The most pixels a document may hold.
///
/// The studio's own limit is the phone's: a 24-megapixel picture is 96 MiB of RGBA per layer, and
/// with the undo history that is more than an app gets to keep. 16 MP is 64 MiB a layer, which
/// leaves room for a second layer and its history.
pub const MAX_PIXELS: u64 = 16_000_000;

/// A layer's pixels at full canvas size, 8-bit RGBA, row-major.
///
/// Behind an `Arc` so an undo snapshot costs a refcount rather than a copy of every layer: a 16 MP
/// document is 64 MiB a layer, and 32 of those would be two gigabytes. An edit is the only thing
/// that ever copies, through `Arc::make_mut`.
pub type Pixels = std::sync::Arc<Vec<u8>>;

/// One layer in the tree.
///
/// `children` is only meaningful for a group. A layer's `content` is what the tree row draws its
/// icon from and what the compositor does with it.
#[derive(Clone, Debug)]
pub struct Layer {
    pub id: u64,
    pub name: String,
    pub kind: Kind,
    pub visible: bool,
    pub locked: bool,
    pub opacity: f64,
    pub blend: Blend,
    /// Group clipping: children are clipped to the parent's alpha.
    pub clipped: bool,
    pub children: Vec<Layer>,
    pub content: Content,
}

/// What a layer is, for the row icon and for what the compositor does with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Pixels,
    Group,
    /// An adjustment layer: applies to every layer beneath it, inside its group.
    Adjustment,
    /// A run of text, drawn at `x`,`y`.
    Text,
}

impl Kind {
    /// The name the studio's rows use. The UI compares against `"group"` to decide whether a row
    /// has children, so that one is fixed; the rest are its own vocabulary.
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Pixels => "pixels",
            Kind::Group => "group",
            Kind::Adjustment => "adjustment",
            Kind::Text => "text",
        }
    }
}

/// What a layer holds.
#[derive(Clone, Debug)]
pub enum Content {
    /// Full-canvas RGBA. Allocated lazily for a new layer: `None` means transparent.
    Pixels(Option<Pixels>),
    /// A group has no pixels of its own.
    Group,
    /// An adjustment layer's settings.
    Adjustment(Adjustment),
    /// A text layer.
    Text(TextRun),
}

/// The adjustments the studio offers.
#[derive(Clone, Debug, PartialEq)]
pub enum Adjustment {
    /// The Curves panel's graph: control points in 0..=255, `[input, output]` pairs.
    Curves(Curves),
    /// Levels: black point, white point, gamma, per 1.0 scale (output levels kept as the default).
    Levels { black: f64, white: f64, gamma: f64 },
    /// Brightness and contrast, each in -1..=1.
    BrightnessContrast { brightness: f64, contrast: f64 },
}

/// A curve: per-channel control points, sorted by input.
///
/// The RGB curve is applied to every channel; the per-channel ones after it. The panel sends one
/// channel at a time, so the others stay at their identity.
#[derive(Clone, Debug, PartialEq)]
pub struct Curves {
    pub rgb: Vec<(u8, u8)>,
    pub red: Vec<(u8, u8)>,
    pub green: Vec<(u8, u8)>,
    pub blue: Vec<(u8, u8)>,
}

impl Default for Curves {
    fn default() -> Self {
        Self { rgb: Vec::new(), red: Vec::new(), green: Vec::new(), blue: Vec::new() }
    }
}

impl Curves {
    /// Whether every channel is the identity (nothing to apply).
    pub fn is_identity(&self) -> bool {
        [&self.rgb, &self.red, &self.green, &self.blue].iter().all(|c| curve_is_identity(c))
    }

    /// A 256-entry lookup table for `channel` (0 = all, 1 = red, 2 = green, 3 = blue): the RGB
    /// curve first, then the channel's own.
    pub fn lut(&self, channel: u8) -> [u8; 256] {
        let mut lut = [0u8; 256];
        let own = match channel {
            1 => Some(&self.red),
            2 => Some(&self.green),
            3 => Some(&self.blue),
            _ => None,
        };
        for (i, out) in lut.iter_mut().enumerate() {
            let mut v = i as u8;
            v = apply_curve(&self.rgb, v);
            if let Some(c) = own {
                v = apply_curve(c, v);
            }
            *out = v;
        }
        lut
    }
}

/// Whether a control-point list leaves every input where it is.
fn curve_is_identity(points: &[(u8, u8)]) -> bool {
    points.iter().all(|&(i, o)| i == o)
}

/// Map one value through a curve by linear interpolation between its control points.
///
/// Outside the first and last point the curve is held flat, which is what the panel draws and what
/// every curve tool does.
fn apply_curve(points: &[(u8, u8)], value: u8) -> u8 {
    if points.len() < 2 || curve_is_identity(points) {
        return value;
    }
    let mut sorted: Vec<(i32, i32)> = points.iter().map(|&(i, o)| (i as i32, o as i32)).collect();
    sorted.sort_by_key(|p| p.0);
    let x = value as i32;
    if x <= sorted[0].0 {
        return sorted[0].1 as u8;
    }
    if x >= sorted[sorted.len() - 1].0 {
        return sorted[sorted.len() - 1].1 as u8;
    }
    for pair in sorted.windows(2) {
        let ((x0, y0), (x1, y1)) = (pair[0], pair[1]);
        if x >= x0 && x <= x1 {
            if x1 == x0 {
                return y1 as u8;
            }
            let t = (x - x0) as f64 / (x1 - x0) as f64;
            return ((y0 as f64) + t * ((y1 - y0) as f64)).round().clamp(0.0, 255.0) as u8;
        }
    }
    value
}

/// A run of text on the canvas.
#[derive(Clone, Debug, PartialEq)]
pub struct TextRun {
    pub text: String,
    /// The point the baseline starts at, in document pixels.
    pub x: f64,
    pub y: f64,
    /// Cap height in pixels.
    pub size: f64,
    /// `#RRGGBB`.
    pub color: String,
}

/// The blend modes the studio offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blend {
    Normal,
    Multiply,
    Screen,
    Overlay,
    SoftLight,
}

impl Blend {
    /// The name the studio's panel shows and sends back.
    pub fn as_str(self) -> &'static str {
        match self {
            Blend::Normal => "Normal",
            Blend::Multiply => "Multiply",
            Blend::Screen => "Screen",
            Blend::Overlay => "Overlay",
            Blend::SoftLight => "Soft Light",
        }
    }

    /// The blend the panel sent, or `None` when it named one this engine does not have.
    ///
    /// Returning `None` rather than falling back to Normal matters: a command that quietly
    /// changes the picture to a different blend than the one asked for is worse than one that
    /// says the mode is unknown.
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim() {
            "Normal" => Some(Blend::Normal),
            "Multiply" => Some(Blend::Multiply),
            "Screen" => Some(Blend::Screen),
            "Overlay" => Some(Blend::Overlay),
            "Soft Light" => Some(Blend::SoftLight),
            _ => None,
        }
    }
}

/// The document: a canvas and its layer tree.
#[derive(Clone, Debug)]
pub struct Document {
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// Bottom first, as a painter's list; the studio lists them top first.
    pub layers: Vec<Layer>,
    pub next_layer: u64,
    /// The selection mask, full canvas, 0 = not selected, 255 = selected. `None` = no selection.
    pub selection: Option<Vec<u8>>,
}

impl Document {
    /// A new document with one opaque background layer.
    pub fn new(name: impl Into<String>, width: u32, height: u32, background: &str) -> Result<Self, String> {
        if width == 0 || height == 0 {
            return Err("A canvas needs a width and a height.".into());
        }
        if u64::from(width) * u64::from(height) > MAX_PIXELS {
            return Err(format!(
                "{} × {} is {} megapixels; the most this editor holds is {}.",
                width,
                height,
                (u64::from(width) * u64::from(height)) as f64 / 1e6,
                MAX_PIXELS / 1_000_000
            ));
        }
        let fill = parse_colour(background)?.map(|c| filled(width, height, c));
        let mut doc = Self { name: name.into(), width, height, layers: Vec::new(), next_layer: 1, selection: None };
        let id = doc.take_id();
        doc.layers.push(Layer {
            id,
            name: "Background".into(),
            kind: Kind::Pixels,
            visible: true,
            locked: false,
            opacity: 1.0,
            blend: Blend::Normal,
            clipped: false,
            children: Vec::new(),
            content: Content::Pixels(fill),
        });
        Ok(doc)
    }

    pub fn take_id(&mut self) -> u64 {
        let id = self.next_layer;
        self.next_layer += 1;
        id
    }

    pub fn pixels(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }

    pub fn blank(&self) -> Pixels {
        std::sync::Arc::new(vec![0u8; (self.width as usize) * (self.height as usize) * 4])
    }

    /// The layer with `id`, anywhere in the tree.
    pub fn find(&self, id: u64) -> Option<&Layer> {
        fn walk<'a>(layers: &'a [Layer], id: u64) -> Option<&'a Layer> {
            for l in layers {
                if l.id == id {
                    return Some(l);
                }
                if let Some(found) = walk(&l.children, id) {
                    return Some(found);
                }
            }
            None
        }
        walk(&self.layers, id)
    }

    pub fn find_mut(&mut self, id: u64) -> Option<&mut Layer> {
        fn walk<'a>(layers: &'a mut [Layer], id: u64) -> Option<&'a mut Layer> {
            for l in layers {
                if l.id == id {
                    return Some(l);
                }
                if let Some(found) = walk(&mut l.children, id) {
                    return Some(found);
                }
            }
            None
        }
        walk(&mut self.layers, id)
    }

    /// Remove `id` from wherever it is in the tree.
    pub fn remove(&mut self, id: u64) -> Option<Layer> {
        fn walk(layers: &mut Vec<Layer>, id: u64) -> Option<Layer> {
            if let Some(i) = layers.iter().position(|l| l.id == id) {
                return Some(layers.remove(i));
            }
            for l in layers.iter_mut() {
                if let Some(found) = walk(&mut l.children, id) {
                    return Some(found);
                }
            }
            None
        }
        walk(&mut self.layers, id)
    }

    /// Every layer id in the tree, bottom first.
    pub fn ids(&self) -> Vec<u64> {
        fn walk(layers: &[Layer], out: &mut Vec<u64>) {
            for l in layers {
                out.push(l.id);
                walk(&l.children, out);
            }
        }
        let mut out = Vec::new();
        walk(&self.layers, &mut out);
        out
    }

    /// The tree as the studio's Layers panel reads it: top first, with children nested.
    ///
    /// `active` is the layer being edited and `selected` every layer the user has picked; the row
    /// shows both, and the panel's footer names the active one.
    pub fn inspect(&self, active: Option<u64>, selected: &[u64], history: &[String]) -> Value {
        fn node(l: &Layer, active: Option<u64>, selected: &[u64]) -> Value {
            let mut v = json!({
                "id": l.id,
                "name": l.name,
                "kind": l.kind.as_str(),
                "visible": l.visible,
                "locked": l.locked,
                "opacity": l.opacity,
                "blend": l.blend.as_str(),
                "clipped": l.clipped,
                "selected": selected.contains(&l.id),
                "active": active == Some(l.id),
            });
            let adjustment = match &l.content {
                Content::Adjustment(a) => Some(adjustment_json(a)),
                _ => None,
            };
            if let Some(a) = adjustment {
                v["adjustment"] = a;
            }
            if let Content::Text(t) = &l.content {
                v["text"] = json!(t.text);
                v["size"] = json!(t.size);
                v["color"] = json!(t.color);
                v["x"] = json!(t.x);
                v["y"] = json!(t.y);
            }
            if !l.children.is_empty() {
                v["children"] = Value::Array(l.children.iter().rev().map(|c| node(c, active, selected)).collect());
            }
            v
        }
        json!({
            "name": self.name,
            "width": self.width,
            "height": self.height,
            "activeLayer": active,
            "selectedLayers": selected,
            "hasSelection": self.selection.is_some(),
            "selectionBounds": self.selection.as_ref().map(|m| selection_bounds(m, self.width, self.height)),
            "layers": self.layers.iter().rev().map(|l| node(l, active, selected)).collect::<Vec<_>>(),
            "history": history,
        })
    }
}

/// The bounding box of a selection mask, as `[x, y, width, height]`.
pub fn selection_bounds(mask: &[u8], width: u32, height: u32) -> [i32; 4] {
    let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for y in 0..height as usize {
        for x in 0..width as usize {
            if mask[y * width as usize + x] > 0 {
                x0 = x0.min(x as i32);
                y0 = y0.min(y as i32);
                x1 = x1.max(x as i32 + 1);
                y1 = y1.max(y as i32 + 1);
            }
        }
    }
    if x1 <= x0 {
        return [0, 0, 0, 0];
    }
    [x0, y0, x1 - x0, y1 - y0]
}

/// An adjustment as JSON, for `document.inspect`.
pub fn adjustment_json(a: &Adjustment) -> Value {
    match a {
        Adjustment::Curves(c) => json!({
            "type": "curves",
            "rgb": pairs(c.rgb.clone()),
            "red": pairs(c.red.clone()),
            "green": pairs(c.green.clone()),
            "blue": pairs(c.blue.clone()),
        }),
        Adjustment::Levels { black, white, gamma } => {
            json!({ "type": "levels", "black": black, "white": white, "gamma": gamma })
        }
        Adjustment::BrightnessContrast { brightness, contrast } => {
            json!({ "type": "brightness", "brightness": brightness, "contrast": contrast })
        }
    }
}

fn pairs(points: Vec<(u8, u8)>) -> Value {
    Value::Array(points.into_iter().map(|(i, o)| json!([i, o])).collect())
}

/// A full-canvas buffer filled with one colour.
pub fn filled(width: u32, height: u32, rgba: [u8; 4]) -> Pixels {
    std::sync::Arc::new(rgba.repeat((width as usize) * (height as usize)))
}

/// A colour the panels send: `#RGB`, `#RRGGBB`, `#RRGGBBAA`, or a CSS name.
///
/// `"transparent"` and `"none"` mean no paint (so a new canvas starts empty rather than white);
/// an unknown name is an error, because silently painting the wrong colour is worse than saying so.
pub fn parse_colour(text: &str) -> Result<Option<[u8; 4]>, String> {
    let t = text.trim();
    match t.to_ascii_lowercase().as_str() {
        "transparent" | "none" | "empty" => return Ok(None),
        "white" => return Ok(Some([255, 255, 255, 255])),
        "black" => return Ok(Some([0, 0, 0, 255])),
        _ => {}
    }
    let Some(hex) = t.strip_prefix('#') else {
        return Err(format!("`{text}` is not a colour"));
    };
    let v = |s: &str| u8::from_str_radix(s, 16).map_err(|_| format!("`{text}` is not a colour"));
    let out = match hex.len() {
        3 => {
            let r = v(&hex[0..1])?;
            let g = v(&hex[1..2])?;
            let b = v(&hex[2..3])?;
            [r * 17, g * 17, b * 17, 255]
        }
        6 => [v(&hex[0..2])?, v(&hex[2..4])?, v(&hex[4..6])?, 255],
        8 => [v(&hex[0..2])?, v(&hex[2..4])?, v(&hex[4..6])?, v(&hex[6..8])?],
        _ => return Err(format!("`{text}` is not a colour")),
    };
    Ok(Some(out))
}

/// `#RRGGBB` for a colour, as the panels show it.
pub fn hex(rgba: [u8; 4]) -> String {
    format!("#{:02X}{:02X}{:02X}", rgba[0], rgba[1], rgba[2])
}
