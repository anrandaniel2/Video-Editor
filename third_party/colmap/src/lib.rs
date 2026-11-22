//! Safe Rust bindings to the stripped-down COLMAP + GLOMAP (OpenGL + sparse SfM, no UI) engine.

use std::ffi::{CStr, c_char, c_int};

/// Single grayscale video frame sampled for COLMAP sparse reconstruction.
#[derive(Debug, Clone)]
pub struct GrayFrame<'a> {
    pub time_s: f64,
    pub width: u32,
    pub height: u32,
    pub gray: &'a [u8],
}

/// Solver configuration passed to COLMAP / GLOMAP.
#[derive(Debug, Clone, Copy)]
pub struct SolveConfig {
    pub initial_focal_px: f64,
    pub lock_fov: bool,
    pub max_features: usize,
    pub use_opengl: bool,
    pub prefer_glomap: bool,
}

impl Default for SolveConfig {
    fn default() -> Self {
        Self {
            initial_focal_px: 0.0,
            lock_fov: false,
            max_features: 2048,
            use_opengl: true,
            prefer_glomap: true,
        }
    }
}

/// Reconstructed camera pose (`x_cam = R_cw * x_world + t_cw`) for one sampled frame.
#[derive(Debug, Clone, Copy)]
pub struct ReconstructedPose {
    pub time_s: f64,
    pub registered: bool,
    pub r_cw: [[f64; 3]; 3],
    pub t_cw: [f64; 3],
    pub focal_px: f64,
}

/// Triangulated sparse 3D point in COLMAP's world frame.
#[derive(Debug, Clone, Copy)]
pub struct ReconstructedPoint3D {
    pub xyz: [f64; 3],
    pub observations: u32,
    pub reproj_error: f64,
}

/// Output of the stripped-down COLMAP / GLOMAP sparse solver.
#[derive(Debug, Clone)]
pub struct SequenceReconstruction {
    pub used_opengl: bool,
    pub used_glomap: bool,
    pub focal_px: f64,
    pub mean_reproj_px: f64,
    pub poses: Vec<ReconstructedPose>,
    pub points: Vec<ReconstructedPoint3D>,
}

/// Structured failure modes returned by [`solve_sequence`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColmapError {
    InvalidInput(String),
    InsufficientMotion(String),
    Planar2D(String),
    Internal(String),
}

