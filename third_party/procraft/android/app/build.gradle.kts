plugins {
    id("com.android.library")
    alias(libs.plugins.kotlin.compose)
}

// ProCraft's own tree in this repository: its cargo workspace, and the repository root above it.
val procraftDir: File = projectDir.parentFile.parentFile
val repositoryDir: File = procraftDir.parentFile.parentFile

// The library's floor: FilmCraft's (29). ProCraft's Rust is no longer a library of its own — it is
// linked into FilmCraft's (see android/native in the app), which links against Android 10.
val minApi = 29

/**
 * Copies the repository's Inter faces (`assets/fonts`, one copy for the whole app) into `res/font`,
 * and their licence into `res/raw`: a font is a resource, and both themes, the core's text engine
 * and the licence screen read it from there.
 */
abstract class FontResources : DefaultTask() {
    @get:InputDirectory @get:PathSensitive(PathSensitivity.RELATIVE) abstract val sourceDir: DirectoryProperty
    @get:OutputDirectory abstract val outputDir: DirectoryProperty

    @TaskAction
    fun copy() {
        val out = outputDir.get().asFile
        out.deleteRecursively()
        File(out, "font").mkdirs()
        File(out, "raw").mkdirs()
        FACES.forEach { (face, resource) ->
            sourceDir.get().file(face).asFile.copyTo(File(out, "font/$resource"), overwrite = true)
        }
        // the faces' licence, shown in the app: shipped beside them, in res/raw
        File(out, "raw/inter_ofl.txt").writeText(sourceDir.get().file("OFL-Inter.txt").asFile.readText())
    }

    companion object {
        /** the face in `assets/fonts`, and the resource it becomes (resource names are lower case) */
        val FACES = listOf(
            "Inter-Regular.ttf" to "inter_regular.ttf",
            "Inter-Medium.ttf" to "inter_medium.ttf",
            "Inter-SemiBold.ttf" to "inter_semibold.ttf",
            "Inter-Bold.ttf" to "inter_bold.ttf",
        )
    }
}

android {
    namespace = "io.github.artcraftandroid.procraft"
    compileSdk = 37

    defaultConfig {
        minSdk = minApi
        consumerProguardFiles("proguard-rules.pro")
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    buildFeatures {
        compose = true
    }
}

androidComponents {
    beforeVariants(selector().withBuildType("release")) { builder ->
        builder.hostTests.getValue(com.android.build.api.variant.HostTestBuilder.UNIT_TEST_TYPE).enable = true
    }
    onVariants { variant ->
        val name = variant.name.replaceFirstChar { it.uppercase() }
        val fonts = tasks.register<FontResources>("interFonts$name") {
            group = "resources"
            description = "Copies the repository's Inter faces into res/font"
            sourceDir.set(File(repositoryDir, "assets/fonts"))
        }
        variant.sources.res?.addGeneratedSourceDirectory(fonts, FontResources::outputDir)
    }
}

tasks.configureEach {
    if (name.contains("ArtProfile")) enabled = false
}

dependencies {
    // The one native library, and the Kotlin bindings of both its halves: ProCraft's Rust is
    // linked into FilmCraft's, so this module takes the library rather than building one.
    implementation(project(":native"))
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(platform(libs.androidx.compose.bom))
    implementation(libs.androidx.compose.ui)
    implementation(libs.androidx.compose.material3)
    implementation("androidx.input:input-motionprediction:1.0.0")
    testImplementation(libs.junit)
}
