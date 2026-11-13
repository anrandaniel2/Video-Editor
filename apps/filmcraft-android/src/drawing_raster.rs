//! Native raster operations used by the drawing frontend. All pixels are straight RGBA8 at
//! the bridge boundary. Selections and Android display objects stay in the frontend for now.
use crate::MobileError;
pub(crate) fn validate(pixels: &[u8], width: u32, height: u32) -> Result<(), MobileError> {
    let count = u64::from(width) * u64::from(height);
    if width == 0 || height == 0 || count > 4_194_304 || pixels.len() as u64 != count * 4 {
        return Err(MobileError::Command("Invalid drawing dimensions or pixel buffer".into()));
    }
    Ok(())
}
pub(crate) fn byte(x: f32) -> u8 { x.round().clamp(0.0, 255.0) as u8 }

/// Pixel operations run in Rust; alpha is preserved except for the explicit alpha filters.
#[uniffi::export]
pub fn drawing_filter_rgba(mut pixels: Vec<u8>, width: u32, height: u32, operation: String, amount: f32) -> Result<Vec<u8>, MobileError> {
    validate(&pixels, width, height)?;
    let amount = if amount.is_finite() { amount.clamp(0.0, 1.0) } else { 0.5 };
    if matches!(operation.as_str(), "flip-x" | "flip-y" | "rotate-180" | "offset-wrap") {
        let w=width as usize; let h=height as usize; let source=pixels.clone();
        for y in 0..h { for x in 0..w {
            let (sx,sy)=match operation.as_str() {"flip-x"=>(w-1-x,y),"flip-y"=>(x,h-1-y),"rotate-180"=>(w-1-x,h-1-y),_=>( (x+w/2)%w,(y+h/2)%h )};
            pixels[(y*w+x)*4..(y*w+x)*4+4].copy_from_slice(&source[(sy*w+sx)*4..(sy*w+sx)*4+4]);
        } }
        return Ok(pixels);
    }
    if operation == "auto-levels" || operation == "equalize" {
        let mut histogram=[[0u64;256];3];
        for p in pixels.chunks_exact(4) { for c in 0..3 {histogram[c][p[c] as usize]+=u64::from(p[3]);} }
        let mut tables=[[0u8;256];3];
        for c in 0..3 {
            let total: u64=histogram[c].iter().sum();
            let low=histogram[c].iter().position(|n|*n>0).unwrap_or(0); let high=histogram[c].iter().rposition(|n|*n>0).unwrap_or(255);
            let mut cumulative=0u64;let baseline=histogram[c][low];
            for v in 0..256 {
                cumulative+=histogram[c][v];
                tables[c][v]=if high<=low || total<=baseline {v as u8} else if operation=="auto-levels" {byte((v as f32-low as f32)*255.0/(high-low) as f32)} else {byte(cumulative.saturating_sub(baseline) as f32*255.0/(total-baseline) as f32)};
            }
        }
        for p in pixels.chunks_exact_mut(4) {for c in 0..3 {p[c]=tables[c][p[c] as usize];}}
        return Ok(pixels);
    }
    if operation == "pixelize" {
        let block = (2.0 + amount * 62.0).round() as usize;
        let w = width as usize; let h = height as usize;
        for y in (0..h).step_by(block) { for x in (0..w).step_by(block) {
            let right = (x + block).min(w); let bottom = (y + block).min(h);
            let mut sum = [0u64; 4];
            for yy in y..bottom { for xx in x..right {
                let i = (yy*w+xx)*4; let a = u64::from(pixels[i+3]);
                for c in 0..3 { sum[c] += u64::from(pixels[i+c])*a; } sum[3]+=a;
            } }
            let count = ((right-x)*(bottom-y)) as u64;
            let color = [if sum[3]>0 {(sum[0]/sum[3]) as u8} else {0}, if sum[3]>0 {(sum[1]/sum[3]) as u8} else {0}, if sum[3]>0 {(sum[2]/sum[3]) as u8} else {0}, (sum[3]/count) as u8];
            for yy in y..bottom { for xx in x..right { pixels[(yy*w+xx)*4..(yy*w+xx)*4+4].copy_from_slice(&color); } }
        } }
        return Ok(pixels);
    }
    if operation == "grow-alpha" || operation == "shrink-alpha" {
        // Separable square max/min filters, bounded radius and two linear image passes.
        let radius = (1.0 + amount*15.0).round() as usize;
        let w = width as usize; let h = height as usize;
        let grow = operation == "grow-alpha";
        let mut temp = vec![0u8; w*h];
        for y in 0..h { for x in 0..w {
            let mut a = if grow {0} else {255};
            for xx in x.saturating_sub(radius)..=(x+radius).min(w-1) { let v=pixels[(y*w+xx)*4+3]; a=if grow {a.max(v)} else {a.min(v)}; }
            temp[y*w+x]=a;
        } }
        for y in 0..h { for x in 0..w {
            let mut a = if grow {0} else {255};
            for yy in y.saturating_sub(radius)..=(y+radius).min(h-1) { let v=temp[yy*w+x]; a=if grow {a.max(v)} else {a.min(v)}; }
            pixels[(y*w+x)*4+3]=a;
        } }
        return Ok(pixels);
    }
    if matches!(operation.as_str(), "blur" | "sharpen" | "edges" | "emboss") {
        let w = width as usize; let h = height as usize;
        let source = pixels.clone();
        let kernel: [f32; 9] = match operation.as_str() {
            "blur" => [1.0/9.0; 9], "sharpen" => [0.0,-1.0,0.0,-1.0,5.0,-1.0,0.0,-1.0,0.0],
            "edges" => [-1.0,-1.0,-1.0,-1.0,8.0,-1.0,-1.0,-1.0,-1.0],
            _ => [-2.0,-1.0,0.0,-1.0,1.0,1.0,0.0,1.0,2.0],
        };
        for y in 0..h { for x in 0..w {
            let index = (y * w + x) * 4;
            for c in 0..3 {
                let mut sum = if operation == "emboss" { 128.0 } else { 0.0 };
                for ky in 0..3 { for kx in 0..3 {
                    let xx = (x as isize + kx as isize - 1).clamp(0, w as isize - 1) as usize;
                    let yy = (y as isize + ky as isize - 1).clamp(0, h as isize - 1) as usize;
                    let sample = (yy * w + xx) * 4;
                    // Ignore hidden RGB of transparent pixels to avoid colour fringes.
                    let rgb = if source[sample + 3] == 0 { source[index + c] } else { source[sample + c] };
                    sum += f32::from(rgb) * kernel[ky * 3 + kx];
                } }
                pixels[index + c] = byte(f32::from(source[index + c]) * (1.0 - amount) + sum * amount);
            }
        } }
        return Ok(pixels);
    }
    for p in pixels.chunks_exact_mut(4) {
        let r = f32::from(p[0]); let g = f32::from(p[1]); let b = f32::from(p[2]);
        let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        let rgb = match operation.as_str() {
            "red-channel" => [r;3],
            "green-channel" => [g;3],
            "blue-channel" => [b;3],
            "alpha-channel" => { let a=f32::from(p[3]);p[3]=255;[a;3] },
            "solarize" => {let threshold=amount*255.0;[if r>threshold {255.0-r} else {r},if g>threshold {255.0-g} else {g},if b>threshold {255.0-b} else {b}]},
            "temperature" => {let shift=(amount-0.5)*100.0;[r+shift,g,b-shift]},
            "alpha-threshold" => {p[3]=if f32::from(p[3])>=amount*255.0 {255} else {0};[r,g,b]},
            "gamma" => { let gamma = 0.1 + amount*2.9; [255.0*(r/255.0).powf(1.0/gamma),255.0*(g/255.0).powf(1.0/gamma),255.0*(b/255.0).powf(1.0/gamma)] },
            "invert" => [255.0-r,255.0-g,255.0-b],
            "grayscale" => [luma; 3],
            "sepia" => [r*0.393+g*0.769+b*0.189,r*0.349+g*0.686+b*0.168,r*0.272+g*0.534+b*0.131],
            "threshold" => [if luma >= amount * 255.0 { 255.0 } else { 0.0 }; 3],
            "brightness" => [r+(amount-0.5)*255.0,g+(amount-0.5)*255.0,b+(amount-0.5)*255.0],
            "contrast" => { let gain = (amount * 4.0).max(0.01); [(r-127.5)*gain+127.5,(g-127.5)*gain+127.5,(b-127.5)*gain+127.5] },
            "saturation" => { let gain = amount * 2.0; [luma+(r-luma)*gain,luma+(g-luma)*gain,luma+(b-luma)*gain] },
            "posterize" => { let levels = (amount * 14.0).round() + 2.0; let step = 255.0/(levels-1.0); [(r/step).round()*step,(g/step).round()*step,(b/step).round()*step] },
            "alpha-luma" => { p[3] = byte(f32::from(p[3]) * luma / 255.0); [r,g,b] },
            "opaque" => { p[3] = 255; [r,g,b] },
            _ => return Err(MobileError::Command("Unknown drawing filter".into())),
        };
        p[0]=byte(rgb[0]); p[1]=byte(rgb[1]); p[2]=byte(rgb[2]);
    }
    Ok(pixels)
}