impl std::fmt::Display for ColmapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(m)
            | Self::InsufficientMotion(m)
            | Self::Planar2D(m)
            | Self::Internal(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for ColmapError {}

mod ffi {
    use super::{c_char, c_int};

    #[repr(C)]
    pub struct ColmapInputFrame {
        pub time_s: f64,
        pub width: u32,
        pub height: u32,
        pub gray: *const u8,
    }

    #[repr(C)]
    pub struct ColmapSolveConfig {
        pub initial_focal_px: f64,
        pub lock_fov: c_int,
        pub max_features: c_int,
        pub use_opengl: c_int,
        pub prefer_glomap: c_int,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ColmapSolvedPose {
        pub time_s: f64,
        pub registered: c_int,
        pub r_cw: [f64; 9],
        pub t_cw: [f64; 3],
        pub focal_px: f64,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ColmapSolvedPoint3D {
        pub xyz: [f64; 3],
        pub observations: u32,
        pub reproj_error: f64,
    }

    #[repr(C)]
    pub struct ColmapSolveResult {
        pub status: c_int,
        pub used_opengl: c_int,
        pub used_glomap: c_int,
        pub focal_px: f64,
        pub mean_reproj_px: f64,
        pub num_poses: usize,
        pub poses: *mut ColmapSolvedPose,
        pub num_points: usize,
        pub points: *mut ColmapSolvedPoint3D,
        pub error_msg: [c_char; 256],
    }

    extern "C" {
        pub fn colmap_solve_sequence(
            frames: *const ColmapInputFrame,
            num_frames: usize,
            config: *const ColmapSolveConfig,
            out_result: *mut ColmapSolveResult,
        ) -> c_int;

        pub fn colmap_free_result(result: *mut ColmapSolveResult);
    }
}

/// Solve sparse 3D camera poses and sparse 3D structure across `frames` using COLMAP / GLOMAP.
pub fn solve_sequence(
    frames: &[GrayFrame<'_>],
    config: &SolveConfig,
) -> Result<SequenceReconstruction, ColmapError> {
    if frames.len() < 2 {
        return Err(ColmapError::InvalidInput(
            "Need at least 2 sampled frames for COLMAP match-move".to_string(),
        ));
    }
    for (idx, f) in frames.iter().enumerate() {
        let expected = (f.width as usize).saturating_mul(f.height as usize);
        if f.width == 0 || f.height == 0 || f.gray.len() < expected {
            return Err(ColmapError::InvalidInput(format!(
                "Invalid grayscale buffer on frame {idx}"
            )));
        }
    }

    let c_frames: Vec<ffi::ColmapInputFrame> = frames
        .iter()
        .map(|f| ffi::ColmapInputFrame {
            time_s: f.time_s,
            width: f.width,
            height: f.height,
            gray: f.gray.as_ptr(),
        })
        .collect();

    let c_cfg = ffi::ColmapSolveConfig {
        initial_focal_px: config.initial_focal_px,
        lock_fov: c_int::from(config.lock_fov),
        max_features: config.max_features.clamp(256, 8192) as c_int,
        use_opengl: c_int::from(config.use_opengl),
        prefer_glomap: c_int::from(config.prefer_glomap),
    };

    let mut raw = ffi::ColmapSolveResult {
        status: -1,
        used_opengl: 0,
        used_glomap: 0,
        focal_px: 0.0,
        mean_reproj_px: 0.0,
        num_poses: 0,
        poses: std::ptr::null_mut(),
        num_points: 0,
        points: std::ptr::null_mut(),
        error_msg: [0; 256],
    };

    let rc = unsafe {
        ffi::colmap_solve_sequence(c_frames.as_ptr(), c_frames.len(), &c_cfg, &mut raw)
    };

    struct Guard(*mut ffi::ColmapSolveResult);
    impl Drop for Guard {
        fn drop(&mut self) {
            unsafe { ffi::colmap_free_result(self.0) };
        }
    }
    let _guard = Guard(&mut raw);

    if rc != 0 || raw.status != 0 {
        let code = if rc != 0 { rc } else { raw.status };
        let msg = unsafe { CStr::from_ptr(raw.error_msg.as_ptr()) }
            .to_string_lossy()
            .trim()
            .to_string();
        let text = if msg.is_empty() {
            format!("COLMAP solver failed with status {code}")
        } else {
            msg
        };
        return Err(match code {
            -1 => ColmapError::InvalidInput(text),
            -2 => ColmapError::InsufficientMotion(text),
            -5 => ColmapError::Planar2D(text),
            _ => ColmapError::Internal(text),
        });
    }

    let raw_poses = if raw.poses.is_null() || raw.num_poses == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(raw.poses, raw.num_poses) }
    };
    let poses = raw_poses
        .iter()
        .map(|p| ReconstructedPose {
            time_s: p.time_s,
            registered: p.registered != 0,
            r_cw: [
                [p.r_cw[0], p.r_cw[1], p.r_cw[2]],
                [p.r_cw[3], p.r_cw[4], p.r_cw[5]],
                [p.r_cw[6], p.r_cw[7], p.r_cw[8]],
            ],
            t_cw: p.t_cw,
            focal_px: p.focal_px,
        })
        .collect();

    let raw_points = if raw.points.is_null() || raw.num_points == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(raw.points, raw.num_points) }
    };
    let points = raw_points
        .iter()
        .map(|pt| ReconstructedPoint3D {
            xyz: pt.xyz,
            observations: pt.observations,
            reproj_error: pt.reproj_error,
        })
        .collect();

    Ok(SequenceReconstruction {
        used_opengl: raw.used_opengl != 0,
        used_glomap: raw.used_glomap != 0,
        focal_px: raw.focal_px,
        mean_reproj_px: raw.mean_reproj_px,
        poses,
        points,
    })
}
