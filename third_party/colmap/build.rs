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

fn configure_common(b: &mut cc::Build, root: &Path, is_android: bool) {
    b.warnings(false)
        .opt_level(2)
        .flag_if_supported("-fvisibility=hidden")
        .include(root.join("eigen"))
        .include(root.join("poselib"))
        .include(root.join("ceres/include"))
        .include(root.join("ceres/internal"))
        .include(root.join("ceres/internal/ceres/miniglog"))
        .include(root.join("colmap/src"))
        .include(root.join("bridge"))
        .define("COLMAP_GPU_ENABLED", None)
        .define("COLMAP_OPENGL_ENABLED", None)
        .define("VL_DISABLE_SSE2", None)
        .define("VL_DISABLE_AVX", None)
        .define("VL_DISABLE_THREADS", None)
        .define("NDEBUG", None)
        .cargo_metadata(false);
    if is_android {
        b.define("ANDROID", None);
    }
}

fn build_cpp_group(name: &str, files: &[PathBuf], root: &Path, is_android: bool) {
    let mut b = cc::Build::new();
    configure_common(&mut b, root, is_android);
    b.cpp(true)
        .flag_if_supported("-std=c++17")
        .flag_if_supported("/std:c++17")
        .flag_if_supported("-fvisibility-inlines-hidden");
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
            configure_common(&mut c_build, root, is_android);
            c_build.flag_if_supported("-std=gnu11");
            c_build.flag("-iquote");
            c_build.flag(root.join("colmap/src/thirdparty/VLFeat").to_string_lossy().as_ref());
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
        let mut probe = cc::Build::new();
        probe.cpp(true);
        let compiler = probe.get_compiler();
        let mut candidate_dirs: Vec<PathBuf> = Vec::new();
        let mut push_dir = |d: PathBuf| {
            if d.exists() && !candidate_dirs.contains(&d) {
                candidate_dirs.push(d);
            }
        };

        for lib_name in ["libc++_static.a", "libc++abi.a"] {
            if let Ok(out) = compiler.to_command().arg(format!("-print-file-name={lib_name}")).output() {
                if out.status.success() {
                    let s = String::from_utf8_lossy(&out.stdout);
                    let p = PathBuf::from(s.trim());
                    if p.is_absolute() && p.exists() {
                        if let Some(parent) = p.parent() {
                            push_dir(parent.to_path_buf());
                        }
                    }
                }
            }
        }

        // Fallback NDK sysroot lookup relative to `<ndk>/toolchains/llvm/prebuilt/<host>/bin/clang++`.
        let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
        let ndk_triple = match target_arch.as_str() {
            "aarch64" => "aarch64-linux-android",
            "arm" => "arm-linux-androideabi",
            "x86_64" => "x86_64-linux-android",
            "x86" => "i686-linux-android",
            _ => "aarch64-linux-android",
        };
        if let Some(prebuilt) = compiler.path().parent().and_then(Path::parent) {
            push_dir(prebuilt.join("sysroot/usr/lib").join(ndk_triple));
        }
        for env_key in ["ANDROID_NDK_HOME", "ANDROID_NDK_ROOT", "NDK_HOME"] {
            if let Ok(ndk) = env::var(env_key) {
                let prebuilts = PathBuf::from(ndk).join("toolchains/llvm/prebuilt");
                if let Ok(entries) = fs::read_dir(prebuilts) {
                    for entry in entries.flatten() {
                        push_dir(entry.path().join("sysroot/usr/lib").join(ndk_triple));
                    }
                }
            }
        }

        // Stage ONLY libc++_static.a and libc++abi.a into OUT_DIR under private archive names.
        // Never add `<ndk>/sysroot/usr/lib/<triple>` directly to `rustc-link-search`: that directory
        // also holds static Bionic `libc.a`/`libdl.a`/`libm.a` and `libc++_shared.so`.
        for (src_name, dst_stem) in [
            ("libc++_static.a", "filmcraft_cxx_static"),
            ("libc++abi.a", "filmcraft_cxxabi_static"),
        ] {
            let src = candidate_dirs
                .iter()
                .map(|d| d.join(src_name))
                .find(|p| p.exists())
                .unwrap_or_else(|| panic!("Failed to locate Android NDK {src_name} in {candidate_dirs:?}"));
            let dst = out_dir.join(format!("lib{dst_stem}.a"));
            fs::copy(&src, &dst).unwrap_or_else(|e| panic!("Failed to stage {src:?} -> {dst:?}: {e}"));
            println!("cargo:rustc-link-lib=static={dst_stem}");
        }
        // Note: cargo-ndk's generated `libgcc.a` (`INPUT(-lunwind)`) already links a single unwinder
        // at the end of the link line after `c++abi`; do not link a second `libunwind.a`.
        println!("cargo:rustc-link-lib=log");
        println!("cargo:rustc-link-lib=dl");
        println!("cargo:rustc-link-lib=m");
    } else if target_os == "linux" {
        println!("cargo:rustc-link-lib=stdc++");
        println!("cargo:rustc-link-lib=dl");
        println!("cargo:rustc-link-lib=pthread");
        println!("cargo:rustc-link-lib=m");
    } else if target_os == "macos" || target_os == "ios" {
        println!("cargo:rustc-link-lib=c++");
    }
}
