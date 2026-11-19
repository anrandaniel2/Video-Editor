# ProCraft integration

Vendored from https://github.com/artcraft-android/procraft at
149800ca5e0143816de9c00e12e8319bba34d54e (MIT OR Apache-2.0).
Upstream copyright, licenses, NOTICE and attribution accompany the source.

FilmCraft builds this isolated Cargo workspace and the Android frontend as the
`:procraft` Android library. Both libfilmcraft_android.so and libprocraft_core.so
ship; the drawing screen uses ProCraft's real native canvas, not DrawingView.
The entire upstream project is included: Android, desktop, iOS, plugins, assets,
documentation, packaging and tools. The library manifest has no launcher,
widget, or competing FileProvider. The host supplies the export FileProvider path.

Local modifications: Android application converted to library, host close callback
in CanvasViewModel (before destroying the session), close and gallery buttons in Chrome. The complete upstream gallery remains
reachable from the grid icon beside the host X.
FilmCraft's ProcraftIntegration.kt hosts AppRoot and CompactColors/HeldColor.
Closing a standalone canvas exports PNG to Pictures/FilmCraft before leaving;
closing a project canvas exports/imports a drawing clip into that project.
Errors retain the open session. Native layered autosaves remain in artworks/.


## Save/return regression checks

The release workflow runs ProCraft's Android JVM regression suite against the
embedded library (`:procraft:testReleaseUnitTest`) as well as building the APK.
Hardware/device checks remain separate from JVM/build validation:

- Home > new drawing > draw > X: PNG in Pictures/FilmCraft; returns Home.
- Project > Draw > draw > X: one drawing clip; returns to the same timeline.
- Deny/fail destination write: error shown; open canvas retained for retry.
- Open artwork grid, immediately return to FilmCraft: pending layered saves finish.
- Close during a transform/fill/opacity edit: commit before native save/export.
- Rapid double-tap X: only one save/import operation.
- Import image, cancel opening/close: no leftover procraft-input cache PNG.
- Reopen drawing from the artwork grid: layers and latest stroke retained.

Timeline insertion is awaited by the host rather than launched in a second scope.
On failed insertion the partial project PNG is removed and the native canvas stays
open; successful insertion transfers bitmap ownership to the controller. Export
and input cache files are cleaned up on success, failure, and cancellation.

## Opening state

The embedded host reads `CanvasViewModel.session` before it renders AppRoot.
Unlike upstream's gallery route, that branch may not observe `screen` at all.
`session` must therefore be Compose snapshot state: an ordinary field leaves the
host displaying its loading screen after native initialization completes.
The new-artwork and picture-import entry points also return their opening jobs;
the host awaits those jobs and reports failure rather than polling indefinitely.

Device regression: open a blank Home drawing, a Home imported image, and Draw
inside a project. Each must replace the loading view with an interactive canvas.

## Upstream brush rendering fixes

GPU files `crates/gpu/src/{lib.rs,canvas.rs,shaders/composite.wgsl}` updated verbatim
from upstream baafdf9567 (other sources remain based on the original pinned revision).
These include upstream commits:

- 76c4d8db60: Samsung Xclipse Vulkan stroke blending compiler workaround; restore
  Vulkan-first Android selection instead of routing these GPUs to OpenGL ES where
  grainy brushes, liquify and 3D painting could draw nothing.
- 832a5908a5: pad OpenGL brush texture arrays so their layer count is not a multiple
  of six. The old 58 stock shapes plus 8 spare layers were treated as cube maps,
  causing brush shapes and grains to sample blank.

The custom preview queue/cache/resource changes from 0.0.46 remain reverted.
The upstream `array_textures_on_opengl_are_never_taken_for_cube_maps` regression
is run with the native GPU library unit tests in the release workflow.
