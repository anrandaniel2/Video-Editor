package io.github.artcraftandroid.filmcraft.ui.editor

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Path
import android.view.MotionEvent
import android.view.View
import androidx.compose.runtime.*
import androidx.compose.foundation.layout.*
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
internal fun OnlineSoundsPanel(ed: EditorController, close: () -> Unit) {
    var url by remember { mutableStateOf("") }
    AlertDialog(onDismissRequest = close, title = { Text("Online sound effects") }, text = {
        Column {
            Text("Paste a direct HTTPS sound download link. WAV, MP3, OGG, FLAC and AAC supported, up to 100 MB. Only import sounds you have permission to use. Downloads are saved inside your project.")
            OutlinedTextField(url, { url = it }, label = { Text("Audio download URL") }, singleLine = true)
        }
    }, confirmButton = { TextButton(onClick = { ed.importOnlineSound(url) }, enabled = url.startsWith("https://") && ed.busy == null) { Text("Import sound") } }, dismissButton = { TextButton(onClick = close) { Text("Cancel") } })
}

@Composable
internal fun ColorPicker(label: String, color: Int, change: (Int) -> Unit) {
    val hsv = remember(color) { FloatArray(3).also { android.graphics.Color.colorToHSV(color, it) } }
    var hex by remember(color) { mutableStateOf("%06X".format(color and 0xffffff)) }
    Column(Modifier.fillMaxWidth().padding(12.dp)) {
        Text(label)
        listOf("Hue", "Saturation", "Brightness").forEachIndexed { index, name ->
            Text(name)
            Slider(value = hsv[index], onValueChange = { v ->
                val next = hsv.copyOf(); next[index] = v
                change(android.graphics.Color.HSVToColor(next))
            }, valueRange = if (index == 0) 0f..360f else 0f..1f)
        }
        OutlinedTextField(hex, { text ->
            hex = text.removePrefix("#").take(6)
            if (hex.length == 6) hex.toLongOrNull(16)?.let { change((0xff000000L or it).toInt()) }
        }, label = { Text("Hex RGB") }, singleLine = true)
    }
}

