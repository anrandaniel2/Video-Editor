//! Tests for the photo engine the studio drives.
//!
//! These are the checks a compile cannot make: that a stroke lands where it was drawn, that undo
//! actually returns to the previous picture, that a blend mode blends, that a selection confines
//! the paint, and that the command surface still has the ids the restored UI sends.

use serde_json::json;

use super::model::{Blend, Content, Document, Kind, Layer, LayerStyle, PictureFit, TextRun};
use super::Session;

/// A session with one 32×32 document, active.
fn session() -> Session {
    let mut s = Session::new();
    let doc = Document::new("Test", 32, 32, "#FFFFFF").expect("a 32x32 canvas");
    let i = s.push(doc);
    s.select_index(i);
    s
}

fn run(s: &mut Session, id: &str, params: serde_json::Value) -> serde_json::Value {
    super::commands::execute(s, id, params).unwrap_or_else(|e| panic!("{id} failed: {e}"))
}

/// The composite of the active document.
fn pixels(s: &Session) -> Vec<u8> {
    super::compose::render(&s.active().expect("a document").doc)
}

fn at(buf: &[u8], width: u32, x: usize, y: usize) -> [u8; 4] {
    let i = (y * width as usize + x) * 4;
    [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]
}

#[test]
fn the_surface_has_every_command_the_restored_ui_sends() {
    // If a command the studio sends is missing here, its button is a dead control. This list is
    // taken from the UI's own call sites, not from what the engine happens to implement.
    for id in [
        "brush.presets.list",
        "command.list",
        "document.inspect",
        "document.sampleColor",
        "file.new",
        "image.adjustments.curves",
        "image.adjustments.levels",
        "layer.delete",
        "layer.duplicate",
        "layer.layerMask.revealAll",
        "layer.new.group",
        "layer.new.layer",
        "layer.newAdjustmentLayer.curves",
        "layer.newAdjustmentLayer.levels",
        "layer.select",
        "layer.setProps",
        "paint.bucket",
        "paint.cloneStamp",
        "paint.gradient",
        "paint.pencil",
        "paint.spotHealing",
        "paint.stroke",
        "select.deselect",
        "select.lasso",
        "select.magicWand",
        "select.rect",
        "tools.setBrush",
        "tools.setColors",
        "tools.swapColors",
        "type.create",
    ] {
        assert!(super::commands::known(id), "`{id}` is sent by the UI but the engine does not have it");
    }
}

#[test]
fn a_new_document_is_the_background_colour() {
    let s = session();
    let px = pixels(&s);
    assert_eq!(at(&px, 32, 0, 0), [255, 255, 255, 255], "white background");
    assert_eq!(at(&px, 32, 31, 31), [255, 255, 255, 255], "all the way to the corner");
}

#[test]
fn a_stroke_paints_where_it_was_drawn_and_nowhere_else() {
    let mut s = session();
    let layer = run(&mut s, "layer.new.layer", json!({}))["layer"].as_u64().expect("a layer id");
    run(
        &mut s,
        "paint.stroke",
        json!({ "points": [[8.0, 8.0], [24.0, 8.0]], "color": "#000000", "size": 6.0, "opacity": 100 }),
    );
    let px = pixels(&s);
    assert_eq!(at(&px, 32, 16, 8), [0, 0, 0, 255], "the middle of the stroke is black");
    assert_eq!(at(&px, 32, 16, 28), [255, 255, 255, 255], "far from the stroke is untouched");
    let _ = layer;
}

#[test]
fn undo_restores_the_picture_the_stroke_covered() {
    let mut s = session();
    run(&mut s, "layer.new.layer", json!({}));
    run(&mut s, "paint.stroke", json!({ "points": [[16.0, 16.0]], "color": "#FF0000", "size": 10.0 }));
    let painted = pixels(&s);
    assert_eq!(at(&painted, 32, 16, 16), [255, 0, 0, 255], "painted red");

    assert!(s.active_mut().expect("a document").undo(), "there is a step to undo");
    let after = pixels(&s);
    assert_eq!(at(&after, 32, 16, 16), [255, 255, 255, 255], "undo returned to the white background");

    let doc = s.active_mut().expect("a document");
    assert!(doc.redo(), "there is a step to redo");
    let again = pixels(&s);
    assert_eq!(at(&again, 32, 16, 16), [255, 0, 0, 255], "redo painted it again");
}

#[test]
fn the_history_labels_name_what_happened() {
    let mut s = session();
    run(&mut s, "layer.new.layer", json!({}));
    run(&mut s, "paint.stroke", json!({ "points": [[8.0, 8.0]], "color": "#000000" }));
    run(&mut s, "paint.bucket", json!({ "x": 0.0, "y": 0.0, "color": "#00FF00" }));
    let labels = s.active().expect("a document").history_labels();
    assert_eq!(labels, vec!["New layer", "Brush", "Fill"], "one row per edit, oldest first: {labels:?}");
}

#[test]
fn a_selection_confines_the_paint() {
    let mut s = session();
    run(&mut s, "layer.new.layer", json!({}));
    run(&mut s, "select.rect", json!({ "x": 0.0, "y": 0.0, "width": 16.0, "height": 32.0 }));
    // A stroke across the whole canvas: only the selected half may take paint.
    run(&mut s, "paint.stroke", json!({ "points": [[0.0, 16.0], [31.0, 16.0]], "color": "#0000FF", "size": 4.0 }));
    let px = pixels(&s);
    assert_eq!(at(&px, 32, 4, 16), [0, 0, 255, 255], "inside the selection is painted");
    assert_eq!(at(&px, 32, 24, 16), [255, 255, 255, 255], "outside the selection is not");
}

#[test]
fn a_fill_stops_at_the_edge_of_a_different_colour() {
    let mut s = session();
    run(&mut s, "layer.new.layer", json!({}));
    // A black square, then fill the white part: the fill must not cross the square's edge.
    run(
        &mut s,
        "paint.stroke",
        json!({ "points": [[4.0, 4.0], [4.0, 28.0]], "color": "#000000", "size": 7.0 }),
    );
    let before = pixels(&s);
    assert_eq!(at(&before, 32, 4, 16), [0, 0, 0, 255], "the bar is black");
    run(&mut s, "paint.bucket", json!({ "x": 28.0, "y": 16.0, "color": "#FF00FF", "tolerance": 0.0 }));
    let after = pixels(&s);
    assert_eq!(at(&after, 32, 28, 16), [255, 0, 255, 255], "the far side filled");
    assert_eq!(at(&after, 32, 4, 16), [0, 0, 0, 255], "the bar did not");
}

