package io.github.artcraftandroid.photocraft.studio

import android.graphics.Bitmap
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.input.pointer.PointerEventType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import io.github.artcraftandroid.photocraft.editor.Tool
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.withLock
import org.json.JSONArray
import org.json.JSONObject

/**
 * The document viewport.
 *
 * The reference put a fixed photograph here and painted over nothing. Here the picture is the
 * engine's composite of the open document, and the active tool's gestures reach the engine: a drag
 * paints a stroke *as it goes* — the engine renders the brush under the finger, so what the canvas
 * shows while the stroke is drawn is the stroke that commits, not a line standing in for one — and
 * a tap fills or samples, the Hand tool pans, the Zoom tool zooms.
 *
 * A stroke is one History step: the engine paints the preview out of band and adopts it when the
 * finger lifts (`stroke_begin` / `stroke_add` / `stroke_end`), so a five-second stroke is one
 * `Undo`, and the document is untouched until then.
 */
@Composable
fun StudioCanvas(
    engine: StudioEngine,
    state: StudioState,
    modifier: Modifier = Modifier,
    onPickedColour: (String) -> Unit,
    onFailure: (String) -> Unit,
) {
    val scope = rememberCoroutineScope()
    var viewport by remember { mutableStateOf(IntSize.Zero) }
    var cursor by remember { mutableStateOf<Offset?>(null) }
    // The reference's own artwork stands in until the document has pixels of its own, so the studio
    // looks like the design at rest; the first stroke replaces it with the engine's composite.
    var showReference by remember { mutableStateOf(true) }
    // The pixels the engine has painted for the stroke being drawn: the damaged rectangles, in
    // the order the dabs were laid. Each is the engine's composite of the whole stroke in that
    // rectangle, so drawing them in order shows the stroke exactly as it will commit.
    var parts by remember { mutableStateOf<List<LivePaint>>(emptyList()) }
    // The stroke's points as the canvas has them (the smoothing string, when the brush lags).
    var strokePoints by remember { mutableStateOf<List<Pair<Float, Float>>>(emptyList()) }
    val livePaint = parts
    val string = strokePoints

    val tool = remember(state.selectedTool, state.workspace) {
        Workspaces.of(state.workspace).engineTool(state.selectedTool)
    }
    val painting = tool in setOf(Tool.Brush, Tool.Pencil, Tool.Eraser)

    // The document's size follows the engine's (a new, opened or cropped document is a new size),
    // so the canvas' fit and the ring's scale are the document's own.
    LaunchedEffect(engine.width, engine.height) {
        state.docWidth = engine.width
        state.docHeight = engine.height
        engine.refreshPreview()
    }

    /** The document's rect in the view: the reference's cover-fit, then pan and zoom. */
    fun documentRect(): Rect {
        if (viewport.width == 0 || viewport.height == 0) return Rect.Zero
        val vw = viewport.width.toFloat()
        val vh = viewport.height.toFloat()
        // `object-fit: cover` over the viewport, as the reference sizes its artwork.
        val scale = maxOf(vw / state.docWidth.toFloat(), vh / state.docHeight.toFloat()) * state.zoom
        val w = state.docWidth * scale
        val h = state.docHeight * scale
        val cx = vw / 2f + state.panX
        val cy = vh / 2f + state.panY
        return Rect(cx - w / 2f, cy - h / 2f, cx + w / 2f, cy + h / 2f)
    }

    fun toDocument(p: Offset): Pair<Float, Float> {
        val r = documentRect()
        if (r.width <= 0f || r.height <= 0f) return 0f to 0f
        return ((p.x - r.left) / r.width * state.docWidth).coerceIn(0f, state.docWidth.toFloat()) to
            ((p.y - r.top) / r.height * state.docHeight).coerceIn(0f, state.docHeight.toFloat())
    }

    /** The command a painting tool commits as: the Pencil aliases, everything else is the brush. */
    fun strokeCommand(): String = if (tool == Tool.Pencil) "paint.pencil" else "paint.stroke"

    /** The stroke's params: the tool's colour, size and opacity, and the point it starts at. */
    fun strokeParams(first: Pair<Float, Float>): JSONObject {
        val p = JSONObject()
            .put("points", JSONArray().put(point(first, 1f, 0L)))
            .put("color", parseColour(state.value("colour")).toArgbHex())
            .put("size", (state.value("brushSize").toFloatOrNull() ?: 24f).toDouble())
            .put("opacity", (state.value("brushOpacity").toIntOrNull() ?: 100).coerceIn(1, 100))
        if (tool == Tool.Eraser) p.put("erase", true)
        return p
    }

    /** Fetch the pixels of a damage rect (JSON `{x, y, width, height}`) for the canvas. */
    suspend fun refreshLive(damage: String?) {
        val d = damage?.let { runCatching { JSONObject(it) }.getOrNull() } ?: return
        val x = d.optInt("x")
        val y = d.optInt("y")
        val w = d.optInt("width")
        val h = d.optInt("height")
        if (w <= 0 || h <= 0) return
        val frame = Rect(x.toFloat(), y.toFloat(), (x + w).toFloat(), (y + h).toFloat())
        // A long stroke lays many small rectangles; past a few dozen, fold them into one that
        // covers the stroke's whole reach, so a long stroke costs one bitmap, not one per frame.
        val kept = parts
        // The fold starts from this frame, so the union covers the dabs already drawn *and* the
        // new one; folding only the kept parts would drop a dab that fell outside their reach.
        val union = kept.fold(frame) { a, p -> Rect(minOf(a.left, p.doc.left), minOf(a.top, p.doc.top), maxOf(a.right, p.doc.right), maxOf(a.bottom, p.doc.bottom)) }
        val fetch = if (kept.size >= 24) union else frame
        val bmp = engine.strokePixels(
            fetch.left.toInt(), fetch.top.toInt(),
            (fetch.width.toInt()).coerceAtLeast(1), (fetch.height.toInt()).coerceAtLeast(1),
        ) ?: return
        parts = if (kept.size >= 24) listOf(LivePaint(fetch, bmp)) else kept + LivePaint(frame, bmp)
        state.docWidth = engine.width
        state.docHeight = engine.height
    }

    Box(
        modifier
            .onSizeChanged { viewport = it }
            .background(Sc.artworkBackdrop(state.workspace))
            // View tools: the hand drags, the zoom tool steps the zoom up per tap (its reference
            // behaviour), and a pinch zooms.
            .pointerInput(tool) {
                if (tool == Tool.Hand) {
                    detectDragGestures { change, drag ->
                        state.panX = (state.panX + drag.x).coerceIn(-400f, 400f)
                        state.panY = (state.panY + drag.y).coerceIn(-300f, 300f)
                        change.consume()
                    }
                }
            }
            .pointerInput(Unit) {
                // A pinch zooms the document, as the trackpad's does in the reference.
                detectTransformGestures { _, _, zoom, _ ->
                    if (zoom != 1f) state.zoom = (state.zoom * zoom).coerceIn(0.75f, 2.5f)
                }
            }
            .pointerInput(tool) {
                detectTapGestures(
                    onTap = { pos ->
                        when (tool) {
                            // The reference zooms a step per click, up to 250 %.
                            Tool.Zoom -> state.zoom = (state.zoom + 0.15f).coerceAtMost(2.5f)
                            Tool.Hand -> Unit
                            Tool.Type -> {
                                val (x, y) = toDocument(pos)
                                scope.engine {
                                    engine.createText(x, y, text = "Text", size = (state.value("fontSize").toFloatOrNull() ?: 64f))
                                    onFailure("A text layer was made; its words are set in the Text panel.")
                                }
                            }
                            else -> {
                                val (x, y) = toDocument(pos)
                                val colour = state.value("colour")
                                val size = state.value("brushSize").toFloatOrNull() ?: 24f
                                val opacity = state.value("brushOpacity").toIntOrNull() ?: 100
                                scope.launch {
                                    runCatching {
                                        val picked = engine.tap(tool, x, y, colour, size, opacity)
                                        if (picked != null) onPickedColour(picked)
                                    }.onFailure { onFailure(it.message ?: "The engine refused that.") }
                                }
                            }
                        }
                    },
                )
            }
            // Painting: the stroke is drawn as it goes, and is one History step when the finger
            // lifts. The points carry the pointer's pressure, so a stylus paints as its pressure
            // says (a finger reports 1.0, and the brush's curves read that as full).
            .pointerInput(tool, state.workspace) {
                if (!painting) return@pointerInput
                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = false)
                    val first = toDocument(down.position)
                    showReference = false
                    var pressure = down.pressure.takeIf { it > 0f } ?: 1f
                    // The stroke's clock: the engine's speed rules (spacing, build-up) use it.
                    val startMs = System.currentTimeMillis()
                    var points = listOf(first)
                    strokePoints = points
                    // The points go to one consumer, in order: a stroke's dabs must be laid in
                    // the order the finger drew them, and the input loop must not wait on them.
                    val queue = kotlinx.coroutines.channels.Channel<JSONArray>(kotlinx.coroutines.channels.Channel.UNLIMITED)
                    scope.launch {
                        // The engine's calls are serialised here: the damage rects are read back in
                        // the order the dabs were laid, so a frame never shows an older stroke.
                        val gate = kotlinx.coroutines.sync.Mutex()
                        suspend fun call(block: suspend () -> String?) = gate.withLock {
                            runCatching { refreshLive(block()) }
                                .onFailure { onFailure(it.message ?: "The stroke failed.") }
                        }
                        // Begin with the first point only: a stroke starts where the finger did,
                        // and the points after it come in through the queue.
                        val beginning = strokeParams(first)
                        val began = runCatching { engine.strokeBegin(strokeCommand(), beginning.toString()) }
                            .getOrElse {
                                onFailure(it.message ?: "The brush could not start.")
                                false
                            }
                        if (!began) {
                            strokePoints = emptyList()
                            return@launch
                        }
                        // The stroke's own first dab, drawn now: the canvas shows the brush under
                        // the finger from the first touch, not from the first move.
                        gate.withLock {
                            runCatching { refreshLive(engine.strokeBounds()) }
                        }
                        // The engine's brush wants time while the pointer is still (an airbrush's
                        // build-up, smoothing catching up): tick it while the stroke lasts.
                        val ticking = java.util.concurrent.atomic.AtomicBoolean(true)
                        val ticker = launch {
                            while (ticking.get()) {
                                kotlinx.coroutines.delay(33)
                                if (runCatching { engine.strokeWantsTime() }.getOrDefault(false)) {
                                    gate.withLock { runCatching { refreshLive(engine.strokeTick()) } }
                                }
                            }
                        }
                        try {
                            for (batch in queue) {
                                // The first point is already in the stroke (its params carried it).
                                call { engine.strokeAdd(batch.toString()) }
                            }
                        } finally {
                            ticking.set(false)
                            ticker.cancel()
                            gate.withLock {
                                // The commit: the engine takes the pixels it previewed, so this
                                // stroke is one History step and nothing is re-rendered. The
                                // engine's refresh runs inside `strokeEnd`, so the composite it
                                // reads back already has the stroke in it.
                                val ended = runCatching { engine.strokeEnd() }
                                if (ended.isFailure) {
                                    runCatching { engine.strokeCancel() }
                                    onFailure(ended.exceptionOrNull()?.message ?: "The stroke failed.")
                                }
                                // The composite first, then the preview dropped: a frame between
                                // the two must not show the canvas without the stroke it has just
                                // committed. That ordering is `strokeEnd`'s refresh, which has
                                // landed by here.
                                parts = emptyList()
                                strokePoints = emptyList()
                            }
                        }
                    }
                    try {
                        var pending = JSONArray()
                        fun flush() {
                            if (pending.length() == 0) return
                            queue.trySend(pending)
                            pending = JSONArray()
                        }
                        while (true) {
                            val event = awaitPointerEvent()
                            val change = event.changes.firstOrNull { it.id == down.id } ?: break
                            if (!change.pressed) break
                            val at = System.currentTimeMillis() - startMs
                            pressure = change.pressure.takeIf { it > 0f } ?: pressure
                            val p = toDocument(change.position)
                            // Only real travel joins the stroke: a held finger does not add a
                            // point per frame (the engine's airbrush lays its own dabs over time).
                            val previous = points.last()
                            val dx = p.first - previous.first
                            val dy = p.second - previous.second
                            if (dx * dx + dy * dy >= 0.25f) {
                                points = points + p
                                strokePoints = points
                                pending.put(point(p, pressure, at))
                                // A batch per frame's worth of input: small enough to stay
                                // responsive, large enough that the engine paints a stroke.
                                if (pending.length() >= 6) flush()
                            }
                            change.consume()
                        }
                        flush()
                    } finally {
                        // Closing the queue lets the consumer finish the stroke: it is one
                        // History step, and the canvas clears its preview when it lands.
                        queue.close()
                    }
                }
            }
            // Vector shapes: a drag draws the shape's box, and the engine makes the shape layer.
            // (The reference had a Rectangle tool drawn as an icon only.)
            .pointerInput(tool, state.workspace, state.selectedTool) {
                if (state.workspace != WorkspaceId.Vector || state.selectedTool !in setOf("rectangle", "ellipse")) return@pointerInput
                val kind = if (state.selectedTool == "ellipse") "ellipse" else "rect"
                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = false)
                    val start = toDocument(down.position)
                    var last = start
                    while (true) {
                        val event = awaitPointerEvent()
                        val change = event.changes.firstOrNull { it.id == down.id } ?: break
                        if (!change.pressed) break
                        last = toDocument(change.position)
                        change.consume()
                    }
                    run {
                        val x = minOf(start.first, last.first)
                        val y = minOf(start.second, last.second)
                        val w = kotlin.math.abs(last.first - start.first)
                        val h = kotlin.math.abs(last.second - start.second)
                        // A tap makes a shape at the default size; a drag makes the box drawn.
                        val (bw, bh) = if (w < 3f || h < 3f) 160f to 120f else w to h
                        val colour = state.value("colour")
                        // The Vector workspace's tools draw outlines: a shape is a fill and a
                        // stroke, and the stroke is what a person sees while it is being drawn.
                        scope.engine {
                            engine.createShape(kind, x, y, bw, bh, colour, strokeOnly = true)
                        }
                    }
                }
            }
            // The brush ring follows the pointer.
            .pointerInput(Unit) {
                awaitPointerEventScope {
                    while (true) {
                        val event = awaitPointerEvent()
                        if (event.type == PointerEventType.Exit) {
                            cursor = null
                        } else {
                            cursor = event.changes.lastOrNull()?.position
                        }
                    }
                }
            },
    ) {
        // A blank document still shows the reference's picture, as the design does at rest; the
        // first stroke replaces it with the engine's pixels.
        if (showReference && engine.layers.isEmpty()) {
            WorkspaceArtwork(state.workspace, Modifier.fillMaxSize())
        }
        val bitmap = engine.preview
        val cursorPoint = cursor
        val paint = livePaint
        val ring = ((state.value("brushSize").toFloatOrNull() ?: 24f) / state.docWidth.toFloat())
        Canvas(Modifier.fillMaxSize()) {
            val r = documentRect()
            if (r.width <= 0f || r.height <= 0f) return@Canvas
            if (bitmap != null) {
                drawImage(
                    image = bitmap.asImageBitmap(),
                    srcOffset = IntOffset.Zero,
                    srcSize = IntSize(bitmap.width, bitmap.height),
                    dstOffset = IntOffset(r.left.toInt(), r.top.toInt()),
                    dstSize = IntSize(
                        r.width.toInt().coerceAtLeast(1),
                        r.height.toInt().coerceAtLeast(1),
                    ),
                    filterQuality = FilterQuality.Medium,
                )
            }
            // The stroke so far, from the engine itself: the brush's own dabs, not a line.
            for (part in paint) {
                val sx = r.left + part.doc.left / state.docWidth * r.width
                val sy = r.top + part.doc.top / state.docHeight * r.height
                val sw = (part.doc.right - part.doc.left) / state.docWidth * r.width
                val sh = (part.doc.bottom - part.doc.top) / state.docHeight * r.height
                drawImage(
                    image = part.bitmap.asImageBitmap(),
                    srcOffset = IntOffset.Zero,
                    srcSize = IntSize(part.bitmap.width, part.bitmap.height),
                    dstOffset = IntOffset(sx.toInt(), sy.toInt()),
                    dstSize = IntSize(sw.toInt().coerceAtLeast(1), sh.toInt().coerceAtLeast(1)),
                    filterQuality = FilterQuality.Low,
                )
            }
            // The document's grid, as the reference's View menu draws it.
            if (state.isOn("grid")) {
                val step = 40f * state.zoom
                var x = r.left
                while (x < r.right) {
                    drawLine(Color(0x38008DBE), Offset(x, r.top), Offset(x, r.bottom), 1f)
                    x += step
                }
                var y = r.top
                while (y < r.bottom) {
                    drawLine(Color(0x38008DBE), Offset(r.left, y), Offset(r.right, y), 1f)
                    y += step
                }
            }
            // Until the engine's first pixels arrive, the stroke's path stands in for them.
            if (paint.isEmpty() && string.size > 1) {
                val path = androidx.compose.ui.graphics.Path()
                string.forEachIndexed { i, (x, y) ->
                    val px = r.left + x / state.docWidth * r.width
                    val py = r.top + y / state.docHeight * r.height
                    if (i == 0) path.moveTo(px, py) else path.lineTo(px, py)
                }
                drawPath(
                    path,
                    parseColour(state.value("colour")).copy(alpha = 0.7f),
                    style = Stroke(
                        width = ((state.value("brushSize").toFloatOrNull() ?: 24f) / state.docWidth * r.width)
                            .coerceAtLeast(1.2f),
                        cap = StrokeCap.Round,
                        join = StrokeJoin.Round,
                    ),
                )
            }
            // The tool's ring, at the pointer.
            if (cursorPoint != null && painting) {
                val d = (ring * r.width).coerceIn(4f, 260f)
                drawCircle(Color.White.copy(alpha = 0.9f), d / 2f, cursorPoint, style = Stroke(1f))
                drawCircle(Color.Black.copy(alpha = 0.4f), d / 2f + 1f, cursorPoint, style = Stroke(1f))
            }
        }
    }
}

/** The engine's pixels for the stroke being drawn, over the document rect they cover. */
private data class LivePaint(val doc: Rect, val bitmap: Bitmap)

/** One pointer sample as the engine's wire form: `[x, y, pressure, tiltX, tiltY, rotation, time, wheel]`. */
private fun point(p: Pair<Float, Float>, pressure: Float, timeMs: Long): JSONArray =
    JSONArray().put(p.first.toDouble()).put(p.second.toDouble()).put(pressure.toDouble().coerceIn(0.0, 1.0))
        .put(0.0).put(0.0).put(0.0).put(timeMs.toDouble()).put(0.0)

/** A colour as `#rrggbb`, as the engine's commands take one. */
private fun Color.toArgbHex(): String {
    val r = (red * 255f).toInt().coerceIn(0, 255)
    val g = (green * 255f).toInt().coerceIn(0, 255)
    val b = (blue * 255f).toInt().coerceIn(0, 255)
    return String.format("#%02X%02X%02X", r, g, b)
}