/// Contiguous four-connected fill. Queue storage is bounded to one index per canvas pixel.
#[uniffi::export]
pub fn drawing_flood_fill(mut pixels: Vec<u8>, width: u32, height: u32, x: u32, y: u32, rgba: Vec<u8>, tolerance: u8, mask: Vec<u8>) -> Result<Vec<u8>, MobileError> {
    validate(&pixels, width, height)?;
    if x >= width || y >= height || rgba.len() != 4 { return Err(MobileError::Command("Invalid fill position or colour".into())); }
    if !mask.is_empty() && mask.len() != width as usize * height as usize { return Err(MobileError::Command("Invalid fill selection mask".into())); }
    let w = width as usize; let h = height as usize; let start = y as usize * w + x as usize;
    let target: [u8; 4] = std::array::from_fn(|c| pixels[start * 4 + c]);
    let mut visited = vec![false; w*h]; let mut queue = Vec::with_capacity(w*h); queue.push(start); visited[start] = true;
    let mut head = 0;
    while head < queue.len() {
        let index = queue[head]; head += 1;
        if !mask.is_empty() && mask[index] == 0 { continue; }
        let matches = (0..4).all(|c| (c < 3 && target[3] == 0 && pixels[index*4+3] == 0) || target[c].abs_diff(pixels[index*4+c]) <= tolerance);
        if !matches { continue; }
        pixels[index*4..index*4+4].copy_from_slice(&rgba);
        let xx = index % w; let yy = index / w;
        for next in [if xx>0 {Some(index-1)} else {None}, if xx+1<w {Some(index+1)} else {None}, if yy>0 {Some(index-w)} else {None}, if yy+1<h {Some(index+w)} else {None}].into_iter().flatten() {
            if !visited[next] { visited[next]=true;queue.push(next); }
        }
    }
    Ok(pixels)
}

/// Paint blend against a solid colour, using the engine's established W3C blend formulas.
#[uniffi::export]
pub fn drawing_blend_color(mut pixels: Vec<u8>, width: u32, height: u32, rgba: Vec<u8>, opacity: f32, mode: u32) -> Result<Vec<u8>, MobileError> {
    validate(&pixels, width, height)?;
    if rgba.len()!=4 || mode>=27 || !opacity.is_finite() { return Err(MobileError::Command("Invalid paint blend".into())); }
    let s = [f32::from(rgba[0])/255.0,f32::from(rgba[1])/255.0,f32::from(rgba[2])/255.0];
    let blend = filmcraft_render::Blend::from_index(mode);
    let t = opacity.clamp(0.0,1.0) * f32::from(rgba[3])/255.0;
    for p in pixels.chunks_exact_mut(4) {
        let b = [f32::from(p[0])/255.0,f32::from(p[1])/255.0,f32::from(p[2])/255.0];
        let rgb = filmcraft_render::blend::blend_rgb(blend,b,s);
        for c in 0..3 { p[c]=byte((b[c]*(1.0-t)+rgb[c]*t)*255.0); }
    }
    Ok(pixels)
}