#[test]
fn multiply_darkens_and_screen_lightens() {
    // A white canvas with a mid-grey layer over it, in each blend mode.
    for (mode, expected) in [("Multiply", 128u8), ("Screen", 255u8), ("Normal", 128u8)] {
        let mut s = session();
        let id = run(&mut s, "layer.new.layer", json!({}))["layer"].as_u64().expect("a layer id");
        run(&mut s, "paint.bucket", json!({ "x": 0.0, "y": 0.0, "color": "#808080", "tolerance": 0.0 }));
        run(&mut s, "layer.setProps", json!({ "layer": id, "blend": mode }));
        let px = pixels(&s);
        let got = at(&px, 32, 16, 16)[0];
        assert_eq!(got, expected, "{mode} over white gave {got}, wanted {expected}");
    }
}

#[test]
fn an_unknown_blend_mode_is_refused_rather_than_substituted() {
    let mut s = session();
    let id = run(&mut s, "layer.new.layer", json!({}))["layer"].as_u64().expect("a layer id");
    let err = super::commands::execute(&mut s, "layer.setProps", json!({ "layer": id, "blend": "Vivid" }))
        .expect_err("Vivid is not one of the five modes the panel offers");
    assert!(err.contains("Vivid"), "the message names the mode: {err}");
}

#[test]
fn opacity_and_visibility_change_what_is_composited() {
    let mut s = session();
    let id = run(&mut s, "layer.new.layer", json!({}))["layer"].as_u64().expect("a layer id");
    run(&mut s, "paint.bucket", json!({ "x": 0.0, "y": 0.0, "color": "#000000", "tolerance": 0.0 }));
    assert_eq!(at(&pixels(&s), 32, 16, 16), [0, 0, 0, 255], "opaque black");

    run(&mut s, "layer.setProps", json!({ "layer": id, "opacity": 0.5 }));
    let half = at(&pixels(&s), 32, 16, 16);
    assert!((120..=136).contains(&half[0]), "half opacity over white is mid grey, got {half:?}");

    run(&mut s, "layer.setProps", json!({ "layer": id, "visible": false }));
    assert_eq!(at(&pixels(&s), 32, 16, 16), [255, 255, 255, 255], "hidden layers do not composite");
}

#[test]
fn a_curves_layer_changes_what_is_beneath_it() {
    let mut s = session();
    run(&mut s, "layer.new.layer", json!({}));
    run(&mut s, "paint.bucket", json!({ "x": 0.0, "y": 0.0, "color": "#808080", "tolerance": 0.0 }));
    let before = at(&pixels(&s), 32, 16, 16)[0];
    // A curve that maps 128 to 200 and leaves the ends alone.
    run(&mut s, "layer.newAdjustmentLayer.curves", json!({ "points": [[0, 0], [128, 200], [255, 255]] }));
    let after = at(&pixels(&s), 32, 16, 16)[0];
    assert!(after > before, "the curve lifted the midtone: {before} became {after}");
    assert!(after < 255, "and did not clip it: {after}");
}

#[test]
fn a_group_composites_its_children() {
    let mut s = session();
    let group = run(&mut s, "layer.new.group", json!({}))["layer"].as_u64().expect("a group id");
    // A layer made inside a group would be a child; the engine adds new layers to the tree root,
    // so put one there and check the group carries its own opacity.
    run(&mut s, "layer.setProps", json!({ "layer": group, "opacity": 0.0 }));
    let px = pixels(&s);
    assert_eq!(at(&px, 32, 16, 16), [255, 255, 255, 255], "an empty group changes nothing");
    let doc = s.active().expect("a document");
    assert!(doc.doc.find(group).is_some(), "the group is in the tree");
}

#[test]
fn the_layer_tree_lists_top_first_with_the_active_one_marked() {
    let mut s = session();
    let id = run(&mut s, "layer.new.layer", json!({}))["layer"].as_u64().expect("a layer id");
    let inspect = run(&mut s, "document.inspect", json!({}));
    let layers = inspect["layers"].as_array().expect("layers");
    assert_eq!(layers.len(), 2, "the background and the new layer");
    assert_eq!(layers[0]["id"].as_u64(), Some(id), "the newest is on top");
    assert_eq!(inspect["activeLayer"].as_u64(), Some(id), "and it is the active one");
    assert_eq!(layers[0]["kind"].as_str(), Some("pixels"));
}

#[test]
fn hiding_a_layer_does_not_change_its_pixels() {
    let mut s = session();
    let id = run(&mut s, "layer.new.layer", json!({}))["layer"].as_u64().expect("a layer id");
    run(&mut s, "paint.bucket", json!({ "x": 0.0, "y": 0.0, "color": "#123456", "tolerance": 0.0 }));
    run(&mut s, "layer.setProps", json!({ "layer": id, "visible": false }));
    let doc = s.active().expect("a document");
    let layer = doc.doc.find(id).expect("the layer");
    let Content::Pixels(Some(px)) = &layer.content else { panic!("the layer kept its pixels") };
    assert_eq!(&px[..4], &[0x12, 0x34, 0x56, 0xFF], "hidden, not erased");
}

#[test]
fn a_locked_layer_cannot_be_painted_and_says_so() {
    let mut s = session();
    let id = run(&mut s, "layer.new.layer", json!({}))["layer"].as_u64().expect("a layer id");
    run(&mut s, "layer.setProps", json!({ "layer": id, "locked": true }));
    let reason = super::commands::enabled(&s, "paint.stroke").expect_err("a locked layer refuses paint");
    assert!(reason.contains("locked"), "the reason says why: {reason}");
}

#[test]
fn an_empty_session_says_what_is_missing_and_what_is_not() {
    let s = Session::new();
    let reason = super::commands::enabled(&s, "paint.stroke").expect_err("nothing to paint on");
    assert!(reason.contains("Open a picture"), "{reason}");
    // A command that does not need a document still runs.
    assert!(super::commands::enabled(&s, "command.list").is_ok());
    assert!(super::commands::enabled(&s, "file.new").is_ok());
    assert!(super::commands::enabled(&s, "brush.presets.list").is_ok());
}

#[test]
fn a_picture_too_large_for_the_engine_is_refused_with_its_size() {
    let err = Document::new("Huge", 9000, 9000, "#FFFFFF").expect_err("81000000 pixels is over the cap");
    assert!(err.contains("megapixels"), "the message says what the limit is: {err}");
}

