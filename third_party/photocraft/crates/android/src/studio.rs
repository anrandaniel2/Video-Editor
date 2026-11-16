//! The drawing studio's brush shelf: the drawing app's brush sets, over the engine's preset list.
//!
//! The studio's Brushes panel lists sets and brushes, the way the drawing app does: a column of
//! sets, each opening onto its brushes, with a preview for each. The engine already keeps the
//! library ([`photocraft_engine::Session::tools`]`::presets`) and every command that changes it
//! (`brush.presets.save`, `…update`, `…delete`, `…list`, `tools.setBrush`), so the panel drives
//! those; this module adds only what the panel needs to show the shelf:
//!
//! - [`PhotoSession::studio_brush_sets`]: the live library, grouped into its sets, in the order the
//!   drawing app shows them. A set the person emptied is gone; a set they made (with a preset of
//!   their own under a new group) is there too, after the shipped ones.
//! - [`PhotoSession::studio_install_brushes`]: puts the drawing app's library in place of the
//!   engine's generic preset set. Called once for a new studio document, so the studio's brushes
//!   are the drawing app's — the same names, the same sets, the same strokes.

use std::collections::BTreeMap;

use photocraft_paint::studiobrush;
use serde_json::{Value, json};

use crate::{PhotoError, PhotoSession};

/// One brush in a set: what the panel's grid shows.
#[derive(Clone, Debug, uniffi::Record)]
pub struct StudioBrush {
    /// The preset's name, and the name `tools.setBrush`'s `preset` takes.
    pub name: String,
    /// How wide the stroke draws at size 100 %: the tip's own size, in pixels.
    pub size: f32,
    /// 0..1.
    pub hardness: f32,
    /// `"round"` or `"sampled"`: a computed tip or a bitmap.
    pub tip: String,
    /// The brush's own stabilisation, 0..1, or `-1` when it has none. `tools.setBrush` keeps the
    /// tool's smoothing otherwise (it is a tool option there), so the panel sends this back with
    /// the preset to keep the stroke the way the brush was made.
    pub smoothing: f32,
    /// The preset the engine ships (true) or one the person made or imported (false).
    pub builtin: bool,
}

/// One set of brushes: the Brushes panel's column of sets.
#[derive(Clone, Debug, uniffi::Record)]
pub struct StudioBrushSet {
    pub name: String,
    pub brushes: Vec<StudioBrush>,
}

/// The drawing app's brush shelf, with no session and no document: every set and brush the studio
/// ships, for a host that wants the same names (FilmCraft's drawing canvas lists this shelf beside
/// the studio's, so a brush is the same brush in both).
#[uniffi::export]
pub fn studio_shelf() -> Vec<StudioBrushSet> {
    let brushes: Vec<Value> = studiobrush::library()
        .into_iter()
        .map(|p| {
            json!({
                "name": p.name,
                "group": p.group,
                "size": p.brush.size,
                "hardness": p.brush.hardness,
                "builtin": p.builtin,
                "tip": match &p.brush.tip {
                    photocraft_paint::TipShape::Round => "round",
                    _ => "sampled",
                },
                "smoothing": if p.brush.smoothing.enabled { p.brush.smoothing.amount } else { -1.0 },
            })
        })
        .collect();
    let mut sets: BTreeMap<String, Vec<StudioBrush>> = BTreeMap::new();
    for v in &brushes {
        let Some((group, brush)) = brush_of(v) else { continue };
        sets.entry(group).or_default().push(brush);
    }
    let order = studibrush_order();
    let mut out: Vec<StudioBrushSet> = sets.into_iter().map(|(name, brushes)| StudioBrushSet { name, brushes }).collect();
    out.sort_by(|a, b| {
        let rank = |n: &str| order.iter().position(|g| g.eq_ignore_ascii_case(n)).unwrap_or(usize::MAX);
        rank(&a.name).cmp(&rank(&b.name)).then_with(|| a.name.cmp(&b.name))
    });
    out
}

/// A preset as the engine lists it, flattened for the shelf, with the set it is filed under.
fn brush_of(v: &Value) -> Option<(String, StudioBrush)> {
    let group = v.get("group").and_then(Value::as_str).unwrap_or_default().trim().to_owned();
    let brush = StudioBrush {
        name: v.get("name")?.as_str()?.to_owned(),
        size: v.get("size").and_then(Value::as_f64).unwrap_or(20.0) as f32,
        hardness: v.get("hardness").and_then(Value::as_f64).unwrap_or(1.0) as f32,
        tip: v.get("tip").and_then(Value::as_str).unwrap_or("round").to_owned(),
        smoothing: v.get("smoothing").and_then(Value::as_f64).unwrap_or(-1.0) as f32,
        builtin: v.get("builtin").and_then(Value::as_bool).unwrap_or(false),
    };
    // A preset with no group of its own is filed by the library, so imported and older presets
    // land in their set; anything else the person made is "Custom".
    let group = if group.is_empty() { studio_group(&brush.name).unwrap_or_else(|| "Custom".to_owned()) } else { group };
    Some((group, brush))
}

