//! Validate shader semantics on CI even when no hardware adapter is attached.
#[test]
fn scene3d_shader_is_valid() {
    let source=include_str!("../src/scene3d.wgsl");
    let module=naga::front::wgsl::parse_str(source).unwrap_or_else(|e| panic!("{}",e.emit_to_string(source)));
    naga::valid::Validator::new(naga::valid::ValidationFlags::all(),naga::valid::Capabilities::empty())
        .validate(&module).unwrap_or_else(|e| panic!("{}",e.emit_to_string(source)));
}

#[test]
fn scene3d_hardware_depth_and_skinning() {
    use filmcraft_render::scene3d::{Scene,Draw,Vertex};
    use filmcraft_render::model::{Material,math};
    // Syntax/semantics are always validated above; exercise the actual pipeline when CI has a
    // Vulkan adapter (including Mesa's software implementation).
    if filmcraft_gpu::ExportRenderer::new().is_none() { return; }
    let vertex=|x,y,z| Vertex { position:[x,y,z],normal:[0.0,0.0,-1.0],tangent:[1.0,0.0,0.0,1.0],uv:[[0.0;2];2],color:[1.0;4],joints:[0.0;4],weights:[1.0,0.0,0.0,0.0],morph_index:0.0 };
    let mut deformation=Vec::new();
    for _ in 0..4 {deformation.extend(math::IDENTITY);}
    deformation.extend([1.0,1.0,0.0,3.0]);
    for _ in 0..2 {deformation.extend(math::IDENTITY);}
    let near=Draw {vertices:vec![vertex(-20.0,-20.0,50.0),vertex(20.0,-20.0,50.0),vertex(0.0,20.0,50.0)],material:Material {base_color:[0.0,0.8,0.0,1.0],double_sided:true,unlit:true,..Default::default()},model:None,pbr:true,deformation};
    let far=Draw {vertices:vec![vertex(-40.0,-40.0,100.0),vertex(40.0,-40.0,100.0),vertex(0.0,40.0,100.0)],material:Material {base_color:[0.8,0.0,0.0,1.0],double_sided:true,unlit:true,..Default::default()},model:None,pbr:true,deformation:Vec::new()};
    let scene=Scene {width:64,height:64,camera:[32.0,32.0,64.0],ambient:[1.0;3],opacity:1.0,lights:Vec::new(),draws:vec![near,far]};
    let image=filmcraft_gpu::scene3d::render(&scene).expect("an available GPU must render the scene");
    let i=(32*64+32)*4;
    assert!(image.px[i]<0.01 && (image.px[i+1]-0.8).abs()<0.01 && image.px[i+3]>0.99,"near green triangle must win depth test: {:?}",&image.px[i..i+4]);
}
