//! Native dab scheduling and real GPU brush coverage. No software painter is silently substituted.
use crate::{drawing::{DrawingCoverage,DrawingStrokePoint},drawing_raster::{validate,byte},MobileError};
#[derive(Clone,Debug,uniffi::Record)]
pub struct DrawingNativeBrush {
    pub tip:u32,pub spacing:f32,pub hardness:f32,pub aspect:f32,pub rotation:f32,pub scatter:f32,
    pub size_jitter:f32,pub rotation_jitter:f32,pub flow:f32,pub seed:u32,pub follow_direction:bool,pub symmetry:u32,
}
fn invalid()->MobileError {MobileError::Command("Invalid native brush settings or stroke bounds".into())}
fn validate_brush(s:&DrawingNativeBrush)->Result<(),MobileError> {
    if s.tip>7 || s.symmetry>5 || [s.spacing,s.hardness,s.aspect,s.rotation,s.scatter,s.size_jitter,s.rotation_jitter,s.flow].iter().any(|v|!v.is_finite()) || !(0.02..=4.0).contains(&s.spacing) || !(0.0..=1.0).contains(&s.hardness) || !(0.1..=1.0).contains(&s.aspect) || s.rotation.abs()>180.0 || !(0.0..=2.0).contains(&s.scatter) || !(0.0..=1.0).contains(&s.size_jitter) || !(0.0..=180.0).contains(&s.rotation_jitter) || !(0.0..=1.0).contains(&s.flow) {return Err(invalid());}Ok(())
}
fn random(seed:&mut u32)->f32 {*seed^=*seed<<13;*seed^=*seed>>17;*seed^=*seed<<5;(*seed>>8) as f32/16_777_216.0}
fn schedule(points:&[DrawingStrokePoint],settings:&DrawingNativeBrush,width:u32,height:u32)->Result<Vec<[f32;9]>,MobileError> {
    validate_brush(settings)?;
    if points.is_empty() || points.len()>8192 || points.iter().any(|p|!p.x.is_finite() || !p.y.is_finite() || !p.radius.is_finite() || !(0.01..=2048.0).contains(&p.radius) || p.x.abs()>16_777_216.0 || p.y.abs()>16_777_216.0) {return Err(invalid());}
    let mut seed=if settings.seed==0 {0x9e3779b9} else {settings.seed};let mut dabs=Vec::new();
    let mut emit=|x:f32,y:f32,radius:f32,direction:f32|->Result<(),MobileError> {
        if dabs.len()>=16_384 {return Err(MobileError::Command("Stroke requires too many brush dabs; increase spacing or use a shorter stroke".into()));}
        let size=radius*(1.0-settings.size_jitter*random(&mut seed)).max(0.01);
        let rotation=settings.rotation.to_radians()+if settings.follow_direction {direction} else {0.0}+(random(&mut seed)*2.0-1.0)*settings.rotation_jitter.to_radians();
        let dx=(random(&mut seed)*2.0-1.0)*radius*settings.scatter;let dy=(random(&mut seed)*2.0-1.0)*radius*settings.scatter;
        let texture_seed=random(&mut seed)*65535.0;
        dabs.push([x+dx,y+dy,size,size*settings.aspect,rotation,settings.hardness,settings.flow,settings.tip as f32,texture_seed]);Ok(())
    };
    let first=&points[0];let direction=points.get(1).map(|p|(p.y-first.y).atan2(p.x-first.x)).unwrap_or(0.0);
    emit(first.x,first.y,first.radius,direction)?;
    let mut remaining=(first.radius*2.0*settings.spacing).max(0.25);
    for pair in points.windows(2) {
        let a=&pair[0];let b=&pair[1];let dx=b.x-a.x;let dy=b.y-a.y;let length=dx.hypot(dy);if length<0.0001 {continue;}
        let direction=dy.atan2(dx);let mut distance=remaining;
        while distance<=length {
            let t=distance/length;let radius=a.radius+(b.radius-a.radius)*t;
            emit(a.x+dx*t,a.y+dy*t,radius,direction)?;distance+=(radius*2.0*settings.spacing).max(0.25);
        }
        remaining=distance-length;
    }
    drop(emit);
    let matrices=crate::drawing_raster::drawing_symmetry_matrices(width as f32,height as f32,settings.symmetry);
    if dabs.len()*(matrices.len()+1)>16_384 {return Err(invalid());}
    let original=dabs.clone();
    for m in matrices {for dab in &original {
        let mut next=*dab;next[0]=m[0]*dab[0]+m[1]*dab[1]+m[2];next[1]=m[3]*dab[0]+m[4]*dab[1]+m[5];
        let ux=dab[4].cos();let uy=dab[4].sin();next[4]=(m[3]*ux+m[4]*uy).atan2(m[0]*ux+m[1]*uy);dabs.push(next);
    }}Ok(dabs)
}
#[uniffi::export]
pub fn drawing_native_brush_coverage(points:Vec<DrawingStrokePoint>,width:u32,height:u32,settings:DrawingNativeBrush)->Result<DrawingCoverage,MobileError> {
    if width==0 || height==0 || width>32768 || height>32768 {return Err(invalid());}
    let dabs=schedule(&points,&settings,width,height)?;
    let mut left=width as f32;let mut top=height as f32;let mut right=0.0f32;let mut bottom=0.0f32;
    for d in &dabs {let c=d[4].cos().abs();let s=d[4].sin().abs();let rx=c*d[2]+s*d[3]+1.0;let ry=s*d[2]+c*d[3]+1.0;left=left.min(d[0]-rx);top=top.min(d[1]-ry);right=right.max(d[0]+rx);bottom=bottom.max(d[1]+ry);}
    // Quantized bounds reduce GPU texture reallocations as the stroke grows.
    let left=((left.floor().clamp(0.0,width as f32) as u32)/64)*64;let top=((top.floor().clamp(0.0,height as f32) as u32)/64)*64;
    let right=(right.ceil().clamp(0.0,width as f32) as u32).div_ceil(64).saturating_mul(64).min(width);let bottom=(bottom.ceil().clamp(0.0,height as f32) as u32).div_ceil(64).saturating_mul(64).min(height);
    if right<=left || bottom<=top {return Ok(DrawingCoverage {left:0,top:0,width:0,height:0,alpha:vec![]});}
    if u64::from(right-left)*u64::from(bottom-top)>4_194_304 {return Err(MobileError::Command("GPU stroke coverage exceeds 4,194,304 pixels; use a shorter stroke".into()));}
    let hardware=filmcraft_gpu::drawing::status();
    if !hardware.verified {return Err(MobileError::Render(format!("Native GPU drawing unavailable: {}. Compatibility drawing must be selected explicitly.",hardware.detail)));}
    if right-left>hardware.max_dimension || bottom-top>hardware.max_dimension {return Err(MobileError::Command(format!("Stroke coverage exceeds the GPU's {} px texture dimension; use a shorter stroke",hardware.max_dimension)));}
    let alpha=filmcraft_gpu::drawing::dab_coverage(&dabs,left,top,right-left,bottom-top).ok_or_else(||MobileError::Render(format!("Native GPU brush failed: {}",filmcraft_gpu::drawing::status().detail)))?;
    Ok(DrawingCoverage {left,top,width:right-left,height:bottom-top,alpha})
}
#[uniffi::export]
pub fn drawing_native_brush_paint(mut pixels:Vec<u8>,width:u32,height:u32,points:Vec<DrawingStrokePoint>,settings:DrawingNativeBrush,color:Vec<u8>,opacity:f32,selection:Vec<u8>,erase:bool)->Result<Vec<u8>,MobileError> {
    validate(&pixels,width,height)?;
    if color.len()!=4 || !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) || (!selection.is_empty() && selection.len()!=pixels.len()/4) {return Err(invalid());}
    let tile=drawing_native_brush_coverage(points,width,height,settings)?;
    for y in 0..tile.height as usize {for x in 0..tile.width as usize {
        let index=(y+tile.top as usize)*width as usize+x+tile.left as usize;let i=index*4;
        let selected=if selection.is_empty() {1.0} else {f32::from(selection[index])/255.0};
        let sa=f32::from(tile.alpha[y*tile.width as usize+x])/255.0*opacity*selected*f32::from(color[3])/255.0;
        let da=f32::from(pixels[i+3])/255.0;
        if erase {pixels[i+3]=byte(da*(1.0-sa)*255.0);if pixels[i+3]==0 {pixels[i..i+3].fill(0);}continue;}
        let alpha=sa+da*(1.0-sa);
        for c in 0..3 {pixels[i+c]=if alpha>0.0 {byte((f32::from(color[c])*sa+f32::from(pixels[i+c])*da*(1.0-sa))/alpha)} else {0};}pixels[i+3]=byte(alpha*255.0);
    }}Ok(pixels)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn brush()->DrawingNativeBrush {DrawingNativeBrush {tip:0,spacing:0.5,hardness:1.0,aspect:1.0,rotation:0.0,scatter:0.0,size_jitter:0.0,rotation_jitter:0.0,flow:1.0,seed:1,follow_direction:false,symmetry:0}}
    #[test] fn scheduling_is_spacing_based_and_repeatable()->Result<(),MobileError> {let points=vec![DrawingStrokePoint{x:0.0,y:0.0,radius:5.0},DrawingStrokePoint{x:20.0,y:0.0,radius:5.0}];let a=schedule(&points,&brush(),100,100)?;assert_eq!(a.len(),5);assert_eq!(a,schedule(&points,&brush(),100,100)?);Ok(())}
    #[test] fn symmetry_replicates_dabs()->Result<(),MobileError> {let mut s=brush();s.symmetry=3;let d=schedule(&[DrawingStrokePoint{x:10.0,y:20.0,radius:5.0}],&s,100,100)?;assert_eq!(d.len(),4);assert_eq!((d[1][0],d[1][1]),(90.0,20.0));Ok(())}
}

