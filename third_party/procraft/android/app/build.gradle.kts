import javax.inject.Inject
import org.gradle.process.ExecOperations

plugins {
    id("com.android.library")
    alias(libs.plugins.kotlin.compose)
}

// The Rust half of the app: the `procraft-core` crate (crates/core) in the ProCraft cargo
// workspace, one folder up.
val cargoWorkspaceDir: File = projectDir.parentFile.parentFile
val rustAbis: List<String> =
    providers.gradleProperty("filmcraft.abis").getOrElse("arm64-v8a,x86_64").split(',').map { it.trim() }.filter { it.isNotEmpty() }
val minApi = 28

/** rustup's `cargo` (Android Studio may not see rustup's PATH), else `cargo` from PATH; `CARGO` overrides. */
val cargoExecutable: String = run {
    val exe = if (System.getProperty("os.name").lowercase().startsWith("windows")) "cargo.exe" else "cargo"
    val rustup = File(System.getProperty("user.home"), ".cargo/bin/$exe")
    providers.environmentVariable("CARGO").orNull ?: if (rustup.isFile) rustup.absolutePath else exe
}

/** Builds `libprocraft_core.so` (release profile) for each ABI with cargo-ndk. */
abstract class CargoNdkBuild : DefaultTask() {
    @get:Inject abstract val execOperations: ExecOperations
    @get:Input abstract val cargo: Property<String>
    @get:Input abstract val workspaceDir: Property<String>
    @get:Input abstract val ndkDir: Property<String>
    @get:Input abstract val abis: ListProperty<String>
    @get:Input abstract val apiLevel: Property<Int>
    @get:OutputDirectory abstract val outputDir: DirectoryProperty

    @TaskAction
    fun build() {
        val targets = abis.get().flatMap { listOf("-t", it) }
        val out = outputDir.get().asFile.absolutePath
        // the library mustn't hold the paths it was built in (panic locations name source files)
        val cargoHome = System.getenv("CARGO_HOME")?.takeIf { it.isNotBlank() } ?: File(System.getProperty("user.home"), ".cargo").absolutePath
        val remap = listOf(
            "--remap-path-prefix=${File(workspaceDir.get()).absolutePath}=/procraft",
            "--remap-path-prefix=$cargoHome=/cargo",
        )
        // Identical code folding: one copy of each distinct function body rather than one per
        // monomorphisation and generic instantiation. Code only, never data, so nothing the
        // library exports or reads by symbol changes (the app's own build says the same).
        val fold = listOf("-Clink-arg=-Wl,--icf=all")
        execOperations.exec {
            workingDir(workspaceDir.get())
            environment("ANDROID_NDK_HOME", ndkDir.get())
            environment("CARGO_ENCODED_RUSTFLAGS", (remap + fold).joinToString("\u001f"))
            commandLine(
                listOf(cargo.get(), "ndk") + targets +
                    listOf("--platform", apiLevel.get().toString(), "-o", out, "build", "--release", "-p", "procraft-core", "--lib"),
            )
        }
        // only the app's library ships (cargo-ndk copies any other shared library cargo left)
        File(out).walk().filter { it.isFile && it.name != "libprocraft_core.so" }.forEach { it.delete() }
    }
}

/** The repository root: ProCraft sits in `third_party`, and the fonts are FilmCraft's. */
val repositoryDir: File = cargoWorkspaceDir.parentFile.parentFile

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

/** Generates the Kotlin bindings from the library cargo built (UniFFI library mode). */
abstract class UniffiBindgen : DefaultTask() {
    @get:Inject abstract val execOperations: ExecOperations
    @get:Input abstract val cargo: Property<String>
    @get:Input abstract val workspaceDir: Property<String>
    @get:InputFile @get:PathSensitive(PathSensitivity.NONE) abstract val library: RegularFileProperty
    @get:OutputDirectory abstract val outputDir: DirectoryProperty

    @TaskAction
    fun generate() {
        val out = outputDir.get().asFile
        out.deleteRecursively()
        out.mkdirs()
        execOperations.exec {
            workingDir(workspaceDir.get())
            commandLine(
                cargo.get(), "run", "--quiet", "-p", "procraft-bindgen", "--",
                "generate", library.get().asFile.absolutePath, "--language", "kotlin", "--out-dir", out.absolutePath, "--no-format",
            )
        }
    }
}

android {
    namespace = "io.github.artcraftandroid.procraft"
    compileSdk = 37
    ndkVersion = "29.0.14206865"

    defaultConfig {
        minSdk = minApi
        consumerProguardFiles("proguard-rules.pro")
        ndk { abiFilters.addAll(rustAbis) }
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
        val cargoBuild = tasks.register<CargoNdkBuild>("cargoBuild$name") {
            group = "rust"
            description = "Builds libprocraft_core.so for $rustAbis"
            cargo.set(cargoExecutable)
            workspaceDir.set(cargoWorkspaceDir.absolutePath)
            ndkDir.set(androidComponents.sdkComponents.ndkDirectory.map { it.asFile.absolutePath })
            abis.set(rustAbis)
            apiLevel.set(minApi)
            // cargo knows what changed
            outputs.upToDateWhen { false }
        }
        val bindgen = tasks.register<UniffiBindgen>("uniffiBindgen$name") {
            group = "rust"
            description = "Generates the Kotlin bindings of libprocraft_core.so"
            cargo.set(cargoExecutable)
            workspaceDir.set(cargoWorkspaceDir.absolutePath)
            library.set(cargoBuild.flatMap { it.outputDir.file("${rustAbis.first()}/libprocraft_core.so") })
        }
        variant.sources.jniLibs?.addGeneratedSourceDirectory(cargoBuild, CargoNdkBuild::outputDir)
        variant.sources.java?.addGeneratedSourceDirectory(bindgen, UniffiBindgen::outputDir)
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
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(platform(libs.androidx.compose.bom))
    implementation(libs.androidx.compose.ui)
    implementation(libs.androidx.compose.material3)
    implementation("androidx.input:input-motionprediction:1.0.0")
    implementation(libs.jna) { artifact { type = "aar" } }
    testImplementation(libs.junit)
}
