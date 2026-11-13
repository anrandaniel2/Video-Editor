package io.github.artcraftandroid.filmcraft.ui.editor

import android.app.Application
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.net.Uri
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelStore
import io.github.artcraftandroid.procraft.canvas.CanvasViewModel
import io.github.artcraftandroid.procraft.canvas.TextRenderer
import io.github.artcraftandroid.procraft.ui.*
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** Each embedded canvas/picker owns its model; closing releases threads and autosave. */
@Composable
private fun rememberProcraftModel(): CanvasViewModel {
    val app = LocalContext.current.applicationContext as Application
    val store = remember { ViewModelStore() }
    val model = remember {
        TextRenderer.prepare(app.assets, File(app.filesDir, "fonts"))
        io.github.artcraftandroid.procraft.core.setGpuRecordFolder(app.filesDir.path)
        ViewModelProvider.create(store, ViewModelProvider.AndroidViewModelFactory.getInstance(app))[CanvasViewModel::class]
    }
    DisposableEffect(model) { onDispose { model.hostClose = null; store.clear() } }
    return model
}

/** The actual upstream five-mode picker, including its native hue mapping and palettes. */
@Composable
internal fun ProcraftColorPicker(color: Int, change: (Int) -> Unit) {
    val vm = rememberProcraftModel()
    val onChange by rememberUpdatedState(change)
    val alpha by rememberUpdatedState(color ushr 24)
    val held = remember { HeldColor(color and 0xFFFFFF) { onChange((alpha shl 24) or (it and 0xFFFFFF)) } }
    DisposableEffect(vm) {
        CanvasSpace.set(false) // FilmCraft text, lights and 3D colours are sRGB.
        onDispose { }
    }
    ProCraftTheme {
        CompactColors(vm, held, arrowX = 130.dp)
    }
}

@Composable
internal fun ProcraftDrawingPanel(ed: EditorController, close: () -> Unit) {
    val context = LocalContext.current
    val vm = rememberProcraftModel()
    val scope = rememberCoroutineScope()
    val standalone = remember { ed.standaloneDrawing }
    val blank = remember { ed.blankDrawing }
    val frame = remember { ed.stop(); if (blank) null else ed.picture?.asAndroidBitmap()?.copy(Bitmap.Config.ARGB_8888, false) }
    var saving by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    var initialised by remember { mutableStateOf(false) }
    var openedOnce by remember { mutableStateOf(false) }
    LaunchedEffect(vm.session) { if (vm.session != null) openedOnce = true }
    LaunchedEffect(vm) {
        try {
            if (frame == null) {
                vm.newArtwork(ed.timeline?.width?.toInt() ?: 1080, ed.timeline?.height?.toInt() ?: 1920)
            } else {
                val file = withContext(Dispatchers.IO) {
                    File(context.cacheDir, "procraft-input-${java.util.UUID.randomUUID()}.png").also { out ->
                        out.outputStream().use { check(frame.compress(Bitmap.CompressFormat.PNG, 100, it)) }
                    }
                }
                vm.newFromPicture(Uri.fromFile(file))
                // Import is asynchronous; don't remove its input until the native session is ready.
                while (vm.session == null && vm.message == null) kotlinx.coroutines.delay(25)
                file.delete()
            }
            initialised = true
        } catch (e: kotlinx.coroutines.CancellationException) { throw e }
        catch (e: Exception) { error = e.message ?: "Could not open drawing." }
    }
    DisposableEffect(Unit) { onDispose { frame?.recycle(); ed.blankDrawing = false; CanvasSpace.set(false) } }
    val requestClose: () -> Unit = {
        if (!saving && vm.session != null) {
            saving = true
            scope.launch {
                try {
                    // Apply active transforms before flattening; keep the native layered autosave.
                    vm.endTransform(true)
                    vm.endAdjustment(true)
                    vm.saveForHost()
                    val file = vm.exportImage(false) ?: kotlin.error("Could not export the drawing. Please try again.")
                    val image = withContext(Dispatchers.IO) { BitmapFactory.decodeFile(file.path) ?: kotlin.error("Could not read exported drawing.") }
                    if (standalone) {
                        try { withContext(Dispatchers.IO) { saveDrawingPhoto(context, image) } }
                        finally { image.recycle() }
                        android.widget.Toast.makeText(context, "Saved to Pictures/FilmCraft", android.widget.Toast.LENGTH_LONG).show()
                        ed.closeEditor()
                    } else {
                        ed.importDrawing(image) // Controller takes ownership and returns to the timeline.
                    }
                } catch (e: kotlinx.coroutines.CancellationException) { throw e }
                catch (e: Exception) { error = e.message ?: "Could not save drawing." }
                finally { saving = false }
            }
        } else if (!saving && initialised && vm.session == null) {
            if (standalone) ed.closeEditor() else close()
        }
    }
    SideEffect { vm.hostClose = requestClose }
    Dialog(onDismissRequest = requestClose, properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false)) {
        io.github.artcraftandroid.filmcraft.ui.ImmersiveDialog()
        Box(Modifier.fillMaxSize().background(Color.Black)) {
            if (vm.session != null || (openedOnce && vm.screen == io.github.artcraftandroid.procraft.canvas.Screen.GALLERY)) {
                AppRoot(vm)
                if (vm.session == null) {
                    BackHandler { requestClose() }
                    IconButton(onClick = requestClose, modifier = Modifier.align(androidx.compose.ui.Alignment.BottomEnd).padding(16.dp)) {
                        Icon(PcIcons.Cross, contentDescription = "Return to FilmCraft", tint = Color.White)
                    }
                }
            }
            else Column(Modifier.fillMaxSize().padding(24.dp)) {
                Text(error ?: vm.message ?: "Opening drawing…", color = Color.White)
                TextButton(onClick = { if (standalone) ed.closeEditor() else close() }) { Text("Close") }
            }
            if (saving) {
                Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = .45f))) {
                    // Intercept input while the export snapshot is being saved.
                    BackHandler { }
                    AlertDialog(onDismissRequest = {}, confirmButton = {}, text = { Text("Saving drawing…") })
                }
            }
            error?.let { message ->
                AlertDialog(onDismissRequest = { error = null }, title = { Text("Drawing") }, text = { Text(message) }, confirmButton = { TextButton(onClick = { error = null }) { Text("OK") } })
            }
        }
    }
}
