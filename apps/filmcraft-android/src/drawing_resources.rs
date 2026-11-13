//! Portable colour palettes and validated brush-preset resources.
use crate::MobileError;
use serde_json::{json,Value};
fn invalid(message: &str) -> MobileError {MobileError::Command(message.into())}
#[derive(Clone,Debug,uniffi::Record)]
pub struct DrawingPaletteColor {pub name:String,pub red:u8,pub green:u8,pub blue:u8}
/// GIMP palette text, also readable by Krita's palette importer. Does not claim KPL support.
#[uniffi::export]
pub fn drawing_import_palette(text:String) -> Result<Vec<DrawingPaletteColor>,MobileError> {
    if text.len()>262_144 || !text.lines().next().is_some_and(|s|s.trim_start_matches('\u{feff}').trim()=="GIMP Palette") {return Err(invalid("Expected a GIMP .gpl palette"));}
    let mut colors=Vec::new();
    for line in text.lines().skip(1) {
        let line=line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("Name:") || line.starts_with("Columns:") {continue;}
        let mut fields=line.split_whitespace();
        let mut next=||fields.next().and_then(|s|s.parse::<u8>().ok()).ok_or_else(||invalid("Invalid palette RGB value"));
        let red=next()?;let green=next()?;let blue=next()?;
        let name=fields.collect::<Vec<_>>().join(" ");
        if name.len()>320 || colors.len()>=256 {return Err(invalid("Palette supports at most 256 colours and 320-byte names"));}
        colors.push(DrawingPaletteColor {name,red,green,blue});
    }
    if colors.is_empty() {return Err(invalid("Palette contains no colours"));}Ok(colors)
}
#[uniffi::export]
pub fn drawing_export_palette(name:String,colors:Vec<DrawingPaletteColor>) -> Result<String,MobileError> {
    if colors.is_empty() || colors.len()>256 || name.len()>320 {return Err(invalid("Invalid palette size or name"));}
    let mut out=format!("GIMP Palette\nName: {}\nColumns: 8\n#\n",name.replace(['\n','\r']," "));
    for color in colors {
        if color.name.len()>320 {return Err(invalid("Palette colour name too long"));}
        out.push_str(&format!("{:3} {:3} {:3}\t{}\n",color.red,color.green,color.blue,color.name.replace(['\n','\r']," ")));
    }Ok(out)
}
#[derive(Clone,Debug,uniffi::Record)]
pub struct DrawingBrushPreset {
    pub name:String,pub brush:String,pub size:f32,pub opacity:f32,pub stabilization:f32,pub pressure_curve:f32,pub use_pressure:bool,pub symmetry:u32,
}
fn validate(p:&DrawingBrushPreset)->Result<(),MobileError> {
    if p.name.trim().is_empty() || p.name.len()>320 || p.brush.trim().is_empty() || p.brush.len()>320 || !p.size.is_finite() || !(1.0..=100.0).contains(&p.size) || !p.opacity.is_finite() || !(0.0..=1.0).contains(&p.opacity) || !p.stabilization.is_finite() || !(0.0..=100.0).contains(&p.stabilization) || !p.pressure_curve.is_finite() || !(0.25..=4.0).contains(&p.pressure_curve) || p.symmetry>5 {return Err(invalid("Invalid brush preset settings"));}Ok(())
}
#[uniffi::export]
pub fn drawing_encode_brush_presets(presets:Vec<DrawingBrushPreset>)->Result<String,MobileError> {
    if presets.len()>128 {return Err(invalid("At most 128 brush presets are supported"));}
    let mut values=Vec::new();
    for p in presets {validate(&p)?;values.push(json!({"name":p.name,"brush":p.brush,"size":p.size,"opacity":p.opacity,"stabilization":p.stabilization,"pressure_curve":p.pressure_curve,"use_pressure":p.use_pressure,"symmetry":p.symmetry}));}
    serde_json::to_string(&json!({"format":"filmcraft-brushes","version":1,"presets":values})).map_err(|_|invalid("Could not encode presets"))
}
#[uniffi::export]
pub fn drawing_decode_brush_presets(text:String)->Result<Vec<DrawingBrushPreset>,MobileError> {
    if text.len()>262_144 {return Err(invalid("Brush preset file too large"));}
    let root:Value=serde_json::from_str(&text).map_err(|_|invalid("Invalid brush preset JSON"))?;
    if root["format"].as_str()!=Some("filmcraft-brushes") || root["version"].as_u64()!=Some(1) {return Err(invalid("Unsupported brush preset format"));}
    let values=root["presets"].as_array().ok_or_else(||invalid("Missing presets"))?;
    if values.len()>128 {return Err(invalid("At most 128 brush presets are supported"));}
    let mut out=Vec::new();
    for v in values {
        let string=|key:&str|v[key].as_str().map(str::to_owned).ok_or_else(||invalid("Missing preset text"));
        let number=|key:&str|v[key].as_f64().map(|n|n as f32).ok_or_else(||invalid("Missing preset number"));
        let symmetry=v["symmetry"].as_u64().filter(|n|*n<=5).ok_or_else(||invalid("Invalid symmetry"))? as u32;
        let p=DrawingBrushPreset {name:string("name")?,brush:string("brush")?,size:number("size")?,opacity:number("opacity")?,stabilization:number("stabilization")?,pressure_curve:number("pressure_curve")?,use_pressure:v["use_pressure"].as_bool().ok_or_else(||invalid("Missing pressure setting"))?,symmetry};
        validate(&p)?;out.push(p);
    }Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn palette_roundtrip_and_bounds()->Result<(),MobileError> {
        let colors=vec![DrawingPaletteColor {name:"Deep blue".into(),red:1,green:2,blue:200}];
        let text=drawing_export_palette("Test".into(),colors)?;let out=drawing_import_palette(text)?;
        assert_eq!(out[0].name,"Deep blue");assert_eq!(out[0].blue,200);
        assert!(drawing_import_palette("GIMP Palette\n256 0 0 bad".into()).is_err());Ok(())
    }
    #[test] fn brush_roundtrip()->Result<(),MobileError> {
        let presets=vec![DrawingBrushPreset {name:"Ink".into(),brush:"Pen".into(),size:8.0,opacity:0.8,stabilization:20.0,pressure_curve:1.0,use_pressure:true,symmetry:0}];
        let result=drawing_decode_brush_presets(drawing_encode_brush_presets(presets)?)?;assert_eq!(result[0].size,8.0);Ok(())
    }
}
