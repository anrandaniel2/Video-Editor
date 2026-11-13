//! Versioned native layered drawing persistence. This is FilmCraft's format, not KRA.
//! Untrusted lengths are validated before allocation; Android handles URI permissions only.
use crate::MobileError;
const MAX_PIXELS: u64 = 4_194_304;
const MAX_BYTES: usize = 96 * 1024 * 1024;
const MAGIC: &[u8; 8] = b"FCDRAW03";
#[derive(Clone, Debug, uniffi::Record)]
pub struct DrawingRasterLayer {
    pub name: String,
    pub visible: bool,
    pub opacity: f32,
    pub rgba: Vec<u8>,
    pub blend_mode: u32,
    pub clipped: bool,
    pub locked: bool,
    pub alpha_locked: bool,
    pub mask: Vec<u8>,
    pub mask_enabled: bool,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct DrawingDocumentSnapshot {
    pub width: u32,
    pub height: u32,
    pub active_layer: u32,
    /// Bottom-to-top paint layers. Background reference pictures are not part of this format.
    pub layers: Vec<DrawingRasterLayer>,
}
fn invalid() -> MobileError { MobileError::Command("Invalid or unsupported layered drawing".into()) }
fn dimensions(width: u32, height: u32, count: usize, active: u32) -> Result<usize, MobileError> {
    let pixels = u64::from(width) * u64::from(height);
    if width == 0 || height == 0 || pixels > MAX_PIXELS || count == 0 || count > 16 || active as usize >= count || pixels * 4 * count as u64 > MAX_BYTES as u64 { return Err(invalid()); }
    Ok(pixels as usize * 4)
}
#[uniffi::export]
pub fn drawing_encode_document(document: DrawingDocumentSnapshot) -> Result<Vec<u8>, MobileError> {
    let length = dimensions(document.width, document.height, document.layers.len(), document.active_layer)?;
    for layer in &document.layers {
        if (!layer.mask.is_empty() && layer.mask.len()!=length/4) || layer.blend_mode>=27 || layer.blend_mode==1 || layer.name.len() > 320 || layer.rgba.len() != length || !layer.opacity.is_finite() || !(0.0..=1.0).contains(&layer.opacity) { return Err(invalid()); }
    }
    if document.layers.iter().map(|l|l.rgba.len()+l.mask.len()).sum::<usize>()>MAX_BYTES {return Err(invalid());}
    let mut out = Vec::with_capacity(24 + (length + 327) * document.layers.len());
    out.extend_from_slice(MAGIC);
    for number in [document.width, document.height, document.active_layer, document.layers.len() as u32] { out.extend_from_slice(&number.to_le_bytes()); }
    for layer in document.layers {
        out.extend_from_slice(&(layer.name.len() as u16).to_le_bytes()); out.extend_from_slice(layer.name.as_bytes());
        out.push(u8::from(layer.visible)); out.extend_from_slice(&layer.opacity.to_le_bytes());
        out.extend_from_slice(&layer.blend_mode.to_le_bytes());out.push(u8::from(layer.clipped));out.push(u8::from(layer.locked));out.push(u8::from(layer.alpha_locked));
        out.push(u8::from(!layer.mask.is_empty()));out.push(u8::from(layer.mask_enabled));out.extend_from_slice(&layer.mask);
        out.extend_from_slice(&layer.rgba);
    }
    Ok(out)
}
struct Reader<'a> { bytes: &'a [u8], at: usize }
impl<'a> Reader<'a> {
    fn take(&mut self, length: usize) -> Result<&'a [u8], MobileError> {
        let end = self.at.checked_add(length).ok_or_else(invalid)?;
        let data = self.bytes.get(self.at..end).ok_or_else(invalid)?; self.at = end; Ok(data)
    }
    fn u32(&mut self) -> Result<u32, MobileError> { let b = self.take(4)?; Ok(u32::from_le_bytes([b[0],b[1],b[2],b[3]])) }
}
#[uniffi::export]
pub fn drawing_decode_document(bytes: Vec<u8>) -> Result<DrawingDocumentSnapshot, MobileError> {
    if bytes.len() > MAX_BYTES + 8192 {return Err(invalid());}
    let mut reader = Reader { bytes: &bytes, at: 0 };
    let magic=reader.take(8)?;
    let version=if magic==MAGIC {3} else if magic==b"FCDRAW02" {2} else if magic==b"FCDRAW01" {1} else {return Err(invalid());};
    let width = reader.u32()?; let height = reader.u32()?; let active_layer = reader.u32()?; let count = reader.u32()? as usize;
    let length = dimensions(width,height,count,active_layer)?;
    let mut layers = Vec::with_capacity(count);
    for _ in 0..count {
        let len = reader.take(2)?; let len = u16::from_le_bytes([len[0],len[1]]) as usize;
        if len > 320 {return Err(invalid());}
        let name = std::str::from_utf8(reader.take(len)?).map_err(|_| invalid())?.to_string();
        let visible = match reader.take(1)?[0] {0=>false,1=>true,_=>return Err(invalid())};
        let opacity = f32::from_bits(reader.u32()?);
        if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {return Err(invalid());}
        let (blend_mode,clipped,locked,alpha_locked)=if version>=2 {
            let mode=reader.u32()?;if mode>=27 || mode==1 {return Err(invalid());}
            let mut boolean=||match reader.take(1)?[0] {0=>Ok(false),1=>Ok(true),_=>Err(invalid())};
            (mode,boolean()?,boolean()?,boolean()?)
        } else {(0,false,false,false)};
        let (mask,mask_enabled)=if version>=3 {
            let present=reader.take(1)?[0];let enabled=reader.take(1)?[0];
            if present>1 || enabled>1 {return Err(invalid());}
            (if present==1 {reader.take(length/4)?.to_vec()} else {vec![]},enabled==1)
        } else {(vec![],true)};
        let rgba = reader.take(length)?.to_vec(); layers.push(DrawingRasterLayer {name,visible,opacity,rgba,blend_mode,clipped,locked,alpha_locked,mask,mask_enabled});
    }
    if layers.iter().map(|l|l.rgba.len()+l.mask.len()).sum::<usize>()>MAX_BYTES {return Err(invalid());}
    if reader.at != bytes.len() {return Err(invalid());}
    Ok(DrawingDocumentSnapshot {width,height,active_layer,layers})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn layered_roundtrip() -> Result<(),MobileError> {
        let doc=DrawingDocumentSnapshot {width:1,height:1,active_layer:0,layers:vec![DrawingRasterLayer {name:"Ink α".into(),visible:false,opacity:0.4,rgba:vec![1,2,3,4],blend_mode:3,clipped:true,locked:true,alpha_locked:true,mask:vec![],mask_enabled:true}]};
        let encoded=drawing_encode_document(doc)?;
        let decoded=drawing_decode_document(encoded.clone())?;
        assert_eq!(decoded.layers[0].name,"Ink α");assert_eq!(decoded.layers[0].rgba,vec![1,2,3,4]);assert!(!decoded.layers[0].visible);
        for length in [0,7,23,encoded.len()-1] {assert!(drawing_decode_document(encoded[..length].to_vec()).is_err());}
        let mut trailing=encoded;trailing.push(1);assert!(drawing_decode_document(trailing).is_err());Ok(())
    }
    #[test] fn refuses_invalid_dimensions() {
        assert!(dimensions(u32::MAX,u32::MAX,16,0).is_err());assert!(dimensions(10,10,0,0).is_err());assert!(dimensions(10,10,2,2).is_err());
    }
}

