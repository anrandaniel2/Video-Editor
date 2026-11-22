//! Scene lights: the coloured, placeable light objects a sequence's 3D layers are lit with.

use serde_json::{Value, json};

use crate::commands::{CommandSpec, bad, has_seq};
use crate::{EngineError, Result, Session};

/// One spec that journals its edit (undoable), like the other panels' set commands.
fn set_spec(id: &'static str, label: &'static str, params: &'static str, run: fn(&mut Session, &Value) -> Result<Value>) -> CommandSpec {
    CommandSpec { id, label, menu: &[], shortcut: None, params, enabled: has_seq, run, journal: true }
}

fn light_from(v: &Value, cmd: &str) -> Result<filmcraft_project::Light3D> {
    serde_json::from_value(v.clone()).map_err(|e| bad(cmd, format!("bad light: {e}")))
}

fn scene_camera_json(s: &Session) -> Result<Value> {
    let seq = s.active_sequence().ok_or(EngineError::NoSequence)?;
    Ok(match &seq.scene_camera {
        Some(c) => json!({
            "position": c.position,
            "yaw": c.yaw,
            "pitch": c.pitch,
            "roll": c.roll,
            "fov": c.fov,
        }),
        None => Value::Null,
    })
}

fn camera_solves_json(s: &Session) -> Result<Value> {
    let seq = s.active_sequence().ok_or(EngineError::NoSequence)?;
    let list: Vec<Value> = seq
        .camera_solves
        .iter()
        .map(|cs| {
            let base = seq.find_item(cs.clip).map(|(_, c)| c.start.0).unwrap_or(0);
            json!({
                "id": cs.id,
                "clip": cs.clip.0,
                "name": cs.name,
                "rotationOnly": cs.rotation_only,
                "shotsSkipped": cs.shots_skipped,
                "frames": cs.frames.len(),
                "points": cs.points.len(),
                "start": base.saturating_add(cs.frames.first().map(|f| f.time.0).unwrap_or(0)),
                "end": base.saturating_add(cs.frames.last().map(|f| f.time.0).unwrap_or(0)),
                "medianRmsPx": cs.median_rms(),
                "quality": (cs.median_confidence() * 100.0).clamp(0.0, 100.0),
                "lost": cs.lost_frames(),
            })
        })
        .collect();
    Ok(json!({ "solves": list }))
}

fn matchmove_json(s: &Session) -> Result<Value> {
    let seq = s.active_sequence().ok_or(EngineError::NoSequence)?;
    Ok(match &seq.matchmove {
        Some(m) => json!({
            "solve": m.solve,
            "enabled": m.enabled,
            "scalePx": m.scale_px,
            "yawTrim": m.yaw_trim,
            "pitchTrim": m.pitch_trim,
            "rollTrim": m.roll_trim,
            "offset": m.offset,
        }),
        None => Value::Null,
    })
}

fn set_matchmove_cmd(s: &mut Session, p: &Value) -> Result<Value> {
    let solve = p.get("solve").and_then(Value::as_u64).unwrap_or(0);
    let merge = p.get("merge").and_then(Value::as_str).map(str::to_string);
    if solve == 0 {
        s.edit_sequence_as("Clear Match-Move", merge.as_deref(), |seq, _ctx, _st| {
            seq.matchmove = None;
            Ok(())
        })?;
        return Ok(Value::Null);
    }
    let has_solve = s.active_sequence().ok_or(EngineError::NoSequence)?.camera_solves.iter().any(|cs| cs.id == solve);
    if !has_solve {
        return Err(bad("scene.setMatchmove", format!("no camera solve {solve}")));
    }
    let enabled = p.get("enabled").and_then(Value::as_bool);
    let f_opt = |k: &str| p.get(k).and_then(Value::as_f64).filter(|v| v.is_finite()).map(|v| v as f32);
    let scale_px = f_opt("scalePx");
    let yaw_trim = f_opt("yawTrim");
    let pitch_trim = f_opt("pitchTrim");
    let roll_trim = f_opt("rollTrim");
    let offset = p.get("offset").and_then(Value::as_array).and_then(|arr| {
        (arr.len() >= 3).then(|| {
            std::array::from_fn::<_, 3, _>(|i| arr.get(i).and_then(Value::as_f64).filter(|v| v.is_finite()).unwrap_or(0.0) as f32)
        })
    });
    s.edit_sequence_as("Set Match-Move", merge.as_deref(), |seq, _ctx, _st| {
        let base = seq.matchmove.filter(|m| m.solve == solve).unwrap_or_else(|| filmcraft_project::SequenceMatchMove::new(solve));
        let off = offset.unwrap_or(base.offset);
        seq.matchmove = Some(filmcraft_project::SequenceMatchMove {
            solve,
            enabled: enabled.unwrap_or(base.enabled),
            scale_px: scale_px.unwrap_or(base.scale_px).clamp(1.0, 100_000.0),
            yaw_trim: yaw_trim.unwrap_or(base.yaw_trim).clamp(-180.0, 180.0),
            pitch_trim: pitch_trim.unwrap_or(base.pitch_trim).clamp(-180.0, 180.0),
            roll_trim: roll_trim.unwrap_or(base.roll_trim).clamp(-180.0, 180.0),
            offset: [off[0].clamp(-100_000.0, 100_000.0), off[1].clamp(-100_000.0, 100_000.0), off[2].clamp(-100_000.0, 100_000.0)],
        });
        Ok(())
    })?;
    matchmove_json(s)
}

