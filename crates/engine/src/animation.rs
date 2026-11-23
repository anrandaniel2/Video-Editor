//! Phone-friendly, typed-by-definition animation lanes for every effect, mask, 3D camera, and 3D light parameter.
use filmcraft_project::{ClipId, Interpolation, LightKind, Param, ParamDef, ParamKind, ParamValue, SceneCamera3D, TrackItem};
use filmcraft_time::Tick;
use serde_json::{Value,json};
use crate::{Session, Result, EngineError, commands::{CommandSpec,has_seq,bad}};

fn value(v:&ParamValue)->Value { crate::commands::param_value_to_json(v) }

fn lane(it:&TrackItem, effect:usize, mask:Option<usize>, group:&str, def:&ParamDef, param:&Param, now:Tick)->Value {
    let origin=it.moving_source_time_at(it.start);
    let speed=it.speed.abs()*if it.reverse {-1.0} else {1.0};
    let timeline=|time:Tick| if speed.abs()>1e-9 {it.start.0.saturating_add(((time-origin).0 as f64/speed).round() as i64)} else {it.start.0};
    json!({
        "effect":effect,"mask":mask,"group":group,"param":def.id,"label":def.label,"kind":def.kind,
        "value":value(&param.value_at(now)),"animated":param.is_animated(),
        "keys":param.keyframes.iter().map(|k| json!({"time":k.time.0,"timeline":timeline(k.time),"value":value(&k.value),"interpolation":k.interp.label(),"inInfluence":k.in_influence,"outInfluence":k.out_influence})).collect::<Vec<_>>()
    })
}

pub fn commands()->Vec<CommandSpec> {
    vec![
        CommandSpec {
            id:"animation.list",label:"Animation Lanes",menu:&[],shortcut:None,params:r#"{"clip":id}"#,enabled:has_seq,journal:false,
            run:|s,p| {
                let id=ClipId(p.get("clip").and_then(Value::as_u64).ok_or_else(||bad("animation.list","need clip"))?);
                let q=s.active_sequence().ok_or(EngineError::NoSequence)?;
                let (_,it)=q.find_item(id).ok_or_else(||bad("animation.list","clip does not exist"))?;
                let now=it.source_time_at(s.playhead().clamp(it.start,(it.end()-Tick(1)).max(it.start)));
                let mut lanes=Vec::new();
                for (ei,e) in it.effects.iter().enumerate() {
                    if let Some(def)=e.def() {
                        for d in def.params.iter().filter(|d| d.animatable) {
                            let default=Param::new(d.default.clone());
                            lanes.push(lane(it,ei,None,def.name,d,e.param(d.id).unwrap_or(&default),now));
                        }
                    }
                    for (mi,m) in e.masks.iter().enumerate() {
                        for d in filmcraft_project::mask::mask_param_defs() {
                            if let Some(param)=m.param(d.id) {
                                lanes.push(lane(it,ei,Some(mi),&format!("{} · {}",e.def().map(|d| d.name).unwrap_or(&e.effect),m.name),d,param,now));
                            }
                        }
                    }
                }
                Ok(json!({"clip":id.0,"mediaTime":now.0,"start":it.start.0,"duration":it.duration.0,"lanes":lanes}))
            }
        },
        CommandSpec {
            id:"animation.all",label:"Animate All Properties",menu:&[],shortcut:None,
            params:r#"{"clip":id,"action":"enable|disable|key"}"#,enabled:has_seq,journal:true,run:all,
        },
        CommandSpec {
            id:"animation.curve",label:"Animation Curve Preset",menu:&[],shortcut:None,params:r#"{"clip":id,"effect":index,"mask":index?,"param":str,"mediaTime":ticks,"preset":"linear|hold|easeIn|easeOut|easeInOut|strong"}"#,enabled:has_seq,journal:true,
            run:curve,
        },
        CommandSpec {
            id:"animation.sceneList",label:"3D Scene Animation Lanes",menu:&[],shortcut:None,
            params:r#"{"target":"camera|light:<id>"}"#,enabled:has_seq,journal:false,
            run:scene_list,
        },
        CommandSpec {
            id:"animation.sceneAll",label:"Animate All 3D Scene Properties",menu:&[],shortcut:None,
            params:r#"{"target":"camera|light:<id>","action":"enable|disable|key"}"#,enabled:has_seq,journal:true,
            run:scene_all,
        },
        CommandSpec {
            id:"animation.sceneCurve",label:"3D Scene Curve Preset",menu:&[],shortcut:None,
            params:r#"{"target":"camera|light:<id>","param":str,"mediaTime":ticks,"preset":"linear|hold|easeIn|easeOut|easeInOut|strong"}"#,enabled:has_seq,journal:true,
            run:scene_curve,
        },
        CommandSpec {
            id:"animation.sceneSetParam",label:"Set 3D Scene Parameter",menu:&[],shortcut:None,
            params:r#"{"target":"camera|light:<id>","param":str,"value":any,"time":ticks?,"keyframe":bool?,"merge":str?}"#,enabled:has_seq,journal:true,
            run:scene_set_param,
        },
        CommandSpec {
            id:"animation.sceneSetKeyframe",label:"Add 3D Scene Keyframe",menu:&[],shortcut:None,
            params:r#"{"target":"camera|light:<id>","param":str,"time":ticks?}"#,enabled:has_seq,journal:true,
            run:scene_set_keyframe,
        },
        CommandSpec {
            id:"animation.sceneMoveKeyframes",label:"Move 3D Scene Keyframes",menu:&[],shortcut:None,
            params:r#"{"target":"camera|light:<id>","param":str,"times":[ticks],"delta":ticks}"#,enabled:has_seq,journal:true,
            run:scene_move_keyframes,
        },
        CommandSpec {
            id:"animation.sceneDeleteKeyframe",label:"Delete 3D Scene Keyframe",menu:&[],shortcut:None,
            params:r#"{"target":"camera|light:<id>","param":str,"time":ticks}"#,enabled:has_seq,journal:true,
            run:scene_delete_keyframe,
        },
    ]
}

