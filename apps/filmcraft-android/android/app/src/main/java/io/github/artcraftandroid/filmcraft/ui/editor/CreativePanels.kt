package io.github.artcraftandroid.filmcraft.ui.editor

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.coroutines.Dispatchers
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Path
import android.view.MotionEvent
import android.view.View
import androidx.compose.runtime.*
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.horizontalScroll
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.compose.foundation.Canvas as ComposeCanvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.Alignment
import androidx.compose.ui.draw.blur
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.animation.core.FastOutSlowInEasing
import io.github.artcraftandroid.filmcraft.ui.*
import java.io.File
import java.net.HttpURLConnection
import java.net.URI

/** Downloads only on explicit request, to project-local storage, with a bounded streaming copy. */
internal fun downloadSound(address: String, dir: File, video: Boolean = false): File {
    var url = URI(URI(address.trim()).toASCIIString()).toURL()
    dir.mkdirs()
    val temporary = File(dir, "${if (video) "video" else "sound"}-${java.util.UUID.randomUUID()}.part")
    try {
        repeat(6) {
            require(url.protocol == "https") { "Use a direct HTTPS audio link." }
            val connection = url.openConnection() as HttpURLConnection
            connection.connectTimeout = 15000
            connection.readTimeout = 30000
            connection.instanceFollowRedirects = false
            connection.setRequestProperty("User-Agent", ONLINE_MEDIA_AGENT)
            connection.setRequestProperty("Accept", "*/*")
            if (url.host == "mixkit.co" || url.host.endsWith(".mixkit.co")) connection.setRequestProperty("Referer", "https://mixkit.co/")
            try {
                val code = connection.responseCode
                if (code in 300..399) {
                    url = java.net.URL(url, connection.getHeaderField("Location") ?: error("Missing redirect location"))
                } else {
                    require(code in 200..299) { "Download failed (HTTP $code). ${if (code == 429) "The server is rate limiting requests; try again shortly." else "The media server refused the request. Try another result."}" }
                    val type = connection.contentType.orEmpty().substringBefore(';').lowercase()
                    val ext = if (video) when {
                        type.contains("webm") -> "webm"
                        type.contains("mp4") -> "mp4"
                        type.contains("quicktime") -> "mov"
                        else -> url.path.substringAfterLast('.', "").lowercase()
                    } else when {
                        type.contains("wav") -> "wav"
                        type.contains("ogg") -> "ogg"
                        type.contains("flac") -> "flac"
                        type.contains("mp4") || type.contains("aac") -> "m4a"
                        type.contains("mpeg") -> "mp3"
                        else -> url.path.substringAfterLast('.', "").lowercase()
                    }
                    require(ext in (if (video) listOf("webm", "mp4", "mov", "m4v") else listOf("wav", "mp3", "ogg", "flac", "m4a", "aac", "opus")) && !type.contains("text/html")) { "The service did not return a supported ${if (video) "video" else "audio"} file." }
                    require(connection.contentLengthLong <= (if (video) 250L else 100L) * 1024 * 1024) { "File exceeds the download size limit." }
                    connection.inputStream.use { input -> temporary.outputStream().use { output ->
                        val buffer = ByteArray(32768)
                        var total = 0L
                        while (true) {
                            val count = input.read(buffer)
                            if (count < 0) break
                            total += count
                            require(total <= (if (video) 250L else 100L) * 1024 * 1024) { "File exceeds the ${if (video) 250 else 100} MB limit." }
                            output.write(buffer, 0, count)
                        }
                        require(total > 0) { "The sound file was empty." }
                    } }
                    val out = File(dir, temporary.nameWithoutExtension + ".$ext")
                    check(temporary.renameTo(out)) { "Could not save sound." }
                    return out
                }
            } finally { connection.disconnect() }
        }
        error("Too many download redirects.")
    } finally { temporary.delete() }
}

@Composable
internal fun ColorPicker(label: String, color: Int, change: (Int) -> Unit) {
    var expanded by remember { mutableStateOf(false) }
    Row(Modifier.fillMaxWidth().padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(label, modifier = Modifier.weight(1f))
        Box(Modifier.size(36.dp).clip(RoundedCornerShape(8.dp)).background(Color(color))
            .border(1.dp, Color.White.copy(alpha = .7f), RoundedCornerShape(8.dp))
            .semantics { contentDescription = label }
            .clickable { expanded = true })
    }
    if (expanded) Dialog(onDismissRequest = { expanded = false }, properties = DialogProperties(decorFitsSystemWindows = false)) {
        ImmersiveDialog()
        Column(Modifier.widthIn(max = 340.dp).editorGlass(24.dp, layer = 4f).padding(20.dp), horizontalAlignment = Alignment.CenterHorizontally) {
            Text(label, color = Color.White)
            Spacer(Modifier.height(12.dp))
            WheelPicker(color, change)
            FcIconButton(LucideIcons.X.glyph(), "Close color picker", { expanded = false })
        }
    }
}

/** Aligned live backdrop, redrawn by the canvas after strokes and camera gestures. */
private class DrawingBackdrop(context: android.content.Context, private val drawing: DrawingView) : View(context) {
    private var buffer: Bitmap? = null
    private val bitmapPaint = Paint(Paint.FILTER_BITMAP_FLAG)
    override fun onAttachedToWindow() { super.onAttachedToWindow(); drawing.backdropObservers.add(this) }
    override fun onDetachedFromWindow() { drawing.backdropObservers.remove(this); buffer?.recycle(); buffer = null; super.onDetachedFromWindow() }
    override fun onDraw(canvas: Canvas) {
        if (width == 0 || height == 0) return
        // Quarter-resolution backdrop keeps the blur inexpensive during two-finger navigation.
        val w = (width / 4).coerceAtLeast(1); val h = (height / 4).coerceAtLeast(1)
        if (buffer?.width != w || buffer?.height != h) { buffer?.recycle(); buffer = Bitmap.createBitmap(w, h, Bitmap.Config.ARGB_8888) }
        val local = IntArray(2); val source = IntArray(2)
        getLocationInWindow(local); drawing.getLocationInWindow(source)
        val target = Canvas(buffer!!)
        target.scale(w.toFloat() / width, h.toFloat() / height)
        target.translate((source[0] - local[0]).toFloat(), (source[1] - local[1]).toFloat())
        drawing.drawScene(target)
        canvas.drawBitmap(buffer!!, null, android.graphics.Rect(0, 0, width, height), bitmapPaint)
    }
}

@Composable
private fun DrawingGlass(view: DrawingView, modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    Box(modifier.clip(RoundedCornerShape(20.dp))) {
        AndroidView(factory = { DrawingBackdrop(it, view) }, modifier = Modifier.matchParentSize().blur(20.dp))
        Column(Modifier.padding(horizontal = 8.dp, vertical = 2.dp), content = content)
    }
}

@Composable
private fun DrawIcon(icon: Glyph, label: String, enabled: Boolean = true, click: () -> Unit) {
    IconButton(onClick = click, enabled = enabled) { GlyphIcon(icon, if (enabled) Color.White else Color.Gray, description = label) }
}

