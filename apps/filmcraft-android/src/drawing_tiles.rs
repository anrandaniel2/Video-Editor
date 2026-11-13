//! Sparse scene-linear float painting storage. Android native-brush commits use this backing;
//! compatibility tools still operate on byte caches and rebase affected layers.
use std::{collections::{BTreeMap,HashSet,VecDeque},sync::{Arc,Mutex}};
use crate::{MobileError,drawing_document::DrawingDocumentSnapshot};
const TILE:usize=64;
const VALUES:usize=TILE*TILE*4;
const BUDGET:usize=192*1024*1024;
type Pixels=Arc<Vec<f32>>;
#[derive(Clone)]
struct Layer {id:u32,name:String,visible:bool,opacity:f32,tiles:BTreeMap<(u32,u32),Pixels>}
#[derive(Clone)]
struct State {width:u32,height:u32,active:u32,next:u32,layers:Vec<Layer>}
struct Session {state:State,past:VecDeque<State>,future:VecDeque<State>}
#[derive(uniffi::Object)]
pub struct DrawingTiledDocument {inner:Mutex<Session>}
#[derive(uniffi::Record)]
pub struct DrawingTileLayerInfo {pub id:u32,pub name:String,pub visible:bool,pub opacity:f32,pub allocated_tiles:u32}
#[derive(uniffi::Record)]
pub struct DrawingTileInfo {pub width:u32,pub height:u32,pub active:u32,pub layers:Vec<DrawingTileLayerInfo>,pub live_bytes:u64,pub history_bytes:u64,pub can_undo:bool,pub can_redo:bool}
fn err(s:&str)->MobileError {MobileError::Command(s.into())}
fn memory<'a>(states:impl Iterator<Item=&'a State>)->usize {let mut seen=HashSet::new();for state in states {for layer in &state.layers {for tile in layer.tiles.values() {seen.insert(Arc::as_ptr(tile) as usize);}}}seen.len()*VALUES*4}
fn layer(state:&State,id:u32)->Result<&Layer,MobileError> {state.layers.iter().find(|l|l.id==id).ok_or_else(||err("Unknown tiled paint layer"))}
fn layer_mut(state:&mut State,id:u32)->Result<&mut Layer,MobileError> {state.layers.iter_mut().find(|l|l.id==id).ok_or_else(||err("Unknown tiled paint layer"))}
fn pixel(layer:&Layer,x:u32,y:u32)->[f32;4] {let Some(tile)=layer.tiles.get(&(x/64,y/64)) else {return [0.0;4]};let i=((y as usize%TILE)*TILE+x as usize%TILE)*4;[tile[i],tile[i+1],tile[i+2],tile[i+3]]}
fn valid_pixel(p:&[f32])->bool {p.len()==4 && p.iter().all(|v|v.is_finite()) && (0.0..=1.0).contains(&p[3]) && p[..3].iter().all(|v|*v>=0.0 && *v<=65504.0*p[3])}
fn over(dst:[f32;4],src:[f32;4])->[f32;4] {let k=1.0-src[3];[src[0]+dst[0]*k,src[1]+dst[1]*k,src[2]+dst[2]*k,src[3]+dst[3]*k]}
fn decode(v:u8)->f32 {let x=v as f32/255.0;if x<=0.04045 {x/12.92} else {((x+0.055)/1.055).powf(2.4)}}
fn encode(x:f32)->u8 {let x=x.clamp(0.0,1.0);let x=if x<=0.0031308 {12.92*x} else {1.055*x.powf(1.0/2.4)-0.055};(x*255.0).round() as u8}
impl Session {
    fn commit(&mut self,next:State)->Result<(),MobileError> {
        if memory(std::iter::once(&next))>BUDGET {return Err(err("Tiled document exceeds 192 MiB live-pixel budget"));}
        self.past.push_back(self.state.clone());self.future.clear();self.state=next;
        while !self.past.is_empty() && (self.past.len()>64 || memory(std::iter::once(&self.state).chain(self.past.iter()))>BUDGET) {self.past.pop_front();}Ok(())
    }
}
#[uniffi::export]
impl DrawingTiledDocument {
    #[uniffi::constructor]
    pub fn new(width:u32,height:u32)->Result<Arc<Self>,MobileError> {
        if width==0 || height==0 || width>32768 || height>32768 {return Err(err("Tiled dimensions must be 1–32768"));}
        Ok(Arc::new(Self {inner:Mutex::new(Session {state:State {width,height,active:1,next:2,layers:vec![Layer {id:1,name:"Paint".into(),visible:true,opacity:1.0,tiles:BTreeMap::new()}]},past:VecDeque::new(),future:VecDeque::new()})}))
    }
    /// Independent editable checkpoint sharing immutable tiles, with no retained undo history.
    pub fn fork(&self)->Result<Arc<Self>,MobileError> {
        let state=self.inner.lock().map_err(|_|err("Tiled state unavailable"))?.state.clone();
        Ok(Arc::new(Self {inner:Mutex::new(Session {state,past:VecDeque::new(),future:VecDeque::new()})}))
    }
    pub fn discard_history(&self)->Result<(),MobileError> {
        let mut s=self.inner.lock().map_err(|_|err("Tiled state unavailable"))?;s.past.clear();s.future.clear();Ok(())
    }
    /// Straight sRGB8 projection of one layer, excluding layer opacity/visibility for Android caching.
    pub fn layer_srgb(&self,id:u32)->Result<Vec<u8>,MobileError> {
        let s=self.inner.lock().map_err(|_|err("Tiled state unavailable"))?;
        if u64::from(s.state.width)*u64::from(s.state.height)>4_194_304 {return Err(err("Layer projection exceeds bitmap budget"));}
        let source=layer(&s.state,id)?;let mut out=Vec::with_capacity(s.state.width as usize*s.state.height as usize*4);
        for y in 0..s.state.height {for x in 0..s.state.width {let p=pixel(source,x,y);for value in &p[..3] {out.push(encode(if p[3]>0.0 {*value/p[3]} else {0.0}));}out.push((p[3].clamp(0.0,1.0)*255.0).round() as u8);}}Ok(out)
    }
    pub fn info(&self)->Result<DrawingTileInfo,MobileError> {
        let s=self.inner.lock().map_err(|_|err("Tiled state unavailable"))?;
        let live=memory(std::iter::once(&s.state));let total=memory(std::iter::once(&s.state).chain(s.past.iter()).chain(s.future.iter()));
        Ok(DrawingTileInfo {width:s.state.width,height:s.state.height,active:s.state.active,layers:s.state.layers.iter().map(|l|DrawingTileLayerInfo {id:l.id,name:l.name.clone(),visible:l.visible,opacity:l.opacity,allocated_tiles:l.tiles.len() as u32}).collect(),live_bytes:live as u64,history_bytes:(total-live) as u64,can_undo:!s.past.is_empty(),can_redo:!s.future.is_empty()})
    }
    /// Atomic rectangular write. Input is premultiplied scene-linear sRGB-primary float RGBA.
    pub fn write_region(&self,id:u32,x:u32,y:u32,width:u32,height:u32,rgba:Vec<f32>)->Result<(),MobileError> {
        let mut s=self.inner.lock().map_err(|_|err("Tiled state unavailable"))?;
        if width==0 || height==0 || u64::from(width)*u64::from(height)>4_194_304 || x.checked_add(width).map_or(true,|v|v>s.state.width) || y.checked_add(height).map_or(true,|v|v>s.state.height) || rgba.len() as u64!=u64::from(width)*u64::from(height)*4 || rgba.chunks_exact(4).any(|p|!valid_pixel(p)) {return Err(err("Invalid float pixel region"));}
        let old=layer(&s.state,id)?;
        let keys=(x/64..=(x+width-1)/64).flat_map(|tx|(y/64..=(y+height-1)/64).map(move |ty|(tx,ty))).collect::<Vec<_>>();
        let missing=keys.iter().filter(|k|!old.tiles.contains_key(k)).count();
        if memory(std::iter::once(&s.state))+missing*VALUES*4>BUDGET {return Err(err("Tile allocation exceeds budget"));}
        let mut next=s.state.clone();let target=layer_mut(&mut next,id)?;
        for key in keys {
            let tile=target.tiles.entry(key).or_insert_with(||Arc::new(vec![0.0;VALUES]));let pixels=Arc::make_mut(tile);
            let left=x.max(key.0*64);let top=y.max(key.1*64);let right=(x+width).min(key.0*64+64);let bottom=(y+height).min(key.1*64+64);
            for py in top..bottom {for px in left..right {let src=(((py-y)*width+px-x)*4) as usize;let dst=((py as usize%64)*64+px as usize%64)*4;pixels[dst..dst+4].copy_from_slice(&rgba[src..src+4]);}}
            if pixels.iter().all(|v|*v==0.0) {target.tiles.remove(&key);}
        }
        s.commit(next)
    }
    /// Commit GPU-generated coverage directly into scene-linear tiles as one undo transaction.
    /// Colour is straight linear RGBA; selection, if present, matches the coverage rectangle.
    pub fn paint_coverage(&self,id:u32,coverage:crate::drawing::DrawingCoverage,color:Vec<f32>,opacity:f32,selection:Vec<u8>,erase:bool,alpha_locked:bool)->Result<(),MobileError> {
        if color.len()!=4 || !color.iter().all(|v|v.is_finite()) || color[..3].iter().any(|v|!(0.0..=65504.0).contains(v)) || !(0.0..=1.0).contains(&color[3]) || !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {return Err(err("Invalid linear brush colour or opacity"));}
        let mut s=self.inner.lock().map_err(|_|err("Tiled state unavailable"))?;
        let count=u64::from(coverage.width)*u64::from(coverage.height);
        if count>4_194_304 || coverage.alpha.len() as u64!=count || (!selection.is_empty() && selection.len()!=coverage.alpha.len()) || coverage.left.checked_add(coverage.width).map_or(true,|v|v>s.state.width) || coverage.top.checked_add(coverage.height).map_or(true,|v|v>s.state.height) {return Err(err("Invalid brush coverage bounds"));}
        layer(&s.state,id)?;
        if count==0 || opacity==0.0 || color[3]==0.0 || (erase && alpha_locked) {return Ok(());}
        let mut next=s.state.clone();let target=layer_mut(&mut next,id)?;let mut touched=HashSet::new();let mut changed=false;
        for y in 0..coverage.height {for x in 0..coverage.width {
            let at=(y*coverage.width+x) as usize;let selected=if selection.is_empty() {1.0} else {selection[at] as f32/255.0};
            let strength=coverage.alpha[at] as f32/255.0*opacity*color[3]*selected;if strength==0.0 {continue;}
            let px=coverage.left+x;let py=coverage.top+y;let key=(px/64,py/64);let old=pixel(target,px,py);
            if (erase || alpha_locked) && old[3]==0.0 {continue;}
            let mut result=if erase {old.map(|v|v*(1.0-strength))} else if alpha_locked {
                [color[0]*old[3]*strength+old[0]*(1.0-strength),color[1]*old[3]*strength+old[1]*(1.0-strength),color[2]*old[3]*strength+old[2]*(1.0-strength),old[3]]
            } else {over(old,[color[0]*strength,color[1]*strength,color[2]*strength,strength])};
            result[3]=result[3].clamp(0.0,1.0);
            for c in 0..3 {result[c]=result[c].clamp(0.0,65504.0*result[3]);}
            if old==result {continue;}changed=true;
            let tile=target.tiles.entry(key).or_insert_with(||Arc::new(vec![0.0;VALUES]));let pixels=Arc::make_mut(tile);let dst=((py as usize%64)*64+px as usize%64)*4;pixels[dst..dst+4].copy_from_slice(&result);touched.insert(key);
        }}
        for key in touched {if target.tiles.get(&key).is_some_and(|tile|tile.iter().all(|v|*v==0.0)) {target.tiles.remove(&key);}}
        if changed {s.commit(next)?;}Ok(())
    }
    /// GPU coverage is mandatory; GPU failure leaves the document and undo stack unchanged.
    pub fn paint_gpu_stroke(&self,id:u32,points:Vec<crate::drawing::DrawingStrokePoint>,settings:crate::drawing_brush_engine::DrawingNativeBrush,color:Vec<f32>,opacity:f32,erase:bool,alpha_locked:bool)->Result<(),MobileError> {
        let (width,height)={let s=self.inner.lock().map_err(|_|err("Tiled state unavailable"))?;layer(&s.state,id)?;(s.state.width,s.state.height)};
        let coverage=crate::drawing_brush_engine::drawing_native_brush_coverage(points,width,height,settings)?;
        self.paint_coverage(id,coverage,color,opacity,vec![],erase,alpha_locked)
    }
    /// Linear-light adjustments retain HDR values, unlike the byte compatibility adjustment path.
    /// Selection is full-canvas grayscale; opacity/alpha are preserved exactly.
    pub fn adjust_linear(&self,id:u32,settings:crate::drawing_raster::DrawingAdjustments,selection:Vec<u8>)->Result<(),MobileError> {
        // Reuse the public settings validator without processing a document-sized byte buffer.
        crate::drawing_raster::drawing_adjust_rgba(vec![0;4],1,1,settings.clone())?;
        let mut s=self.inner.lock().map_err(|_|err("Tiled state unavailable"))?;
        if !selection.is_empty() && selection.len() as u64!=u64::from(s.state.width)*u64::from(s.state.height) {return Err(err("Invalid adjustment selection"));}
        layer(&s.state,id)?;
        if settings.exposure==0.0 && settings.brightness==0.0 && settings.contrast==1.0 && settings.saturation==1.0 && settings.hue_degrees==0.0 && settings.gamma==1.0 && settings.black_point==0.0 && settings.white_point==1.0 && settings.red_gain==1.0 && settings.green_gain==1.0 && settings.blue_gain==1.0 {return Ok(());}
        let mut next=s.state.clone();let width=next.width;let height=next.height;let target=layer_mut(&mut next,id)?;
        let angle=settings.hue_degrees.to_radians();let cosine=angle.cos();let sine=angle.sin();let gains=[settings.red_gain,settings.green_gain,settings.blue_gain];let mut changed=false;
        for (&(tx,ty),tile) in &mut target.tiles {
            let pixels=Arc::make_mut(tile);
            for (index,p) in pixels.chunks_exact_mut(4).enumerate() {
                if p[3]==0.0 {continue;}let x=tx*64+(index%64) as u32;let y=ty*64+(index/64) as u32;if x>=width || y>=height {continue;}
                let strength=if selection.is_empty() {1.0} else {selection[(y*width+x) as usize] as f32/255.0};if strength==0.0 {continue;}
                let mut rgb=[0.0;3];for c in 0..3 {
                    let level=((p[c]/p[3]-settings.black_point)/(settings.white_point-settings.black_point)).max(0.0);
                    let exposed=(level.powf(1.0/settings.gamma)*2.0f32.powf(settings.exposure)).min(65504.0);
                    rgb[c]=((exposed+settings.brightness-0.5)*settings.contrast+0.5).clamp(0.0,65504.0);
                }
                if settings.hue_degrees!=0.0 || settings.saturation!=1.0 {
                    let y=0.299*rgb[0]+0.587*rgb[1]+0.114*rgb[2];let i=0.596*rgb[0]-0.274*rgb[1]-0.322*rgb[2];let q=0.211*rgb[0]-0.523*rgb[1]+0.312*rgb[2];let ii=(i*cosine-q*sine)*settings.saturation;let qq=(i*sine+q*cosine)*settings.saturation;
                    rgb=[y+0.956*ii+0.621*qq,y-0.272*ii-0.647*qq,y-1.106*ii+1.703*qq];
                }
                for c in 0..3 {let value=(rgb[c]*gains[c]).clamp(0.0,65504.0)*p[3];let blended=p[c]*(1.0-strength)+value*strength;changed|=p[c]!=blended;p[c]=blended.clamp(0.0,65504.0*p[3]);}
            }
        }
        if changed {s.commit(next)?;}Ok(())
    }
    pub fn read_region(&self,id:u32,x:u32,y:u32,width:u32,height:u32)->Result<Vec<f32>,MobileError> {
        let s=self.inner.lock().map_err(|_|err("Tiled state unavailable"))?;
        if width==0 || height==0 || u64::from(width)*u64::from(height)>4_194_304 || x.checked_add(width).map_or(true,|v|v>s.state.width) || y.checked_add(height).map_or(true,|v|v>s.state.height) {return Err(err("Invalid read region"));}
        let source=layer(&s.state,id)?;let mut out=Vec::with_capacity(width as usize*height as usize*4);for py in y..y+height {for px in x..x+width {out.extend_from_slice(&pixel(source,px,py));}}Ok(out)
    }
    pub fn add_layer(&self,name:String,duplicate:bool)->Result<u32,MobileError> {
        if name.len()>320 {return Err(err("Layer name too long"));}let mut s=self.inner.lock().map_err(|_|err("Tiled state unavailable"))?;if s.state.layers.len()>=256 {return Err(err("Tiled layer limit is 256"));}
        let mut next=s.state.clone();let id=next.next;next.next=next.next.checked_add(1).ok_or_else(||err("Layer identifiers exhausted"))?;
        let source=layer(&next,next.active)?;let tiles=if duplicate {source.tiles.clone()} else {BTreeMap::new()};let at=next.layers.iter().position(|l|l.id==next.active).ok_or_else(||err("Invalid active layer"))?+1;
        next.layers.insert(at,Layer {id,name,visible:true,opacity:1.0,tiles});next.active=id;s.commit(next)?;Ok(id)
    }
    pub fn configure_layer(&self,id:u32,name:String,visible:bool,opacity:f32)->Result<(),MobileError> {
        if name.len()>320 || !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {return Err(err("Invalid layer properties"));}let mut s=self.inner.lock().map_err(|_|err("Tiled state unavailable"))?;let mut next=s.state.clone();let l=layer_mut(&mut next,id)?;l.name=name;l.visible=visible;l.opacity=opacity;s.commit(next)
    }
    pub fn select_layer(&self,id:u32)->Result<(),MobileError> {let mut s=self.inner.lock().map_err(|_|err("Tiled state unavailable"))?;layer(&s.state,id)?;s.state.active=id;Ok(())}
    pub fn delete_layer(&self,id:u32)->Result<(),MobileError> {let mut s=self.inner.lock().map_err(|_|err("Tiled state unavailable"))?;if s.state.layers.len()==1 {return Err(err("Keep one paint layer"));}layer(&s.state,id)?;let mut next=s.state.clone();next.layers.retain(|l|l.id!=id);if next.active==id {next.active=next.layers[0].id;}s.commit(next)}
    pub fn reorder_layer(&self,id:u32,index:u32)->Result<(),MobileError> {let mut s=self.inner.lock().map_err(|_|err("Tiled state unavailable"))?;if index as usize>=s.state.layers.len() {return Err(err("Invalid layer position"));}let mut next=s.state.clone();let at=next.layers.iter().position(|l|l.id==id).ok_or_else(||err("Unknown tiled layer"))?;let l=next.layers.remove(at);next.layers.insert(index as usize,l);s.commit(next)}
    /// SDR display conversion only. HDR values remain unchanged in tile storage.
    /// tone_map: 0 clips, 1 applies component-wise Reinhard; exposure is in stops.
    pub fn display_region(&self,x:u32,y:u32,width:u32,height:u32,exposure:f32,tone_map:u32)->Result<Vec<u8>,MobileError> {
        let s=self.inner.lock().map_err(|_|err("Tiled state unavailable"))?;
        if width==0 || height==0 || u64::from(width)*u64::from(height)>4_194_304 || x.checked_add(width).map_or(true,|v|v>s.state.width) || y.checked_add(height).map_or(true,|v|v>s.state.height) || !exposure.is_finite() || !(-20.0..=20.0).contains(&exposure) || tone_map>1 {return Err(err("Invalid display region or transform"));}
        let mut out=Vec::with_capacity(width as usize*height as usize*4);let gain=2.0f32.powf(exposure);
        for py in y..y+height {for px in x..x+width {let mut p=[0.0;4];for l in &s.state.layers {if l.visible {let mut src=pixel(l,px,py);for c in &mut src {*c*=l.opacity;}p=over(p,src);}}
            for value in &p[..3] {let linear=if p[3]>0.0 {*value/p[3]*gain} else {0.0};out.push(encode(if tone_map==1 {linear/(1.0+linear)} else {linear}));}out.push((p[3].clamp(0.0,1.0)*255.0).round() as u8);
        }}Ok(out)
    }
    /// Lossless sparse f32 persistence. Profile is fixed to scene-linear sRGB primaries.
    pub fn encode(&self)->Result<Vec<u8>,MobileError> {
        let s=self.inner.lock().map_err(|_|err("Tiled state unavailable"))?;
        let serialized=s.state.layers.iter().map(|l|l.tiles.len()).sum::<usize>()*VALUES*4;
        if serialized>BUDGET {return Err(err("Serialized tiles exceed file budget; shared duplicates are written independently"));}
        let mut out=b"FCTILE01".to_vec();
        for n in [s.state.width,s.state.height,s.state.active,s.state.next,s.state.layers.len() as u32] {out.extend_from_slice(&n.to_le_bytes());}
        for l in &s.state.layers {
            out.extend_from_slice(&l.id.to_le_bytes());out.extend_from_slice(&(l.name.len() as u32).to_le_bytes());out.extend_from_slice(l.name.as_bytes());out.push(u8::from(l.visible));out.extend_from_slice(&l.opacity.to_le_bytes());out.extend_from_slice(&(l.tiles.len() as u32).to_le_bytes());
            for ((x,y),tile) in &l.tiles {out.extend_from_slice(&x.to_le_bytes());out.extend_from_slice(&y.to_le_bytes());for v in tile.iter() {out.extend_from_slice(&v.to_le_bytes());}}
        }Ok(out)
    }
    pub fn undo(&self)->Result<(),MobileError> {let mut s=self.inner.lock().map_err(|_|err("Tiled state unavailable"))?;if let Some(previous)=s.past.pop_back() {let current=s.state.clone();s.future.push_back(current);s.state=previous;}Ok(())}
    pub fn redo(&self)->Result<(),MobileError> {let mut s=self.inner.lock().map_err(|_|err("Tiled state unavailable"))?;if let Some(next)=s.future.pop_back() {let current=s.state.clone();s.past.push_back(current);s.state=next;}Ok(())}
}
/// Explicit import gate: unsupported blending, clipping and masks are rejected, never flattened silently.
#[uniffi::export]
pub fn drawing_import_tiled(document:DrawingDocumentSnapshot)->Result<Arc<DrawingTiledDocument>,MobileError> {
    let bytes=crate::drawing_document::drawing_encode_document(document.clone())?;drop(bytes);
    if document.layers.iter().any(|l|l.blend_mode!=0 || l.clipped || !l.mask.is_empty() || l.locked || l.alpha_locked) {return Err(err("Tiled import currently accepts normal, unmasked, unlocked paint layers only"));}
    let result=DrawingTiledDocument::new(document.width,document.height)?;
    {let mut s=result.inner.lock().map_err(|_|err("Tiled state unavailable"))?;let mut next=s.state.clone();next.layers.clear();
        for (index,source) in document.layers.iter().enumerate() {let id=index as u32+1;let mut l=Layer {id,name:source.name.clone(),visible:source.visible,opacity:source.opacity,tiles:BTreeMap::new()};
            // Preflight each allocated tile to prevent a dense import from exceeding the budget.
            for y in 0..document.height {for x in 0..document.width {let i=((y*document.width+x)*4) as usize;let a=source.rgba[i+3] as f32/255.0;if a==0.0 {continue;}let key=(x/64,y/64);
                if !l.tiles.contains_key(&key) {let used=next.layers.iter().map(|l|l.tiles.len()).sum::<usize>()+l.tiles.len()+1;if used*VALUES*4>BUDGET {return Err(err("Imported float tiles exceed budget"));}l.tiles.insert(key,Arc::new(vec![0.0;VALUES]));}
                let tile=l.tiles.get_mut(&key).ok_or_else(||err("Missing imported tile"))?;let tile=Arc::make_mut(tile);let dst=((y as usize%64)*64+x as usize%64)*4;tile[dst]=decode(source.rgba[i])*a;tile[dst+1]=decode(source.rgba[i+1])*a;tile[dst+2]=decode(source.rgba[i+2])*a;tile[dst+3]=a;
            }}next.layers.push(l);
        }next.active=document.active_layer+1;next.next=next.layers.len() as u32+1;s.state=next;
    }Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn sparse_hdr_roundtrip_and_history()->Result<(),MobileError> {let doc=DrawingTiledDocument::new(32768,32768)?;assert_eq!(doc.info()?.live_bytes,0);doc.write_region(1,32767,32767,1,1,vec![4.0,2.0,1.0,1.0])?;assert_eq!(doc.info()?.live_bytes,(VALUES*4) as u64);assert_eq!(doc.read_region(1,32767,32767,1,1)?,vec![4.0,2.0,1.0,1.0]);doc.undo()?;assert_eq!(doc.info()?.live_bytes,0);doc.redo()?;assert_eq!(doc.read_region(1,32767,32767,1,1)?[0],4.0);Ok(())}
    #[test] fn duplicates_share_until_written()->Result<(),MobileError> {let doc=DrawingTiledDocument::new(65,1)?;doc.write_region(1,63,0,2,1,vec![1.0;8])?;let id=doc.add_layer("Copy".into(),true)?;assert_eq!(doc.info()?.live_bytes,(VALUES*8) as u64);doc.write_region(id,63,0,1,1,vec![0.0;4])?;assert_eq!(doc.read_region(1,63,0,1,1)?,vec![1.0;4]);assert_eq!(doc.read_region(id,63,0,1,1)?,vec![0.0;4]);Ok(())}
    #[test] fn rejects_nonfinite_and_invalid_premultiplication()->Result<(),MobileError> {let doc=DrawingTiledDocument::new(1,1)?;assert!(doc.write_region(1,0,0,1,1,vec![f32::NAN,0.0,0.0,1.0]).is_err());assert!(doc.write_region(1,0,0,1,1,vec![1.0,0.0,0.0,0.0]).is_err());assert_eq!(doc.info()?.live_bytes,0);Ok(())}
    #[test] fn linear_blend_display()->Result<(),MobileError> {let doc=DrawingTiledDocument::new(1,1)?;doc.write_region(1,0,0,1,1,vec![0.5,0.5,0.5,1.0])?;let display=doc.display_region(0,0,1,1,0.0,0)?;assert!((display[0] as i32-188).abs()<=1);assert_eq!(display[3],255);Ok(())}
}