fn delete_camera_solve_cmd(s: &mut Session, p: &Value) -> Result<Value> {
    let id = p.get("id").and_then(Value::as_u64).ok_or_else(|| bad("scene.deleteCameraSolve", "id is required"))?;
    s.edit_sequence_as("Delete Camera Solve", None, |seq, _ctx, _st| {
        seq.camera_solves.retain(|cs| cs.id != id);
        if seq.matchmove.is_some_and(|m| m.solve == id) {
            seq.matchmove = None;
        }
        Ok(())
    })?;
    camera_solves_json(s)
}

fn interp_name(i: filmcraft_project::Interpolation) -> &'static str {
    match i {
        filmcraft_project::Interpolation::Linear => "linear",
        filmcraft_project::Interpolation::Hold => "hold",
        filmcraft_project::Interpolation::EaseIn => "easeIn",
        filmcraft_project::Interpolation::EaseOut => "easeOut",
        filmcraft_project::Interpolation::AutoBezier
        | filmcraft_project::Interpolation::ContinuousBezier
        | filmcraft_project::Interpolation::Bezier => "smooth",
    }
}

fn parse_interp(name: &str) -> filmcraft_project::Interpolation {
    match name {
        "hold" => filmcraft_project::Interpolation::Hold,
        "easeIn" | "ease_in" => filmcraft_project::Interpolation::EaseIn,
        "easeOut" | "ease_out" => filmcraft_project::Interpolation::EaseOut,
        "smooth" | "autoBezier" | "bezier" => filmcraft_project::Interpolation::AutoBezier,
        _ => filmcraft_project::Interpolation::Linear,
    }
}

fn tick_arg(s: &Session, p: &Value) -> filmcraft_time::Tick {
    let raw = p.get("time").and_then(Value::as_i64).map(filmcraft_time::Tick).unwrap_or_else(|| s.playhead());
    s.sequence_rate().snap(raw)
}

fn lighting_json_at(s: &Session, at: filmcraft_time::Tick) -> Result<Value> {
    let seq = s.active_sequence().ok_or(EngineError::NoSequence)?;
    let l = &seq.scene_lighting;
    let lights: Vec<Value> = l
        .lights
        .iter()
        .map(|raw| {
            let eval = raw.evaluated_at(at);
            let mut obj = serde_json::to_value(&eval).unwrap_or_else(|_| json!({}));
            obj["animated"] = json!(raw.is_animated());
            obj["hasKeyframe"] = json!(raw.has_keyframe_at(at));
            obj["interpolation"] = json!(interp_name(raw.interpolation_at(at)));
            obj["keyframes"] = json!(raw.keyframe_times().into_iter().map(|t| t.0).collect::<Vec<i64>>());
            obj
        })
        .collect();
    Ok(json!({
        "lights": lights,
        "ambientColor": l.ambient_color,
        "ambientLevel": l.ambient_level,
    }))
}

