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
internal fun downloadSound(address: String, dir: File): File {
    var url = URI(address.trim()).toURL()
    dir.mkdirs()
    val temporary = File(dir, "sound-${java.util.UUID.randomUUID()}.part")
    try {
        repeat(6) {
            require(url.protocol == "https") { "Use a direct HTTPS audio link." }
            val connection = url.openConnection() as HttpURLConnection
            connection.connectTimeout = 15000
            connection.readTimeout = 30000
            connection.instanceFollowRedirects = false
            try {
                val code = connection.responseCode
                if (code in 300..399) {
                    url = java.net.URL(url, connection.getHeaderField("Location") ?: error("Missing redirect location"))
                } else {
                    require(code in 200..299) { "Download failed (HTTP $code)." }
                    val type = connection.contentType.orEmpty().substringBefore(';').lowercase()
                    val ext = when {
                        type.contains("wav") -> "wav"
                        type.contains("ogg") -> "ogg"
                        type.contains("flac") -> "flac"
                        type.contains("mp4") || type.contains("aac") -> "m4a"
                        type.contains("mpeg") -> "mp3"
                        else -> url.path.substringAfterLast('.', "").lowercase()
                    }
                    require(ext in listOf("wav", "mp3", "ogg", "flac", "m4a", "aac", "opus") && !type.contains("text/html")) { "This is a webpage, not a supported audio file. Paste its direct download link." }
                    connection.inputStream.use { input -> temporary.outputStream().use { output ->
                        val buffer = ByteArray(32768)
                        var total = 0L
                        while (true) {
                            val count = input.read(buffer)
                            if (count < 0) break
                            total += count
                            require(total <= 100L * 1024 * 1024) { "Sounds must be under 100 MB." }
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
            TextButton(onClick = { expanded = false }) { Text("Done") }
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
    var shape by remember { mutableStateOf("Circle") }
    var revision by remember { mutableIntStateOf(0) }
    val entrance = remember { Animatable(0f) }
    LaunchedEffect(Unit) { entrance.animateTo(1f, tween(420, easing = FastOutSlowInEasing)) }
    Dialog(onDismissRequest = close, properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false)) {
        ImmersiveDialog()
        Box(Modifier.fillMaxSize().background(Color(0xFF0F1118)).systemBarsPadding()) {
            AndroidView(factory = { view }, modifier = Modifier.fillMaxSize().padding(top = 56.dp).graphicsLayer {
                scaleX = .65f + .35f * entrance.value; scaleY = scaleX; alpha = entrance.value
            })
            Row(Modifier.align(Alignment.TopCenter).fillMaxWidth().padding(horizontal = 12.dp), horizontalArrangement = Arrangement.SpaceBetween) {
                TextButton(onClick = close) { Text("Cancel") }
                EditorExportButton({ exportMessage = null; menu = "Export" }, enabled = ed.busy == null && !importing && !exporting && view.transformMode == null)
            }
            DrawingGlass(view, Modifier.align(Alignment.CenterStart).padding(start = 8.dp, top = 60.dp, bottom = 110.dp).width(58.dp)) {
                Column(Modifier.heightIn(max = 380.dp).verticalScroll(rememberScrollState())) {
                    listOf("Brush" to LucideIcons.Paintbrush.glyph(), "Eraser" to LucideIcons.DrawEraser.glyph(), "Smudge" to LucideIcons.DrawSmudge.glyph(), "Blur" to LucideIcons.DrawBlur.glyph(), "Lasso" to LucideIcons.DrawLasso.glyph(), "Magic wand" to LucideIcons.WandSparkles.glyph(), "Shapes" to LucideIcons.Shapes.glyph()).forEach { (name, icon) ->
                        Box(Modifier.background(if (tool == name) Color.White.copy(alpha = .18f) else Color.Transparent, RoundedCornerShape(10.dp))) {
                            DrawIcon(icon, name, view.transformMode == null) { tool = name; view.tool = name; if (name == "Brush") menu = "Brushes"; if (name == "Shapes") menu = "Shapes" }
                        }
                    }
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
            if ((tool == "Lasso" || tool == "Magic wand") && view.transformMode == null) {
                DrawingGlass(view, Modifier.align(Alignment.TopCenter).padding(top = 54.dp, start = 72.dp, end = 8.dp).widthIn(max = 420.dp)) {
                    Row {
                        DrawIcon(LucideIcons.Copy.glyph(), "Copy selection", view.selection != null) { view.copySelection() }
                        DrawIcon(LucideIcons.ClipboardPaste.glyph(), "Paste", view.canPaste && view.canAddLayer) { view.paste() }
                        DrawIcon(LucideIcons.DrawMove.glyph(), "Move selection", view.selection != null) { selectedOnly = true; view.beginTransform("Normal", true); transformScale = 1f; menu = "Transform" }
                        DrawIcon(LucideIcons.Maximize.glyph(), "Transform selection", view.selection != null) { selectedOnly = true; menu = "Transform" }
                        DrawIcon(LucideIcons.CircleX.glyph(), "Clear selection", view.selection != null) { view.clearSelection(); revision++ }
                    }
                    if (tool == "Lasso") Row {
                        listOf("Add", "Subtract", "Replace").forEach { mode ->
                            TextButton(onClick = { selectionMode = mode; view.selectionMode = mode }) { Text(if (selectionMode == mode) "• $mode" else mode) }
                        }
                    } else {
                        Text("Tap touching same-color areas to add them. Tolerance ${tolerance.toInt()}", color = Color.White, style = MaterialTheme.typography.labelSmall)
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
                    Text(menu!!, color = Color.White)
                    TextButton(onClick = { if (view.transformMode != null) view.finishTransform(false); menu = null }) { Text("Close") }
                }
                when (menu) {
                    "Export" -> Column(Modifier.heightIn(max = 420.dp).verticalScroll(rememberScrollState())) {
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
                        TextButton(onClick = { view.commitShapeDraft(); ed.importDrawing(view.bitmap()) }, enabled = !exporting && ed.busy == null) { Text("Add to timeline") }
                    }
                    "Colors" -> WheelPicker(color) { color = it; view.inkColor = it }
                    "Shapes" -> {
                        Text("Drag circles, triangles and squares from their center. Rectangles: corner to corner.", color = Color.White, style = MaterialTheme.typography.bodySmall)
                        Column(Modifier.heightIn(max = 280.dp).verticalScroll(rememberScrollState())) {
                            view.shapes.names.forEach { name -> TextButton(onClick = { shape = name; tool = "Shapes"; view.tool = tool; view.changeShape(name); menu = null }) { Text(if (shape == name) "• $name" else name) } }
                        }
                    }
                    "Brushes" -> {
                        OutlinedTextField(brushQuery, { brushQuery = it }, label = { Text("Search 150 brushes") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                        Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState())) {
                            (listOf("All") + DrawingBrushes.families).forEach { family -> TextButton(onClick = { brushFamily = family }) { Text(if (brushFamily == family) "• $family" else family) } }
                        }
                        val shown = DrawingBrushes.all.filter { (brushFamily == "All" || it.family == brushFamily) && it.name.contains(brushQuery, true) }
                        LazyColumn(Modifier.fillMaxWidth().heightIn(max = 280.dp)) {
                            items(shown, key = { it.name }) { preset ->
                                Row(Modifier.fillMaxWidth().clickable { brush = preset.name; view.brush = preset.name; tool = "Brush"; view.tool = tool; menu = null }.padding(8.dp), verticalAlignment = Alignment.CenterVertically) {
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
                                Text("Scale ${(transformScale * 100).toInt()}%", color = Color.White)
                                Slider(transformScale, { transformScale = it; view.setScale(it) }, valueRange = .1f..4f)
                            }
                            if (view.transformMode == "Mesh") {
                                Text("Mesh divisions ${meshPoints.toInt()}", color = Color.White)
                                Slider(meshPoints, { meshPoints = it; view.resetMesh(it.toInt()) }, valueRange = 2f..12f, steps = 9)
                            }
                            Row {
                                TextButton(onClick = { view.finishTransform(false); menu = null }) { Text("Cancel transform") }
                                TextButton(onClick = { view.finishTransform(true); menu = null }) { Text("Apply") }
                            }
                        }
                    }
                    "Layers" -> {
                        TextButton(onClick = { view.addLayer() }, enabled = view.canAddLayer) { Text("+ Add layer") }
                        TextButton(onClick = { imagePicker.launch("image/*") }, enabled = view.canAddLayer && !importing) { Text(if (importing) "Importing…" else "Import picture") }
                        importError?.let { Text(it, color = Color.White) }
                        Column(Modifier.heightIn(max = 220.dp).verticalScroll(rememberScrollState())) {
                            (view.layers - 1 downTo 0).forEach { index ->
                                TextButton(onClick = { view.layer = index; revision++ }) { Text("${if (view.layer == index) "●" else "○"} Layer ${index + 1}") }
                            }
                        }
                    }
                }
            }
            DrawingGlass(view, Modifier.align(Alignment.BottomCenter).padding(horizontal = 10.dp, vertical = 8.dp).fillMaxWidth()) {
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.SpaceEvenly) {
                    DrawIcon(LucideIcons.Paintbrush.glyph(), "Brushes", view.transformMode == null) { menu = if (menu == "Brushes") null else "Brushes" }
                    Box(Modifier.size(32.dp).clip(RoundedCornerShape(7.dp)).background(Color(color)).border(2.dp, Color.White, RoundedCornerShape(7.dp)).semantics { contentDescription = "Brush color" }.clickable(enabled = view.transformMode == null) { menu = if (menu == "Colors") null else "Colors" })
                    DrawIcon(LucideIcons.Undo2.glyph(), "Undo", revision >= 0 && view.canUndo) { view.undo() }
                    DrawIcon(LucideIcons.Redo2.glyph(), "Redo", revision >= 0 && view.canRedo) { view.redo() }
                    DrawIcon(LucideIcons.Layers.glyph(), "Layers", view.transformMode == null) { menu = if (menu == "Layers") null else "Layers" }
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
        }
    }
    // Touch edits update undo/redo availability without polling.
    SideEffect { view.onEdit = { revision++ } }
}
