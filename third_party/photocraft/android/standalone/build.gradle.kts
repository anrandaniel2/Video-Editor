plugins {
    id("com.android.application")
    alias(libs.plugins.kotlin.compose)
}

// The standalone PhotoCraft app: a thin activity over the editor library next door (../app),
// which is the same module FilmCraft includes as :photocraft.
android {
    namespace = "io.github.artcraftandroid.photocraft.standalone"
    compileSdk = 37

    defaultConfig {
        applicationId = "io.github.artcraftandroid.photocraft"
        minSdk = 28
        targetSdk = 36
        versionName = providers.gradleProperty("photocraft.version").getOrElse("0.0.1-beta")
        versionCode = providers.gradleProperty("photocraft.versionCode").getOrElse("101").toInt()
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    buildFeatures {
        compose = true
    }
}

dependencies {
    implementation(project(":app"))
    implementation(libs.androidx.activity.compose)
    implementation(platform(libs.androidx.compose.bom))
    implementation(libs.androidx.compose.ui)
    implementation(libs.androidx.compose.material3)
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.lifecycle.runtime.ktx)
}