/// Display adapter consumes these canvas-centred matrices for every brush family.
#[uniffi::export]
pub fn drawing_symmetry_matrices(width: f32, height: f32, mode: u32) -> Vec<Vec<f32>> {
    if !width.is_finite() || !height.is_finite() || width<=0.0 || height<=0.0 { return vec![]; }
    let matrix = |a:f32,b:f32,c:f32,d:f32| vec![a,b,width*0.5-a*width*0.5-b*height*0.5,c,d,height*0.5-c*width*0.5-d*height*0.5,0.0,0.0,1.0];
    match mode {
        1 => vec![matrix(-1.0,0.0,0.0,1.0)],
        2 => vec![matrix(1.0,0.0,0.0,-1.0)],
        3 => vec![matrix(-1.0,0.0,0.0,1.0),matrix(1.0,0.0,0.0,-1.0),matrix(-1.0,0.0,0.0,-1.0)],
        4|5 => { let count=if mode==4 {4} else {8}; (1..count).map(|i| {let angle=i as f32*std::f32::consts::TAU/count as f32;matrix(angle.cos(),-angle.sin(),angle.sin(),angle.cos())}).collect() },
        _ => vec![],
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    #[test] fn fill_stays_inside_connected_region() {
        let pixels=vec![0,0,0,255, 255,255,255,255, 0,0,0,255];
        let out=drawing_flood_fill(pixels,3,1,0,0,vec![255,0,0,255],0,vec![]);
        assert!(out.is_ok()); if let Ok(p)=out {assert_eq!(&p[0..4],&[255,0,0,255]);assert_eq!(&p[8..12],&[0,0,0,255]);}
    }
    #[test] fn inversion_preserves_alpha() {
        let out=drawing_filter_rgba(vec![10,20,30,40],1,1,"invert".into(),1.0);
        assert!(out.is_ok()); if let Ok(p)=out {assert_eq!(p,vec![245,235,225,40]);}
    }
    #[test] fn symmetry_rotations_keep_canvas_centre_fixed() {
        for m in drawing_symmetry_matrices(100.0,200.0,5) {assert!((m[0]*50.0+m[1]*100.0+m[2]-50.0).abs()<0.001);assert!((m[3]*50.0+m[4]*100.0+m[5]-100.0).abs()<0.001);}
    }
}

