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

## Local continuation: gradients and additional filters

Not yet built or device-verified. Release builds/pushes are paused at the user's request until full feature coverage.

- Native two-stop gradient painting: linear, radial, diamond and conical; clamp/repeat/reflect; reverse; transparent end stop; configurable endpoint colour, origin, angle and extent. Interpolation uses premultiplied colour so transparent stops do not introduce hidden-colour fringes. Android applies the result through the active selection and records undo. Processing runs off the UI thread.
- Alpha-weighted pixelize, gamma correction and grow/shrink alpha filters. Alpha morphology changes the alpha channel only, not a smart colour-propagation or inpainting operation.
- Added regression test cases for gradient transparency/opacity/invalid geometry, alpha morphology and pixelize transparency. These cases have not been executed during this no-build continuation.

These additions do not implement Krita gradient resource/preset compatibility, multi-stop gradient editing, interactive on-canvas gradient handles or the full Krita filter collection.

## Local continuation: selection operations

- Native binary selection grow/shrink/border/invert. Grow/shrink use separable sliding windows; outside-canvas samples are unselected. Radius is bounded to 256 canvas pixels.
- Native magic-wand matching replaces the Kotlin flood traversal, with contiguous or all-matching-colour modes and alpha-aware tolerance.
- Add/subtract/intersect/replace selection composition, select-all, selection from active-layer nonzero opacity, and undoable erase-selected-pixels controls.
- Empty boolean-operation results now remain empty selections instead of becoming unrestricted painting.
- Regression cases added for morphology, canvas boundaries and zero/large radii. No builds or test execution during this continuation.

Selection storage and boundary rasterization still use Android Paths/Regions; selection operations do not yet have independent undo history, feathered coverage, vector selection editing, or Krita selection-mask compatibility. This is not a complete selection-engine migration. Magic-wand bridge execution remains synchronous; the new morphology operations run on the background dispatcher.

## Larger local continuation: layers, transforms, filters and canvas tools

Still local-only; no build, push or test execution.

- Layer names, visibility and opacity now participate in display, merged sampling and bitmap export. Duplicate, delete, reorder and merge-down operations have undo/redo stack snapshots; property changes restore their prior bitmap/property state through undo. Undo memory accounting includes stack snapshots (one indivisible snapshot may exceed the nominal 64 MiB history target). The document's existing 16-layer/96 MiB raster budget remains.
- Native Rust merge-down supports translucent RGBA source-over and blend formulas; UI exposes Normal, Multiply, Screen, Overlay, Soft Light and Difference for destructive merge-down. Both layers must be visible; their opacities are baked into the merged pixels. This is not a nondestructive layer blend stack.
- Native exact horizontal/vertical flips, 180-degree rotation and half-canvas wrap/offset. Operations respect selection clipping and pixel undo.
- Native auto-levels, histogram equalization, solarize, temperature, alpha threshold and red/green/blue/alpha channel extraction. Histogram statistics weight pixels by alpha; flat channels remain unchanged.
- Cut selected pixels, reset/fit view, zoom and rotation sliders, mirror-view drawing (inverse touch mapping follows the mirror), display-only checkerboard and configurable grid overlay. View overlays do not enter export.
- Added Rust regression cases for flip involutions, histogram edge cases and translucent two-layer compositing.

Layer storage, metadata and stack-history orchestration still reside in Android; merge calculations reside in Rust. Group layers, clipping/inheritance stacks, adjustment layers, non-destructive masks, persistent layer blending, colour-managed HDR paint devices, feathered selection masks, and a Rust document/history model remain outstanding. No claim of complete Krita parity is made by this inventory.

## Broader local continuation: document persistence, resources, adjustments and tools

All changes below are local source changes. No release build, push, test execution or device validation was performed.

### Native layered document persistence

- Versioned `.fcdraw` binary encoder/decoder in Rust, with dimension, aggregate pixel-memory, layer-count, active-layer, UTF-8, opacity, truncation and trailing-data validation. Raw RGBA preserves layer pixels without a lossy flatten.
- Android's storage picker saves/opens documents. Opening requires replacement confirmation; decoding and image allocation occur before replacing the live stack. Canvas dimensions and active layer are restored; old undo/clipboard/staging data is released.
- This is **not `.kra` interoperability**. Reference frames, undo/selection state, resource collections, colour profiles, layer groups and masks are not stored. Files are uncompressed and may be large. Existing raster limits apply (4,194,304 pixels/canvas, 16 layers, 96 MiB total layer pixels).

### Reusable resources

- Rust GIMP `.gpl` palette import/export; persistent palette UI with named swatches, save-current-colour, apply/remove, and explicit replacement confirmation on import. Up to 256 colours.
- Rust-validated `.fcbrushes` resource format for current brush identity, size, opacity, stabilization, pressure response/toggle and symmetry. Named presets persist between sessions and can be imported/exported, applied or removed. Up to 128 presets. Unknown brush identities are rejected on application.
- These are not Krita `.kpp` brush engines or `.kpl` palette archives. UI persistence uses Android preferences; resource parsing/validation uses Rust.

