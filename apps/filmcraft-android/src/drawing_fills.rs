//! Multi-stop gradients, deterministic procedural fills and portable gradient resources.
use crate::{drawing_raster::{validate,byte},MobileError};
use serde_json::{json,Value};
#[derive(Clone,Debug,uniffi::Record)]
pub struct DrawingGradientStop {pub position:f32,pub rgba:Vec<u8>}
#[derive(Clone,Debug,uniffi::Record)]
pub struct DrawingGradientPreset {pub name:String,pub stops:Vec<DrawingGradientStop>,pub kind:u32,pub repeat:u32,pub reverse:bool,pub interpolation:u32,pub linear_light:bool}
fn invalid()->MobileError {MobileError::Command("Invalid gradient or fill settings".into())}
fn check(p:&DrawingGradientPreset)->Result<(),MobileError> {
    if p.name.len()>320 || p.kind>6 || p.repeat>2 || p.interpolation>3 || p.stops.len()<2 || p.stops.len()>32 || p.stops.iter().any(|s|!s.position.is_finite() || !(0.0..=1.0).contains(&s.position) || s.rgba.len()!=4) || p.stops.windows(2).any(|s|s[0].position>s[1].position) {return Err(invalid());}Ok(())
}
fn linear(v:f32)->f32 {if v<=0.04045 {v/12.92} else {((v+0.055)/1.055).powf(2.4)}}
fn encoded(v:f32)->f32 {if v<=0.0031308 {v*12.92} else {1.055*v.powf(1.0/2.4)-0.055}}
fn composite(p:&mut [u8],color:[f32;4],opacity:f32) {
    let sa=color[3]*opacity;let da=f32::from(p[3])/255.0;let alpha=sa+da*(1.0-sa);
    for c in 0..3 {p[c]=if alpha>0.0 {byte((color[c]*sa+f32::from(p[c])/255.0*da*(1.0-sa))/alpha*255.0)} else {0};}p[3]=byte(alpha*255.0);
}
fn gradient_color(p:&DrawingGradientPreset,t:f32)->[f32;4] {
    let index=p.stops.partition_point(|s|s.position<=t);
    let first=&p.stops[index.saturating_sub(1)];let last=&p.stops[index.min(p.stops.len()-1)];
    let mut weight=if last.position-first.position>0.000001 {((t-first.position)/(last.position-first.position)).clamp(0.0,1.0)} else {0.0};
    weight=match p.interpolation {1=>weight*weight*(3.0-2.0*weight),2=>(1.0-(weight*std::f32::consts::PI).cos())*0.5,3=>0.0,_=>weight};
    let a=f32::from(first.rgba[3])/255.0;let b=f32::from(last.rgba[3])/255.0;let alpha=a*(1.0-weight)+b*weight;let mut color=[0.0,0.0,0.0,alpha];
    for c in 0..3 {
        let mut x=f32::from(first.rgba[c])/255.0;let mut y=f32::from(last.rgba[c])/255.0;if p.linear_light {x=linear(x);y=linear(y);}
        let value=if alpha>0.0 {(x*a*(1.0-weight)+y*b*weight)/alpha} else {0.0};color[c]=if p.linear_light {encoded(value)} else {value};
    }color
}
#[uniffi::export]
pub fn drawing_multistop_gradient(mut pixels:Vec<u8>,width:u32,height:u32,start:Vec<f32>,end:Vec<f32>,preset:DrawingGradientPreset,opacity:f32)->Result<Vec<u8>,MobileError> {
    validate(&pixels,width,height)?;check(&preset)?;
    if start.len()!=2 || end.len()!=2 || start.iter().chain(end.iter()).any(|v|!v.is_finite()) || !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {return Err(invalid());}
    let dx=end[0]-start[0];let dy=end[1]-start[1];let length=dx.hypot(dy);if !length.is_finite() || length<0.001 {return Err(invalid());}let ux=dx/length;let uy=dy/length;
    for (i,pixel) in pixels.chunks_exact_mut(4).enumerate() {
        let x=(i%width as usize) as f32+0.5-start[0];let y=(i/width as usize) as f32+0.5-start[1];let a=x*ux+y*uy;let b=-x*uy+y*ux;
        let phase=(y.atan2(x)-dy.atan2(dx)).rem_euclid(std::f32::consts::TAU)/std::f32::consts::TAU;
        let raw=match preset.kind {0=>a/length,1=>x.hypot(y)/length,2=>(a.abs()+b.abs())/length,3=>phase,4=>a.abs().max(b.abs())/length,5=>a.abs()/length,_=>x.hypot(y)/length+phase};
        let t=match preset.repeat {1=>raw.rem_euclid(1.0),2=>1.0-(raw.rem_euclid(2.0)-1.0).abs(),_=>raw.clamp(0.0,1.0)};
        let color=gradient_color(&preset,if preset.reverse {1.0-t} else {t});composite(pixel,color,opacity);
    }Ok(pixels)
}
#[uniffi::export]
pub fn drawing_pattern_fill(mut pixels:Vec<u8>,width:u32,height:u32,kind:u32,scale:f32,angle:f32,density:f32,first:Vec<u8>,last:Vec<u8>,opacity:f32,seed:u32)->Result<Vec<u8>,MobileError> {
    validate(&pixels,width,height)?;
    if kind>7 || !scale.is_finite() || !(2.0..=512.0).contains(&scale) || !angle.is_finite() || !density.is_finite() || !(0.02..=0.98).contains(&density) || first.len()!=4 || last.len()!=4 || !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {return Err(invalid());}
    let radians=angle.to_radians();let cos=radians.cos();let sin=radians.sin();
    for (i,pixel) in pixels.chunks_exact_mut(4).enumerate() {
        let x=(i%width as usize) as f32+0.5;let y=(i/width as usize) as f32+0.5;let u=(x*cos+y*sin)/scale;let v=(-x*sin+y*cos)/scale;
        let fx=u.rem_euclid(1.0);let fy=v.rem_euclid(1.0);
        let ink=match kind {
            0=>(u.floor() as i64+v.floor() as i64).rem_euclid(2)==0,
            1=>fx<density,
            2=>(fx-0.5).hypot(fy-0.5)<density*0.5,
            3=>fx<density || fy<density,
            4=>{let row=v.floor() as i64;let shifted=(u+if row.rem_euclid(2)==0 {0.0} else {0.5}).rem_euclid(1.0);fy<density*0.2 || shifted<density*0.2},
            5=>(fx-fy).abs()<density*0.5 || (fx+fy-1.0).abs()<density*0.5,
            6=>fx+fy<density*2.0,
            _=>{let mut hash=(u.floor() as i64 as u32).wrapping_mul(0x85ebca6b)^(v.floor() as i64 as u32).wrapping_mul(0xc2b2ae35)^seed;hash^=hash>>16;hash=hash.wrapping_mul(0x7feb352d);(hash&65535) as f32/65535.0<density},
        };
        let color=if ink {&first} else {&last};composite(pixel,[f32::from(color[0])/255.0,f32::from(color[1])/255.0,f32::from(color[2])/255.0,f32::from(color[3])/255.0],opacity);
    }Ok(pixels)
}
#[uniffi::export]
pub fn drawing_encode_gradients(presets:Vec<DrawingGradientPreset>)->Result<String,MobileError> {
    if presets.len()>64 {return Err(invalid());}let mut list=Vec::new();
    for p in presets {check(&p)?;list.push(json!({"name":p.name,"kind":p.kind,"repeat":p.repeat,"reverse":p.reverse,"interpolation":p.interpolation,"linear_light":p.linear_light,"stops":p.stops.iter().map(|s|json!({"position":s.position,"rgba":s.rgba})).collect::<Vec<_>>()}));}
    serde_json::to_string(&json!({"format":"filmcraft-gradients","version":1,"presets":list})).map_err(|_|invalid())
}
#[uniffi::export]
pub fn drawing_decode_gradients(text:String)->Result<Vec<DrawingGradientPreset>,MobileError> {
    if text.len()>262_144 {return Err(invalid());}let root:Value=serde_json::from_str(&text).map_err(|_|invalid())?;
    if root["format"].as_str()!=Some("filmcraft-gradients") || root["version"].as_u64()!=Some(1) {return Err(invalid());}
    let list=root["presets"].as_array().ok_or_else(invalid)?;if list.len()>64 {return Err(invalid());}let mut output=Vec::new();
    for p in list {
        let integer=|key:&str|p[key].as_u64().and_then(|n|u32::try_from(n).ok()).ok_or_else(invalid);
        let boolean=|key:&str|p[key].as_bool().ok_or_else(invalid);
        let values=p["stops"].as_array().ok_or_else(invalid)?;if values.len()>32 {return Err(invalid());}let mut stops=Vec::new();
        for s in values {let rgba=s["rgba"].as_array().ok_or_else(invalid)?.iter().map(|v|v.as_u64().and_then(|v|u8::try_from(v).ok()).ok_or_else(invalid)).collect::<Result<Vec<_>,_>>()?;stops.push(DrawingGradientStop {position:s["position"].as_f64().ok_or_else(invalid)? as f32,rgba});}
        let preset=DrawingGradientPreset {name:p["name"].as_str().ok_or_else(invalid)?.into(),stops,kind:integer("kind")?,repeat:integer("repeat")?,reverse:boolean("reverse")?,interpolation:integer("interpolation")?,linear_light:boolean("linear_light")?};check(&preset)?;output.push(preset);
    }Ok(output)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn preset()->DrawingGradientPreset {DrawingGradientPreset {name:"Test".into(),stops:vec![DrawingGradientStop {position:0.0,rgba:vec![255,0,0,255]},DrawingGradientStop {position:1.0,rgba:vec![0,0,255,0]}],kind:0,repeat:0,reverse:false,interpolation:0,linear_light:false}}
    #[test] fn transparent_stop_does_not_leak_blue() {let c=gradient_color(&preset(),0.5);assert_eq!(c,[1.0,0.0,0.0,0.5]);}
    #[test] fn gradient_resource_roundtrip()->Result<(),MobileError> {let list=drawing_decode_gradients(drawing_encode_gradients(vec![preset()])?)?;assert_eq!(list[0].stops[1].rgba,vec![0,0,255,0]);Ok(())}
    #[test] fn pattern_is_deterministic()->Result<(),MobileError> {let render=||drawing_pattern_fill(vec![0;64],4,4,7,2.0,0.0,0.5,vec![255;4],vec![0;4],1.0,123);assert_eq!(render()?,render()?);Ok(())}
}