#[uniffi::export]
pub fn drawing_decode_tiled(data:Vec<u8>)->Result<Arc<DrawingTiledDocument>,MobileError> {
    struct Reader<'a> {data:&'a [u8],at:usize}
    impl<'a> Reader<'a> {
        fn take(&mut self,n:usize)->Result<&'a [u8],MobileError> {let end=self.at.checked_add(n).ok_or_else(||err("Invalid tile file length"))?;let bytes=self.data.get(self.at..end).ok_or_else(||err("Truncated tile file"))?;self.at=end;Ok(bytes)}
        fn number(&mut self)->Result<u32,MobileError> {let b=self.take(4)?;Ok(u32::from_le_bytes([b[0],b[1],b[2],b[3]]))}
    }
    if data.len()>BUDGET+128*1024 {return Err(err("Tile file exceeds limit"));}
    let mut r=Reader {data:&data,at:0};if r.take(8)?!=b"FCTILE01" {return Err(err("Unsupported tile document"));}
    let width=r.number()?;let height=r.number()?;let active=r.number()?;let next=r.number()?;let count=r.number()?;
    if count==0 || count>256 {return Err(err("Invalid tiled layer count"));}let result=DrawingTiledDocument::new(width,height)?;
    let mut layers=Vec::new();let mut ids=HashSet::new();let mut allocated=0usize;
    for _ in 0..count {
        let id=r.number()?;if id==0 || id>=next || !ids.insert(id) {return Err(err("Invalid or duplicate layer identifier"));}
        let length=r.number()? as usize;if length>320 {return Err(err("Layer name too long"));}let name=std::str::from_utf8(r.take(length)?).map_err(|_|err("Invalid UTF-8 layer name"))?.to_owned();
        let visible=match r.take(1)?[0] {0=>false,1=>true,_=>return Err(err("Invalid layer visibility"))};let opacity=f32::from_bits(r.number()?);if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {return Err(err("Invalid layer opacity"));}
        let count=r.number()? as usize;allocated=allocated.checked_add(count).ok_or_else(||err("Tile allocation overflow"))?;if allocated>BUDGET/(VALUES*4) {return Err(err("Tile allocation budget exceeded"));}
        let mut tiles=BTreeMap::new();for _ in 0..count {let tx=r.number()?;let ty=r.number()?;if tx>=(width+63)/64 || ty>=(height+63)/64 || tiles.contains_key(&(tx,ty)) {return Err(err("Invalid or duplicate tile coordinate"));}
            let raw=r.take(VALUES*4)?;let mut values=Vec::with_capacity(VALUES);for b in raw.chunks_exact(4) {values.push(f32::from_le_bytes([b[0],b[1],b[2],b[3]]));}
            for (i,p) in values.chunks_exact(4).enumerate() {if !valid_pixel(p) || ((tx*64+(i%64) as u32>=width || ty*64+(i/64) as u32>=height) && p.iter().any(|v|*v!=0.0)) {return Err(err("Invalid float pixel or nonzero edge padding"));}}
            if values.iter().all(|v|*v==0.0) {return Err(err("Noncanonical empty tile"));}tiles.insert((tx,ty),Arc::new(values));
        }layers.push(Layer {id,name,visible,opacity,tiles});
    }
    if !ids.contains(&active) || r.at!=data.len() {return Err(err("Invalid active layer or trailing tile data"));}
    {let mut s=result.inner.lock().map_err(|_|err("Tiled state unavailable"))?;s.state=State {width,height,active,next,layers};}Ok(result)
}

