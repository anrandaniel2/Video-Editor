//! Bounded OpenRaster flat paint-stack interchange. Unsupported semantics fail explicitly.
use std::io::{Cursor,Read,Write};
use crate::{MobileError,drawing_document::{DrawingDocumentSnapshot,DrawingRasterLayer,drawing_encode_document,drawing_composite_document}};
const LIMIT:usize=96*1024*1024;
const MODES:&[(u32,&str)]=&[(0,"svg:src-over"),(2,"svg:darken"),(3,"svg:multiply"),(4,"svg:color-burn"),(7,"svg:lighten"),(8,"svg:screen"),(9,"svg:color-dodge"),(12,"svg:overlay"),(13,"svg:soft-light"),(14,"svg:hard-light"),(19,"svg:difference"),(20,"svg:exclusion"),(23,"svg:hue"),(24,"svg:saturation"),(25,"svg:color"),(26,"svg:luminosity")];
fn err(s:&str)->MobileError {MobileError::Command(format!("OpenRaster: {s}"))}
fn escape(s:&str)->String {s.replace('&',"&amp;").replace('<',"&lt;").replace('>',"&gt;").replace('"',"&quot;").replace('\'',"&apos;")}
fn encode_png(width:u32,height:u32,rgba:&[u8])->Result<Vec<u8>,MobileError> {
    let mut out=Vec::new();{let mut encoder=png::Encoder::new(&mut out,width,height);encoder.set_color(png::ColorType::Rgba);encoder.set_depth(png::BitDepth::Eight);let mut writer=encoder.write_header().map_err(|e|err(&e.to_string()))?;writer.write_image_data(rgba).map_err(|e|err(&e.to_string()))?;}Ok(out)
}
fn member(archive:&mut zip::ZipArchive<Cursor<&[u8]>>,name:&str,limit:usize)->Result<Vec<u8>,MobileError> {
    let file=archive.by_name(name).map_err(|_|err("Missing archive member"))?;
    if file.size()>limit as u64 {return Err(err("Archive member too large"));}
    let mut bytes=Vec::new();file.take(limit as u64+1).read_to_end(&mut bytes).map_err(|e|err(&e.to_string()))?;if bytes.len()>limit {return Err(err("Decompressed archive member too large"));}Ok(bytes)
}
#[uniffi::export]
pub fn drawing_encode_openraster(document:DrawingDocumentSnapshot)->Result<Vec<u8>,MobileError> {
    drop(drawing_encode_document(document.clone())?);
    if document.layers.iter().any(|l|l.clipped || l.locked || l.alpha_locked || !l.mask.is_empty() || !MODES.iter().any(|(id,_)|*id==l.blend_mode) || l.name.chars().any(|c|c<' ' && c!='\t' && c!='\n' && c!='\r')) {return Err(err("Export supports flat paint layers without masks, clipping or locks. Save .fcdraw to retain these features."));}
    let mut archive=zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options=zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    archive.start_file("mimetype",options).map_err(|e|err(&e.to_string()))?;archive.write_all(b"image/openraster").map_err(|e|err(&e.to_string()))?;
    let mut xml=format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><image version=\"0.0.3\" w=\"{}\" h=\"{}\" name=\"Drawing\"><stack>",document.width,document.height);
    let mut payload=0usize;
    for (index,layer) in document.layers.iter().enumerate().rev() {
        let src=format!("data/layer{index}.png");let mode=MODES.iter().find(|(id,_)|*id==layer.blend_mode).ok_or_else(||err("Unsupported blend"))?.1;
        xml.push_str(&format!("<layer name=\"{}\" src=\"{}\" x=\"0\" y=\"0\" opacity=\"{}\" visibility=\"{}\" composite-op=\"{}\" selected=\"{}\"/>",escape(&layer.name),src,layer.opacity,if layer.visible {"visible"} else {"hidden"},mode,if index==document.active_layer as usize {"true"} else {"false"}));
        let data=encode_png(document.width,document.height,&layer.rgba)?;payload+=data.len();if payload>LIMIT {return Err(err("Encoded layers exceed archive budget"));}
        archive.start_file(src,options).map_err(|e|err(&e.to_string()))?;archive.write_all(&data).map_err(|e|err(&e.to_string()))?;
    }
    xml.push_str("</stack></image>");archive.start_file("stack.xml",options).map_err(|e|err(&e.to_string()))?;archive.write_all(xml.as_bytes()).map_err(|e|err(&e.to_string()))?;
    let composite=drawing_composite_document(document.clone())?;
    let tw=document.width.min(256);let th=((u64::from(document.height)*u64::from(tw)/u64::from(document.width)).max(1)) as u32;
    let (tw,th)=if th>256 {(((u64::from(tw)*256/u64::from(th)).max(1)) as u32,256)} else {(tw,th)};
    let mut thumbnail=Vec::with_capacity(tw as usize*th as usize*4);
    for y in 0..th {for x in 0..tw {let sx=u64::from(x)*u64::from(document.width)/u64::from(tw);let sy=u64::from(y)*u64::from(document.height)/u64::from(th);let index=(sy as usize*document.width as usize+sx as usize)*4;thumbnail.extend_from_slice(&composite[index..index+4]);}}
    let thumbnail=encode_png(tw,th,&thumbnail)?;
    archive.start_file("Thumbnails/thumbnail.png",options).map_err(|e|err(&e.to_string()))?;archive.write_all(&thumbnail).map_err(|e|err(&e.to_string()))?;
    let merged=encode_png(document.width,document.height,&composite)?;
    if payload+merged.len()+thumbnail.len()+xml.len()+16384>LIMIT+1024*1024 {return Err(err("Archive exceeds file budget"));}
    archive.start_file("mergedimage.png",options).map_err(|e|err(&e.to_string()))?;archive.write_all(&merged).map_err(|e|err(&e.to_string()))?;
    Ok(archive.finish().map_err(|e|err(&e.to_string()))?.into_inner())
}
#[uniffi::export]
pub fn drawing_decode_openraster(bytes:Vec<u8>)->Result<DrawingDocumentSnapshot,MobileError> {
    if bytes.len()>LIMIT+1024*1024 {return Err(err("Archive too large"));}
    let mut archive=zip::ZipArchive::new(Cursor::new(bytes.as_slice())).map_err(|e|err(&e.to_string()))?;
    if archive.len()>128 {return Err(err("Too many archive members"));}
    let mut names=std::collections::HashSet::new();for index in 0..archive.len() {let f=archive.by_index(index).map_err(|e|err(&e.to_string()))?;if !names.insert(f.name().to_owned()) {return Err(err("Duplicate archive member"));}}
    if member(&mut archive,"mimetype",64)?!=b"image/openraster" {return Err(err("Invalid MIME declaration"));}
    let xml=member(&mut archive,"stack.xml",64*1024)?;let xml=std::str::from_utf8(&xml).map_err(|_|err("stack.xml must be UTF-8"))?;
    // roxmltree rejects DTDs by default; no network or filesystem entity resolution.
    let tree=roxmltree::Document::parse(xml).map_err(|e|err(&e.to_string()))?;let root=tree.root_element();
    if root.tag_name().name()!="image" {return Err(err("Missing image element"));}
    let dimension=|key:&str|->Result<u32,MobileError> {root.attribute(key).ok_or_else(||err("Missing canvas dimension"))?.parse().map_err(|_|err("Invalid canvas dimension"))};let width=dimension("w")?;let height=dimension("h")?;
    let pixels=u64::from(width)*u64::from(height);if width==0 || height==0 || pixels>4_194_304 {return Err(err("Canvas exceeds raster limits"));}
    let stacks=root.children().filter(|n|n.is_element()).collect::<Vec<_>>();if stacks.len()!=1 || stacks[0].tag_name().name()!="stack" {return Err(err("Expected one root paint stack"));}
    let stack=stacks[0];if stack.attribute("opacity").is_some_and(|v|v!="1" && v!="1.0") || stack.attribute("visibility").is_some_and(|v|v!="visible") || stack.attribute("composite-op").is_some_and(|v|v!="svg:src-over") {return Err(err("Root stack effects are unsupported"));}
    let nodes=stack.children().filter(|n|n.is_element()).collect::<Vec<_>>();if nodes.is_empty() || nodes.len()>16 || pixels*4*nodes.len() as u64>LIMIT as u64 {return Err(err("Layer count or pixel budget exceeded"));}
    let mut layers=Vec::new();let mut active=0;
    for node in nodes.into_iter().rev() {
        if node.tag_name().name()!="layer" || node.children().any(|n|n.is_element()) {return Err(err("Groups, vector layers and masks require a richer importer"));}
        for attribute in node.attributes() {
            if !matches!(attribute.name(),"name"|"src"|"x"|"y"|"opacity"|"visibility"|"composite-op"|"selected") {
                return Err(err("Unsupported layer metadata; import would discard layer semantics"));
            }
        }
        let name=node.attribute("name").unwrap_or("Paint").to_owned();if name.len()>320 {return Err(err("Layer name too long"));}
        let mode=node.attribute("composite-op").unwrap_or("svg:src-over");let blend_mode=MODES.iter().find(|(_,v)|*v==mode).ok_or_else(||err("Unsupported layer blend mode"))?.0;
        let opacity:f32=node.attribute("opacity").unwrap_or("1").parse().map_err(|_|err("Invalid layer opacity"))?;if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {return Err(err("Invalid layer opacity"));}
        let visible=match node.attribute("visibility").unwrap_or("visible") {"visible"=>true,"hidden"=>false,_=>return Err(err("Invalid visibility"))};
        let coord=|key:&str|->Result<i64,MobileError> {node.attribute(key).unwrap_or("0").parse().map_err(|_|err("Invalid layer offset"))};let x=coord("x")?;let y=coord("y")?;if x.unsigned_abs()>16_777_216 || y.unsigned_abs()>16_777_216 {return Err(err("Layer offset out of range"));}
        let src=node.attribute("src").ok_or_else(||err("Missing layer image"))?;if src.starts_with('/') || src.contains('\\') || src.split('/').any(|s|s==".." || s.is_empty()) || !src.ends_with(".png") {return Err(err("Unsafe layer image path"));}
        let data=member(&mut archive,src,32*1024*1024)?;let mut decoder=png::Decoder::new(Cursor::new(data));decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);decoder.set_limits(png::Limits {bytes:32*1024*1024});let mut reader=decoder.read_info().map_err(|e|err(&e.to_string()))?;
        let iw=reader.info().width;let ih=reader.info().height;if iw==0 || ih==0 || u64::from(iw)*u64::from(ih)>4_194_304 || reader.info().animation_control.is_some() {return Err(err("Unsupported layer image size or animation"));}
        if reader.info().bit_depth==png::BitDepth::Sixteen {return Err(err("16-bit OpenRaster import is not supported by the byte editor"));}
        let size=reader.output_buffer_size().ok_or_else(||err("Invalid PNG buffer"))?;if size>32*1024*1024 {return Err(err("PNG output too large"));}let mut raw=vec![0;size];let info=reader.next_frame(&mut raw).map_err(|e|err(&e.to_string()))?;
        let channels=match info.color_type {png::ColorType::Rgba=>4,png::ColorType::Rgb=>3,png::ColorType::GrayscaleAlpha=>2,png::ColorType::Grayscale=>1,_=>return Err(err("Unsupported PNG colour type"))};
        let mut rgba=vec![0;pixels as usize*4];for py in 0..ih {for px in 0..iw {let dx=x+i64::from(px);let dy=y+i64::from(py);if dx<0 || dy<0 || dx>=i64::from(width) || dy>=i64::from(height) {continue;}let i=(py as usize*iw as usize+px as usize)*channels;let dst=(dy as usize*width as usize+dx as usize)*4;let p=match channels {4=>[raw[i],raw[i+1],raw[i+2],raw[i+3]],3=>[raw[i],raw[i+1],raw[i+2],255],2=>[raw[i],raw[i],raw[i],raw[i+1]],_=>[raw[i],raw[i],raw[i],255]};rgba[dst..dst+4].copy_from_slice(&p);}}
        if node.attribute("selected")==Some("true") {active=layers.len() as u32;}
        layers.push(DrawingRasterLayer {name,visible,opacity,rgba,blend_mode,clipped:false,locked:false,alpha_locked:false,mask:vec![],mask_enabled:true});
    }
    Ok(DrawingDocumentSnapshot {width,height,active_layer:active,layers})
}
#[cfg(test)]
mod tests {
    use super::*;
    fn document()->DrawingDocumentSnapshot {DrawingDocumentSnapshot {width:1,height:1,active_layer:0,layers:vec![DrawingRasterLayer {name:"A & \"B\"".into(),visible:true,opacity:0.5,rgba:vec![20,30,40,128],blend_mode:3,clipped:false,locked:false,alpha_locked:false,mask:vec![],mask_enabled:true}]}}
    #[test] fn roundtrip()->Result<(),MobileError> {let original=document();let data=drawing_encode_openraster(original.clone())?;let restored=drawing_decode_openraster(data)?;assert_eq!(restored.layers[0].rgba,original.layers[0].rgba);assert_eq!(restored.layers[0].name,original.layers[0].name);assert_eq!(restored.layers[0].blend_mode,3);assert_eq!(restored.layers[0].opacity,0.5);Ok(())}
    #[test] fn reject_lossy_export() {let mut d=document();d.layers[0].mask=vec![128];assert!(drawing_encode_openraster(d).is_err());}
    #[test] fn reject_invalid_archive() {assert!(drawing_decode_openraster(vec![0;32]).is_err());}
}