fn lighting_json(s: &Session) -> Result<Value> {
    lighting_json_at(s, s.sequence_rate().snap(s.playhead()))
}

pub fn commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "lights.list",
            label: "Scene Lights",
            menu: &[],
            shortcut: None,
            params: r#"{"time":i64?}"#,
            enabled: has_seq,
            run: |s, p| lighting_json_at(s, tick_arg(s, p)),
            journal: false,
        },
        set_spec(
            "lights.set",
            "Set Scene Light",
            r#"{"id":u64=0 (0 = new), "name":str, "enabled":bool, "kind":"directional"|"point"|"spot", "color":[r,g,b] sRGB, "intensity":f64, "position":[x,y,z], "angle":f64, "elevation":f64, "range":f64, "cone":f64, "softness":f64, "time":i64?, "merge":str?}"#,
            |s, p| {
                let id = p.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
                let mut light = light_from(p, "lights.set")?;
                let at = tick_arg(s, p);
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
                            Some(slot) => slot.update_from_edit(&light, at),
                            None => lights.push(light.clone()),
                        }
                        id
                    };
                    Ok(id)
                })?;
                let mut v = lighting_json_at(s, at)?;
                v["id"] = json!(out);
                Ok(v)
            },
        ),
        set_spec("lights.toggleAnimation", "Toggle Light Keyframes", r#"{"id":u64, "time":i64?}"#, |s, p| {
            let id = p.get("id").and_then(Value::as_u64).ok_or_else(|| bad("lights.toggleAnimation", "id is required"))?;
            let at = tick_arg(s, p);
            s.edit_sequence_as("Toggle Light Keyframes", None, |seq, _ctx, _st| {
                let light = seq.scene_lighting.lights.iter_mut().find(|l| l.id == id).ok_or_else(|| bad("lights.toggleAnimation", format!("no light {id}")))?;
                let _ = light.toggle_animation(at);
                Ok(())
            })?;
            lighting_json_at(s, at)
        }),
        set_spec("lights.addKeyframe", "Add Light Keyframe", r#"{"id":u64, "time":i64?, "interpolation":str?}"#, |s, p| {
            let id = p.get("id").and_then(Value::as_u64).ok_or_else(|| bad("lights.addKeyframe", "id is required"))?;
            let at = tick_arg(s, p);
            let interp_override = p.get("interpolation").and_then(Value::as_str).map(parse_interp);
            s.edit_sequence_as("Add Light Keyframe", None, |seq, _ctx, _st| {
                let light = seq.scene_lighting.lights.iter_mut().find(|l| l.id == id).ok_or_else(|| bad("lights.addKeyframe", format!("no light {id}")))?;
                let interp = interp_override.unwrap_or_else(|| light.interpolation_at(at));
                light.add_keyframe_at(at, interp);
                Ok(())
            })?;
            lighting_json_at(s, at)
        }),
        set_spec("lights.removeKeyframe", "Remove Light Keyframe", r#"{"id":u64, "time":i64?}"#, |s, p| {
            let id = p.get("id").and_then(Value::as_u64).ok_or_else(|| bad("lights.removeKeyframe", "id is required"))?;
            let at = tick_arg(s, p);
            s.edit_sequence_as("Remove Light Keyframe", None, |seq, _ctx, _st| {
                let light = seq.scene_lighting.lights.iter_mut().find(|l| l.id == id).ok_or_else(|| bad("lights.removeKeyframe", format!("no light {id}")))?;
                let _ = light.remove_keyframe_at(at);
                Ok(())
            })?;
            lighting_json_at(s, at)
        }),
        set_spec("lights.setInterpolation", "Set Light Keyframe Curve", r#"{"id":u64, "time":i64?, "interpolation":str}"#, |s, p| {
            let id = p.get("id").and_then(Value::as_u64).ok_or_else(|| bad("lights.setInterpolation", "id is required"))?;
            let interp_str = p.get("interpolation").and_then(Value::as_str).unwrap_or("linear");
            let interp = parse_interp(interp_str);
            let at = tick_arg(s, p);
            s.edit_sequence_as("Set Light Keyframe Curve", None, |seq, _ctx, _st| {
                let light = seq.scene_lighting.lights.iter_mut().find(|l| l.id == id).ok_or_else(|| bad("lights.setInterpolation", format!("no light {id}")))?;
                let _ = light.set_interpolation_at(at, interp);
                Ok(())
            })?;
            lighting_json_at(s, at)
        }),
        set_spec("lights.ambient", "Set Ambient Light", r#"{"color":[r,g,b] sRGB, "level":f64, "merge":str?}"#, |s, p| {
            let color =
                p.get("color").and_then(|v| v.as_array()).map(|a| std::array::from_fn::<_, 3, _>(|i| a.get(i).and_then(|x| x.as_f64()).unwrap_or(1.0) as f32));
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
        }),
        CommandSpec {
            id: "scene.camera",
            label: "Scene Camera",
            menu: &[],
            shortcut: None,
            params: "{}",
            enabled: has_seq,
            run: |s, _| scene_camera_json(s),
            journal: false,
        },
        set_spec("scene.setCamera", "Set Scene Camera", r#"{"position":[x,y,z] f64, "yaw":f64, "pitch":f64, "roll":f64, "fov":f64, "merge":str?}"#, |s, p| {
            let pos = p
                .get("position")
                .and_then(|v| v.as_array().cloned())
                .map(|a| std::array::from_fn::<_, 3, _>(|i| a.get(i).and_then(|x| x.as_f64()).unwrap_or(0.0) as f32))
                .unwrap_or([0.0; 3]);
            let pick = |k: &str, d: f64| p.get(k).and_then(|v| v.as_f64()).unwrap_or(d);
            let cam = filmcraft_project::SceneCamera3D {
                position: pos,
                yaw: pick("yaw", 0.0) as f32,
                pitch: pick("pitch", 0.0) as f32,
                roll: pick("roll", 0.0) as f32,
                fov: (pick("fov", filmcraft_project::CLASSIC_FOV as f64) as f32).clamp(1.0, 179.0),
            };
            let merge = p.get("merge").and_then(|v| v.as_str()).map(str::to_string);
            s.edit_sequence_as("Set Scene Camera", merge.as_deref(), |seq, _ctx, _st| {
                seq.scene_camera = Some(cam);
                Ok(())
            })?;
            scene_camera_json(s)
        }),
        set_spec("scene.clearCamera", "Remove Scene Camera", r#"{}"#, |s, _p| {
            s.edit_sequence_as("Remove Scene Camera", None, |seq, _ctx, _st| {
                seq.scene_camera = None;
                Ok(())
            })?;
            Ok(json!({ "cleared": true }))
        }),
        set_spec("lights.remove", "Remove Scene Light", r#"{"id":u64}"#, |s, p| {
            let id = p.get("id").and_then(|v| v.as_u64()).ok_or_else(|| bad("lights.remove", "id is required"))?;
            s.edit_sequence_as("Remove Scene Light", None, |seq, _ctx, _st| {
                seq.scene_lighting.lights.retain(|l| l.id != id);
                Ok(())
            })?;
            lighting_json(s)
        }),
        CommandSpec {
            id: "scene.cameraSolves",
            label: "Camera Solves",
            menu: &[],
            shortcut: None,
            params: "{}",
            enabled: has_seq,
            run: |s, _| camera_solves_json(s),
            journal: false,
        },
        set_spec("scene.deleteCameraSolve", "Delete Camera Solve", r#"{"id":u64}"#, delete_camera_solve_cmd),
        CommandSpec {
            id: "scene.matchmove",
            label: "Camera Match-Move",
            menu: &[],
            shortcut: None,
            params: "{}",
            enabled: has_seq,
            run: |s, _| matchmove_json(s),
            journal: false,
        },
        set_spec(
            "scene.setMatchmove",
            "Set Camera Match-Move",
            r#"{"solve":u64 (0 = clear), "enabled":bool?, "scalePx":f64?, "yawTrim":f64?, "pitchTrim":f64?, "rollTrim":f64?, "merge":str?}"#,
            set_matchmove_cmd,
        ),
        set_spec("scene.clearMatchmove", "Clear Camera Match-Move", r#"{}"#, |s, _p| {
            s.edit_sequence_as("Clear Match-Move", None, |seq, _ctx, _st| {
                seq.matchmove = None;
                Ok(())
            })?;
            Ok(json!({ "cleared": true }))
        }),
        set_spec("scene.levelMatchmove", "Auto-Level Match-Move Ground", r#"{}"#, level_matchmove_cmd),
        set_spec("scene.bakeMatchmoveFrame", "Bake Match-Move Camera", r#"{"time":i64?, "disableMatchmove":bool?}"#, bake_matchmove_cmd),
        CommandSpec {
            id: "scene.exportMatchmove",
            label: "Export Camera Match-Move",
            menu: &[],
            shortcut: None,
            params: "{}",
            enabled: has_seq,
            run: |s, _| export_matchmove_json(s),
            journal: false,
        },
    ]
}

