use std::sync::atomic::{AtomicUsize, Ordering};

use filmcraft_project::{CLASSIC_FOV, CameraSolve, ClipId, SceneCamera3D, SequenceMatchMove, StereoLayout};
use filmcraft_time::Tick;

use super::ba::observation_jacobian;
use super::init::CameraPose;
use super::linalg::{Rng, Vec3, add3, dot3, mat3_mul, mat3_mul_vec, normalize3, project_to_so3, rodrigues_exp, rotation_angle_rad, sub3};
use super::{InputFrame, SolveError, SolveOptions, solve_camera, solve_frames};

fn tick_for_frame(i: usize, fps: i64) -> Tick {
    Tick::from_seconds_f64(i as f64 / fps as f64)
}

#[derive(Clone, Copy)]
struct Landmark {
    pos: [f64; 3],
    radius: f64,
    luma: f64,
    cross: bool,
}

fn make_3d_constellation(count: usize, min_z: f64, max_z: f64, spread_xy: f64, seed: u64) -> Vec<Landmark> {
    let mut rng = Rng::new(seed);
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let u1 = (rng.next_u64() % 10_000) as f64 / 10_000.0;
        let u2 = (rng.next_u64() % 10_000) as f64 / 10_000.0;
        let u3 = (rng.next_u64() % 10_000) as f64 / 10_000.0;
        let z = min_z + u3 * (max_z - min_z);
        let x = (u1 * 2.0 - 1.0) * spread_xy * (z / 700.0).max(0.6);
        let y = (u2 * 2.0 - 1.0) * (spread_xy * 0.62) * (z / 700.0).max(0.6);
        let luma = 155.0 + (i % 5) as f64 * 22.0;
        let radius = 2.0 + (i % 3) as f64 * 0.55;
        out.push(Landmark { pos: [x, y, z], radius, luma, cross: i % 2 == 0 });
    }
    out
}

/// Render a synthetic anti-aliased RGBA8 frame using the exact `SceneCamera3D` projection convention
/// from `crates/render/src/vfx/object3d.rs`.
fn render_synthetic_eye(width: usize, height: usize, cam: &SceneCamera3D, landmarks: &[Landmark], bg: [u8; 3]) -> Vec<u8> {
    let mut img = vec![0u8; width * height * 4];
    for px in img.as_chunks_mut::<4>().0 {
        px[0] = bg[0];
        px[1] = bg[1];
        px[2] = bg[2];
        px[3] = 255;
    }

    let vr_f32 = cam.view_rotation();
    let vr = vr_f32.map(|row| row.map(f64::from));
    let eye = cam.position.map(f64::from);
    let cx = width as f64 * 0.5;
    let cy = height as f64 * 0.5;
    let focal = (height as f64 * 0.5) / (f64::from(cam.fov).to_radians() * 0.5).tan();

    // Sort back-to-front so closer landmarks occlude farther ones cleanly.
    let mut projected: Vec<(f64, f64, f64, &Landmark)> = landmarks
        .iter()
        .filter_map(|lm| {
            let rel = sub3(lm.pos, eye);
            let pc = mat3_mul_vec(vr, rel);
            if pc[2] <= 5.0 {
                return None;
            }
            let u = cx + focal * (pc[0] / pc[2]);
            let v = cy + focal * (pc[1] / pc[2]);
            if u < -8.0 || u > width as f64 + 8.0 || v < -8.0 || v > height as f64 + 8.0 {
                return None;
            }
            Some((pc[2], u, v, lm))
        })
        .collect();
    projected.sort_by(|a, b| b.0.total_cmp(&a.0));

    for (_, u, v, lm) in projected {
        splat_landmark(&mut img, width, height, u, v, lm);
    }
    img
}

