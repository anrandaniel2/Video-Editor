//! Native flipbook document state, exposure timing and structural undo. Not Krita animation parity.
use std::{collections::{HashSet,VecDeque},sync::{Arc,Mutex}};
use crate::{MobileError,drawing_document::{DrawingDocumentSnapshot,drawing_decode_document,drawing_encode_document}};
const DOCUMENT_BUDGET:usize=96*1024*1024;
const HISTORY_BUDGET:usize=192*1024*1024;
#[derive(Clone)]
struct Frame {document:Arc<DrawingDocumentSnapshot>,name:String,exposure:u32}
#[derive(Clone)]
struct State {frames:Vec<Frame>,active:usize,fps:u32}
struct Session {state:State,past:VecDeque<State>,future:VecDeque<State>}
#[derive(uniffi::Record)]
pub struct DrawingAnimationFrameInfo {pub name:String,pub exposure:u32}
#[derive(uniffi::Record)]
pub struct DrawingAnimationInfo {pub frames:Vec<DrawingAnimationFrameInfo>,pub active:u32,pub fps:u32,pub total_ticks:u32,pub can_undo:bool,pub can_redo:bool}
#[derive(uniffi::Object)]
pub struct DrawingAnimation {inner:Mutex<Session>}
fn error(message:&str)->MobileError {MobileError::Command(message.into())}
fn bytes(doc:&DrawingDocumentSnapshot)->usize {doc.layers.iter().map(|l|l.rgba.len()+l.mask.len()).sum()}
fn valid(doc:&DrawingDocumentSnapshot)->Result<(),MobileError> {
    let pixels=u64::from(doc.width)*u64::from(doc.height);
    if doc.width==0 || doc.height==0 || pixels>4_194_304 || doc.layers.is_empty() || doc.layers.len()>16 || doc.active_layer as usize>=doc.layers.len() || bytes(doc)>DOCUMENT_BUDGET || doc.layers.iter().any(|l|l.rgba.len() as u64!=pixels*4 || (!l.mask.is_empty() && l.mask.len() as u64!=pixels) || !l.opacity.is_finite() || !(0.0..=1.0).contains(&l.opacity) || l.blend_mode>=27 || l.blend_mode==1 || l.name.len()>320) {return Err(error("Invalid animation frame document"));}Ok(())
}
fn usage<'a>(states:impl Iterator<Item=&'a State>)->usize {
    let mut seen=HashSet::new();let mut total=0;
    for state in states {for frame in &state.frames {if seen.insert(Arc::as_ptr(&frame.document) as usize) {total+=bytes(&frame.document);}}}total
}
fn same(a:&DrawingDocumentSnapshot,b:&DrawingDocumentSnapshot)->bool {
    a.width==b.width && a.height==b.height && a.active_layer==b.active_layer && a.layers.len()==b.layers.len() && a.layers.iter().zip(&b.layers).all(|(a,b)|a.name==b.name && a.visible==b.visible && a.opacity==b.opacity && a.rgba==b.rgba && a.blend_mode==b.blend_mode && a.clipped==b.clipped && a.locked==b.locked && a.alpha_locked==b.alpha_locked && a.mask==b.mask && a.mask_enabled==b.mask_enabled)
}
impl Session {
    fn change(&mut self,next:State)->Result<(),MobileError> {
        if next.frames.is_empty() || next.frames.len()>120 || next.active>=next.frames.len() || usage(std::iter::once(&next))>DOCUMENT_BUDGET {return Err(error("Animation exceeds 120 frames or its 96 MiB unique-pixel budget"));}
        self.past.push_back(self.state.clone());self.future.clear();self.state=next;
        while self.past.len()>32 || (!self.past.is_empty() && usage(std::iter::once(&self.state).chain(self.past.iter()))>HISTORY_BUDGET) {self.past.pop_front();}Ok(())
    }
}
#[uniffi::export]
impl DrawingAnimation {
    #[uniffi::constructor]
    pub fn new(document:DrawingDocumentSnapshot)->Result<Arc<Self>,MobileError> {
        valid(&document)?;Ok(Arc::new(Self {inner:Mutex::new(Session {state:State {frames:vec![Frame {document:Arc::new(document),name:"Frame 1".into(),exposure:1}],active:0,fps:12},past:VecDeque::new(),future:VecDeque::new()})}))
    }
    pub fn info(&self)->Result<DrawingAnimationInfo,MobileError> {
        let s=self.inner.lock().map_err(|_|error("Animation state unavailable"))?;
        Ok(DrawingAnimationInfo {frames:s.state.frames.iter().map(|f|DrawingAnimationFrameInfo {name:f.name.clone(),exposure:f.exposure}).collect(),active:s.state.active as u32,fps:s.state.fps,total_ticks:s.state.frames.iter().map(|f|f.exposure).sum(),can_undo:!s.past.is_empty(),can_redo:!s.future.is_empty()})
    }
    pub fn current(&self)->Result<DrawingDocumentSnapshot,MobileError> {let s=self.inner.lock().map_err(|_|error("Animation state unavailable"))?;Ok((*s.state.frames[s.state.active].document).clone())}
    pub fn frame(&self,index:u32)->Result<DrawingDocumentSnapshot,MobileError> {let s=self.inner.lock().map_err(|_|error("Animation state unavailable"))?;Ok((**s.state.frames.get(index as usize).map(|f|&f.document).ok_or_else(||error("Unknown animation frame"))?).clone())}
    pub fn update_current(&self,document:DrawingDocumentSnapshot)->Result<(),MobileError> {
        valid(&document)?;let mut s=self.inner.lock().map_err(|_|error("Animation state unavailable"))?;
        let old=&s.state.frames[s.state.active].document;
        if document.width!=old.width || document.height!=old.height {return Err(error("Frame dimensions changed. Start a new flipbook after document resizing."));}
        if same(old,&document) {return Ok(());}let mut next=s.state.clone();next.frames[next.active].document=Arc::new(document);s.change(next)
    }
    pub fn select(&self,index:u32)->Result<(),MobileError> {let mut s=self.inner.lock().map_err(|_|error("Animation state unavailable"))?;if index as usize>=s.state.frames.len() {return Err(error("Unknown animation frame"));}s.state.active=index as usize;Ok(())}
    pub fn insert(&self,duplicate:bool)->Result<(),MobileError> {
        let mut s=self.inner.lock().map_err(|_|error("Animation state unavailable"))?;let mut next=s.state.clone();let source=&next.frames[next.active];
        let document=if duplicate {source.document.clone()} else {let mut document=(*source.document).clone();for l in &mut document.layers {l.rgba.fill(0);l.mask.clear();l.mask_enabled=true;}Arc::new(document)};
        let index=next.active+1;next.frames.insert(index,Frame {document,name:format!("Frame {}",next.frames.len()+1),exposure:1});next.active=index;s.change(next)
    }
    pub fn delete_current(&self)->Result<(),MobileError> {let mut s=self.inner.lock().map_err(|_|error("Animation state unavailable"))?;if s.state.frames.len()==1 {return Err(error("Keep at least one animation frame"));}let mut next=s.state.clone();next.frames.remove(next.active);next.active=next.active.min(next.frames.len()-1);s.change(next)}
    pub fn move_current(&self,offset:i32)->Result<(),MobileError> {let mut s=self.inner.lock().map_err(|_|error("Animation state unavailable"))?;let mut next=s.state.clone();let index=next.active as i64+i64::from(offset);if index<0 || index>=next.frames.len() as i64 {return Err(error("Frame move outside timeline"));}let frame=next.frames.remove(next.active);next.active=index as usize;next.frames.insert(next.active,frame);s.change(next)}
    pub fn configure(&self,name:String,exposure:u32,fps:u32)->Result<(),MobileError> {if name.len()>320 || !(1..=120).contains(&exposure) || !(1..=60).contains(&fps) {return Err(error("Invalid frame name, exposure or frame rate"));}let mut s=self.inner.lock().map_err(|_|error("Animation state unavailable"))?;let mut next=s.state.clone();let frame=&mut next.frames[next.active];frame.name=name;frame.exposure=exposure;next.fps=fps;s.change(next)}
    pub fn undo(&self)->Result<(),MobileError> {let mut s=self.inner.lock().map_err(|_|error("Animation state unavailable"))?;if let Some(previous)=s.past.pop_back() {let current=s.state.clone();s.future.push_back(current);s.state=previous;}Ok(())}
    pub fn redo(&self)->Result<(),MobileError> {let mut s=self.inner.lock().map_err(|_|error("Animation state unavailable"))?;if let Some(next)=s.future.pop_back() {let current=s.state.clone();s.past.push_back(current);s.state=next;}Ok(())}
    pub fn frame_at_tick(&self,tick:u32,looped:bool)->Result<u32,MobileError> {let s=self.inner.lock().map_err(|_|error("Animation state unavailable"))?;let total:u32=s.state.frames.iter().map(|f|f.exposure).sum();let mut tick=if looped {tick%total} else {tick.min(total-1)};for (index,frame) in s.state.frames.iter().enumerate() {if tick<frame.exposure {return Ok(index as u32);}tick-=frame.exposure;}Err(error("Invalid exposure timeline"))}
    /// Full-frame APNG with exact exposure/fps fractions and alpha replacement, not accumulated ghosts.
    pub fn encode_apng(&self,looped:bool)->Result<Vec<u8>,MobileError> {
        struct Bounded(Vec<u8>);
        impl std::io::Write for Bounded {
            fn write(&mut self,bytes:&[u8])->std::io::Result<usize> {
                if self.0.len().saturating_add(bytes.len())>DOCUMENT_BUDGET {return Err(std::io::Error::new(std::io::ErrorKind::InvalidData,"APNG exceeds 96 MiB export limit"));}
                self.0.extend_from_slice(bytes);Ok(bytes.len())
            }
            fn flush(&mut self)->std::io::Result<()> {Ok(())}
        }
        // Share immutable snapshots while exporting; release session lock before compression.
        let state=self.inner.lock().map_err(|_|error("Animation state unavailable"))?.state.clone();
        let first=&state.frames[0].document;let mut output=Bounded(Vec::new());
        {
            let mut encoder=png::Encoder::new(&mut output,first.width,first.height);
            encoder.set_color(png::ColorType::Rgba);encoder.set_depth(png::BitDepth::Eight);
            encoder.set_animated(state.frames.len() as u32,if looped {0} else {1}).map_err(|e|error(&e.to_string()))?;
            encoder.set_blend_op(png::BlendOp::Source).map_err(|e|error(&e.to_string()))?;
            encoder.set_dispose_op(png::DisposeOp::None).map_err(|e|error(&e.to_string()))?;
            let mut writer=encoder.write_header().map_err(|e|error(&e.to_string()))?;
            for frame in &state.frames {
                writer.set_frame_delay(frame.exposure as u16,state.fps as u16).map_err(|e|error(&e.to_string()))?;
                let pixels=crate::drawing_document::drawing_composite_document((*frame.document).clone())?;
                writer.write_image_data(&pixels).map_err(|e|error(&e.to_string()))?;
            }
            writer.finish().map_err(|e|error(&e.to_string()))?;
        }Ok(output.0)
    }
    pub fn encode(&self)->Result<Vec<u8>,MobileError> {
        let s=self.inner.lock().map_err(|_|error("Animation state unavailable"))?;
        let mut out=b"FCANIM01".to_vec();for value in [s.state.fps,s.state.active as u32,s.state.frames.len() as u32] {out.extend_from_slice(&value.to_le_bytes());}
        for frame in &s.state.frames {let data=drawing_encode_document((*frame.document).clone())?;if out.len()+data.len()+frame.name.len()+12>DOCUMENT_BUDGET+1_048_576 {return Err(error("Animation file exceeds the 97 MiB raw-file limit"));}out.extend_from_slice(&(frame.name.len() as u32).to_le_bytes());out.extend_from_slice(frame.name.as_bytes());out.extend_from_slice(&frame.exposure.to_le_bytes());out.extend_from_slice(&(data.len() as u32).to_le_bytes());out.extend(data);}Ok(out)
    }
}
/// Decode transactionally into a new session; truncated/oversized frames never replace the caller.
#[uniffi::export]
pub fn drawing_decode_animation(data:Vec<u8>)->Result<Arc<DrawingAnimation>,MobileError> {
    if data.len()>DOCUMENT_BUDGET+1_048_576 {return Err(error("Animation file too large"));}
    let mut at=0usize;
    fn take<'a>(data:&'a [u8],at:&mut usize,n:usize)->Result<&'a [u8],MobileError> {let end=at.checked_add(n).ok_or_else(||error("Invalid animation length"))?;let bytes=data.get(*at..end).ok_or_else(||error("Truncated animation"))?;*at=end;Ok(bytes)}
    fn number(data:&[u8],at:&mut usize)->Result<u32,MobileError> {let b=take(data,at,4)?;Ok(u32::from_le_bytes([b[0],b[1],b[2],b[3]]))}
    if take(&data,&mut at,8)?!=b"FCANIM01" {return Err(error("Unsupported animation format"));}
    let fps=number(&data,&mut at)?;let active=number(&data,&mut at)? as usize;let count=number(&data,&mut at)? as usize;
    if !(1..=60).contains(&fps) || count==0 || count>120 || active>=count {return Err(error("Invalid animation header"));}
    let mut frames=Vec::new();let mut total=0usize;let mut size=None;
    for _ in 0..count {
        let name_length=number(&data,&mut at)? as usize;if name_length>320 {return Err(error("Frame name too long"));}let name=std::str::from_utf8(take(&data,&mut at,name_length)?).map_err(|_|error("Invalid frame name"))?.to_owned();let exposure=number(&data,&mut at)?;if !(1..=120).contains(&exposure) {return Err(error("Invalid exposure"));}
        let length=number(&data,&mut at)? as usize;let document=drawing_decode_document(take(&data,&mut at,length)?.to_vec())?;valid(&document)?;
        if let Some((w,h))=size {if document.width!=w || document.height!=h {return Err(error("All animation frames must have equal dimensions"));}} else {size=Some((document.width,document.height));}
        total+=bytes(&document);if total>DOCUMENT_BUDGET {return Err(error("Animation pixel budget exceeded"));}frames.push(Frame {document:Arc::new(document),name,exposure});
    }
    if at!=data.len() {return Err(error("Unexpected animation trailing data"));}
    Ok(Arc::new(DrawingAnimation {inner:Mutex::new(Session {state:State {frames,active,fps},past:VecDeque::new(),future:VecDeque::new()})}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawing_document::DrawingRasterLayer;
    fn document()->DrawingDocumentSnapshot {DrawingDocumentSnapshot {width:1,height:1,active_layer:0,layers:vec![DrawingRasterLayer {name:"Ink".into(),visible:true,opacity:1.0,rgba:vec![255,0,0,255],blend_mode:0,clipped:false,locked:false,alpha_locked:false,mask:vec![],mask_enabled:true}]}}
    #[test] fn exposure_mapping_and_structural_undo()->Result<(),MobileError> {
        let animation=DrawingAnimation::new(document())?;
        animation.configure("First".into(),2,12)?;animation.insert(true)?;animation.configure("Second".into(),3,12)?;
        assert_eq!(animation.frame_at_tick(0,false)?,0);assert_eq!(animation.frame_at_tick(1,false)?,0);assert_eq!(animation.frame_at_tick(2,false)?,1);assert_eq!(animation.frame_at_tick(5,true)?,0);assert_eq!(animation.frame_at_tick(50,false)?,1);
        animation.delete_current()?;assert_eq!(animation.info()?.frames.len(),1);animation.undo()?;assert_eq!(animation.info()?.frames.len(),2);animation.redo()?;assert_eq!(animation.info()?.frames.len(),1);Ok(())
    }
    #[test] fn animation_roundtrip_and_truncation()->Result<(),MobileError> {
        let animation=DrawingAnimation::new(document())?;animation.insert(false)?;let encoded=animation.encode()?;
        let reopened=drawing_decode_animation(encoded.clone())?;assert_eq!(reopened.info()?.active,1);assert_eq!(reopened.info()?.frames.len(),2);assert_eq!(reopened.current()?.layers[0].rgba,vec![0;4]);
        assert!(drawing_decode_animation(encoded[..encoded.len()-1].to_vec()).is_err());let mut trailing=encoded;trailing.push(0);assert!(drawing_decode_animation(trailing).is_err());Ok(())
    }
    #[test] fn duplicate_is_copy_on_write_and_frame_dimensions_are_fixed()->Result<(),MobileError> {
        let animation=DrawingAnimation::new(document())?;animation.insert(true)?;let mut edit=document();edit.layers[0].rgba=vec![0,255,0,255];animation.update_current(edit)?;
        assert_eq!(animation.frame(0)?.layers[0].rgba,vec![255,0,0,255]);assert_eq!(animation.frame(1)?.layers[0].rgba,vec![0,255,0,255]);
        let mut resized=document();resized.width=2;resized.layers[0].rgba=vec![0;8];assert!(animation.update_current(resized).is_err());Ok(())
    }
}