#[cfg(test)]
mod persistence_tests {
    use super::*;
    #[test] fn hdr_persistence_and_truncation()->Result<(),MobileError> {
        let source=DrawingTiledDocument::new(65,65)?;source.write_region(1,64,64,1,1,vec![12.5,0.25,0.0,0.5])?;
        let bytes=source.encode()?;let reopened=drawing_decode_tiled(bytes.clone())?;assert_eq!(reopened.read_region(1,64,64,1,1)?,vec![12.5,0.25,0.0,0.5]);
        assert!(drawing_decode_tiled(bytes[..bytes.len()-1].to_vec()).is_err());let mut trailing=bytes;trailing.push(0);assert!(drawing_decode_tiled(trailing).is_err());Ok(())
    }
}

#[cfg(test)]
mod painting_tests {
    use super::*;
    fn coverage(value:u8)->crate::drawing::DrawingCoverage {crate::drawing::DrawingCoverage {left:0,top:0,width:1,height:1,alpha:vec![value]}}
    #[test] fn hdr_coverage_selection_erase_and_undo()->Result<(),MobileError> {
        let doc=DrawingTiledDocument::new(1,1)?;
        doc.paint_coverage(1,coverage(255),vec![8.0,2.0,0.0,1.0],1.0,vec![],false,false)?;
        assert_eq!(doc.read_region(1,0,0,1,1)?,vec![8.0,2.0,0.0,1.0]);
        doc.paint_coverage(1,coverage(255),vec![0.0,0.0,0.0,1.0],1.0,vec![0],true,false)?;
        assert_eq!(doc.read_region(1,0,0,1,1)?[0],8.0);
        doc.paint_coverage(1,coverage(255),vec![0.0,0.0,0.0,1.0],1.0,vec![],true,false)?;
        assert_eq!(doc.info()?.live_bytes,0);doc.undo()?;assert_eq!(doc.read_region(1,0,0,1,1)?[0],8.0);Ok(())
    }
    #[test] fn alpha_lock_preserves_coverage()->Result<(),MobileError> {
        let doc=DrawingTiledDocument::new(1,1)?;doc.write_region(1,0,0,1,1,vec![0.5,0.0,0.0,0.5])?;
        doc.paint_coverage(1,coverage(255),vec![0.0,4.0,0.0,1.0],1.0,vec![],false,true)?;
        assert_eq!(doc.read_region(1,0,0,1,1)?,vec![0.0,2.0,0.0,0.5]);Ok(())
    }
}