/// Two-stop gradient painted source-over in premultiplied colour, without transparent-stop halos.
/// Geometry and sampling are native; callers provide canvas coordinates, not display coordinates.
#[uniffi::export]
pub fn drawing_gradient_rgba(mut pixels: Vec<u8>, width: u32, height: u32, start: Vec<f32>, end: Vec<f32>, first: Vec<u8>, last: Vec<u8>, kind: u32, repeat: u32, reverse: bool, opacity: f32) -> Result<Vec<u8>, MobileError> {
    validate(&pixels,width,height)?;
    if start.len()!=2 || end.len()!=2 || first.len()!=4 || last.len()!=4 || kind>3 || repeat>2 || !opacity.is_finite() || start.iter().chain(end.iter()).any(|v| !v.is_finite()) {
        return Err(MobileError::Command("Invalid gradient parameters".into()));
    }
    let dx=end[0]-start[0]; let dy=end[1]-start[1]; let length=dx.hypot(dy);
    if !length.is_finite() || length<0.001 {return Err(MobileError::Command("Gradient endpoints must be distinct".into()));}
    let ux=dx/length; let uy=dy/length;
    for (i,p) in pixels.chunks_exact_mut(4).enumerate() {
        let x=(i % width as usize) as f32+0.5-start[0]; let y=(i / width as usize) as f32+0.5-start[1];
        let along=x*ux+y*uy; let across=-x*uy+y*ux;
        let raw=match kind {0=>along/length,1=>x.hypot(y)/length,2=>(along.abs()+across.abs())/length,_=>(y.atan2(x)-dy.atan2(dx)).rem_euclid(std::f32::consts::TAU)/std::f32::consts::TAU};
        let t=match repeat {1=>raw.rem_euclid(1.0),2=>1.0-(raw.rem_euclid(2.0)-1.0).abs(),_=>raw.clamp(0.0,1.0)};
        let t=if reverse {1.0-t} else {t};
        let a0=f32::from(first[3])/255.0; let a1=f32::from(last[3])/255.0;
        let sa=(a0*(1.0-t)+a1*t)*opacity.clamp(0.0,1.0); let da=f32::from(p[3])/255.0; let out=sa+da*(1.0-sa);
        for c in 0..3 {
            let source=(f32::from(first[c])*a0*(1.0-t)+f32::from(last[c])*a1*t)*opacity.clamp(0.0,1.0);
            p[c]=if out>0.0 {byte((source+f32::from(p[c])*da*(1.0-sa))/out)} else {0};
        }
        p[3]=byte(out*255.0);
    }
    Ok(pixels)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod gradient_filter_tests {
    use super::*;
    #[test]
    fn transparent_stop_does_not_add_hidden_colour() {
        let result = drawing_gradient_rgba(vec![0;4],1,1,vec![0.0,0.5],vec![1.0,0.5],vec![255,0,0,255],vec![0,0,255,0],0,0,false,1.0).expect("valid gradient");
        assert_eq!(result,vec![255,0,0,128]);
    }
    #[test]
    fn zero_opacity_gradient_preserves_destination() {
        let original=vec![10,20,30,255];
        let result=drawing_gradient_rgba(original.clone(),1,1,vec![0.0,0.0],vec![1.0,0.0],vec![255;4],vec![0;4],0,0,false,0.0).expect("valid gradient");
        assert_eq!(result,original);
    }
    #[test]
    fn pixelize_ignores_hidden_transparent_rgb() {
        let result=drawing_filter_rgba(vec![255,0,0,255,0,0,255,0],2,1,"pixelize".into(),0.0).expect("valid pixels");
        assert_eq!(result,vec![255,0,0,127,255,0,0,127]);
    }
    #[test]
    fn alpha_morphology_expands_and_contracts() {
        let source=vec![1,2,3,0,1,2,3,255,1,2,3,0];
        let grow=drawing_filter_rgba(source.clone(),3,1,"grow-alpha".into(),0.0).expect("valid pixels");
        let shrink=drawing_filter_rgba(source,3,1,"shrink-alpha".into(),0.0).expect("valid pixels");
        assert_eq!([grow[3],grow[7],grow[11]],[255,255,255]);
        assert_eq!([shrink[3],shrink[7],shrink[11]],[0,0,0]);
    }
    #[test]
    fn degenerate_gradient_is_rejected() {
        assert!(drawing_gradient_rgba(vec![0;4],1,1,vec![0.0,0.0],vec![0.0,0.0],vec![255;4],vec![0;4],0,0,false,1.0).is_err());
    }
}

/// Binary selection morphology. Outside the canvas is unselected, including during erosion.
/// Rectangular radius uses separable sliding windows, O(width*height), independent of radius.
#[uniffi::export]
pub fn drawing_selection_mask(mask: Vec<u8>, width: u32, height: u32, operation: String, radius: u32) -> Result<Vec<u8>, MobileError> {
    let n=u64::from(width)*u64::from(height);
    if width==0 || height==0 || n>4_194_304 || mask.len() as u64!=n || radius>256 {
        return Err(MobileError::Command("Invalid selection mask or radius".into()));
    }
    let w=width as usize; let h=height as usize; let r=radius as usize;
    let source:Vec<u8>=mask.iter().map(|v| if *v>=128 {255} else {0}).collect();
    if operation=="invert" { return Ok(source.iter().map(|v|255-v).collect()); }
    if !matches!(operation.as_str(),"grow"|"shrink"|"border") {return Err(MobileError::Command("Unknown selection operation".into()));}
    let morph=|grow:bool| {
        let mut temp=vec![0u8;w*h]; let mut out=vec![0u8;w*h];
        for y in 0..h {
            let mut count=0usize;
            for xx in 0..=r.min(w-1) {count+=usize::from(source[y*w+xx]!=0);}
            for x in 0..w {
                temp[y*w+x]=if (grow && count>0) || (!grow && x>=r && x+r<w && count==2*r+1) {255} else {0};
                if x>=r {count-=usize::from(source[y*w+x-r]!=0);}
                if x+r+1<w {count+=usize::from(source[y*w+x+r+1]!=0);}
            }
        }
        for x in 0..w {
            let mut count=0usize;
            for yy in 0..=r.min(h-1) {count+=usize::from(temp[yy*w+x]!=0);}
            for y in 0..h {
                out[y*w+x]=if (grow && count>0) || (!grow && y>=r && y+r<h && count==2*r+1) {255} else {0};
                if y>=r {count-=usize::from(temp[(y-r)*w+x]!=0);}
                if y+r+1<h {count+=usize::from(temp[(y+r+1)*w+x]!=0);}
            }
        }
        out
    };
    match operation.as_str() {
        "grow"=>Ok(morph(true)), "shrink"=>Ok(morph(false)),
        _=> {let outer=morph(true);let inner=morph(false);Ok(outer.iter().zip(inner.iter()).map(|(a,b)|a.saturating_sub(*b)).collect())}
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod selection_tests {
    use super::*;
    #[test] fn grow_and_shrink_square() {
        let mut mask=vec![0;25];mask[12]=255;
        let grown=drawing_selection_mask(mask,5,5,"grow".into(),1).expect("mask");
        assert_eq!(grown.iter().filter(|v|**v==255).count(),9);
        let shrunk=drawing_selection_mask(grown,5,5,"shrink".into(),1).expect("mask");
        assert_eq!(shrunk.iter().filter(|v|**v==255).count(),1);assert_eq!(shrunk[12],255);
    }
    #[test] fn canvas_edge_is_unselected_for_erosion() {
        let result=drawing_selection_mask(vec![255;9],3,3,"shrink".into(),1).expect("mask");
        assert_eq!(result,vec![0,0,0,0,255,0,0,0,0]);
    }
    #[test] fn large_radius_and_zero_radius() {
        assert_eq!(drawing_selection_mask(vec![255],1,1,"shrink".into(),256).expect("mask"),vec![0]);
        assert_eq!(drawing_selection_mask(vec![255],1,1,"grow".into(),0).expect("mask"),vec![255]);
    }
}

