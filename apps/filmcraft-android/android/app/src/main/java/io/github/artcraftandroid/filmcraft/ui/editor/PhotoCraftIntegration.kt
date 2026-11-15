package io.github.artcraftandroid.filmcraft.ui.editor

import android.content.Context
import android.graphics.BitmapFactory
import android.net.Uri
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.shape.RoundedCornerShape
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
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.viewmodel.CreationExtras
import io.github.artcraftandroid.filmcraft.ui.Fc
import io.github.artcraftandroid.filmcraft.ui.FcType
import io.github.artcraftandroid.filmcraft.ui.ImmersiveDialog
import io.github.artcraftandroid.photocraft.editor.Panel as PhotoPanel
import io.github.artcraftandroid.photocraft.editor.PhotoEditorScreen
import io.github.artcraftandroid.photocraft.editor.PhotoEditorViewModel
import io.github.artcraftandroid.photocraft.ui.PhotoCraftTheme
import java.io.File
import java.util.UUID
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * The photo editor, hosted in FilmCraft. It is PhotoCraft's own Compose editor (the module in
 * third_party/photocraft, included as `:photocraft`) over PhotoCraft's Rust engine: the tools
 * paint, the panels list the engine's own commands, and the canvas is a region render of the
 * document.
 *
 * The host owns the file system, as it does for the drawing canvas. It hands the editor the bytes
 * of the picture to open (or starts a blank canvas the size of the timeline) and takes back the
 * bytes of the export: onto the timeline at the playhead, or — for a photo opened from Home — into
 * the photo library, after which the temporary project goes.
 */