/// Bucket fill with an independent reference image, optional global matching, region growth and
/// grayscale selection restriction. Painting uses source-over, not replacement of destination alpha.
#[uniffi::export]
pub fn drawing_fill_region(mut pixels:Vec<u8>,reference:Vec<u8>,width:u32,height:u32,x:u32,y:u32,color:Vec<u8>,tolerance:u8,contiguous:bool,grow:u32,selection:Vec<u8>)->Result<Vec<u8>,MobileError> {
    validate(&pixels,width,height)?;validate(&reference,width,height)?;
    if color.len()!=4 || grow>64 || (!selection.is_empty() && selection.len()!=pixels.len()/4) {return Err(invalid());}
    let mut region=crate::drawing_raster::drawing_color_selection(reference,width,height,x,y,tolerance,contiguous)?;
    if grow>0 {region=crate::drawing_raster::drawing_selection_mask(region,width,height,"grow".into(),grow)?;}
    let rgba=[f32::from(color[0])/255.0,f32::from(color[1])/255.0,f32::from(color[2])/255.0,f32::from(color[3])/255.0];
    for (i,pixel) in pixels.chunks_exact_mut(4).enumerate() {
        let selected=if selection.is_empty() {1.0} else {f32::from(selection[i])/255.0};
        composite(pixel,rgba,selected*f32::from(region[i])/255.0);
    }Ok(pixels)
}
#[cfg(test)]
mod bucket_tests {
    use super::*;
    #[test] fn independent_reference_restricts_fill()->Result<(),MobileError> {
        let out=drawing_fill_region(vec![0;12],vec![0,0,0,255,255,255,255,255,0,0,0,255],3,1,0,0,vec![255,0,0,255],0,true,0,vec![])?;
        assert_eq!(out,vec![255,0,0,255,0,0,0,0,0,0,0,0]);Ok(())
    }
    #[test] fn global_fill_selects_disconnected_matching_regions()->Result<(),MobileError> {
        let out=drawing_fill_region(vec![0;12],vec![0,0,0,255,255,255,255,255,0,0,0,255],3,1,0,0,vec![255,0,0,255],0,false,0,vec![])?;
        assert_eq!(out[3],255);assert_eq!(out[7],0);assert_eq!(out[11],255);Ok(())
    }
}