private data class Ink(val path: Path, val color: Int, val width: Float, val alpha: Int, val brush: String, val layer: Int)
private class DrawingView(context: android.content.Context, val backgroundFrame: Bitmap?, val canvasW: Int, val canvasH: Int) : View(context) {
    private fun fit() = minOf(width.toFloat() / canvasW, height.toFloat() / canvasH)
    private fun left() = (width - canvasW * fit()) / 2
    private fun top() = (height - canvasH * fit()) / 2
    init { setLayerType(LAYER_TYPE_SOFTWARE, null) }
    var onEdit: () -> Unit = {}
    val strokes = mutableListOf<Ink>()
    val redo = mutableListOf<Ink>()
    var inkColor = android.graphics.Color.BLACK
    var brushSize = 12f
    var opacity = 1f
    var brush = "Pen"
    var layer = 0
    var layers = 1
    private var active: Ink? = null
    private var zoom = 1f
    private var dx = 0f
    private var dy = 0f
    private var oldSpan = 0f
    private var oldX = 0f
    private var oldY = 0f
    private var navigating = false
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
    private fun render(canvas: Canvas) {
        for (l in 0 until layers) {
            val saved = canvas.saveLayer(0f, 0f, canvasW.toFloat(), canvasH.toFloat(), null)
            strokes.filter { it.layer == l }.forEach { s ->
                paint.reset(); paint.isAntiAlias = true; paint.style = Paint.Style.STROKE
                paint.strokeCap = Paint.Cap.ROUND; paint.strokeJoin = Paint.Join.ROUND
                paint.color = s.color; paint.alpha = s.alpha; paint.strokeWidth = s.width
                if (s.brush == "Eraser") paint.xfermode = android.graphics.PorterDuffXfermode(android.graphics.PorterDuff.Mode.CLEAR)
                if (s.brush == "Airbrush") paint.maskFilter = android.graphics.BlurMaskFilter(s.width * .4f, android.graphics.BlurMaskFilter.Blur.NORMAL)
                if (s.brush == "Marker") paint.alpha = (s.alpha * .4f).toInt()
                canvas.drawPath(s.path, paint)
            }
            canvas.restoreToCount(saved)
        }
    }
    override fun onDraw(canvas: Canvas) {
        canvas.drawColor(android.graphics.Color.rgb(15, 17, 24))
        canvas.save(); canvas.translate(left() + dx, top() + dy); canvas.scale(fit() * zoom, fit() * zoom)
        canvas.clipRect(0f, 0f, canvasW.toFloat(), canvasH.toFloat())
        paint.reset(); paint.color = android.graphics.Color.BLACK
        canvas.drawRect(0f, 0f, canvasW.toFloat(), canvasH.toFloat(), paint)
        backgroundFrame?.let { canvas.drawBitmap(it, null, android.graphics.Rect(0, 0, canvasW, canvasH), paint) }
        render(canvas); canvas.restore()
    }
    fun bitmap(): Bitmap = Bitmap.createBitmap(canvasW, canvasH, Bitmap.Config.ARGB_8888).also { render(Canvas(it)) }
    override fun onTouchEvent(event: MotionEvent): Boolean {
        parent?.requestDisallowInterceptTouchEvent(true)
        if (event.pointerCount > 1) {
            if (!navigating) { active?.let { strokes.remove(it) }; active = null; navigating = true; oldSpan = 0f; onEdit() }
            val x = (event.getX(0) + event.getX(1)) / 2
            val y = (event.getY(0) + event.getY(1)) / 2
            val span = kotlin.math.hypot(event.getX(0) - event.getX(1), event.getY(0) - event.getY(1))
            if (oldSpan > 0 && event.actionMasked == MotionEvent.ACTION_MOVE) {
                val next = (zoom * span / oldSpan).coerceIn(.25f, 8f)
                dx = x - left() - (oldX - left() - dx) * next / zoom; dy = y - top() - (oldY - top() - dy) * next / zoom; zoom = next
            }
            oldSpan = span; oldX = x; oldY = y; invalidate(); return true
        }
        if (navigating) { if (event.actionMasked == MotionEvent.ACTION_UP || event.actionMasked == MotionEvent.ACTION_CANCEL) navigating = false; return true }
        val scale = fit() * zoom
        val x = ((event.x - left() - dx) / scale).coerceIn(0f, canvasW.toFloat())
        val y = ((event.y - top() - dy) / scale).coerceIn(0f, canvasH.toFloat())
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                if (event.x < left() + dx || event.x > left() + dx + canvasW * scale || event.y < top() + dy || event.y > top() + dy + canvasH * scale) return true
                active = Ink(Path().apply { moveTo(x, y); lineTo(x + .01f, y) }, inkColor, if (brush == "Pencil") brushSize * .4f else brushSize, (opacity * 255).toInt(), brush, layer)
                strokes.add(active!!)
            }
            MotionEvent.ACTION_MOVE -> active?.path?.lineTo(x, y)
            MotionEvent.ACTION_UP -> { if (active != null) redo.clear(); active = null; onEdit() }
            MotionEvent.ACTION_CANCEL -> { active?.let { strokes.remove(it) }; active = null }
        }
        invalidate(); return true
    }
}