@Composable
private fun WheelPicker(color: Int, change: (Int) -> Unit) {
    val hsv = remember(color) { FloatArray(3).also { android.graphics.Color.colorToHSV(color, it) } }
    val latestColor by rememberUpdatedState(color)
    val latestChange by rememberUpdatedState(change)
    // Radial saturation over a sweep hue wheel; brightness is independent.
    fun pick(point: Offset, width: Int, height: Int) {
        val x = point.x - width / 2f; val y = point.y - height / 2f
        val radius = minOf(width, height) / 2f
        val hue = ((Math.toDegrees(kotlin.math.atan2(y, x).toDouble()) + 360) % 360).toFloat()
        val current = FloatArray(3).also { android.graphics.Color.colorToHSV(latestColor, it) }
        latestChange(android.graphics.Color.HSVToColor(floatArrayOf(hue, (kotlin.math.hypot(x, y) / radius).coerceIn(0f, 1f), current[2])))
    }
    ComposeCanvas(Modifier.size(220.dp).pointerInput(Unit) {
        detectTapGestures { pick(it, size.width, size.height) }
    }.pointerInput(Unit) {
        detectDragGestures { changeEvent, _ -> changeEvent.consume(); pick(changeEvent.position, size.width, size.height) }
    }) {
        val radius = size.minDimension / 2
        drawCircle(Brush.sweepGradient(listOf(Color.Red, Color.Yellow, Color.Green, Color.Cyan, Color.Blue, Color.Magenta, Color.Red)))
        drawCircle(Brush.radialGradient(listOf(Color.White, Color.Transparent), radius = radius))
        drawCircle(Color.Black.copy(alpha = 1 - hsv[2]))
        val angle = Math.toRadians(hsv[0].toDouble())
        val position = center + Offset(kotlin.math.cos(angle).toFloat(), kotlin.math.sin(angle).toFloat()) * (radius * hsv[1])
        drawCircle(Color.Black, 9.dp.toPx(), position, style = Stroke(3.dp.toPx()))
        drawCircle(Color.White, 9.dp.toPx(), position, style = Stroke(1.5.dp.toPx()))
    }
    Text("Brightness", color = Color.White)
    Slider(hsv[2], { change(android.graphics.Color.HSVToColor(floatArrayOf(hsv[0], hsv[1], it))) }, valueRange = 0f..1f)
}

@Composable
internal fun DrawingPanel(ed: EditorController, close: () -> Unit) {
    MaterialTheme(colorScheme = MaterialTheme.colorScheme.copy(
        primary = Color.White, onPrimary = Color.Black, secondary = Color.White,
        onSurface = Color.White, onSurfaceVariant = Color.White.copy(alpha = .75f),
    )) { DrawingPanelContent(ed, close) }
}

