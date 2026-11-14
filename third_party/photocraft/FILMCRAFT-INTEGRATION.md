# PhotoCraft integration

Vendored from https://github.com/storytold/photocraft at
`c65e70d87a360724c12a4ed38a87edbea247091b` (v0.23.1, MIT OR Apache-2.0).
Upstream copyright, licenses, NOTICE and attribution accompany the source.

This is the real PhotoCraft — the Photoshop-class image editor from the ArtCraft suite — not a
reimplementation. FilmCraft builds its engine for Android as the `:photocraft` library: the
`photocraft-android-core` crate (`android/core`) is the cdylib the app loads, and
`cargo-ndk` compiles it for every ABI the app ships. `apps/filmcraft-android/android/app` depends
on the module, so the release workflow's `assembleRelease` is what proves it compiles and packages
`libphotocraft_core.so`.

## What is vendored

- `crates/*` — every engine crate: `doc`, `ops`, `compose`, `color`, `geom`, `raster`, `paint`,
  `text`, `vector`, `io`, `format`, `raw`, `gpu`, `algo`, `automation`, `plugins`, `ipp`,
  `tablet`, `testkit`, `engine`.
- `android/core` — new: the Android bridge (UniFFI). It owns an opened document and answers what a
  host screen asks: its size, colour mode, bit depth, the flattened layer stack, and a composited
  preview as straight sRGB RGBA8.
- `android/bindgen` — new: `uniffi-bindgen` for the bridge, the same release as FilmCraft's own
  bindings (`0.32.2`), so the generated Kotlin matches the Kotlin it is compiled against.
- `assets/*` — the four UI fonts `crates/text`'s build script deflates into the library
  (`fonts::BUNDLED`), the spell-check dictionary `spell.rs` includes, the app icons and the icon
  set: the build stops without the first two, and keeping the rest keeps the tree upstream's.
- Upstream `Cargo.toml` (workspace members trimmed), `Cargo.lock`, `clippy.toml`, `rustfmt.toml`,
  `README.md`, `ROADMAP.md`, `AGENTS.md`, licenses, NOTICE and ATTRIBUTION.

## What is not vendored

- `crates/ui-egui` — the desktop (egui) front end. Android hosts its own Compose UI on the same
  engine; no crate in the kept set depends on it.
- `apps/*` (the desktop app, the CLI and the web builds), `xtask`, `docs`, `book`, `packaging`,
  `perf`, `scorecard`, `tests`, `examples`, `contributors`, `scripts`. The `corpus/` test data is
  not vendored: those tests are upstream's real-file suite and are not part of the app build.

The dependency set is untouched: `craft-libs` (codecs, PSD, ICC, file formats), `wgpu` and the rest
are fetched at build time at the revisions upstream pins.

## Local modifications

- Workspace `members`/`default-members` trimmed to `crates/*`, `android/core`, `android/bindgen`;
  the `photocraft-ui-egui` workspace entry and its test-profile override removed with it.
- `uniffi = "=0.32.2"` added to `[workspace.dependencies]` for the two Android crates.
- `android/core`, `android/bindgen`, `android/app` (the Gradle library module) and this file added.

Nothing else changed: the engine crates are upstream's source at that commit.

## The bridge surface (android/core)

```rust
PcSession::open(name: String, bytes: Vec<u8>) -> Result<Arc<PcSession>, PcError>  // any format PhotoCraft reads
session.document()  -> PcDocument   // name, size, colour mode, bits a channel, dpi, layer stack
session.warnings()  -> Vec<String>  // what the import approximated or dropped
session.layer_count() -> u32
session.render(max_side: u32) -> PcPixels   // straight sRGB RGBA8, box-filtered down to max_side
pc_version() -> String
```

`render` composites in bands of the document's own rows and box-filters each band into the asked
size as it arrives, so a 50 MP document previews without a 50 MP float buffer ever existing. That
is what keeps it inside a phone's memory; the pixels arrive in `ARGB_8888` order and go straight
into a bitmap.

The bridge is deliberately thin. Panels, tools, dialogs and the Compose chrome belong to Kotlin;
document changes belong to `photocraft-engine`'s command system, which the bridge will expose as
the toolbars land.

## Build

```
cd third_party/photocraft
cargo ndk -t arm64-v8a --platform 29 build --release -p photocraft-android-core --lib
cargo run -p photocraft-bindgen -- generate <libphotocraft_core.so> --language kotlin --out-dir out
```

`android/app/build.gradle.kts` runs both (`cargoBuild<Variant>`, and `uniffiBindgen<Variant>` once
the bindings step is wired) with the same `-Pfilmcraft.abis` the rest of the app uses. The Gradle
module only packages the library today; the Kotlin bindings are generated but not yet compiled
into a host screen.

## Device checks (once the Kotlin host lands)

- Open a PSD with groups, an adjustment layer, a masked layer and type: the layer panel lists them
  bottom to top with their blend modes and opacity, and the preview matches the desktop app.
- Open a 48 MP JPEG: the preview appears without the process being killed for memory, and the
  longest side is at most the requested size.
- Open a camera raw and a `.pcraft` bundle: both list their layers; warnings are shown, not lost.
- Open a file that is not an image: the error is shown and Home is still usable.