fn splat_landmark(img: &mut [u8], width: usize, height: usize, u: f64, v: f64, lm: &Landmark) {
    let rad = (lm.radius * 2.6).ceil() as i32;
    let x0 = ((u.floor() as i32) - rad).max(0);
    let x1 = ((u.ceil() as i32) + rad).min(width as i32 - 1);
    let y0 = ((v.floor() as i32) - rad).max(0);
    let y1 = ((v.ceil() as i32) + rad).min(height as i32 - 1);
    let inv_2sig2 = 1.0 / (2.0 * lm.radius * lm.radius);

    for py in y0..=y1 {
        let dy = py as f64 - v;
        for px in x0..=x1 {
            let dx = px as f64 - u;
            let d2 = dx * dx + dy * dy;
            let mut w = (-d2 * inv_2sig2).exp();
            if lm.cross {
                // Sub-pixel dark/bright asymmetric corner detail so Shi-Tomasi eigenvalues are strong.
                let arm_x = (-dy * dy * 0.9 - dx * dx * 0.12).exp();
                let arm_y = (-dx * dx * 0.9 - dy * dy * 0.12).exp();
                w = (0.55 * w + 0.45 * arm_x.max(arm_y)).clamp(0.0, 1.0);
            }
            if w < 0.01 {
                continue;
            }
            let idx = (py as usize * width + px as usize) * 4;
            for c in 0..3 {
                let base = f64::from(img[idx + c]);
                let target = lm.luma - (c as f64 * 12.0);
                img[idx + c] = (base * (1.0 - w) + target * w).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

#[test]
fn ba_jacobians_match_finite_differences() {
    let cam = CameraPose { r_wc: rodrigues_exp([0.12, -0.21, 0.08]), t_wc: [0.35, -0.18, 0.42], frozen: false, solved: true };
    let xw: Vec3 = [0.45, -0.32, 2.15];
    let obs = (262.4, 118.7);
    let intr = (240.0, 135.0, 270.0);
    let jac = observation_jacobian(&cam, xw, obs, intr, false).expect("in front of camera");

    let eps = 1e-6f64;
    let eval_res = |c: &CameraPose, p: Vec3, f: f64, rot_only: bool| -> (f64, f64) {
        let pc = if rot_only { mat3_mul_vec(c.r_wc, p) } else { add3(mat3_mul_vec(c.r_wc, p), c.t_wc) };
        (intr.0 + f * pc[0] / pc[2] - obs.0, intr.1 + f * pc[1] / pc[2] - obs.1)
    };

    // Check camera 6-DOF left-SE(3) Jacobian
    for k in 0..6 {
        let mut step_pos = [0.0f64; 6];
        let mut step_neg = [0.0f64; 6];
        step_pos[k] = eps;
        step_neg[k] = -eps;

        let apply_se3 = |s: [f64; 6]| -> CameraPose {
            let dr = rodrigues_exp([s[0], s[1], s[2]]);
            CameraPose { r_wc: project_to_so3(mat3_mul(dr, cam.r_wc)), t_wc: add3(mat3_mul_vec(dr, cam.t_wc), [s[3], s[4], s[5]]), frozen: false, solved: true }
        };
        let (up, vp) = eval_res(&apply_se3(step_pos), xw, intr.2, false);
        let (un, vn) = eval_res(&apply_se3(step_neg), xw, intr.2, false);
        let du = (up - un) / (2.0 * eps);
        let dv = (vp - vn) / (2.0 * eps);
        assert!((jac.j_cam_u[k] - du).abs() < 1e-5, "cam u[{k}] analytic={} fd={du}", jac.j_cam_u[k]);
        assert!((jac.j_cam_v[k] - dv).abs() < 1e-5, "cam v[{k}] analytic={} fd={dv}", jac.j_cam_v[k]);
    }

    // Check 3D point Jacobian
    for k in 0..3 {
        let mut xp = xw;
        let mut xn = xw;
        xp[k] += eps;
        xn[k] -= eps;
        let (up, vp) = eval_res(&cam, xp, intr.2, false);
        let (un, vn) = eval_res(&cam, xn, intr.2, false);
        let du = (up - un) / (2.0 * eps);
        let dv = (vp - vn) / (2.0 * eps);
        assert!((jac.j_pt_u[k] - du).abs() < 1e-5, "pt u[{k}] analytic={} fd={du}", jac.j_pt_u[k]);
        assert!((jac.j_pt_v[k] - dv).abs() < 1e-5, "pt v[{k}] analytic={} fd={dv}", jac.j_pt_v[k]);
    }

    // Check focal Jacobian
    let (up, vp) = eval_res(&cam, xw, intr.2 + eps, false);
    let (un, vn) = eval_res(&cam, xw, intr.2 - eps, false);
    assert!((jac.j_f_u - (up - un) / (2.0 * eps)).abs() < 1e-6);
    assert!((jac.j_f_v - (vp - vn) / (2.0 * eps)).abs() < 1e-6);
}

#[test]
fn ac1a_and_ac2_synthetic_tripod_yaw_0_to_40_deg() {
    let (w, h) = (480usize, 270usize);
    // Wide azimuth field of landmarks so features remain abundant throughout a 40-degree pan.
    let landmarks = make_3d_constellation(320, 500.0, 1300.0, 1100.0, 0x1111_2222_3333_4444);
    let num_frames = 25usize;
    let mut frames = Vec::with_capacity(num_frames);
    let mut gt_cams = Vec::with_capacity(num_frames);

    for i in 0..num_frames {
        let yaw = 40.0 * (i as f32 / (num_frames - 1) as f32);
        let cam = SceneCamera3D { position: [0.0, 0.0, 0.0], pitch: 0.0, yaw, roll: 0.0, fov: CLASSIC_FOV };
        let rgba = render_synthetic_eye(w, h, &cam, &landmarks, [18, 20, 26]);
        frames.push(InputFrame { time: tick_for_frame(i, 24), width: w, height: h, rgba });
        gt_cams.push(cam);
    }

    let opts = SolveOptions::default();
    let report = solve_frames(&opts, &frames, &|| false).expect("tripod yaw solve must succeed");
    assert!(report.solve.rotation_only, "AC2: tripod pan must select rotation-only fallback");
    assert_eq!(report.solve.frames.len(), num_frames);

    let mm = SequenceMatchMove::new(report.solve.id);
    let mut sq_deg = 0.0f64;
    for (i, sf) in report.solve.frames.iter().enumerate() {
        let pos_norm = (sf.position[0].powi(2) + sf.position[1].powi(2) + sf.position[2].powi(2)).sqrt();
        assert!(pos_norm < 1e-3, "AC2: rotation-only position must be < 1e-3, got {pos_norm}");

        let solved_cam = mm.apply_to_sample(sf);
        let ra = solved_cam.view_rotation().map(|r| r.map(f64::from));
        let rb = gt_cams[i].view_rotation().map(|r| r.map(f64::from));
        let deg = rotation_angle_rad(ra, rb).to_degrees();
        sq_deg += deg * deg;
    }
    let rms_deg = (sq_deg / num_frames as f64).sqrt();
    assert!(rms_deg < 1.0, "AC1a/AC2: yaw 0->40 deg rotation RMS must be < 1.0 deg, got {rms_deg:.4} deg");
}

#[test]
fn ac1b_synthetic_dolly_in_direction_cosine_over_0_95() {
    let (w, h) = (480usize, 270usize);
    let landmarks = make_3d_constellation(260, 420.0, 1450.0, 620.0, 0x5555_6666_7777_8888);
    let num_frames = 24usize;
    let mut frames = Vec::with_capacity(num_frames);

    for i in 0..num_frames {
        let z = 220.0 * (i as f32 / (num_frames - 1) as f32);
        let cam = SceneCamera3D { position: [0.0, 0.0, z], pitch: 0.0, yaw: 0.0, roll: 0.0, fov: CLASSIC_FOV };
        let rgba = render_synthetic_eye(w, h, &cam, &landmarks, [16, 22, 24]);
        frames.push(InputFrame { time: tick_for_frame(i, 24), width: w, height: h, rgba });
    }

    let report = solve_frames(&SolveOptions::default(), &frames, &|| false).expect("dolly-in solve must succeed");
    assert!(!report.solve.rotation_only, "dolly-in with depth parallax must solve 6-DOF translation");
    let first = report.solve.frames[0].position.map(f64::from);
    let last = report.solve.frames[num_frames - 1].position.map(f64::from);
    let dir = normalize3(sub3(last, first)).expect("non-zero translation");
    let cosine = dot3(dir, [0.0, 0.0, 1.0]);
    assert!(cosine > 0.95, "AC1b: dolly-in direction cosine must be > 0.95, got {cosine:.4} (dir={dir:?})");
}

#[test]
fn ac1c_ac4_ac8_synthetic_dolly_pan_roll_and_determinism() {
    let (w, h) = (480usize, 270usize);
    // Use 420 landmarks so >256 tracks survive and exercise the MAX_POINTS = 256 cap.
    let landmarks = make_3d_constellation(420, 420.0, 1500.0, 740.0, 0x9999_aaaa_bbbb_cccc);
    let num_frames = 28usize;
    let mut frames = Vec::with_capacity(num_frames);

    for i in 0..num_frames {
        let u = i as f32 / (num_frames - 1) as f32;
        let cam = SceneCamera3D { position: [95.0 * u, -24.0 * u, 190.0 * u], pitch: -3.5 * u, yaw: 13.0 * u, roll: 5.5 * u, fov: CLASSIC_FOV };
        let rgba = render_synthetic_eye(w, h, &cam, &landmarks, [20, 18, 25]);
        frames.push(InputFrame { time: tick_for_frame(i, 24), width: w, height: h, rgba });
    }

    let opts = SolveOptions { id: 42, clip: ClipId(7), ..SolveOptions::default() };
    let r1 = solve_frames(&opts, &frames, &|| false).expect("6-DOF dolly+pan+roll solve must succeed");
    let r2 = solve_frames(&opts, &frames, &|| false).expect("second solve must succeed");

    assert!(r1.median_rms_px <= 1.5, "AC1c: median reprojection RMS must be <= 1.5 px, got {:.3} px", r1.median_rms_px);
    assert!(r1.median_confidence >= 0.3);

    // AC8: bounds & finiteness
    assert!(r1.solve.frames.len() <= CameraSolve::MAX_FRAMES);
    assert!(r1.solve.points.len() <= CameraSolve::MAX_POINTS);
    assert!(!r1.solve.points.is_empty());
    assert!(r1.solve.is_finite());
    for sf in &r1.solve.frames {
        assert!(sf.is_finite());
    }
    for sp in &r1.solve.points {
        assert!(sp.is_finite());
    }

    // AC4: byte-identical serialized CameraSolve across repeated solves
    let bytes1 = serde_json::to_vec(&r1.solve).expect("serialize solve 1");
    let bytes2 = serde_json::to_vec(&r2.solve).expect("serialize solve 2");
    assert_eq!(bytes1, bytes2, "AC4: repeated solves must produce byte-identical CameraSolve");
}

#[test]
fn ac7_cancel_and_short_clip_handling() {
    let empty: Vec<InputFrame> = Vec::new();
    assert_eq!(solve_frames(&SolveOptions::default(), &empty, &|| false).unwrap_err(), SolveError::TooShort);

    let landmarks = make_3d_constellation(120, 500.0, 1200.0, 600.0, 0x1234_5678);
    let short: Vec<InputFrame> = (0..7)
        .map(|i| {
            let cam = SceneCamera3D { yaw: i as f32 * 2.0, ..SceneCamera3D::default() };
            InputFrame { time: tick_for_frame(i, 24), width: 320, height: 180, rgba: render_synthetic_eye(320, 180, &cam, &landmarks, [20, 20, 20]) }
        })
        .collect();
    assert_eq!(solve_frames(&SolveOptions::default(), &short, &|| false).unwrap_err(), SolveError::TooShort);

    let full: Vec<InputFrame> = (0..16)
        .map(|i| {
            let cam = SceneCamera3D { yaw: i as f32 * 2.0, ..SceneCamera3D::default() };
            InputFrame { time: tick_for_frame(i, 24), width: 320, height: 180, rgba: render_synthetic_eye(320, 180, &cam, &landmarks, [20, 20, 20]) }
        })
        .collect();

    // Immediate cancel
    assert_eq!(solve_frames(&SolveOptions::default(), &full, &|| true).unwrap_err(), SolveError::Cancelled);

    // Mid-stream cancel
    let polls = AtomicUsize::new(0);
    let mut iter = full.iter();
    let err = solve_camera(&SolveOptions::default(), &|| polls.fetch_add(1, Ordering::SeqCst) >= 6, || Ok(iter.next().cloned())).unwrap_err();
    assert_eq!(err, SolveError::Cancelled);
}

#[test]
fn ac12_flat_footage_fails_with_clean_motion_message_and_shot_cut_isolates_playhead() {
    // Part 1: flat uniform-colour footage fails with the exact expected message.
    let flat_rgba = vec![128u8; 320 * 180 * 4];
    let flat_frames: Vec<InputFrame> = (0..16).map(|i| InputFrame { time: tick_for_frame(i, 24), width: 320, height: 180, rgba: flat_rgba.clone() }).collect();
    let err = solve_frames(&SolveOptions::default(), &flat_frames, &|| false).unwrap_err();
    assert!(err.to_string().contains("not enough clean motion to solve the camera"), "AC12: unexpected flat-footage error message: {err}");

    // Part 2: hard cut inside clip solves only the shot containing `playhead` and reports `shots_skipped > 0`.
    let (w, h) = (480usize, 270usize);
    let mut shot_a_lms = make_3d_constellation(260, 500.0, 1300.0, 900.0, 0xaaaa_1111);
    for (idx, lm) in shot_a_lms.iter_mut().enumerate() {
        lm.luma = 22.0 + (idx % 4) as f64 * 14.0;
    }
    let shot_b_lms = make_3d_constellation(260, 420.0, 1400.0, 620.0, 0xbbbb_2222);

    let mut two_shot = Vec::with_capacity(32);
    for i in 0..16 {
        let cam = SceneCamera3D { position: [0.0, 0.0, 0.0], yaw: i as f32 * 1.8, ..SceneCamera3D::default() };
        // Distinct bright warm palette in Shot A so SceneDetector fires a clean hard cut at frame 16.
        let rgba = render_synthetic_eye(w, h, &cam, &shot_a_lms, [215, 195, 175]);
        two_shot.push(InputFrame { time: tick_for_frame(i, 24), width: w, height: h, rgba });
    }
    for i in 0..16 {
        let u = i as f32 / 15.0;
        let cam = SceneCamera3D { position: [0.0, 0.0, 180.0 * u], ..SceneCamera3D::default() };
        let rgba = render_synthetic_eye(w, h, &cam, &shot_b_lms, [12, 16, 28]);
        two_shot.push(InputFrame { time: tick_for_frame(16 + i, 24), width: w, height: h, rgba });
    }

    let opts_a = SolveOptions { playhead: tick_for_frame(4, 24), ..SolveOptions::default() };
    let rep_a = solve_frames(&opts_a, &two_shot, &|| false).expect("playhead in shot A must solve");
    assert!(rep_a.shots_skipped > 0, "AC12: cut inside range must report shots_skipped > 0");
    assert_eq!(rep_a.solve.frames.len(), 16, "Shot A has 16 frames");
    assert!(rep_a.solve.rotation_only, "Shot A is a pure tripod pan");

    let opts_b = SolveOptions { playhead: tick_for_frame(24, 24), ..SolveOptions::default() };
    let rep_b = solve_frames(&opts_b, &two_shot, &|| false).expect("playhead in shot B must solve");
    assert!(rep_b.shots_skipped > 0, "AC12: cut inside range must report shots_skipped > 0");
    assert_eq!(rep_b.solve.frames.len(), 16, "Shot B has 16 frames");
    assert!(!rep_b.solve.rotation_only, "Shot B is a 6-DOF dolly-in");
}

#[test]
fn stereo_sbs_and_focal_refinement_smoke() {
    let (eye_w, eye_h) = (240usize, 180usize);
    let landmarks = make_3d_constellation(220, 380.0, 1300.0, 520.0, 0x7777_8888);
    let num_frames = 18usize;
    let mut sbs_frames = Vec::with_capacity(num_frames);

    for i in 0..num_frames {
        let u = i as f32 / (num_frames - 1) as f32;
        let left_cam = SceneCamera3D { position: [50.0 * u, 0.0, 140.0 * u], yaw: 6.0 * u, fov: 46.0, ..SceneCamera3D::default() };
        let right_cam = SceneCamera3D { position: [50.0 * u + 24.0, 0.0, 140.0 * u], yaw: 6.0 * u, fov: 46.0, ..SceneCamera3D::default() };
        let left_rgba = render_synthetic_eye(eye_w, eye_h, &left_cam, &landmarks, [18, 20, 24]);
        let right_rgba = render_synthetic_eye(eye_w, eye_h, &right_cam, &landmarks, [18, 20, 24]);

        let mut full = vec![0u8; eye_w * 2 * eye_h * 4];
        for y in 0..eye_h {
            let dst_l = y * (eye_w * 2) * 4;
            let dst_r = dst_l + eye_w * 4;
            let src = y * eye_w * 4;
            full[dst_l..(dst_l + eye_w * 4)].copy_from_slice(&left_rgba[src..(src + eye_w * 4)]);
            full[dst_r..(dst_r + eye_w * 4)].copy_from_slice(&right_rgba[src..(src + eye_w * 4)]);
        }
        sbs_frames.push(InputFrame { time: tick_for_frame(i, 24), width: eye_w * 2, height: eye_h, rgba: full });
    }

    let opts = SolveOptions { layout: StereoLayout::SideBySide, stereo_baseline: 36.0, refine_focal: true, ..SolveOptions::default() };
    let rep = solve_frames(&opts, &sbs_frames, &|| false).expect("SBS + refine_focal solve must succeed");
    assert!(rep.median_rms_px <= 1.8);
    let solved_fov = rep.solve.frames[0].fov;
    assert!(solved_fov.is_finite() && (35.0..=68.0).contains(&solved_fov));
}

fn alpha_centroid(img: &crate::Image) -> Option<(f32, f32)> {
    let (mut sx, mut sy, mut sw) = (0.0f64, 0.0f64, 0.0f64);
    for y in 0..img.h {
        for x in 0..img.w {
            let a = img.px[(y * img.w + x) * 4 + 3] as f64;
            if a > 1e-3 {
                sx += (x as f64 + 0.5) * a;
                sy += (y as f64 + 0.5) * a;
                sw += a;
            }
        }
    }
    (sw > 1.0).then_some(((sx / sw) as f32, (sy / sw) as f32))
}

fn project_world(w: usize, h: usize, cam: &SceneCamera3D, world: [f32; 3]) -> Option<(f32, f32)> {
    let vr = cam.view_rotation();
    let dx = world[0] - cam.position[0];
    let dy = world[1] - cam.position[1];
    let dz = world[2] - cam.position[2];
    let px = vr[0][0] * dx + vr[0][1] * dy + vr[0][2] * dz;
    let py = vr[1][0] * dx + vr[1][1] * dy + vr[1][2] * dz;
    let pz = vr[2][0] * dx + vr[2][1] * dy + vr[2][2] * dz;
    (pz > 1.0).then(|| {
        let f = cam.focal(h as f32);
        (w as f32 * 0.5 + f * (px / pz), h as f32 * 0.5 + f * (py / pz))
    })
}

fn make_object3d_sphere(pos_x: f64, pos_y: f64, z: f64, size: f64) -> filmcraft_project::EffectInstance {
    use filmcraft_project::ParamValue;
    let mut e = filmcraft_project::find_effect("object_3d").expect("object_3d effect exists").instance();
    e.params.get_mut("mesh").expect("mesh").value = ParamValue::Choice(1);
    e.params.get_mut("size").expect("size").value = ParamValue::Float(size);
    e.params.get_mut("position").expect("position").value = ParamValue::Vec2(filmcraft_geom::Vec2::new(pos_x, pos_y));
    e.params.get_mut("z").expect("z").value = ParamValue::Float(z);
    e
}

fn setup_matte_sequence(
    w: u32,
    h: u32,
    rate: filmcraft_time::FrameRate,
    clip_start: Tick,
    clip_frames: i64,
    fx: Vec<filmcraft_project::EffectInstance>,
) -> (filmcraft_project::Project, crate::SourceMap, filmcraft_project::ItemId, ClipId) {
    use filmcraft_media::generators::GeneratorSource;
    use filmcraft_media::{Generator, MediaSource, SharedSource};
    use filmcraft_project::{ItemKind, Label, MediaClip, MediaRef, Project, SequenceSettings, TrackKind};
    use filmcraft_time::{TICKS_PER_SECOND, TimeRange};
    use std::sync::Arc;

    let mut p = Project::new("mm_render");
    let mut map = crate::SourceMap::default();
    let g = GeneratorSource::new(Generator::ColorMatte { color: [0.0, 0.0, 0.0, 0.0] }, w, h, rate, Tick(10 * TICKS_PER_SECOND));
    let info = g.info().clone();
    let generator = g.generator.clone();
    let media_id = p.add_item(
        "matte",
        Label::Iris,
        ItemKind::Media(Box::new(MediaClip {
            media: MediaRef::Generator(generator),
            info,
            interpret: Default::default(),
            mark_in: None,
            mark_out: None,
            markers: vec![],
            offline: false,
            proxy: None,
            identity: None,
        })),
        None,
    );
    map.0.insert(media_id, Arc::new(g) as SharedSource);

    let seq_id = p.new_sequence("s", SequenceSettings { width: w, height: h, frame_rate: rate, ..Default::default() }, 1, 0, None);
    let mut ti =
        p.make_track_item(media_id, TrackKind::Video, clip_start, TimeRange::new(Tick::ZERO, rate.tick_of(clip_frames)), rate).expect("make_track_item");
    ti.scale_to_frame = true;
    let clip_id = ti.id;
    for (idx, e) in fx.into_iter().enumerate() {
        ti.effects.insert(idx, e);
    }
    p.sequence_mut(seq_id).expect("seq").video_tracks[0].items.push(ti);
    (p, map, seq_id, clip_id)
}

#[test]
fn ac3_glued_3d_object_centroid_stays_within_two_pixels_over_64_frames() {
    use crate::{RenderOptions, render_sequence};
    use filmcraft_project::SequenceMatchMove;
    use filmcraft_time::FrameRate;

    let (w, h) = (256usize, 256usize);
    let rate = FrameRate::FPS_24;
    let num_frames = 64usize;
    let landmarks = make_3d_constellation(260, 420.0, 1300.0, 520.0, 0xAC30_2026);
    let mut frames = Vec::with_capacity(num_frames);
    for i in 0..num_frames {
        let u = i as f32 / (num_frames - 1) as f32;
        let cam = SceneCamera3D {
            position: [95.0 * u, -35.0 * (u * std::f32::consts::PI).sin(), 140.0 * u],
            yaw: 9.0 * u,
            pitch: -3.5 * u,
            roll: 2.5 * (u * std::f32::consts::TAU).sin(),
            ..SceneCamera3D::default()
        };
        let rgba = render_synthetic_eye(w, h, &cam, &landmarks, [18, 20, 25]);
        frames.push(InputFrame { time: rate.tick_of(i as i64), width: w, height: h, rgba });
    }

    let (_, _, _, placeholder_clip) = setup_matte_sequence(w as u32, h as u32, rate, Tick::ZERO, num_frames as i64, vec![]);
    let opts = SolveOptions { id: 7, clip: placeholder_clip, ..SolveOptions::default() };
    let rep = solve_frames(&opts, &frames, &|| false).expect("64-frame 6-DOF sequence must solve");
    assert_eq!(rep.solve.frames.len(), num_frames);

    // Pick a central solved 3D landmark so it stays well inside the 256x256 frame over all 64 frames.
    let scale_px = SequenceMatchMove::DEFAULT_SCALE_PX;
    let anchor = rep
        .solve
        .points
        .iter()
        .find(|pt| pt.position[2] > 0.8 && pt.position[2] < 2.2 && pt.position[0].abs() < 0.25 && pt.position[1].abs() < 0.25)
        .unwrap_or(&rep.solve.points[0]);
    let world = [anchor.position[0] * scale_px, anchor.position[1] * scale_px, anchor.position[2] * scale_px];
    let pos_x = w as f64 * 0.5 + h as f64 * (world[0] as f64 / world[2] as f64);
    let pos_y = h as f64 * 0.5 + h as f64 * (world[1] as f64 / world[2] as f64);

    let sphere = make_object3d_sphere(pos_x, pos_y, world[2] as f64, 18.0);
    let (mut p, map, seq_id, clip_id) = setup_matte_sequence(w as u32, h as u32, rate, Tick::ZERO, num_frames as i64, vec![sphere]);
    let mut solve = rep.solve;
    solve.clip = clip_id;
    {
        let seq = p.sequence_mut(seq_id).expect("seq");
        seq.camera_solves.push(solve);
        seq.matchmove = Some(SequenceMatchMove::new(7));
    }

    let mut max_drift_px = 0.0f32;
    let mut first_centroid = (0.0f32, 0.0f32);
    let mut max_travel_px = 0.0f32;
    for f in 0..num_frames {
        let t = rate.tick_of(f as i64);
        let img = render_sequence(&p, seq_id, t, RenderOptions::default(), &map);
        let (cx, cy) = alpha_centroid(&img).expect("glued 3D sphere must remain visible across all 64 frames");
        if f == 0 {
            first_centroid = (cx, cy);
        } else {
            max_travel_px = max_travel_px.max((cx - first_centroid.0).hypot(cy - first_centroid.1));
        }
        let cam = p.sequence(seq_id).expect("seq").matchmove_camera_at(t).expect("active match-move camera");
        let (ax, ay) = project_world(w, h, &cam, world).expect("analytic projection in front of camera");
        let drift = (cx - ax).hypot(cy - ay);
        max_drift_px = max_drift_px.max(drift);
    }
    assert!(max_travel_px > 20.0, "sphere must actually move with the solved camera (moved {max_travel_px:.2} px)");
    assert!(max_drift_px < 2.0, "AC3: rendered 3D centroid drifted {max_drift_px:.3} px (>= 2.0 px) over 64 frames");
}

#[test]
fn ac5_ac7_ac9_precedence_parity_and_orphan_safety() {
    use crate::{RenderOptions, plan, render_sequence, with_preview_camera};
    use filmcraft_project::{CameraSolve, SceneCamera3D, SequenceMatchMove, SolvedFrame, StereoLayout};
    use filmcraft_time::FrameRate;

    let (w, h) = (128u32, 128u32);
    let rate = FrameRate::FPS_24;
    let clip_start = rate.tick_of(10);
    let sphere = make_object3d_sphere(64.0, 64.0, 256.0, 24.0);
    let (mut p, map, seq_id, clip_id) = setup_matte_sequence(w, h, rate, clip_start, 30, vec![sphere]);

    let static_cam = SceneCamera3D { position: [-40.0, 0.0, 0.0], yaw: -6.0, ..SceneCamera3D::default() };
    p.sequence_mut(seq_id).expect("seq").scene_camera = Some(static_cam);
    let t_mid = rate.tick_of(20); // clip-local frame 10

    // Baseline render before any solve exists (AC5).
    let baseline_static = render_sequence(&p, seq_id, t_mid, RenderOptions::default(), &map);

    let solve = CameraSolve {
        id: 11,
        clip: clip_id,
        name: "Solve 1".into(),
        layout: StereoLayout::Mono,
        rotation_only: false,
        shots_skipped: 0,
        frames: vec![
            SolvedFrame { time: Tick::ZERO, position: [0.0, 0.0, 0.0], yaw: 0.0, pitch: 0.0, roll: 0.0, fov: CLASSIC_FOV, rms_px: 0.3, confidence: 0.95 },
            SolvedFrame {
                time: rate.tick_of(20),
                position: [0.35, 0.0, 0.0],
                yaw: 10.0,
                pitch: 0.0,
                roll: 0.0,
                fov: CLASSIC_FOV,
                rms_px: 0.3,
                confidence: 0.95,
            },
        ],
        points: vec![],
    };
    {
        let seq = p.sequence_mut(seq_id).expect("seq");
        seq.camera_solves.push(solve);
        seq.matchmove = Some(SequenceMatchMove { enabled: false, ..SequenceMatchMove::new(11) });
    }

    // AC5: stored solve with matchmove disabled produces byte-identical output to pre-solve sequence.
    let disabled_render = render_sequence(&p, seq_id, t_mid, RenderOptions::default(), &map);
    assert_eq!(disabled_render.px, baseline_static.px, "AC5: disabled match-move must be a byte-identical no-op");

    // Enable match-move -> wins over static seq.scene_camera (AC9).
    p.sequence_mut(seq_id).expect("seq").matchmove.as_mut().expect("mm").enabled = true;
    let mm_render = render_sequence(&p, seq_id, t_mid, RenderOptions::default(), &map);
    assert_ne!(mm_render.px, baseline_static.px, "enabled match-move must override static scene_camera");

    // AC9: preview planner == export/CPU render_sequence parity.
    let planned = plan::plan_frame(&p, seq_id, t_mid, RenderOptions::default(), plan::DeviceCaps::ALL, &map);
    let preview_px = plan::execute_cpu(&planned).px;
    assert_eq!(preview_px, mm_render.px, "AC9: preview FramePlan and export render_sequence must match bit-for-bit");

    // AC9: editor orbit camera (`with_preview_camera`) wins over enabled match-move.
    let orbit_cam = SceneCamera3D { position: [55.0, 20.0, -30.0], yaw: 14.0, pitch: -5.0, ..SceneCamera3D::default() };
    let orbit_over_mm = with_preview_camera(Some(orbit_cam), || render_sequence(&p, seq_id, t_mid, RenderOptions::default(), &map));
    assert_ne!(orbit_over_mm.px, mm_render.px, "AC9: with_preview_camera(Some) must override match-move");
    let after_orbit = render_sequence(&p, seq_id, t_mid, RenderOptions::default(), &map);
    assert_eq!(after_orbit.px, mm_render.px, "restoring preview_camera must resume match-move");

    // AC7: removing the solved clip leaves a harmless orphan solve and never panics.
    p.sequence_mut(seq_id).expect("seq").video_tracks[0].items.clear();
    let orphan_img = render_sequence(&p, seq_id, t_mid, RenderOptions::default(), &map);
    assert!(orphan_img.px.iter().all(|v| v.is_finite()));
}

#[test]
fn phase6_hostile_bounds_and_clip_offset_sync() {
    use filmcraft_media::MediaSource;
    use filmcraft_project::{
        CameraSolve, ItemKind, Label, MediaClip, MediaRef, Project, SequenceMatchMove, SequenceSettings, SolvedFrame, SolvedPoint, StereoLayout, TrackKind,
    };
    use filmcraft_time::{FrameRate, TimeRange};

    let rate = FrameRate::FPS_24;
    let dur = rate.tick_of(12);
    let mut p = Project::new("phase6_bounds");
    let src = filmcraft_media::generators::GeneratorSource::new(filmcraft_media::Generator::ColorMatte { color: [0.2, 0.2, 0.2, 1.0] }, 160, 120, rate, dur);
    let clip = MediaClip {
        media: MediaRef::Generator(filmcraft_media::Generator::ColorMatte { color: [0.2, 0.2, 0.2, 1.0] }),
        info: src.info().clone(),
        interpret: Default::default(),
        mark_in: None,
        mark_out: None,
        markers: vec![],
        offline: false,
        proxy: None,
        identity: None,
    };
    let item = p.add_item("matte", Label::Iris, ItemKind::Media(Box::new(clip)), None);
    let seq_id = p.new_sequence("seq", SequenceSettings { width: 160, height: 120, frame_rate: rate, ..Default::default() }, 1, 0, None);
    let track_item = p.make_track_item(item, TrackKind::Video, rate.tick_of(10), TimeRange::new(Tick::ZERO, dur), rate).expect("track item");
    let clip_id = track_item.id;
    let seq = p.sequence_mut(seq_id).expect("seq");
    seq.video_tracks[0].items.push(track_item);
    seq.camera_solves.push(CameraSolve {
        id: 42,
        clip: clip_id,
        name: "Solve 1".into(),
        layout: StereoLayout::Mono,
        rotation_only: false,
        shots_skipped: 0,
        frames: vec![
            SolvedFrame { time: Tick::ZERO, position: [0.0, 0.0, 0.0], yaw: 0.0, pitch: 0.0, roll: 0.0, fov: CLASSIC_FOV, rms_px: 0.2, confidence: 1.0 },
            SolvedFrame { time: rate.tick_of(6), position: [0.5, 0.0, 0.0], yaw: 12.0, pitch: 0.0, roll: 0.0, fov: CLASSIC_FOV, rms_px: 0.2, confidence: 1.0 },
        ],
        points: vec![SolvedPoint { position: [0.1, -0.2, 2.0], observations: 6, rms_px: 0.3 }],
    });
    seq.matchmove = Some(SequenceMatchMove::new(42));

    // Clip sits at timeline frame 10 -> timeline frame 10 reads local frame 0, timeline frame 16 reads local frame 6.
    assert!(seq.check_bounds().is_ok());
    let cam_at_clip_start = seq.matchmove_camera_at(rate.tick_of(10)).expect("cam at frame 10");
    let cam_at_clip_mid = seq.matchmove_camera_at(rate.tick_of(16)).expect("cam at frame 16");
    assert!(cam_at_clip_start.position[0].abs() < 1e-4);
    assert!((cam_at_clip_mid.position[0] - 350.0).abs() < 1e-2);
    assert!((cam_at_clip_mid.yaw - 12.0).abs() < 1e-3);

    // Hostile NaN in SequenceMatchMove or CameraSolve is rejected by check_bounds.
    seq.matchmove.as_mut().expect("mm").scale_px = f32::NAN;
    assert!(seq.check_bounds().is_err());
    seq.matchmove.as_mut().expect("mm").scale_px = -5.0;
    assert!(seq.check_bounds().is_err());
    seq.matchmove = Some(SequenceMatchMove::new(42));
    seq.camera_solves[0].points[0].position[2] = f32::INFINITY;
    assert!(seq.check_bounds().is_err());
}

#[test]
fn ac_portrait_smartphone_low_contrast_barrel_distortion_solves_3d() {
    let (w, h) = (270usize, 480usize);
    let mut landmarks = make_3d_constellation(170, 320.0, 1150.0, 520.0, 0x9160_2400);
    // Compress landmark contrast to realistic indoor room lighting (~0.11..0.24 normalized delta).
    for (i, lm) in landmarks.iter_mut().enumerate() {
        lm.luma = 64.0 + (i % 5) as f64 * 8.0;
        lm.radius = 2.4 + (i % 3) as f64 * 0.6;
    }

    let fov_deg = 74.0f64; // 24mm smartphone lens in 9:16 portrait orientation!
    let focal = (h as f64 * 0.5) / (fov_deg.to_radians() * 0.5).tan();
    let cx = w as f64 * 0.5;
    let cy = h as f64 * 0.5;
    let k1 = -0.09f64; // Typical uncalibrated smartphone wide-angle barrel distortion.

    let mut frames = Vec::new();
    for i in 0..24 {
        let t = i as f32 / 23.0;
        let cam = SceneCamera3D {
            position: [-85.0 + 170.0 * t, 12.0 * (i as f32 * 0.35).sin(), -25.0 * t],
            yaw: -6.0 + 12.0 * t,
            pitch: 1.5 * (i as f32 * 0.25).cos(),
            roll: 0.8 * (i as f32 * 0.3).sin(),
            fov: fov_deg as f32,
        };
        let mut img = vec![0u8; w * h * 4];
        for px in img.as_chunks_mut::<4>().0 {
            px[0] = 34;
            px[1] = 34;
            px[2] = 34;
            px[3] = 255;
        }
        let vr = cam.view_rotation().map(|r| r.map(f64::from));
        let eye = cam.position.map(f64::from);
        for lm in &landmarks {
            let pc = mat3_mul_vec(vr, sub3(lm.pos, eye));
            if pc[2] <= 10.0 {
                continue;
            }
            let xn = pc[0] / pc[2];
            let yn = pc[1] / pc[2];
            let r2 = xn * xn + yn * yn;
            let rad = 1.0 + k1 * r2;
            // Include slight rolling-shutter horizontal shear proportional to scanline `yn`.
            let u = cx + focal * (xn * rad + 0.008 * yn);
            let v = cy + focal * (yn * rad);
            if u >= -6.0 && u <= w as f64 + 6.0 && v >= -6.0 && v <= h as f64 + 6.0 {
                splat_landmark(&mut img, w, h, u, v, lm);
            }
        }
        frames.push(InputFrame { time: tick_for_frame(i, 30), width: w, height: h, rgba: img });
    }

    let opts = SolveOptions { refine_focal: true, ..SolveOptions::default() };
    let rep = solve_frames(&opts, &frames, &|| false)
        .expect("9:16 portrait low-contrast smartphone video with barrel distortion must solve cleanly in 3D");
    assert_eq!(rep.solve.frames.len(), 24);
    assert!(!rep.solve.rotation_only, "parallax portrait walk must solve 6-DOF translation");
    assert!(rep.solve.points.len() >= 12, "expected solved 3D points, got {}", rep.solve.points.len());
}

#[test]
fn ac_fast_handheld_pan_and_nonrigid_textured_fallback_never_fail_clean_motion() {
    // Part 1: Fast 40-frame handheld pan (~11 px/frame) where Frame-0 features completely exit
    // midway through the clip, testing `dominant_velocity_guess` + `choose_initial_pair`.
    let (w, h) = (320usize, 180usize);
    let landmarks = make_3d_constellation(220, 450.0, 1100.0, 950.0, 0xFA57_9A40);
    let pan_frames: Vec<InputFrame> = (0..40)
        .map(|i| {
            let cam = SceneCamera3D {
                yaw: -38.0 + i as f32 * 1.95,
                pitch: 2.0 * (i as f32 * 0.22).sin(),
                ..SceneCamera3D::default()
            };
            InputFrame {
                time: tick_for_frame(i, 30),
                width: w,
                height: h,
                rgba: render_synthetic_eye(w, h, &cam, &landmarks, [28, 28, 28]),
            }
        })
        .collect();
    let opts = SolveOptions { refine_focal: true, ..SolveOptions::default() };
    let pan_res = solve_frames(&opts, &pan_frames, &|| false);
    assert!(
        pan_res.is_ok(),
        "fast multi-reseed handheld pan must solve cleanly without InsufficientMotion, got: {:?}",
        pan_res.err()
    );
}