/// Native magic-wand selection, four-connected or all matching pixels.
#[uniffi::export]
pub fn drawing_color_selection(pixels: Vec<u8>, width: u32, height: u32, x: u32, y: u32, tolerance: u8, contiguous: bool) -> Result<Vec<u8>, MobileError> {
    validate(&pixels,width,height)?;
    if x>=width || y>=height {return Err(MobileError::Command("Selection seed outside canvas".into()));}
    let w=width as usize;let h=height as usize;let seed=y as usize*w+x as usize;
    let target=&pixels[seed*4..seed*4+4];
    let similar=|i:usize| {
        let p=&pixels[i*4..i*4+4];
        target[3].abs_diff(p[3])<=tolerance && ((target[3]<16 && p[3]<16) || (0..3).all(|c|target[c].abs_diff(p[c])<=tolerance))
    };
    let mut mask=vec![0u8;w*h];
    if !contiguous {for (i,v) in mask.iter_mut().enumerate() {if similar(i) {*v=255;}}return Ok(mask);}
    let mut seen=vec![false;w*h];let mut queue=vec![seed];seen[seed]=true;let mut head=0;
    while head<queue.len() {
        let i=queue[head];head+=1;if !similar(i) {continue;}mask[i]=255;
        let x=i%w;let y=i/w;
        for next in [if x>0 {Some(i-1)} else {None},if x+1<w {Some(i+1)} else {None},if y>0 {Some(i-w)} else {None},if y+1<h {Some(i+w)} else {None}].into_iter().flatten() {
            if !seen[next] {seen[next]=true;queue.push(next);}
        }
    }
    Ok(mask)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod transform_histogram_tests {
    use super::*;
    #[test] fn flipping_twice_restores_all_pixels() {
        let input=vec![1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16];
        for operation in ["flip-x","flip-y","rotate-180"] {
            let once=drawing_filter_rgba(input.clone(),2,2,operation.into(),0.0).expect("valid transform");
            let twice=drawing_filter_rgba(once,2,2,operation.into(),0.0).expect("valid transform");
            assert_eq!(twice,input);
        }
    }
    #[test] fn auto_levels_retains_alpha_and_expands_range() {
        let out=drawing_filter_rgba(vec![50,50,50,255,100,100,100,128],2,1,"auto-levels".into(),0.0).expect("valid filter");
        assert_eq!(out,vec![0,0,0,255,255,255,255,128]);
    }
    #[test] fn equalizing_flat_channel_is_identity() {
        let input=vec![50,50,50,255];
        assert_eq!(drawing_filter_rgba(input.clone(),1,1,"equalize".into(),0.0).expect("valid filter"),input);
    }
}

