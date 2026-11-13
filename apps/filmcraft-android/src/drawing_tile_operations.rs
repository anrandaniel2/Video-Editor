//! Float-native operations used by the existing editor controls.
use super::*;
fn clean(mut p:[f32;4])->[f32;4] {p[3]=p[3].clamp(0.0,1.0);for c in 0..3 {p[c]=p[c].clamp(0.0,65504.0*p[3]);}p}
fn put(target:&mut Layer,x:u32,y:u32,p:[f32;4],allocated:&mut usize)->Result<(),MobileError> {
    if p.iter().all(|v|*v==0.0) {return Ok(());}
    let key=(x/64,y/64);
    if !target.tiles.contains_key(&key) {if *allocated>=BUDGET/(VALUES*4) {return Err(err("Float operation exceeds tile budget"));}target.tiles.insert(key,Arc::new(vec![0.0;VALUES]));*allocated+=1;}
    let tile=target.tiles.get_mut(&key).ok_or_else(||err("Missing output tile"))?;let i=((y as usize%64)*64+x as usize%64)*4;Arc::make_mut(tile)[i..i+4].copy_from_slice(&p);Ok(())
}
fn selected(selection:&[u8],index:usize)->f32 {if selection.is_empty() {1.0} else {selection[index] as f32/255.0}}
impl DrawingTiledDocument {
    fn map_layer(&self,id:u32,selection:&[u8],alpha_locked:bool,mut operation:impl FnMut([f32;4])->[f32;4])->Result<(),MobileError> {
        let mut s=self.inner.lock().map_err(|_|err("Tiled state unavailable"))?;let source=layer(&s.state,id)?;
        let width=s.state.width;let height=s.state.height;
        if u64::from(width)*u64::from(height)>4_194_304 || (!selection.is_empty() && selection.len() as u64!=u64::from(width)*u64::from(height)) {return Err(err("Invalid float operation selection or size"));}
        let mut output=source.clone();output.tiles.clear();let mut allocated=s.state.layers.iter().filter(|l|l.id!=id).map(|l|l.tiles.len()).sum::<usize>();let mut changed=false;
        for y in 0..height {for x in 0..width {let old=pixel(source,x,y);let strength=selected(selection,(y*width+x) as usize);
            let mut p=if strength==0.0 {old} else {clean(operation(old))};
            if alpha_locked {if p[3]>0.0 {for c in 0..3 {p[c]=p[c]/p[3]*old[3];}} else {p[..3].copy_from_slice(&old[..3]);}p[3]=old[3];}
            for c in 0..4 {p[c]=old[c]*(1.0-strength)+p[c]*strength;}let p=clean(p);changed|=p!=old;put(&mut output,x,y,p,&mut allocated)?;
        }}if changed {let mut next=s.state.clone();*layer_mut(&mut next,id)?=output;s.commit(next)?;}Ok(())
    }
}
#[uniffi::export]
impl DrawingTiledDocument {
    /// LUT values span linear 0..1; values beyond one extrapolate the last segment.
    pub fn apply_curve_float(&self,id:u32,values:Vec<u8>,channel:u32,selection:Vec<u8>,alpha_locked:bool)->Result<(),MobileError> {
        if values.len()!=256 || channel>4 {return Err(err("Invalid float curve"));}
        let curve=|v:f32| {let index=(v*255.0).floor().min(254.0) as usize;let fraction=v*255.0-index as f32;((values[index] as f32+(values[index+1] as f32-values[index] as f32)*fraction)/255.0).clamp(0.0,65504.0)};
        self.map_layer(id,&selection,alpha_locked,|p| {
            let mut rgb=if p[3]>0.0 {[p[0]/p[3],p[1]/p[3],p[2]/p[3]]} else {[0.0;3]};let mut alpha=p[3];
            if channel==4 {alpha=curve(alpha).clamp(0.0,1.0);} else if channel==0 {for c in &mut rgb {*c=curve(*c);}} else {rgb[(channel-1) as usize]=curve(rgb[(channel-1) as usize]);}
            [rgb[0]*alpha,rgb[1]*alpha,rgb[2]*alpha,alpha]
        })
    }
    pub fn filter_float(&self,id:u32,operation:String,amount:f32,selection:Vec<u8>,alpha_locked:bool)->Result<(),MobileError> {
        if !amount.is_finite() || !(0.0..=1.0).contains(&amount) || !matches!(operation.as_str(),"invert"|"grayscale"|"sepia"|"threshold"|"brightness"|"contrast"|"saturation"|"posterize"|"gamma"|"temperature"|"solarize"|"red-channel"|"green-channel"|"blue-channel"|"alpha-channel"|"alpha-threshold"|"alpha-luma"|"opaque") {return Err(err("Unsupported float filter or amount"));}
        self.map_layer(id,&selection,alpha_locked,|p| {
            let [r,g,b]=if p[3]>0.0 {[p[0]/p[3],p[1]/p[3],p[2]/p[3]]} else {[0.0;3]};let luma=0.2126*r+0.7152*g+0.0722*b;let mut alpha=p[3];
            let rgb=match operation.as_str() {
                "invert"=>[1.0-r,1.0-g,1.0-b],"grayscale"=>[luma;3],
                "sepia"=>[r*0.393+g*0.769+b*0.189,r*0.349+g*0.686+b*0.168,r*0.272+g*0.534+b*0.131],
                "threshold"=>[if luma>=amount {1.0} else {0.0};3],
                "brightness"=>[r+amount-0.5,g+amount-0.5,b+amount-0.5],
                "contrast"=>{let gain=(amount*4.0).max(0.01);[(r-0.5)*gain+0.5,(g-0.5)*gain+0.5,(b-0.5)*gain+0.5]},
                "saturation"=>{let gain=amount*2.0;[luma+(r-luma)*gain,luma+(g-luma)*gain,luma+(b-luma)*gain]},
                "posterize"=>{let steps=(amount*14.0).round()+1.0;[(r*steps).round()/steps,(g*steps).round()/steps,(b*steps).round()/steps]},
                "gamma"=>{let power=1.0/(0.1+amount*2.9);[r.powf(power),g.powf(power),b.powf(power)]},
                "temperature"=>{let shift=(amount-0.5)*100.0/255.0;[r+shift,g,b-shift]},
                "solarize"=>[if r>amount {1.0-r} else {r},if g>amount {1.0-g} else {g},if b>amount {1.0-b} else {b}],
                "red-channel"=>[r;3],"green-channel"=>[g;3],"blue-channel"=>[b;3],
                "alpha-channel"=>{alpha=1.0;[p[3];3]},
                "alpha-threshold"=>{alpha=if p[3]>=amount {1.0} else {0.0};[r,g,b]},
                "alpha-luma"=>{alpha*=luma.clamp(0.0,1.0);[r,g,b]},
                _=>{alpha=1.0;[r,g,b]},
            };[rgb[0].clamp(0.0,65504.0)*alpha,rgb[1].clamp(0.0,65504.0)*alpha,rgb[2].clamp(0.0,65504.0)*alpha,alpha]
        })
    }
    pub fn remap_float(&self,id:u32,operation:String,selection:Vec<u8>,alpha_locked:bool)->Result<(),MobileError> {
        if !matches!(operation.as_str(),"flip-x"|"flip-y"|"rotate-180"|"offset-wrap") {return Err(err("Unsupported float layer remap"));}
        let mut s=self.inner.lock().map_err(|_|err("Tiled state unavailable"))?;let width=s.state.width;let height=s.state.height;let source=layer(&s.state,id)?;
        if u64::from(width)*u64::from(height)>4_194_304 || (!selection.is_empty() && selection.len() as u64!=u64::from(width)*u64::from(height)) {return Err(err("Invalid remap selection or size"));}
        let mut output=source.clone();output.tiles.clear();let mut allocated=s.state.layers.iter().filter(|l|l.id!=id).map(|l|l.tiles.len()).sum::<usize>();let mut changed=false;
        for y in 0..height {for x in 0..width {
            let (sx,sy)=match operation.as_str() {"flip-x"=>(width-1-x,y),"flip-y"=>(x,height-1-y),"rotate-180"=>(width-1-x,height-1-y),_=>((x+width/2)%width,(y+height/2)%height)};
            let old=pixel(source,x,y);let mut p=pixel(source,sx,sy);let strength=selected(&selection,(y*width+x) as usize);
            if alpha_locked {if p[3]>0.0 {for c in 0..3 {p[c]=p[c]/p[3]*old[3];}} else {p[..3].copy_from_slice(&old[..3]);}p[3]=old[3];}
            for c in 0..4 {p[c]=old[c]*(1.0-strength)+p[c]*strength;}let p=clean(p);changed|=p!=old;put(&mut output,x,y,p,&mut allocated)?;
        }}if changed {let mut next=s.state.clone();*layer_mut(&mut next,id)?=output;s.commit(next)?;}Ok(())
    }
    /// Geometry preserves float premultiplication; resize uses bilinear linear-light sampling.
    pub fn transform_float(&self,operation:String,width:u32,height:u32,offset_x:i32,offset_y:i32)->Result<(),MobileError> {
        let mut s=self.inner.lock().map_err(|_|err("Tiled state unavailable"))?;let old=&s.state;
        let (width,height)=match operation.as_str() {"rotate-cw"|"rotate-ccw"=>(old.height,old.width),"scale"|"canvas"|"crop"=>(width,height),_=>return Err(err("Unsupported float geometry"))};
        if width==0 || height==0 || width>32768 || height>32768 || u64::from(width)*u64::from(height)>4_194_304 || offset_x.unsigned_abs()>16_777_216 || offset_y.unsigned_abs()>16_777_216 {return Err(err("Invalid float geometry dimensions"));}
        let mut next=old.clone();next.width=width;next.height=height;let mut allocated=0usize;
        for (target,source) in next.layers.iter_mut().zip(&old.layers) {target.tiles.clear();
            for y in 0..height {for x in 0..width {
                let p=if operation=="scale" {
                    let fx=((x as f64+0.5)*old.width as f64/width as f64-0.5).clamp(0.0,(old.width-1) as f64);let fy=((y as f64+0.5)*old.height as f64/height as f64-0.5).clamp(0.0,(old.height-1) as f64);
                    let x0=fx.floor() as u32;let y0=fy.floor() as u32;let tx=(fx-x0 as f64) as f32;let ty=(fy-y0 as f64) as f32;let mut sum=[0.0;4];
                    for (xx,yy,weight) in [(x0,y0,(1.0-tx)*(1.0-ty)),((x0+1).min(old.width-1),y0,tx*(1.0-ty)),(x0,(y0+1).min(old.height-1),(1.0-tx)*ty),((x0+1).min(old.width-1),(y0+1).min(old.height-1),tx*ty)] {let p=pixel(source,xx,yy);for c in 0..4 {sum[c]+=p[c]*weight;}}clean(sum)
                } else {
                    let (sx,sy)=match operation.as_str() {"rotate-cw"=>(i64::from(y),i64::from(old.height)-1-i64::from(x)),"rotate-ccw"=>(i64::from(old.width)-1-i64::from(y),i64::from(x)),"crop"=>(i64::from(x)+i64::from(offset_x),i64::from(y)+i64::from(offset_y)),_=>(i64::from(x)-i64::from(offset_x),i64::from(y)-i64::from(offset_y))};
                    if sx>=0 && sy>=0 && sx<i64::from(old.width) && sy<i64::from(old.height) {pixel(source,sx as u32,sy as u32)} else {[0.0;4]}
                };put(target,x,y,p,&mut allocated)?;
            }}
        }s.commit(next)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn geometry_keeps_hdr_and_undo()->Result<(),MobileError> {let d=DrawingTiledDocument::new(2,1)?;d.write_region(1,0,0,2,1,vec![8.0,0.0,0.0,1.0,0.0,2.0,0.0,0.5])?;d.transform_float("rotate-cw".into(),0,0,0,0)?;assert_eq!(d.info()?.height,2);assert_eq!(d.read_region(1,0,1,1,1)?,vec![0.0,2.0,0.0,0.5]);d.undo()?;assert_eq!(d.info()?.width,2);Ok(())}
    #[test] fn identity_curve_keeps_hdr()->Result<(),MobileError> {let d=DrawingTiledDocument::new(1,1)?;d.write_region(1,0,0,1,1,vec![4.0,2.0,0.0,1.0])?;d.apply_curve_float(1,(0..=255).collect(),0,vec![],false)?;assert_eq!(d.read_region(1,0,0,1,1)?,vec![4.0,2.0,0.0,1.0]);Ok(())}
    #[test] fn remap_preserves_hdr_and_selection()->Result<(),MobileError> {let d=DrawingTiledDocument::new(2,1)?;d.write_region(1,0,0,2,1,vec![8.0,0.0,0.0,1.0,0.0,4.0,0.0,1.0])?;d.remap_float(1,"flip-x".into(),vec![],false)?;assert_eq!(d.read_region(1,0,0,1,1)?,vec![0.0,4.0,0.0,1.0]);d.remap_float(1,"flip-x".into(),vec![0,0],false)?;assert_eq!(d.read_region(1,0,0,1,1)?,vec![0.0,4.0,0.0,1.0]);Ok(())}
    #[test] fn crop_and_canvas_preserve_exact_float_samples()->Result<(),MobileError> {let d=DrawingTiledDocument::new(2,1)?;d.write_region(1,1,0,1,1,vec![3.5,0.0,0.0,0.5])?;d.transform_float("crop".into(),1,1,1,0)?;assert_eq!(d.read_region(1,0,0,1,1)?,vec![3.5,0.0,0.0,0.5]);d.transform_float("canvas".into(),2,1,1,0)?;assert_eq!(d.read_region(1,1,0,1,1)?,vec![3.5,0.0,0.0,0.5]);assert_eq!(d.read_region(1,0,0,1,1)?,vec![0.0;4]);Ok(())}
    #[test] fn bilinear_scale_uses_premultiplied_hdr()->Result<(),MobileError> {let d=DrawingTiledDocument::new(2,1)?;d.write_region(1,0,0,2,1,vec![4.0,0.0,0.0,1.0,0.0,0.0,0.0,0.0])?;d.transform_float("scale".into(),1,1,0,0)?;assert_eq!(d.read_region(1,0,0,1,1)?,vec![2.0,0.0,0.0,0.5]);Ok(())}
    #[test] fn filter_selection_and_alpha_lock()->Result<(),MobileError> {let d=DrawingTiledDocument::new(1,1)?;d.write_region(1,0,0,1,1,vec![0.25,0.0,0.0,0.5])?;d.filter_float(1,"opaque".into(),0.0,vec![],true)?;assert_eq!(d.read_region(1,0,0,1,1)?,vec![0.25,0.0,0.0,0.5]);d.filter_float(1,"invert".into(),0.0,vec![0],false)?;assert_eq!(d.read_region(1,0,0,1,1)?,vec![0.25,0.0,0.0,0.5]);Ok(())}
}
