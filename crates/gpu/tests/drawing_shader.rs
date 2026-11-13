//! Source and hardware regression cases; not executed during the user's no-build phase.
#[test]
fn native_brush_shaders_are_valid() {
    for source in [include_str!("../src/drawing.wgsl"),include_str!("../src/drawing_dabs.wgsl")] {
        let module=naga::front::wgsl::parse_str(source).unwrap_or_else(|e|panic!("{}",e.emit_to_string(source)));
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(),naga::valid::Capabilities::all()).validate(&module).unwrap_or_else(|e|panic!("{}",e.emit_to_string(source)));
    }
}
#[test]
fn native_dab_flow_and_bounds_on_available_adapter() {
    if !filmcraft_gpu::drawing::available() {return;}
    let dabs=[[16.0,16.0,8.0,8.0,0.0,1.0,0.5,0.0,1.0]];
    let pixels=filmcraft_gpu::drawing::dab_coverage(&dabs,0,0,32,32).expect("available adapter should render a bounded dab");
    assert_eq!(pixels.len(),1024);assert_eq!(pixels[0],0);assert!((i32::from(pixels[16*32+16])-128).abs()<=1);
}

#[test]
fn fragment_shading_compatibility_shader_is_valid() {
    let source=include_str!("../src/drawing_dabs.wgsl").replace("@interpolate(perspective, sample)","@interpolate(perspective, center)").replace(", @builtin(sample_index) sample: u32", "").replace("if sample >= 4u { discard; }", "");
    let module=naga::front::wgsl::parse_str(&source).unwrap_or_else(|e|panic!("{}",e.emit_to_string(&source)));
    naga::valid::Validator::new(naga::valid::ValidationFlags::all(),naga::valid::Capabilities::all()).validate(&module).expect("per-fragment compatibility shader validates");
}
#[test]
fn hardware_status_and_flow_accumulation() {
    let status=filmcraft_gpu::drawing::status();
    if !filmcraft_gpu::drawing::available() {assert!(!status.verified);assert!(!status.detail.is_empty());return;}
    assert!(status.verified);assert!(!status.adapter.is_empty());assert!(status.max_dimension>=32);
    let dab=[16.25,16.25,8.0,8.0,0.0,1.0,0.5,0.0,1.0];
    let result=filmcraft_gpu::drawing::dab_coverage(&[dab,dab],0,0,32,32).expect("two hardware dabs");
    assert!((i32::from(result[16*32+16])-191).abs()<=2);assert_eq!(result[0],0);
}
#[test]
fn malformed_brush_input_is_rejected() {
    assert!(filmcraft_gpu::drawing::dab_coverage(&[[0.0,0.0,-1.0,1.0,0.0,1.0,1.0,0.0,0.0]],0,0,32,32).is_none());
    assert!(filmcraft_gpu::drawing::coverage(&[[f32::NAN,0.0,1.0]],0,0,32,32).is_none());
}