#[cfg(test)]
mod apng_tests {
    use super::*;
    #[test] fn export_preserves_frames_timing_and_source_alpha()->Result<(),MobileError> {
        use crate::drawing_document::DrawingRasterLayer;
        let animation=DrawingAnimation::new(DrawingDocumentSnapshot {width:1,height:1,active_layer:0,layers:vec![DrawingRasterLayer {name:"Ink".into(),visible:true,opacity:1.0,rgba:vec![255,0,0,255],blend_mode:0,clipped:false,locked:false,alpha_locked:false,mask:vec![],mask_enabled:true}]})?;
        animation.configure("First".into(),2,12)?;animation.insert(false)?;animation.configure("Blank".into(),3,12)?;
        let bytes=animation.encode_apng(false)?;let mut reader=png::Decoder::new(std::io::Cursor::new(bytes)).read_info().map_err(|e|error(&e.to_string()))?;
        let control=reader.info().animation_control.ok_or_else(||error("Missing APNG control"))?;assert_eq!(control.num_frames,2);assert_eq!(control.num_plays,1);
        let mut pixels=vec![0;4];reader.next_frame(&mut pixels).map_err(|e|error(&e.to_string()))?;assert_eq!(pixels,vec![255,0,0,255]);
        let frame=reader.info().frame_control.ok_or_else(||error("Missing frame timing"))?;assert_eq!(frame.delay_num,2);assert_eq!(frame.delay_den,12);
        reader.next_frame(&mut pixels).map_err(|e|error(&e.to_string()))?;assert_eq!(pixels,vec![0;4]);let frame=reader.info().frame_control.ok_or_else(||error("Missing frame timing"))?;assert_eq!(frame.delay_num,3);assert_eq!(frame.blend_op,png::BlendOp::Source);Ok(())
    }
}
