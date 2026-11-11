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
private class DrawingView(context: android.content.Context) : View(context) {
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
            val saved = canvas.saveLayer(0f, 0f, 1024f, 1024f, null)
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
        canvas.drawColor(android.graphics.Color.LTGRAY)
        canvas.save(); canvas.translate(dx, dy); canvas.scale(width / 1024f * zoom, width / 1024f * zoom)
        paint.reset(); paint.color = android.graphics.Color.WHITE
        canvas.drawRect(0f, 0f, 1024f, 1024f, paint)
        render(canvas); canvas.restore()
    }
    fun bitmap(): Bitmap = Bitmap.createBitmap(1024, 1024, Bitmap.Config.ARGB_8888).also { render(Canvas(it)) }
    override fun onTouchEvent(event: MotionEvent): Boolean {
        parent?.requestDisallowInterceptTouchEvent(true)
        if (event.pointerCount > 1) {
            if (!navigating) { active?.let { strokes.remove(it) }; active = null; navigating = true; oldSpan = 0f }
            val x = (event.getX(0) + event.getX(1)) / 2
            val y = (event.getY(0) + event.getY(1)) / 2
            val span = kotlin.math.hypot(event.getX(0) - event.getX(1), event.getY(0) - event.getY(1))
            if (oldSpan > 0 && event.actionMasked == MotionEvent.ACTION_MOVE) {
                val next = (zoom * span / oldSpan).coerceIn(.25f, 8f)
                dx = x - (oldX - dx) * next / zoom; dy = y - (oldY - dy) * next / zoom; zoom = next
            }
            oldSpan = span; oldX = x; oldY = y; invalidate(); return true
        }
        if (navigating) { if (event.actionMasked == MotionEvent.ACTION_UP) navigating = false; return true }
        val scale = width / 1024f * zoom
        val x = ((event.x - dx) / scale).coerceIn(0f, 1024f)
        val y = ((event.y - dy) / scale).coerceIn(0f, 1024f)
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                redo.clear()
                active = Ink(Path().apply { moveTo(x, y); lineTo(x + .01f, y) }, inkColor, if (brush == "Pencil") brushSize * .4f else brushSize, (opacity * 255).toInt(), brush, layer)
                strokes.add(active!!)
            }
            MotionEvent.ACTION_MOVE -> active?.path?.lineTo(x, y)
            MotionEvent.ACTION_UP -> active = null
            MotionEvent.ACTION_CANCEL -> { active?.let { strokes.remove(it) }; active = null }
        }
        invalidate(); return true
    }
}

@Composable
internal fun DrawingPanel(ed: EditorController, close: () -> Unit) {
    val context = androidx.compose.ui.platform.LocalContext.current
    val view = remember { DrawingView(context) }
    var color by remember { mutableIntStateOf(android.graphics.Color.BLACK) }
    var size by remember { mutableFloatStateOf(12f) }
    var opacity by remember { mutableFloatStateOf(1f) }
    var layer by remember { mutableIntStateOf(0) }
    var layers by remember { mutableIntStateOf(1) }
    var brush by remember { mutableStateOf("Pen") }
    Dialog(onDismissRequest = close, properties = DialogProperties(usePlatformDefaultWidth = false)) {
        Surface(Modifier.fillMaxSize()) {
            Column(Modifier.verticalScroll(rememberScrollState()).padding(12.dp)) {
                Row { TextButton(onClick = close) { Text("Cancel") }; TextButton(onClick = { ed.importDrawing(view.bitmap()) }, enabled = ed.busy == null) { Text("Add drawing") } }
                Text("1024 × 1024 • transparent export • pinch to zoom, two fingers to pan")
                AndroidView(factory = { view }, modifier = Modifier.fillMaxWidth().height(340.dp))
                Row {
                    TextButton(onClick = { if (view.strokes.isNotEmpty()) view.redo.add(view.strokes.removeAt(view.strokes.lastIndex)); view.invalidate() }) { Text("Undo") }
                    TextButton(onClick = { if (view.redo.isNotEmpty()) view.strokes.add(view.redo.removeAt(view.redo.lastIndex)); view.invalidate() }) { Text("Redo") }
                    TextButton(onClick = { if (layers < 16) { layers++; layer = layers - 1; view.layers = layers; view.layer = layer } }) { Text("+ Layer") }
                    TextButton(onClick = { layer = (layer + 1) % layers; view.layer = layer }) { Text("Layer ${layer + 1}/$layers") }
                }
                Row { listOf("Pen", "Pencil", "Marker", "Airbrush", "Eraser").forEach { name -> TextButton(onClick = { brush = name; view.brush = name }, contentPadding = PaddingValues(3.dp)) { Text(if (brush == name) "•$name" else name) } } }
                Text("Brush size ${size.toInt()}"); Slider(size, { size = it; view.brushSize = it }, valueRange = 1f..100f)
                Text("Opacity ${(opacity * 100).toInt()}%"); Slider(opacity, { opacity = it; view.opacity = it }, valueRange = .01f..1f)
                ColorPicker("Brush color", color) { color = it; view.inkColor = it }
            }
        }
    }
}