fn curve(s:&mut Session,p:&Value)->Result<Value> {
    let cmd="animation.curve";
    let clip=ClipId(p.get("clip").and_then(Value::as_u64).ok_or_else(||bad(cmd,"need clip"))?);
    let effect=p.get("effect").and_then(Value::as_u64).ok_or_else(||bad(cmd,"need effect index"))? as usize;
    let param=p.get("param").and_then(Value::as_str).ok_or_else(||bad(cmd,"need param"))?.to_owned();
    let time=Tick(p.get("mediaTime").and_then(Value::as_i64).ok_or_else(||bad(cmd,"need mediaTime"))?);
    let preset=p.get("preset").and_then(Value::as_str).unwrap_or("linear");
    let (a,b,influence)=match preset {
        "linear"=>(Interpolation::Linear,Interpolation::Linear,1.0/3.0),
        "hold"=>(Interpolation::Hold,Interpolation::Linear,1.0/3.0),
        "easeIn"=>(Interpolation::EaseOut,Interpolation::Linear,0.6),
        "easeOut"=>(Interpolation::Linear,Interpolation::EaseIn,0.6),
        "easeInOut"=>(Interpolation::Bezier,Interpolation::Bezier,1.0/3.0),
        "strong"=>(Interpolation::Bezier,Interpolation::Bezier,0.75),
        _=>return Err(bad(cmd,"unknown curve preset")),
    };
    s.edit_sequence("Animation Curve Preset",|q,_,_| {
        let (_,it)=q.find_item_mut(clip).ok_or_else(||bad(cmd,"clip does not exist"))?;
        let e=it.effects.get_mut(effect).ok_or_else(||bad(cmd,"effect does not exist"))?;
        let pm=crate::masks::target_param(e,p,&param).ok_or_else(||bad(cmd,"parameter does not exist"))?;
        let i=pm.keyframes.iter().position(|k| k.time==time).ok_or_else(||bad(cmd,"keyframe does not exist"))?;
        pm.keyframes[i].interp=a; pm.keyframes[i].out_influence=influence;
        if let Some(next)=pm.keyframes.get_mut(i+1) {next.interp=b;next.in_influence=influence;}
        Ok(())
    })?;
    Ok(Value::Null)
}


