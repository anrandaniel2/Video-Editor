//! `uniffi-bindgen` for `photocraft-android-core`: generates its Kotlin bindings from the built
//! library, e.g.
//! `cargo run -p photocraft-bindgen -- generate libphotocraft_core.so --language kotlin --out-dir out`
//! (`android/app/build.gradle.kts` runs it; the uniffi.toml beside the core crate names the Kotlin
//! package).

fn main() {
    uniffi::uniffi_bindgen_main()
}