#[cfg(test)]
mod import_tests {
    use super::*;
    fn archive(xml:&str)->Result<Vec<u8>,MobileError> {
        let mut out=zip::ZipWriter::new(Cursor::new(Vec::new()));let options=zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name,data) in [("mimetype",b"image/openraster".to_vec()),("stack.xml",xml.as_bytes().to_vec()),("data/a.png",encode_png(2,1,&[255,0,0,255,0,255,0,128])?)] {
            out.start_file(name,options).map_err(|e|err(&e.to_string()))?;out.write_all(&data).map_err(|e|err(&e.to_string()))?;
        }Ok(out.finish().map_err(|e|err(&e.to_string()))?.into_inner())
    }
    #[test] fn negative_offset_clips_to_canvas()->Result<(),MobileError> {
        let data=archive("<image w=\"1\" h=\"1\"><stack><layer src=\"data/a.png\" x=\"-1\"/></stack></image>")?;
        assert_eq!(drawing_decode_openraster(data)?.layers[0].rgba,vec![0,255,0,128]);Ok(())
    }
    #[test] fn rejects_groups_traversal_and_unknown_semantics()->Result<(),MobileError> {
        for content in ["<stack><layer src=\"data/a.png\"/></stack>","<layer src=\"../a.png\"/>","<layer src=\"data/a.png\" composite-op=\"custom:blend\"/>","<layer src=\"data/a.png\" edit-locked=\"true\"/>"] {
            assert!(drawing_decode_openraster(archive(&format!("<image w=\"1\" h=\"1\"><stack>{content}</stack></image>"))?).is_err());
        }Ok(())
    }
}
