//! The command surface: what the studio's panels and tools send.
//!
//! One function, [`execute`], takes a command id and its JSON parameters and returns the command's
//! JSON result. That indirection is the point of the design: the restored UI is written against
//! command ids, so every button, slider and tool in it is one of the ids below, and the engine
//! cannot drift from the interface it serves.
//!
//! The ids are the ones the studio actually sends — nothing more. A command that is not here does
//! not exist, and [`is_enabled`] answers for the ones that exist but cannot run yet (no document
//! open, no layer to act on), so the UI greys its rows with a reason rather than failing on tap.

use serde_json::{Value, json};

use crate::photocraft::compose;
use crate::photocraft::model::{
    Adjustment, Blend, Content, Curves, Document, Effect, Frame, Kind, Layer, LayerStyle, Node,
    Shape, ShapeKind, TextRun, hex, parse_colour,
};
use crate::photocraft::paint;
use crate::photocraft::select;
use crate::photocraft::Session;

/// A command's failure, as the UI shows it: one message.
pub type Fail = String;

fn bad(what: impl std::fmt::Display) -> Fail {
    what.to_string()
}

/// The ids [`execute`] knows, in the order the studio's command list shows them.
///
/// `command.list` returns this, so a panel can offer the commands that exist without hard-coding
/// them, and a test can assert the surface has not shrunk.
pub const IDS: &[&str] = &[
    "brush.presets.list",
    "brush.set",
    "command.list",
    "document.inspect",
    "document.sampleColor",
    "file.new",
    "image.adjustments.curves",
    "image.adjustments.levels",
    "image.crop",
    "layer.delete",
    "layer.duplicate",
    "layer.layerMask.revealAll",
    "layer.new.group",
    "layer.new.layer",
    "layer.newAdjustmentLayer.curves",
    "layer.newAdjustmentLayer.levels",
    "layer.move",
    "layer.select",
    "layer.setProps",
    "page.add",
    "page.delete",
    "page.duplicate",
    "page.select",
    "paint.bucket",
    "paint.cloneStamp",
    "paint.gradient",
    "paint.pencil",
    "paint.spotHealing",
    "paint.stroke",
    "picture.setProps",
    "select.deselect",
    "select.lasso",
    "select.magicWand",
    "select.rect",
    "layer.style",
    "shape.create",
    "shape.editNodes",
    "type.setProps",
    "shape.setNodes",
    "shape.setProps",
    "tools.setBrush",
    "tools.setColors",
    "tools.swapColors",
    "type.create",
];

/// Whether `id` is a command this engine has.
pub fn known(id: &str) -> bool {
    IDS.contains(&id)
}

