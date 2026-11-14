import javax.inject.Inject
import org.gradle.process.ExecOperations

plugins {
    id("com.android.library")
}

// The Rust half of the vendored PhotoCraft: the `photocraft-android-core` crate (android/core) in
// the third_party/photocraft cargo workspace, two folders up. This module is what makes the release
// workflow compile PhotoCraft's engine for Android: `:app` depends on it, so `assembleRelease`
// builds `libphotocraft_core.so` and packages it beside FilmCraft's own library.
val cargoWorkspaceDir: File = projectDir.parentFile.parentFile
val rustAbis: List<String> =
    providers.gradleProperty("filmcraft.abis").getOrElse("arm64-v8a,x86_64").split(',').map { it.trim() }.filter { it.isNotEmpty() }
val minApi = 29

/** rustup's `cargo` (Android Studio may not see rustup's PATH), else `cargo` from PATH; `CARGO` overrides. */
val cargoExecutable: String = run {
    val exe = if (System.getProperty("os.name").lowercase().startsWith("windows")) "cargo.exe" else "cargo"
    val rustup = File(System.getProperty("user.home"), ".cargo/bin/$exe")
    providers.environmentVariable("CARGO").orNull ?: if (rustup.isFile) rustup.absolutePath else exe
}

/** Builds `libphotocraft_core.so` (release profile) for each ABI with cargo-ndk. */
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
            "--remap-path-prefix=${File(workspaceDir.get()).absolutePath}=/photocraft",
            "--remap-path-prefix=$cargoHome=/cargo",
        )
        execOperations.exec {
            workingDir(workspaceDir.get())
            environment("ANDROID_NDK_HOME", ndkDir.get())
            // the job sets CARGO_TERM_COLOR=always; a compile error is easier to find in plain text
            environment("CARGO_TERM_COLOR", "never")
            environment("CARGO_ENCODED_RUSTFLAGS", remap.joinToString("\u001f"))
            // The build is long and its output is where a compile error shows up: stream it.
            isIgnoreExitValue = false
            commandLine(
                listOf(cargo.get(), "ndk") + targets +
                    listOf("--platform", apiLevel.get().toString(), "-o", out, "build", "--release", "-p", "photocraft-android-core", "--lib"),
            )
        }
        // only the engine's library ships (cargo-ndk copies any other shared library cargo left)
        File(out).walk().filter { it.isFile && it.name != "libphotocraft_core.so" }.forEach { it.delete() }
    }
}

android {
    namespace = "io.github.artcraftandroid.photocraft"
    compileSdk = 37
    ndkVersion = "29.0.14206865"

    defaultConfig {
        minSdk = minApi
        ndk { abiFilters.addAll(rustAbis) }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

androidComponents {
    onVariants { variant ->
        val name = variant.name.replaceFirstChar { it.uppercase() }
        val cargoBuild = tasks.register<CargoNdkBuild>("cargoBuild$name") {
            group = "rust"
            description = "Builds libphotocraft_core.so for $rustAbis"
            cargo.set(cargoExecutable)
            workspaceDir.set(cargoWorkspaceDir.absolutePath)
            ndkDir.set(androidComponents.sdkComponents.ndkDirectory.map { it.asFile.absolutePath })
            abis.set(rustAbis)
            apiLevel.set(minApi)
            // cargo knows what changed
            outputs.upToDateWhen { false }
        }
        variant.sources.jniLibs?.addGeneratedSourceDirectory(cargoBuild, CargoNdkBuild::outputDir)
    }
}