### Colour adjustments and curves

- Combined native exposure, brightness, contrast, saturation, hue rotation, gamma, input black/white points and per-channel gains. Settings apply together as one pixel undo step, constrained to the active selection.
- Native piecewise-linear curve sampling and lookup application for RGB master, individual R/G/B and alpha. UI has five editable anchors, graph, Linear/S-curve/Faded/Invert shortcuts.
- Native work runs off the UI thread. Controls show a curve graph, not a live preview of the adjusted image. Operations are currently destructive raster operations rather than non-destructive filter masks. Hue adjustment uses display-encoded YIQ, not ICC-managed colour transforms.

### Additional selection and colour tools

- Native rectangle and ellipse selection rasterization with canvas clipping, reverse-drag support, add/subtract/intersect/replace integration and display-only drag outlines.
- Native alpha-weighted colour sampling; touch/drag picker with radius 0–32, current-layer or merged-visible sampling. Cached sampling image is released at gesture completion/cancel/disposal. Picker patch transfer is bounded to 65×65 pixels instead of transferring a full canvas on each motion.
- RGB colour sliders, hex display and start/end colour swap. Picker updates the foreground-colour UI directly.

Regression source cases cover document roundtrip/truncation/dimension rejection, palette and preset roundtrips, channel curve isolation, geometric selection and transparent sampling. They have not been run in this no-build phase.

## Local continuation: document geometry, advanced filtering and selection history

- Rust document-wide canvas resize, premultiplied-alpha bilinear image scaling, exact clockwise/counterclockwise 90-degree rotation, crop and visible-alpha trim bounds. Metadata and active-layer index survive transforms. Resize permits centering or top-left anchoring; crop is the selection's rectangular bounds, not an irregular mask.
- Geometry controls validate the combined canvas/layer raster budget. New bitmaps are prepared before replacing the live document. Undo/redo stack entries now include width and height, so geometry undo restores the previous canvas size as well as pixels/layer metadata. Geometry changes clear dimension-dependent clipboard, selection, staging and coverage caches.
- Native advanced-filter panel: Gaussian blur, unsharp mask, high pass, motion blur with angle, median 3x3, Sobel edges, mono/RGB/Gaussian noise, pixel spread and vignette. Radius/strength/angle and reproducible noise seed controls are exposed as applicable. Gaussian/motion filtering uses premultiplied colours; zero-strength operations preserve all input bytes, including hidden transparent RGB. Native calculations run on the background dispatcher; bitmap installation still uses the Android document frontend.
- Explicit selection edits (including deselect, morphology, geometric/lasso/wand combination, select-all and layer-opacity selection) now participate in the shared undo/redo timeline. Internal layer switching/undo invalidation does not create recursive history entries. History is capped at 128 entries in addition to its bitmap-memory target; Path allocation sizes are not measured by that bitmap target.
- Regression source cases added for quarter-turn roundtrip, transparency-safe scaling/blur, visible-alpha bounds, deterministic noise and zero-strength filters. Tests have not been executed.

These remain destructive raster/document operations. There are no non-destructive filter masks, native HDR tiles, ICC-managed filter pipeline, complete Krita filter-plugin set, or full KRA document support yet. Layer storage and history orchestration remain Android-side. No build or push was triggered in this continuation.

## Local continuation: non-destructive layer controls and native compositing

- Hardware-display layer blend choices now include Normal, Darken, Multiply, Colour Burn, Lighten, Screen, Colour Dodge, Overlay, Soft Light, Hard Light, Difference, Exclusion, Hue, Saturation, Colour and Luminosity. They apply to isolated layer contents on Android API 29+; export/merged sampling use the same layer composition path. This is a subset of Krita's full blend-mode catalogue.
- Non-destructive immediate-below clipping uses the lower paint layer's **raw alpha, visibility and opacity**. This is not clipping-group base-layer inheritance. A clipped bottom layer is empty; clipped chains are not Krita group masks.
- Paint-lock controls block painting, pixel filters, destructive selection erasure, individual layer transforms and layer deletion/reordering; document-wide transformations remain document operations. Lock flags, alpha lock, clipping and blend mode participate in layer-property undo/redo.
- Alpha lock isolates live brush strokes with source-atop composition and uses native pre-edit alpha constraints at raster commit. Erasing cannot remove locked alpha. Legacy smudge/blur/shape previews are still not a fully native GPU brush engine; final raster constraints may require synchronous full-layer bridge transfers. No performance claim is made.
- `.fcdraw` version 2 stores blend mode and clipping/lock flags; version 1 remains readable with Normal/unlocked defaults. The hardware frontend rejects document blend modes it cannot display rather than silently substituting Normal.
- Native Rust full-stack compositing/flattening supports the engine's 26 deterministic blend formulas, visibility/opacity and immediate-below clipping. Flatten asks for confirmation and replaces all paint layers with the visible result (hidden layers discarded, reference frame excluded); stack undo restores the original. Trim bounds now use this visible composite, accounting for clipping.
- Pairwise destructive merge is restricted to visible, unlocked, unclipped Normal layers to avoid falsely claiming that arbitrary backdrop-dependent stack interactions can always be merged into a pair. Full-stack flatten is available for the other supported interactions.
- Regression source cases added for old/new document formats, lock flags, multiply/clipping, hidden clip bases, alpha growth/erasure and recolouring. They have not been executed. No builds or pushes were triggered.