@Composable
internal fun PhotoEditorPanel(ed: EditorController, close: () -> Unit) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val standalone = remember { ed.standalonePhoto }
    // The picture comes from the host: a picked file, or the frame the controller saved off the
    // main thread. Null means a blank canvas.
    val picture = remember { ed.drawingPicture }
    // One model for the panel's lifetime. It lives in its own store rather than the editor's, so
    // closing the panel clears it: the model's engine, and the Rust session behind it, go with it.
    val store = remember { ViewModelStore() }
    val vm = remember { ViewModelProvider.create(store, PhotoEditorFactory)[PhotoEditorViewModel::class] }
    DisposableEffect(store) { onDispose { store.clear() } }
    var error by remember { mutableStateOf<String?>(null) }
    var saving by remember { mutableStateOf(false) }
    var done by remember { mutableStateOf(false) }
    // Layered exports (PSD, TIFF) leave through the save picker: they are the editor's own
    // documents, which neither a video timeline nor the photo library can hold.
    var layeredBytes by remember { mutableStateOf<ByteArray?>(null) }
    val saveLayered = rememberLauncherForActivityResult(DrawingSaveContract()) { uri ->
        val bytes = layeredBytes
        layeredBytes = null
        if (uri == null) {
            // The user backed out of the picker: the editor stays open, unchanged.
            saving = false
            return@rememberLauncherForActivityResult
        }
        scope.launch {
            try {
                withContext(Dispatchers.IO) {
                    context.contentResolver.openOutputStream(uri, "w")?.use { it.write(bytes ?: ByteArray(0)) }
                        ?: throw IllegalStateException("The file couldn't be created.")
                }
                done = true
                close()
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                error = e.message ?: "The file couldn't be saved."
            } finally {
                saving = false
            }
        }
    }
    LaunchedEffect(vm) {
        try {
            if (picture != null) {
                val (name, bytes) = withContext(Dispatchers.IO) { sourceBytes(context, picture) }
                vm.openBytes(name, bytes)
            } else {
                val w = ed.timeline?.width?.toInt() ?: 1920
                val h = ed.timeline?.height?.toInt() ?: 1080
                vm.newDocument(w, h)
            }
            // Creating and opening are coroutines in the model: wait for the document, or the
            // reason there is none.
            vm.awaitDocument()
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            error = e.message ?: "The picture couldn't be opened."
        } finally {
            // The editor has the bytes now: the frame the host stashed for it can go.
            ed.forgetStashedPicture()
        }
    }

    /**
     * Hand the exported bytes where they belong. Flat pictures (PNG, JPEG, WebP, BMP, GIF) go to
     * the timeline as an image clip, or, for a photo opened from Home, to the photo library.
     * Layered files (PSD, TIFF) are the editor's own documents, not pictures a video timeline or
     * the photo library can hold: they go to the file picker, for the user to keep.
     */
    fun finish(bytes: ByteArray, format: String) {
        if (saving || done) return
        saving = true
        scope.launch {
            var output: File? = null
            try {
                if (format != "png" && format != "jpeg" && format != "webp" && format != "bmp" && format != "gif") {
                    layeredBytes = bytes
                    saveLayered.launch("photo.$format" to mimeFor(format))
                    return@launch // the picker's callback finishes the save
                }
                val bitmap = withContext(Dispatchers.IO) {
                    val f = File(context.cacheDir, "photocraft-export-${UUID.randomUUID()}.$format")
                    output = f
                    f.writeBytes(bytes)
                    BitmapFactory.decodeStream(f.inputStream())
                        ?: throw IllegalStateException("The export couldn't be read back.")
                }
                if (standalone) {
                    // Home-opened photos go straight to the library, in the format exported;
                    // the temporary project goes when the editor closes.
                    try {
                        // Saving again updates the same library picture, rather than making
                        // another copy of it.
                        val saved = withContext(Dispatchers.IO) {
                            saveDrawingPhoto(context, bitmap, ed.drawingPhotoDestination, drawingFormat(format), ed.photoQuality)
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
                withContext(Dispatchers.IO) { output?.delete() }
            }
        }
    }

    val dismiss: () -> Unit = { if (!saving && !done) close() }
    Dialog(onDismissRequest = dismiss, properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false)) {
        ImmersiveDialog()
        PhotoCraftTheme {
            Box(Modifier.fillMaxSize().background(Color.Black)) {
                PhotoEditorScreen(vm, onExport = { bytes, format -> finish(bytes, format) })
                // Back closes the editor's open panel first, as every other panel does, and then
                // the editor itself. One handler for both: a second, always-on one would take
                // over from this and leave the floating panels unable to intercept back.
                BackHandler {
                    if (saving) return@BackHandler
                    if (vm.panel != PhotoPanel.None) vm.closePanel() else dismiss()
                }
                if (saving) {
                    Box(Modifier.fillMaxSize().background(Fc.Scrim)) {
                        AlertDialog(
                            onDismissRequest = {},
                            containerColor = Fc.Panel,
                            shape = RoundedCornerShape(24.dp),
                            confirmButton = {},
                            text = { Text("Saving the photo…", style = FcType.Body, color = Fc.Title) },
                        )
                    }
                }
                // The editor's own signalled error (a command the engine refused, a failed export)
                // and the host's (a picture that couldn't be read, a save that failed).
                (error ?: vm.error)?.let { message ->
                    AlertDialog(
                        onDismissRequest = { error = null; vm.dismissError() },
                        containerColor = Fc.Panel,
                        shape = RoundedCornerShape(24.dp),
                        title = { Text("Photo", style = FcType.DialogTitle, color = Fc.Title) },
                        text = { Text(message, style = FcType.Body, color = Fc.Body) },
                        confirmButton = {
                            TextButton(onClick = { error = null; vm.dismissError() }) { Text("OK", color = Fc.Accent) }
                        },
                        dismissButton = if (standalone) null else {
                            {
                                TextButton(onClick = { error = null; vm.dismissError(); close() }) {
                                    Text("Close", color = Fc.Accent)
                                }
                            }
                        },
                    )
                }
            }
        }
    }
}

/** Makes the photo editor's model, which takes nothing: the panel hands it its document. */
private object PhotoEditorFactory : ViewModelProvider.Factory {
    @Suppress("UNCHECKED_CAST")
    override fun <T : ViewModel> create(modelClass: kotlin.reflect.KClass<T>, extras: CreationExtras): T =
        PhotoEditorViewModel() as T
}

/** The format the photo library saves, from the export's format id. */
private fun drawingFormat(format: String): DrawingFormat = when (format.lowercase()) {
    "jpeg", "jpg" -> DrawingFormat.JPEG
    else -> DrawingFormat.PNG // the library's lossless default; the bytes are decoded first anyway
}

/** The MIME type of a layered export handed to the file picker. */
private fun mimeFor(format: String): String = when (format.lowercase()) {
    "psd" -> "image/vnd.adobe.photoshop"
    "tiff", "tif" -> "image/tiff"
    else -> "application/octet-stream"
}

/** A picture's bytes and the name its decoder hint comes from. */
private fun sourceBytes(context: Context, uri: Uri): Pair<String, ByteArray> {
    // A picked photo is a content Uri; the frame the controller stashed is a file Uri, which has
    // no provider to ask for a display name.
    val name = when (uri.scheme) {
        "file" -> uri.lastPathSegment
        else -> context.contentResolver.query(uri, null, null, null, null)?.use { c ->
            val i = c.getColumnIndex(android.provider.OpenableColumns.DISPLAY_NAME)
            if (i >= 0 && c.moveToFirst()) c.getString(i) else null
        }
    }?.takeIf { it.isNotBlank() } ?: "photo.png"
    val bytes = context.contentResolver.openInputStream(uri)?.use { it.readBytes() }
        ?: throw IllegalStateException("The picture couldn't be read.")
    return name to bytes
}
