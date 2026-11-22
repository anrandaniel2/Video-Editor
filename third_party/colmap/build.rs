use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn collect_ext(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|d| d.path())).collect();
    paths.sort();
    for p in paths {
        if p.is_dir() {
            collect_ext(&p, ext, out);
        } else if p.extension().and_then(|e| e.to_str()) == Some(ext) {
            out.push(p);
        }
    }
}

fn configure_common(b: &mut cc::Build, root: &Path) {
    b.warnings(false)
        .opt_level(2)
        .include(root.join("eigen"))
        .include(root.join("poselib"))
        .include(root.join("ceres/include"))
        .include(root.join("ceres/config"))
        .include(root.join("ceres/internal"))
        .include(root.join("ceres/internal/ceres/miniglog"))
        .include(root.join("colmap/src"))
        .include(root.join("colmap/src/thirdparty/VLFeat"))
        .include(root.join("bridge"))
        .define("COLMAP_GPU_ENABLED", None)
        .define("COLMAP_OPENGL_ENABLED", None)
        .define("VL_DISABLE_SSE2", None)
        .define("VL_DISABLE_AVX", None)
        .define("VL_DISABLE_THREADS", None)
        .define("NDEBUG", None)
        .cargo_metadata(false);
}

fn build_cpp_group(name: &str, files: &[PathBuf], root: &Path, is_android: bool) {
    let mut b = cc::Build::new();
    configure_common(&mut b, root);
    b.cpp(true)
        .flag_if_supported("-std=c++17")
        .flag_if_supported("/std:c++17");
    if is_android {
        b.cpp_link_stdlib(None);
    }
    b.files(files);
    b.compile(name);
}

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let is_android = target_os == "android";

    println!("cargo:rerun-if-changed=bridge/colmap_matchmove_c.h");
    println!("cargo:rerun-if-changed=bridge/colmap_matchmove_c.cc");

    let mut vlfeat_c = Vec::new();
    collect_ext(&manifest_dir.join("colmap/src/thirdparty/VLFeat"), "c", &mut vlfeat_c);

    let mut poselib_cc = Vec::new();
    collect_ext(&manifest_dir.join("poselib/PoseLib"), "cc", &mut poselib_cc);

    let mut ceres_cc = Vec::new();
    collect_ext(&manifest_dir.join("ceres/internal/ceres"), "cc", &mut ceres_cc);

    let mut colmap_cc = Vec::new();
    collect_ext(&manifest_dir.join("colmap/src/colmap"), "cc", &mut colmap_cc);
    collect_ext(&manifest_dir.join("bridge"), "cc", &mut colmap_cc);

    // Three balanced build streams (at most 3 concurrent C++ compilers to bound peak RAM on CI).
    std::thread::scope(|s| {
        let root = &manifest_dir;
        s.spawn(move || build_cpp_group("colmap_sparse", &colmap_cc, root, is_android));
        s.spawn(move || build_cpp_group("colmap_ceres", &ceres_cc, root, is_android));
        s.spawn(move || {
            build_cpp_group("colmap_poselib", &poselib_cc, root, is_android);
            let mut c_build = cc::Build::new();
            configure_common(&mut c_build, root);
            c_build.flag_if_supported("-std=gnu11");
            c_build.files(&vlfeat_c);
            c_build.compile("colmap_vlfeat");
        });
    });

    // Emit libraries in topological dependency order:
    // colmap_sparse -> (colmap_poselib, colmap_ceres, colmap_vlfeat)
    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=colmap_sparse");
    println!("cargo:rustc-link-lib=static=colmap_poselib");
    println!("cargo:rustc-link-lib=static=colmap_ceres");
    println!("cargo:rustc-link-lib=static=colmap_vlfeat");

    if is_android {
        println!("cargo:rustc-link-lib=static=c++_static");
        println!("cargo:rustc-link-lib=static=c++abi");
        println!("cargo:rustc-link-lib=log");
        println!("cargo:rustc-link-lib=dl");
    } else if target_os == "linux" {
        println!("cargo:rustc-link-lib=stdc++");
        println!("cargo:rustc-link-lib=dl");
        println!("cargo:rustc-link-lib=pthread");
        println!("cargo:rustc-link-lib=m");
    } else if target_os == "macos" || target_os == "ios" {
        println!("cargo:rustc-link-lib=c++");
    }
}
