//! Validate shader semantics on CI even when no hardware adapter is attached.
#[test]
fn scene3d_shader_is_valid() {
    let source=include_str!("../src/scene3d.wgsl");
    let module=naga::front::wgsl::parse_str(source).unwrap_or_else(|e| panic!("{}",e.emit_to_string(source)));
    naga::valid::Validator::new(naga::valid::ValidationFlags::all(),naga::valid::Capabilities::empty())
        .validate(&module).unwrap_or_else(|e| panic!("{}",e.emit_to_string(source)));
}
