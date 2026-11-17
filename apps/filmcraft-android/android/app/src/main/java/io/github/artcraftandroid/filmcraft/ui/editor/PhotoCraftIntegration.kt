package io.github.artcraftandroid.filmcraft.ui.editor

import android.content.Context
import android.graphics.BitmapFactory
import android.net.Uri
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
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
import io.github.artcraftandroid.photocraft.studio.StudioScreen
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
/**
 * The name to open a picked picture under.
 *
 * A content URI's path segment is frequently not a filename at all, so the file's display name is
 * asked for first and the MIME type is turned into an extension if even that has none. The
 * importer uses the name to tell a DPX from a QOI, so guessing here is what decides whether the
 * file opens.
 */
private fun pictureName(context: android.content.Context, uri: android.net.Uri): String {
    val display = runCatching {
        context.contentResolver.query(uri, null, null, null, null)?.use { c ->
            if (c.moveToFirst()) {
                val i = c.getColumnIndex(android.provider.OpenableColumns.DISPLAY_NAME)
                if (i >= 0) c.getString(i) else null
            } else {
                null
            }
        }
    }.getOrNull()
    val fallback = uri.lastPathSegment ?: "picture"
    val name = display ?: fallback
    if (name.substringAfterLast('.', "").isNotEmpty()) return name
    val ext = when (context.contentResolver.getType(uri)) {
        "image/png" -> "png"
        "image/jpeg" -> "jpg"
        "image/webp" -> "webp"
        "image/gif" -> "gif"
        "image/tiff" -> "tiff"
        "image/bmp" -> "bmp"
        "image/svg+xml" -> "svg"
        "image/jxl" -> "jxl"
        else -> "png"
    }
    return "$name.$ext"
}

/**
 * The MIME types the picture picker accepts, one per format the engine opens.
 *
 * The wide image type covers most of them; the rest are the formats that only have a MIME type if some app
 * on the device claims it, so they are named by extension as well. Together they make the picker
 * offer the same set the importer can read.
 */
private val PictureMimeTypes = arrayOf(
    "image/png", "image/jpeg", "image/webp", "image/gif", "image/tiff", "image/bmp",
    "image/x-icon", "image/x-tga", "image/x-qoi", "image/x-exr", "image/vnd.dpx",
    "image/dpx", "image/x-portable-anymap", "image/svg+xml", "image/jxl", "image/avif",
    "image/x-canon-cr2", "image/x-nikon-nef", "image/x-sony-arw", "image/x-adobe-dng",
)

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

    // Picking a picture from the studio (or its menu) places it on the document it opened.
    // OpenDocument rather than GetContent: it takes a list of types, so the formats with no
    // registered MIME type are still offered, and it grants a readable URI rather than a
    // one-shot content handle.
    val pickPicture = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        scope.launch {
            try {
                withContext(Dispatchers.IO) {
                    val bytes = context.contentResolver.openInputStream(uri)?.use { it.readBytes() }
                        ?: throw IllegalStateException("The picture couldn't be read.")
                    // The name matters: the importer sniffs several formats by their extension,
                    // and a picker's lastPathSegment is often just an opaque id with no suffix.
                    vm.studioEngine.engine.placeBytes(pictureName(context, uri), bytes)
                    vm.studioEngine.refreshAll()
                }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                error = e.message ?: "The picture couldn't be placed."
            }
        }
    }

    val dismiss: () -> Unit = { if (!saving && !done) close() }
    Dialog(onDismissRequest = dismiss, properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false)) {
        ImmersiveDialog()
        PhotoCraftTheme {
            Box(Modifier.fillMaxSize().background(Color.Black)) {
                // The studio is the editor's interface: the reference UI's workspaces over this
                // model's own PhotoCraft session, so the panels are the engine's and the host's
                // save path is unchanged. It waits for the document, as the classic editor does:
                // a store opened from a frame is briefly empty.
                val documentOpen = vm.status?.active != null && !vm.busy
                if (documentOpen) {
                    StudioScreen(
                        engine = vm.studioEngine,
                        initialWidth = ed.timeline?.width?.toInt() ?: 1920,
                        initialHeight = ed.timeline?.height?.toInt() ?: 1080,
                        title = vm.studioEngine.status?.active?.name ?: "Untitled",
                        onClose = dismiss,
                        onExport = { format, bytes, _ -> finish(bytes, format) },
                        // `image/*` is what the picker filters on, but several of the formats the
                        // engine reads (TGA, QOI, DPX) have no registered MIME type on Android, so
                        // the extensions are listed too -- otherwise the file is simply not there
                        // to choose.
                        onPickPicture = {
                            pickPicture.launch(arrayOf("image/*", *PictureMimeTypes))
                        },
                        // The classic editor, for the panels the studio doesn't show.
                        onOpenClassic = { vm.choosePanel(PhotoPanel.Classic) },
                    )
                } else if (error == null) {
                    // Opening: the reference's own backdrop, so the switch is not a blank frame.
                    Box(Modifier.fillMaxSize(), contentAlignment = androidx.compose.ui.Alignment.Center) {
                        Text("Opening the picture…", style = FcType.Body, color = Fc.Body)
                    }
                }
                // The classic editor, for the panels the studio doesn't show. Its own back handler
                // closes it before the studio's does.
                if (vm.panel == PhotoPanel.Classic) {
                    PhotoEditorScreen(vm, onExport = { bytes, format -> finish(bytes, format) })
                    BackHandler { if (!saving) vm.closePanel() }
                }
                // The studio closes itself on back (its overlays first); this is the host's last
                // resort, and a save in flight still wins.
                BackHandler(enabled = !saving && vm.panel != PhotoPanel.Classic) { dismiss() }
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
