//! `uniffi-bindgen` for `photocraft-android`: generates its bindings from the built library, e.g.
//! `cargo run -p photocraft-bindgen -- generate libphotocraft_android.so --language kotlin --out-dir out`
//! (`android/app/build.gradle.kts` runs it).

fn main() {
    uniffi::uniffi_bindgen_main()
}
