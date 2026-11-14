package io.github.artcraftandroid.filmcraft.ui.editor

import android.app.Application
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.net.Uri
import androidx.activity.compose.BackHandler
import androidx.compose.ui.Alignment
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
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** Each embedded canvas/picker owns its model; closing releases threads and autosave. */
@Composable
internal fun rememberProcraftModel(): CanvasViewModel {
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
    ProcraftHostServices(vm)
    val doubleTaps = remember { io.github.artcraftandroid.procraft.canvas.PenDoubleTaps() }
    val standalone = remember { ed.standaloneDrawing }
    SideEffect { vm.hostTransient = !standalone }
    val blank = remember { ed.blankDrawing }
    val frame = remember {
        ed.stop()
        if (blank) null else ed.picture?.asAndroidBitmap()?.let { source ->
            val w = ed.timeline?.width?.toInt() ?: source.width
            val h = ed.timeline?.height?.toInt() ?: source.height
            Bitmap.createScaledBitmap(source, w, h, true).let { scaled ->
                if (scaled === source) source.copy(Bitmap.Config.ARGB_8888, false) else scaled
            }
        }
    }
    var exportMenu by remember { mutableStateOf(false) }
    var exportFormat by remember { mutableStateOf(DrawingFormat.PNG) }
    var exportQuality by remember { mutableIntStateOf(95) }
    var saving by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    var initialised by remember { mutableStateOf(false) }
    var completed by remember { mutableStateOf(false) }
    LaunchedEffect(vm) {
        var input: File? = null
        try {
            if (ed.drawingArtwork != null) {
                vm.openArtwork(requireNotNull(ed.drawingArtwork))
                snapshotFlow { vm.opening }.first { !it }
            } else if (ed.drawingPicture != null) {
                input = withContext(Dispatchers.IO) { io.github.artcraftandroid.filmcraft.engine.MediaImports.preparePicture(context, requireNotNull(ed.drawingPicture)) }
                vm.newFromPicture(Uri.fromFile(requireNotNull(input)))?.join()
            } else if (ed.drawingPreset != null) {
                vm.newPresetArtwork(requireNotNull(ed.drawingPreset))
                snapshotFlow { vm.opening }.first { !it }
            } else if (frame == null) {
                vm.newArtwork(ed.timeline?.width?.toInt() ?: 1080, ed.timeline?.height?.toInt() ?: 1920)?.join()
            } else {
                input = File(context.cacheDir, "procraft-input-${java.util.UUID.randomUUID()}.png")
                val file = requireNotNull(input)
                withContext(Dispatchers.IO) {
                    file.outputStream().use { check(frame.compress(Bitmap.CompressFormat.PNG, 100, it)) }
                }
                vm.newFromPicture(Uri.fromFile(file))?.join()
            }
            // Await the real opening job, rather than polling a non-observable session field.
            check(vm.session != null) { vm.message ?: "The drawing couldn't be opened." }
            if (!standalone) vm.prepareHostOverlay(frame != null)
            if (standalone) vm.artworkFolder()?.let { dir ->
                val (w, h) = vm.canvasSize()
                ed.drawingArtwork = io.github.artcraftandroid.procraft.gallery.Artwork(dir, vm.artworkTitle(), w, h, System.currentTimeMillis())
            }
            initialised = true
        } catch (e: kotlinx.coroutines.CancellationException) {
            throw e
        } catch (e: Exception) {
            error = e.message ?: "Could not open drawing."
            initialised = true
        } finally {
            withContext(kotlinx.coroutines.NonCancellable + Dispatchers.IO) { input?.delete() }
        }
    }
    DisposableEffect(Unit) { onDispose { frame?.recycle(); ed.blankDrawing = false; CanvasSpace.set(false) } }
    val performClose: () -> Unit = {
        if (!saving && !completed && (vm.session != null || initialised)) {
            saving = true
            scope.launch {
                try {
                    vm.saveForHost()
                    if (vm.session != null && standalone && vm.animation?.pages == false && vm.animationFrameCount() > 1) {
                        // Lossless animated bridge, not a pre-encoded video: the editor's actual
                        // renderer/encoder still owns output size, codec, quality and frame rate.
                        val (canvasWidth, canvasHeight) = vm.canvasSize()
                        val frameCount = vm.animationFrameCount()
                        val slots = vm.session?.animationSequence()?.size ?: frameCount
                        check(slots <= 10_000 && canvasWidth.toLong() * canvasHeight * 4L * frameCount <= 768L * 1024 * 1024) {
                            "This animation exceeds the frame preparation limit. Reduce its canvas size or frame count before exporting."
                        }
                        val animated = vm.exportAnimation(CanvasViewModel.Animated.PNG, false, false, false, false, vm.artworkFps())
                            ?: kotlin.error("Could not prepare animation frames.")
                        try { ed.exportDrawingAnimation(animated, vm.artworkFps().toInt()) }
                        finally { withContext(kotlinx.coroutines.NonCancellable + Dispatchers.IO) { animated.delete() } }
                        completed = true
                        return@launch
                    }
                    if (vm.session != null) {
                        val file = vm.imageForHost() ?: kotlin.error("Could not export the drawing. Please try again.")
                        try {
                            val image = withContext(Dispatchers.IO) {
                                BitmapFactory.decodeFile(file.path) ?: kotlin.error("Could not read exported drawing.")
                            }
                            if (standalone) {
                                try {
                                    withContext(Dispatchers.IO) {
                                        val link = vm.artworkFolder()?.let { File(it, ".filmcraft-photo") }
                                        val previous = link?.takeIf { it.isFile }?.readText()?.takeIf { it.isNotBlank() }?.let(Uri::parse)
                                        val uri = saveDrawingPhoto(context, image, previous ?: ed.drawingPhotoDestination, exportFormat, exportQuality)
                                        link?.writeText(uri.toString())
                                    }
                                } finally { image.recycle() }

                            } else {
                                ed.importDrawing(image) // Suspends until timeline insertion succeeds; owns the bitmap.
                            }
                        } finally {
                            withContext(kotlinx.coroutines.NonCancellable + Dispatchers.IO) { file.delete() }
                        }
                    }
                    completed = true
                    if (standalone) ed.closeEditor() else close()
                } catch (e: kotlinx.coroutines.CancellationException) {
                    throw e
                } catch (e: Exception) {
                    error = e.message ?: "Could not save drawing."
                } finally {
                    saving = false
                }
            }
        }
    }
    val requestClose: () -> Unit = {
        if (!saving && !completed) { if (standalone && vm.session != null && (vm.animation?.pages != false || vm.animationFrameCount() <= 1)) exportMenu = true else performClose() }
    }
    SideEffect { vm.hostClose = requestClose }
    Dialog(onDismissRequest = requestClose, properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false)) {
        io.github.artcraftandroid.filmcraft.ui.ImmersiveDialog()
        val dialogView = androidx.compose.ui.platform.LocalView.current
        SideEffect {
            val window = (dialogView.parent as? androidx.compose.ui.window.DialogWindowProvider)?.window
            if (context.resources.configuration.isScreenWideColorGamut) window?.colorMode = android.content.pm.ActivityInfo.COLOR_MODE_WIDE_COLOR_GAMUT
        }
        CompositionLocalProvider(androidx.compose.ui.platform.LocalLayoutDirection provides androidx.compose.ui.unit.LayoutDirection.Ltr) {
            Box(Modifier.fillMaxSize().background(Color.Black).procraftKeys(vm, doubleTaps)) {
                if (vm.session != null) {
                    AppRoot(vm)
                    if (!vm.barsHidden) Column(
                        Modifier.align(Alignment.BottomCenter).navigationBarsPadding().padding(bottom = if (vm.animation != null) 170.dp else 18.dp)
                            .then(rememberFrost(28.dp)).background(Pc.PopoverFrost).padding(10.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                    ) {
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            io.github.artcraftandroid.filmcraft.ui.SelectChip("Draw", !vm.hostShapes, { vm.hostShapes = false; vm.endQuickShape() })
                            io.github.artcraftandroid.filmcraft.ui.SelectChip("Shapes", vm.hostShapes, { vm.hostShapes = true; if (vm.tool != io.github.artcraftandroid.procraft.core.ToolKind.PAINT) vm.toolPressed(io.github.artcraftandroid.procraft.core.ToolKind.PAINT) })
                        }
                        if (vm.hostShapes) {
                            Text("Draw a rough shape; ProCraft snaps it when you lift.", color = Color.White, style = MaterialTheme.typography.bodySmall)
                            Text("Line, arc, curve, circle, ellipse, triangle, rectangle or square.", color = Color.White, style = MaterialTheme.typography.bodySmall)
                            Row {
                                TextButton(onClick = { vm.quickShapeLast(); vm.quickShapeEditing = true }) { Text("Edit last shape") }
                                TextButton(onClick = { vm.endQuickShape() }) { Text("Done") }
                            }
                        }
                    }
                }
                else Column(Modifier.fillMaxSize().padding(24.dp)) {
                    Text(error ?: vm.message ?: "Opening drawing…", color = Color.White)
                    TextButton(onClick = { if (standalone) ed.closeEditor() else close() }) { Text("Close") }
                }
                if (exportMenu) {
                    AlertDialog(onDismissRequest = { exportMenu = false },
                        title = { Text("Export drawing") },
                        modifier = Modifier.then(io.github.artcraftandroid.procraft.ui.rememberFrost(35.dp)),
                        containerColor = io.github.artcraftandroid.procraft.ui.Pc.PopoverFrost,
                        shape = androidx.compose.foundation.shape.RoundedCornerShape(35.dp),
                        text = {
                            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                                Text("Image format")
                                listOf(DrawingFormat.PNG, DrawingFormat.JPEG, DrawingFormat.JXL, DrawingFormat.AVIF).chunked(2).forEach { formats ->
                                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                        formats.forEach { format -> io.github.artcraftandroid.filmcraft.ui.SelectChip(format.label, exportFormat == format, { exportFormat = format }) }
                                    }
                                }
                                if (exportFormat == DrawingFormat.JPEG || exportFormat == DrawingFormat.AVIF) {
                                    Text("Quality · $exportQuality%")
                                    io.github.artcraftandroid.filmcraft.ui.FcSlider((exportQuality - 1) / 99f, { exportQuality = (it * 99 + 1).toInt() }, description = "Image quality")
                                } else Text("Lossless · full quality. PNG and JPEG XL preserve transparency.")
                                if (exportFormat == DrawingFormat.JPEG) Text("JPEG uses a white background for transparent areas.")
                            }
                        },
                        confirmButton = { TextButton(onClick = { exportMenu = false; performClose() }) { Text("Export and close") } },
                        dismissButton = { TextButton(onClick = { exportMenu = false }) { Text("Keep drawing") } },
                    )
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
}
