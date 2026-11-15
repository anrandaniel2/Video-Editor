# PhotoCraft integration

Vendored from PhotoCraft (the photo editor of the ArtCraft/Storytold family), version 0.23.1,
under MIT OR Apache-2.0. Upstream copyright, licenses, NOTICE and attribution accompany the
source (`LICENSE-MIT`, `LICENSE-APACHE`, `NOTICE`, `ATTRIBUTION.md`).

FilmCraft builds this isolated Cargo workspace and the Android editor as the `:photocraft`
Android **library**. `libphotocraft_android.so` ships next to FilmCraft's own library, and the
photo editor is PhotoCraft's real Rust engine — not a reimplementation.

## What is here

- `crates/*`: the engine crates the editor needs (geom, color, raster, doc, ops, compose, paint,
  algo, io, text, vector, format, raw, plugins, engine, ipp, testkit). The desktop egui UI
  (`crates/ui-egui`, `crates/gpu`, `apps/*`, `xtask`) is **not** vendored: the Android app drives
  `photocraft-engine` directly. `Cargo.toml`'s `members` lists exactly what is here.
- `assets/fonts`, `assets/dict`, `crates/algo/data/libfacedetection.bin`: the files the crates
  embed with `include_bytes!` (text rendering, spell check, face detection).
- `crates/android`: the UniFFI bridge, `photocraft-android` (`libphotocraft_android.so`).
- `android/bindgen`: `uniffi-bindgen` for that library (UniFFI 0.32.2, exactly).
- `android/app`: the editor library — `io.github.artcraftandroid.photocraft` — with the Compose
  editor, the theme, the icons and the engine wrapper. FilmCraft includes this directory as
  `:photocraft`; it declares no activity of its own.
- `android/standalone`: the standalone PhotoCraft app, a thin activity on top of the library.

## The bridge

`crates/android/src/lib.rs` is the only door between the engine and Kotlin:

- `PhotoSession.execute(id, paramsJson) -> json`: every tool, adjustment, filter, layer command
  and selection is one of the engine's own command ids. The editor does not keep a parallel model
  of the document.
- `PhotoSession.exportBytes(format, quality) -> ByteArray`: encoded file bytes. Layered formats
  (PSD, layered TIFF) keep the layers.
- The editor's model releases the session in `onCleared`, and FilmCraft clears the model's
  `ViewModelStore` when the panel closes: the Rust documents (and their pixels) are freed then,
  not when the process dies.
- `PhotoSession.render/renderRegion/thumbnail`: composited RGBA8 for the canvas. The editor
  renders the region it shows (`documentView()`: the whole picture at 100 %, a window of it as
  the zoom goes up), so what is on screen and what a finger maps to are the same rectangle.
  `render`/`thumbnail` go through the engine's own area-averaged reduction
  (`photocraft_compose::render_reduced`), not a full-size composite followed by a resize: a
  fit-to-screen preview of a 24 MP document costs a thumbnail's memory, not 380 MB.
- The engine's file-system commands (`file.open`, `file.save`, …) are unused: the app shell owns
  the file system (SAF, MediaStore, the project folder) and hands the engine bytes.

The panels are built from the engine's registry (`command.list`), so a panel lists exactly the
commands the engine has, with the engine's own labels, menu placement and enabled/disabled state.

## The editor

The canvas is a real editor surface, not a preview:

- **Painting on the canvas.** A drag is a stroke: Brush, Pencil, Eraser, Clone Stamp (a tap sets
  its source first) and the Healing Brush paint along it, drawn under the finger as it goes and
  sent to the engine once, on release, so each stroke is one History step. A tap with Fill
  (`paint.bucket`), the Gradient tool, the Magic Wand (`select.magicWand`), the Eyedropper
  (`document.sampleColor`) or the Type tool acts on its own.
- **Selections.** The Rectangular Marquee takes a drag as `select.rect`; the Lasso takes the
  path the finger drew (thinned to a few pixels of travel) as `select.lasso`.
- **Zoom and pan.** The Hand and Zoom tools move the view: a drag pans, a pinch zooms to 16×,
  and the bottom-right readout resets to 100 %. Painting tools keep the finger for paint, as on
  a desktop, where the view moves with the space bar.
- **Colours.** The top-left swatches (and the Colour panel) set the foreground and background
  the painting tools and Fill use; swap is one tap, and the swatches reach the engine through
  `tools.setColors`.
- **Undo and redo** sit beside them, enabled by the engine's own history (`status().canUndo`),
  and `Back` closes an open panel before the editor.
- **Curves and Levels** adjust the image for real through `image.adjustments.{curves,levels}`, or
  become editable adjustment layers through `layer.newAdjustmentLayer.{curves,levels}`. One
  History step per slider drag, and Reset is a step of its own.
- **Everything else** — the Adjustments, Filters and Image panels — is the engine's registry:
  every command the engine has, with its own label, menu and disabled reason.

## Exports

The Export panel writes what the engine encodes: PNG, JPEG, WebP, BMP, GIF, TIFF and PSD. FilmCraft
puts flat formats on the timeline (or the photo library, for a photo opened from Home); layered
files (PSD, TIFF) are documents rather than pictures, so they leave through the system save picker
for the user to keep. The standalone app shares every format.

## How FilmCraft hosts it

`apps/filmcraft-android/.../ui/editor/PhotoCraftIntegration.kt`:

- **Home › Edit a photo** (or *+* › Edit a photo): a picture from the photo library. The editor
  opens it; **export** saves the result to `Pictures/FilmCraft` and leaves the editor. The
  temporary project is removed, as a Home drawing's is.
- **Editor › Photo**: the picture under the playhead (the frame is written to the project's media
  folder for the editor to open). **Export** puts the result on the timeline as one image clip.

The host supplies the bytes and takes back the bytes; the editor never writes into the project.
Closing without an export changes nothing.

## Build

`android/app/build.gradle.kts` mirrors ProCraft's: `cargo ndk` builds
`libphotocraft_android.so` for `photocraft.abis` (or FilmCraft's `filmcraft.abis`), and
`uniffiBindgen` generates the Kotlin bindings from the built library. Nothing is checked in.
The release workflow caches `third_party/photocraft`'s `target` and builds the library as part of
`:app:assembleRelease`.