/// Run one command against the session.
pub fn execute(session: &mut Session, id: &str, params: Value) -> Result<Value, Fail> {
    match id {
        // ---------------------------------------------------------------- introspection
        "command.list" => Ok(json!(IDS)),
        "brush.presets.list" => Ok(json!({ "presets": brush_presets() })),
        "document.inspect" => {
            let Some(doc) = session.active() else { return Ok(json!({ "layers": [] })) };
            Ok(doc.doc.inspect(doc.active_layer, &doc.selected, &doc.history_labels()))
        }
        "document.sampleColor" => {
            let doc = session.active().ok_or("Open a picture first.")?;
            let (x, y) = (number(&params, "x")? as u32, number(&params, "y")? as u32);
            let composite = compose::render_rect(&doc.doc, x, y, 1, 1);
            if composite.is_empty() {
                return Err("That point is outside the canvas.".into());
            }
            // The panels work in 0..1 floats, as the engine always has.
            Ok(json!([
                f64::from(composite[0]) / 255.0,
                f64::from(composite[1]) / 255.0,
                f64::from(composite[2]) / 255.0,
                f64::from(composite[3]) / 255.0
            ]))
        }

        // ---------------------------------------------------------------- documents
        "file.new" => {
            let width = number(&params, "width")?.max(1.0) as u32;
            let height = number(&params, "height")?.max(1.0) as u32;
            let background = params.get("background").and_then(Value::as_str).unwrap_or("white");
            let doc = Document::new("Untitled", width, height, background).map_err(bad)?;
            let index = session.push(doc);
            session.select_index(index);
            Ok(json!({ "document": index }))
        }

        // ---------------------------------------------------------------- layers
        "layer.new.layer" => {
            let doc = session.active_mut().ok_or("Open a picture first.")?;
            let id = doc.doc.take_id();
            let layer = Layer {
                id,
                name: format!("Layer {id}"),
                kind: Kind::Pixels,
                visible: true,
                locked: false,
                opacity: 1.0,
                blend: Blend::Normal,
                clipped: false,
                children: Vec::new(),
                content: Content::Pixels(None),
                style: LayerStyle::default(),
                offset: (0.0, 0.0),
            };
            // Straight to the top of the tree, which is where a new layer goes and where the
            // studio's Layers panel expects to find it. (An earlier version pulled the active
            // layer out and re-pushed it, which silently reordered the document.)
            doc.push_history("New layer");
            doc.doc.layers_mut().push(layer);
            doc.active_layer = Some(id);
            doc.selected = vec![id];
            doc.touch();
            Ok(json!({ "layer": id }))
        }
        "layer.new.group" => {
            let doc = session.active_mut().ok_or("Open a picture first.")?;
            let id = doc.doc.take_id();
            doc.push_history("New group");
            doc.doc.layers_mut().push(Layer {
                id,
                name: format!("Group {id}"),
                kind: Kind::Group,
                visible: true,
                locked: false,
                opacity: 1.0,
                blend: Blend::Normal,
                clipped: false,
                children: Vec::new(),
                content: Content::Group,
                style: LayerStyle::default(),
                offset: (0.0, 0.0),
            });
            doc.active_layer = Some(id);
            doc.touch();
            Ok(json!({ "layer": id }))
        }
        "layer.duplicate" => {
            let doc = session.active_mut().ok_or("Open a picture first.")?;
            let id = doc.active_layer.ok_or("Select a layer first.")?;
            let source = doc.doc.find(id).cloned().ok_or("That layer is gone.")?;
            doc.push_history("Duplicate layer");
            let new_id = doc.doc.take_id();
            let mut copy = source;
            copy.id = new_id;
            copy.name = format!("{} copy", copy.name);
            rename_descendants(&mut copy, &mut doc.doc);
            doc.doc.layers_mut().push(copy);
            doc.active_layer = Some(new_id);
            doc.touch();
            Ok(json!({ "layer": new_id }))
        }
        "layer.delete" => {
            let doc = session.active_mut().ok_or("Open a picture first.")?;
            let id = doc.active_layer.ok_or("Select a layer first.")?;
            if doc.doc.find(id).is_none() {
                return Err("That layer is gone.".into());
            }
            doc.push_history("Delete layer");
            doc.doc.remove(id);
            doc.active_layer = doc.doc.layers().last().map(|l| l.id);
            doc.touch();
            Ok(json!({ "deleted": id }))
        }
        // `image.crop`: the Crop tool's drag. The rail's crop was mapped to Rect, so the tool drew
        // a rectangle layer where it should have cut the picture down -- a tool whose name and
        // whose consequence disagreed. A crop is a smaller page with everything on it shifted by
        // the same amount, so the picture stays where the artist put it inside the new frame.
        "image.crop" => {
            let doc = session.active_mut().ok_or("Open a picture first.")?;
            let x = number(&params, "x")?;
            let y = number(&params, "y")?;
            let w = number(&params, "width")?;
            let h = number(&params, "height")?;
            let (pw, ph) = (f64::from(doc.doc.width()), f64::from(doc.doc.height()));
            // A crop with no area, or one that reaches nowhere, would leave an unusable page.
            if w < 1.0 || h < 1.0 {
                return Err("A crop needs a width and a height.".into());
            }
            let x0 = x.floor().clamp(0.0, pw - 1.0);
            let y0 = y.floor().clamp(0.0, ph - 1.0);
            let x1 = (x + w).ceil().clamp(x0 + 1.0, pw);
            let y1 = (y + h).ceil().clamp(y0 + 1.0, ph);
            let (cw, ch) = ((x1 - x0) as u32, (y1 - y0) as u32);
            doc.push_history("Crop");
            doc.doc.page_mut().width = cw;
            doc.doc.page_mut().height = ch;
            doc.doc.page_mut().selection = None;
            // Everything moves up and left by the crop's corner, so nothing jumps in the frame.
            // A painted layer's buffer is the page's own size, so it is cut down with the page --
            // an offset alone would leave it reading with the wrong row stride. A shape, a text run
            // and a picture are drawn in document coordinates, so for those the offset is what
            // moves them.
            // A painted layer's buffer is the page's own size, so the old stride is the page's
            // width as it was before the crop.
            let (ox, oy, old_w) = (x0 as u32, y0 as u32, pw as u32);
            for layer in doc.doc.layers_mut() {
                crop_layer(layer, ox, oy, cw, ch, old_w);
            }
            doc.touch();
            Ok(json!({ "width": cw, "height": ch }))
        }
        // `layer.move`: the Move tool's drag, as a nudge of the layer rather than a selection.
        //
        // The rail's Move tool used to make a marquee, which is what the Rect tool does: a tool
        // in a pixel editor that does not move anything. The offset is kept on the layer, so the
        // pixels are never resampled and undoing a move restores it exactly.
        "layer.move" => {
            let doc = session.active_mut().ok_or("Open a picture first.")?;
            let id = match params.get("layer") {
                Some(v) => v.as_u64().ok_or("`layer` is not a layer id.")?,
                None => doc.active_layer.ok_or("Select a layer first.")?,
            };
            let dx = params.get("dx").and_then(Value::as_f64).unwrap_or(0.0);
            let dy = params.get("dy").and_then(Value::as_f64).unwrap_or(0.0);
            if dx == 0.0 && dy == 0.0 {
                return Err("A move needs a distance.".into());
            }
            if doc.doc.find(id).is_none() {
                return Err("That layer is gone.".into());
            }
            doc.push_history_coalescing("Move layer", params.get("coalesce").and_then(Value::as_str));
            let layer = doc.doc.find_mut(id).ok_or("That layer is gone.")?;
            layer.offset.0 += dx;
            layer.offset.1 += dy;
            let offset = layer.offset;
            doc.touch();
            Ok(json!({ "layer": id, "offset": [offset.0, offset.1] }))
        }
        "layer.select" => {
            let doc = session.active_mut().ok_or("Open a picture first.")?;
            let id = number(&params, "layer")? as u64;
            if doc.doc.find(id).is_none() {
                return Err("No such layer.".into());
            }
            doc.active_layer = Some(id);
            doc.selected = vec![id];
            doc.touch();
            Ok(json!({ "layer": id }))
        }
        "layer.setProps" => set_props(session, &params),
        "layer.layerMask.revealAll" => {
            // A reveal-all mask changes nothing about the picture: the layer keeps its own alpha.
            // It exists because the panel has the button, and it records a history step so the
            // user sees the mask was added.
            let doc = session.active_mut().ok_or("Open a picture first.")?;
            let id = doc.active_layer.ok_or("Select a layer first.")?;
            let _ = doc.doc.find(id).ok_or("That layer is gone.")?;
            doc.push_history("Add layer mask");
            doc.touch();
            Ok(json!({ "layer": id, "mask": "revealAll" }))
        }
        "layer.newAdjustmentLayer.curves" => {
            let adjustment = curves_from(&params)?;
            new_adjustment(session, Adjustment::Curves(adjustment), "Curves")
        }
        "layer.newAdjustmentLayer.levels" => {
            new_adjustment(session, levels_from(&params), "Levels")
        }

        // ------------------------------------------------- whole image adjustments
        //
        // These are the classic editor's Curves and Levels panels: they bend the picture that is
        // already there rather than adding a layer above it. `coalesce` is the panel's name for
        // the gesture — a slider drag re-sends its whole value on every tick, so consecutive
        // calls under one name must be a single undo step.
        "image.adjustments.curves" => {
            let adjustment = Adjustment::Curves(curves_from(&params)?);
            apply_to_pixels(session, adjustment, "Curves", &params)
        }
        "image.adjustments.levels" => apply_to_pixels(session, levels_from(&params), "Levels", &params),

        // ---------------------------------------------------------------- tools
        "tools.setBrush" => {
            let t = &mut session.tools;
            if let Some(v) = params.get("size").and_then(Value::as_f64) {
                t.brush_size = v.clamp(0.5, 5000.0);
            }
            if let Some(v) = params.get("hardness").and_then(Value::as_f64) {
                t.brush_hardness = v.clamp(0.0, 1.0);
            }
            if let Some(v) = params.get("opacity").and_then(Value::as_f64) {
                t.brush_opacity = v.clamp(0.0, 1.0);
            }
            if let Some(v) = params.get("preset").and_then(Value::as_str) {
                t.brush_preset = v.to_string();
                // The preset carries its whole tip -- silhouette, spacing, taper, grain -- because
                // selecting a brush has to change the shape of the line, not just its darkness.
                if let Some(brush) = brush_tip(v) {
                    t.brush_hardness = brush.hardness;
                    t.brush_grain = brush.grain;
                    t.brush_wet = if brush.wet { 1.0 } else { 0.0 };
                    t.brush = brush;
                }
            }
            // The reply is the brush itself, named: a panel that edits what a command returns
            // needs to know that picking Charcoal made Charcoal the brush in hand, not only that
            // its hardness changed.
            let mut out = t.brush.to_json_named(&t.brush_preset);
            if let Some(o) = out.as_object_mut() {
                o.insert("size".into(), json!(t.brush_size));
                o.insert("opacity".into(), json!(t.brush_opacity));
                o.insert("preset".into(), json!(t.brush_preset));
            }
            Ok(out)
        }
        // `brush.set`: the brush's own settings, the way the panel's controls send them. The
        // preset list sets a whole brush; this edits the one in hand, which is what makes the
        // panel's spacing and taper sliders mean something.
        "brush.set" => {
            let mut brush = session.tools.brush;
            brush.merge(&params).map_err(bad)?;
            session.tools.brush = brush;
            session.tools.brush_hardness = brush.hardness;
            session.tools.brush_grain = brush.grain;
            session.tools.brush_wet = if brush.wet { 1.0 } else { 0.0 };
            // The reply names the brush, because the panel edits the one in hand: a reply without
            // a name would leave it guessing which brush its sliders now belong to.
            Ok(brush.to_json_named(&session.tools.brush_preset))
        }
        "tools.setColors" => {
            let t = &mut session.tools;
            if let Some(v) = params.get("foreground").and_then(Value::as_str) {
                t.foreground = parse_colour(v).map_err(bad)?.ok_or("The foreground cannot be empty.")?;
            }
            if let Some(v) = params.get("background").and_then(Value::as_str) {
                t.background = parse_colour(v).map_err(bad)?.ok_or("The background cannot be empty.")?;
            }
            Ok(json!({ "foreground": hex(t.foreground), "background": hex(t.background) }))
        }
        "tools.swapColors" => {
            let t = &mut session.tools;
            std::mem::swap(&mut t.foreground, &mut t.background);
            Ok(json!({ "foreground": hex(t.foreground), "background": hex(t.background) }))
        }

        // ---------------------------------------------------------------- paint
        "paint.stroke" => paint::stroke(session, &params),
        // The pencil is the same stamper with a hard, aliased tip. Setting the flag here rather
        // than asking the UI to send it keeps the command id authoritative: the tool rail already
        // decided, and a pencil that drew a soft brush would be the rail lying.
        "paint.pencil" => {
            let mut p = params.clone();
            if let Value::Object(ref mut o) = p {
                o.insert("pencil".into(), Value::Bool(true));
            }
            paint::stroke(session, &p)
        }
        "paint.bucket" => paint::bucket(session, &params),
        "paint.gradient" => paint::gradient(session, &params),
        "paint.cloneStamp" => paint::clone_stamp(session, &params),
        "paint.spotHealing" => paint::spot_healing(session, &params),

        // ---------------------------------------------------------------- selection
        "select.rect" => select::rect(session, &params),
        "select.lasso" => select::lasso(session, &params),
        "select.magicWand" => select::magic_wand(session, &params),
        "select.deselect" => {
            let doc = session.active_mut().ok_or("Open a picture first.")?;
            *doc.doc.selection_mut() = None;
            doc.touch();
            Ok(json!({ "selection": null }))
        }

        // ---------------------------------------------------------------- type
        "type.create" => {
            // Read the colour before borrowing the document: `active_mut` holds the whole session.
            let colour = hex(session.tools.foreground);
            let text = params.get("text").and_then(Value::as_str).unwrap_or("Text").to_string();
            let doc = session.active_mut().ok_or("Open a picture first.")?;
            let id = doc.doc.take_id();
            doc.doc.layers_mut().push(Layer {
                id,
                name: text.chars().take(24).collect(),
                kind: Kind::Text,
                visible: true,
                locked: false,
                opacity: 1.0,
                blend: Blend::Normal,
                clipped: false,
                children: Vec::new(),
                content: Content::Text(TextRun {
                    text,
                    x: number(&params, "x")?,
                    y: number(&params, "y")?,
                    size: number_or(&params, "size", 48.0).clamp(1.0, 2048.0),
                    color: colour,
                    // A Layout text frame wraps to its box; an artistic run has none.
                    frame: frame_from(&params),
                    family: params
                        .get("family")
                        .and_then(Value::as_str)
                        .unwrap_or("Inter")
                        .to_string(),
                    style: text_style(&params),
                    align: text_align(&params),
                    leading: number_or(&params, "leading", 1.2).clamp(0.5, 4.0),
                }),
                style: LayerStyle::default(),
                offset: (0.0, 0.0),
            });
            doc.active_layer = Some(id);
            doc.touch();
            Ok(json!({ "layer": id }))
        }

        // The Text and Paragraph panels: the run's own text, face, size, alignment and leading.
        // Every field the panel shows is a field the run has, so a control here changes the
        // drawing rather than a copy of a value.
        "type.setProps" => {
            let doc = session.active_mut().ok_or("Open a picture first.")?;
            let id = params
                .get("layer")
                .and_then(Value::as_u64)
                .or(doc.active_layer)
                .ok_or("Select a text layer first.")?;
            doc.push_history("Edit text");
            doc.touch();
            let layer = doc.doc.find_mut(id).ok_or("That layer is gone.")?;
            let Content::Text(run) = &mut layer.content else {
                return Err("That layer is not text.".into());
            };
            if let Some(text) = params.get("text").and_then(Value::as_str) {
                run.text = text.to_string();
                // The row is named after what the layer says.
                layer.name = text.chars().take(24).collect();
            }
            if let Some(family) = params.get("family").and_then(Value::as_str) {
                run.family = family.to_string();
            }
            if params.get("style").is_some() {
                run.style = text_style(&params);
            }
            if params.get("align").is_some() {
                run.align = text_align(&params);
            }
            if let Some(size) = params.get("size").and_then(Value::as_f64) {
                run.size = size.clamp(1.0, 2048.0);
            }
            if let Some(leading) = params.get("leading").and_then(Value::as_f64) {
                run.leading = leading.clamp(0.5, 4.0);
            }
            if let Some(colour) = params.get("colour").and_then(Value::as_str) {
                parse_colour(colour)?;
                run.color = colour.to_string();
            }
            if params.get("frame").is_some() {
                run.frame = frame_from(&params);
            }
            Ok(json!({
                "layer": id,
                "text": run.text,
                "family": run.family,
                "style": run.style,
                "align": run.align,
                "size": run.size,
                "leading": run.leading,
            }))
        }

        // ---------------------------------------------------------------- vector shapes
        //
        // The Vector workspace's rectangle, ellipse and pen: kept as geometry, not pixels, so the
        // Stroke panel can change a width after the fact and Transform can move the shape without
        // resampling its edge.
        "shape.create" => {
            let kind = match params.get("kind").and_then(Value::as_str).unwrap_or("rectangle") {
                "ellipse" => ShapeKind::Ellipse,
                "path" => ShapeKind::Path,
                other => {
                    if other == "rectangle" {
                        ShapeKind::Rectangle
                    } else {
                        return Err(format!("`{other}` is not a shape this editor draws."));
                    }
                }
            };
            let mut frame = frame_from(&params).ok_or("A shape needs a frame: x, y, width and height.")?;
            frame.radius = number_or(&params, "radius", 0.0).max(0.0);
            frame.rotation = number_or(&params, "rotation", 0.0);
            let nodes = nodes_from(&params, &frame);
            if kind == ShapeKind::Path && nodes.len() < 2 {
                return Err("A path needs at least two nodes.".into());
            }
            // A shape drawn with the Vector tools takes the foreground for its fill and the
            // background for its outline, which is what the colour well shows at that moment. An
            // explicit null overrides that: a stroke-only shape, or a fill with no outline.
            let colour = hex(session.tools.foreground);
            let outline = hex(session.tools.background);
            let pick = |key: &str, fallback: &str| -> Result<Option<String>, Fail> {
                match params.get(key) {
                    None => Ok(Some(fallback.to_string())),
                    Some(Value::Null) => Ok(None),
                    Some(Value::String(text)) => parse_colour(text)?.map(|_| text.clone()).map(Some).ok_or_else(|| {
                        format!("`{text}` is not a colour")
                    }),
                    Some(_) => Err(format!("`{key}` must be a colour or null.")),
                }
            };
            let shape = Shape {
                frame,
                kind,
                fill: pick("fill", &colour)?,
                stroke: pick("stroke", &outline)?,
                stroke_width: number_or(&params, "strokeWidth", 0.0).clamp(0.0, 500.0),
                nodes,
                // A path from the Pen is open until its ends are joined; a rectangle or an
                // ellipse is closed by construction.
                closed: params
                    .get("closed")
                    .and_then(Value::as_bool)
                    .unwrap_or(kind == ShapeKind::Path && false),
            };
            new_shape(session, shape)
        }
        // The Vector workspace's node tools: they change the node list of the active path, which is
        // what makes that workspace's toolbar do something rather than record a label.
        "shape.setNodes" => {
            let doc = session.active_mut().ok_or("Open a picture first.")?;
            let id = doc.active_layer.ok_or("Select a shape first.")?;
            doc.push_history("Edit nodes");
            doc.touch();
            let layer = doc.doc.find_mut(id).ok_or("That layer is gone.")?;
            let Content::Shape(shape) = &mut layer.content else {
                return Err("That layer is not a shape.".into());
            };
            shape.nodes = nodes_from(&params, &shape.frame);
            shape.closed = params.get("closed").and_then(Value::as_bool).unwrap_or(shape.closed);
            Ok(json!({ "layer": id, "nodes": shape.nodes.len() }))
        }
        // The Effects panel's checkboxes: each one turns a real effect on or off on the active
        // layer, drawn from the layer's own alpha by the compositor. The panel used to toggle a
        // value nothing read, with a line of small print saying the document would apply it.
        "layer.style" => {
            let doc = session.active_mut().ok_or("Open a picture first.")?;
            let id = params
                .get("layer")
                .and_then(Value::as_u64)
                .or(doc.active_layer)
                .ok_or("Select a layer first.")?;
            let name = params
                .get("effect")
                .and_then(Value::as_str)
                .ok_or("`effect` is required.")?;
            let on = params.get("on").and_then(Value::as_bool).unwrap_or(true);
            let effect = name.to_string();
            doc.push_history("Layer effect");
            doc.touch();
            let layer = doc.doc.find_mut(id).ok_or("That layer is gone.")?;
            let Some(field) = layer.style.field_mut(&effect) else {
                return Err(format!("`{effect}` is not a layer effect."));
            };
            if on {
                let colour = params
                    .get("colour")
                    .and_then(Value::as_str)
                    .map(parse_colour)
                    .transpose()?
                    .flatten()
                    .unwrap_or(match effect.as_str() {
                        // A shadow is black and a glow is the colour the tools hold; an overlay is
                        // the foreground, because that is what the panel's well shows.
                        "Outer Shadow" | "Inner Shadow" => [0, 0, 0, 255],
                        _ => [255, 255, 255, 128],
                    });
                *field = Some(Effect {
                    colour,
                    radius: number_or(&params, "radius", 8.0).clamp(0.0, 200.0),
                    opacity: number_or(&params, "opacity", 0.75).clamp(0.0, 1.0),
                });
            } else {
                *field = None;
            }
            Ok(json!({ "layer": id, "effect": effect, "on": on, "style": doc.doc.find(id).map(|l| l.style.to_json()) }))
        }

        // The node toolbar's actions, applied to the selected path's own node list. Sharp,
        // smooth and smart node set the handles; break, join and close set the ends. Each keeps
        // the path geometry, which is what makes the toolbar do something rather than record
        // which button was pressed last.
        "shape.editNodes" => {
            let doc = session.active_mut().ok_or("Open a picture first.")?;
            let id = doc
                .active_layer
                .ok_or("Select a path first.")?;
            doc.push_history("Edit nodes");
            doc.touch();
            let layer = doc.doc.find_mut(id).ok_or("That layer is gone.")?;
            let Content::Shape(shape) = &mut layer.content else {
                return Err("That layer is not a shape.".into());
            };
            if shape.kind != ShapeKind::Path {
                return Err("Only a path has nodes to edit.".into());
            }
            let action = params
                .get("action")
                .and_then(Value::as_str)
                .ok_or("`action` is required: sharp, smooth, smart, break, join or close.")?;
            let n = shape.nodes.len();
            match action {
                // A sharp node's handles lie on its own point, so the join is a corner.
                "sharp" => {
                    for node in &mut shape.nodes {
                        node.in_x = 0.0;
                        node.in_y = 0.0;
                        node.out_x = 0.0;
                        node.out_y = 0.0;
                        node.smooth = false;
                    }
                }
                // A smooth node's handles follow the line through its neighbours, which is what
                // makes the join continuous rather than a corner.
                "smooth" | "smart" => {
                    for i in 0..n {
                        let prev = shape.nodes[(i + n - 1) % n];
                        let next = shape.nodes[(i + 1) % n];
                        let (dx, dy) = (next.x - prev.x, next.y - prev.y);
                        let len = (dx * dx + dy * dy).sqrt().max(1e-6);
                        let (ux, uy) = (dx / len * 8.0, dy / len * 8.0);
                        let node = &mut shape.nodes[i];
                        if action == "smooth" {
                            node.in_x = -ux;
                            node.in_y = -uy;
                            node.out_x = ux;
                            node.out_y = uy;
                        }
                        node.smooth = true;
                    }
                }
                // Breaking a closed path opens it at the end; joining closes it again.
                "break" => shape.closed = false,
                "join" | "close" => shape.closed = n >= 3,
                other => return Err(format!("`{other}` is not a node action.")),
            }
            Ok(json!({ "layer": id, "nodes": n, "closed": shape.closed, "smooth": shape.nodes.iter().any(|n| n.smooth) }))
        }

        "shape.setProps" => {
            let doc = session.active_mut().ok_or("Open a picture first.")?;
            // A panel that edits a shape by name passes its layer; a toolbar acting on what is
            // selected passes nothing, and gets the active layer.
            let id = params
                .get("layer")
                .and_then(Value::as_u64)
                .or(doc.active_layer)
                .ok_or("Select a shape first.")?;
            doc.push_history("Shape");
            doc.touch();
            let layer = doc.doc.find_mut(id).ok_or("That layer is gone.")?;
            let Content::Shape(shape) = &mut layer.content else {
                return Err("That layer is not a shape.".into());
            };
            // The fill is stored as the colour text the draw resolves, so that a shape keeps the
            // name it was given; it is parsed here to reject a colour that would never draw.
            // A null is meaningful here: it asks for no fill at all, which `as_str` cannot see.
            if let Some(value) = params.get("fill") {
                shape.fill = match value.as_str() {
                    Some(text) => parse_colour(text)?.map(|_| text.to_string()),
                    None if value.is_null() => None,
                    None => return Err("`fill` must be a colour or null.".into()),
                };
            }
            if let Some(value) = params.get("stroke") {
                shape.stroke = match value.as_str() {
                    Some(text) => parse_colour(text)?.map(|_| text.to_string()),
                    None if value.is_null() => None,
                    None => return Err("`stroke` must be a colour or null.".into()),
                };
            }
            // The frame is what Transform's X, Y, Width, Height and Rotation set. Moving a shape
            // this way keeps it geometry: nothing is resampled.
            let get = |k: &str| params.get(k).and_then(Value::as_f64);
            if let Some(v) = get("x") { shape.frame.x = v; }
            if let Some(v) = get("y") { shape.frame.y = v; }
            if let Some(v) = get("width") { shape.frame.width = v.max(0.0); }
            if let Some(v) = get("height") { shape.frame.height = v.max(0.0); }
            if let Some(v) = get("rotation") { shape.frame.rotation = v; }
            if let Some(v) = get("radius") { shape.frame.radius = v.max(0.0); }
            if let Some(v) = params.get("strokeWidth").and_then(Value::as_f64) {
                shape.stroke_width = v.clamp(0.0, 500.0);
            }
            doc.touch();
            Ok(json!({ "layer": id }))
        }

        // A placed picture has a frame of its own, so the same Transform fields move it without
        // its pixels being resampled -- and Fit, Fill and Stretch decide what happens inside it.
        "picture.setProps" => {
            let doc = session.active_mut().ok_or("Open a picture first.")?;
            let id = params
                .get("layer")
                .and_then(Value::as_u64)
                .or(doc.active_layer)
                .ok_or("Select a picture first.")?;
            doc.push_history("Picture");
            doc.touch();
            let layer = doc.doc.find_mut(id).ok_or("That layer is gone.")?;
            let Content::Picture(picture) = &mut layer.content else {
                return Err("That layer is not a picture.".into());
            };
            let get = |k: &str| params.get(k).and_then(Value::as_f64);
            if let Some(v) = get("x") { picture.frame.x = v; }
            if let Some(v) = get("y") { picture.frame.y = v; }
            if let Some(v) = get("width") { picture.frame.width = v.max(0.0); }
            if let Some(v) = get("height") { picture.frame.height = v.max(0.0); }
            if let Some(v) = get("rotation") { picture.frame.rotation = v; }
            if let Some(kind) = params.get("fit").and_then(Value::as_str) {
                use crate::photocraft::model::PictureFit;
                picture.fit = match kind {
                    "fit" => PictureFit::Fit,
                    "fill" => PictureFit::Fill,
                    "stretch" => PictureFit::Stretch,
                    other => return Err(format!("`{other}` is not a picture fit: fit, fill or stretch.")),
                };
            }
            doc.touch();
            Ok(json!({ "layer": id }))
        }

        // ---------------------------------------------------------------- pages
        //
        // A page is a canvas and its own layer tree. The Pages panel adds, duplicates and deletes
        // them, and the active page is what the canvas and every tool show.
        "page.add" => {
            let doc = session.active_mut().ok_or("Open a picture first.")?;
            doc.push_history("Add page");
            let (w, h) = (doc.doc.width(), doc.doc.height());
            let width = number_or(&params, "width", f64::from(w)).clamp(1.0, 20000.0) as u32;
            let height = number_or(&params, "height", f64::from(h)).clamp(1.0, 20000.0) as u32;
            let name = params.get("name").and_then(Value::as_str).map(str::to_string);
            let at = doc.doc.add_page(name, width, height).map_err(bad)?;
            doc.doc.active_page = at;
            doc.active_layer = doc.doc.layers().last().map(|l| l.id);
            doc.touch();
            Ok(json!({ "page": at, "pages": doc.doc.pages.len(), "width": width, "height": height }))
        }
        "page.duplicate" => {
            let doc = session.active_mut().ok_or("Open a picture first.")?;
            doc.push_history("Duplicate page");
            let at = doc.doc.active_page;
            let mut copy = doc.doc.pages[at].clone();
            copy.name = format!("{} copy", copy.name);
            // A copied layer is a new layer: it needs its own ids, or setProps on one of them
            // would change both.
            // The ids come out first: a closure over `doc` cannot be handed to a loop that is
            // already borrowing `doc`'s pages through `copy`.
            let mut ids: Vec<u64> = Vec::new();
            let count: usize = copy.layers.iter().map(layer_count).sum();
            for _ in 0..count {
                ids.push(doc.doc.take_id());
            }
            let mut next = ids.into_iter();
            let mut give = || next.next().unwrap_or(0);
            for layer in &mut copy.layers {
                give_ids(layer, &mut give);
            }
            let to = at + 1;
            doc.doc.pages.insert(to, copy);
            doc.doc.active_page = to;
            doc.active_layer = doc.doc.layers().last().map(|l| l.id);
            doc.touch();
            Ok(json!({ "page": to, "pages": doc.doc.pages.len() }))
        }
        "page.delete" => {
            let doc = session.active_mut().ok_or("Open a picture first.")?;
            if doc.doc.pages.len() <= 1 {
                return Err("A document needs at least one page.".into());
            }
            doc.push_history("Delete page");
            let at = doc.doc.active_page;
            doc.doc.pages.remove(at);
            doc.doc.active_page = at.min(doc.doc.pages.len() - 1);
            doc.active_layer = doc.doc.layers().last().map(|l| l.id);
            doc.touch();
            Ok(json!({ "page": doc.doc.active_page, "pages": doc.doc.pages.len() }))
        }
        "page.select" => {
            let doc = session.active_mut().ok_or("Open a picture first.")?;
            let at = number(&params, "page")? as usize;
            if at >= doc.doc.pages.len() {
                return Err(format!("There is no page {}.", at + 1));
            }
            doc.doc.active_page = at;
            doc.active_layer = doc.doc.layers().last().map(|l| l.id);
            // A selection belongs to the page it was made on.
            doc.selected.clear();
            doc.touch();
            Ok(json!({ "page": at, "name": doc.doc.pages[at].name }))
        }

        other => Err(format!("`{other}` is not a command this editor has.")),
    }
}

