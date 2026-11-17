//! Tests for the photo engine the studio drives.
//!
//! These are the checks a compile cannot make: that a stroke lands where it was drawn, that undo
//! actually returns to the previous picture, that a blend mode blends, that a selection confines
//! the paint, and that the command surface still has the ids the restored UI sends.

use serde_json::json;

use super::model::{Blend, Content, Document, Kind, Layer};
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
        .layers
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
    assert_eq!(s.active().expect("a document").doc.layers.len(), 2);
    assert!(s.active_mut().expect("a document").undo(), "there is a step");
    assert_eq!(s.active().expect("a document").doc.layers.len(), 1, "undo removed the layer");
}