/** Blur a frozen preview behind translucent, bordered glass; controls stay sharp. */
@Composable
private fun DrawingGlass(frame: Bitmap?, modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    Box(modifier.clip(RoundedCornerShape(24.dp)).border(1.dp, Color.White.copy(alpha = .22f), RoundedCornerShape(24.dp))) {
        frame?.let { Image(it.asImageBitmap(), null, Modifier.matchParentSize().blur(28.dp), contentScale = androidx.compose.ui.layout.ContentScale.Crop) }
        Box(Modifier.matchParentSize().background(Color(0xD0232835)))
        Column(Modifier.padding(12.dp), content = content)
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
    val frame = remember { ed.stop(); ed.picture?.asAndroidBitmap()?.copy(Bitmap.Config.ARGB_8888, false) }
    val view = remember {
        val w = ed.timeline?.width?.toInt()?.coerceAtLeast(1) ?: 1080
        val h = ed.timeline?.height?.toInt()?.coerceAtLeast(1) ?: 1920
        val ratio = minOf(1f, 2048f / maxOf(w, h))
        DrawingView(context, frame, (w * ratio).toInt().coerceAtLeast(1), (h * ratio).toInt().coerceAtLeast(1))
    }
    DisposableEffect(Unit) { onDispose { frame?.recycle() } }
    var color by remember { mutableIntStateOf(android.graphics.Color.BLACK) }
    var brushWidth by remember { mutableFloatStateOf(12f) }
    var opacity by remember { mutableFloatStateOf(1f) }
    var layer by remember { mutableIntStateOf(0) }
    var layers by remember { mutableIntStateOf(1) }
    var menu by remember { mutableStateOf<String?>(null) }
    var brush by remember { mutableStateOf("Pen") }
    var revision by remember { mutableIntStateOf(0) }
    val entrance = remember { Animatable(0f) }
    LaunchedEffect(Unit) { entrance.animateTo(1f, tween(420, easing = FastOutSlowInEasing)) }
    Dialog(onDismissRequest = close, properties = DialogProperties(usePlatformDefaultWidth = false)) {
        Box(Modifier.fillMaxSize().background(Color(0xFF0F1118)).systemBarsPadding()) {
            AndroidView(factory = { view }, modifier = Modifier.fillMaxSize().padding(bottom = 110.dp, top = 56.dp).graphicsLayer {
                scaleX = .65f + .35f * entrance.value; scaleY = scaleX; alpha = entrance.value
            })
            Row(Modifier.align(Alignment.TopCenter).fillMaxWidth().padding(horizontal = 12.dp), horizontalArrangement = Arrangement.SpaceBetween) {
                TextButton(onClick = close) { Text("Cancel") }
                TextButton(onClick = { ed.importDrawing(view.bitmap()) }, enabled = ed.busy == null) { Text("Done") }
            }
            if (menu != null) DrawingGlass(frame, Modifier.align(Alignment.BottomCenter).padding(bottom = 138.dp, start = 16.dp, end = 16.dp).widthIn(max = 380.dp)) {
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                    Text(menu!!, color = Color.White)
                    TextButton(onClick = { menu = null }) { Text("Close") }
                }
                when (menu) {
                    "Colors" -> WheelPicker(color) { color = it; view.inkColor = it }
                    "Brushes" -> {
                        listOf("Pen", "Pencil", "Marker", "Airbrush", "Eraser").forEach { name ->
                            Row(Modifier.fillMaxWidth().clickable { brush = name; view.brush = name; menu = null }.padding(8.dp), verticalAlignment = Alignment.CenterVertically) {
                                Text(if (brush == name) "• $name" else name, color = Color.White, modifier = Modifier.width(90.dp))
                                ComposeCanvas(Modifier.weight(1f).height(28.dp)) {
                                    val path = androidx.compose.ui.graphics.Path().apply { moveTo(0f, size.height / 2); cubicTo(size.width * .3f, -10f, size.width * .6f, size.height + 10f, size.width, size.height / 2) }
                                    drawPath(path, Color.White.copy(alpha = if (name == "Marker") .4f else 1f), style = Stroke(if (name == "Pencil") 2f else if (name == "Airbrush") 16f else 7f, cap = androidx.compose.ui.graphics.StrokeCap.Round))
                                }
                            }
                        }
                        Text("Size ${brushWidth.toInt()}", color = Color.White)
                        Slider(brushWidth, { brushWidth = it; view.brushSize = it }, valueRange = 1f..100f)
                    }
                    "Layers" -> {
                        TextButton(onClick = { if (layers < 16) { layers++; layer = layers - 1; view.layers = layers; view.layer = layer } }) { Text("+ Add layer") }
                        Column(Modifier.heightIn(max = 220.dp).verticalScroll(rememberScrollState())) {
                            (layers - 1 downTo 0).forEach { index ->
                                TextButton(onClick = { layer = index; view.layer = index }) { Text("${if (layer == index) "●" else "○"} Layer ${index + 1}") }
                            }
                        }
                    }
                }
            }
            DrawingGlass(frame, Modifier.align(Alignment.BottomCenter).padding(12.dp).fillMaxWidth()) {
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.SpaceEvenly) {
                    DrawIcon(LucideIcons.Paintbrush.glyph(), "Brushes") { menu = if (menu == "Brushes") null else "Brushes" }
                    Box(Modifier.size(32.dp).clip(RoundedCornerShape(7.dp)).background(Color(color)).border(2.dp, Color.White, RoundedCornerShape(7.dp)).semantics { contentDescription = "Brush color" }.clickable { menu = if (menu == "Colors") null else "Colors" })
                    DrawIcon(LucideIcons.Undo2.glyph(), "Undo", revision >= 0 && view.strokes.isNotEmpty()) { view.redo.add(view.strokes.removeAt(view.strokes.lastIndex)); view.invalidate(); revision++ }
                    DrawIcon(LucideIcons.Redo2.glyph(), "Redo", revision >= 0 && view.redo.isNotEmpty()) { view.strokes.add(view.redo.removeAt(view.redo.lastIndex)); view.invalidate(); revision++ }
                    DrawIcon(LucideIcons.Layers.glyph(), "Layers") { menu = if (menu == "Layers") null else "Layers" }
                }
                Row(verticalAlignment = Alignment.CenterVertically) {
                    GlyphIcon(Glyphs.Checker, Color.White, description = "Opacity")
                    Slider(opacity, { opacity = it; view.opacity = it }, modifier = Modifier.weight(1f), valueRange = 0f..1f)
                    Text("${(opacity * 100).toInt()}%", color = Color.White)
                }
            }
        }
    }
    // Touch edits update undo/redo availability without polling.
    SideEffect { view.onEdit = { revision++ } }
}