fn level_matchmove_cmd(s: &mut Session, _p: &Value) -> Result<Value> {
    let normal = {
        let seq = s.active_sequence().ok_or(EngineError::NoSequence)?;
        let mm = seq.matchmove.ok_or_else(|| bad("scene.levelMatchmove", "no match-move solve is selected"))?;
        let cs = seq.camera_solves.iter().find(|c| c.id == mm.solve).ok_or_else(|| bad("scene.levelMatchmove", "camera solve not found"))?;
        cs.fit_ground_plane().map(|(n, _)| n).ok_or_else(|| bad("scene.levelMatchmove", "not enough 3D floor points to estimate a ground plane"))?
    };
    s.edit_sequence_as("Auto-Level Match-Move Ground", None, |seq, _ctx, _st| {
        if let Some(mm) = seq.matchmove.as_mut() {
            let _ = mm.level_to_normal(normal);
        }
        Ok(())
    })?;
    matchmove_json(s)
}

fn bake_matchmove_cmd(s: &mut Session, p: &Value) -> Result<Value> {
    let at = p.get("time").and_then(Value::as_i64).map(filmcraft_time::Tick).unwrap_or_else(|| s.playhead());
    let disable = p.get("disableMatchmove").and_then(Value::as_bool).unwrap_or(true);
    let cam = {
        let seq = s.active_sequence().ok_or(EngineError::NoSequence)?;
        seq.matchmove_camera_at(at).ok_or_else(|| bad("scene.bakeMatchmoveFrame", "no active match-move camera at the playhead"))?
    };
    s.edit_sequence_as("Bake Match-Move Camera", None, |seq, _ctx, _st| {
        seq.scene_camera = Some(cam);
        if disable && let Some(mm) = seq.matchmove.as_mut() {
            mm.enabled = false;
        }
        Ok(())
    })?;
    scene_camera_json(s)
}

