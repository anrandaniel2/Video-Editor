plugins {
    alias(libs.plugins.android.application) apply false
    // :photocraft (third_party/photocraft's editor) is a library module in this build.
    alias(libs.plugins.android.library) apply false
    alias(libs.plugins.kotlin.compose) apply false
}
