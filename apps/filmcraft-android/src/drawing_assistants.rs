//! Painting-assistant constraints in canvas space. Guides are UI-only; snapping is native.
use crate::{drawing::DrawingInput,MobileError};
#[derive(Clone,Debug,uniffi::Record)]
pub struct DrawingAssistantSettings {
    /// 0=off, 1=ruler, 2=parallel ruler, 3=rotated grid points, 4=radial line, 5=concentric circle.
    pub mode:u32,
    pub origin_x:f32,
    pub origin_y:f32,
    pub angle_degrees:f32,
    pub spacing:f32,
    pub strength:f32,
}
#[uniffi::export]
pub fn drawing_assistant_sample(sample:DrawingInput,anchor:DrawingInput,settings:DrawingAssistantSettings)->Result<DrawingInput,MobileError> {
    let s=settings;
    if s.mode>5 || [sample.x,sample.y,anchor.x,anchor.y,s.origin_x,s.origin_y,s.angle_degrees,s.spacing,s.strength].iter().any(|v|!v.is_finite()) || !(1.0..=4096.0).contains(&s.spacing) || !(0.0..=1.0).contains(&s.strength) {return Err(MobileError::Command("Invalid assistant geometry".into()));}
    if s.mode==0 || s.strength==0.0 {return Ok(sample);}
    let angle=s.angle_degrees.to_radians();let ux=angle.cos();let uy=angle.sin();
    let dx=sample.x-s.origin_x;let dy=sample.y-s.origin_y;
    let project=|ox:f32,oy:f32,vx:f32,vy:f32| {let distance=(sample.x-ox)*vx+(sample.y-oy)*vy;(ox+distance*vx,oy+distance*vy)};
    let (x,y)=match s.mode {
        1=>project(s.origin_x,s.origin_y,ux,uy),
        2=>project(anchor.x,anchor.y,ux,uy),
        3=>{let a=((dx*ux+dy*uy)/s.spacing).round()*s.spacing;let b=((-dx*uy+dy*ux)/s.spacing).round()*s.spacing;(s.origin_x+a*ux-b*uy,s.origin_y+a*uy+b*ux)},
        4=>{let ax=anchor.x-s.origin_x;let ay=anchor.y-s.origin_y;let length=ax.hypot(ay);if length>0.001 {project(s.origin_x,s.origin_y,ax/length,ay/length)} else {project(s.origin_x,s.origin_y,ux,uy)}},
        _=>{let radius=(anchor.x-s.origin_x).hypot(anchor.y-s.origin_y);let radius=if radius<0.001 {s.spacing} else {radius};let length=dx.hypot(dy);if length>0.001 {(s.origin_x+dx/length*radius,s.origin_y+dy/length*radius)} else {(s.origin_x+ux*radius,s.origin_y+uy*radius)}},
    };
    let x=sample.x+(x-sample.x)*s.strength;let y=sample.y+(y-sample.y)*s.strength;
    if !x.is_finite() || !y.is_finite() {return Err(MobileError::Command("Assistant geometry exceeds numeric limits".into()));}
    Ok(DrawingInput {x,y,pressure:sample.pressure})
}
#[cfg(test)]
mod tests {
    use super::*;
    fn settings(mode:u32)->DrawingAssistantSettings {DrawingAssistantSettings {mode,origin_x:0.0,origin_y:0.0,angle_degrees:0.0,spacing:10.0,strength:1.0}}
    fn point(x:f32,y:f32)->DrawingInput {DrawingInput {x,y,pressure:0.4}}
    #[test] fn ruler_and_parallel()->Result<(),MobileError> {
        let ruler=drawing_assistant_sample(point(20.0,30.0),point(5.0,7.0),settings(1))?;assert_eq!((ruler.x,ruler.y),(20.0,0.0));
        let parallel=drawing_assistant_sample(point(20.0,30.0),point(5.0,7.0),settings(2))?;assert_eq!((parallel.x,parallel.y),(20.0,7.0));assert_eq!(parallel.pressure,0.4);Ok(())
    }
    #[test] fn circle_and_grid()->Result<(),MobileError> {
        let circle=drawing_assistant_sample(point(0.0,100.0),point(10.0,0.0),settings(5))?;assert!((circle.x.hypot(circle.y)-10.0).abs()<0.001);
        let grid=drawing_assistant_sample(point(14.0,26.0),point(0.0,0.0),settings(3))?;assert_eq!((grid.x,grid.y),(10.0,30.0));Ok(())
    }
    #[test] fn strength_zero_is_passthrough()->Result<(),MobileError> {
        let mut s=settings(1);s.strength=0.0;let p=drawing_assistant_sample(point(3.0,7.0),point(0.0,0.0),s)?;assert_eq!((p.x,p.y),(3.0,7.0));Ok(())
    }
}

#[cfg(test)]
mod additional_tests {
    use super::*;
    #[test] fn radial_projection_and_partial_strength()->Result<(),MobileError> {
        let anchor=DrawingInput {x:10.0,y:10.0,pressure:1.0};let sample=DrawingInput {x:20.0,y:0.0,pressure:0.5};
        let settings=DrawingAssistantSettings {mode:4,origin_x:0.0,origin_y:0.0,angle_degrees:0.0,spacing:10.0,strength:0.5};
        let result=drawing_assistant_sample(sample,anchor,settings)?;
        assert!((result.x-15.0).abs()<0.001);assert!((result.y-5.0).abs()<0.001);assert_eq!(result.pressure,0.5);Ok(())
    }
    #[test] fn invalid_settings_are_rejected() {
        let sample=DrawingInput {x:1.0,y:2.0,pressure:1.0};
        let settings=DrawingAssistantSettings {mode:3,origin_x:0.0,origin_y:0.0,angle_degrees:0.0,spacing:0.0,strength:1.0};
        assert!(drawing_assistant_sample(sample,sample,settings).is_err());
    }
}
