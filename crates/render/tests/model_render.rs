//! The 3D object effect renders imported GLB models with their own materials and animation.
use filmcraft_project::{ParamValue, find_effect};
use filmcraft_render::effects::FxCtx;
use filmcraft_render::image::Image;
use filmcraft_time::Tick;

fn fixture(name: &str) -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name).to_string_lossy().into_owned()
}

fn ctx(seconds: f64) -> FxCtx<'static> {
    FxCtx {
        t: Tick::from_seconds_f64(seconds), px_scale: 1.0, seconds,
        timecode: "00:00:00:00", clip_name: "model", project: None, env: None,
        frame_rate: 24.0, working: filmcraft_color::WorkingSpace::Rec709,
    }
}

fn model_effect(path: &str) -> filmcraft_project::EffectInstance {
    let mut e = find_effect("object_3d").unwrap().instance();
    e.params.get_mut("mesh").unwrap().value = ParamValue::Choice(7);
    e.params.get_mut("model_path").unwrap().value = ParamValue::Text(path.into());
    e.params.get_mut("model_anim").unwrap().value = ParamValue::Float(0.0);
    e.params.get_mut("size").unwrap().value = ParamValue::Float(120.0);
    e.params.get_mut("z").unwrap().value = ParamValue::Float(300.0);
    e
}

fn render(e: &filmcraft_project::EffectInstance, seconds: f64) -> Image {
    let mut img = Image::new(160, 120);
    filmcraft_render::effects::apply(&mut img, e, &ctx(seconds));
    img
}

fn coverage(img: &Image) -> usize {
    img.px.chunks_exact(4).filter(|p| p[3] > 0.05).count()
}

#[test]
fn textured_model_draws_pbr_pixels() {
    let e = model_effect(&fixture("BoxTextured.glb"));
    let img = render(&e, 0.0);
    assert!(coverage(&img) > 500, "model covered only {} pixels", coverage(&img));
    let colors: std::collections::HashSet<_> = img.px.chunks_exact(4).filter(|p| p[3] > 0.5).map(|p| [(p[0] * 31.0) as u8, (p[1] * 31.0) as u8, (p[2] * 31.0) as u8]).collect();
    assert!(colors.len() > 8, "textured model had only {} shades", colors.len());
}

#[test]
fn rigged_and_morph_models_render_and_animate() {
    for name in ["RiggedSimple.glb", "AnimatedMorphCube.glb"] {
        let e = model_effect(&fixture(name));
        let a = render(&e, 0.0);
        let b = render(&e, 0.4);
        assert!(coverage(&a) > 100, "{name} did not render");
        let diff = a.px.iter().zip(&b.px).filter(|(x, y)| (*x - *y).abs() > 1e-3).count();
        assert!(diff > 20, "{name} animation changed only {diff} samples");
    }
}

#[test]
fn missing_model_leaves_the_layer_unchanged() {
    let e = model_effect("/does/not/exist.glb");
    let img = render(&e, 0.0);
    assert_eq!(coverage(&img), 0);
}