/// Document geometry acts on every paint layer, retaining layer identity metadata and active layer.
/// Scale interpolation is premultiplied-alpha bilinear; canvas/crop/quarter-turns copy exact pixels.
#[uniffi::export]
pub fn drawing_transform_document(mut document: DrawingDocumentSnapshot, operation: String, width: u32, height: u32, offset_x: i32, offset_y: i32) -> Result<DrawingDocumentSnapshot,MobileError> {
    let old_length=dimensions(document.width,document.height,document.layers.len(),document.active_layer)?;
    if document.layers.iter().any(|l|(!l.mask.is_empty() && l.mask.len()!=old_length/4) || l.blend_mode>=27 || l.blend_mode==1 || l.rgba.len()!=old_length || !l.opacity.is_finite() || !(0.0..=1.0).contains(&l.opacity)) {return Err(invalid());}
    let old_w=document.width as usize;let old_h=document.height as usize;
    let (new_w,new_h)=match operation.as_str() {"rotate-cw"|"rotate-ccw"=>(old_h,old_w),"scale"|"canvas"|"crop"=>(width as usize,height as usize),_=>return Err(invalid())};
    let new_length=dimensions(new_w as u32,new_h as u32,document.layers.len(),document.active_layer)?;
    if new_length*document.layers.len()+new_length/4*document.layers.iter().filter(|l|!l.mask.is_empty()).count()>MAX_BYTES {return Err(invalid());}
    if offset_x.unsigned_abs()>16_777_216 || offset_y.unsigned_abs()>16_777_216 {return Err(invalid());}
    for layer in &mut document.layers {
        if !layer.mask.is_empty() {
            let mask_pixels:Vec<u8>=layer.mask.iter().flat_map(|v|[*v,*v,*v,255]).collect();
            let mask_doc=DrawingDocumentSnapshot {width:document.width,height:document.height,active_layer:0,layers:vec![DrawingRasterLayer {name:String::new(),visible:true,opacity:1.0,rgba:mask_pixels,blend_mode:0,clipped:false,locked:false,alpha_locked:false,mask:vec![],mask_enabled:true}]};
            let transformed=drawing_transform_document(mask_doc,operation.clone(),new_w as u32,new_h as u32,offset_x,offset_y)?;
            layer.mask=transformed.layers[0].rgba.chunks_exact(4).map(|p|p[0]).collect();
        }
        let mut output=vec![0u8;new_length];
        for y in 0..new_h {for x in 0..new_w {
            let dst=(y*new_w+x)*4;
            if operation=="scale" {
                let fx=((x as f64+0.5)*old_w as f64/new_w as f64-0.5).clamp(0.0,(old_w-1) as f64);
                let fy=((y as f64+0.5)*old_h as f64/new_h as f64-0.5).clamp(0.0,(old_h-1) as f64);
                let x0=fx.floor() as usize;let y0=fy.floor() as usize;let x1=(x0+1).min(old_w-1);let y1=(y0+1).min(old_h-1);
                let tx=(fx-x0 as f64) as f32;let ty=(fy-y0 as f64) as f32;let mut sum=[0.0f32;4];
                for (xx,yy,weight) in [(x0,y0,(1.0-tx)*(1.0-ty)),(x1,y0,tx*(1.0-ty)),(x0,y1,(1.0-tx)*ty),(x1,y1,tx*ty)] {
                    let i=(yy*old_w+xx)*4;let a=f32::from(layer.rgba[i+3])/255.0;
                    for c in 0..3 {sum[c]+=f32::from(layer.rgba[i+c])*a*weight;}sum[3]+=a*weight;
                }
                for c in 0..3 {output[dst+c]=if sum[3]>0.0 {(sum[c]/sum[3]).round().clamp(0.0,255.0) as u8} else {0};}
                output[dst+3]=(sum[3]*255.0).round().clamp(0.0,255.0) as u8;
            } else {
                let (sx,sy)=match operation.as_str() {
                    "rotate-cw"=>(y as i64,(old_h-1-x) as i64),"rotate-ccw"=>((old_w-1-y) as i64,x as i64),
                    "crop"=>(x as i64+i64::from(offset_x),y as i64+i64::from(offset_y)),
                    _=>(x as i64-i64::from(offset_x),y as i64-i64::from(offset_y)),
                };
                if sx>=0 && sy>=0 && sx<old_w as i64 && sy<old_h as i64 {
                    let src=(sy as usize*old_w+sx as usize)*4;output[dst..dst+4].copy_from_slice(&layer.rgba[src..src+4]);
                }
            }
        }}
        layer.rgba=output;
    }
    document.width=new_w as u32;document.height=new_h as u32;Ok(document)
}