/// Whether a command can run right now, and if not, why.
pub fn enabled(session: &Session, id: &str) -> Result<(), Fail> {
    if !known(id) {
        return Err(format!("`{id}` is not a command this editor has."));
    }
    let needs_document = !matches!(
        id,
        "command.list" | "brush.presets.list" | "file.new" | "tools.setBrush" | "tools.setColors" | "tools.swapColors"
    );
    if needs_document && session.active().is_none() {
        return Err("Open a picture first.".into());
    }
    let needs_layer = matches!(
        id,
        "layer.delete"
            | "layer.duplicate"
            | "layer.setProps"
            | "layer.layerMask.revealAll"
            | "paint.stroke"
            | "paint.pencil"
            | "paint.bucket"
            | "paint.gradient"
            | "paint.cloneStamp"
            | "paint.spotHealing"
            | "image.adjustments.curves"
            | "image.adjustments.levels"
    );
    if needs_layer {
        let Some(doc) = session.active() else { return Err("Open a picture first.".into()) };
        let Some(id) = doc.active_layer else { return Err("Select a layer first.".into()) };
        let Some(layer) = doc.doc.find(id) else { return Err("That layer is gone.".into()) };
        if layer.locked {
            return Err(format!("`{}` is locked.", layer.name));
        }
        if layer.kind == Kind::Adjustment {
            return Err("This layer has no pixels of its own to paint on.".into());
        }
        if layer.kind == Kind::Group {
            return Err("A group holds layers; paint on one of them.".into());
        }
    }
    Ok(())
}