Full Krita parity is still absent: group/vector/filter/file/clone layers; non-destructive mask stacks; complete brush engines/resource import; native tiled HDR/ICC-managed painting; animation/onion-skin timeline; vector/text document editing; assistants; KRA/plugin interoperability; and other systems still require implementation. Android still owns layer storage/history and many raster tools. This inventory must not be represented as completion of that target.

## Local continuation: grayscale transparency masks and mask painting

- One independent grayscale transparency mask per raster paint layer: create reveal-all/from selection, enable/disable without changing pixels, invert, threshold, reveal/hide all, feather, remove without altering pixels, and bake into layer alpha. Mask edits/baking have undo/redo entries. This is not a full Krita stack of transparency/filter/transform/colourize masks.
- Reveal/hide mask painting uses the Rust stroke coverage backend, including actual GPU 4x MSAA where available and the existing explicit fallback. Size, opacity, pressure curve and stabilization are connected. Strokes commit on lift; a display-only stroke guide is shown during input. Selection coverage limits painting. Excessively long/complex strokes are rejected rather than partially committed. Symmetry/patterned brush families are not yet connected to mask painting.
- Display/export/sample compositing applies enabled masks with destination-in composition. Immediate-below clipping additionally respects the lower layer's mask. Native Rust compositing and alpha baking implement the same mask semantics. Disabled masks remain saved but do not affect composition.
- `.fcdraw` version 3 saves grayscale mask bytes and enabled state; version 1/2 readers remain supported. Mask lengths and aggregate raster+mask budgets are checked. Masks transform with layer geometry, using the same crop/canvas/quarter-turn mapping and scaling interpolation as paint. Canvas extension fills the mask with black outside its old bounds.
- Immutable mask byte buffers accompany copied layer properties through undo; an 8 MiB alpha-bitmap cache serves display masks. Bitmap-history budgeting now includes mask bytes conservatively; layer add/duplicate and native geometry/serialization account for mask storage. Native processing/geometry temporarily allocates additional buffers beyond the stored-document budget.
- Regression source cases added for mask feathering/inversion, document roundtrip/composition, disabled masks, mask rotation and invalid mask sizes. They have not been executed. No build or push was triggered.

Remaining limitations include direct on-mask thumbnail editing, multiple/nested masks, filter/transform masks, feathered selection storage, grouped alpha inheritance, complete Rust document/history ownership and full Krita compatibility. This update does not establish full parity or device correctness.

## Local continuation: clone stamp and tonal retouch brushes

- Native clone stamping samples an immutable pre-stroke source, with premultiplied bilinear interpolation, source-over alpha and out-of-bounds source skipping. Source selection has a display-only crosshair; aligned mode keeps the source/destination offset between strokes, while unaligned mode resets at each stroke. Sampling can use active-layer raw pixels or the visible merged image including a reference frame.
- Native Dodge, Burn, Desaturate and Colorize strokes. Tonal operations preserve destination alpha; Colorize uses the engine's colour blend formula. All-tones/shadows/midtones/highlights controls weight the applied amount by luminance. These are display-encoded tonal tools, not HDR exposure/healing/content-aware tools.
- Shared Rust stroke coverage supplies real hardware-MSAA coverage where available, with the existing explicit fallback. Brush size, opacity, stylus size pressure, stabilization, selection boundaries, paint locks, alpha locks and pixel undo are connected. Selection coverage is applied once by the native operation, not multiplied by a second frontend clip.
- Retouch strokes commit on lift and show a display-only guide while input is active. Patterned brush tips, symmetry and a full live GPU retouch pipeline are not implemented. Native pixel processing runs on the background dispatcher; bridge snapshot/install work still uses the Android raster frontend.
- Source/offset state clears when document geometry changes. Pending retouch work uses the drawing View's lifecycle-cancelled scope. Long/complex strokes fail rather than being partially committed.
- Regression source cases added for immutable clone sampling, zero-selection rejection and alpha preservation. Tests have not been executed. A static integration issue in the mask-history alpha-constraint parameter was also corrected. No build or push triggered.

## Local continuation: painting assistants

