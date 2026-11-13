# Drawing GPU support

## Scope

The requested target is now GPU drawing support, **not Krita parity**. Unrelated painting features are deferred, not a release acceptance target for this work.

The drawing canvas is hardware-composited by Android HWUI. Native brush/eraser coverage is rasterized with wgpu on a real four-sample RGBA8 attachment and resolved by the GPU. Resolved coverage is read back asynchronously and committed to native float paint tiles; Android bitmaps cache the display. This is not a fully GPU-resident document: tile colour math, history, most image processing and persistence remain CPU-side. Legacy patterned brushes are an explicitly chosen compatibility mode, not silently advertised as GPU procedural brushes.

## Native path

- Default native brush and eraser: pressure, historical stylus samples, stabilization, symmetry, selection, layer masks/clipping and alpha lock.
- Asynchronous coalesced GPU previews and background stroke commits. No synchronous GPU readback in the native onDraw path.
- Persistent float checkpoints, bitmap-generation invalidation for compatibility edits, native-preserving undo/redo and `.fcdraw` save/open.
- Separate drawing device so drawing validation/device loss cannot mark the video export device failed.
- Hardware adapters only: CPU devices and known SwiftShader/llvmpipe/lavapipe adapters are rejected. Renderability, blending, four-sample support and resolve support are checked. Failed device creation tries the next discovered hardware backend.
- Sample-rate shading when supported. Downlevel GLES uses per-fragment procedural opacity on the same genuine four-sample target; this is reported accurately, not called sample-rate shading.
- Pipeline validation is error-scoped. Before success is reported, initialization actually renders a dab, resolves it, reads it back, and checks outside transparency, centre flow and partial edge coverage. Device loss/uncaptured errors/readback failure disable the drawing device and produce an explicit failure. A failed cached device requires restarting the app; Refresh GPU status reads status, not a device reset.
- Drawing → Native GPU brush engine shows hardware name/backend, self-test outcome, sample count, shading mode and whether the attached Android canvas is hardware accelerated.

## Verification in this workspace

Passed:

- `tools/validate-drawing-shaders.mjs` with Naga **30.0.1** (from `naga-wasm@30.2.0` installed outside the checkout).
- Both original WGSL shaders plus the per-fragment specialization parse, validate and emit SPIR-V.
- The per-fragment vertex/fragment entry points translate to GLSL ES 3.00.
- Rust/Kotlin source syntax scans (Kotlin parser-only normalization noted in the migration document).
- `git diff --check`.

Blocked before compilation:

- `cargo test -p filmcraft-gpu --test drawing_shader`: `cargo: command not found`.
- `./gradlew assembleDebug -Pfilmcraft.abis=arm64-v8a --no-daemon`: no Java installation / JAVA_HOME.

No APK was produced, no GPU hardware test was executed here, and nothing was pushed or released. Shader translation is not Rust/Kotlin compilation or evidence of phone driver operation.

## Build/device acceptance still required

1. On a provisioned build host, run the GPU regression suite and Android arm64 debug build. This validates accumulated native code and generated UniFFI Kotlin names as well as drawing changes.
2. On a real phone, open the Native GPU brush controls. Require a non-software adapter and a passing hardware self-test; confirm the attached Android canvas says hardware accelerated.
3. Exercise native brush and eraser with S Pen hover, pressure and fast historical samples. Check selection boundaries, symmetry, masks, clipping, alpha lock, cancellation and two-finger navigation.
4. Verify opaque/soft strokes and flow accumulation; undo/redo and `.fcdraw` roundtrip must preserve strokes. Confirm the UI remains responsive while preview jobs run.
5. Check the unsupported/device-failure path: no silent compatibility switch, no partially committed stroke and no disabling video export's separate GPU device.