/// Two-layer source-over compositing in display-encoded RGBA, including translucent inputs.
#[uniffi::export]
pub fn drawing_merge_layers(mut lower: Vec<u8>, upper: Vec<u8>, width: u32, height: u32, lower_opacity: f32, upper_opacity: f32, mode: u32) -> Result<Vec<u8>, MobileError> {
    validate(&lower,width,height)?;validate(&upper,width,height)?;
    if !lower_opacity.is_finite() || !upper_opacity.is_finite() || mode>=27 || mode==1 {return Err(MobileError::Command("Invalid layer composite parameters".into()));}
    for (b,s) in lower.chunks_exact_mut(4).zip(upper.chunks_exact(4)) {
        let ba=f32::from(b[3])/255.0*lower_opacity.clamp(0.0,1.0);let sa=f32::from(s[3])/255.0*upper_opacity.clamp(0.0,1.0);
        let bc=[f32::from(b[0])/255.0,f32::from(b[1])/255.0,f32::from(b[2])/255.0];let sc=[f32::from(s[0])/255.0,f32::from(s[1])/255.0,f32::from(s[2])/255.0];
        let blend=filmcraft_render::blend::blend_rgb(filmcraft_render::Blend::from_index(mode),bc,sc);let alpha=sa+ba*(1.0-sa);
        for c in 0..3 {b[c]=if alpha>0.0 {byte(255.0*(sa*(1.0-ba)*sc[c]+sa*ba*blend[c]+(1.0-sa)*ba*bc[c])/alpha)} else {0};}
        b[3]=byte(alpha*255.0);
    }
    Ok(lower)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod layer_tests {
    use super::*;
    #[test] fn source_over_translucent_layers() {
        let out=drawing_merge_layers(vec![0,0,255,128],vec![255,0,0,128],1,1,1.0,1.0,0).expect("layers");
        assert_eq!(out,vec![170,0,85,192]);
    }
    #[test] fn hidden_upper_layer_preserves_lower() {
        let lower=vec![40,50,60,128];
        assert_eq!(drawing_merge_layers(lower.clone(),vec![255;4],1,1,1.0,0.0,0).expect("layers"),lower);
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct DrawingAdjustments {
    pub exposure: f32,
    pub brightness: f32,
    pub contrast: f32,
    pub saturation: f32,
    pub hue_degrees: f32,
    pub gamma: f32,
    pub black_point: f32,
    pub white_point: f32,
    pub red_gain: f32,
    pub green_gain: f32,
    pub blue_gain: f32,
}
#[uniffi::export]
pub fn drawing_adjust_rgba(mut pixels: Vec<u8>, width: u32, height: u32, settings: DrawingAdjustments) -> Result<Vec<u8>, MobileError> {
    validate(&pixels,width,height)?;
    let s=settings;
    if [s.exposure,s.brightness,s.contrast,s.saturation,s.hue_degrees,s.gamma,s.black_point,s.white_point,s.red_gain,s.green_gain,s.blue_gain].iter().any(|v|!v.is_finite()) || !(-4.0..=4.0).contains(&s.exposure) || !(-1.0..=1.0).contains(&s.brightness) || !(0.0..=4.0).contains(&s.contrast) || !(0.0..=2.0).contains(&s.saturation) || !(-180.0..=180.0).contains(&s.hue_degrees) || !(0.1..=4.0).contains(&s.gamma) || s.black_point<0.0 || s.white_point>1.0 || s.white_point-s.black_point<0.001 || [s.red_gain,s.green_gain,s.blue_gain].iter().any(|v|!(0.0..=2.0).contains(v)) {return Err(MobileError::Command("Invalid colour adjustment settings".into()));}
    let angle=s.hue_degrees.to_radians();let cosine=angle.cos();let sine=angle.sin();
    for p in pixels.chunks_exact_mut(4) {
        let mut rgb=[0.0;3];
        for c in 0..3 {
            let level=((f32::from(p[c])/255.0-s.black_point)/(s.white_point-s.black_point)).clamp(0.0,1.0);
            rgb[c]=((level.powf(1.0/s.gamma)*2.0f32.powf(s.exposure)+s.brightness-0.5)*s.contrast+0.5).clamp(0.0,1.0);
        }
        // Hue rotation in YIQ, with luminance unchanged before gamut clipping.
        let y=0.299*rgb[0]+0.587*rgb[1]+0.114*rgb[2];
        let i=0.596*rgb[0]-0.274*rgb[1]-0.322*rgb[2];let q=0.211*rgb[0]-0.523*rgb[1]+0.312*rgb[2];
        let ii=(i*cosine-q*sine)*s.saturation;let qq=(i*sine+q*cosine)*s.saturation;
        let adjusted=[y+0.956*ii+0.621*qq,y-0.272*ii-0.647*qq,y-1.106*ii+1.703*qq];
        for c in 0..3 {p[c]=byte(adjusted[c]*[s.red_gain,s.green_gain,s.blue_gain][c]*255.0);}
    }
    Ok(pixels)
}

/// A 256-entry channel curve; channel 0=RGB master, 1/2/3=R/G/B, 4=alpha.
#[uniffi::export]
pub fn drawing_apply_curve(mut pixels: Vec<u8>, width: u32, height: u32, values: Vec<u8>, channel: u32) -> Result<Vec<u8>, MobileError> {
    validate(&pixels,width,height)?;
    if values.len()!=256 || channel>4 {return Err(MobileError::Command("Curve needs 256 samples and a valid channel".into()));}
    for pixel in pixels.chunks_exact_mut(4) {
        if channel==0 {for c in 0..3 {pixel[c]=values[pixel[c] as usize];}}
        else {let c=(channel-1) as usize;pixel[c]=values[pixel[c] as usize];}
    }
    Ok(pixels)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod curve_tests {
    use super::*;
    #[test] fn curve_channel_isolation()->Result<(),MobileError> {
        let invert=(0..256).map(|v|(255-v) as u8).collect();
        assert_eq!(drawing_apply_curve(vec![10,20,30,40],1,1,invert,2)?,vec![10,235,30,40]);Ok(())
    }
    #[test] fn curve_rejects_wrong_table_length() {assert!(drawing_apply_curve(vec![0;4],1,1,vec![0;255],0).is_err());}
}

/// Equally spaced curve anchors. Produces a deterministic piecewise-linear lookup table.
#[uniffi::export]
pub fn drawing_curve_samples(anchors: Vec<f32>) -> Result<Vec<u8>, MobileError> {
    if anchors.len()<2 || anchors.len()>32 || anchors.iter().any(|v|!v.is_finite() || !(0.0..=1.0).contains(v)) {return Err(MobileError::Command("Invalid curve anchors".into()));}
    Ok((0..256).map(|x| {
        let position=x as f32*(anchors.len()-1) as f32/255.0;
        let index=(position.floor() as usize).min(anchors.len()-2);let weight=position-index as f32;
        byte((anchors[index]*(1.0-weight)+anchors[index+1]*weight)*255.0)
    }).collect())
}

/// Rasterize a rectangle or ellipse selection from canvas-space endpoints.
#[uniffi::export]
pub fn drawing_geometric_selection(width:u32,height:u32,start:Vec<f32>,end:Vec<f32>,ellipse:bool)->Result<Vec<u8>,MobileError> {
    let count=u64::from(width)*u64::from(height);
    if width==0 || height==0 || count>4_194_304 || start.len()!=2 || end.len()!=2 || start.iter().chain(end.iter()).any(|v|!v.is_finite()) {return Err(MobileError::Command("Invalid selection geometry".into()));}
    let left=start[0].min(end[0]);let right=start[0].max(end[0]);
    let top=start[1].min(end[1]);let bottom=start[1].max(end[1]);
    let mut mask=vec![0u8;count as usize];let rx=(right-left)*0.5;let ry=(bottom-top)*0.5;
    if !rx.is_finite() || !ry.is_finite() {return Err(MobileError::Command("Selection geometry is too large".into()));}
    if rx<0.001 || ry<0.001 {return Ok(mask);}
    let cx=(right+left)*0.5;let cy=(bottom+top)*0.5;
    for y in top.floor().clamp(0.0,height as f32) as usize..bottom.ceil().clamp(0.0,height as f32) as usize {for x in left.floor().clamp(0.0,width as f32) as usize..right.ceil().clamp(0.0,width as f32) as usize {
        let px=x as f32+0.5;let py=y as f32+0.5;
        if px>=left && px<right && py>=top && py<bottom && (!ellipse || ((px-cx)/rx).powi(2)+((py-cy)/ry).powi(2)<=1.0) {mask[y*width as usize+x]=255;}
    }}Ok(mask)
}

/// Alpha-weighted sample of a bounded rectangular RGBA patch. Transparent hidden RGB is ignored.
#[uniffi::export]
pub fn drawing_average_color(pixels:Vec<u8>,width:u32,height:u32)->Result<Vec<u8>,MobileError> {
    validate(&pixels,width,height)?;
    if width>65 || height>65 {return Err(MobileError::Command("Colour sampling patch too large".into()));}
    let mut sum=[0u64;4];
    for p in pixels.chunks_exact(4) {let alpha=u64::from(p[3]);for c in 0..3 {sum[c]+=u64::from(p[c])*alpha;}sum[3]+=alpha;}
    if sum[3]==0 {return Ok(vec![0;4]);}
    Ok(vec![(sum[0]/sum[3]) as u8,(sum[1]/sum[3]) as u8,(sum[2]/sum[3]) as u8,(sum[3]/(u64::from(width)*u64::from(height))) as u8])
}
#[cfg(test)]
#[allow(clippy::expect_used)]
mod sampling_geometry_tests {
    use super::*;
    #[test] fn rectangle_reversed_drag()->Result<(),MobileError> {
        assert_eq!(drawing_geometric_selection(2,2,vec![2.0,2.0],vec![0.0,0.0],false)?,vec![255;4]);Ok(())
    }
    #[test] fn sample_ignores_hidden_rgb()->Result<(),MobileError> {
        assert_eq!(drawing_average_color(vec![0,255,0,255,255,0,0,0],2,1)?,vec![0,255,0,127]);Ok(())
    }
}