- Native Rust sample constraints for ruler through configured origin, parallel ruler through stroke start, rotated grid intersections, radial line through stroke start, and concentric circle through stroke start.
- Controls for origin (sliders or canvas tap), angle, spacing, snap strength and display-only guides. Guides use bounded line/ring counts and are excluded from exports. Settings are session-local, not serialized assistant resources.
- Connected to pen/eraser input, historical samples, smudge/blur, mask painting and native retouch input. Native snapping is applied after stabilization where stroke samples are stabilized; pen-up endpoints are snapped too. Selections, fill, shapes and transforms are intentionally not constrained. Clone offsets account for the snapped destination anchor.
- Hover footprints follow native snapping. Parallel/radial/circle assistants have an active first-contact anchor; pre-contact hover cannot predict a future stroke anchor. Paths approximate constrained samples with the current frontend curve interpolation: this is not an exact analytic vector-circle drawing system. Grid snapping can visibly step between intersections.
- Regression source cases cover ruler/parallel projection, grid intersections, circle radius, radial partial strength, pressure preservation and invalid settings. Tests have not been executed. No builds or pushes triggered.

This is a limited assistant collection, not Krita assistant parity. Perspective grids, spline/fisheye/vanishing-point editors, multiple simultaneous assistants, assistant resources and persistent document assistant objects remain outstanding. The raster/document engine migration and other missing Krita systems remain incomplete.

## Larger local continuation: multi-stop gradients, resources, patterns, selection and transforms

No builds, pushes or test execution in this pass.

### Multi-stop gradients and reusable resources

- Replaced the simple-gradient UI with up to 32 ordered stops: edit stop position/alpha, capture foreground colour, add into the largest gap, remove stops, and display a colour-strip preview.
- Seven native gradient shapes: linear, radial, diamond, conical, square, bilinear/mirrored-linear and spiral. Clamp/repeat/reflect, reverse, Linear/Smoothstep/Cosine/Constant interpolation, premultiplied transparent-stop handling and optional linear-light sRGB interpolation are connected.
- Native validated `.fcgradients` collections: persistent named presets, apply/remove, storage-picker import/export and replacement confirmation. Up to 64 presets/collection. Saved presets contain shape/stop/interpolation settings; angle/origin/length remain per-application UI geometry. This is not Krita resource compatibility or ICC colour management.
- The UI colour strip is an encoded-colour preview and does not render every interpolation/shape choice; the canvas result is produced by native raster evaluation. These remain destructive fills with selection/alpha-lock/undo integration, not gradient layers or a live interactive on-canvas gradient editor.

### Procedural fill patterns and bucket expansion

- Eight native procedural patterns: checker, stripes, dots, grid, brick, crosshatch, triangles and deterministic random tiles. Cell size, rotation, density, captured background colour/transparency, opacity and random seed are connected. Shapes are sampled at pixel centres, not claimed as GPU texture brushes or vector-pattern resources.
- Native bucket fill now accepts an independent reference image, contiguous or all-matching-colour regions, square-radius growth and grayscale selection restriction. Merged sampling includes visible layer effects and the reference frame; painting targets only the active layer. Region matching/growth and source-over pixel evaluation run off the UI thread. Growth is not gap closing or smart anti-leak filling.

### Polygon selection and expanded affine transform

- Native even-odd polygon scan conversion, up to 128 vertices; tap-to-add draft, vertex undo, cancel and explicit Apply. Existing add/subtract/intersect/replace and selection undo/redo apply to committed polygons. Draft handles/outlines are display-only. Multi-touch navigation removes the uncommitted contact vertex rather than accidentally extending a polygon.
- Normal layer/selection transforms now use a native validated affine matrix: linked or independent X/Y scale, rotation, X/Y skew, flips and movable pivot, plus existing drag translation. Display bounds follow the same transformed corner matrix. Singular/out-of-range settings leave the last valid preview intact and disable Apply until corrected. Controls reset independently of drag translation.
- The engine calculates matrices; Android still rasterizes transformed bitmaps. Perspective and mesh continue to use existing frontend rasterization. This is not a completed Rust transform engine.

Regression source cases added for gradient transparency/resources, deterministic patterns, independent-reference/global bucket fill, polygon winding and affine pivot invariance. Full Krita parity and device correctness remain unestablished.

## Local continuation: native GPU dab brush engine

This pass changes brush execution rather than merely adding menu entries. It is **unbuilt and device-unverified**; no release build or push was triggered.

### Rust scheduling and GPU rasterization

- Native distance-based dab scheduling across a stroke, pressure-derived radius interpolation, spacing, hardness, elliptical aspect, tip rotation, per-dab flow, scatter, size jitter, rotation jitter, direction following, seeded layout and symmetry replication. Bounds reject more than 8192 samples/16384 replicated dabs rather than silently truncating a stroke.
- Added a dedicated wgpu brush pipeline and WGSL shader. The attachment has four hardware samples and a GPU resolve. UV interpolation is explicitly per sample, and the fragment shader uses `sample_index`; analytic tip clipping/softness is therefore evaluated at the hardware sample positions, not represented as CPU supersampling.
- Eight procedural shader tips: round, square, diamond, five-lobed star, ring, bristles, chalk and spray. Dab alpha accumulates through fixed-function blending. These are original procedural tips, **not Krita brush-engine/preset compatibility**, and do not replace all 150 compatibility presets.
- GPU readback targets use the existing bounded/reused painter resources; quantized 64-pixel tile bounds reduce reallocations as a stroke grows. Device maximum texture dimensions are checked before target allocation.

