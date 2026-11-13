struct DabVertex {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(perspective, sample) local: vec2<f32>,
    @location(1) @interpolate(flat) parameters: vec4<f32>,
};
@vertex fn vs_dab(@location(0) position: vec2<f32>, @location(1) local: vec2<f32>, @location(2) parameters: vec4<f32>) -> DabVertex {
    var result: DabVertex;
    result.position = vec4<f32>(position, 0.0, 1.0);
    result.local = local;
    result.parameters = parameters;
    return result;
}
fn noise(cell: vec2<f32>, seed: f32) -> f32 {
    return fract(sin(dot(cell, vec2<f32>(127.1, 311.7)) + seed * 17.13) * 43758.5453);
}
@fragment fn fs_dab(input: DabVertex, @builtin(sample_index) sample: u32) -> @location(0) vec4<f32> {
    let p = input.local;
    let hardness = input.parameters.x;
    let flow = input.parameters.y;
    let tip = u32(input.parameters.z + 0.5);
    let seed = input.parameters.w;
    let radius = length(p);
    var distance = radius;
    var texture_alpha = 1.0;
    switch tip {
        case 1u: { distance = max(abs(p.x), abs(p.y)); }
        case 2u: { distance = abs(p.x) + abs(p.y); }
        case 3u: {
            let angle = atan2(p.y, p.x);
            let boundary = 0.62 + 0.38 * cos(angle * 5.0);
            distance = radius / max(boundary, 0.1);
        }
        case 4u: { distance = abs(radius - 0.73) / 0.27; }
        case 5u: { texture_alpha = select(0.2, 1.0, fract((p.x + 1.0) * 9.0) < 0.3); }
        case 6u: { texture_alpha = select(0.0, 0.85, noise(floor(p * 32.0), seed) > 0.37); }
        case 7u: {
            let cell = floor(p * 7.0);
            let centre = vec2<f32>(noise(cell, seed), noise(cell, seed + 11.0));
            let delta = fract(p * 7.0) - centre;
            texture_alpha = select(0.0, 1.0, length(delta) < 0.25);
        }
        default: {}
    }
    if distance > 1.0 { discard; }
    var alpha = 1.0;
    if hardness < 0.999 {
        alpha = 1.0 - smoothstep(hardness, 1.0, distance);
    }
    // sample_index requests sample-rate shading on the real four-sample attachment.
    if sample >= 4u { discard; }
    return vec4<f32>(1.0, 1.0, 1.0, alpha * texture_alpha * flow);
}