/// Apply painting alpha-lock against a pre-edit layer. Erasing cannot reduce its original alpha.
#[uniffi::export]
pub fn drawing_preserve_alpha(mut pixels:Vec<u8>,before:Vec<u8>,width:u32,height:u32)->Result<Vec<u8>,MobileError> {
    validate(&pixels,width,height)?;validate(&before,width,height)?;
    for (p,b) in pixels.chunks_exact_mut(4).zip(before.chunks_exact(4)) {
        if b[3]==0 {p.copy_from_slice(&[0,0,0,0]);}
        else {if p[3]<b[3] {p[..3].copy_from_slice(&b[..3]);}p[3]=b[3];}
    }Ok(pixels)
}

#[cfg(test)]
mod alpha_lock_tests {
    use super::*;
    #[test] fn alpha_lock_rejects_growth_and_erasure()->Result<(),MobileError> {
        let before=vec![0,0,0,0,200,100,50,128];let after=vec![255,0,0,255,0,0,0,0];
        assert_eq!(drawing_preserve_alpha(after,before,2,1)?,vec![0,0,0,0,200,100,50,128]);Ok(())
    }
    #[test] fn alpha_lock_accepts_recolouring()->Result<(),MobileError> {
        assert_eq!(drawing_preserve_alpha(vec![255,0,0,255],vec![0,0,0,128],1,1)?,vec![255,0,0,128]);Ok(())
    }
}

/// Grayscale layer-mask editing, with zero outside the canvas for feathering.
#[uniffi::export]
pub fn drawing_process_mask(mut mask:Vec<u8>,width:u32,height:u32,operation:String,radius:u32)->Result<Vec<u8>,MobileError> {
    let count=u64::from(width)*u64::from(height);
    if width==0 || height==0 || count>4_194_304 || mask.len() as u64!=count || radius>128 {return Err(MobileError::Command("Invalid layer mask".into()));}
    match operation.as_str() {
        "invert"=>{for value in &mut mask {*value=255-*value;}},
        "threshold"=>{for value in &mut mask {*value=if *value>=128 {255} else {0};}},
        "white"=>mask.fill(255),"black"=>mask.fill(0),
        "feather"=>{
            if radius==0 {return Ok(mask);}
            let w=width as usize;let h=height as usize;let r=radius as usize;let divisor=2*radius+1;let mut temp=vec![0u8;w*h];
            for y in 0..h {
                let mut sum:u32=(0..=r.min(w-1)).map(|x|u32::from(mask[y*w+x])).sum();
                for x in 0..w {temp[y*w+x]=(sum/divisor) as u8;if x>=r {sum-=u32::from(mask[y*w+x-r]);}if x+r+1<w {sum+=u32::from(mask[y*w+x+r+1]);}}
            }
            for x in 0..w {
                let mut sum:u32=(0..=r.min(h-1)).map(|y|u32::from(temp[y*w+x])).sum();
                for y in 0..h {mask[y*w+x]=(sum/divisor) as u8;if y>=r {sum-=u32::from(temp[(y-r)*w+x]);}if y+r+1<h {sum+=u32::from(temp[(y+r+1)*w+x]);}}
            }
        }
        _=>return Err(MobileError::Command("Unknown mask operation".into())),
    }Ok(mask)
}
#[cfg(test)]
mod layer_mask_tests {
    use super::*;
    #[test] fn grayscale_mask_inversion_is_lossless()->Result<(),MobileError> {
        let original=vec![0,64,128,255];let once=drawing_process_mask(original.clone(),4,1,"invert".into(),0)?;
        assert_eq!(drawing_process_mask(once,4,1,"invert".into(),0)?,original);Ok(())
    }
    #[test] fn feather_spreads_with_zero_boundary()->Result<(),MobileError> {
        let result=drawing_process_mask(vec![0,0,0,0,255,0,0,0,0],3,3,"feather".into(),1)?;
        assert_eq!(result,vec![28;9]);Ok(())
    }
}

#[uniffi::export]
pub fn drawing_mask_pixels(mut pixels:Vec<u8>,mask:Vec<u8>,width:u32,height:u32)->Result<Vec<u8>,MobileError> {
    validate(&pixels,width,height)?;
    if mask.len()!=pixels.len()/4 {return Err(MobileError::Command("Mask and layer dimensions differ".into()));}
    for (p,m) in pixels.chunks_exact_mut(4).zip(mask.iter()) {p[3]=((u16::from(p[3])*u16::from(*m)+127)/255) as u8;}
    Ok(pixels)
}

/// Paint mask values using the same hardware-MSAA coverage backend as standard pen strokes.
#[uniffi::export]
pub fn drawing_paint_layer_mask(mut mask:Vec<u8>,width:u32,height:u32,points:Vec<crate::drawing::DrawingStrokePoint>,value:u8,opacity:f32,selection:Vec<u8>)->Result<Vec<u8>,MobileError> {
    let count=u64::from(width)*u64::from(height);
    if width==0 || height==0 || count>4_194_304 || mask.len() as u64!=count || (!selection.is_empty() && selection.len()!=mask.len()) || !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {return Err(MobileError::Command("Invalid mask stroke".into()));}
    let coverage=crate::drawing::drawing_stroke_coverage(points,width,height).ok_or_else(||MobileError::Command("Mask stroke exceeds the coverage renderer's bounds".into()))?;
    for y in 0..coverage.height as usize {for x in 0..coverage.width as usize {
        let index=(y+coverage.top as usize)*width as usize+x+coverage.left as usize;
        let coverage_alpha=f32::from(coverage.alpha[y*coverage.width as usize+x])/255.0;
        let selected=if selection.is_empty() {1.0} else {f32::from(selection[index])/255.0};
        let alpha=coverage_alpha*selected*opacity;mask[index]=byte(f32::from(mask[index])*(1.0-alpha)+f32::from(value)*alpha);
    }}Ok(mask)
}