### Canvas and resource integration

- Explicit native-engine mode with live GPU coverage preview and background native pixel commit. Size, opacity, stylus size pressure, stabilization, assistants, selection, symmetry, layer blend/mask/clipping, paint locks, alpha locks and undo are connected. S Pen eraser-tool input removes alpha instead of painting foreground colour. Native errors do not silently substitute Android patterned brushes; no pixels are committed on failed native painting.
- Native flow is accumulated per dab; stroke opacity is applied after that accumulation. Preview caches the last coverage tile across unchanged backdrop captures. It still performs **synchronous GPU readback on the UI thread** when input changes; layers/pixel installation still use Android bitmaps. This is not a zero-copy/full-GPU document engine, and no runtime performance claim is made.
- Controls include native-tip/dynamics settings, starting recipes (ink, soft airbrush, flat marker, chalk, spray), persistent named settings and `.fcnativebrush` import/export with Rust validation. Size/overall opacity remain the common drawing controls; the native resource records dab settings and symmetry. Selecting a compatibility brush/preset disables native mode explicitly.
- Hover footprint uses native size/aspect/rotation, square/diamond outlines where applicable and snapped screen position. Other procedural tips use a bounding ellipse. Jitter/scatter/direction-following may extend/change the actual footprint relative to that static hover outline.

### Regression source cases

Added scheduler repeatability/spacing, symmetry replication, resource roundtrip, dense-stroke bounds/invalid inputs, WGSL parsing/validation and optional available-adapter flow/bounds cases. **None have been executed in this no-build phase.** Source changes do not establish successful shader compilation or actual phone GPU execution.

Still missing are Krita's complete brush engines/resources, paint-device tiling/HDR/colour management, layer groups/types, animation, plugin/file compatibility and other parity systems. The native dab engine is a substantial implemented subsystem, not completion of full Krita parity.

## Local continuation: native flipbook animation subsystem

No build, push, test execution or phone verification was performed. This implements an initial bounded flipbook, not the whole Krita animation workflow.

- Native thread-safe UniFFI animation object owns frame documents, active frame, names, exposure holds, frame rate, structural history and exposure-to-frame timing. Duplicate frames share immutable document snapshots until edited. Unique live-frame pixels/masks are bounded to 96 MiB, 120 frames and 32 structural undo entries with a 192 MiB unique-document history target.
- Connected add-blank, duplicate, delete, reorder, frame selection, naming, 1–120 tick exposure holds, 1–60 fps, frame undo/redo and explicit start-new confirmation. Saving/switching/playing checkpoints the current Android canvas into the native frame model. Pixel undo remains local to the current Android frame; switching frames clears that local stack. Native frame undo restores checkpoints/structural states rather than every Android pointer event.
- Exposure-aware loop/nonloop playback renders the native visible composite on demand into a display-only preview. Canvas editing is blocked during playback. Rendering may miss requested timings on large documents; frame delivery/performance has not been measured.
- Previous-frame red/next-frame green onion skins with adjustable opacity, display-only and excluded from image exports/frame pixels. Adjacent composites refresh when frame state changes or onion mode is toggled.
- Native `.fcanim` encode/decode preserves complete layered frame documents and timing, with equal-dimension, UTF-8, count, exposure, payload, pixel-budget, truncation and trailing-data checks. Storage-picker save/open includes replacement confirmation. Files are uncompressed; serialized duplicates can exceed the 97 MiB file cap even if live snapshots share memory, in which case saving fails explicitly.
- Opening a normal `.fcdraw` clears the attached flipbook session. Resizing a frame canvas requires explicitly starting a new flipbook; cross-frame geometry conversion is not implemented. Reference frames remain view-only, outside the native frame payload.
- Regression source cases cover exposure mapping/looping, structural undo/redo, document roundtrip/truncation, immutable duplicate editing and equal-dimension enforcement. Not executed.

Still absent: per-layer keyframe channels/holds, interpolation/tweening, audio synchronization, timeline scrubbing UI with thumbnails, animation curves, animation video/GIF/APNG export, KRA animation interoperability, complete onion-skin configuration and persistent high-bit-depth frame tiles. The requested HDR/ICC, vector/text, group/layer/resource and other major parity systems are still incomplete; this pass does not mark them done.

## Local continuation: sparse high-bit-depth document core

Added `drawing_tiles.rs`, a standalone exported native API, **not yet the Android editor's backing store**. No build/test execution or push.