#[uniffi::export]
pub fn drawing_encode_native_brush(s:DrawingNativeBrush)->Result<String,MobileError> {
    validate_brush(&s)?;
    serde_json::to_string(&serde_json::json!({"format":"filmcraft-native-brush","version":1,"tip":s.tip,"spacing":s.spacing,"hardness":s.hardness,"aspect":s.aspect,"rotation":s.rotation,"scatter":s.scatter,"size_jitter":s.size_jitter,"rotation_jitter":s.rotation_jitter,"flow":s.flow,"seed":s.seed,"follow_direction":s.follow_direction,"symmetry":s.symmetry})).map_err(|_|invalid())
}
#[uniffi::export]
pub fn drawing_decode_native_brush(text:String)->Result<DrawingNativeBrush,MobileError> {
    if text.len()>8192 {return Err(invalid());}let v:serde_json::Value=serde_json::from_str(&text).map_err(|_|invalid())?;
    if v["format"].as_str()!=Some("filmcraft-native-brush") || v["version"].as_u64()!=Some(1) {return Err(invalid());}
    let number=|key:&str|v[key].as_f64().map(|v|v as f32).ok_or_else(invalid);
    let integer=|key:&str|v[key].as_u64().and_then(|v|u32::try_from(v).ok()).ok_or_else(invalid);
    let s=DrawingNativeBrush {tip:integer("tip")?,spacing:number("spacing")?,hardness:number("hardness")?,aspect:number("aspect")?,rotation:number("rotation")?,scatter:number("scatter")?,size_jitter:number("size_jitter")?,rotation_jitter:number("rotation_jitter")?,flow:number("flow")?,seed:integer("seed")?,follow_direction:v["follow_direction"].as_bool().ok_or_else(invalid)?,symmetry:integer("symmetry")?};validate_brush(&s)?;Ok(s)
}

