# Drawing engine: implementation status

This is an implementation inventory, **not a claim of Krita compatibility or complete feature parity**.

## Reference reviewed

KDE/krita on GitHub, particularly `libs/brush`, `libs/image`, `libs/pigment`, `libs/psd`, `plugins/paintops`, `plugins/tools`, `plugins/filters`, `plugins/impex`, and `plugins/assistants`.

Krita is a C++/Qt application. Its sources are not a Rust dependency. Reimplementations must be distinguished from source ports and any copied source must follow its license.

## Implemented in Rust

- Distance-adaptive stroke stabilization and pressure response, exposed through UniFFI.
- Bounded pen/eraser coverage generation.
- Connected flood fill with alpha-aware tolerance and a selection mask.
- Fourteen raster filters: invert, grayscale, sepia, threshold, brightness, contrast, saturation, posterize, blur, sharpen, edge detection, emboss, luminance-to-alpha and make-opaque.
- Destructive selected-colour blending using 26 engine blend formulas. This is not a layer blend-mode stack.
- Canvas-centred horizontal/vertical/both-axis symmetry and 4/8-fold radial transforms, used by the existing brush renderer.
- Undo integration for filter/fill/colour operations; cancellation-aware background execution for filters and colour operations.
- Real GPU 4x MSAA: stroke geometry rendered to a four-sample RGBA8 attachment, resolved by the GPU to a single-sample texture. Standard pen and eraser use this path when available; coverage tiles are read back to the current Android raster layer storage.
- Explicit software fallback if the GPU is unavailable or fails.

## Android integration

- Hardware-accelerated View display/compositing, not a forced software View layer.
- Legacy patterned-brush previews are rendered to a compatibility bitmap to preserve software-only mask/blur behavior; this is not GPU brush rendering.
- S Pen hover cursor, size outline, centre dot, and a visibility control. Cursor is display-only, not part of exports.
- Bounded coverage cache avoids redrawing unchanged stroke segments for each glass backdrop capture.

## Existing features still awaiting Rust migration

Android raster layer storage/compositing, patterned brushes, smudge/blur, selection/wand/lasso operations, clipboard, transform rasterization, shape construction and most undo storage remain Android implementations. They must not be represented as a complete Rust engine.

## Major Krita systems not implemented here

This list is deliberately non-exhaustive: Krita-compatible brush preset/resource loading and brush engines; tiled high-bit-depth paint devices; colour-managed painting and proofing; layer groups, mask stacks and all layer types; all blending modes and filters; painting assistants; vector tools; texture/wraparound painting; animation/onion skins; full `.kra` interoperability and other import/export plugins; scripting/plugin compatibility; and the remaining selection, transformation and painting tools.

A successful APK build verifies compilation and packaging, not functional parity, GPU execution on a specific phone, stylus feel or performance. Those require device-level validation.