#[uniffi::export]
impl PhotoSession {
    /// The studio's brush shelf: every set, in the drawing app's order, each with its brushes in
    /// the library's order. Presets the person made or imported come under their own group, after
    /// the shipped sets.
    ///
    /// Read from the engine's live preset list: a preset saved, renamed or deleted in the panel is
    /// in the next call, with no second copy of the library to keep in step.
    pub fn studio_brush_sets(&self) -> Result<Vec<StudioBrushSet>, PhotoError> {
        let listed = {
            let mut s = self.lock();
            s.execute("brush.presets.list", json!({})).map_err(|e| PhotoError::Command(e.to_string()))?
        };
        let presets = listed.get("presets").and_then(Value::as_array).cloned().unwrap_or_default();
        // Grouped by set, in a name-ordered map so a set's brushes keep the library's order
        // (`builtin()` and the preset store both append, so the list is already in shelf order).
        let mut sets: BTreeMap<String, Vec<StudioBrush>> = BTreeMap::new();
        for p in &presets {
            let Some((group, brush)) = brush_of(p) else { continue };
            sets.entry(group).or_default().push(brush);
        }
        let order = studibrush_order();
        let mut out: Vec<StudioBrushSet> = sets.into_iter().map(|(name, brushes)| StudioBrushSet { name, brushes }).collect();
        // The shipped sets first, in the drawing app's order; the person's own after them, by name.
        out.sort_by(|a, b| {
            let rank = |n: &str| order.iter().position(|g| g.eq_ignore_ascii_case(n)).unwrap_or(usize::MAX);
            rank(&a.name).cmp(&rank(&b.name)).then_with(|| a.name.cmp(&b.name))
        });
        Ok(out)
    }

    /// Put the drawing studio's brushes in place of the engine's generic preset set: the six
    /// brushes a photo editor starts with are not the shelf of a drawing app.
    ///
    /// Called for a new studio document. Presets the person made or imported are kept (they are
    /// theirs, not the engine's); the engine's own are replaced. Returns how many brushes the shelf
    /// now has.
    pub fn studio_install_brushes(&self) -> Result<u32, PhotoError> {
        let mut s = self.lock();
        let library = studiobrush::library();
        let (mut added, mut removed) = (0u32, 0u32);
        // Drop the engine's generic built-ins that the studio's own library replaces (by name), so
        // no set lists both.
        let shelf: std::collections::HashSet<String> = library.iter().map(|p| p.name.to_lowercase()).collect();
        s.tools.presets.retain(|p| {
            let drop = p.builtin && !shelf.contains(&p.name.to_lowercase());
            if drop {
                removed += 1;
            }
            !drop
        });
        for p in library {
            if s.tools.presets.iter().any(|q| q.name.eq_ignore_ascii_case(&p.name)) {
                continue;
            }
            s.tools.presets.push(p);
            added += 1;
        }
        if added > 0 || removed > 0 {
            s.brush_presets_changed();
        }
        Ok(s.tools.presets.len() as u32)
    }
}

/// The drawing app's set order (a `&'static str` list, as `studiobrush::SET_NAMES`).
fn studibrush_order() -> Vec<&'static str> {
    studiobrush::SET_NAMES.to_vec()
}

/// The group a studio brush is filed under, by name, from the library.
fn studio_group(name: &str) -> Option<String> {
    studiobrush::find(name).map(|p| p.group)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shelf_is_the_library_with_no_session() {
        let shelf = studio_shelf();
        assert_eq!(shelf.len(), studiobrush::SET_NAMES.len(), "one set per shipped set");
        assert_eq!(shelf[0].name, "Sketching");
        assert_eq!(shelf[0].brushes[0].name, "Graphite");
        assert_eq!(shelf.iter().map(|s| s.brushes.len()).sum::<usize>(), studiobrush::library().len());
        assert!(shelf.iter().all(|s| s.brushes.iter().all(|b| b.size > 0.0)));
    }

    #[test]
    fn a_brush_carries_its_own_stabilisation() {
        // `tools.setBrush` keeps the *tool's* smoothing when a preset has its own (`picked_over`),
        // so the shelf has to publish each brush's amount for the panel to send it back; without
        // it every studio brush painted with the tool's default and lost its streamline.
        let shelf = studio_shelf();
        let pen = shelf.iter().flat_map(|s| &s.brushes).find(|b| b.name == "Ink Pen").expect("Ink Pen");
        assert!(pen.smoothing > 0.0, "Ink Pen's stabilisation: {}", pen.smoothing);
        let pencil = shelf.iter().flat_map(|s| &s.brushes).find(|b| b.name == "Soft Pencil").expect("Soft Pencil");
        assert!(pencil.smoothing < 0.0, "Soft Pencil has no stabilisation: {}", pencil.smoothing);
        // And sending it back puts it on the brush. (`tools.setBrush` needs no document.)
        let s = PhotoSession::new();
        s.execute("tools.setBrush".into(), json!({ "preset": "Ink Pen", "smoothing": pen.smoothing }).to_string()).unwrap();
        let b: Value = serde_json::from_str(&s.brush()).unwrap();
        assert_eq!(b["smoothing"]["enabled"], Value::Bool(true));
        assert!((b["smoothing"]["amount"].as_f64().unwrap() - f64::from(pen.smoothing)).abs() < 1e-6);
    }

    #[test]
    fn every_library_brush_has_a_set_in_the_order() {
        let order = studibrush_order();
        for p in studiobrush::library() {
            assert!(order.iter().any(|g| g.eq_ignore_ascii_case(&p.group)), "{}: {}", p.name, p.group);
            assert_eq!(studio_group(&p.name).as_deref(), Some(p.group.as_str()));
        }
    }
}