fn export_matchmove_json(s: &Session) -> Result<Value> {
    let seq = s.active_sequence().ok_or(EngineError::NoSequence)?;
    let mm = seq.matchmove.filter(|m| m.enabled).ok_or_else(|| bad("scene.exportMatchmove", "no active match-move solve"))?;
    let cs = seq.camera_solves.iter().find(|c| c.id == mm.solve).ok_or_else(|| bad("scene.exportMatchmove", "camera solve not found"))?;
    let frames_eval = seq.evaluated_matchmove_frames().unwrap_or_default();
    let fh = seq.settings.height.max(1) as f32;
    let frames: Vec<Value> = frames_eval
        .into_iter()
        .map(|f| {
            json!({
                "time": f.timeline.0,
                "seconds": f.timeline.seconds(),
                "position": f.camera.position,
                "yaw": f.camera.yaw,
                "pitch": f.camera.pitch,
                "roll": f.camera.roll,
                "fov": f.camera.fov,
                "focalPx": f.camera.focal(fh),
                "rmsPx": f.rms_px,
                "confidence": f.confidence,
                "lost": f.lost,
            })
        })
        .collect();
    let points: Vec<Value> = cs
        .points
        .iter()
        .map(|pt| {
            let w = mm.apply_to_point(pt.position);
            json!({
                "position": w,
                "rmsPx": pt.rms_px,
                "observations": pt.observations,
            })
        })
        .collect();
    Ok(json!({
        "solve": cs.id,
        "name": cs.name,
        "width": seq.settings.width,
        "height": seq.settings.height,
        "fps": seq.settings.frame_rate.as_f64(),
        "rotationOnly": cs.rotation_only,
        "scalePx": mm.scale_px,
        "yawTrim": mm.yaw_trim,
        "pitchTrim": mm.pitch_trim,
        "rollTrim": mm.roll_trim,
        "offset": mm.offset,
        "frames": frames,
        "points": points,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Session;
    use filmcraft_time::{TICKS_PER_SECOND, Tick};

    #[test]
    fn light_keyframes_interpolate_and_persist_roundtrip() {
        let mut s = Session::default();
        s.execute("file.newSequence", json!({ "name": "s", "fps": 24, "width": 320, "height": 180, "video": 1, "audio": 0 })).unwrap();

        let added = s.execute("lights.set", json!({ "id": 0, "name": "Key Spot", "kind": "spot" })).unwrap();
        let id = added["id"].as_u64().unwrap();

        // Enable keyframing at t = 0: initial keyframe stamped with default properties.
        s.set_playhead(Tick::ZERO);
        let toggled = s.execute("lights.toggleAnimation", json!({ "id": id })).unwrap();
        let l0 = toggled["lights"].as_array().unwrap().iter().find(|l| l["id"].as_u64() == Some(id)).unwrap();
        assert_eq!(l0["animated"].as_bool(), Some(true));
        assert_eq!(l0["hasKeyframe"].as_bool(), Some(true));
        assert_eq!(l0["keyframes"].as_array().unwrap().len(), 1);

        // Set explicit values at t = 0.
        s.execute(
            "lights.set",
            json!({
                "id": id, "name": "Key Spot", "enabled": true, "kind": "spot",
                "color": [1.0, 0.0, 0.0], "intensity": 1.0,
                "position": [0.0, 0.0, 600.0], "angle": 100.0, "elevation": 20.0,
                "range": 800.0, "cone": 20.0, "softness": 0.2,
                "time": 0
            }),
        )
        .unwrap();

        // Set Linear curve so midpoint is exact 50%.
        s.execute("lights.setInterpolation", json!({ "id": id, "interpolation": "linear" })).unwrap();

        // Stamp second keyframe at t = 1s.
        s.execute(
            "lights.set",
            json!({
                "id": id, "name": "Key Spot", "enabled": true, "kind": "spot",
                "color": [0.0, 1.0, 0.5], "intensity": 3.0,
                "position": [200.0, -100.0, 1000.0], "angle": 160.0, "elevation": 60.0,
                "range": 1600.0, "cone": 50.0, "softness": 0.8,
                "time": TICKS_PER_SECOND
            }),
        )
        .unwrap();

        // Evaluate at midpoint t = 0.5s via `lights.list`.
        let mid = s.execute("lights.list", json!({ "time": TICKS_PER_SECOND / 2 })).unwrap();
        let lm = mid["lights"].as_array().unwrap().iter().find(|l| l["id"].as_u64() == Some(id)).unwrap();
        assert_eq!(lm["hasKeyframe"].as_bool(), Some(false));
        assert_eq!(lm["keyframes"].as_array().unwrap().len(), 2);
        assert!((lm["intensity"].as_f64().unwrap() - 2.0).abs() < 1e-4);
        assert!((lm["color"][0].as_f64().unwrap() - 0.5).abs() < 1e-4);
        assert!((lm["color"][1].as_f64().unwrap() - 0.5).abs() < 1e-4);
        assert!((lm["position"][0].as_f64().unwrap() - 100.0).abs() < 1e-3);
        assert!((lm["angle"].as_f64().unwrap() - 130.0).abs() < 1e-3);
        assert!((lm["cone"].as_f64().unwrap() - 35.0).abs() < 1e-3);
        assert!((lm["softness"].as_f64().unwrap() - 0.5).abs() < 1e-3);

        // JSON persistence round-trip preserves keyframe tracks.
        let saved = serde_json::to_string(s.active_sequence().unwrap()).unwrap();
        let loaded: filmcraft_project::Sequence = serde_json::from_str(&saved).unwrap();
        let eval_mid = loaded.scene_lighting.evaluated_at(Tick(TICKS_PER_SECOND / 2));
        assert!((eval_mid.lights[0].intensity - 2.0).abs() < 1e-4);
        assert!((eval_mid.lights[0].cone - 35.0).abs() < 1e-3);
    }
}