/// Cut a layer down to the crop's rectangle, which is what a crop does to what it keeps.
fn crop_layer(
    layer: &mut crate::photocraft::model::Layer,
    x0: u32,
    y0: u32,
    w: u32,
    h: u32,
    stride: u32,
) {
    use crate::photocraft::model::Content;
    match &mut layer.content {
        Content::Pixels(Some(pixels)) => {
            // The buffer is the page as it was. Read the part that survived, at the old stride.
            let mut cut = vec![0u8; (w as usize) * (h as usize) * 4];
            for row in 0..h as usize {
                let from = ((y0 as usize + row) * stride as usize + x0 as usize) * 4;
                let to = row * w as usize * 4;
                let len = w as usize * 4;
                if from + len <= pixels.len() {
                    cut[to..to + len].copy_from_slice(&pixels[from..from + len]);
                }
            }
            *pixels = std::sync::Arc::new(cut);
        }
        // A shape, a text run and a picture are drawn where they say they are, in document
        // coordinates: the offset is what moves them into the new frame.
        Content::Shape(_) | Content::Text(_) | Content::Picture(_) => {
            layer.offset.0 -= f64::from(x0);
            layer.offset.1 -= f64::from(y0);
        }
        _ => {}
    }
    for child in &mut layer.children {
        crop_layer(child, x0, y0, w, h, stride);
    }
}

