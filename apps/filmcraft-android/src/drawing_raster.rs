//! Native raster operations used by the drawing frontend. All pixels are straight RGBA8 at
//! the bridge boundary. Selections and Android display objects stay in the frontend for now.
use crate::MobileError;
fn validate(pixels: &[u8], width: u32, height: u32) -> Result<(), MobileError> {
    let count = u64::from(width) * u64::from(height);
    if width == 0 || height == 0 || count > 4_194_304 || pixels.len() as u64 != count * 4 {
        return Err(MobileError::Command("Invalid drawing dimensions or pixel buffer".into()));
    }
    Ok(())
}
fn byte(x: f32) -> u8 { x.round().clamp(0.0, 255.0) as u8 }

/// Pixel operations run in Rust; alpha is preserved except for the explicit alpha filters.
#[uniffi::export]
pub fn drawing_filter_rgba(mut pixels: Vec<u8>, width: u32, height: u32, operation: String, amount: f32) -> Result<Vec<u8>, MobileError> {
    validate(&pixels, width, height)?;
    let amount = if amount.is_finite() { amount.clamp(0.0, 1.0) } else { 0.5 };
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
