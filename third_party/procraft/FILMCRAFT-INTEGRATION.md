# ProCraft integration

Vendored from https://github.com/artcraft-android/procraft at
149800ca5e0143816de9c00e12e8319bba34d54e (MIT OR Apache-2.0).
Upstream copyright, licenses, NOTICE and attribution accompany the source.

FilmCraft builds this isolated Cargo workspace and the Android frontend as the
`:procraft` Android library. Both libfilmcraft_android.so and libprocraft_core.so
ship; the drawing screen uses ProCraft's real native canvas, not DrawingView.
The desktop workspace member is omitted. The library manifest has no launcher,
widget, or competing FileProvider. The host supplies the export FileProvider path.

Local modifications: Android application converted to library, host close callback
in CanvasViewModel (before destroying the session), close label in Chrome.
FilmCraft's ProcraftIntegration.kt hosts AppRoot and CompactColors/HeldColor.
Closing a standalone canvas exports PNG to Pictures/FilmCraft before leaving;
closing a project canvas exports/imports a drawing clip into that project.
Errors retain the open session. Native layered autosaves remain in artworks/.