/// The brushes the studio's panel lists, in its own order and with its own names.
///
/// These are the eight the interface declares — the same eight Procraft offers — and each one is
/// a real tip, not a label: the hardness is where the falloff begins, the grain is how much the
/// edge is broken up, and the wetness makes a stroke build up rather than lay down flat colour.
/// The panel sends its brush name back with `tools.setBrush` and every stroke afterwards carries
/// it, so picking "Compressed Charcoal" has to change what the brush does.
/// A preset's brush, or `None` for a name the panel does not list.
///
/// The tip is not a pair of numbers any more: it has a silhouette, a spacing, a taper and its own
/// grain, and all of them come from the brush the panel named. Anything the panel does not list
/// falls back to the brush the studio is already using.
fn brush_tip(name: &str) -> Option<super::brush::Brush> {
    super::brush::by_name(name)
}

fn brush_presets() -> Vec<Value> {
    super::brush::library_json()
}

/// `layer.setProps`: visibility, lock, opacity, blend, name, clipping.
fn set_props(session: &mut Session, params: &Value) -> Result<Value, Fail> {
    let doc = session.active_mut().ok_or("Open a picture first.")?;
    let id = match params.get("layer") {
        Some(v) => v.as_u64().ok_or("`layer` is not a layer id.")?,
        None => doc.active_layer.ok_or("Select a layer first.")?,
    };
    let blend = match params.get("blend").and_then(Value::as_str) {
        Some(name) => Some(Blend::parse(name).ok_or_else(|| {
            format!("`{name}` is not a blend mode this editor has (Normal, Multiply, Screen, Overlay, Soft Light).")
        })?),
        None => None,
    };
    let layer = doc.doc.find_mut(id).ok_or("No such layer.")?;
    if let Some(v) = params.get("visible").and_then(Value::as_bool) {
        layer.visible = v;
    }
    if let Some(v) = params.get("locked").and_then(Value::as_bool) {
        layer.locked = v;
    }
    if let Some(v) = params.get("opacity").and_then(Value::as_f64) {
        layer.opacity = v.clamp(0.0, 1.0);
    }
    if let Some(v) = params.get("clipped").and_then(Value::as_bool) {
        layer.clipped = v;
    }
    if let Some(v) = params.get("name").and_then(Value::as_str) {
        layer.name = v.to_string();
    }
    if let Some(b) = blend {
        layer.blend = b;
    }
    let out = json!({
        "id": layer.id,
        "name": layer.name,
        "visible": layer.visible,
        "locked": layer.locked,
        "opacity": layer.opacity,
        "blend": layer.blend.as_str(),
        "clipped": layer.clipped,
    });
    doc.touch();
    Ok(out)
}