/// Visible-paint alpha bounds; fully transparent documents are rejected instead of silently erased.
#[uniffi::export]
pub fn drawing_document_bounds(document:DrawingDocumentSnapshot)->Result<Vec<u32>,MobileError> {
    let w=document.width as usize;let height=document.height;let width=document.width;
    let pixels=drawing_composite_document(document)?;
    let mut left=width;let mut top=height;let mut right=0;let mut bottom=0;
    for (i,pixel) in pixels.chunks_exact(4).enumerate() {
        if pixel[3]>0 {let x=(i%w) as u32;let y=(i/w) as u32;left=left.min(x);top=top.min(y);right=right.max(x+1);bottom=bottom.max(y+1);}
    }
    if right<=left || bottom<=top {return Err(MobileError::Command("No visible painted pixels to trim".into()));}Ok(vec![left,top,right-left,bottom-top])
}

#[cfg(test)]
mod geometry_tests {
    use super::*;
    fn document()->DrawingDocumentSnapshot {DrawingDocumentSnapshot {width:2,height:1,active_layer:0,layers:vec![DrawingRasterLayer {name:"Ink".into(),visible:true,opacity:1.0,rgba:vec![255,0,0,255,0,255,0,0],blend_mode:0,clipped:false,locked:false,alpha_locked:false,mask:vec![],mask_enabled:true}]}}
    #[test] fn four_quarter_turns_restore_document()->Result<(),MobileError> {
        let original=document();let mut result=original.clone();
        for _ in 0..4 {result=drawing_transform_document(result,"rotate-cw".into(),0,0,0,0)?;}
        assert_eq!(result.width,original.width);assert_eq!(result.height,original.height);assert_eq!(result.layers[0].rgba,original.layers[0].rgba);Ok(())
    }
    #[test] fn scaling_transparency_preserves_red_not_hidden_green()->Result<(),MobileError> {
        let result=drawing_transform_document(document(),"scale".into(),3,1,0,0)?;
        assert_eq!(&result.layers[0].rgba[4..8],&[255,0,0,128]);Ok(())
    }
    #[test] fn trim_uses_visible_alpha()->Result<(),MobileError> {assert_eq!(drawing_document_bounds(document())?,vec![0,0,1,1]);Ok(())}
}

