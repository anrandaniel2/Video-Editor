//! Extended raster filters. All buffers and filter support radii are bounded at the bridge.
use crate::MobileError;
use crate::drawing_raster::{validate,byte};
fn packed(p:&[u8])->[f32;4] {let a=f32::from(p[3])/255.0;[f32::from(p[0])*a,f32::from(p[1])*a,f32::from(p[2])*a,a]}
fn unpack(p:&mut [u8],v:[f32;4]) {for c in 0..3 {p[c]=if v[3]>0.000001 {byte(v[c]/v[3])} else {0};}p[3]=byte(v[3]*255.0);}
fn random(state:&mut u64)->f32 {let mut x=*state;x^=x<<13;x^=x>>7;x^=x<<17;*state=x;(x>>40) as f32/16_777_216.0}

#[uniffi::export]
pub fn drawing_advanced_filter(mut pixels:Vec<u8>,width:u32,height:u32,operation:String,radius:u32,strength:f32,angle:f32,seed:u64)->Result<Vec<u8>,MobileError> {
    validate(&pixels,width,height)?;
    if radius==0 || radius>24 || !strength.is_finite() || !(0.0..=1.0).contains(&strength) || !angle.is_finite() {return Err(MobileError::Command("Invalid advanced filter parameters".into()));}
    if !matches!(operation.as_str(),"gaussian"|"unsharp"|"highpass"|"motion"|"median"|"sobel"|"noise-mono"|"noise-rgb"|"noise-gaussian"|"spread"|"vignette") {return Err(MobileError::Command("Unknown advanced filter".into()));}
    if strength==0.0 {return Ok(pixels);}
    let w=width as usize;let h=height as usize;let r=radius as isize;
    let mut state=if seed==0 {0x9e3779b97f4a7c15} else {seed};
    match operation.as_str() {
        "gaussian"|"unsharp"|"highpass" => {
            let sigma=(radius as f32/3.0).max(0.5);let mut weights:Vec<f32>=(-r..=r).map(|v|(-(v*v) as f32/(2.0*sigma*sigma)).exp()).collect();
            let total: f32=weights.iter().sum();for weight in &mut weights {*weight/=total;}
            let mut horizontal=vec![[0.0;4];w*h];
            for y in 0..h {for x in 0..w {
                let mut value=[0.0;4];
                for (k,weight) in (-r..=r).zip(weights.iter()) {let xx=(x as isize+k).clamp(0,w as isize-1) as usize;let p=packed(&pixels[(y*w+xx)*4..(y*w+xx)*4+4]);for c in 0..4 {value[c]+=p[c]*weight;}}
                horizontal[y*w+x]=value;
            }}
            for y in 0..h {for x in 0..w {
                let i=(y*w+x)*4;let mut value=[0.0;4];
                for (k,weight) in (-r..=r).zip(weights.iter()) {let yy=(y as isize+k).clamp(0,h as isize-1) as usize;for c in 0..4 {value[c]+=horizontal[yy*w+x][c]*weight;}}
                if operation=="gaussian" {
                    let original=packed(&pixels[i..i+4]);for c in 0..4 {value[c]=original[c]*(1.0-strength)+value[c]*strength;}unpack(&mut pixels[i..i+4],value);
                } else {
                    for c in 0..3 {let blurred=if value[3]>0.000001 {value[c]/value[3]} else {f32::from(pixels[i+c])};let difference=f32::from(pixels[i+c])-blurred;
                        pixels[i+c]=if operation=="unsharp" {byte(f32::from(pixels[i+c])+difference*strength*4.0)} else {byte(f32::from(pixels[i+c])*(1.0-strength)+(128.0+difference)*strength)};
                    }
                }
            }}
        }
        "motion" => {
            let source=pixels.clone();let a=angle.to_radians();let dx=a.cos();let dy=a.sin();
            for y in 0..h {for x in 0..w {
                let mut value=[0.0;4];
                for k in -r..=r {let xx=(x as f32+k as f32*dx).round().clamp(0.0,(w-1) as f32) as usize;let yy=(y as f32+k as f32*dy).round().clamp(0.0,(h-1) as f32) as usize;let sample=packed(&source[(yy*w+xx)*4..(yy*w+xx)*4+4]);for c in 0..4 {value[c]+=sample[c]/(2*r+1) as f32;}}
                let i=(y*w+x)*4;let original=packed(&source[i..i+4]);for c in 0..4 {value[c]=original[c]*(1.0-strength)+value[c]*strength;}unpack(&mut pixels[i..i+4],value);
            }}
        }
        "median"|"sobel" => {
            let source=pixels.clone();
            for y in 0..h {for x in 0..w {
                let i=(y*w+x)*4;
                if operation=="median" {
                    for c in 0..3 {let mut samples=[0u8;9];let mut count=0;
                        for yy in -1..=1 {for xx in -1..=1 {let sx=(x as isize+xx).clamp(0,w as isize-1) as usize;let sy=(y as isize+yy).clamp(0,h as isize-1) as usize;let index=(sy*w+sx)*4;
                            samples[count]=if source[index+3]==0 {source[i+c]} else {source[index+c]};count+=1;
                        }}samples.sort_unstable();pixels[i+c]=byte(f32::from(source[i+c])*(1.0-strength)+f32::from(samples[4])*strength);
                    }
                } else {
                    let mut gx=0.0;let mut gy=0.0;let kx=[-1.0,0.0,1.0,-2.0,0.0,2.0,-1.0,0.0,1.0];let ky=[-1.0,-2.0,-1.0,0.0,0.0,0.0,1.0,2.0,1.0];let mut k=0;
                    for yy in -1..=1 {for xx in -1..=1 {let sx=(x as isize+xx).clamp(0,w as isize-1) as usize;let sy=(y as isize+yy).clamp(0,h as isize-1) as usize;let p=packed(&source[(sy*w+sx)*4..(sy*w+sx)*4+4]);let lum=0.2126*p[0]+0.7152*p[1]+0.0722*p[2];gx+=lum*kx[k];gy+=lum*ky[k];k+=1;}}
                    let edge=gx.hypot(gy);for c in 0..3 {pixels[i+c]=byte(f32::from(source[i+c])*(1.0-strength)+edge*strength);}
                }
            }}
        }
        "noise-mono"|"noise-rgb"|"noise-gaussian" => {
            for p in pixels.chunks_exact_mut(4) {
                let shared=(random(&mut state)-0.5)*255.0*strength;
                for c in 0..3 {
                    let noise=match operation.as_str() {
                        "noise-rgb"=>(random(&mut state)-0.5)*255.0*strength,
                        "noise-gaussian"=>(-2.0*random(&mut state).max(0.000001).ln()).sqrt()*(std::f32::consts::TAU*random(&mut state)).cos()*64.0*strength,
                        _=>shared,
                    };p[c]=byte(f32::from(p[c])+noise);
                }
            }
        }
        "spread" => {
            let source=pixels.clone();
            for y in 0..h {for x in 0..w {
                if random(&mut state)>strength {continue;}
                let xx=(x as isize+((random(&mut state)*2.0-1.0)*radius as f32).round() as isize).clamp(0,w as isize-1) as usize;
                let yy=(y as isize+((random(&mut state)*2.0-1.0)*radius as f32).round() as isize).clamp(0,h as isize-1) as usize;
                pixels[(y*w+x)*4..(y*w+x)*4+4].copy_from_slice(&source[(yy*w+xx)*4..(yy*w+xx)*4+4]);
            }}
        }
        "vignette" => {
            for y in 0..h {for x in 0..w {
                let nx=((x as f32+0.5)/w as f32-0.5)*2.0;let ny=((y as f32+0.5)/h as f32-0.5)*2.0;let gain=(1.0-strength*(nx*nx+ny*ny)*0.5).clamp(0.0,1.0);
                let i=(y*w+x)*4;for c in 0..3 {pixels[i+c]=byte(f32::from(pixels[i+c])*gain);}
            }}
        }
        _=>return Err(MobileError::Command("Unknown advanced filter".into())),
    }
    Ok(pixels)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn seeded_noise_is_repeatable()->Result<(),MobileError> {
        let input=vec![128;4*9];let a=drawing_advanced_filter(input.clone(),3,3,"noise-rgb".into(),1,0.8,0.0,123)?;
        let b=drawing_advanced_filter(input,3,3,"noise-rgb".into(),1,0.8,0.0,123)?;assert_eq!(a,b);Ok(())
    }
    #[test] fn gaussian_does_not_leak_hidden_colour()->Result<(),MobileError> {
        let out=drawing_advanced_filter(vec![255,0,0,255,0,255,0,0],2,1,"gaussian".into(),2,1.0,0.0,1)?;
        assert_eq!(out[1],0);assert_eq!(out[5],0);assert_eq!(out[0],255);assert_eq!(out[4],255);Ok(())
    }
    #[test] fn zero_strength_is_identity()->Result<(),MobileError> {
        let input=vec![10,20,30,255];
        for operation in ["gaussian","unsharp","highpass","noise-mono","noise-rgb","noise-gaussian","vignette","motion","sobel","median"] {
            assert_eq!(drawing_advanced_filter(input.clone(),1,1,operation.into(),1,0.0,0.0,1)?,input);
        }Ok(())
    }
}