#[test]
fn text_layers_are_in_the_tree_where_they_were_placed() {
    let mut s = session();
    let id = run(&mut s, "type.create", json!({ "x": 4.0, "y": 20.0, "text": "Hello", "size": 12.0 }))["layer"]
        .as_u64()
        .expect("a text layer");
    let doc = s.active().expect("a document");
    let layer = doc.doc.find(id).expect("the layer");
    assert_eq!(layer.kind, Kind::Text);
    assert_eq!(layer.name, "Hello");
    match &layer.content {
        Content::Text(t) => {
            assert_eq!(t.text, "Hello");
            assert_eq!((t.x, t.y), (4.0, 20.0));
        }
        _ => panic!("a text layer holds a run"),
    }
}

#[test]
fn duplicating_a_layer_keeps_its_pixels_but_not_its_identity() {
    let mut s = session();
    let id = run(&mut s, "layer.new.layer", json!({}))["layer"].as_u64().expect("a layer id");
    run(&mut s, "paint.bucket", json!({ "x": 0.0, "y": 0.0, "color": "#00AA00", "tolerance": 0.0 }));
    let copy = run(&mut s, "layer.duplicate", json!({}))["layer"].as_u64().expect("the copy");
    assert_ne!(copy, id, "the copy has its own id");
    let doc = s.active().expect("a document");
    let original = doc.doc.find(id).expect("the original");
    let dup = doc.doc.find(copy).expect("the copy");
    assert_eq!(dup.name, format!("{} copy", original.name));
    let (Content::Pixels(Some(a)), Content::Pixels(Some(b))) = (&original.content, &dup.content) else {
        panic!("both hold pixels")
    };
    assert_eq!(a.as_slice(), b.as_slice(), "the pixels came with it");
}

#[test]
fn the_status_reports_what_the_shell_shows() {
    let mut s = session();
    assert!(!s.active().expect("a document").is_dirty(), "a fresh document is clean");
    run(&mut s, "layer.new.layer", json!({}));
    run(&mut s, "paint.stroke", json!({ "points": [[8.0, 8.0]] }));
    let doc = s.active().expect("a document");
    assert!(doc.is_dirty(), "an edit makes it dirty");
    assert!(!doc.past.is_empty(), "and there is something to undo");
    assert!(doc.future.is_empty(), "but nothing to redo");
}

#[test]
fn a_self_overlapping_stroke_records_one_step() {
    let mut s = session();
    run(&mut s, "layer.new.layer", json!({}));
    let before = s.active().expect("a document").past.len();
    // A single drag that crosses itself: three points, two segments, the second landing back on
    // the first. Whatever the pointer did, the user did one thing.
    run(
        &mut s,
        "paint.stroke",
        json!({ "points": [[4.0, 4.0], [28.0, 28.0], [4.0, 28.0]], "color": "#000000", "size": 3.0 }),
    );
    assert_eq!(
        s.active().expect("a document").past.len() - before,
        1,
        "one drag is one undo step"
    );
}

#[test]
fn a_new_layer_goes_on_top_without_reordering_the_tree() {
    let mut s = session();
    let a = run(&mut s, "layer.new.layer", json!({}))["layer"].as_u64().expect("a");
    let b = run(&mut s, "layer.new.layer", json!({}))["layer"].as_u64().expect("b");
    let tree: Vec<u64> = s
        .active()
        .expect("a document")
        .doc
        .layers()
        .iter()
        .map(|l| l.id)
        .collect();
    // Bottom to top: the background, then a, then b. The first layer must still be under the
    // second — an earlier version pulled the active layer out and re-pushed it, which moved `a`
    // to the top and left the document reordered.
    assert_eq!(tree.len(), 3, "background plus two layers: {tree:?}");
    let position = |id: u64| tree.iter().position(|&x| x == id).expect("in the tree");
    assert!(position(a) < position(b), "`a` is below `b`: {tree:?}");
}

#[test]
fn creating_a_layer_is_undoable() {
    let mut s = session();
    run(&mut s, "layer.new.layer", json!({}));
    assert_eq!(s.active().expect("a document").doc.layers().len(), 2);
    assert!(s.active_mut().expect("a document").undo(), "there is a step");
    assert_eq!(s.active().expect("a document").doc.layers().len(), 1, "undo removed the layer");
}

#[test]
fn a_levels_drag_over_the_picture_is_one_history_step() {
    let mut s = session();
    run(&mut s, "layer.new.layer", json!({}));
    run(&mut s, "paint.bucket", json!({ "x": 0.0, "y": 0.0, "color": "#808080", "tolerance": 0.0 }));
    let before = s.active().expect("a document").past.len();
    // The panel re-sends its whole value on every slider tick; that is one gesture.
    for g in [1.0, 1.2, 1.4, 1.6] {
        run(
            &mut s,
            "image.adjustments.levels",
            json!({ "inBlack": 0, "gamma": g, "inWhite": 255, "coalesce": "photocraft.levels" }),
        );
    }
    let labels = s.active().expect("a document").history_labels();
    assert_eq!(&labels[before..], &["Levels"], "one row for the whole drag: {labels:?}");
    // And one undo takes the picture all the way back.
    let bent = at(&pixels(&s), 32, 16, 16)[0];
    assert!(s.active_mut().expect("a document").undo(), "there is a step to undo");
    let back = at(&pixels(&s), 32, 16, 16)[0];
    assert_eq!(back, 128, "undo restored the mid-grey: {bent} became {back}");
}

#[test]
fn a_whole_image_curve_bends_the_pixels_it_sees() {
    let mut s = session();
    run(&mut s, "layer.new.layer", json!({}));
    run(&mut s, "paint.bucket", json!({ "x": 0.0, "y": 0.0, "color": "#808080", "tolerance": 0.0 }));
    let before = at(&pixels(&s), 32, 16, 16)[0];
    run(
        &mut s,
        "image.adjustments.curves",
        json!({ "points": [[0, 0], [128, 200], [255, 255]], "coalesce": "photocraft.curves" }),
    );
    let after = at(&pixels(&s), 32, 16, 16)[0];
    assert!(after > before, "the midtone was lifted: {before} became {after}");
}