- Sparse 64×64 float32 premultiplied scene-linear sRGB-primary RGBA tiles; empty canvas allocates no pixel tiles. Dimensions up to 32768 per axis, 256 paint layers. Native HDR channel values above 1 are retained (bounded to 65504 straight RGB), not quantized to Android bytes.
- Immutable shared tiles and copy-on-write region edits; duplicate layers share pixels until changed. Atomic bounded rectangular reads/writes; fully zero tiles reclaimed. Live unique tile storage capped at 192 MiB; structural/tile history retains at most 64 entries and evicts oldest entries to meet the same unique-tile budget. Temporary edit buffers and metadata are additional memory; this is not a process-RSS cap.
- Native layer create/duplicate/delete/reorder/select/name/visibility/opacity and undo/redo. Normal source-over compositing operates on linear premultiplied float values.
- Explicit SDR viewport conversion supports exposure stops and clipping or simple component-wise Reinhard preview. This is not an ICC display transform, HDR display surface or perceptual tone mapping.
- Lossless `.fctile` binary encode/decode preserves float bits and sparse layers; checks identifiers, dimensions, canonical nonempty tiles, edge padding, finite premultiplied pixel values, names, counts, lengths, budgets and trailing data. Shared tiles are serialized independently; saving can reject a document whose unique in-memory tiles fit but whose serialized duplicates exceed the budget.
- Explicit 8-bit layered import converts sRGB to linear floats. Unsupported blends, clipping, masks and locks are rejected rather than silently lost. Import does not alter the source document.
- Unexecuted regression definitions cover sparse edge tiles, HDR values, duplicate copy-on-write, linear display conversion, invalid floats, persistence and truncated files.

**Not complete:** Kotlin still renders/edits Android bitmap documents. GPU brush output, selections, masks, filters, animation frames, storage pickers and the layer UI must be migrated to this core before it supplies high-bit-depth editing to users. Native tile persistence currently has no Kotlin picker/menu. Group trees, adjustment/filter layers, vector/text layers, complete mask stacks, ICC profiles, HDR display/export, KRA/KPP/KPL compatibility and remaining brush engines are still missing. This change is architectural groundwork, not completion of those major features.

## Local continuation: connected OpenRaster interchange

Implemented native `drawing_openraster.rs` and Android `DrawingOpenRasterPanel`, accessible from Document → OpenRaster layered interchange. This is `.ora` support, **not `.kra` compatibility or full Krita parity**. No build/test execution, release or push.

- Rust handles ZIP/XML/PNG parsing and encoding. Android supplies storage-picker permissions, replacement confirmation, background dispatch and controls. Successful import replaces the canvas and detaches its flipbook; decode failures leave the current canvas intact.
- Export writes the uncompressed first `mimetype` member, UTF-8 `stack.xml`, one lossless RGBA PNG per layer, merged composite and a bounded thumbnail. Paint order, active layer, names, visibility, opacity and supported SVG blend IDs are mapped explicitly.
- Import supports stored/deflated archives, byte RGB/RGBA/grayscale PNGs, transparency expansion and signed layer offsets clipped into the canvas. Byte-editor limits remain 16 layers, 4,194,304 pixels and 96 MiB layer pixels. Stack XML is limited to 64 KiB; file/member/PNG allocations have independent bounds, duplicate ZIP entries and unsafe layer paths are rejected. Files are never extracted to disk; XML DTD/entity loading is disabled by parser defaults.
- Nested groups, unsupported blends, unknown layer metadata, masks/clipping/locks and 16-bit/APNG layers fail explicitly. Export requires a flat unmasked/unclipped/unlocked supported paint stack; use `.fcdraw` to preserve existing mask/lock metadata. PNG colour profiles are not transformed; this importer is not colour-managed or suitable for colour-critical ICC conversion.
- Added unexecuted regression definitions for layer/pixel/name/blend roundtrip, negative offsets, malformed archives, rejected masks/groups/traversal/unknown semantics.
- Added pinned native archive/XML dependencies and existing PNG version. Cargo.lock was updated from package registry metadata without executing Cargo (Cargo is not installed in this workspace). Dependency resolution and generated Kotlin function names remain build-unverified.

Major unresolved work is unchanged outside interchange: migrate the editor/brush/mask/filter/animation paths to native tiled pixels; full layer tree/type/mask stacks; complete native brush engines and resource compatibility; ICC/HDR display/export; vector/text editing; per-layer animation keyframes and animated export; KRA compatibility; performance/device/roundtrip validation. This addition does not mark those items finished.

## Local continuation: APNG export and native tile painting

