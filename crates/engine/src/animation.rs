//! Phone-friendly, typed-by-definition animation lanes for every effect and mask parameter.
use filmcraft_project::{ClipId, Param, ParamDef, ParamValue, TrackItem, Interpolation};
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
            id:"animation.curve",label:"Animation Curve Preset",menu:&[],shortcut:None,params:r#"{"clip":id,"effect":index,"mask":index?,"param":str,"mediaTime":ticks,"preset":"linear|hold|easeIn|easeOut|easeInOut|strong"}"#,enabled:has_seq,journal:true,
            run:curve,
        }
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
