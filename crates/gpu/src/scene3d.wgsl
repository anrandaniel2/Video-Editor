// Perspective-correct 3D rasterization. All material sampling and scene lighting executes here.
struct Scene { camera: vec4<f32>, size: vec4<f32>, ambient: vec4<f32> }
struct Light { color: vec4<f32>, direction: vec4<f32>, position: vec4<f32>, cone: vec4<f32> }
struct Material { base: vec4<f32>, props: vec4<f32>, emissive: vec4<f32>, flags: vec4<f32> }
@group(0) @binding(0) var<uniform> scene: Scene;
@group(0) @binding(1) var<storage, read> lights: array<Light>;
@group(0) @binding(2) var<uniform> mat: Material;
@group(0) @binding(3) var base_tex: texture_2d<f32>;
@group(0) @binding(4) var mr_tex: texture_2d<f32>;
@group(0) @binding(5) var normal_tex: texture_2d<f32>;
@group(0) @binding(6) var occ_tex: texture_2d<f32>;
@group(0) @binding(7) var emit_tex: texture_2d<f32>;
@group(0) @binding(8) var base_s: sampler;
@group(0) @binding(9) var mr_s: sampler;
@group(0) @binding(10) var normal_s: sampler;
@group(0) @binding(11) var occ_s: sampler;
@group(0) @binding(12) var emit_s: sampler;
@group(0) @binding(13) var<storage, read> deformation: array<vec4<f32>>;
struct Input {
    @location(0) position: vec3<f32>, @location(1) normal: vec3<f32>,
    @location(2) tangent: vec4<f32>, @location(3) color: vec4<f32>,
    @location(4) uv0: vec2<f32>, @location(5) uv1: vec2<f32>, @location(6) uv2: vec2<f32>,
    @location(7) uv3: vec2<f32>, @location(8) uv4: vec2<f32>,
    @location(9) joints: vec4<f32>, @location(10) weights: vec4<f32>, @location(11) morph_index: f32,
}
struct Varying {
    @builtin(position) clip: vec4<f32>,
    @location(0) position: vec3<f32>, @location(1) normal: vec3<f32>,
    @location(2) tangent: vec4<f32>, @location(3) color: vec4<f32>,
    @location(4) uv0: vec2<f32>, @location(5) uv1: vec2<f32>, @location(6) uv2: vec2<f32>,
    @location(7) uv3: vec2<f32>, @location(8) uv4: vec2<f32>,
}
fn matrix_at(i:u32) -> mat4x4<f32> { return mat4x4<f32>(deformation[i],deformation[i+1u],deformation[i+2u],deformation[i+3u]); }
@vertex fn vs(v: Input) -> Varying {
    var position=v.position; var normal=v.normal; var tangent=v.tangent.xyz;
    let info=deformation[16u];
    if info.x>0.5 {
        let joint_count=u32(info.y); let morph_count=u32(info.z); let vertex_count=u32(info.w);
        let weights_start=17u+joint_count*8u;
        let deltas_start=weights_start+morph_count;
        for(var morph=0u; morph<morph_count; morph=morph+1u) {
            let weight=deformation[weights_start+morph].x;
            let offset=deltas_start+(morph*vertex_count+u32(v.morph_index))*3u;
            position=position+weight*deformation[offset].xyz;
            normal=normal+weight*deformation[offset+1u].xyz;
            tangent=tangent+weight*deformation[offset+2u].xyz;
        }
        var p=vec3<f32>(0.0); var n=vec3<f32>(0.0); var t=vec3<f32>(0.0); var total=0.0;
        for(var k=0u; k<4u; k=k+1u) {
            let joint=u32(v.joints[k]); let weight=v.weights[k];
            if weight>0.0 && joint<joint_count {
                let m=matrix_at(17u+joint*8u); let nm=matrix_at(21u+joint*8u);
                p=p+weight*(m*vec4<f32>(position,1.0)).xyz;
                n=n+weight*(nm*vec4<f32>(normal,0.0)).xyz;
                t=t+weight*(m*vec4<f32>(tangent,0.0)).xyz;
                total=total+weight;
            }
        }
        if total>0.000001 {position=p;normal=safe_normal(n);tangent=safe_normal(t);}
        else {
            let m=matrix_at(8u); let nm=matrix_at(12u);
            position=(m*vec4<f32>(position,1.0)).xyz;
            normal=safe_normal((nm*vec4<f32>(normal,0.0)).xyz);
            tangent=safe_normal((m*vec4<f32>(tangent,0.0)).xyz);
        }
        position=(matrix_at(0u)*vec4<f32>(position,1.0)).xyz;
        normal=(matrix_at(4u)*vec4<f32>(normal,0.0)).xyz;
        tangent=(matrix_at(0u)*vec4<f32>(tangent,0.0)).xyz;
    }
    var o: Varying;
    // Right-handed editor camera looks down +Z; near plane at z=1. Infinite far plane.
    o.clip = vec4<f32>(
        2.0 * (scene.camera.x * position.z + scene.camera.z * position.x) / scene.size.x - position.z,
        position.z - 2.0 * (scene.camera.y * position.z + scene.camera.z * position.y) / scene.size.y,
        position.z - 1.0, position.z);
    o.position=position; o.normal=normal; o.tangent=vec4<f32>(tangent,v.tangent.w); o.color=v.color;
    o.uv0=v.uv0; o.uv1=v.uv1; o.uv2=v.uv2; o.uv3=v.uv3; o.uv4=v.uv4;
    return o;
}
fn safe_normal(v: vec3<f32>) -> vec3<f32> { return v / max(length(v), 0.000001); }
@fragment fn fs(v: Varying, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    // Sample before divergent control flow, keeping implicit derivatives well-defined.
    let tex=textureSample(base_tex,base_s,v.uv0);
    let mr=textureSample(mr_tex,mr_s,v.uv1);
    let nt=textureSample(normal_tex,normal_s,v.uv2).xyz;
    let ao=1.0 + mat.props.w*(textureSample(occ_tex,occ_s,v.uv3).r-1.0);
    let emit=mat.emissive.xyz*textureSample(emit_tex,emit_s,v.uv4).xyz;
    let dx=dpdx(v.position); let dy=dpdy(v.position);
    let duvdx=dpdx(v.uv2); let duvdy=dpdy(v.uv2);
    let pbr=mat.flags.w>0.5;
    if pbr && mat.flags.y<0.5 && !front { discard; }
    var n=safe_normal(v.normal);
    if pbr {
        if !front { n = -n; }
        if mat.emissive.w>0.5 {
            var t=v.tangent.xyz;
            var bit=vec3<f32>(0.0);
            if dot(t,t)<0.000001 {
                let det=duvdx.x*duvdy.y-duvdx.y*duvdy.x;
                t=safe_normal((dx*duvdy.y-dy*duvdx.y)*sign(det));
                bit=safe_normal((-dx*duvdy.x+dy*duvdx.x)*sign(det));
            } else {
                t=safe_normal(t-n*dot(t,n));
                bit=cross(n,t)*v.tangent.w;
            }
            let mapped=vec3<f32>((nt.xy*2.0-1.0)*mat.props.z,max(nt.z*2.0-1.0,0.0001));
            n=safe_normal(t*mapped.x+bit*mapped.y+n*mapped.z);
        }
    } else if n.z>0.0 { n = -n; }
    let base=max(mat.base.xyz*tex.xyz*v.color.xyz,vec3<f32>(0.0));
    var alpha=mat.base.w*tex.w*v.color.w*scene.camera.w;
    if pbr {
        if mat.flags.x<0.5 { alpha=scene.camera.w; }
        else if mat.flags.x<1.5 {
            if alpha<scene.size.z { discard; }
            alpha=scene.camera.w;
        }
    }
    alpha=clamp(alpha,0.0,1.0);
    if alpha<=0.0 { discard; }
    if mat.flags.z>0.5 { return vec4<f32>((base+emit)*alpha,alpha); }
    let metallic=clamp(mat.props.x*mr.b,0.0,1.0);
    let rough=clamp(mat.props.y*mr.g,0.045,1.0);
    var view=vec3<f32>(0.0,0.0,-1.0);
    if pbr { view=safe_normal(-v.position); }
    let nv=max(dot(n,view),0.0001);
    let a2=pow(rough,4.0);
    let k=pow(rough+1.0,2.0)/8.0;
    let f0=mix(vec3<f32>(0.04),base,metallic);
    var direct=vec3<f32>(0.0); var specular=vec3<f32>(0.0);
    for(var i=0u; i<u32(scene.ambient.w); i=i+1u) {
        let light=lights[i];
        var toward=light.direction.xyz; var weight=1.0;
        if light.color.w>0.5 {
            let to=light.position.xyz-v.position;
            let distance=max(length(to),0.0001); toward=to/distance;
            if light.position.w>0.0 { weight=pow(1.0-pow(min(distance/light.position.w,1.0),2.0),2.0); }
            if light.color.w>1.5 {
                let cone=clamp((dot(toward,light.direction.xyz)-light.cone.x)/max(light.cone.y-light.cone.x,0.000001),0.0,1.0);
                weight=weight*cone*cone*(3.0-2.0*cone);
            }
        }
        let nl=max(dot(n,toward),0.0);
        let h=safe_normal(view+toward);
        if pbr {
            let nh=max(dot(n,h),0.0); let vh=max(dot(view,h),0.0);
            let den=nh*nh*(a2-1.0)+1.0;
            let d=a2/max(3.14159265*den*den,0.000001);
            let g=(nv/(nv*(1.0-k)+k))*(nl/max(nl*(1.0-k)+k,0.000001));
            let f=f0+(vec3<f32>(1.0)-f0)*pow(1.0-vh,5.0);
            let kd=(vec3<f32>(1.0)-f)*(1.0-metallic);
            let spec=d*g*f/max(4.0*nv*nl,0.00001);
            direct=direct+(kd*base/3.14159265+spec)*nl*3.0*weight*light.color.xyz;
        } else {
            direct=direct+light.color.xyz*nl*weight;
            specular=specular+light.color.xyz*pow(max(dot(n,h),0.0),2.0+(1.0-mat.props.y)*96.0)*weight;
        }
    }
    var color: vec3<f32>;
    if pbr {
        color=direct+base*(1.0-metallic)*0.22*ao*scene.ambient.xyz+f0*(0.08+0.12*(1.0-rough))*ao*scene.ambient.xyz+emit;
    } else {
        color=clamp(base*(0.28*scene.ambient.xyz+0.72*direct)+specular*mat.props.x*0.9,vec3<f32>(0.0),vec3<f32>(1.0));
    }
    return vec4<f32>(max(color,vec3<f32>(0.0))*alpha,alpha);
}
