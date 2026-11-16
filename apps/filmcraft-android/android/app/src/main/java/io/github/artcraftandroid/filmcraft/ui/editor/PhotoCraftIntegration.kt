package io.github.artcraftandroid.filmcraft.ui.editor

import androidx.compose.runtime.Composable

/**
 * The photo editor.
 *
 * It is the drawing canvas — Procraft, the same engine and the same 80-file interface Draw uses —
 * opened on a picture instead of a blank page. That is the point: a photo has every brush, layer,
 * adjustment, selection and filter the drawing app has, and strokes feel the way they feel there,
 * because it is literally the same code. Nothing here re-implements a brush.
 *
 * The host still owns the file system, as it does for Draw: it hands the canvas the picture to
 * open (a frame from under the playhead, or a file picked from Home) and takes back the result —
 * onto the timeline at the playhead, or into the photo library for a photo opened from Home.
 */
@Composable
internal fun PhotoEditorPanel(ed: EditorController, close: () -> Unit) = ProcraftDrawingPanel(ed, close, photo = true)