/// Native document compositor. Clipping uses the immediately preceding paint layer's raw alpha,
/// visibility and opacity; it is deliberately not a Krita group/alpha-inheritance implementation.
#[uniffi::export]
pub fn drawing_composite_document(document:DrawingDocumentSnapshot)->Result<Vec<u8>,MobileError> {
    let length=dimensions(document.width,document.height,document.layers.len(),document.active_layer)?;
    if document.layers.iter().any(|l|(!l.mask.is_empty() && l.mask.len()!=length/4) || l.rgba.len()!=length || l.blend_mode>=27 || l.blend_mode==1 || !l.opacity.is_finite() || !(0.0..=1.0).contains(&l.opacity)) {return Err(invalid());}
    let mut output=vec![0;length];
    for (index,layer) in document.layers.iter().enumerate() {
        if !layer.visible {continue;}
        let mut source=layer.rgba.clone();
        if layer.mask_enabled && !layer.mask.is_empty() {for (pixel,mask) in source.chunks_exact_mut(4).zip(layer.mask.iter()) {pixel[3]=((u16::from(pixel[3])*u16::from(*mask)+127)/255) as u8;}}
        if layer.clipped {
            for (i,p) in source.chunks_exact_mut(4).enumerate() {
                let mask=if index>0 {let below=&document.layers[index-1];if below.visible {f32::from(below.rgba[i*4+3])*below.opacity/255.0 * if below.mask_enabled && !below.mask.is_empty() {f32::from(below.mask[i])/255.0} else {1.0}} else {0.0}} else {0.0};
                p[3]=(f32::from(p[3])*mask).round().clamp(0.0,255.0) as u8;
            }
        }
        output=crate::drawing_raster::drawing_merge_layers(output,source,document.width,document.height,1.0,layer.opacity,layer.blend_mode)?;
    }
    Ok(output)
}

#[uniffi::export]
pub fn drawing_flatten_document(document:DrawingDocumentSnapshot)->Result<DrawingDocumentSnapshot,MobileError> {
    let width=document.width;let height=document.height;let rgba=drawing_composite_document(document)?;
    Ok(DrawingDocumentSnapshot {width,height,active_layer:0,layers:vec![DrawingRasterLayer {name:"Flattened paint".into(),visible:true,opacity:1.0,rgba,blend_mode:0,clipped:false,locked:false,alpha_locked:false,mask:vec![],mask_enabled:true}]})
}