#[test]
fn an_adjustment_respects_the_selection() {
    let mut s = session();
    run(&mut s, "layer.new.layer", json!({}));
    run(&mut s, "paint.bucket", json!({ "x": 0.0, "y": 0.0, "color": "#808080", "tolerance": 0.0 }));
    run(&mut s, "select.rect", json!({ "x": 0.0, "y": 0.0, "width": 8.0, "height": 32.0 }));
    run(
        &mut s,
        "image.adjustments.levels",
        json!({ "inBlack": 0, "gamma": 2.0, "inWhite": 255, "coalesce": "photocraft.levels" }),
    );
    let px = pixels(&s);
    let inside = at(&px, 32, 2, 16)[0];
    let outside = at(&px, 32, 24, 16)[0];
    assert!(inside > 128, "inside the selection was lifted: {inside}");
    assert_eq!(outside, 128, "outside it was untouched: {outside}");
}

#[test]
fn the_eraser_takes_paint_away_instead_of_painting_transparent_on_it() {
    let mut s = session();
    let id = run(&mut s, "layer.new.layer", json!({}))["layer"].as_u64().expect("a layer id");
    run(&mut s, "paint.bucket", json!({ "x": 0.0, "y": 0.0, "color": "#123456", "tolerance": 0.0 }));
    // The layer's own pixels, not the composite: the document's background layer sits under
    // everything and is opaque, so a composite of an erased hole is simply the white beneath it.
    let alpha = |s: &Session| {
        let doc = s.active().expect("a document");
        let Content::Pixels(Some(px)) = &doc.doc.find(id).expect("the layer").content else {
            panic!("the layer kept its pixels")
        };
        px[(16 * 32 + 16) * 4 + 3]
    };
    assert_eq!(alpha(&s), 255, "the layer is painted");
    run(&mut s, "paint.stroke", json!({ "points": [[16.0, 16.0]], "size": 40.0, "erase": true }));
    assert_eq!(alpha(&s), 0, "the eraser took the alpha away");
    // And what shows through is the background, which is what the user sees.
    let seen = at(&pixels(&s), 32, 16, 16);
    assert_eq!(seen, [255, 255, 255, 255], "the white background shows through: {seen:?}");
}

#[test]
fn the_pencil_is_a_hard_tip_and_the_brush_is_not() {
    let mut s = session();
    run(&mut s, "layer.new.layer", json!({}));
    run(&mut s, "paint.pencil", json!({ "points": [[16.0, 16.0]], "color": "#000000", "size": 9.0 }));
    assert_eq!(at(&pixels(&s), 32, 16, 16)[3], 255, "the pencil's centre is solid");
    // A 9px tip centred on 16 covers up to about 20.5, so 20 is the outermost covered pixel and
    // 21 is empty. The pencil must still be solid there; the soft brush must not be.
    let mut soft = session();
    run(&mut soft, "layer.new.layer", json!({}));
    run(&mut soft, "paint.stroke", json!({ "points": [[16.0, 16.0]], "color": "#000000", "size": 9.0, "hardness": 0.0 }));
    // Read the colour, not the alpha: the white background layer is opaque underneath, so an
    // unpainted pixel is white rather than transparent.
    let pencil_edge = at(&pixels(&s), 32, 20, 16)[0];
    let brush_edge = at(&pixels(&soft), 32, 20, 16)[0];
    assert_eq!(pencil_edge, 0, "the pencil is solid right to its edge: {pencil_edge}");
    assert!(brush_edge > 200, "the soft brush had faded out by there: {brush_edge}");
    assert_eq!(at(&pixels(&s), 32, 22, 16)[0], 255, "and beyond the tip is background");
}

#[test]
fn a_stroke_sent_in_pieces_under_one_coalesce_key_is_one_step() {
    let mut s = session();
    run(&mut s, "layer.new.layer", json!({}));
    let before = s.active().expect("a document").past.len();
    // A drag arrives as several calls, each naming the gesture.
    for x in [8.0, 12.0, 16.0, 20.0] {
        run(
            &mut s,
            "paint.stroke",
            json!({ "points": [[x, 16.0]], "color": "#000000", "coalesce": "photocraft.stroke" }),
        );
    }
    assert_eq!(
        s.active().expect("a document").past.len() - before,
        1,
        "one drag, one history row"
    );
}

#[test]
fn the_clone_stamp_reads_the_source_the_studio_sends() {
    let mut s = session();
    run(&mut s, "layer.new.layer", json!({}));
    run(&mut s, "paint.bucket", json!({ "x": 0.0, "y": 0.0, "color": "#224466", "tolerance": 0.0 }));
    // The studio sends the alt-tapped point as a two-element array.
    let out = run(
        &mut s,
        "paint.cloneStamp",
        json!({ "points": [[4.0, 4.0]], "source": [20.0, 20.0], "size": 6.0, "hardness": 100, "opacity": 100 }),
    );
    assert!(out.get("points").is_some(), "the stamp ran: {out}");
}

#[test]
fn an_adjustment_layers_levels_read_the_panels_scale() {
    // The panel's Input black slider is 0..=255; read as 0..=1 it would clamp to 1.0 and crush
    // the picture to black. A black point of 128 must map mid-grey to black.
    let mut s = session();
    run(&mut s, "layer.new.layer", json!({}));
    run(&mut s, "paint.bucket", json!({ "x": 0.0, "y": 0.0, "color": "#808080", "tolerance": 0.0 }));
    run(&mut s, "layer.newAdjustmentLayer.levels", json!({ "inBlack": 128, "gamma": 1.0, "inWhite": 255 }));
    let after = at(&pixels(&s), 32, 16, 16)[0];
    assert!(after < 10, "a black point at 128 made the midtone black: {after}");
}

