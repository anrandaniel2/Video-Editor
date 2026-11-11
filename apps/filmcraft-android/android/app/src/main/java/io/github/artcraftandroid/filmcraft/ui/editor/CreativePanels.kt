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
internal fun ColorPicker(label: String, color: Int, change: (Int) -> Unit) {
    var expanded by remember { mutableStateOf(false) }
    Row(Modifier.fillMaxWidth().padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(label, modifier = Modifier.weight(1f))
        Box(Modifier.size(36.dp).clip(RoundedCornerShape(8.dp)).background(Color(color))
            .border(1.dp, Color.White.copy(alpha = .7f), RoundedCornerShape(8.dp))
            .semantics { contentDescription = label }
            .clickable { expanded = true })
    }
    if (expanded) Dialog(onDismissRequest = { expanded = false }) {
        Column(Modifier.widthIn(max = 340.dp).editorGlass(24.dp).padding(20.dp), horizontalAlignment = Alignment.CenterHorizontally) {
            Text(label, color = Color.White)
            Spacer(Modifier.height(12.dp))
            WheelPicker(color, change)
            TextButton(onClick = { expanded = false }) { Text("Done") }
        }
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
    private var angle = 0f
    private var oldAngle = 0f
    private var lastX = 0f
    private var lastY = 0f
    val backdropObservers = mutableSetOf<View>()
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
    private fun transform() = android.graphics.Matrix().apply {
        postScale(fit() * zoom, fit() * zoom)
        postRotate(angle)
        postTranslate(left() + dx, top() + dy)
    }
    private fun canvasPoint(x: Float, y: Float): FloatArray {
        val inverse = android.graphics.Matrix()
        transform().invert(inverse)
        return floatArrayOf(x, y).also { inverse.mapPoints(it) }
    }
    fun drawScene(canvas: Canvas) {
        canvas.drawColor(android.graphics.Color.rgb(15, 17, 24))
        canvas.save(); canvas.concat(transform())
        canvas.clipRect(0f, 0f, canvasW.toFloat(), canvasH.toFloat())
        paint.reset(); paint.color = android.graphics.Color.BLACK; paint.isFilterBitmap = true
        canvas.drawRect(0f, 0f, canvasW.toFloat(), canvasH.toFloat(), paint)
        backgroundFrame?.let { canvas.drawBitmap(it, null, android.graphics.Rect(0, 0, canvasW, canvasH), paint) }
        render(canvas); canvas.restore()
    }
    override fun onDraw(canvas: Canvas) {
        drawScene(canvas)
        backdropObservers.forEach { it.postInvalidateOnAnimation() }
    }
    /** Midpoint quadratic interpolation consumes historical samples as well as the latest event. */
    private fun sample(x: Float, y: Float) {
        active?.path?.quadTo(lastX, lastY, (lastX + x) * .5f, (lastY + y) * .5f)
        lastX = x; lastY = y
    }
    fun bitmap(): Bitmap = Bitmap.createBitmap(canvasW, canvasH, Bitmap.Config.ARGB_8888).also { render(Canvas(it)) }
    override fun onTouchEvent(event: MotionEvent): Boolean {
        parent?.requestDisallowInterceptTouchEvent(true)
        if (event.pointerCount > 1) {
            if (!navigating) { active?.let { strokes.remove(it) }; active = null; navigating = true; oldSpan = 0f; onEdit() }
            val x = (event.getX(0) + event.getX(1)) / 2
            val y = (event.getY(0) + event.getY(1)) / 2
            val span = kotlin.math.hypot(event.getX(0) - event.getX(1), event.getY(0) - event.getY(1))
            val currentAngle = Math.toDegrees(kotlin.math.atan2(event.getY(1) - event.getY(0), event.getX(1) - event.getX(0)).toDouble()).toFloat()
            if (oldSpan > 0 && event.actionMasked == MotionEvent.ACTION_MOVE) {
                val anchor = canvasPoint(oldX, oldY)
                zoom = (zoom * span / oldSpan).coerceIn(.25f, 8f)
                val delta = ((currentAngle - oldAngle + 540f) % 360f) - 180f
                angle = (angle + delta) % 360f
                val rotated = floatArrayOf(anchor[0], anchor[1])
                android.graphics.Matrix().apply { postScale(fit() * zoom, fit() * zoom); postRotate(angle) }.mapPoints(rotated)
                dx = x - left() - rotated[0]; dy = y - top() - rotated[1]
            }
            oldSpan = span; oldAngle = currentAngle; oldX = x; oldY = y; invalidate(); return true
        }
        if (navigating) { if (event.actionMasked == MotionEvent.ACTION_UP || event.actionMasked == MotionEvent.ACTION_CANCEL) navigating = false; return true }
        val point = canvasPoint(event.x, event.y)
        val x = point[0]; val y = point[1]
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                if (x !in 0f..canvasW.toFloat() || y !in 0f..canvasH.toFloat()) return true
                lastX = x; lastY = y
                active = Ink(Path().apply { moveTo(x, y); lineTo(x + .01f, y) }, inkColor, if (brush == "Pencil") brushSize * .4f else brushSize, (opacity * 255).toInt(), brush, layer)
                strokes.add(active!!)
            }
            MotionEvent.ACTION_MOVE -> {
                for (i in 0 until event.historySize) {
                    val historic = canvasPoint(event.getHistoricalX(i), event.getHistoricalY(i))
                    sample(historic[0], historic[1])
                }
                sample(x, y)
            }
            MotionEvent.ACTION_UP -> { active?.path?.lineTo(x, y); if (active != null) redo.clear(); active = null; onEdit() }
            MotionEvent.ACTION_CANCEL -> { active?.let { strokes.remove(it) }; active = null }
        }
        invalidate(); return true
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
    Box(modifier.clip(RoundedCornerShape(20.dp)).border(1.dp, Color.White.copy(alpha = .22f), RoundedCornerShape(20.dp))) {
        AndroidView(factory = { DrawingBackdrop(it, view) }, modifier = Modifier.matchParentSize().blur(20.dp))
        Box(Modifier.matchParentSize().background(Color(0x99232835)))
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
            AndroidView(factory = { view }, modifier = Modifier.fillMaxSize().padding(top = 56.dp).graphicsLayer {
                scaleX = .65f + .35f * entrance.value; scaleY = scaleX; alpha = entrance.value
            })
            Row(Modifier.align(Alignment.TopCenter).fillMaxWidth().padding(horizontal = 12.dp), horizontalArrangement = Arrangement.SpaceBetween) {
                TextButton(onClick = close) { Text("Cancel") }
                TextButton(onClick = { ed.importDrawing(view.bitmap()) }, enabled = ed.busy == null) { Text("Done") }
            }
            if (menu != null) DrawingGlass(view, Modifier.align(Alignment.BottomCenter).padding(bottom = 108.dp, start = 16.dp, end = 16.dp).widthIn(max = 380.dp)) {
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
            DrawingGlass(view, Modifier.align(Alignment.BottomCenter).padding(horizontal = 10.dp, vertical = 8.dp).fillMaxWidth()) {
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.SpaceEvenly) {
                    DrawIcon(LucideIcons.Paintbrush.glyph(), "Brushes") { menu = if (menu == "Brushes") null else "Brushes" }
                    Box(Modifier.size(32.dp).clip(RoundedCornerShape(7.dp)).background(Color(color)).border(2.dp, Color.White, RoundedCornerShape(7.dp)).semantics { contentDescription = "Brush color" }.clickable { menu = if (menu == "Colors") null else "Colors" })
                    DrawIcon(LucideIcons.Undo2.glyph(), "Undo", revision >= 0 && view.strokes.isNotEmpty()) { view.redo.add(view.strokes.removeAt(view.strokes.lastIndex)); view.invalidate(); revision++ }
                    DrawIcon(LucideIcons.Redo2.glyph(), "Redo", revision >= 0 && view.redo.isNotEmpty()) { view.strokes.add(view.redo.removeAt(view.redo.lastIndex)); view.invalidate(); revision++ }
                    DrawIcon(LucideIcons.Layers.glyph(), "Layers") { menu = if (menu == "Layers") null else "Layers" }
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
