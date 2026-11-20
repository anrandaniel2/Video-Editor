# PhotoCraft embedded into ProCraft — verification record

This is the record for the work branch `arena/46e0c024-video-editor`: what was done, what was
verified, how, and exactly how to reproduce it. It is a companion to the commits, not a
substitute for them.

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