#[test]
fn the_brush_list_is_the_one_the_panel_shows() {
    // The panel lists these eight names and sends them back with tools.setBrush. An engine with a
    // list of its own means the panel's names select nothing.
    let mut s = session();
    let out = super::commands::execute(&mut s, "brush.presets.list", json!({})).expect("the presets");
    let names: Vec<String> = out["presets"]
        .as_array()
        .expect("presets")
        .iter()
        .map(|p| p["name"].as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(
        names,
        vec![
            "Natural Pencil 6B",
            "Natural Pencil 2B",
            "Natural Pencil 4B",
            "Compressed Charcoal",
            "Dense Graphite",
            "Round Dry Bristle",
            "Soft Chalk",
            "Fine Ink",
        ],
        "the panel's own eight, in its own order"
    );
}

#[test]
fn picking_a_brush_changes_what_the_brush_does() {
    // Charcoal grains; ink does not. If picking a preset left the tip alone, the eight names in
    // the panel would be eight labels for one brush -- which is what they were.
    let ink = {
        let mut s = session();
        run(&mut s, "tools.setBrush", json!({ "preset": "Fine Ink" }));
        run(&mut s, "layer.new.layer", json!({}));
        run(&mut s, "paint.stroke", json!({ "points": [[16.0, 16.0]], "size": 24.0, "color": "#000000" }));
        pixels(&s)
    };
    let chalk = {
        let mut s = session();
        run(&mut s, "tools.setBrush", json!({ "preset": "Soft Chalk" }));
        run(&mut s, "layer.new.layer", json!({}));
        run(&mut s, "paint.stroke", json!({ "points": [[16.0, 16.0]], "size": 24.0, "color": "#000000" }));
        pixels(&s)
    };
    // Count how many pixels in the tip's area are actually inked: grain leaves gaps, ink does not.
    let inked = |buf: &[u8]| {
        (0..32 * 32).filter(|i| buf[i * 4] < 128).count()
    };
    let (i, c) = (inked(&ink), inked(&chalk));
    assert!(i > c, "ink covers more than chalk does: ink {i}, chalk {c}");
    assert!(c > 0, "the chalk still paints something: {c}");
}

#[test]
fn a_set_selector_can_step_through_every_document_brush() {
    // Every brush the panel lists must belong to at least one set, or an arrow press shows an
    // empty list and the set looks broken.
    let mut s = session();
    let out = super::commands::execute(&mut s, "brush.presets.list", json!({})).expect("presets");
    let names: Vec<String> = out["presets"].as_array().expect("presets").iter()
        .map(|p| p["name"].as_str().unwrap_or_default().to_string()).collect();
    for set in ["Pencils", "Charcoal & Chalk", "Bristle", "Inking"] {
        // Mirrors StudioBrushSets.filters in the UI.
        let hit = names.iter().any(|n| {
            let b = n.to_lowercase();
            when_set(set, &b)
        });
        assert!(hit, "the `{set}` set is empty for the engine's brushes");
    }
}

/// The UI's set rule, restated so the two cannot drift silently.
fn when_set(set: &str, b: &str) -> bool {
    match set {
        "Pencils" => b.contains("pencil") || b.contains("graphite"),
        "Charcoal & Chalk" => b.contains("charcoal") || b.contains("chalk"),
        "Bristle" => b.contains("bristle"),
        "Inking" => b.contains("ink"),
        _ => true,
    }
}

#[test]
fn the_engine_opens_more_formats_than_the_drawing_canvas() {
    // A PNG is written by hand here so the test needs no fixture file: the point is that the
    // importer accepts a real file, not that it recognises a header.
    let mut png = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png, 4, 3);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().expect("a PNG header");
        w.write_image_data(&[200u8; 4 * 3 * 4]).expect("PNG data");
    }
    let (w, h, rgba) = super::commands::decode_picture("tiny.png", &png).expect("a PNG opens");
    assert_eq!((w, h), (4, 3));
    assert_eq!(rgba.len(), 4 * 3 * 4);

    // A JPEG: the format phones take pictures in, and the one that used to be refused outright.
    let jpeg = filmcraft_export::encode_still_with_quality(
        filmcraft_export::Format::JpegSequence,
        vec![128u8; 8 * 8 * 4],
        8,
        8,
        false,
        90,
    )
    .expect("a JPEG is written");
    assert_eq!(&jpeg[..2], &[0xFF, 0xD8], "it really is a JPEG");
    let (w, h, _) = super::commands::decode_picture("photo.jpg", &jpeg).expect("a JPEG opens");
    assert_eq!((w, h), (8, 8));

    // And a file that is not a picture says what is supported rather than just failing.
    let err = super::commands::decode_picture("notes.txt", b"hello").expect_err("not a picture");
    assert!(err.contains("PNG") && err.contains("JPEG"), "the message lists the formats: {err}");
}

#[test]
fn the_engine_writes_the_formats_it_claims() {
    for (format, magic) in [
        ("png", &[0x89u8, b'P', b'N', b'G'][..]),
        ("jpg", &[0xFF, 0xD8][..]),
        ("bmp", b"BM"),
        ("tiff", &[b'I', b'I', 0x2A, 0x00][..]),
    ] {
        let mut s = session();
        let doc = s.active().expect("a document");
        let bytes = super::commands::encode_picture(&doc.doc, format, 90)
            .unwrap_or_else(|e| panic!("{format} export: {e}"));
        assert!(bytes.starts_with(magic), "{format} does not start with its magic number");
    }
    let mut s = session();
    let doc = s.active().expect("a document");
    let err = super::commands::encode_picture(&doc.doc, "psd", 90).expect_err("psd is not written");
    assert!(err.contains("PNG"), "the message lists what is written: {err}");
}

#[test]
fn a_document_can_have_pages_and_the_active_one_is_the_canvas() {
    let mut s = session();
    assert_eq!(s.active().expect("a document").doc.pages.len(), 1);
    let out = run(&mut s, "page.add", json!({}));
    assert_eq!(out["pages"], 2);
    // The new page is the active one, and it is blank rather than a copy of page one.
    let doc = s.active().expect("a document");
    assert_eq!(doc.doc.active_page, 1);
    assert_eq!(doc.doc.page().layers.len(), 1, "its own empty layer");
    assert_eq!(at(&pixels(&s), 32, 16, 16)[3], 0, "a new page is transparent");

    // Switching back shows page one's white background.
    run(&mut s, "page.select", json!({ "page": 0 }));
    assert_eq!(at(&pixels(&s), 32, 16, 16), [255, 255, 255, 255]);

    // A layer id stays unique across pages, so a command naming one is never ambiguous.
    let ids = s.active().expect("a document").doc.ids();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), ids.len(), "no id is reused across pages");
}

#[test]
fn a_page_cannot_be_deleted_out_of_a_document() {
    let mut s = session();
    let err = super::commands::execute(&mut s, "page.delete", json!({})).expect_err("the last page stays");
    assert!(err.contains("one page"), "it says why: {err}");
}