fn all(s:&mut Session,p:&Value)->Result<Value> {
    let cmd="animation.all";
    let id=ClipId(p.get("clip").and_then(Value::as_u64).ok_or_else(||bad(cmd,"need clip"))?);
    let action=p.get("action").and_then(Value::as_str).unwrap_or("enable");
    if !matches!(action,"enable"|"disable"|"key") {return Err(bad(cmd,"unknown action"));}
    let ph=s.playhead();
    s.edit_sequence("Animate All Properties",|q,_,_| {
        let (_,it)=q.find_item_mut(id).ok_or_else(||bad(cmd,"clip does not exist"))?;
        let now=it.effect_time_at(ph.clamp(it.start,(it.end()-Tick(1)).max(it.start)));
        let update=|param:&mut Param| {
            match action {
                "disable"=> {param.value=param.value_at(now);param.keyframes.clear();},
                "key"=>param.add_keyframe(now),
                _=>if !param.is_animated() {param.add_keyframe(now);},
            }
        };
        for e in &mut it.effects {
            if let Some(def)=e.def() {
                for d in def.params.iter().filter(|d|d.animatable) {
                    update(e.params.entry(d.id.to_string()).or_insert_with(||Param::new(d.default.clone())));
                }
            }
            for m in &mut e.masks {for param in m.params_mut() {update(param);}}
        }
        Ok(())
    })?;
    Ok(Value::Null)
}

#[derive(Clone, Copy)]
enum SceneTarget {
    Camera,
    Light(u64),
}

fn parse_scene_target(p: &Value, cmd: &str) -> Result<SceneTarget> {
    let raw = p.get("target").and_then(Value::as_str).ok_or_else(|| bad(cmd, "need target"))?;
    if raw == "camera" {
        return Ok(SceneTarget::Camera);
    }
    if let Some(rest) = raw.strip_prefix("light:")
        && let Ok(id) = rest.parse::<u64>()
    {
        return Ok(SceneTarget::Light(id));
    }
    Err(bad(cmd, format!("unknown scene target '{raw}'")))
}

fn float_def(id: &'static str, label: &'static str, min: f64, max: f64, soft_min: f64, soft_max: f64, unit: &'static str, decimals: u8, default: f64) -> ParamDef {
    ParamDef {
        id,
        label,
        kind: ParamKind::Float { min, max, soft_min, soft_max, unit, decimals },
        default: ParamValue::Float(default),
        animatable: true,
        group: None,
    }
}

fn camera_param_defs() -> Vec<ParamDef> {
    vec![
        float_def("x", "Position X", -20_000.0, 20_000.0, -2500.0, 2500.0, "px", 0, 0.0),
        float_def("y", "Position Y", -20_000.0, 20_000.0, -2500.0, 2500.0, "px", 0, 0.0),
        float_def("z", "Position Z", -20_000.0, 20_000.0, -1500.0, 3500.0, "px", 0, 0.0),
        ParamDef { id: "yaw", label: "Yaw", kind: ParamKind::Angle, default: ParamValue::Float(0.0), animatable: true, group: None },
        float_def("pitch", "Pitch", -89.0, 89.0, -89.0, 89.0, "°", 1, 0.0),
        ParamDef { id: "roll", label: "Roll", kind: ParamKind::Angle, default: ParamValue::Float(0.0), animatable: true, group: None },
        float_def("fov", "Field of View", 5.0, 170.0, 15.0, 120.0, "°", 1, f64::from(filmcraft_project::CLASSIC_FOV)),
    ]
}