#[cfg(test)]
mod linear_adjustment_tests {
    use super::*;
    fn settings()->crate::drawing_raster::DrawingAdjustments {crate::drawing_raster::DrawingAdjustments {exposure:1.0,brightness:0.0,contrast:1.0,saturation:1.0,hue_degrees:0.0,gamma:1.0,black_point:0.0,white_point:1.0,red_gain:1.0,green_gain:1.0,blue_gain:1.0}}
    #[test] fn hdr_exposure_and_selection_keep_alpha()->Result<(),MobileError> {
        let doc=DrawingTiledDocument::new(1,1)?;doc.write_region(1,0,0,1,1,vec![2.0,1.0,0.5,0.5])?;
        doc.adjust_linear(1,settings(),vec![0])?;assert_eq!(doc.read_region(1,0,0,1,1)?,vec![2.0,1.0,0.5,0.5]);
        doc.adjust_linear(1,settings(),vec![])?;assert_eq!(doc.read_region(1,0,0,1,1)?,vec![4.0,2.0,1.0,0.5]);doc.undo()?;assert_eq!(doc.read_region(1,0,0,1,1)?[0],2.0);Ok(())
    }
    #[test] fn extreme_gamma_and_zero_contrast_remain_finite()->Result<(),MobileError> {
        let doc=DrawingTiledDocument::new(1,1)?;doc.write_region(1,0,0,1,1,vec![65504.0,0.0,0.0,1.0])?;
        let mut s=settings();s.gamma=0.1;s.contrast=0.0;doc.adjust_linear(1,s,vec![])?;assert!(doc.read_region(1,0,0,1,1)?.iter().all(|v|v.is_finite()));Ok(())
    }
}

#[path = "drawing_tile_operations.rs"]
mod operations;