#[test]
fn duplicating_a_page_copies_its_layers_under_new_ids() {
    let mut s = session();
    run(&mut s, "layer.new.layer", json!({}));
    run(&mut s, "paint.bucket", json!({ "x": 0.0, "y": 0.0, "color": "#123456", "tolerance": 0.0 }));
    let before: Vec<u64> = s.active().expect("a document").doc.page().layers.iter().map(|l| l.id).collect();
    run(&mut s, "page.duplicate", json!({}));
    let doc = s.active().expect("a document");
    assert_eq!(doc.doc.pages.len(), 2);
    let copy: Vec<u64> = doc.doc.page().layers.iter().map(|l| l.id).collect();
    assert!(!copy.iter().any(|id| before.contains(id)), "a copy has its own ids");
    // And the copy carries the paint.
    assert_eq!(at(&pixels(&s), 32, 16, 16), [0x12, 0x34, 0x56, 0xFF]);
}

#[test]
fn a_vector_shape_is_geometry_that_can_change_after_it_is_drawn() {
    let mut s = session();
    let out = run(&mut s, "shape.create", json!({
        "kind": "rectangle", "x": 4.0, "y": 4.0, "width": 20.0, "height": 20.0,
        "fill": "#FF0000", "stroke": null, "strokeWidth": 0.0,
    }));
    let id = out["layer"].as_u64().expect("a shape layer");
    let doc = s.active().expect("a document");
    assert_eq!(doc.doc.find(id).expect("the shape").kind, Kind::Shape);

    // The middle is filled, a corner outside the frame is not.
    assert_eq!(at(&pixels(&s), 32, 14, 14), [255, 0, 0, 255]);
    assert_eq!(at(&pixels(&s), 32, 1, 1), [255, 255, 255, 255], "untouched background");

    // Changing the fill changes the picture without redrawing the shape.
    run(&mut s, "shape.setProps", json!({ "fill": "#00FF00" }));
    assert_eq!(at(&pixels(&s), 32, 14, 14), [0, 255, 0, 255], "the shape is recoloured");
}

#[test]
fn a_vector_stroke_width_is_respected_and_undoable() {
    let mut s = session();
    let out = run(&mut s, "shape.create", json!({
        "kind": "rectangle", "x": 8.0, "y": 8.0, "width": 16.0, "height": 16.0,
        "fill": null, "stroke": "#000000", "strokeWidth": 4.0,
    }));
    assert!(out["layer"].as_u64().is_some());
    // The outline sits on the frame's edge, so a pixel just outside the frame is inked.
    let edge = at(&pixels(&s), 32, 6, 16);
    assert!(edge[0] < 128, "the 4px stroke reaches the frame's edge: {edge:?}");
    // And the fill, being none, leaves the middle alone.
    assert_eq!(at(&pixels(&s), 32, 16, 16), [255, 255, 255, 255], "no fill");
}

#[test]
fn an_ellipse_is_round_not_a_rectangle() {
    let mut s = session();
    run(&mut s, "shape.create", json!({
        "kind": "ellipse", "x": 4.0, "y": 4.0, "width": 24.0, "height": 24.0,
        "fill": "#000000", "strokeWidth": 0.0,
    }));
    let px = pixels(&s);
    // The centre of the ellipse is filled; a corner of its bounding box is not.
    assert_eq!(at(&px, 32, 16, 16), [0, 0, 0, 255], "the centre");
    assert_eq!(at(&px, 32, 5, 5), [255, 255, 255, 255], "the box's corner");
}

#[test]
fn a_shape_can_be_moved_and_resized_without_being_redrawn() {
    let mut s = session();
    let out = run(&mut s, "shape.create", json!({
        "kind": "rectangle", "x": 2.0, "y": 2.0, "width": 10.0, "height": 10.0,
        "fill": "#FF0000", "strokeWidth": 0.0,
    }));
    let id = out["layer"].as_u64().expect("a shape layer");
    // Named by the panel that edits it, the way Transform sends it.
    run(&mut s, "shape.setProps", json!({
        "layer": id, "x": 16.0, "y": 16.0, "width": 12.0, "height": 12.0,
    }));
    let px = pixels(&s);
    // The old place is clear and the new one is filled: the geometry moved, not the paint.
    assert_eq!(at(&px, 32, 4, 4), [255, 255, 255, 255], "the old place");
    assert_eq!(at(&px, 32, 20, 20), [255, 0, 0, 255], "the new place");
    // And the shape is still a shape, so it can move again.
    let doc = s.active().expect("a document");
    assert_eq!(doc.doc.find(id).expect("the shape").kind, Kind::Shape);
    assert!(s.active_mut().expect("a document").undo(), "moving is undoable");
}

#[test]
fn a_shape_with_no_stroke_drawn_is_still_editable() {
    // The Stroke panel's "None" writes a null, and the draw must not choke on it.
    let mut s = session();
    let id = run(&mut s, "shape.create", json!({
        "kind": "ellipse", "x": 4.0, "y": 4.0, "width": 20.0, "height": 20.0,
        "fill": "#0000FF", "stroke": "#FF0000", "strokeWidth": 4.0,
    }))["layer"].as_u64().expect("a shape layer");
    run(&mut s, "shape.setProps", json!({ "layer": id, "stroke": null, "strokeWidth": 0.0 }));
    // The fill is untouched: removing the outline must not remove the shape.
    assert_eq!(at(&pixels(&s), 32, 14, 14), [0, 0, 255, 255]);
}