fn light_param_defs(kind: LightKind) -> Vec<ParamDef> {
    let mut defs = Vec::new();
    if kind != LightKind::Directional {
        defs.push(float_def("x", "Position X", -20_000.0, 20_000.0, -2000.0, 3000.0, "px", 0, 0.0));
        defs.push(float_def("y", "Position Y", -20_000.0, 20_000.0, -2000.0, 3000.0, "px", 0, 0.0));
        defs.push(float_def("z", "Position Z", -20_000.0, 20_000.0, -500.0, 4000.0, "px", 0, 480.0));
    }
    if kind != LightKind::Point {
        defs.push(ParamDef { id: "angle", label: "Aim Angle", kind: ParamKind::Angle, default: ParamValue::Float(225.0), animatable: true, group: None });
        defs.push(float_def("elevation", "Elevation", -90.0, 90.0, -90.0, 90.0, "°", 1, 48.0));
    }
    defs.push(float_def("intensity", "Intensity", 0.0, 20.0, 0.0, 4.0, "", 2, 1.0));
    defs.push(ParamDef { id: "color", label: "Color", kind: ParamKind::Color, default: ParamValue::Color([1.0, 1.0, 1.0, 1.0]), animatable: true, group: None });
    if kind == LightKind::Spot {
        defs.push(float_def("cone", "Cone Angle", 1.0, 89.0, 5.0, 85.0, "°", 1, 35.0));
        defs.push(float_def("softness", "Softness", 0.0, 1.0, 0.0, 1.0, "", 2, 0.5));
    }
    if kind != LightKind::Directional {
        defs.push(float_def("range", "Range", 0.0, 20_000.0, 0.0, 4000.0, "px", 0, 0.0));
    }
    defs
}

