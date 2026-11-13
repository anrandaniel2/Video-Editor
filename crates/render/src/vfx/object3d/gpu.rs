//! Draw-packet construction. No lighting or rasterization is performed here.
use super::*;
use crate::scene3d::{Draw, Light, Scene, Vertex};

fn xyz(v: V3) -> [f32; 3] { [v.x, v.y, v.z] }
fn vertex(v: Vert) -> Vertex {
    Vertex { position: xyz(v.p), normal: xyz(v.n), tangent: [0.0, 0.0, 0.0, 1.0], uv: [[0.0; 2]; 2], color: [1.0; 4], joints:[0.0;4], weights:[0.0;4], morph_index:0.0 }
}

pub(super) fn gpu_scene(img: &Image, p: &Params, cx: &FxCtx) -> Option<Scene> {
    let mut scene = Scene {
        width: img.w, height: img.h,
        camera: [p.pos.0, p.pos.1, img.h.max(1) as f32],
        ambient: p.ambient, opacity: p.opacity,
        lights: p.lights.iter().map(|l| Light {
            kind: l.kind, color: l.color, direction: xyz(l.toward), position: xyz(l.pos),
            range: l.range, cos_outer: l.cos_outer, cos_inner: l.cos_inner,
        }).collect(), draws: Vec::new(),
    };
    let off = p.translation;
    let place = vertex;
    // Primitive/text vertex transforms are applied by the same vertex stage as model skinning.
    let primitive_deformation = || {
        use crate::model::math;
        let x=rotate(V3::new(1.0,0.0,0.0),p.rot);
        let y=rotate(V3::new(0.0,1.0,0.0),p.rot);
        let z=rotate(V3::new(0.0,0.0,1.0),p.rot);
        let object=[x.x,x.y,x.z,0.0,y.x,y.y,y.z,0.0,z.x,z.y,z.z,0.0,off.x,off.y,off.z,1.0];
        let mut data=Vec::new(); data.extend(object); data.extend(math::normal_matrix(&object));
        data.extend(math::IDENTITY);data.extend(math::IDENTITY);data.extend([1.0,0.0,0.0,0.0]);
        data
    };
    let material = |col: [f32; 4]| crate::model::Material {
        base_color: { let c = lin(col); [c[0], c[1], c[2], col[3]] },
        metallic: p.metal, roughness: p.rough, double_sided: true,
        ..Default::default()
    };
    if p.mesh == 7 {
        let model = crate::model::load_cached(std::path::Path::new(&p.model_path)).ok()?;
        let anim = if p.model_anim >= 0 { Some(p.model_anim as usize) } else { None };
        let time = p.model_time + if p.model_play { cx.seconds as f32 * p.model_speed } else { 0.0 };
        let pose = model.pose(anim, time, p.model_loop);
        let center = std::array::from_fn::<_, 3, _>(|i| (model.bounds.0[i] + model.bounds.1[i]) * 0.5);
        let extent = (0..3).map(|i| model.bounds.1[i] - model.bounds.0[i]).fold(0.0f32, f32::max).max(1e-6);
        let fit = p.size * p.scale / extent;
        for inst in &model.instances {
            let prim = &model.meshes[inst.mesh].prims[inst.prim];
            use crate::model::math;
            let axis = |v| xyz(rotate(v,p.rot));
            let x=axis(V3::new(fit,0.0,0.0)); let y=axis(V3::new(0.0,-fit,0.0)); let z=axis(V3::new(0.0,0.0,-fit));
            let t=rotate(V3::new(-center[0]*fit,center[1]*fit,center[2]*fit),p.rot).add(off);
            let object=[x[0],x[1],x[2],0.0,y[0],y[1],y[2],0.0,z[0],z[1],z[2],0.0,t.x,t.y,t.z,1.0];
            let node=pose.world.get(inst.node).copied().unwrap_or(math::IDENTITY);
            let bones=inst.skin.and_then(|i| pose.skins.get(i));
            let bone_normals=inst.skin.and_then(|i| pose.skin_normals.get(i));
            let count=bones.map(Vec::len).unwrap_or(0);
            let mut deformation=Vec::new();
            deformation.extend(object); deformation.extend(math::normal_matrix(&object));
            deformation.extend(node); deformation.extend(math::normal_matrix(&node));
            deformation.extend([1.0,count as f32,prim.morphs.len() as f32,prim.verts.len() as f32]);
            if let Some(bones)=bones {
                for (i,m) in bones.iter().enumerate() {
                    deformation.extend(*m);
                    deformation.extend(bone_normals.and_then(|n| n.get(i)).copied().unwrap_or(math::IDENTITY));
                }
            }
            for i in 0..prim.morphs.len() {
                let weight=pose.weights.get(inst.node).and_then(|w| w.get(i)).copied().filter(|w| w.is_finite()).unwrap_or(0.0);
                deformation.extend([weight,0.0,0.0,0.0]);
            }
            for target in &prim.morphs {
                for i in 0..prim.verts.len() {
                    for delta in [&target.dpos,&target.dnormal,&target.dtangent] {
                        let d=delta.get(i).copied().unwrap_or([0.0;3]);
                        deformation.extend([d[0],d[1],d[2],0.0]);
                    }
                }
            }
            let vertices = prim.indices.iter().map(|&i| {
                let v=prim.verts[i as usize];
                Vertex { position:v.pos,normal:v.normal,tangent:v.tangent,uv:v.uv,color:v.color,
                    joints:v.joints.map(|j|j as f32),weights:v.weights,morph_index:i as f32 }
            }).collect();
            let mut material = model.material(prim.material).clone();
            let tint = lin(p.color);
            for i in 0..3 { material.base_color[i] *= tint[i]; }
            scene.draws.push(Draw { vertices, material, model: Some(model.clone()), pbr: true, deformation });
        }
    } else if p.mesh == 6 {
        let cs = text_contours(p);
        let depth = p.depth.max(0.5);
        let bevel = p.bevel.min(depth * 0.45);
        let front = if bevel > 0.25 { cs.iter().map(|c| inset(c, 0.94)).collect::<Vec<_>>() } else { cs.clone() };
        let mut face_vertices = Vec::new();
        let mut side_vertices = Vec::new();
        // Even-odd tessellation retains holes (O/B/etc.) and disconnected glyphs.
        for (contours, z, normal, output) in [(&cs, 0.0, V3::new(0.0,0.0,1.0), &mut side_vertices), (&front, -depth, V3::new(0.0,0.0,-1.0), &mut face_vertices)] {
            let mut path = lyon_path::Path::builder();
            for ct in contours.iter().filter(|c| c.len() >= 3) {
                path.begin(lyon_path::math::point(ct[0].0, ct[0].1));
                for &(x,y) in &ct[1..] { path.line_to(lyon_path::math::point(x,y)); }
                path.close();
            }
            let path = path.build();
            let mut geometry: lyon_tessellation::VertexBuffers<[f32; 2], u32> = lyon_tessellation::VertexBuffers::new();
            lyon_tessellation::FillTessellator::new().tessellate_path(
                &path, &lyon_tessellation::FillOptions::default().with_fill_rule(lyon_tessellation::FillRule::EvenOdd),
                &mut lyon_tessellation::BuffersBuilder::new(&mut geometry, |v: lyon_tessellation::FillVertex<'_>| { let p = v.position(); [p.x,p.y] }),
            ).ok()?;
            output.extend(geometry.indices.iter().map(|&i| { let v = geometry.vertices[i as usize]; place(Vert { p: V3::new(v[0],v[1],z), n: normal }) }));
        }
        for ct in &cs {
            let ins = inset(ct, 0.94);
            for i in 0..ct.len() {
                let j = (i+1)%ct.len();
                let (x0,y0)=ct[i]; let (x1,y1)=ct[j];
                let n = V3::new(y1-y0,x0-x1,0.0).norm();
                let top = -depth+bevel;
                let q = [V3::new(x0,y0,0.0),V3::new(x1,y1,0.0),V3::new(x1,y1,top),V3::new(x0,y0,top)];
                side_vertices.extend([0,1,2,0,2,3].map(|k| place(Vert { p:q[k],n })));
                if bevel > 0.25 && ins.len()==ct.len() {
                    let (bx0,by0)=ins[i]; let (bx1,by1)=ins[j];
                    let n=V3::new(y1-y0,x0-x1,-(x1-x0).hypot(y1-y0)*0.7).norm();
                    let q=[V3::new(x0,y0,top),V3::new(x1,y1,top),V3::new(bx1,by1,-depth),V3::new(bx0,by0,-depth)];
                    face_vertices.extend([0,1,2,0,2,3].map(|k| place(Vert { p:q[k],n })));
                }
            }
        }
        scene.draws.push(Draw { vertices: face_vertices, material: material(p.color), model: None, pbr: false, deformation:primitive_deformation() });
        scene.draws.push(Draw { vertices: side_vertices, material: material(p.side), model: None, pbr: false, deformation:primitive_deformation() });
    } else {
        let k=p.size*p.scale*0.5;
        let dz=if p.mesh==5 {0.0} else if p.depth>0.0 {p.depth*p.scale*0.5} else {k};
        let radius=k*scene.camera[2]/p.z.max(1.0);
        let vertices=mesh_tris(p.mesh,k,dz,radius).into_iter().flat_map(|t| t.map(place)).collect();
        scene.draws.push(Draw { vertices, material:material(p.color), model:None, pbr:false, deformation:primitive_deformation() });
    }
    Some(scene)
}