/// Clone and tonal retouching share bounded native stroke coverage. Clone samples an immutable
/// pre-stroke image, avoiding feedback/smearing; tonal tools retain destination alpha.
#[uniffi::export]
pub fn drawing_retouch_stroke(mut pixels:Vec<u8>,source:Vec<u8>,width:u32,height:u32,points:Vec<crate::drawing::DrawingStrokePoint>,operation:String,offset:Vec<f32>,color:Vec<u8>,strength:f32,selection:Vec<u8>,tonal_range:u32)->Result<Vec<u8>,MobileError> {
    validate(&pixels,width,height)?;
    if !matches!(operation.as_str(),"Clone stamp"|"Dodge"|"Burn"|"Desaturate"|"Colorize") || !strength.is_finite() || !(0.0..=1.0).contains(&strength) || tonal_range>3 || (!selection.is_empty() && selection.len()!=pixels.len()/4) || color.len()!=3 {return Err(MobileError::Command("Invalid retouch settings".into()));}
    if operation=="Clone stamp" {validate(&source,width,height)?;if offset.len()!=2 || offset.iter().any(|v|!v.is_finite()) {return Err(MobileError::Command("Set a valid clone source first".into()));}}
    let coverage=crate::drawing::drawing_stroke_coverage(points,width,height).ok_or_else(||MobileError::Command("Retouch stroke exceeds the coverage renderer's bounds".into()))?;
    let w=width as usize;let h=height as usize;
    for yy in 0..coverage.height as usize {for xx in 0..coverage.width as usize {
        let x=xx+coverage.left as usize;let y=yy+coverage.top as usize;let index=y*w+x;let i=index*4;
        let selected=if selection.is_empty() {1.0} else {f32::from(selection[index])/255.0};
        let mut amount=f32::from(coverage.alpha[yy*coverage.width as usize+xx])/255.0*strength*selected;
        if amount==0.0 {continue;}
        if operation=="Clone stamp" {
            let fx=x as f32+offset[0];let fy=y as f32+offset[1];
            if fx<0.0 || fy<0.0 || fx>(w-1) as f32 || fy>(h-1) as f32 {continue;}
            let x0=fx.floor() as usize;let y0=fy.floor() as usize;let x1=(x0+1).min(w-1);let y1=(y0+1).min(h-1);let tx=fx-x0 as f32;let ty=fy-y0 as f32;let mut sample=[0.0;4];
            for (sx,sy,weight) in [(x0,y0,(1.0-tx)*(1.0-ty)),(x1,y0,tx*(1.0-ty)),(x0,y1,(1.0-tx)*ty),(x1,y1,tx*ty)] {
                let s=(sy*w+sx)*4;let alpha=f32::from(source[s+3])/255.0;
                for c in 0..3 {sample[c]+=f32::from(source[s+c])*alpha*weight;}sample[3]+=alpha*weight;
            }
            let sa=sample[3]*amount;let da=f32::from(pixels[i+3])/255.0;let alpha=sa+da*(1.0-sa);
            for c in 0..3 {pixels[i+c]=if alpha>0.0 {byte((sample[c]*amount+f32::from(pixels[i+c])*da*(1.0-sa))/alpha)} else {0};}
            pixels[i+3]=byte(alpha*255.0);
        } else {
            let rgb=[f32::from(pixels[i])/255.0,f32::from(pixels[i+1])/255.0,f32::from(pixels[i+2])/255.0];let luma=0.2126*rgb[0]+0.7152*rgb[1]+0.0722*rgb[2];
            amount*=match tonal_range {1=>(1.0-luma).powi(2),2=>4.0*luma*(1.0-luma),3=>luma.powi(2),_=>1.0};
            let adjusted=match operation.as_str() {
                "Dodge"=>rgb.map(|v|v+(1.0-v)*0.5),"Burn"=>rgb.map(|v|v*0.5),"Desaturate"=>[luma;3],
                _=>filmcraft_render::blend::blend_rgb(filmcraft_render::Blend::Color,rgb,[f32::from(color[0])/255.0,f32::from(color[1])/255.0,f32::from(color[2])/255.0]),
            };
            for c in 0..3 {pixels[i+c]=byte((rgb[c]*(1.0-amount)+adjusted[c]*amount)*255.0);}
        }
    }}Ok(pixels)
}

#[cfg(test)]
mod retouch_tests {
    use super::*;
    fn points()->Vec<crate::drawing::DrawingStrokePoint> {vec![crate::drawing::DrawingStrokePoint {x:1.0,y:0.5,radius:3.0}]}
    #[test] fn clone_samples_snapshot_not_modified_target()->Result<(),MobileError> {
        let input=vec![255,0,0,255,0,0,255,255];
        let out=drawing_retouch_stroke(input.clone(),input,2,1,points(),"Clone stamp".into(),vec![-1.0,0.0],vec![0;3],1.0,vec![],0)?;
        assert_eq!(out,vec![255,0,0,255,255,0,0,255]);Ok(())
    }
    #[test] fn selection_rejects_all_retouching()->Result<(),MobileError> {
        let input=vec![10,20,30,128,40,50,60,64];
        let out=drawing_retouch_stroke(input.clone(),vec![],2,1,points(),"Dodge".into(),vec![],vec![0;3],1.0,vec![0;2],0)?;
        assert_eq!(out,input);Ok(())
    }
    #[test] fn tonal_tools_preserve_alpha()->Result<(),MobileError> {
        for tool in ["Dodge","Burn","Desaturate","Colorize"] {
            let out=drawing_retouch_stroke(vec![100,150,200,128,20,30,40,64],vec![],2,1,points(),tool.into(),vec![],vec![255,0,0],1.0,vec![],0)?;
            assert_eq!(out[3],128);assert_eq!(out[7],64);
        }Ok(())
    }
}
