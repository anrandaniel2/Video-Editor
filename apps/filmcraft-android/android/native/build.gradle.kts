import javax.inject.Inject
import org.gradle.process.ExecOperations

plugins {
    alias(libs.plugins.android.library)
}

// The one native library the app loads, and the Kotlin bindings of both of its halves.
//
// The Rust is the `filmcraft-android` crate (apps/filmcraft-android: the android project is its
// android/ folder), which now carries ProCraft's drawing and 3D half as a path dependency:
// `procraft-core`. Both halves need wgpu — and with it naga, the shader compiler — so shipping
// them as two shared libraries meant two copies of the GPU stack in the APK, one per library.
// Linked together they hold one, and UniFFI's scaffolding keeps each component's own namespace,
// so both Kotlin packages are generated from this library and both sets of entry points are in it.
//
// The module exists so that one module owns the library: :app and :procraft take the library and
// its bindings from here rather than generating them twice.
val cargoWorkspaceDir: File = rootDir.parentFile.parentFile.parentFile
val rustAbis: List<String> =
    providers.gradleProperty("filmcraft.abis").getOrElse("arm64-v8a,x86_64").split(',').map { it.trim() }.filter { it.isNotEmpty() }

// FilmCraft's floor. ProCraft's module is built on top of this library, so the library's own floor
// is whichever is higher: the merged code links against Android 10's platform either way.
val minApi = 29

/** rustup's `cargo` (Android Studio may not see rustup's PATH), else `cargo` from PATH; `CARGO` overrides. */
val cargoExecutable: String = run {
    val exe = if (System.getProperty("os.name").lowercase().startsWith("windows")) "cargo.exe" else "cargo"
    val rustup = File(System.getProperty("user.home"), ".cargo/bin/$exe")
    providers.environmentVariable("CARGO").orNull ?: if (rustup.isFile) rustup.absolutePath else exe
}

/** Builds `libfilmcraft_android.so` (release profile) for each ABI with cargo-ndk. */
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
        // Reproducible: the library mustn't hold the paths it was built in (panic locations name
        // source files), so the same commit builds byte-identical here, on CI and on F-Droid's
        // build server. Encoded flags (0x1f-separated) survive spaces in the paths.
        val cargoHome = System.getenv("CARGO_HOME")?.takeIf { it.isNotBlank() } ?: File(System.getProperty("user.home"), ".cargo").absolutePath
        val remap = listOf(
            "--remap-path-prefix=${File(workspaceDir.get()).absolutePath}=/filmcraft",
            "--remap-path-prefix=$cargoHome=/cargo",
        )
        // Identical code folding: the linker keeps one copy of each distinct function body instead
        // of one per monomorphisation, generic instantiation and inlined-from-the-same-source
        // duplicate, which is where a lot of a Rust library's text is. It folds code only, never
        // data, so the exported symbols and UniFFI's metadata are untouched, and identical
        // instructions at one address run exactly as fast as at two. Android's own platform builds
        // link this way. No build-time cost worth naming: it is a link step, not a codegen one.
        val fold = listOf("-Clink-arg=-Wl,--icf=all")
        execOperations.exec {
            workingDir(workspaceDir.get())
            environment("ANDROID_NDK_HOME", ndkDir.get())
            environment("CARGO_ENCODED_RUSTFLAGS", (remap + fold).joinToString("\u001f"))
            commandLine(
                listOf(cargo.get(), "ndk") + targets +
                    listOf("--platform", apiLevel.get().toString(), "-o", out, "build", "--release", "-p", "filmcraft-android", "--lib"),
            )
        }
        // cargo-ndk copies every shared library cargo left behind (dylib builds of a few
        // dependencies); this library links them statically, so only it ships
        File(out).walk().filter { it.isFile && it.name != "libfilmcraft_android.so" }.forEach { it.delete() }
    }
}

/**
 * Generates the Kotlin bindings of every UniFFI component the library holds (library mode reads
 * each component's metadata out of the library itself, and each crate's `uniffi.toml` names its
 * package).
 */
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
                cargo.get(), "run", "--quiet", "-p", "filmcraft-android-bindgen", "--",
                "generate", library.get().asFile.absolutePath, "--language", "kotlin", "--out-dir", out.absolutePath, "--no-format",
            )
        }
    }
}

android {
    namespace = "io.github.artcraftandroid.filmcraft.native"
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
            description = "Builds libfilmcraft_android.so for $rustAbis"
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
            description = "Generates the Kotlin bindings of libfilmcraft_android.so"
            cargo.set(cargoExecutable)
            workspaceDir.set(cargoWorkspaceDir.absolutePath)
            library.set(cargoBuild.flatMap { it.outputDir.file("${rustAbis.first()}/libfilmcraft_android.so") })
        }
        variant.sources.jniLibs?.addGeneratedSourceDirectory(cargoBuild, CargoNdkBuild::outputDir)
        variant.sources.java?.addGeneratedSourceDirectory(bindgen, UniffiBindgen::outputDir)
    }
}

dependencies {
    // the generated bindings reach the library through JNA, and so does everything that calls them
    api(libs.jna) { artifact { type = "aar" } }
    // UniFFI's bindings for async calls and callback interfaces
    api(libs.kotlinx.coroutines.android)
}