#[cfg(test)]
mod layer_property_tests {
    use super::*;
    fn layer(rgba:Vec<u8>)->DrawingRasterLayer {DrawingRasterLayer {name:"Paint".into(),visible:true,opacity:1.0,rgba,blend_mode:0,clipped:false,locked:false,alpha_locked:false,mask:vec![],mask_enabled:true}}
    #[test] fn version_one_documents_still_open()->Result<(),MobileError> {
        let mut bytes=b"FCDRAW01".to_vec();for value in [1u32,1,0,1] {bytes.extend_from_slice(&value.to_le_bytes());}
        bytes.extend_from_slice(&0u16.to_le_bytes());bytes.push(1);bytes.extend_from_slice(&1.0f32.to_le_bytes());bytes.extend_from_slice(&[255,0,0,255]);
        let doc=drawing_decode_document(bytes)?;assert_eq!(doc.layers[0].blend_mode,0);assert!(!doc.layers[0].locked);assert!(!doc.layers[0].clipped);Ok(())
    }
    #[test] fn clipping_and_multiply_are_native()->Result<(),MobileError> {
        let lower=layer(vec![100,200,50,255]);let mut upper=layer(vec![128,128,128,255]);upper.blend_mode=3;upper.clipped=true;
        let doc=DrawingDocumentSnapshot {width:1,height:1,active_layer:1,layers:vec![lower,upper]};
        assert_eq!(drawing_composite_document(doc)?,vec![50,100,25,255]);Ok(())
    }
    #[test] fn clipping_to_hidden_layer_is_empty()->Result<(),MobileError> {
        let mut lower=layer(vec![255;4]);lower.visible=false;let mut upper=layer(vec![255;4]);upper.clipped=true;
        let doc=DrawingDocumentSnapshot {width:1,height:1,active_layer:1,layers:vec![lower,upper]};
        assert_eq!(drawing_composite_document(doc)?,vec![0;4]);Ok(())
    }
    #[test] fn version_two_preserves_layer_controls()->Result<(),MobileError> {
        let mut item=layer(vec![1,2,3,128]);item.alpha_locked=true;item.locked=true;item.clipped=true;item.blend_mode=13;
        let doc=DrawingDocumentSnapshot {width:1,height:1,active_layer:0,layers:vec![item]};
        let decoded=drawing_decode_document(drawing_encode_document(doc)?)?;
        assert!(decoded.layers[0].alpha_locked);assert!(decoded.layers[0].locked);assert!(decoded.layers[0].clipped);assert_eq!(decoded.layers[0].blend_mode,13);Ok(())
    }
}

#[cfg(test)]
mod mask_document_tests {
    use super::*;
    fn masked()->DrawingDocumentSnapshot {DrawingDocumentSnapshot {width:2,height:1,active_layer:0,layers:vec![DrawingRasterLayer {name:"Masked".into(),visible:true,opacity:1.0,rgba:vec![255,0,0,255,0,255,0,255],blend_mode:0,clipped:false,locked:false,alpha_locked:false,mask:vec![255,0],mask_enabled:true}]}}
    #[test] fn mask_roundtrip_and_composite()->Result<(),MobileError> {
        let decoded=drawing_decode_document(drawing_encode_document(masked())?)?;
        assert_eq!(decoded.layers[0].mask,vec![255,0]);
        assert_eq!(drawing_composite_document(decoded)?,vec![255,0,0,255,0,0,0,0]);Ok(())
    }
    #[test] fn disabled_mask_keeps_all_paint_visible()->Result<(),MobileError> {
        let mut doc=masked();doc.layers[0].mask_enabled=false;
        assert_eq!(drawing_composite_document(doc)?,vec![255,0,0,255,0,255,0,255]);Ok(())
    }
    #[test] fn quarter_turn_transforms_mask_too()->Result<(),MobileError> {
        let doc=drawing_transform_document(masked(),"rotate-cw".into(),0,0,0,0)?;
        assert_eq!(doc.width,1);assert_eq!(doc.height,2);assert_eq!(doc.layers[0].mask,vec![255,0]);Ok(())
    }
    #[test] fn mismatched_mask_is_rejected() {
        let mut doc=masked();doc.layers[0].mask=vec![255];assert!(drawing_encode_document(doc).is_err());
    }
}
