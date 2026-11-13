//! Version-four editor persistence: legacy-compatible raster preview plus optional native float layers.
//! Preview correspondence is validated so native caches cannot silently replace unrelated pixels.
use std::sync::Arc;
use crate::{MobileError,drawing_document::{DrawingDocumentSnapshot,drawing_encode_document,drawing_decode_document},drawing_tiles::{DrawingTiledDocument,drawing_decode_tiled}};
const LIMIT:usize=96*1024*1024+8192;
#[derive(uniffi::Record)]
pub struct DrawingPrecisionDocument {pub document:DrawingDocumentSnapshot,pub native_layers:Vec<Option<Arc<DrawingTiledDocument>>>}
fn err()->MobileError {MobileError::Command("Invalid or oversized precision drawing".into())}
fn validate_layer(document:&DrawingDocumentSnapshot,index:usize,tiles:&DrawingTiledDocument)->Result<(),MobileError> {
    let info=tiles.info()?;
    if info.width!=document.width || info.height!=document.height || info.layers.len()!=1 || info.layers[0].id!=1 || !info.layers[0].visible || info.layers[0].opacity!=1.0 {return Err(err());}
    // Android's ARGB_8888 caches quantize premultiplication. Compare premultiplied bytes
    // within one code value, while requiring exactly matching alpha.
    let projected=tiles.layer_srgb(1)?;
    if projected.chunks_exact(4).zip(document.layers[index].rgba.chunks_exact(4)).any(|(a,b)|a[3]!=b[3] || (0..3).any(|c|(i32::from(a[c])*i32::from(a[3])-i32::from(b[c])*i32::from(b[3])).abs()>255)) {return Err(err());}
    Ok(())
}
#[uniffi::export]
pub fn drawing_encode_precision(document:DrawingDocumentSnapshot,native_layers:Vec<Option<Arc<DrawingTiledDocument>>>)->Result<Vec<u8>,MobileError> {
    if native_layers.len()!=document.layers.len() {return Err(err());}
    if native_layers.iter().all(|v|v.is_none()) {return drawing_encode_document(document);}
    let preview=drawing_encode_document(document.clone())?;let mut out=b"FCDRAW04".to_vec();out.extend_from_slice(&(preview.len() as u32).to_le_bytes());out.extend(preview);out.extend_from_slice(&(native_layers.len() as u32).to_le_bytes());
    for (index,native) in native_layers.iter().enumerate() {
        let data=if let Some(native)=native {validate_layer(&document,index,native)?;native.encode()?} else {vec![]};
        if out.len().saturating_add(data.len()).saturating_add(4)>LIMIT {return Err(err());}out.extend_from_slice(&(data.len() as u32).to_le_bytes());out.extend(data);
    }Ok(out)
}
#[uniffi::export]
pub fn drawing_decode_precision(bytes:Vec<u8>)->Result<DrawingPrecisionDocument,MobileError> {
    if !bytes.starts_with(b"FCDRAW04") {let document=drawing_decode_document(bytes)?;let native_layers=vec![None;document.layers.len()];return Ok(DrawingPrecisionDocument {document,native_layers});}
    if bytes.len()>LIMIT {return Err(err());}
    struct Reader<'a> {bytes:&'a [u8],at:usize}
    impl<'a> Reader<'a> {
        fn take(&mut self,len:usize)->Result<&'a [u8],MobileError> {let end=self.at.checked_add(len).ok_or_else(err)?;let value=self.bytes.get(self.at..end).ok_or_else(err)?;self.at=end;Ok(value)}
        fn number(&mut self)->Result<usize,MobileError> {let b=self.take(4)?;Ok(u32::from_le_bytes([b[0],b[1],b[2],b[3]]) as usize)}
    }
    let mut reader=Reader {bytes:&bytes,at:8};let length=reader.number()?;let document=drawing_decode_document(reader.take(length)?.to_vec())?;let count=reader.number()?;if count!=document.layers.len() {return Err(err());}
    let mut native_layers=Vec::with_capacity(count);let mut live=0u64;
    for index in 0..count {let length=reader.number()?;let native=if length==0 {None} else {
        let native=drawing_decode_tiled(reader.take(length)?.to_vec())?;live=live.checked_add(native.info()?.live_bytes).ok_or_else(err)?;if live>192*1024*1024 {return Err(err());}validate_layer(&document,index,&native)?;Some(native)
    };native_layers.push(native);}
    if reader.at!=bytes.len() {return Err(err());}Ok(DrawingPrecisionDocument {document,native_layers})
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawing_document::DrawingRasterLayer;
    fn preview(rgba:Vec<u8>)->DrawingDocumentSnapshot {DrawingDocumentSnapshot {width:1,height:1,active_layer:0,layers:vec![DrawingRasterLayer {name:"Paint".into(),visible:true,opacity:1.0,rgba,blend_mode:0,clipped:false,locked:false,alpha_locked:false,mask:vec![],mask_enabled:true}]}}
    #[test] fn hdr_precision_roundtrip_and_mismatch()->Result<(),MobileError> {
        let tiles=DrawingTiledDocument::new(1,1)?;tiles.write_region(1,0,0,1,1,vec![4.0,0.0,0.0,1.0])?;
        let document=preview(tiles.layer_srgb(1)?);let encoded=drawing_encode_precision(document,vec![Some(tiles.clone())])?;
        let reopened=drawing_decode_precision(encoded.clone())?;let native=reopened.native_layers[0].as_ref().ok_or_else(err)?;assert_eq!(native.read_region(1,0,0,1,1)?,vec![4.0,0.0,0.0,1.0]);
        assert!(drawing_encode_precision(preview(vec![0;4]),vec![Some(tiles)]).is_err());assert!(drawing_decode_precision(encoded[..encoded.len()-1].to_vec()).is_err());Ok(())
    }
    #[test] fn forks_are_independent()->Result<(),MobileError> {let tiles=DrawingTiledDocument::new(1,1)?;let fork=tiles.fork()?;fork.write_region(1,0,0,1,1,vec![1.0;4])?;assert_eq!(tiles.layer_srgb(1)?,vec![0;4]);assert_eq!(fork.layer_srgb(1)?,vec![255;4]);Ok(())}
}