fn scene_lane_json(group: &str, def: &ParamDef, track: Option<&Param>, current_val: ParamValue) -> Value {
    let animated = track.is_some_and(Param::is_animated);
    let keys = track
        .map(|p| {
            p.keyframes
                .iter()
                .map(|k| {
                    json!({
                        "time": k.time.0,
                        "timeline": k.time.0,
                        "value": value(&k.value),
                        "interpolation": k.interp.label(),
                        "inInfluence": k.in_influence,
                        "outInfluence": k.out_influence,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    json!({
        "effect": 0usize,
        "mask": Value::Null,
        "group": group,
        "param": def.id,
        "label": def.label,
        "kind": def.kind,
        "value": value(&current_val),
        "animated": animated,
        "keys": keys,
    })
}

fn parse_scene_value(def: &ParamDef, v: &Value, cmd: &str) -> Result<ParamValue> {
    match &def.kind {
        ParamKind::Float { min, max, .. } => {
            let x = v.as_f64().ok_or_else(|| bad(cmd, "expected numeric value"))?;
            if !x.is_finite() {
                return Err(bad(cmd, "value must be finite"));
            }
            Ok(ParamValue::Float(x.clamp(*min, *max)))
        }
        ParamKind::Angle => {
            let x = v.as_f64().ok_or_else(|| bad(cmd, "expected angle in degrees"))?;
            if !x.is_finite() {
                return Err(bad(cmd, "angle must be finite"));
            }
            Ok(ParamValue::Float(x))
        }
        ParamKind::Color => {
            let a = v.as_array().ok_or_else(|| bad(cmd, "expected RGB(A) color array"))?;
            if a.len() < 3 {
                return Err(bad(cmd, "color array needs at least 3 channels"));
            }
            let ch = |i: usize, d: f32| a.get(i).and_then(Value::as_f64).map(|x| (x as f32).clamp(0.0, 1.0)).unwrap_or(d);
            Ok(ParamValue::Color([ch(0, 1.0), ch(1, 1.0), ch(2, 1.0), ch(3, 1.0)]))
        }
        _ => Err(bad(cmd, "unsupported scene parameter kind")),
    }
}

fn scene_list(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "animation.sceneList";
    let target = parse_scene_target(p, cmd)?;
    let q = s.active_sequence().ok_or(EngineError::NoSequence)?;
    let now = s.playhead();
    let seq_dur = q.duration().0.max(q.frame_rate().frame_duration().0 * 150);
    let lanes = match target {
        SceneTarget::Camera => {
            let base = q.scene_camera.unwrap_or_default();
            let eval = base.evaluated_with_tracks(&q.scene_camera_tracks, now);
            camera_param_defs()
                .into_iter()
                .map(|d| {
                    let val = eval.static_param_value(d.id).unwrap_or_else(|| d.default.clone());
                    scene_lane_json("3D Camera", &d, q.scene_camera_tracks.get(d.id), val)
                })
                .collect::<Vec<_>>()
        }
        SceneTarget::Light(id) => {
            let light = q
                .scene_lighting
                .lights
                .iter()
                .find(|l| l.id == id)
                .ok_or_else(|| bad(cmd, format!("light {id} does not exist")))?;
            let group = if light.name.trim().is_empty() { format!("Light {id}") } else { light.name.clone() };
            light_param_defs(light.kind)
                .into_iter()
                .map(|d| {
                    let val = light.param_value_at(d.id, now).unwrap_or_else(|| d.default.clone());
                    scene_lane_json(&group, &d, light.tracks.get(d.id), val)
                })
                .collect::<Vec<_>>()
        }
    };
    Ok(json!({
        "target": p.get("target").and_then(Value::as_str).unwrap_or("camera"),
        "mediaTime": now.0,
        "start": 0i64,
        "duration": seq_dur,
        "lanes": lanes,
    }))
}

fn scene_all(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "animation.sceneAll";
    let target = parse_scene_target(p, cmd)?;
    let action = p.get("action").and_then(Value::as_str).unwrap_or("enable");
    if !matches!(action, "enable" | "disable" | "key") {
        return Err(bad(cmd, "unknown action"));
    }
    let now = s.playhead();
    s.edit_sequence("Animate All 3D Scene Properties", |q, _, _| {
        match target {
            SceneTarget::Camera => {
                let eval = q.scene_camera_at(now).unwrap_or_default();
                q.scene_camera = Some(eval);
                if action == "disable" {
                    q.scene_camera_tracks.clear();
                } else {
                    for &id in SceneCamera3D::ANIMATED_PARAMS {
                        let Some(val) = eval.static_param_value(id) else { continue };
                        let entry = q.scene_camera_tracks.entry(id.to_string()).or_insert_with(|| Param::new(val.clone()));
                        entry.value = val.clone();
                        if action == "key" || !entry.is_animated() {
                            entry.put_keyframe(now, val);
                        }
                    }
                }
            }
            SceneTarget::Light(id) => {
                let light = q
                    .scene_lighting
                    .lights
                    .iter_mut()
                    .find(|l| l.id == id)
                    .ok_or_else(|| bad(cmd, format!("light {id} does not exist")))?;
                if action == "disable" {
                    let eval = light.evaluated_at(now);
                    light.color = eval.color;
                    light.intensity = eval.intensity;
                    light.position = eval.position;
                    light.angle = eval.angle;
                    light.elevation = eval.elevation;
                    light.range = eval.range;
                    light.cone = eval.cone;
                    light.softness = eval.softness;
                    light.tracks.clear();
                } else {
                    let interp = light.interpolation_at(now);
                    light.add_keyframe_at(now, interp);
                }
            }
        }
        Ok(())
    })?;
    Ok(Value::Null)
}

fn scene_curve(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "animation.sceneCurve";
    let target = parse_scene_target(p, cmd)?;
    let param = p.get("param").and_then(Value::as_str).ok_or_else(|| bad(cmd, "need param"))?.to_owned();
    let time = Tick(p.get("mediaTime").and_then(Value::as_i64).ok_or_else(|| bad(cmd, "need mediaTime"))?);
    let preset = p.get("preset").and_then(Value::as_str).unwrap_or("linear");
    let (a, b, influence) = match preset {
        "linear" => (Interpolation::Linear, Interpolation::Linear, 1.0 / 3.0),
        "hold" => (Interpolation::Hold, Interpolation::Linear, 1.0 / 3.0),
        "easeIn" => (Interpolation::EaseOut, Interpolation::Linear, 0.6),
        "easeOut" => (Interpolation::Linear, Interpolation::EaseIn, 0.6),
        "easeInOut" => (Interpolation::Bezier, Interpolation::Bezier, 1.0 / 3.0),
        "strong" => (Interpolation::Bezier, Interpolation::Bezier, 0.75),
        _ => return Err(bad(cmd, "unknown curve preset")),
    };
    s.edit_sequence("3D Scene Curve Preset", |q, _, _| {
        let pm = match target {
            SceneTarget::Camera => q.scene_camera_tracks.get_mut(&param).ok_or_else(|| bad(cmd, "camera track not animated"))?,
            SceneTarget::Light(id) => q
                .scene_lighting
                .lights
                .iter_mut()
                .find(|l| l.id == id)
                .ok_or_else(|| bad(cmd, "light does not exist"))?
                .tracks
                .get_mut(&param)
                .ok_or_else(|| bad(cmd, "light track not animated"))?,
        };
        let i = pm.keyframes.iter().position(|k| k.time == time).ok_or_else(|| bad(cmd, "keyframe does not exist"))?;
        pm.keyframes[i].interp = a;
        pm.keyframes[i].out_influence = influence;
        if let Some(next) = pm.keyframes.get_mut(i + 1) {
            next.interp = b;
            next.in_influence = influence;
        }
        Ok(())
    })?;
    Ok(Value::Null)
}

fn scene_set_param(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "animation.sceneSetParam";
    let target = parse_scene_target(p, cmd)?;
    let param_id = p.get("param").and_then(Value::as_str).ok_or_else(|| bad(cmd, "need param"))?.to_owned();
    let raw_val = p.get("value").ok_or_else(|| bad(cmd, "need value"))?;
    let at = p.get("time").and_then(Value::as_i64).map(Tick).unwrap_or_else(|| s.playhead());
    let force_key = p.get("keyframe").and_then(Value::as_bool).unwrap_or(false);
    let merge = p.get("merge").and_then(Value::as_str).map(str::to_owned);
    s.edit_sequence_as("Set 3D Scene Parameter", merge.as_deref(), |q, _, _| {
        match target {
            SceneTarget::Camera => {
                let defs = camera_param_defs();
                let def = defs.iter().find(|d| d.id == param_id).ok_or_else(|| bad(cmd, "unknown camera parameter"))?;
                let val = parse_scene_value(def, raw_val, cmd)?;
                let mut cam = q.scene_camera.unwrap_or_default();
                let _ = cam.set_static_param(&param_id, &val);
                q.scene_camera = Some(cam);
                let already_animated = q.scene_camera_tracks.get(&param_id).is_some_and(Param::is_animated);
                if force_key || already_animated {
                    let entry = q.scene_camera_tracks.entry(param_id.clone()).or_insert_with(|| Param::new(val.clone()));
                    entry.value = val.clone();
                    entry.put_keyframe(at, val);
                }
            }
            SceneTarget::Light(id) => {
                let light = q
                    .scene_lighting
                    .lights
                    .iter_mut()
                    .find(|l| l.id == id)
                    .ok_or_else(|| bad(cmd, "light does not exist"))?;
                let defs = light_param_defs(light.kind);
                let def = defs.iter().find(|d| d.id == param_id).ok_or_else(|| bad(cmd, "unknown light parameter"))?;
                let val = parse_scene_value(def, raw_val, cmd)?;
                let _ = light.set_static_param(&param_id, &val);
                let already_animated = light.tracks.get(&param_id).is_some_and(Param::is_animated);
                if force_key || already_animated {
                    let entry = light.tracks.entry(param_id.clone()).or_insert_with(|| Param::new(val.clone()));
                    entry.value = val.clone();
                    entry.put_keyframe(at, val);
                }
            }
        }
        Ok(())
    })?;
    Ok(Value::Null)
}

fn scene_set_keyframe(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "animation.sceneSetKeyframe";
    let target = parse_scene_target(p, cmd)?;
    let param_id = p.get("param").and_then(Value::as_str).ok_or_else(|| bad(cmd, "need param"))?.to_owned();
    let at = p.get("time").and_then(Value::as_i64).map(Tick).unwrap_or_else(|| s.playhead());
    s.edit_sequence("Add 3D Scene Keyframe", |q, _, _| {
        match target {
            SceneTarget::Camera => {
                let eval = q.scene_camera_at(at).unwrap_or_default();
                q.scene_camera = Some(eval);
                let val = eval.static_param_value(&param_id).ok_or_else(|| bad(cmd, "unknown camera parameter"))?;
                let entry = q.scene_camera_tracks.entry(param_id.clone()).or_insert_with(|| Param::new(val.clone()));
                entry.value = val.clone();
                entry.put_keyframe(at, val);
            }
            SceneTarget::Light(id) => {
                let light = q
                    .scene_lighting
                    .lights
                    .iter_mut()
                    .find(|l| l.id == id)
                    .ok_or_else(|| bad(cmd, "light does not exist"))?;
                let val = light.param_value_at(&param_id, at).ok_or_else(|| bad(cmd, "unknown light parameter"))?;
                let entry = light.tracks.entry(param_id.clone()).or_insert_with(|| Param::new(val.clone()));
                entry.value = val.clone();
                entry.put_keyframe(at, val);
            }
        }
        Ok(())
    })?;
    Ok(Value::Null)
}

fn move_track_keyframes(track: &mut Param, times: &[Tick], delta: i64) {
    let mut moved = Vec::new();
    track.keyframes.retain(|k| {
        if times.contains(&k.time) {
            let mut nk = k.clone();
            nk.time = Tick(k.time.0.saturating_add(delta).max(0));
            moved.push(nk);
            false
        } else {
            true
        }
    });
    for nk in moved {
        track.keyframes.retain(|k| k.time != nk.time);
        track.keyframes.push(nk);
    }
    track.keyframes.sort_by_key(|k| k.time);
}

fn scene_move_keyframes(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "animation.sceneMoveKeyframes";
    let target = parse_scene_target(p, cmd)?;
    let param_id = p.get("param").and_then(Value::as_str).ok_or_else(|| bad(cmd, "need param"))?.to_owned();
    let delta = p.get("delta").and_then(Value::as_i64).ok_or_else(|| bad(cmd, "need delta"))?;
    let times: Vec<Tick> = p
        .get("times")
        .and_then(Value::as_array)
        .ok_or_else(|| bad(cmd, "need times"))?
        .iter()
        .filter_map(|v| v.as_i64().map(Tick))
        .collect();
    s.edit_sequence("Move 3D Scene Keyframes", |q, _, _| {
        let pm = match target {
            SceneTarget::Camera => q.scene_camera_tracks.get_mut(&param_id).ok_or_else(|| bad(cmd, "camera track not animated"))?,
            SceneTarget::Light(id) => q
                .scene_lighting
                .lights
                .iter_mut()
                .find(|l| l.id == id)
                .ok_or_else(|| bad(cmd, "light does not exist"))?
                .tracks
                .get_mut(&param_id)
                .ok_or_else(|| bad(cmd, "light track not animated"))?,
        };
        move_track_keyframes(pm, &times, delta);
        Ok(())
    })?;
    Ok(Value::Null)
}

fn scene_delete_keyframe(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "animation.sceneDeleteKeyframe";
    let target = parse_scene_target(p, cmd)?;
    let param_id = p.get("param").and_then(Value::as_str).ok_or_else(|| bad(cmd, "need param"))?.to_owned();
    let at = Tick(p.get("time").and_then(Value::as_i64).ok_or_else(|| bad(cmd, "need time"))?);
    s.edit_sequence("Delete 3D Scene Keyframe", |q, _, _| {
        match target {
            SceneTarget::Camera => {
                let eval = q.scene_camera_at(at).unwrap_or_default();
                if let Some(pm) = q.scene_camera_tracks.get_mut(&param_id) {
                    pm.remove_keyframe_at(at);
                }
                q.scene_camera_tracks.retain(|_, pm| pm.is_animated());
                if !q.scene_camera_tracks.contains_key(&param_id) {
                    q.scene_camera = Some(eval);
                }
            }
            SceneTarget::Light(id) => {
                let light = q
                    .scene_lighting
                    .lights
                    .iter_mut()
                    .find(|l| l.id == id)
                    .ok_or_else(|| bad(cmd, "light does not exist"))?;
                let eval = light.evaluated_at(at);
                if let Some(pm) = light.tracks.get_mut(&param_id) {
                    pm.remove_keyframe_at(at);
                }
                light.tracks.retain(|_, pm| pm.is_animated());
                if !light.tracks.contains_key(&param_id)
                    && let Some(v) = eval.static_param_value(&param_id)
                {
                    let _ = light.set_static_param(&param_id, &v);
                }
            }
        }
        Ok(())
    })?;
    Ok(Value::Null)
}