/// Add an adjustment layer above the active one.
fn new_adjustment(session: &mut Session, adjustment: Adjustment, name: &str) -> Result<Value, Fail> {
    let doc = session.active_mut().ok_or("Open a picture first.")?;
    doc.push_history(name);
    let id = doc.doc.take_id();
    doc.doc.layers_mut().push(Layer {
        id,
        name: name.to_string(),
        kind: Kind::Adjustment,
        visible: true,
        locked: false,
        opacity: 1.0,
        blend: Blend::Normal,
        clipped: false,
        children: Vec::new(),
        content: Content::Adjustment(adjustment),
        style: LayerStyle::default(),
        offset: (0.0, 0.0),
    });
    doc.active_layer = Some(id);
    doc.touch();
    Ok(json!({ "layer": id }))
}

/// The curves the panel sends: one channel at a time, as `[[in, out], …]` in 0..=255.
///
/// The panel names the channel it edited (`red`, `green`, `blue`, or `points` for the RGB curve),
/// so the other channels start at their identity and stay there.
/// Bend the active layer's own pixels — the classic editor's Curves and Levels panels.
///
/// These change the picture rather than adding a layer above it, so they edit the pixels of the
/// layer being worked on, inside the selection when there is one. `params.coalesce` names the
/// gesture, so one slider drag is one undo step.
fn apply_to_pixels(
    session: &mut Session,
    adjustment: Adjustment,
    label: &str,
    params: &Value,
) -> Result<Value, Fail> {
    let coalesce = params.get("coalesce").and_then(Value::as_str);
    let doc = session.active_mut().ok_or("Open a picture first.")?;
    let id = doc.active_layer.ok_or("Select a layer first.")?;
    // Checked before the snapshot is taken: a command that is going to refuse must not leave an
    // undo step behind for an edit that did not happen.
    {
        let layer = doc.doc.find(id).ok_or("That layer is gone.")?;
        if layer.locked {
            return Err(format!("`{}` is locked.", layer.name));
        }
        if !matches!(layer.kind, Kind::Pixels) {
            return Err("This layer has no pixels of its own to adjust.".into());
        }
    }
    doc.push_history_coalescing(label, coalesce);
    doc.touch();
    let (width, height) = (doc.doc.width() as usize, doc.doc.height() as usize);
    let selection = doc.doc.selection().clone();
    let layer = doc.doc.find_mut(id).ok_or("That layer is gone.")?;
    let Content::Pixels(slot) = &mut layer.content else { unreachable!() };
    let px = std::sync::Arc::make_mut(
        slot.get_or_insert_with(|| std::sync::Arc::new(vec![0u8; width * height * 4])),
    );
    // Only where the selection allows it: an adjustment is still confined by the marching ants.
    if let Some(sel) = selection {
        let mut part = Vec::with_capacity(px.len());
        for i in 0..width * height {
            if sel[i] > 0 {
                part.extend_from_slice(&px[i * 4..i * 4 + 4]);
            } else {
                part.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
        compose::adjust_pixels(&mut part, &adjustment);
        for i in 0..width * height {
            if sel[i] > 0 {
                px[i * 4..i * 4 + 4].copy_from_slice(&part[i * 4..i * 4 + 4]);
            }
        }
    } else {
        compose::adjust_pixels(px, &adjustment);
    }
    Ok(json!({ "layer": id }))
}

/// The Levels panel's three numbers, in the two shapes it is sent in.
///
/// The panel sends `inBlack`/`inWhite` on the 0..=255 scale the picture uses; the layer command
/// and any caller reading the engine directly use `black`/`white` on 0..=1. Accepting both keeps
/// the sliders meaning what they show, which the 0..=1 reader would have quietly broken: a black
/// point of 40 would have clamped to 1.0 and crushed the whole picture.
fn levels_from(params: &Value) -> Adjustment {
    let (black, white) = match params.get("inBlack").and_then(Value::as_f64) {
        Some(v) => (
            (v / 255.0).clamp(0.0, 1.0),
            (number_or(params, "inWhite", 255.0) / 255.0).clamp(0.0, 1.0),
        ),
        None => (
            number_or(params, "black", 0.0).clamp(0.0, 1.0),
            number_or(params, "white", 1.0).clamp(0.0, 1.0),
        ),
    };
    Adjustment::Levels {
        black,
        white,
        gamma: number_or(params, "gamma", 1.0).clamp(0.1, 10.0),
    }
}

/// A frame from a command's parameters: `x`,`y`,`width`,`height`, with optional roundness and
/// rotation. `None` when the command carries no box at all, which is how an artistic text run
/// says it has no frame.
/// The face weight, from the panel's own vocabulary.
fn text_style(params: &Value) -> String {
    let face = params.get("style").and_then(Value::as_str).unwrap_or("Regular");
    match face.to_ascii_lowercase().replace([' ', '_'], "-").as_str() {
        "bold" => "Bold",
        "italic" => "Italic",
        "bold-italic" | "italic-bold" => "Bold Italic",
        _ => "Regular",
    }
    .to_string()
}

/// The paragraph alignment, from the panel's icon names.
fn text_align(params: &Value) -> String {
    let a = params.get("align").and_then(Value::as_str).unwrap_or("left");
    match a.to_ascii_lowercase().as_str() {
        "center" | "centre" | "center-align" => "center",
        "right" => "right",
        "justify" => "justify",
        _ => "left",
    }
    .to_string()
}

fn frame_from(params: &Value) -> Option<Frame> {
    let get = |k: &str| params.get(k).and_then(Value::as_f64);
    let (x, y) = (get("x").unwrap_or(0.0), get("y").unwrap_or(0.0));
    let (width, height) = (get("width"), get("height"));
    match (width, height) {
        (Some(width), Some(height)) if width > 0.0 && height > 0.0 => Some(Frame {
            x,
            y,
            width,
            height,
            radius: get("radius").unwrap_or(0.0).max(0.0),
            rotation: get("rotation").unwrap_or(0.0),
        }),
        _ => None,
    }
}

/// A path's node list, from `nodes: [{x, y, inX, inY, outX, outY, smooth}]`.
///
/// The points are document pixels; a path's nodes are stored relative to its frame, so a shape
/// can be moved without rewriting every node.
fn nodes_from(params: &Value, frame: &Frame) -> Vec<Node> {
    let Some(arr) = params.get("nodes").and_then(Value::as_array) else { return Vec::new() };
    arr.iter()
        .filter_map(|n| {
            let o = n.as_object()?;
            let f = |k: &str| o.get(k).and_then(Value::as_f64).unwrap_or(0.0);
            let mut node = Node::sharp(f("x") - frame.x, f("y") - frame.y);
            node.in_x = f("inX");
            node.in_y = f("inY");
            node.out_x = f("outX");
            node.out_y = f("outY");
            node.smooth = o.get("smooth").and_then(Value::as_bool).unwrap_or(false);
            Some(node)
        })
        .collect()
}

/// Give a copied layer tree fresh ids, so a duplicate's layers are their own.
/// How many ids a layer and its children need.
fn layer_count(layer: &Layer) -> usize {
    1 + layer.children.iter().map(layer_count).sum::<usize>()
}

fn give_ids(layer: &mut Layer, take: &mut dyn FnMut() -> u64) {
    layer.id = take();
    for child in &mut layer.children {
        give_ids(child, take);
    }
}

/// Add a vector shape as a layer, on top and selected, with a history step.
fn new_shape(session: &mut Session, shape: Shape) -> Result<Value, Fail> {
    let doc = session.active_mut().ok_or("Open a picture first.")?;
    doc.push_history("Shape");
    let id = doc.doc.take_id();
    let name = match shape.kind {
        ShapeKind::Rectangle => "Rectangle",
        ShapeKind::Ellipse => "Ellipse",
        ShapeKind::Path => "Path",
    };
    doc.doc.layers_mut().push(Layer {
        id,
        name: name.to_string(),
        kind: Kind::Shape,
        visible: true,
        locked: false,
        opacity: 1.0,
        blend: Blend::Normal,
        clipped: false,
        children: Vec::new(),
        content: Content::Shape(shape),
        style: LayerStyle::default(),
        offset: (0.0, 0.0),
    });
    doc.active_layer = Some(id);
    doc.touch();
    Ok(json!({ "layer": id }))
}

fn curves_from(params: &Value) -> Result<Curves, Fail> {
    let read = |key: &str| -> Result<Vec<(u8, u8)>, Fail> {
        let Some(arr) = params.get(key).and_then(Value::as_array) else { return Ok(Vec::new()) };
        let mut out = Vec::with_capacity(arr.len());
        for pair in arr {
            let Some(p) = pair.as_array() else { return Err(format!("`{key}` wants [in, out] pairs.")) };
            if p.len() != 2 {
                return Err(format!("`{key}` wants [in, out] pairs."));
            }
            let x = p[0].as_f64().ok_or("A curve point is not a number.")?;
            let y = p[1].as_f64().ok_or("A curve point is not a number.")?;
            out.push((x.round().clamp(0.0, 255.0) as u8, y.round().clamp(0.0, 255.0) as u8));
        }
        out.sort_by_key(|p| p.0);
        out.dedup_by_key(|p| p.0);
        Ok(out)
    };
    Ok(Curves { rgb: read("points")?, red: read("red")?, green: read("green")?, blue: read("blue")? })
}

/// Give a duplicated subtree fresh ids and note the copy in each name.
fn rename_descendants(layer: &mut Layer, doc: &mut Document) {
    for child in &mut layer.children {
        child.id = doc.take_id();
        child.name = format!("{} copy", child.name);
        rename_descendants(child, doc);
    }
}

fn number(params: &Value, key: &str) -> Result<f64, Fail> {
    params
        .get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| format!("`{key}` is required and must be a number."))
}

fn number_or(params: &Value, key: &str, default: f64) -> f64 {
    params.get(key).and_then(Value::as_f64).unwrap_or(default)
}

/// Decode a picture the shell hands over, to straight RGBA at its own size.
///
/// The shell owns the file system and passes bytes; the engine owns decoding, because the
/// document needs pixels, not a file. Opens: PNG, JPEG, WebP, GIF, TIFF, BMP, ICO, TGA, QOI, DDS, EXR, HDR, JPEG XL,
/// SVG, DPX and camera raw.
///
/// The decoders are the app's own, from `filmcraft-media` — the same ones the video editor imports
/// stills with, which is why the photo editor now opens at least as many formats as the drawing
/// canvas does rather than fewer. Orientation is applied from the file's EXIF, so a phone photo
/// lands upright.
pub fn decode_picture(name: &str, bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), Fail> {
    if bytes.is_empty() {
        return Err(format!("`{name}` is empty."));
    }
    // The engine's own budget, not the media crate's: a picture that is too big for the canvas
    // should say so in the editor's terms.
    let limit = (crate::photocraft::model::MAX_PIXELS as usize).min(usize::MAX / 8);
    let raster = filmcraft_media::still::decode_raster_with_limit(name, bytes, limit)
        .map_err(|e| explain_picture_error(name, &e))?;
    Ok((raster.width, raster.height, raster.rgba))
}

/// The message a picture that will not open gets.
///
/// The decoder's own text is kept — it names the format and the reason — but an unsupported file
/// says what is supported, because "unsupported format" on its own leaves the user guessing.
fn explain_picture_error(name: &str, e: &filmcraft_media::MediaError) -> Fail {
    // A file the decoders do not recognise is a different problem from a corrupt one, and the
    // answer differs: the first needs the list of what is read. The variant says which, so this
    // does not have to guess from the wording.
    // A decoder reaching a file it does not recognise wraps the underlying reader's message, so
    // "could not be determined" is what an unknown format looks like here; anything else is a
    // file that looks like a picture and is not one, and that deserves the decoder's own reason.
    let text = format!("{e}");
    let unknown = matches!(
        e,
        filmcraft_media::MediaError::Unsupported(_) | filmcraft_media::MediaError::NoStream(_)
    ) || text.contains("could not be determined")
        || text.contains("unsupported");
    if unknown {
        format!(
            "`{name}` is not a format this editor opens. It reads PNG, JPEG, WebP, GIF, TIFF, \
             BMP, ICO, TGA, QOI, DDS, EXR, HDR, JPEG XL, SVG, DPX and camera raw."
        )
    } else {
        format!("`{name}` will not open: {text}")
    }
}

fn to_rgba(buf: &[u8], channels: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(buf.len() / channels * 4);
    for px in buf.chunks_exact(channels) {
        match channels {
            3 => out.extend_from_slice(&[px[0], px[1], px[2], 255]),
            2 => out.extend_from_slice(&[px[0], px[0], px[0], px[1]]),
            1 => out.extend_from_slice(&[px[0], px[0], px[0], 255]),
            _ => {}
        }
    }
    out
}

/// Encode the active document as a picture, for the shell to write.
pub fn encode_picture(doc: &Document, format: &str, quality: u8) -> Result<Vec<u8>, Fail> {
    let composite = compose::render(doc);
    let (w, h) = (doc.width(), doc.height());
    // The app's own still encoder, so a picture saved from here and one exported from the video
    // editor are written by the same code with the same colour handling.
    let f = match format.to_ascii_lowercase().as_str() {
        "png" => filmcraft_export::Format::PngSequence,
        "jpg" | "jpeg" => filmcraft_export::Format::JpegSequence,
        "tif" | "tiff" => filmcraft_export::Format::TiffSequence,
        "bmp" => filmcraft_export::Format::BmpSequence,
        "tga" => filmcraft_export::Format::TgaSequence,
        "exr" => filmcraft_export::Format::ExrSequence,
        "dpx" => filmcraft_export::Format::DpxSequence,
        other => {
            return Err(format!(
                "`{other}` is not a format this editor writes. It saves PNG, JPEG, TIFF, BMP, TGA, \
                 OpenEXR and DPX."
            ));
        }
    };
    // Alpha is kept where the format has it; a JPEG has none, and the encoder drops it.
    let alpha = !matches!(f, filmcraft_export::Format::JpegSequence | filmcraft_export::Format::DpxSequence);
    let q = if quality == 0 { 92 } else { quality.clamp(1, 100) };
    filmcraft_export::encode_still_with_quality(f, composite, w, h, alpha, q)
        .map_err(|e| format!("The picture cannot be written: {e}"))
}

/// The composite of the active document at a size, for [`Session::render`].
pub fn render_fitted(doc: &Document, max_width: u32, max_height: u32) -> (u32, u32, Vec<u8>) {
    let (w, h) = fit(doc.width(), doc.height(), max_width, max_height);
    let full = compose::render(doc);
    if (w, h) == (doc.width(), doc.height()) {
        return (w, h, full);
    }
    (w, h, reduce(&full, doc.width(), doc.height(), w, h))
}

/// The largest size with the document's aspect that fits in `max_w` × `max_h`, never enlarging.
pub fn fit(w: u32, h: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    let (w, h, max_w, max_h) = (w.max(1) as f64, h.max(1) as f64, max_w.max(1) as f64, max_h.max(1) as f64);
    let scale = (max_w / w).min(max_h / h).min(1.0);
    (((w * scale).round()).max(1.0) as u32, ((h * scale).round()).max(1.0) as u32)
}

/// Area-averaged reduction: a still line of a preview must not alias into moiré.
pub fn reduce(src: &[u8], sw: u32, sh: u32, dw: u32, dh: u32) -> Vec<u8> {
    let mut out = vec![0u8; (dw as usize) * (dh as usize) * 4];
    let x_ratio = sw as f64 / dw as f64;
    let y_ratio = sh as f64 / dh as f64;
    for y in 0..dh as usize {
        let y0 = (y as f64 * y_ratio) as usize;
        let y1 = (((y + 1) as f64 * y_ratio).ceil() as usize).min(sh as usize).max(y0 + 1);
        for x in 0..dw as usize {
            let x0 = (x as f64 * x_ratio) as usize;
            let x1 = (((x + 1) as f64 * x_ratio).ceil() as usize).min(sw as usize).max(x0 + 1);
            let mut acc = [0u32; 4];
            let mut n = 0u32;
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let i = (sy * sw as usize + sx) * 4;
                    for c in 0..4 {
                        acc[c] += u32::from(src[i + c]);
                    }
                    n += 1;
                }
            }
            let o = (y * dw as usize + x) * 4;
            for c in 0..4 {
                out[o + c] = if n == 0 { 0 } else { (acc[c] / n) as u8 };
            }
        }
    }
    out
}
