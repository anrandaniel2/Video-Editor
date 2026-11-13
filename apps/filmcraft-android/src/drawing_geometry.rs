//! Native polygon selection rasterization and affine transform construction.
use crate::MobileError;
#[uniffi::export]
pub fn drawing_polygon_selection(width:u32,height:u32,coordinates:Vec<f32>)->Result<Vec<u8>,MobileError> {
    let count=u64::from(width)*u64::from(height);
    if width==0 || height==0 || count>4_194_304 || coordinates.len()<6 || coordinates.len()>256 || coordinates.len()%2!=0 || coordinates.iter().any(|v|!v.is_finite() || v.abs()>16_777_216.0) {return Err(MobileError::Command("Invalid polygon selection".into()));}
    let points:Vec<&[f32]>=coordinates.chunks_exact(2).collect();let mut output=vec![0u8;count as usize];let mut intersections=Vec::with_capacity(points.len());
    // Even-odd scan conversion: shared vertices use a half-open vertical interval.
    for y in 0..height as usize {
        let scan=y as f32+0.5;intersections.clear();
        for i in 0..points.len() {
            let a=points[i];let b=points[(i+1)%points.len()];
            if (a[1]<=scan && b[1]>scan) || (b[1]<=scan && a[1]>scan) {intersections.push(a[0]+(scan-a[1])/(b[1]-a[1])*(b[0]-a[0]));}
        }
        intersections.sort_by(f32::total_cmp);
        for pair in intersections.chunks_exact(2) {
            let left=(pair[0]-0.5).ceil().clamp(0.0,width as f32) as usize;let right=(pair[1]-0.5).ceil().clamp(0.0,width as f32) as usize;
            output[y*width as usize+left..y*width as usize+right].fill(255);
        }
    }Ok(output)
}
#[derive(Clone,Debug,uniffi::Record)]
pub struct DrawingAffineSettings {pub scale_x:f32,pub scale_y:f32,pub angle:f32,pub skew_x:f32,pub skew_y:f32,pub translate_x:f32,pub translate_y:f32,pub pivot_x:f32,pub pivot_y:f32}
#[uniffi::export]
pub fn drawing_affine_matrix(s:DrawingAffineSettings)->Result<Vec<f32>,MobileError> {
    if [s.scale_x,s.scale_y,s.angle,s.skew_x,s.skew_y,s.translate_x,s.translate_y,s.pivot_x,s.pivot_y].iter().any(|v|!v.is_finite()) || s.scale_x.abs()<0.01 || s.scale_y.abs()<0.01 || s.scale_x.abs()>8.0 || s.scale_y.abs()>8.0 || s.skew_x.abs()>75.0 || s.skew_y.abs()>75.0 {return Err(MobileError::Command("Invalid affine transform".into()));}
    let a=s.angle.to_radians();let c=a.cos();let t=a.sin();let kx=s.skew_x.to_radians().tan();let ky=s.skew_y.to_radians().tan();
    // Scale, then skew, then rotate about the requested pivot.
    let xx=(c-t*ky)*s.scale_x;let xy=(c*kx-t)*s.scale_y;let yx=(t+c*ky)*s.scale_x;let yy=(t*kx+c)*s.scale_y;
    let tx=s.pivot_x+s.translate_x-xx*s.pivot_x-xy*s.pivot_y;let ty=s.pivot_y+s.translate_y-yx*s.pivot_x-yy*s.pivot_y;
    let matrix=vec![xx,xy,tx,yx,yy,ty,0.0,0.0,1.0];
    if matrix.iter().any(|v|!v.is_finite()) || (xx*yy-xy*yx).abs()<0.000001 {return Err(MobileError::Command("Transform is singular or out of range".into()));}Ok(matrix)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn polygon_rectangle_and_reverse_winding()->Result<(),MobileError> {let a=drawing_polygon_selection(2,2,vec![0.0,0.0,2.0,0.0,2.0,2.0,0.0,2.0])?;let b=drawing_polygon_selection(2,2,vec![0.0,2.0,2.0,2.0,2.0,0.0,0.0,0.0])?;assert_eq!(a,vec![255;4]);assert_eq!(a,b);Ok(())}
    #[test] fn rotation_keeps_pivot_fixed()->Result<(),MobileError> {let m=drawing_affine_matrix(DrawingAffineSettings {scale_x:2.0,scale_y:1.0,angle:90.0,skew_x:0.0,skew_y:0.0,translate_x:0.0,translate_y:0.0,pivot_x:5.0,pivot_y:7.0})?;assert!((m[0]*5.0+m[1]*7.0+m[2]-5.0).abs()<0.001);assert!((m[3]*5.0+m[4]*7.0+m[5]-7.0).abs()<0.001);Ok(())}
}