#[cfg(test)]
mod resource_tests {
    use super::*;
    #[test] fn settings_roundtrip()->Result<(),MobileError> {
        let brush=DrawingNativeBrush {tip:6,spacing:0.3,hardness:0.4,aspect:0.5,rotation:30.0,scatter:0.2,size_jitter:0.1,rotation_jitter:15.0,flow:0.4,seed:123,follow_direction:true,symmetry:3};
        let result=drawing_decode_native_brush(drawing_encode_native_brush(brush)?)?;assert_eq!(result.tip,6);assert_eq!(result.seed,123);assert_eq!(result.symmetry,3);assert!(result.follow_direction);Ok(())
    }
    #[test] fn zero_spacing_rejected_without_gpu_access() {
        let brush=DrawingNativeBrush {tip:0,spacing:0.0,hardness:1.0,aspect:1.0,rotation:0.0,scatter:0.0,size_jitter:0.0,rotation_jitter:0.0,flow:1.0,seed:1,follow_direction:false,symmetry:0};
        assert!(drawing_encode_native_brush(brush).is_err());
    }
}

#[cfg(test)]
mod bounds_tests {
    use super::*;
    fn brush()->DrawingNativeBrush {DrawingNativeBrush {tip:0,spacing:0.02,hardness:1.0,aspect:1.0,rotation:0.0,scatter:0.0,size_jitter:0.0,rotation_jitter:0.0,flow:1.0,seed:0,follow_direction:false,symmetry:0}}
    #[test] fn dense_long_strokes_fail_before_gpu_submission() {
        let points=[DrawingStrokePoint{x:0.0,y:0.0,radius:0.1},DrawingStrokePoint{x:1_000_000.0,y:0.0,radius:0.1}];
        assert!(schedule(&points,&brush(),100,100).is_err());
    }
    #[test] fn invalid_points_are_rejected() {
        assert!(schedule(&[DrawingStrokePoint{x:f32::NAN,y:0.0,radius:1.0}],&brush(),100,100).is_err());
        assert!(schedule(&[DrawingStrokePoint{x:1.0,y:0.0,radius:-1.0}],&brush(),100,100).is_err());
    }
}
