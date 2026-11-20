# PhotoCraft embedded into ProCraft — verification record

This is the record for the work branch `arena/46e0c024-video-editor`: what was done, what was
verified, how, and exactly how to reproduce it. It is a companion to the commits, not a
substitute for them. It now covers both bundles on the branch: the embedded-canvas work
below, and the 3D scene bundle appended at the end.

## What the work was

1. **PhotoCraft is gone end to end; photo editing is the ProCraft Draw canvas.** A picture is
   opened in the same hosted canvas the drawing uses — the picture painted on a document, then
   given every brush, layer and adjustment the drawing canvas has. A photo saved from the host
   goes back to the photo library (or overwrites its source when the mime type matches); a
   drawing stays linked in the drawings gallery. One decision function (`VisualRoute.kt`) routes
   the three library kinds — `video` → the timeline editor, `drawing` → the layered native
   document, `picture` (and anything unrecognised) → the Draw canvas as a photo document — and
   that routing is unit-tested.
2. **Fc tokens now delegate to the Pc reference tokens** (`Theme.kt`): the surround, gallery
   background, rows, panels, ink, icon and accent colours, plus `FcSize.TopBar`/`PanelCorner`,
   are `Pc.*` underneath (one reference, so a plugin-given accent follows through). The values
   are identical in the dark interface FilmCraft uses. Fc tokens that have no Pc counterpart
   (clips, lanes, negatives, the timeline's own set) stay FilmCraft's own.
3. **A host-only Shapes library panel in ProCraft** (`vm.hostShapes`, set only by a hosting
   app — FilmCraft): the DrawableKinds the QuickShape recognition can snap a held stroke to,
   hung from the top bar's Shapes button as a popover. It draws nothing itself and forks no
   shape code — it drives the native entry points (`chooseQuickShape`, `quickShapeEditing`,
   `endQuickShape`) exactly as QuickShapeBar does. ProCraft standalone never shows it.
4. **CI runs the tests now.** The workflow only assembled the APK before, so no test could
   fail; the Rust suite and `:app`'s Kotlin unit tests run per push (see the command list).

## Verification matrix

All rows below are green in CI on this branch:
[run 44657385345](https://github.com/anrandaniel2/Video-Editor/actions/runs/44657385345)
(its only delta over [44653371718](https://github.com/anrandaniel2/Video-Editor/actions/runs/44653371718),
which also went fully green, is per-test `::notice` annotation lines in the two QuickShape steps).

| # | Deliverable | Command / step | Result |
|---|-------------|----------------|--------|
| 1 | PhotoCraft deletion breaks nothing in FilmCraft's Rust (`filmcraft-android` is the UniFFI crate the whole app is built over) | `cargo test -p filmcraft-android` ("The whole engine") | pass (step exit 0; this step emits annotations only on failure) |
| 2 | QuickShape engine: recognition, handle model, arc boundary tests (incl. `an_arc_handle_index_past_its_nodes_is_ignored`) | `cargo test --manifest-path third_party/procraft/Cargo.toml -p procraft-engine quickshape` | pass — 10 tests, all `ok` (annotated) |
| 3 | QuickShape core: session integration, both entry paths (explicit panel lift via `quickshape_last`, and the long-press `quickshape_hold`) | `cargo test --manifest-path third_party/procraft/Cargo.toml -p procraft-core quickshape` | pass — `tests::quickshape_an_arc_snaps_and_bends_by_its_middle_in_both_entry_paths ... ok`, plus the 6 inline `quickshape::tests::*` |
| 4 | 3D-view panoramas decode pixel-exactly (the decoder cannot silently swap/mix channels) | `cargo test --manifest-path third_party/procraft/Cargo.toml -p procraft-engine environment::` ("The panoramas") | pass — 3 tests, all `ok` (annotated) |
| 5 | Kotlin unit tests, incl. `VisualRouteTest` (video/drawing/picture/unknown routing) | `./gradlew :app:testReleaseUnitTest` (APK job, "Unit tests (Kotlin)") | pass |
| 6 | Everything Kotlin compiles tree-wide (ShapesPanel, Fc→Pc aliasing, VisualRoute, the hosted canvas, gallery) | `./gradlew assembleRelease -Pfilmcraft.abis=arm64-v8a` (APK job, "Build") | pass |
| 7 | APK assembles, SIGNS, size budget, native library presence and entry points | "Sign" → "Size" → "The native libraries, and every entry point" → "APK" | pass — `filmcraft-android-0.0.61-beta.apk`, 34.0 MB (79.1 MB unpacked), arm64-v8a; 53 `ffi_filmcraft_android` entry points, no `libprocraft_core.so` (ProCraft's Rust is linked into FilmCraft's native library, `libfilmcraft_android.so`); signed (public CI test key — see caveats) |
| 8 | whitespace hygiene (`git diff --check`) | authoring sandbox, this branch | pass |

Not verified, deliberately and on the record:

- **On-device behaviour.** No phone ran any of this. The Compose UI (Shapes panel, token
  aliasing, routes) is compile-verified and code-reviewed, not tap-tested; Kotlin unit tests
  cover `VisualRoute` only — there is no Compose/UI test harness in the project.
- **Device Vulkan/GLES rendering.** The GPU-gated Rust tests ran on a GLES-capable software
  stack if and only if `Gpu::headless()` succeeded on the runner; the arc integration test is
  annotated `ok`, which on a GPU-less runner is also what an early-returned test prints. The
  assertions are real where the GPU exists; there is no device-side rendering verification.
- **The `GitHub Release` job** is tag-gated and was skipped on this push (as designed). The
  assembled APK is a **dry-run signed with the public CI test key** (CN=FilmCraft Dry Run),
  not the Play/F-Droid release key; no release was cut.
- **Desktop/other target builds** were not run.

## The CI command list (as wired in `.github/workflows/release.yml`)

```console
# Rust tests job
cargo test --manifest-path third_party/procraft/Cargo.toml -p procraft-engine quickshape
cargo test --manifest-path third_party/procraft/Cargo.toml -p procraft-core quickshape
cargo test -p filmcraft-android
cargo test --manifest-path third_party/procraft/Cargo.toml -p procraft-engine environment::

# APK job (working-directory: apps/filmcraft-android/android)
./gradlew assembleRelease -Pfilmcraft.abis=arm64-v8a
./gradlew :app:testReleaseUnitTest
```

Local reproduction notes:

- `procraft-core`'s tests `include_bytes!` the Inter faces from the ProCraft tree's
  `android/app/src/main/assets/fonts/`, a path the Gradle build fills in place from the
  repository's `assets/fonts/` (the `FontResources` task). `cargo test` has no Gradle, so the
  fonts must be materialised first or the test build fails on the missing include (this was
  the exit 101 that made three CI runs fail before it was diagnosed):

  ```console
  mkdir -p third_party/procraft/android/app/src/main/assets/fonts
  cp assets/fonts/Inter-*.ttf third_party/procraft/android/app/src/main/assets/fonts/
  ```

  (The CI job does exactly this before running the tests.)
- Host Rust only; `--manifest-path` is required for the ProCraft workspace (the root workspace
  excludes `third_party`).
- No cargo/rustc/java/gradle exist in the authoring sandbox; every compile/test result above
  is CI output, not local output.

## Deliberate removals and scope calls (flagged for review)

- **PhotoCraft is deleted, not deprecated**: `third_party/photocraft`, the `photocraft` Rust
  crate (`apps/filmcraft-android/src/photocraft/*` — brush, commands, compose, model, paint,
  select, text and their tests), its Kotlin host (`PhotoCraftIntegration.kt`, `PhotoEditorScreen`,
  CanvasMapping, ViewModel, engine/studio files), `brush_sheet.rs`, the Gradle `sourceSets`,
  the `.gitignore` entries and the CI photo-engine step. 15,224 deleted lines against
  508 added (branch diff against its base).
- **The in-timeline playhead-frame "Photo" button and its stashed-picture path are removed,
  not re-routed** to the hosted canvas. A photo now always opens as its own hosted document;
  the overlay path (`prepareHostOverlay`) exists only for a frame handed to the canvas as
  reference.
- **PhotoCraft's flat PSD/TIFF export is not replaced.** Export is the Draw canvas's formats
  (PNG / JPEG XL / AVIF / JPEG via the close sheet), one picture per photo.
- **Photo documents reopen as photo documents** (Photo library entry), not drawings: the
  gallery's `.filmcraft-photo` link and the `drawingPhotoDestination` carry the distinction.
- **The Shapes panel is host-only by design.** It exists for FilmCraft and nowhere else; the
  `vm.hostShapes` flag is only settable by a host, and the popover hangs off the Shapes button
  which only appears when `vm.hostClose != null` (another host-injected flag).
- **`QuickShapeKind.CURVE`** (a plugin's curve) is shown by the panel but not offered as a
  DrawableKind — it is not a shape the recognition snaps to.
- **`move_handle` ignores an out-of-range handle index** in the engine (new `handle_count`,
  no-op guard) instead of trusting the caller; the arc tests pin the boundary.
- **The only remaining `photocraft` reference** anywhere is the `plugin_file.rs` "other app"
  compatibility fixture (`{"photocraft": {"interface": 1}}`) in ProCraft's plugin format test —
  compatibility surface for plugin files, not the deleted app. The similar name in this branch
  history is intentional.
- **`FcType` styles and FilmCraft-only tokens stay literal** (clips/lanes/negatives); only
  tokens with a true Pc reference were aliased.
- **CI only**: quickshape-test name filters (`quickshape`) select the module tests and the
  `tests::quickshape*` integration test; GPU-gated core tests may early-return on GPU-less
  runners. The visible `running 0 tests` lines in the step logs are the doc-test and
  `brush_parity` binaries matching the filter — expected, not a failure.

## The 3D scene bundle (appended 2026-11-20)

The branch then grew a second, self-contained bundle: the 3D scene. It is verified by the
same CI system, on [run 44832894036](https://github.com/anrandaniel2/Video-Editor/actions/runs/44832894036)
(head `580c124`; three fix commits after the main bundle commit `c80eba7` — a missing
`Renderer.preview_camera` field, a Kotlin nullable-receiver/elvis pair in `GizmoGeometry`, and a
JVM declaration clash between the orbit property's synthetic setter and `setSceneOrbit()`).
Both jobs green; the tag-gated GitHub Release job skipped as designed.

1. **A scene camera in the sequence.** `SceneCamera3D` (serde-optional — projects saved before
   this change decode unchanged). Absent: the classic fixed camera renders exactly as before.
   Present: the 3D layer renders through it in both the CPU rasteriser and the WGSL pipeline
   (a view matrix in the scene uniform; classic path untouched). The engine exposes
   `scene.camera` / `scene.setCamera` / `scene.clearCamera` (`scene.setCamera` clamps fov to
   1..179 degrees and takes an optional merge token for journal coalescing). Exports always
   render the sequence's camera; the editor's orbit view never reaches an export.
2. **The orbit grid view (the "3D Scene" panel).** Editor-only: a floor grid, the world axes
   and a movable orbit target drawn over the preview; drag empty space to orbit, two fingers
   to dolly. The preview renders through an orbit camera handed in by the editor
   (`Renderer::set_scene_preview_camera` + a thread-local `with_preview_camera` the render
   reads); captures and exports bypass it by construction — poster and export paths render
   through the sequence's own camera, not the grid view.
3. **The fused gizmo, fused everywhere.** Move shafts and rotate rings drawn as one
   z-sorted mesh and hit-tested together; works for object 3D layers and scene lights. Its
   screen tangent is taken through the orbit projection when the scene panel is open. The
   object's world position stays *defined* by the classic-camera unprojection (matching
   `camera_translation` in the renderer), so `GizmoGeometry.move()` projects/unprojects
   consistently and no inverse scene projection exists or is required for that math.
4. **A mirror that is a mirror.** The studio environment is a small softbox room in both
   shading paths; `Reflection` (0–200%, 100 is neutral) and `Emissive` (0–100%) are ordinary
   float parameters on the object-3D effect (`env_reflection`, `emissive`) and therefore
   keyframe. `Material.env_intensity` carries the reflection strength in the material uniform.
5. **Panels, flat and glass.** Tool tiles are flat Draw-style buttons (accent when selected),
   the panel header is flatter, slider rows dropped the frosted card, chips are neutral
   rounded rows, preset tiles carry an accent ring when selected, and the tool rail's glass is
   a tighter, narrower pane. Only `Fc` tokens with a true `Pc` reference were re-pointed;
   nothing was renamed.

Verification matrix (this bundle):

| # | Deliverable | Command / step | Result |
|---|-------------|----------------|--------|
| 1 | Rust compiles tree-wide including the new fields (`Renderer.preview_camera`, `SceneCamera3D`) | "The whole engine" (`cargo test -p filmcraft-android`) | pass (step exit 0) |
| 2 | Engine command surface (`scene.camera`/`setCamera`/`clearCamera`) | code review + compile | **no dedicated test exists** — the command handlers are not covered by the Rust suite; not evidence of correctness |
| 3 | Kotlin compiles tree-wide incl. the gizmo and the scene panel | `./gradlew assembleRelease -Pfilmcraft.abis=arm64-v8a` | pass |
| 4 | Kotlin unit tests | `./gradlew :app:testReleaseUnitTest` | pass (step exit 0) |
| 5 | APK assembles, signs, native entry-point presence, size budget | Sign / Size / entry points / APK steps | pass — `filmcraft-android-0.0.61-beta.apk`, 34.1 MB (79.3 MB unpacked), 53 `ffi_filmcraft_android` entry points, arm64-v8a |
| 6 | whitespace hygiene | `git diff --check` before commit | pass |

Not verified, on the record:

- **On-device anything.** No phone ran this: orbit feel, gizmo dragging while orbiting,
  reflection quality on real hardware, and the scene panel's layout. What CI proves is
  compile, unit tests, packaging and entry points.
- **The GPU-gated Rust tests** inherit the earlier caveat (runner GPU stack dependent; some
  early-return when no GPU is present).
- **The WGSL studio environment and PBR fragment** were eyeballed against the CPU path for
  parity (same studio function constants, same scene-camera branch, `env` uniform), not
  pixel-diffed on a device.