- Connected Animation → Export animated PNG through SAF. Native APNG export composites each full layered frame, preserves exposure/fps as rational frame delays, uses source replacement so transparent frames clear previous pixels, and writes either infinite-loop or single-play metadata. Compositing/compression runs off the UI thread after capturing immutable frame snapshots. Output is bounded to 96 MiB by the writer itself. This is 8-bit APNG, not video/GIF export, tweening or per-layer keyframe animation.
- Added native tiled `paint_coverage` and `paint_gpu_stroke`: real GPU brush coverage can now commit directly to sparse float tiles without an 8-bit pixel roundtrip. Straight scene-linear HDR colour, opacity, coverage-local selection, erase and alpha-lock are implemented in one tile-history transaction. Fully cleared tiles are reclaimed; zero-effect coverage avoids adding history. GPU failure returns an error before document mutation; no CPU coverage fallback was added.
- Native brush coverage now accepts sparse-document extents up to 32768 per axis while bounding each actual coverage rectangle to 4,194,304 pixels. Large sparse extents do not allocate a full-canvas coverage image. Symmetry can widen the bounding rectangle enough to exceed that limit; such strokes fail explicitly.
- Added regression source cases for HDR coverage, selection exclusion, erase/reclamation/undo, alpha lock, APNG frame counts/play counts/exposure holds and source-alpha replacement. These were not executed.
- Installed external syntax parsers in an ignored virtual environment (not an application build). Tree-sitter found no syntax errors in native drawing Rust modules. Kotlin parser output was inconclusive: it reports errors around soft keywords and consecutive catch clauses in multiple files, including earlier code. This does not establish Kotlin correctness. No compilation, runtime/device tests, release or push occurred.

Android's normal drawing canvas still uses bitmap storage. The new tile painting API is not yet its default stroke backend, and no claim of end-to-end HDR editor migration is made. Remaining full-parity gaps listed above remain unresolved.

## Local continuation: connected native precision painting and persistence

This section supersedes the earlier statement that tile painting is completely disconnected. **It is not a complete all-tools/document GPU migration.** No application build, device test or release has run.

### Connected drawing path

- The native GPU brush is now the initial drawing mode. Committed strokes fork an existing immutable scene-linear tile checkpoint, or initialize one from that layer's byte pixels, apply actual GPU coverage directly to float tiles, project an SDR bitmap for HWUI and attach the native checkpoint to that bitmap. No float-to-byte-to-float roundtrip occurs between consecutive native strokes.
- Alpha lock, erase, opacity and grayscale selection are applied in native float tile math; display masks/clipping/opacity remain existing layer metadata. An edit failure before installation leaves current pixels/checkpoint unchanged. GPU failures are explicit, with the compatibility brush available by deliberate selection.
- Bitmap copies used by per-layer undo/redo, whole-stack history and duplicate layer fork native checkpoints with shared immutable tiles. Undo exchanges the actual bitmap/checkpoint pair; stack reordering preserves that association. Native forks have independent editable state and no retained internal history, avoiding two independent stroke undo stacks.
- Bitmap generation IDs detect compatibility pixel mutations; affected native caches are discarded instead of reused as stale authority. The old checkpoint remains on the undo bitmap. Metadata/mask-only edits retain float pixels, including alpha-locked metadata changes. Compatibility shapes/transforms/filters/curves/retouch still rebase affected pixels through ARGB_8888. This precision boundary is disclosed in controls.
- Native live tile pixels across current paint layers are capped at 96 MiB. Android history accounting now includes native checkpoint live bytes conservatively (shared tiles may be counted repeatedly), in addition to byte pixels and masks. The existing minimum-one-history-entry behavior remains, so the 64 MiB history target is not a strict total-RSS cap. Temporary GPU/readback, fork and conversion buffers add memory overhead.

### Connected persistence

- `.fcdraw` version four wraps a validated legacy layered snapshot plus optional lossless native tile payloads. It retains layer masks/locks/blends and float values; old v1/v2/v3 files still open, and byte-only documents continue to encode as v3. The editor's normal Save/Open picker uses this codec.
- Native payloads must match each layer's dimensions, normal single-layer internal configuration and SDR preview. The correspondence check accounts for Android's 8-bit premultiply/unpremultiply cache rounding within one premultiplied code value and requires exact alpha. Mismatched/truncated/oversized payloads are rejected before canvas installation. File limit remains 96 MiB + 8192 bytes; documents whose float tiles exceed serialized budget fail explicitly rather than silently dropping precision.
- Native precision is not yet stored in flipbook frames or OpenRaster exports. Animation and interchange remain explicitly byte-based. Image exports/display remain SDR; float persistence is not ICC/HDR display/export support.

### Responsiveness and float adjustments

- Removed synchronous GPU readback from native brush onDraw. A single asynchronous preview job coalesces samples; stale stroke results are ignored, old preview masks are released, and the newest pending sample set is requested after each result. Commit remains background-dispatched. Preview coverage compositing still uses Android's display-space blending and may differ from the final linear-light commit.
- Native-layer colour adjustments now fork and operate directly on scene-linear floats, preserving alpha and HDR values, including exposure, levels/gamma, brightness/contrast, hue/saturation and channel gains. Selection is blended in float. Byte layers retain the prior adjustment path; curves remain byte-based. Extreme gamma/zero-contrast inputs are bounded before multiplication to avoid infinity-times-zero NaNs.
- Added unexecuted regression definitions for precision file roundtrip, HDR retention, preview mismatch/truncation rejection, independent forks, float exposure/selection/undo and extreme adjustment finiteness.