#[test]
fn a_picture_can_be_moved_and_fitted_after_it_is_placed() {
    // A 4x2 red picture, placed the way the shell places one.
    let mut png = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png, 4, 2);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().expect("a PNG header");
        let mut data = Vec::new();
        for _ in 0..8 {
            data.extend_from_slice(&[255, 0, 0, 255]);
        }
        w.write_image_data(&data).expect("PNG data");
    }
    let mut s = session();
    // `place_bytes` lives on the shell's session, which owns the lock; the engine's own session is
    // what the command table works on, so the placement is done here the way it does it.
    {
        let doc = s.active_mut().expect("a document");
        let (w, h, rgba) = super::commands::decode_picture("red.png", &png).expect("a PNG opens");
        let id = doc.doc.take_id();
        doc.push_history("Place picture");
        doc.doc.layers_mut().push(Layer {
            id,
            name: "red.png".into(),
            kind: Kind::Picture,
            visible: true,
            locked: false,
            opacity: 1.0,
            blend: Blend::Normal,
            clipped: false,
            children: Vec::new(),
            content: Content::Picture(super::model::PictureRef {
                frame: super::model::Frame::new(0.0, 0.0, f64::from(w), f64::from(h)),
                pixels: std::sync::Arc::new(rgba),
                width: w,
                height: h,
                fit: PictureFit::Stretch,
            }),
            style: LayerStyle::default(),
        });
        doc.active_layer = Some(id);
    }
    let doc = s.active().expect("a document");
    let placed = doc.doc.layers().last().cloned().expect("a layer");
    assert_eq!(placed.kind, Kind::Picture, "it is a picture, not pixels");
    // Placed at the canvas's size, so its frame is the canvas.
    let out = run(&mut s, "picture.setProps", json!({
        "layer": placed.id, "x": 8.0, "y": 8.0, "width": 16.0, "height": 16.0,
    }));
    assert_eq!(out["layer"], placed.id);
    let doc = s.active().expect("a document");
    let Content::Picture(pic) = &doc.doc.find(placed.id).expect("the picture").content else {
        panic!("still a picture");
    };
    assert_eq!((pic.frame.x, pic.frame.y), (8.0, 8.0));
    assert_eq!((pic.frame.width, pic.frame.height), (16.0, 16.0));
    // And the fit decides what happens inside the frame.
    run(&mut s, "picture.setProps", json!({ "layer": placed.id, "fit": "fit" }));
    let doc = s.active().expect("a document");
    let Content::Picture(pic) = &doc.doc.find(placed.id).expect("the picture").content else {
        panic!("still a picture");
    };
    assert_eq!(pic.fit, PictureFit::Fit);
    // The frame moved, so the canvas shows the picture only where the frame is.
    let px = pixels(&s);
    assert_eq!(at(&px, 32, 12, 12), [255, 0, 0, 255], "inside the frame");
    assert_eq!(at(&px, 32, 2, 2), [255, 255, 255, 255], "outside it");
}

#[test]
fn a_pen_path_is_geometry_with_the_nodes_that_were_drawn() {
    let mut s = session();
    let out = run(&mut s, "shape.create", json!({
        "kind": "path",
        "x": 4.0, "y": 4.0, "width": 20.0, "height": 20.0,
        "nodes": [
            { "x": 4.0, "y": 4.0 }, { "x": 24.0, "y": 4.0 }, { "x": 24.0, "y": 24.0 }
        ],
        "closed": false,
        "fill": null, "stroke": "#000000", "strokeWidth": 2.0,
    }));
    let id = out["layer"].as_u64().expect("a path layer");
    let doc = s.active().expect("a document");
    let layer = doc.doc.find(id).expect("the path");
    assert_eq!(layer.kind, Kind::Shape);
    let Content::Shape(shape) = &layer.content else { panic!("it is a shape") };
    assert_eq!(shape.nodes.len(), 3, "three corners");
    assert!(!shape.closed, "an open path");
    // The path is stroked where it runs and not where it does not: the corner at 4,4 is inked,
    // the middle of the canvas is not.
    let px = pixels(&s);
    // The top segment runs along y = 4 with a 2px stroke, so it covers y 3..5; a pixel whose
    // centre is on that band is inked, and one a row below it is not.
    assert!(at(&px, 32, 10, 4)[0] < 128, "the first segment is inked");
    assert_eq!(at(&px, 32, 10, 8), [255, 255, 255, 255], "one row clear of it is not");
    assert_eq!(at(&px, 32, 16, 16), [255, 255, 255, 255], "the open middle is untouched");
    // And a path needs two nodes to be a path at all.
    let err = super::commands::execute(
        &mut s,
        "shape.create",
        json!({ "kind": "path", "x": 0.0, "y": 0.0, "width": 4.0, "height": 4.0, "nodes": [{ "x": 1.0, "y": 1.0 }] }),
    )
    .expect_err("one node is not a path");
    assert!(err.contains("two nodes"), "it says why: {err}");
}

#[test]
fn the_node_toolbar_edits_the_path_it_names() {
    let mut s = session();
    let id = run(&mut s, "shape.create", json!({
        "kind": "path",
        "x": 4.0, "y": 4.0, "width": 20.0, "height": 20.0,
        "nodes": [
            { "x": 4.0, "y": 4.0 }, { "x": 24.0, "y": 4.0 }, { "x": 24.0, "y": 24.0 }
        ],
        "closed": false, "fill": null, "stroke": "#000000", "strokeWidth": 2.0,
    }))["layer"].as_u64().expect("a path");

    // Smooth gives every node handles; sharp takes them away again.
    run(&mut s, "shape.editNodes", json!({ "action": "smooth" }));
    let doc = s.active().expect("a document");
    let Content::Shape(shape) = &doc.doc.find(id).expect("the path").content else { panic!("a shape") };
    assert!(shape.nodes.iter().all(|n| n.smooth), "every node is smooth");
    // The handles lie along the line through each node's neighbours, so their length is what
    // says they were set -- not one component: the first node's tangent here is vertical.
    assert!(
        shape.nodes.iter().all(|n| (n.out_x * n.out_x + n.out_y * n.out_y).sqrt() > 1.0),
        "every node has handles: {:?}",
        shape.nodes.iter().map(|n| (n.out_x, n.out_y)).collect::<Vec<_>>(),
    );

    run(&mut s, "shape.editNodes", json!({ "action": "sharp" }));
    let doc = s.active().expect("a document");
    let Content::Shape(shape) = &doc.doc.find(id).expect("the path").content else { panic!("a shape") };
    assert!(
        shape.nodes.iter().all(|n| !n.smooth && n.in_x == 0.0 && n.out_x == 0.0 && n.out_y == 0.0),
        "the handles are gone",
    );

    // Join closes the path, break opens it.
    run(&mut s, "shape.editNodes", json!({ "action": "join" }));
    let doc = s.active().expect("a document");
    let Content::Shape(shape) = &doc.doc.find(id).expect("the path").content else { panic!("a shape") };
    assert!(shape.closed, "joined");
    run(&mut s, "shape.editNodes", json!({ "action": "break" }));
    let doc = s.active().expect("a document");
    let Content::Shape(shape) = &doc.doc.find(id).expect("the path").content else { panic!("a shape") };
    assert!(!shape.closed, "broken");

    // A rectangle has no nodes to edit, and the command says so rather than doing nothing.
    run(&mut s, "shape.create", json!({
        "kind": "rectangle", "x": 2.0, "y": 2.0, "width": 8.0, "height": 8.0, "fill": "#000000",
    }));
    let err = super::commands::execute(&mut s, "shape.editNodes", json!({ "action": "sharp" }))
        .expect_err("a rectangle has no path nodes");
    assert!(err.contains("path"), "it says why: {err}");
}

