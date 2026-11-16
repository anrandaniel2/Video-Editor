package io.github.artcraftandroid.filmcraft.ui.editor

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import io.github.artcraftandroid.filmcraft.ui.ImmersiveDialog
import io.github.artcraftandroid.filmcraft.ui.editor.photo.PhotoEditorScreen
import io.github.artcraftandroid.filmcraft.ui.editor.photo.PhotoSession
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * The photo editor.
 *
 * Its own editor, on FilmCraft's own Rust photo engine: the picture is a tiled document, the
 * adjustments run native, and the screen is the photo one rather than the drawing canvas. A photo
 * has controls a drawing does not (exposure, contrast, colour, curves); they live here.
 *
 * The host still owns the file system, as it does everywhere else: it hands the editor a decoded
 * picture and takes back flattened pixels to put on the timeline or in the photo library.
 */
@Composable
internal fun PhotoEditorPanel(ed: EditorController, close: () -> Unit) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val standalone = remember { ed.standalonePhoto }
    val picture = remember { ed.drawingPicture }
    var error by remember { mutableStateOf<String?>(null) }
    var saving by remember { mutableStateOf(false) }
    var done by remember { mutableStateOf(false) }
    val session = remember { PhotoSession(scope) }

    // Open the picture: the one the host stashed (a frame under the playhead, or a file from Home),
    // or a blank canvas the timeline's size.
    LaunchedEffect(picture) {
        try {
            val bitmap = withContext(Dispatchers.IO) {
                if (picture != null) {
                    // The host's importer has already normalised the file; decode it once here at
                    // its own size. The engine takes pixels, not files, so this is the boundary.
                    val file = io.github.artcraftandroid.filmcraft.engine.MediaImports.preparePicture(context, picture)
                    BitmapFactory.decodeFile(file.path) ?: error("The picture couldn't be read.")
                } else {
                    val w = ed.timeline?.width?.toInt() ?: 1920
                    val h = ed.timeline?.height?.toInt() ?: 1080
                    Bitmap.createBitmap(w, h, Bitmap.Config.ARGB_8888)
                }
            }
            session.open(bitmap)
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            error = e.message ?: "The photo couldn't be opened."
        } finally {
            ed.forgetStashedPicture()
        }
    }

    /**
     * Put the result where it belongs: on the timeline as an image clip, or — for a photo opened
     * from Home — in the photo library. Both take a bitmap, so the engine's flattened pixels are
     * wrapped in one here rather than in the screen.
     */
    fun finish(bytes: ByteArray, width: Int, height: Int) {
        if (saving || done) return
        saving = true
        scope.launch {
            try {
                val bitmap = withContext(Dispatchers.IO) {
                    val pixels = IntArray(width * height)
                    for (i in pixels.indices) {
                        val at = i * 4
                        val r = bytes[at].toInt() and 0xFF
                        val g = bytes[at + 1].toInt() and 0xFF
                        val b = bytes[at + 2].toInt() and 0xFF
                        val a = bytes[at + 3].toInt() and 0xFF
                        pixels[i] = (a shl 24) or (r shl 16) or (g shl 8) or b
                    }
                    Bitmap.createBitmap(pixels, width, height, Bitmap.Config.ARGB_8888)
                }
                if (standalone) {
                    try {
                        val saved = withContext(Dispatchers.IO) {
                            saveDrawingPhoto(context, bitmap, ed.drawingPhotoDestination, DrawingFormat.PNG, ed.photoQuality)
                        }
                        ed.drawingPhotoDestination = saved
                    } finally {
                        bitmap.recycle()
                    }
                } else {
                    ed.importDrawing(bitmap) // suspends until the timeline takes it; owns the bitmap
                }
                done = true
                if (standalone) ed.closeEditor() else close()
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                error = e.message ?: "The photo couldn't be saved."
            } finally {
                saving = false
            }
        }
    }

    val dismiss: () -> Unit = { if (!saving && !done) close() }
    Dialog(onDismissRequest = dismiss, properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false)) {
        ImmersiveDialog()
        Box(Modifier.fillMaxSize().background(Color.Black)) {
            PhotoEditorScreen(
                session = session,
                title = ed.timeline?.name ?: "Untitled",
                onExport = { session.flatten { bytes, w, h -> finish(bytes, w, h) } },
                onClose = dismiss,
            )
            if (saving) {
                AlertDialog(
                    onDismissRequest = {},
                    containerColor = Color(0xFF282926),
                    confirmButton = {},
                    text = { Text("Saving the photo…", color = Color.White) },
                )
            }
            (error ?: session.status?.takeIf { session.document == null })?.let { message ->
                AlertDialog(
                    onDismissRequest = { error = null },
                    containerColor = Color(0xFF282926),
                    title = { Text("Photo", color = Color.White) },
                    text = { Text(message, color = Color(0xFFB9BEAF)) },
                    confirmButton = { TextButton(onClick = { error = null }) { Text("OK", color = Color(0xFF7AB8FF)) } },
                )
            }
        }
    }
    DisposableEffect(Unit) {
        onDispose {
            // The host's picture and the blank-page flag describe *this* opening; left behind they
            // would open the next one.
            ed.blankDrawing = false
            ed.drawingPicture = null
        }
    }
}