@Composable
private fun DrawingPanelContent(ed: EditorController, close: () -> Unit) {
    val context = androidx.compose.ui.platform.LocalContext.current
    val frame = remember { ed.stop(); (if (ed.blankDrawing) null else ed.picture)?.asAndroidBitmap()?.copy(Bitmap.Config.ARGB_8888, false) }
    val view = remember {
        val w = ed.timeline?.width?.toInt()?.coerceAtLeast(1) ?: 1080
        val h = ed.timeline?.height?.toInt()?.coerceAtLeast(1) ?: 1920
        val ratio = if (ed.blankDrawing) 1f else minOf(1f, 1536f / maxOf(w, h))
        DrawingView(context, frame, (w * ratio).toInt().coerceAtLeast(1), (h * ratio).toInt().coerceAtLeast(1))
    }
    DisposableEffect(Unit) { onDispose { view.dispose(); frame?.recycle(); ed.blankDrawing = false } }
    var color by remember { mutableIntStateOf(android.graphics.Color.BLACK) }
    var brushWidth by remember { mutableFloatStateOf(12f) }
    var opacity by remember { mutableFloatStateOf(1f) }
    var tool by remember { mutableStateOf("Brush") }
    var selectedOnly by remember { mutableStateOf(false) }
    var currentOnly by remember { mutableStateOf(false) }
    var selectionMode by remember { mutableStateOf("Add") }
    var tolerance by remember { mutableFloatStateOf(16f) }
    var transformScale by remember { mutableFloatStateOf(1f) }
    var meshPoints by remember { mutableFloatStateOf(3f) }
    var importError by remember { mutableStateOf<String?>(null) }
    var importing by remember { mutableStateOf(false) }
    val importScope = rememberCoroutineScope()
    var exportFormat by remember { mutableStateOf(DrawingFormat.PNG) }
    var exportName by remember { mutableStateOf("Drawing") }
    var pendingExport by remember { mutableStateOf<Bitmap?>(null) }
    var pendingFormat by remember { mutableStateOf(DrawingFormat.PNG) }
    var exporting by remember { mutableStateOf(false) }
    var exportMessage by remember { mutableStateOf<String?>(null) }
    DisposableEffect(Unit) { onDispose { pendingExport?.recycle(); pendingExport = null } }
    val saveDrawing = rememberLauncherForActivityResult(DrawingSaveContract()) { uri ->
        val image = pendingExport
        pendingExport = null
        if (image != null) {
            if (uri == null) image.recycle() else importScope.launch {
                exporting = true
                try { withContext(Dispatchers.IO) { writeDrawing(context, uri, image, pendingFormat) }; exportMessage = "Drawing exported." }
                catch (e: kotlinx.coroutines.CancellationException) { throw e }
                catch (e: Exception) { exportMessage = e.message ?: "Export failed. Try another destination." }
                finally { image.recycle(); exporting = false }
            }
        }
    }
    var documentBusy by remember { mutableStateOf(false) }
    var documentMessage by remember { mutableStateOf<String?>(null) }
    var pendingDocument by remember { mutableStateOf<android.net.Uri?>(null) }
    val saveDocument = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/octet-stream")) { uri ->
        if (uri != null) importScope.launch {
            documentBusy = true
            try { val bytes = view.encodeDocument(); writeDrawingResource(context, uri, bytes); documentMessage = "Layered drawing saved." }
            catch (e: kotlinx.coroutines.CancellationException) { throw e }
            catch (e: Exception) { documentMessage = e.message ?: "Could not save the drawing." }
            finally { documentBusy = false }
        }
    }
    val openDocument = rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri -> if (uri != null) pendingDocument = uri }
    val imagePicker = rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri ->
        if (uri != null) importScope.launch {
            importing = true
            try {
                val image = withContext(Dispatchers.IO) {
                    val src = android.graphics.ImageDecoder.createSource(context.contentResolver, uri)
                    android.graphics.ImageDecoder.decodeBitmap(src) { decoder, info, _ ->
                        decoder.allocator = android.graphics.ImageDecoder.ALLOCATOR_SOFTWARE
                        val ratio = minOf(1f, view.canvasW.toFloat() / info.size.width, view.canvasH.toFloat() / info.size.height)
                        decoder.setTargetSize((info.size.width * ratio).toInt().coerceAtLeast(1), (info.size.height * ratio).toInt().coerceAtLeast(1))
                    }
                }
                view.importImage(image); importError = null
            } catch (e: kotlinx.coroutines.CancellationException) { throw e }
            catch (_: Exception) { importError = "Could not import that picture." }
            finally { importing = false }
        }
    }
    var menu by remember { mutableStateOf<String?>(null) }
    var brush by remember { mutableStateOf("Pen") }
    var brushQuery by remember { mutableStateOf("") }
    var brushFamily by remember { mutableStateOf("All") }
    var pickerRadius by remember { mutableFloatStateOf(0f) }
    var penPressure by remember { mutableStateOf(true) }
    var penCursor by remember { mutableStateOf(true) }
    var hardwareMsaa by remember { mutableStateOf<Boolean?>(null) }
    LaunchedEffect(Unit) { hardwareMsaa = withContext(Dispatchers.IO) { io.github.artcraftandroid.filmcraft.core.drawingHardwareMsaaAvailable() } }
    var stabilizer by remember { mutableFloatStateOf(0f) }
    var pressureResponse by remember { mutableFloatStateOf(1f) }
    var wandContiguous by remember { mutableStateOf(true) }
    var canvasMirrored by remember { mutableStateOf(false) }
    var canvasChecker by remember { mutableStateOf(false) }
    var canvasGrid by remember { mutableStateOf(false) }
    var canvasGridSpacing by remember { mutableFloatStateOf(64f) }
    var layerRename by remember { mutableStateOf("") }
    var pendingLayerOpacity by remember { mutableFloatStateOf(1f) }
    var confirmFlatten by remember { mutableStateOf(false) }
    var mergeMode by remember { mutableIntStateOf(0) }
    var selectionRadius by remember { mutableFloatStateOf(8f) }
    var fillContiguous by remember { mutableStateOf(true) }
    var fillMerged by remember { mutableStateOf(false) }
    var fillGrow by remember { mutableFloatStateOf(0f) }
    var gradientKind by remember { mutableIntStateOf(0) }
    var gradientRepeat by remember { mutableIntStateOf(0) }
    var gradientReverse by remember { mutableStateOf(false) }
    var gradientTransparent by remember { mutableStateOf(false) }
    var gradientEnd by remember { mutableIntStateOf(android.graphics.Color.WHITE) }
    var gradientAngle by remember { mutableFloatStateOf(0f) }
    var gradientExtent by remember { mutableFloatStateOf(.5f) }
    var gradientX by remember { mutableFloatStateOf(.5f) }
    var gradientY by remember { mutableFloatStateOf(.5f) }
    var filterOperation by remember { mutableStateOf("grayscale") }
    var filterAmount by remember { mutableFloatStateOf(.5f) }
    var rasterError by remember { mutableStateOf<String?>(null) }
    var symmetry by remember { mutableIntStateOf(0) }
    var shape by remember { mutableStateOf("Circle") }
    var revision by remember { mutableIntStateOf(0) }
    LaunchedEffect(view.layer, revision) {
        layerRename = view.layerName(view.layer)
        pendingLayerOpacity = view.layerOpacity(view.layer)
    }
    val standaloneDrawing = remember { ed.standaloneDrawing }
    var closingDrawing by remember { mutableStateOf(false) }
    val requestClose: () -> Unit = {
        if (!closingDrawing && !exporting && !importing) {
            if (!standaloneDrawing) close() else {
                closingDrawing = true; view.isEnabled = false
                importScope.launch {
                    try {
                        while (view.operationBusy) kotlinx.coroutines.delay(16)
                        check(view.transformMode == null) { "Apply or cancel the transform before saving." }
                        view.commitShapeDraft()
                        val image = view.bitmap()
                        try { withContext(Dispatchers.IO) { saveDrawingPhoto(context, image) } }
                        finally { image.recycle() }
                        android.widget.Toast.makeText(context, "Saved to Pictures/FilmCraft", android.widget.Toast.LENGTH_LONG).show()
                        ed.closeEditor()
                    } catch (e: kotlinx.coroutines.CancellationException) { throw e }
                    catch (e: Exception) { exportMessage = e.message ?: "Could not save the photo."; menu = "Export" }
                    finally { closingDrawing = false; view.isEnabled = true }
                }
            }
        }
    }
    val entrance = remember { Animatable(0f) }
    LaunchedEffect(Unit) { entrance.animateTo(1f, tween(420, easing = FastOutSlowInEasing)) }
    Dialog(onDismissRequest = requestClose, properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false)) {
        ImmersiveDialog()
        Box(Modifier.fillMaxSize().background(Color(0xFF0F1118)).systemBarsPadding()) {
            AndroidView(factory = { view }, modifier = Modifier.fillMaxSize().padding(top = 56.dp).graphicsLayer {
                scaleX = .65f + .35f * entrance.value; scaleY = scaleX; alpha = entrance.value
            })
            Row(Modifier.align(Alignment.TopCenter).fillMaxWidth().padding(horizontal = 12.dp), horizontalArrangement = Arrangement.SpaceBetween) {
                FcIconButton(LucideIcons.X.glyph(), if (standaloneDrawing) "Save photo and close" else "Close", requestClose)
                EditorExportButton({ exportMessage = null; menu = "Export" }, enabled = ed.busy == null && !importing && !exporting && view.transformMode == null)
            }
            if (closingDrawing) Text("Saving photo…", color = Color.White, modifier = Modifier.align(Alignment.TopCenter).padding(top = 56.dp))
            DrawingGlass(view, Modifier.align(Alignment.CenterStart).padding(start = 8.dp, top = 60.dp, bottom = 110.dp).width(58.dp)) {
                Column(Modifier.heightIn(max = 380.dp).verticalScroll(rememberScrollState())) {
                    listOf("Brush" to LucideIcons.Paintbrush.glyph(), "Fill" to LucideIcons.PaintBucket.glyph(), "Eraser" to LucideIcons.DrawEraser.glyph(), "Smudge" to LucideIcons.DrawSmudge.glyph(), "Blur" to LucideIcons.DrawBlur.glyph(), "Lasso" to LucideIcons.DrawLasso.glyph(), "Magic wand" to LucideIcons.WandSparkles.glyph(), "Shapes" to LucideIcons.Shapes.glyph()).forEach { (name, icon) ->
                        Box(Modifier.background(if (tool == name) Color.White.copy(alpha = .18f) else Color.Transparent, RoundedCornerShape(10.dp))) {
                            DrawIcon(icon, if (name == "Brush") "Draw" else name, view.transformMode == null) { tool = name; view.tool = name; if (name == "Brush") menu = "Brushes"; if (name == "Shapes") menu = "Shapes"; if (name == "Fill") menu = "Fill" }
                        }
                    }
                    DrawIcon(LucideIcons.FlipVertical2.glyph(), "Canvas and layer transforms", !view.operationBusy && view.transformMode == null) { rasterError = null; menu = "Canvas" }
                    DrawIcon(LucideIcons.DrawLasso.glyph(), "Selection operations", !view.operationBusy && view.transformMode == null) { rasterError = null; menu = "Selection" }
                    DrawIcon(LucideIcons.SwatchBook.glyph(), "Layer filters", !view.operationBusy && view.transformMode == null) { rasterError = null; menu = "Filters" }
                    DrawIcon(LucideIcons.FlipHorizontal2.glyph(), "Painting symmetry", !view.operationBusy && view.transformMode == null) { menu = "Symmetry" }
                    DrawIcon(LucideIcons.Maximize.glyph(), "Transform current layer", view.transformMode == null) { selectedOnly = false; menu = "Transform" }
                }
            }
            if (tool == "Shapes" && (shape == "Lines" || shape == "Curves")) {
                DrawingGlass(view, Modifier.align(Alignment.TopCenter).padding(top = 54.dp, start = 72.dp, end = 8.dp)) {
                    Text(if (shape == "Curves") "Tap points • drag points or segments to curve" else "Tap to add points • drag points to move", color = Color.White, style = MaterialTheme.typography.labelSmall)
                    Row {
                        TextButton(onClick = { view.connectShape() }, enabled = view.shapes.points.size > 2) { Text(if (view.shapes.connected) "Disconnect" else "Connect") }
                        TextButton(onClick = { view.commitShapeDraft() }, enabled = view.shapes.points.size > 1) { Text("Apply") }
                        TextButton(onClick = { view.cancelShapeDraft() }) { Text("Cancel") }
                    }
                }
            }
            if (tool in listOf("Lasso", "Magic wand", "Rectangle selection", "Ellipse selection", "Polygon selection") && view.transformMode == null) {
                DrawingGlass(view, Modifier.align(Alignment.TopCenter).padding(top = 54.dp, start = 72.dp, end = 8.dp).widthIn(max = 420.dp)) {
                    Row {
                        DrawIcon(LucideIcons.Copy.glyph(), "Copy selection", view.selection != null) { view.copySelection() }
                        DrawIcon(LucideIcons.ClipboardPaste.glyph(), "Paste", view.canPaste && view.canAddLayer) { view.paste() }
                        DrawIcon(LucideIcons.DrawMove.glyph(), "Move selection", view.selection != null) { selectedOnly = true; view.beginTransform("Normal", true); transformScale = 1f; menu = "Transform" }
                        DrawIcon(LucideIcons.Maximize.glyph(), "Transform selection", view.selection != null) { selectedOnly = true; menu = "Transform" }
                        DrawIcon(LucideIcons.CircleX.glyph(), "Clear selection", view.selection != null) { view.clearSelection(); revision++ }
                    }
                    if (tool == "Polygon selection") Row {
                        TextButton(enabled = view.polygonVertexCount >= 3, onClick = { view.commitPolygonSelection(); revision++ }) { Text("Apply polygon") }
                        TextButton(enabled = view.polygonVertexCount > 0, onClick = { view.removePolygonVertex(); revision++ }) { Text("Undo vertex") }
                        TextButton(onClick = { view.cancelPolygonSelection(); revision++ }) { Text("Cancel polygon") }
                    }
                    Row(Modifier.horizontalScroll(rememberScrollState())) {
                        listOf("Add", "Subtract", "Intersect", "Replace").forEach { mode ->
                            TextButton(onClick = { selectionMode = mode; view.selectionMode = mode }) { Text(if (selectionMode == mode) "• $mode" else mode) }
                        }
                    }
                    if (tool == "Magic wand") {
                        Row(verticalAlignment = Alignment.CenterVertically) { Checkbox(wandContiguous, { wandContiguous = it; view.wandContiguous = it }); Text("Contiguous area only", color = Color.White) }
                        Text("Tap to select matching colours. Tolerance ${tolerance.toInt()}", color = Color.White, style = MaterialTheme.typography.labelSmall)
                        Slider(tolerance, { tolerance = it; view.wandTolerance = it.toInt() }, valueRange = 0f..96f, modifier = Modifier.height(30.dp))
                    }
                    Text(if (view.selection == null) "Draw a selection to enable copy, move and transform." else "Selection ready", color = Color.White, style = MaterialTheme.typography.labelSmall)
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Checkbox(currentOnly, { currentOnly = it; view.currentLayerOnly = it })
                        Text("Current layer only", color = Color.White, style = MaterialTheme.typography.labelSmall)
                    }
                }
            }
            if (menu != null) DrawingGlass(view, Modifier.align(Alignment.BottomCenter).padding(bottom = 108.dp, start = 16.dp, end = 16.dp).widthIn(max = 380.dp)) {
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                    Column(Modifier.weight(1f).padding(start = 8.dp, top = 8.dp)) {
                        Text(if (menu == "Brushes") "Draw" else menu!!, color = Color.White, style = MaterialTheme.typography.titleMedium)
                        if (menu == "Brushes") Text(if (view.nativeBrushEnabled) "Native GPU dab engine" else brush, color = Color.White.copy(alpha = .65f), style = MaterialTheme.typography.labelSmall)
                    }
                    FcIconButton(LucideIcons.X.glyph(), "Close drawing menu", { if (view.transformMode != null) view.finishTransform(false); menu = null })
                }
                if (view.operationBusy) { Text("Processing in Rust…", color = Color.White); LinearProgressIndicator(Modifier.fillMaxWidth(), color = Color.White) }
                rasterError?.let { Text(it, color = Color.White) }
                when (menu) {
                    "Resources" -> DrawingResourcePanel(color, { color = it; view.inkColor = it },
                        capturePreset = { io.github.artcraftandroid.filmcraft.core.DrawingBrushPreset("Preset", brush, brushWidth, opacity, stabilizer, pressureResponse, penPressure, symmetry.toUInt()) },
                        applyPreset = { preset ->
                            check(!view.operationBusy && view.transformMode == null) { "Finish the active operation first." }
                            require(preset.brush == "Pen" || DrawingBrushes.all.any { it.name == preset.brush }) { "This preset uses an unavailable brush." }
                            view.nativeBrushEnabled = false; brush = preset.brush; view.brush = brush; tool = "Brush"; view.tool = tool
                            brushWidth = preset.size; view.brushSize = brushWidth; opacity = preset.opacity; view.opacity = opacity
                            stabilizer = preset.stabilization; view.stabilization = stabilizer
                            pressureResponse = preset.pressureCurve; view.pressureCurve = pressureResponse
                            penPressure = preset.usePressure; view.usePressure = penPressure
                            symmetry = preset.symmetry.toInt(); view.symmetryMode = preset.symmetry; revision++
                        })
                    "OpenRaster" -> DrawingOpenRasterPanel(view)
                    "Animation" -> DrawingAnimationPanel(view)
                    "Native GPU brush" -> DrawingNativeBrushControls(view,
                        activate = { enabled -> view.nativeBrushEnabled = enabled; tool = "Brush"; view.tool = tool; view.invalidate(); revision++ },
                        change = { settings -> view.nativeBrushSettings = settings; symmetry = settings.symmetry.toInt(); view.symmetryMode = settings.symmetry; view.invalidate(); revision++ })
                    "Assistants" -> DrawingAssistantPanel(view) { tool = "Assistant origin"; view.tool = tool; menu = null }
                    "Retouch" -> DrawingRetouchPanel(view) { name -> tool = name; view.tool = name; menu = null }
                    "Layer mask" -> DrawingMaskPanel(view) { reveal -> tool = if (reveal) "Reveal mask" else "Hide mask"; view.tool = tool; menu = null }
                    "Adjustments" -> DrawingAdjustmentPanel(view)
                    "Document geometry" -> DrawingDocumentTools(view)
                    "Advanced filters" -> DrawingAdvancedFilters(view)
                    "Document" -> Column(Modifier.heightIn(max = 340.dp).verticalScroll(rememberScrollState())) {
                        Text("FilmCraft layered drawings (.fcdraw) preserve paint layers, names, visibility, opacity, blend modes, clipping, locks, transparency masks, active layer and canvas size. They are not Krita .kra files. Native GPU paint tiles retain float precision in version-four files. Undo history, selections and reference video frames are not saved. Compatibility pixel tools and animation frames currently use 8-bit pixels.", color = Color.White, style = MaterialTheme.typography.bodySmall)
                        OutlinedTextField(exportName, { exportName = it.take(80) }, label = { Text("Document name") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                        Text("${view.canvasW} × ${view.canvasH} · ${view.layers} layers", color = Color.White)
                        TextButton(enabled = !view.operationBusy && !documentBusy, onClick = { menu = "OpenRaster" }) { Text("OpenRaster layered interchange…") }
                        TextButton(enabled = !view.operationBusy && !documentBusy, onClick = { menu = "Animation" }) { Text("Flipbook animation and onion skins…") }
                        TextButton(enabled = !view.operationBusy && !documentBusy, onClick = { menu = "Document geometry" }) { Text("Document size and cropping…") }
                        documentMessage?.let { Text(it, color = Color.White) }
                        if (documentBusy) LinearProgressIndicator(Modifier.fillMaxWidth(), color = Color.White)
                        TextButton(enabled = !documentBusy && !view.operationBusy && view.transformMode == null && exportName.isNotBlank(), onClick = {
                            view.commitShapeDraft(); saveDocument.launch(exportName.trim().map { if (it in "\\/:*?\"<>|") '_' else it }.joinToString("").removeSuffix(".fcdraw") + ".fcdraw")
                        }) { Text("Save layered drawing…") }
                        TextButton(enabled = !documentBusy && !view.operationBusy && view.transformMode == null, onClick = { openDocument.launch("*/*") }) { Text("Open layered drawing…") }
                        if (pendingDocument != null) {
                            Text("Opening replaces this canvas and clears its undo history. Save your current drawing first if needed.", color = Color.White)
                            Row {
                                TextButton(onClick = { pendingDocument = null }) { Text("Cancel") }
                                TextButton(enabled = !documentBusy && !view.operationBusy, onClick = { val uri = pendingDocument ?: return@TextButton; pendingDocument = null; importScope.launch {
                                    documentBusy = true
                                    try {
                                        val bytes = readDrawingResource(context, uri, 96 * 1024 * 1024 + 8192)
                                        view.openDocument(bytes); documentMessage = "Layered drawing opened."; revision++
                                    } catch (e: kotlinx.coroutines.CancellationException) { throw e }
                                    catch (e: Exception) { documentMessage = e.message ?: "Could not open the drawing." }
                                    finally { documentBusy = false }
                                } }) { Text("Replace canvas") }
                            }
                        }
                    }
                    "Picker" -> Column(Modifier.heightIn(max = 300.dp).verticalScroll(rememberScrollState())) {
                        Text("Sample colour by touching or dragging on the canvas. Larger radii average a square patch. Merged sampling includes the reference frame and visible layer opacities.", color = Color.White, style = MaterialTheme.typography.bodySmall)
                        Text("Sample radius ${pickerRadius.toInt()} px", color = Color.White)
                        Slider(pickerRadius, { pickerRadius = it; view.pickerRadius = it.toInt() }, valueRange = 0f..32f)
                        Row(verticalAlignment = Alignment.CenterVertically) { Checkbox(currentOnly, { currentOnly = it; view.currentLayerOnly = it }); Text("Current layer only", color = Color.White) }
                        TextButton(enabled = !view.operationBusy, onClick = { tool = "Color picker"; view.tool = tool; menu = null }) { Text("Pick from canvas") }
                        TextButton(onClick = { tool = "Brush"; view.tool = tool; menu = "Brushes" }) { Text("Return to brush") }
                    }
                    "Selection" -> Column(Modifier.heightIn(max = 340.dp).verticalScroll(rememberScrollState())) {
                        Row(Modifier.horizontalScroll(rememberScrollState())) {
                            listOf("Lasso", "Rectangle selection", "Ellipse selection", "Polygon selection", "Magic wand").forEach { name ->
                                TextButton(enabled = !view.operationBusy, onClick = { tool = name; view.tool = name; menu = null }) { Text(name) }
                            }
                        }
                        Text("Selection operations use a square radius in canvas pixels. An empty result remains an empty selection, not the whole canvas.", color = Color.White, style = MaterialTheme.typography.bodySmall)
                        Row {
                            TextButton(enabled = !view.operationBusy, onClick = { view.selectAll(); revision++ }) { Text("Select all") }
                            TextButton(enabled = !view.operationBusy, onClick = { view.selectLayerOpacity(); revision++ }) { Text("From layer opacity") }
                        }
                        Text("Radius ${selectionRadius.toInt()} px", color = Color.White)
                        Slider(selectionRadius, { selectionRadius = it }, valueRange = 1f..128f)
                        listOf("invert" to "Invert", "grow" to "Grow", "shrink" to "Shrink", "border" to "Border").forEach { (id, name) ->
                            TextButton(enabled = !view.operationBusy && (view.selection != null || id == "invert"), onClick = { importScope.launch {
                                rasterError = null
                                try { view.modifySelection(id, selectionRadius.toUInt()); revision++ }
                                catch (e: kotlinx.coroutines.CancellationException) { throw e }
                                catch (e: Exception) { rasterError = e.message ?: "Selection operation failed" }
                            } }) { Text(name) }
                        }
                        TextButton(enabled = !view.operationBusy && view.selection != null, onClick = { view.cutSelection(); revision++ }) { Text("Cut selected pixels") }
                        TextButton(enabled = !view.operationBusy && view.selection != null, onClick = { view.eraseSelection(); revision++ }) { Text("Erase selected pixels") }
                        TextButton(enabled = !view.operationBusy && view.selection != null, onClick = { view.clearSelection(); revision++ }) { Text("Deselect") }
                    }
                    "Fill" -> {
                        Text("Tap the canvas to fill a matching region. Paint is composited into the current layer and restricted by the selection. Opacity uses the bottom slider.", color = Color.White, style = MaterialTheme.typography.bodySmall)
                        Row(verticalAlignment = Alignment.CenterVertically) { Checkbox(fillContiguous, { fillContiguous = it; view.fillContiguous = it }); Text("Contiguous region only", color = Color.White) }
                        Row(verticalAlignment = Alignment.CenterVertically) { Checkbox(fillMerged, { fillMerged = it; view.fillSampleMerged = it }); Text("Match visible merged image", color = Color.White) }
                        Text("Grow matched region ${fillGrow.toInt()} px", color = Color.White)
                        Slider(fillGrow, { fillGrow = it; view.fillGrow = it.toUInt() }, valueRange = 0f..32f)
                        Text("Growth uses a square radius; it is not automatic gap closing. Merged matching includes the reference frame.", color = Color.White, style = MaterialTheme.typography.bodySmall)
                        Text("Tolerance ${tolerance.toInt()}", color = Color.White)
                        Slider(tolerance, { tolerance = it; view.wandTolerance = it.toInt() }, valueRange = 0f..96f)
                    }
                    "Symmetry" -> Column(Modifier.heightIn(max = 300.dp).verticalScroll(rememberScrollState())) {
                        listOf("Off", "Mirror left / right", "Mirror top / bottom", "Both axes", "Radial · 4", "Radial · 8").forEachIndexed { index, name ->
                            SelectChip(name, symmetry == index, { symmetry = index; view.symmetryMode = index.toUInt(); view.invalidate() }, tag = "drawing_symmetry_$index")
                        }
                    }
                    "Gradient" -> DrawingMultiGradientPanel(view, color)
                    "Pattern fill" -> DrawingPatternPanel(view, color)
                    "Filters" -> Column(Modifier.heightIn(max = 320.dp).verticalScroll(rememberScrollState())) {
                        Text("Applies to the active layer, or the selected part of it. Undo restores the previous pixels.", color = Color.White, style = MaterialTheme.typography.bodySmall)
                        val filters = listOf("grayscale" to "Grayscale", "invert" to "Invert", "sepia" to "Sepia", "threshold" to "Threshold", "brightness" to "Brightness", "contrast" to "Contrast", "saturation" to "Saturation", "posterize" to "Posterize", "blur" to "Blur", "sharpen" to "Sharpen", "edges" to "Edge detection", "emboss" to "Emboss", "alpha-luma" to "Luminance to alpha", "opaque" to "Make opaque", "pixelize" to "Pixelize", "gamma" to "Gamma", "grow-alpha" to "Grow alpha", "shrink-alpha" to "Shrink alpha", "auto-levels" to "Auto levels", "equalize" to "Equalize", "solarize" to "Solarize", "temperature" to "Temperature", "alpha-threshold" to "Alpha threshold", "red-channel" to "Red channel", "green-channel" to "Green channel", "blue-channel" to "Blue channel", "alpha-channel" to "Alpha channel")
                        Row(Modifier.horizontalScroll(rememberScrollState())) { filters.forEach { (id, name) ->
                            SelectChip(name, filterOperation == id, { filterOperation = id; filterAmount = when (id) { "contrast" -> .25f; "gamma" -> .9f / 2.9f; else -> .5f } }, tag = "drawing_filter_$id")
                        } }
                        if (filterOperation !in listOf("grayscale", "invert", "sepia", "alpha-luma", "opaque", "auto-levels", "equalize", "red-channel", "green-channel", "blue-channel", "alpha-channel")) {
                            Text("Amount", color = Color.White); Slider(filterAmount, { filterAmount = it }, enabled = !view.operationBusy)
                        }
                        TextButton(onClick = { importScope.launch {
                            rasterError = null
                            try { view.applyFilter(filterOperation, filterAmount); revision++ }
                            catch (e: kotlinx.coroutines.CancellationException) { throw e }
                            catch (e: Exception) { rasterError = e.message ?: "Filter failed" }
                        } }, enabled = !view.operationBusy) { Text("Apply filter") }
                        TextButton(onClick = { menu = "Assistants" }, enabled = !view.operationBusy) { Text("Painting assistants…") }
                        TextButton(onClick = { menu = "Retouch" }, enabled = !view.operationBusy) { Text("Clone stamp and retouch brushes…") }
                        TextButton(onClick = { menu = "Advanced filters" }, enabled = !view.operationBusy) { Text("Blur, sharpening, edges and noise…") }
                        TextButton(onClick = { menu = "Adjustments" }, enabled = !view.operationBusy) { Text("Colour adjustments and curves…") }
                        TextButton(onClick = { menu = "Pattern fill" }, enabled = !view.operationBusy) { Text("Pattern fills…") }
                        TextButton(onClick = { menu = "Gradient" }, enabled = !view.operationBusy) { Text("Paint gradient…") }
                        TextButton(onClick = { menu = "Colour blend" }, enabled = !view.operationBusy) { Text("Blend selected colour…") }
                    }
                    "Colour blend" -> Column(Modifier.heightIn(max = 300.dp).verticalScroll(rememberScrollState())) {
                        Text("Blend the current colour into existing layer pixels. Alpha is preserved; the opacity slider controls strength.", color = Color.White, style = MaterialTheme.typography.bodySmall)
                        val modes = listOf("Normal", "Dissolve", "Darken", "Multiply", "Color burn", "Linear burn", "Darker color", "Lighten", "Screen", "Color dodge", "Linear dodge", "Lighter color", "Overlay", "Soft light", "Hard light", "Vivid light", "Linear light", "Pin light", "Hard mix", "Difference", "Exclusion", "Subtract", "Divide", "Hue", "Saturation", "Color", "Luminosity")
                        modes.forEachIndexed { index, name -> if (index != 1) TextButton(enabled = !view.operationBusy, onClick = { importScope.launch {
                            rasterError = null
                            try { view.blendColor(index.toUInt()); revision++ }
                            catch (e: kotlinx.coroutines.CancellationException) { throw e }
                            catch (e: Exception) { rasterError = e.message ?: "Blend failed" }
                        } }) { Text(name) } }
                    }
                    "Export" -> Column(Modifier.heightIn(max = 420.dp).verticalScroll(rememberScrollState())) {
                        TextButton(onClick = { menu = "Document" }) { Text("Save / open layered drawing…") }
                        OutlinedTextField(exportName, { exportName = it.take(80) }, label = { Text("File name") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                        DrawingFormat.entries.forEach { format ->
                            TextButton(onClick = { exportFormat = format }) { Text("${if (exportFormat == format) "●" else "○"} Export to ${format.label}") }
                        }
                        Text("PNG and JPEG XL keep transparency. JPEG/JPG use a white background.", color = Color.White, style = MaterialTheme.typography.bodySmall)
                        exportMessage?.let { Text(it, color = Color.White) }
                        TextButton(onClick = {
                            view.commitShapeDraft()
                            pendingExport?.recycle(); pendingExport = view.bitmap(); pendingFormat = exportFormat
                            val name = exportName.trim().map { if (it in "\\/:*?\"<>|") '_' else it }.joinToString("").removeSuffix(".${exportFormat.extension}")
                            saveDrawing.launch("$name.${exportFormat.extension}" to exportFormat.mime)
                        }, enabled = exportName.isNotBlank() && !exporting && pendingExport == null) { Text(if (exporting) "Exporting…" else "Save file") }
                        TextButton(onClick = { view.commitShapeDraft(); ed.importDrawing(view.bitmap()) }, enabled = !standaloneDrawing && !exporting && ed.busy == null) { Text("Add to timeline") }
                    }
                    "Colors" -> Column(Modifier.heightIn(max = 380.dp).verticalScroll(rememberScrollState())) {
                        WheelPicker(color) { color = it; view.inkColor = it }
                        Text(java.lang.String.format(java.util.Locale.ROOT, "#%06X", color and 0xFFFFFF), color = Color.White)
                        listOf("Red", "Green", "Blue").forEachIndexed { index, name ->
                            val components = intArrayOf(android.graphics.Color.red(color), android.graphics.Color.green(color), android.graphics.Color.blue(color))
                            Text("$name ${components[index]}", color = Color.White)
                            Slider(components[index].toFloat(), { value ->
                                components[index] = value.toInt(); color = android.graphics.Color.rgb(components[0], components[1], components[2]); view.inkColor = color
                            }, valueRange = 0f..255f)
                        }
                        Row {
                            TextButton(onClick = { val previous = color; color = gradientEnd; gradientEnd = previous; view.inkColor = color }) { Text("Swap start / end colours") }
                            TextButton(onClick = { menu = "Picker" }) { Text("Colour picker…") }
                        }
                        TextButton(onClick = { menu = "Resources" }) { Text("Saved colours and brush presets…") }
                    }
                    "Shapes" -> {
                        Text("Drag circles, triangles and squares from their center. Rectangles: corner to corner.", color = Color.White, style = MaterialTheme.typography.bodySmall)
                        Column(Modifier.heightIn(max = 280.dp).verticalScroll(rememberScrollState())) {
                            view.shapes.names.forEach { name -> TextButton(onClick = { shape = name; tool = "Shapes"; view.tool = tool; view.changeShape(name); menu = null }) { Text(if (shape == name) "• $name" else name) } }
                        }
                    }
                    "Brushes" -> Column(Modifier.heightIn(max = 360.dp).verticalScroll(rememberScrollState())) {
                        TextButton(onClick = { menu = "Native GPU brush" }) { Text("Native GPU brush engine…") }
                        TextButton(onClick = { menu = "Resources" }) { Text("Save / load brush presets…") }
                        TextButton(onClick = { menu = "Assistants" }, enabled = !view.operationBusy) { Text("Painting assistants…") }
                        TextButton(onClick = { menu = "Retouch" }, enabled = !view.operationBusy) { Text("Clone stamp and retouch brushes…") }
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Checkbox(penPressure, { penPressure = it; view.usePressure = it })
                            Text("Pen pressure · size & opacity", color = Color.White, style = MaterialTheme.typography.labelSmall)
                        }
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Checkbox(penCursor, { penCursor = it; view.showPenCursor = it; view.invalidate() })
                            Text("Pen hover cursor", color = Color.White, style = MaterialTheme.typography.labelSmall)
                        }
                        Text(when (hardwareMsaa) { true -> "Pen, eraser & mask · GPU 4× MSAA"; false -> "GPU unavailable · software coverage"; null -> "Starting GPU canvas…" }, color = Color.White.copy(alpha = .65f), style = MaterialTheme.typography.labelSmall)
                        Text("Stroke control", color = Color.White, style = MaterialTheme.typography.labelLarge)
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Text("Stabilizer", color = Color.White, modifier = Modifier.width(100.dp), style = MaterialTheme.typography.labelSmall)
                            Slider(stabilizer, { stabilizer = it; view.stabilization = it }, valueRange = 0f..100f, modifier = Modifier.weight(1f))
                        }
                        if (penPressure) Row(verticalAlignment = Alignment.CenterVertically) {
                            Text("Pressure curve", color = Color.White, modifier = Modifier.width(100.dp), style = MaterialTheme.typography.labelSmall)
                            Slider(pressureResponse, { pressureResponse = it; view.pressureCurve = it }, valueRange = .25f..4f, modifier = Modifier.weight(1f))
                        }
                        OutlinedTextField(brushQuery, { brushQuery = it }, label = { Text("Search 150 brushes") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                        Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState())) {
                            (listOf("All") + DrawingBrushes.families).forEach { family -> TextButton(onClick = { brushFamily = family }) { Text(if (brushFamily == family) "• $family" else family) } }
                        }
                        val shown = DrawingBrushes.all.filter { (brushFamily == "All" || it.family == brushFamily) && it.name.contains(brushQuery, true) }
                        LazyColumn(Modifier.fillMaxWidth().heightIn(max = 280.dp)) {
                            items(shown, key = { it.name }) { preset ->
                                Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(if (brush == preset.name) Color.White.copy(alpha = .12f) else Color.Transparent).clickable { view.nativeBrushEnabled = false; brush = preset.name; view.brush = preset.name; tool = "Brush"; view.tool = tool; menu = null }.padding(8.dp), verticalAlignment = Alignment.CenterVertically) {
                                    Text(if (brush == preset.name) "• ${preset.name}" else preset.name, color = Color.White, modifier = Modifier.width(130.dp), style = MaterialTheme.typography.labelSmall)
                                    ComposeCanvas(Modifier.weight(1f).height(36.dp)) {
                                        drawIntoCanvas { canvas ->
                                            val path = Path().apply { moveTo(0f, size.height / 2); cubicTo(size.width * .3f, 0f, size.width * .6f, size.height, size.width, size.height / 2) }
                                            DrawingBrushes.draw(canvas.nativeCanvas, path, preset.name, android.graphics.Color.WHITE, 12.dp.toPx(), 1f)
                                        }
                                    }
                                }
                            }
                            if (shown.isEmpty()) item { Text("No matching brushes", color = Color.White) }
                        }
                    }
                    "Transform" -> {
                        if (view.transformMode == null) {
                            Text(if (selectedOnly) "Transform selection on current layer" else "Transform current layer", color = Color.White)
                            listOf("Normal", "Perspective", "Mesh").forEach { mode ->
                                TextButton(onClick = { view.beginTransform(mode, selectedOnly); transformScale = 1f; revision++ }) { Text("$mode transform") }
                            }
                        } else {
                            Text("${view.transformMode} • drag ${if (view.transformMode == "Normal") "to move" else "control points"}", color = Color.White)
                            if (view.transformMode == "Normal") {
                                DrawingAffineControls(view)
                            }
                            if (view.transformMode == "Mesh") {
                                Text("Mesh divisions ${meshPoints.toInt()}", color = Color.White)
                                Slider(meshPoints, { meshPoints = it; view.resetMesh(it.toInt()) }, valueRange = 2f..12f, steps = 9)
                            }
                            Row {
                                TextButton(onClick = { view.finishTransform(false); menu = null }) { Text("Cancel transform") }
                                TextButton(enabled = view.affineError == null, onClick = { view.finishTransform(true); menu = null }) { Text("Apply") }
                            }
                        }
                    }
                    "Canvas" -> Column(Modifier.heightIn(max = 320.dp).verticalScroll(rememberScrollState())) {
                        TextButton(enabled = !view.operationBusy && view.transformMode == null, onClick = { menu = "Document geometry" }) { Text("Resize, crop, trim and rotate layered image…") }
                        TextButton(onClick = { menu = "Assistants" }, enabled = !view.operationBusy) { Text("Painting assistants…") }
                        TextButton(onClick = { menu = "Animation" }, enabled = !view.operationBusy) { Text("Flipbook animation and onion skins…") }
                        Text("View controls do not change exported pixels.", color = Color.White)
                        Text("Zoom ${(view.canvasZoom * 100).toInt()}%", color = Color.White)
                        Slider(view.canvasZoom, { view.setCanvasZoom(it); revision++ }, valueRange = .25f..8f)
                        Text("Rotation ${view.canvasAngle.toInt()}°", color = Color.White)
                        Slider(view.canvasAngle.coerceIn(-180f, 180f), { view.rotateCanvasView(it); revision++ }, valueRange = -180f..180f)
                        TextButton(onClick = { view.resetCanvasView(); revision++ }) { Text("Reset view / fit canvas") }
                        Row(verticalAlignment = Alignment.CenterVertically) { Checkbox(canvasMirrored, { canvasMirrored = it; view.mirrorCanvas = it; view.invalidate() }); Text("Mirror canvas view", color = Color.White) }
                        Row(verticalAlignment = Alignment.CenterVertically) { Checkbox(canvasChecker, { canvasChecker = it; view.showTransparency = it; view.invalidate() }); Text("Transparency checkerboard", color = Color.White) }
                        Row(verticalAlignment = Alignment.CenterVertically) { Checkbox(canvasGrid, { canvasGrid = it; view.showGrid = it; view.invalidate() }); Text("Grid overlay", color = Color.White) }
                        if (canvasGrid) {
                            Text("Grid spacing ${canvasGridSpacing.toInt()} px", color = Color.White)
                            Slider(canvasGridSpacing, { canvasGridSpacing = it; view.gridSpacing = it; view.invalidate() }, valueRange = 8f..256f)
                        }
                        Text("Active layer pixel transforms", color = Color.White)
                        listOf("flip-x" to "Flip horizontally", "flip-y" to "Flip vertically", "rotate-180" to "Rotate 180°", "offset-wrap" to "Offset / wrap half canvas").forEach { (id, name) ->
                            TextButton(enabled = !view.operationBusy && view.transformMode == null, onClick = { importScope.launch {
                                rasterError = null
                                try { view.applyFilter(id, 0f); revision++ }
                                catch (e: kotlinx.coroutines.CancellationException) { throw e }
                                catch (e: Exception) { rasterError = e.message ?: "Transform failed" }
                            } }) { Text(name) }
                        }
                    }
                    "Layers" -> Column(Modifier.heightIn(max = 390.dp).verticalScroll(rememberScrollState())) {
                        TextButton(onClick = { menu = "Document" }, enabled = !view.operationBusy) { Text("Save / open layered drawing…") }
                        TextButton(onClick = { view.addLayer() }, enabled = view.canAddLayer) { Text("+ Add layer") }
                        TextButton(onClick = { imagePicker.launch("image/*") }, enabled = view.canAddLayer && !importing) { Text(if (importing) "Importing…" else "Import picture") }
                        importError?.let { Text(it, color = Color.White) }
                        Row {
                            TextButton(enabled = view.canAddLayer, onClick = { view.duplicateLayer(); revision++ }) { Text("Duplicate") }
                            TextButton(enabled = !view.operationBusy && view.layers > 1 && !view.layerLocked(view.layer), onClick = { view.deleteLayer(); revision++ }) { Text("Delete") }
                        }
                        Row {
                            TextButton(enabled = !view.operationBusy && view.layer < view.layers - 1 && !view.layerLocked(view.layer), onClick = { view.moveLayer(1); revision++ }) { Text("Move up") }
                            TextButton(enabled = !view.operationBusy && view.layer > 0 && !view.layerLocked(view.layer), onClick = { view.moveLayer(-1); revision++ }) { Text("Move down") }
                        }
                        OutlinedTextField(layerRename, { layerRename = it.take(80) }, label = { Text("Layer name") }, singleLine = true, enabled = !view.operationBusy, modifier = Modifier.fillMaxWidth())
                        TextButton(enabled = !view.operationBusy && layerRename.isNotBlank(), onClick = { view.renameLayer(layerRename); revision++ }) { Text("Rename active layer") }
                        Row(verticalAlignment = Alignment.CenterVertically) { Checkbox(view.layerLocked(view.layer), { view.setLayerFlag("locked", it); revision++ }, enabled = !view.operationBusy); Text("Paint lock", color = Color.White) }
                        Row(verticalAlignment = Alignment.CenterVertically) { Checkbox(view.layerAlphaLocked(view.layer), { view.setLayerFlag("alpha", it); revision++ }, enabled = !view.operationBusy); Text("Lock alpha / paint existing pixels only", color = Color.White) }
                        Row(verticalAlignment = Alignment.CenterVertically) { Checkbox(view.layerClipped(view.layer), { view.setLayerFlag("clip", it); revision++ }, enabled = !view.operationBusy); Text("Clip to layer below", color = Color.White) }
                        if (view.layerClipped(view.layer)) Text("Uses the immediately lower layer's raw alpha, visibility and opacity. This is not group alpha inheritance.", color = Color.White, style = MaterialTheme.typography.bodySmall)
                        TextButton(enabled = !view.operationBusy && view.transformMode == null, onClick = { menu = "Layer mask" }) { Text(if (view.hasLayerMask(view.layer)) "Edit transparency mask…" else "Add transparency mask…") }
                        Text("Layer blending · non-destructive", color = Color.White)
                        Row(Modifier.horizontalScroll(rememberScrollState())) {
                            drawingLayerBlends.forEach { (id, name) -> SelectChip(name, view.layerBlendMode(view.layer) == id, { view.setLayerBlendMode(id); revision++ }, tag = "layer_blend_$id") }
                        }
                        Text("Layer opacity ${(pendingLayerOpacity * 100).toInt()}%", color = Color.White)
                        Slider(pendingLayerOpacity, { pendingLayerOpacity = it }, onValueChangeFinished = { view.setLayerOpacity(pendingLayerOpacity); revision++ }, enabled = !view.operationBusy)
                        Row(Modifier.horizontalScroll(rememberScrollState())) {
                            listOf(0 to "Normal", 3 to "Multiply", 8 to "Screen", 12 to "Overlay", 13 to "Soft light", 19 to "Difference").forEach { (id, name) ->
                                SelectChip(name, mergeMode == id, { mergeMode = id }, tag = "merge_mode_$id")
                            }
                        }
                        TextButton(enabled = !view.operationBusy && view.canMergeDown, onClick = { importScope.launch {
                            rasterError = null
                            try { view.mergeLayerDown(mergeMode.toUInt()); revision++ }
                            catch (e: kotlinx.coroutines.CancellationException) { throw e }
                            catch (e: Exception) { rasterError = e.message ?: "Merge failed" }
                        } }) { Text("Merge into layer below") }
                        if (!view.canMergeDown && view.layer > 0) Text("Pair merge requires two visible, unlocked, unclipped Normal layers. Flatten the full stack to bake other layer interactions.", color = Color.White, style = MaterialTheme.typography.bodySmall)
                        TextButton(enabled = !view.operationBusy && view.layers > 1, onClick = { confirmFlatten = true }) { Text("Flatten visible paint…") }
                        if (confirmFlatten) {
                            Text("Replace the whole paint stack with its visible result? Hidden layers are discarded. Undo restores the stack. Reference frames are excluded.", color = Color.White)
                            Row {
                                TextButton(onClick = { confirmFlatten = false }) { Text("Cancel") }
                                TextButton(enabled = !view.operationBusy, onClick = { confirmFlatten = false; importScope.launch {
                                    rasterError = null
                                    try { view.commitShapeDraft(); view.flattenDocument(); revision++ }
                                    catch (e: kotlinx.coroutines.CancellationException) { throw e }
                                    catch (e: Exception) { rasterError = e.message ?: "Flatten failed" }
                                } }) { Text("Flatten") }
                            }
                        }
                        Column(Modifier.heightIn(max = 220.dp).verticalScroll(rememberScrollState())) {
                            (view.layers - 1 downTo 0).forEach { index ->
                                Row(verticalAlignment = Alignment.CenterVertically) {
                                    Checkbox(view.layerVisible(index), { view.toggleLayerVisibility(index); revision++ }, enabled = !view.operationBusy)
                                    TextButton(enabled = !view.operationBusy, onClick = { view.layer = index; layerRename = view.layerName(index); pendingLayerOpacity = view.layerOpacity(index); revision++ }) { Text("${if (view.layer == index) "●" else "○"} ${view.layerName(index)}") }
                                }
                            }
                        }
                    }
                }
            }
            DrawingGlass(view, Modifier.align(Alignment.BottomCenter).padding(horizontal = 10.dp, vertical = 8.dp).fillMaxWidth()) {
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.SpaceEvenly) {
                    DrawIcon(LucideIcons.Paintbrush.glyph(), "Brushes", !view.operationBusy && view.transformMode == null) { menu = if (menu == "Brushes") null else "Brushes" }
                    Box(Modifier.size(32.dp).clip(RoundedCornerShape(7.dp)).background(Color(color)).border(2.dp, Color.White, RoundedCornerShape(7.dp)).semantics { contentDescription = "Brush color" }.clickable(enabled = view.transformMode == null) { menu = if (menu == "Colors") null else "Colors" })
                    DrawIcon(LucideIcons.Undo2.glyph(), "Undo", revision >= 0 && view.canUndo) { view.undo() }
                    DrawIcon(LucideIcons.Redo2.glyph(), "Redo", revision >= 0 && view.canRedo) { view.redo() }
                    DrawIcon(LucideIcons.Layers.glyph(), "Layers", !view.operationBusy && view.transformMode == null) { layerRename = view.layerName(view.layer); pendingLayerOpacity = view.layerOpacity(view.layer); menu = if (menu == "Layers") null else "Layers" }
                }
                Row(verticalAlignment = Alignment.CenterVertically) {
                    GlyphIcon(LucideIcons.Paintbrush.glyph(), Color.White, size = 18.dp, description = "Brush size")
                    Slider(brushWidth, { brushWidth = it; view.brushSize = it }, modifier = Modifier.weight(1f).height(32.dp).semantics { contentDescription = "Brush size" }, valueRange = 1f..100f)
                    Text("${brushWidth.toInt()}", color = Color.White, style = MaterialTheme.typography.labelSmall)
                    Spacer(Modifier.width(8.dp))
                    GlyphIcon(Glyphs.Checker, Color.White, size = 18.dp, description = "Opacity")
                    Slider(opacity, { opacity = it; view.opacity = it }, modifier = Modifier.weight(1f).height(32.dp).semantics { contentDescription = "Brush opacity" }, valueRange = 0f..1f)
                    Text("${(opacity * 100).toInt()}%", color = Color.White, style = MaterialTheme.typography.labelSmall)
                }
            }
            if (closingDrawing) Box(Modifier.fillMaxSize().clickable { }, contentAlignment = Alignment.Center) {
                DrawingGlass(view, Modifier.padding(24.dp)) { Text("Saving photo…", color = Color.White, modifier = Modifier.padding(20.dp)) }
            }
        }
    }
    // Touch edits update undo/redo availability without polling.
    SideEffect { view.onEdit = { revision++ }; view.onColorPicked = { color = it } }
}