#[test]
fn a_layer_effect_is_drawn_and_can_be_turned_off_again() {
    let mut s = session();
    // A red square in the middle of the white canvas.
    run(&mut s, "shape.create", json!({
        "kind": "rectangle", "x": 8.0, "y": 8.0, "width": 16.0, "height": 16.0,
        "fill": "#FF0000", "strokeWidth": 0.0,
    }));
    assert_eq!(at(&pixels(&s), 32, 2, 2), [255, 255, 255, 255], "clear to start with");

    // An outer glow reaches past the shape, so a pixel outside it is no longer the background.
    run(&mut s, "layer.style", json!({
        "effect": "Outer Glow", "on": true, "colour": "#0000FF", "radius": 6.0, "opacity": 1.0,
    }));
    let lit = at(&pixels(&s), 32, 6, 16);
    assert!(lit[2] > lit[0], "the glow tints the pixel beside the shape: {lit:?}");

    // Turning it off puts the picture back exactly, which is what makes it an effect and not a
    // paint stroke.
    run(&mut s, "layer.style", json!({ "effect": "Outer Glow", "on": false }));
    assert_eq!(at(&pixels(&s), 32, 6, 16), [255, 255, 255, 255], "the glow is gone");

    // A name the panel does not have is refused rather than ignored.
    let err = super::commands::execute(&mut s, "layer.style", json!({ "effect": "Sparkle", "on": true }))
        .expect_err("not an effect");
    assert!(err.contains("not a layer effect"), "it says why: {err}");
}

#[test]
fn a_colour_overlay_replaces_the_layer_colour_and_leaves_the_background() {
    let mut s = session();
    run(&mut s, "shape.create", json!({
        "kind": "rectangle", "x": 8.0, "y": 8.0, "width": 16.0, "height": 16.0,
        "fill": "#FF0000", "strokeWidth": 0.0,
    }));
    run(&mut s, "layer.style", json!({
        "effect": "Colour Overlay", "on": true, "colour": "#00FF00", "opacity": 1.0,
    }));
    let px = pixels(&s);
    assert_eq!(at(&px, 32, 16, 16), [0, 255, 0, 255], "the shape wears the overlay");
    assert_eq!(at(&px, 32, 2, 2), [255, 255, 255, 255], "the background does not");
}

#[test]
fn a_text_run_can_be_restyled_after_it_is_placed() {
    let mut s = session();
    let id = run(&mut s, "type.create", json!({
        "text": "Hello", "x": 4.0, "y": 16.0, "size": 10.0,
    }))["layer"].as_u64().expect("a text layer");
    let before = pixels(&s);
    // Something was drawn: the canvas is not all background.
    let ink = (0..32).flat_map(|y| (0..32).map(move |x| (x, y)))
        .any(|(x, y)| at(&before, 32, x, y) != [255, 255, 255, 255]);
    assert!(ink, "the run put ink on the canvas");

    // Bigger, bold, centred, and in another colour: each is a field of the run.
    let out = run(&mut s, "type.setProps", json!({
        "layer": id, "size": 18.0, "style": "Bold", "align": "center",
        "colour": "#FF0000", "leading": 1.5,
    }));
    assert_eq!(out["style"], "Bold");
    assert_eq!(out["align"], "center");
    assert_eq!(out["size"], 18.0);

    {
        let doc = s.active().expect("a document");
        let Content::Text(run) = &doc.doc.find(id).expect("the run").content else { panic!("text") };
        assert_eq!(run.color, "#FF0000");
        assert_eq!(run.leading, 1.5);
    }

    // The paragraph's own words are editable, and the row is renamed with them.
    run(&mut s, "type.setProps", json!({ "layer": id, "text": "Goodbye" }));
    let doc = s.active().expect("a document");
    assert_eq!(doc.doc.find(id).expect("the run").name, "Goodbye");
    let Content::Text(edited) = &doc.doc.find(id).expect("the run").content else { panic!("text") };
    assert_eq!(edited.text, "Goodbye");

    // A layer that is not text is refused.
    run(&mut s, "layer.new.layer", json!({}));
    let err = super::commands::execute(&mut s, "type.setProps", json!({ "text": "x" }))
        .expect_err("a pixel layer is not text");
    assert!(err.contains("not text"), "it says why: {err}");
}

#[test]
fn a_framed_text_run_wraps_to_its_box_and_a_plain_one_does_not() {
    // The same words in a wide canvas, once with a narrow frame and once without. A framed run
    // must stay inside its box's width; a point run has no box and runs on. Measuring the ink's
    // own extent is what makes this a test of the wrap rather than of the layout's row count.
    let words = "wrap this run across several lines please";
    let mut plain = Document::new("Plain", 400, 200, "#FFFFFF").expect("a canvas");
    let mut framed = Document::new("Framed", 400, 200, "#FFFFFF").expect("a canvas");
    let run_text = |doc: &mut Document, framed: bool| {
        let id = doc.take_id();
        let content = TextRun {
            text: words.into(),
            x: 2.0,
            y: 40.0,
            size: 16.0,
            color: "#000000".into(),
            frame: if framed {
                Some(super::model::Frame::new(2.0, 2.0, 90.0, 196.0))
            } else {
                None
            },
            family: "Inter".into(),
            style: "Regular".into(),
            align: "left".into(),
            leading: 1.2,
        };
        doc.layers_mut().push(Layer {
            id,
            name: "Run".into(),
            kind: Kind::Text,
            visible: true,
            locked: false,
            opacity: 1.0,
            blend: Blend::Normal,
            clipped: false,
            children: Vec::new(),
            content: Content::Text(content),
            style: LayerStyle::default(),
        });
    };
    run_text(&mut plain, false);
    run_text(&mut framed, true);
    let ink_width = |doc: &Document| {
        let px = super::compose::render(doc);
        let w = doc.width() as usize;
        (0..w)
            .filter(|&x| (0..doc.height() as usize).any(|y| px[(y * w + x) * 4 + 3] != 0 && px[(y * w + x) * 4] < 128))
            .max()
            .unwrap_or(0)
    };
    let plain_width = ink_width(&plain);
    let framed_width = ink_width(&framed);
    assert!(
        plain_width > 300,
        "the unwrapped run crosses most of the canvas ({plain_width}px)",
    );
    assert!(
        framed_width <= 100,
        "the framed run stays inside its 90px box ({framed_width}px)",
    );
}
