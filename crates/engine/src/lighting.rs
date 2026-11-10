//! Scene lights: the coloured, placeable light objects a sequence's 3D layers are lit with.

use serde_json::{Value, json};

use crate::commands::{CommandSpec, bad, has_seq};
use crate::{EngineError, Result, Session};

/// One spec that journals its edit (undoable), like the other panels' set commands.
fn set_spec(
    id: &'static str,
    label: &'static str,
    params: &'static str,
    run: fn(&mut Session, &Value) -> Result<Value>,
) -> CommandSpec {
    CommandSpec { id, label, menu: &[], shortcut: None, params, enabled: has_seq, run, journal: true }
}

fn light_from(v: &Value, cmd: &str) -> Result<filmcraft_project::Light3D> {
    serde_json::from_value(v.clone()).map_err(|e| bad(cmd, format!("bad light: {e}")))
}

fn lighting_json(s: &Session) -> Result<Value> {
    let seq = s.active_sequence().ok_or(EngineError::NoSequence)?;
    let l = &seq.scene_lighting;
    Ok(json!({
        "lights": l.lights.iter().map(|x| serde_json::to_value(x).unwrap_or(json!({}))).collect::<Vec<_>>(),
        "ambientColor": l.ambient_color,
        "ambientLevel": l.ambient_level,
    }))
}

pub fn commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "lights.list",
            label: "Scene Lights",
            menu: &[],
            shortcut: None,
            params: "{}",
            enabled: has_seq,
            run: |s, _| lighting_json(s),
            journal: false,
        },
        set_spec(
            "lights.set",
            "Set Scene Light",
            r#"{"id":u64=0 (0 = new), "name":str, "enabled":bool, "kind":"directional"|"point"|"spot", "color":[r,g,b] sRGB, "intensity":f64, "position":[x,y,z], "angle":f64, "elevation":f64, "range":f64, "cone":f64, "softness":f64, "merge":str?}"#,
            |s, p| {
                let id = p.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
                let mut light = light_from(p, "lights.set")?;
                let merge = p.get("merge").and_then(|v| v.as_str()).map(str::to_string);
                let out = s.edit_sequence_as("Set Scene Light", merge.as_deref(), |seq, _ctx, _st| {
                    let lights = &mut seq.scene_lighting.lights;
                    let id = if id == 0 {
                        let next = lights.iter().map(|l| l.id).max().unwrap_or(0) + 1;
                        light.id = next;
                        lights.push(light.clone());
                        next
                    } else {
                        light.id = id;
                        match lights.iter_mut().find(|l| l.id == id) {
                            Some(slot) => *slot = light.clone(),
                            None => lights.push(light.clone()),
                        }
                        id
                    };
                    Ok(id)
                })?;
                let mut v = lighting_json(s)?;
                v["id"] = json!(out);
                Ok(v)
            },
        ),
        set_spec(
            "lights.ambient",
            "Set Ambient Light",
            r#"{"color":[r,g,b] sRGB, "level":f64, "merge":str?}"#,
            |s, p| {
                let color = p.get("color").and_then(|v| v.as_array()).map(|a| {
                    std::array::from_fn::<_, 3, _>(|i| a.get(i).and_then(|x| x.as_f64()).unwrap_or(1.0) as f32)
                });
                let level = p.get("level").and_then(|v| v.as_f64()).map(|v| v as f32);
                let merge = p.get("merge").and_then(|v| v.as_str()).map(str::to_string);
                s.edit_sequence_as("Set Ambient Light", merge.as_deref(), |seq, _ctx, _st| {
                    if let Some(c) = color {
                        seq.scene_lighting.ambient_color = c;
                    }
                    if let Some(l) = level {
                        seq.scene_lighting.ambient_level = l.clamp(0.0, 4.0);
                    }
                    Ok(())
                })?;
                lighting_json(s)
            },
        ),
        set_spec(
            "lights.remove",
            "Remove Scene Light",
            r#"{"id":u64}"#,
            |s, p| {
                let id = p.get("id").and_then(|v| v.as_u64()).ok_or_else(|| bad("lights.remove", "id is required"))?;
                s.edit_sequence_as("Remove Scene Light", None, |seq, _ctx, _st| {
                    seq.scene_lighting.lights.retain(|l| l.id != id);
                    Ok(())
                })?;
                lighting_json(s)
            },
        ),
    ]
}