Static verification: Rust drawing modules parse without syntax errors. Kotlin drawing sources parse after in-memory normalization of parser limitations around the `open` soft keyword and newline-separated catch/finally clauses; source files were not rewritten merely to satisfy that parser. These scans do not validate UniFFI types, Rust borrowing, Kotlin compilation, WGSL semantics or device behavior. `git diff --check` passed.

Remaining parity work still includes complete native layer trees/types/mask stacks, all-tool float migration, colour-managed HDR surfaces/export, remaining brush engines/resources, vector/text, full per-layer animation and KRA compatibility. This implementation connects the native painting/storage/history/save path, not those entire subsystems.

## Local continuation: native curves, point filters and document geometry

Added `drawing_tile_operations.rs`, a private operations submodule of the native tiled document; methods are exported through the same UniFFI object and connected to existing editor controls. No build/test execution, release or push.

- Curves on native-backed layers now interpolate a 256-sample table in straight scene-linear colour without byte quantization, extrapolating the last segment above 1. Master/R/G/B and alpha channels are supported. Alpha curves can create black opaque pixels in transparent regions when the curve maps zero upward; allocation limits still apply. Existing byte layers retain their compatibility curve path.
- Native point/colour filters cover invert, grayscale, sepia, threshold, brightness, contrast, saturation, posterize, gamma, temperature, solarize, red/green/blue channel display, alpha-channel display, alpha threshold, luminance-to-alpha and opaque. HDR values are bounded to the native finite range rather than globally clipped to SDR, except operations whose definitions explicitly constrain the output (for example threshold/invert/alpha). Selection blends premultiplied results; alpha lock preserves destination alpha. These linear-light results need not match byte display-space filters.
- Native per-layer horizontal/vertical flips, 180-degree rotation and half-canvas wrapped offset preserve exact float samples outside selection blending. Selection and alpha-lock behavior match the native float-operation contract.
- Existing full-document Scale/Canvas/Crop/Trim/quarter-turn commands now transform attached native checkpoints alongside byte layer metadata/masks, regenerate SDR display caches from the float result, and attach the transformed checkpoints after atomic document installation. Native scale interpolates premultiplied scene-linear float pixels; mask geometry continues through the existing grayscale path. Undo retains the pre-transform native checkpoints. Trim still derives bounds from visible byte projections, not a new float-alpha bounding scan.
- Float output tiles are sparse, zero output pixels allocate nothing, edge padding stays zero, and geometry/map allocation is preflighted per tile. UI enforces the current-document 96 MiB native live-tile budget before installing a result. Invalid parameters, unknown operations and budget failures leave the prior checkpoint/canvas unchanged.
- Added unexecuted regression cases for HDR curves, selection/alpha lock, exact float remaps, crop/canvas coordinate conventions, premultiplied HDR bilinear interpolation, geometry undo and no-op selection.
- Static syntax scans passed for 14 native drawing Rust files and 15 Kotlin drawing files (Kotlin parser-only soft-keyword/catch normalization as described previously); whitespace validation passed. These are not compilation or runtime verification.

This supersedes earlier statements that all curves and document geometry necessarily rebase native layers to bytes. Advanced spatial filters, fill/gradient/pattern, clone/retouch, compatibility strokes/shapes and free/perspective/mesh transforms still have byte compatibility paths. Full Krita parity and complete colour-managed/vector/layer-type systems remain unfinished.

## GPU-only scope decision and hardening

The user has dropped full Krita parity as the active target and asked to finish GPU support instead. See `drawing-gpu-support.md` for the scoped architecture, executed shader checks and build/device acceptance requirements.

This pass isolates drawing from the export GPU device, rejects known software adapters, checks format MSAA/resolve/blend capability, tries alternate hardware devices after creation failure, specializes procedural shaders for adapters without sample-rate shading, and requires an actual render/resolve/readback probe before reporting support. Native eraser now uses the asynchronous tiled commit path alongside native brushes; historical MotionEvent pen samples are retained. A diagnostic panel exposes actual adapter/backend/self-test status. Failure/readback errors are explicit; texture-limit errors request a shorter stroke rather than misreporting missing GPU support.

Executed shader verification through Naga 30.0.1 WASM: original stroke/dab WGSL and per-fragment variant validate and emit SPIR-V; per-fragment entry points translate to GLES 3.00. Rust GPU test execution was attempted but Cargo is absent. Android debug assembly was attempted but Java/JAVA_HOME is absent. Neither command reached compilation. No push/release. The implementation cannot yet be called device-tested or build-verified.
