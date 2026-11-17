package io.github.artcraftandroid.photocraft.studio

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.unit.IntSize
import io.github.artcraftandroid.photocraft.editor.Tool
import kotlinx.coroutines.launch

/**
 * The document viewport.
 *
 * The reference put a fixed photograph here and painted over nothing. Here the picture is the
 * engine's composite of the open document, and the active tool's gestures reach the engine: a drag
 * paints one stroke, a tap fills or samples, the Hand tool pans, the Zoom tool zooms. On a document
 * smaller than the view the composite is fitted; the reference's own artwork shows underneath only
 * when the host opened a blank canvas, so the studio still looks like the design at rest.
 */
@Composable
fun StudioCanvas(
    engine: StudioEngine,
    state: StudioState,
    modifier: Modifier = Modifier,
    onPickedColour: (String) -> Unit,
    onFailure: (String) -> Unit,
    onPlacePicture: () -> Unit = {},
) {
    val scope = rememberCoroutineScope()
    var viewport by remember { mutableStateOf(IntSize.Zero) }
    var cursor by remember { mutableStateOf<Offset?>(null) }
    var strokePoints by remember { mutableStateOf<List<Pair<Float, Float>>>(emptyList()) }
    // The live stroke and the pointer are read outside the draw scope, as Compose requires.
    val liveStroke = strokePoints

    val tool = remember(state.selectedTool, state.workspace) {
        Workspaces.of(state.workspace).engineTool(state.selectedTool)
    }
    val painting = tool in setOf(Tool.Brush, Tool.Pencil, Tool.Eraser)
    // The workspaces that keep their shapes: Vector draws rectangles and ellipses as geometry, and
    // Layout draws picture frames. The reference's own tools are named by id, and those ids are
    // what says which of the four the drag makes.
    val vectorTool: VectorTool? = when (state.workspace) {
        WorkspaceId.Vector -> when (state.selectedTool) {
            "rectangle" -> VectorTool.Rectangle
            "ellipse" -> VectorTool.Ellipse
            // The Pen was mapped to the pencil, so its drag painted. In this workspace it draws
            // a path.
            "pen" -> VectorTool.Pen
            else -> null
        }
        WorkspaceId.Layout -> when (state.selectedTool) {
            "rectangle", "table" -> VectorTool.Rectangle
            "frame" -> VectorTool.FrameRect
            "ellipse" -> VectorTool.FrameEllipse
            else -> null
        }
        else -> null
    }

    // Follow the engine's document: a new composite whenever its revision moves. `preview` and
    // `status` are Compose state on the engine, so the canvas redraws when the stroke lands --
    // without that it only redrew when the drag itself changed something, which is why a stroke
    // appeared to vanish until the next one was drawn.
    val revision = engine.status?.revision
    LaunchedEffect(revision, viewport) {
        engine.refreshPreview()
        state.docWidth = engine.width
        state.docHeight = engine.height
    }

    // Open a document the first time the studio is shown: the panels and the canvas all act on
    // one, and without it every tool answers "Open a picture first".
    LaunchedEffect(Unit) {
        if (!engine.hasDocument && viewport.width > 0) {
            runCatching { engine.newDocument(state.docWidth, state.docHeight) }
        }
    }

    /** The document's rect in the view: the reference's cover-fit, then pan and zoom. */
    fun documentRect(): androidx.compose.ui.geometry.Rect {
        if (viewport.width == 0 || viewport.height == 0) return androidx.compose.ui.geometry.Rect.Zero
        val vw = viewport.width.toFloat()
        val vh = viewport.height.toFloat()
        // `object-fit: cover` over the viewport, as the reference sizes its artwork.
        val scale = maxOf(vw / state.docWidth.toFloat(), vh / state.docHeight.toFloat()) * state.zoom
        val w = state.docWidth * scale
        val h = state.docHeight * scale
        val cx = vw / 2f + state.panX
        val cy = vh / 2f + state.panY
        return androidx.compose.ui.geometry.Rect(cx - w / 2f, cy - h / 2f, cx + w / 2f, cy + h / 2f)
    }

    fun toDocument(p: Offset): Pair<Float, Float> {
        val r = documentRect()
        if (r.width <= 0f || r.height <= 0f) return 0f to 0f
        return ((p.x - r.left) / r.width * state.docWidth).coerceIn(0f, state.docWidth.toFloat()) to
            ((p.y - r.top) / r.height * state.docHeight).coerceIn(0f, state.docHeight.toFloat())
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
                            // The Layout workspace's Place Image tool: a tap is how a picture
                            // gets onto the page, rather than reaching for a menu.
                            Tool.Place -> onPlacePicture()
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
            // A vector shape: the drag makes geometry, not paint, in the workspaces that keep
            // one. Which shape comes from the tab that is selected, not from the engine tool --
            // a frame and a rectangle are the same drag with different consequences.
            .pointerInput(tool, state.workspace, state.selectedTool) {
                if (vectorTool != null) {
                    detectDragGestures(
                        onDragStart = { pos -> strokePoints = listOf(toDocument(pos)) },
                        onDragEnd = {
                            val points = strokePoints
                            strokePoints = emptyList()
                            if (points.size > 1) {
                                val first = points.first()
                                val last = points.last()
                                val colour = state.value("colour")
                                val size = state.value("brushSize").toFloatOrNull() ?: 24f
                                val outline = state.value("outlineColour").ifEmpty { "#000000" }
                                val stroke = state.value("shapeStrokeWidth").toFloatOrNull() ?: 0f
                                scope.launch {
                                    runCatching {
                                        when (vectorTool) {
                                            VectorTool.Pen -> engine.penPath(points, colour, outline, stroke)
                                            VectorTool.FrameRect -> engine.frameDrag(false, first, last)
                                            VectorTool.FrameEllipse -> engine.frameDrag(true, first, last)
                                            VectorTool.Ellipse ->
                                                engine.shapeDrag("ellipse", first, last, colour, outline, stroke)
                                            else -> engine.shapeDrag("rectangle", first, last, colour, outline, stroke)
                                        }
                                    }.onFailure { onFailure(it.message ?: "The shape failed.") }
                                }
                            }
                        },
                        onDragCancel = { strokePoints = emptyList() },
                    ) { change, _ ->
                        strokePoints = strokePoints + toDocument(change.position)
                        change.consume()
                    }
                }
            }
            // Selecting: the marquee, the lasso and the Move tool's box all take a drag. They were
            // inert, which is most of why the Pixel and Vector workspaces felt dead.
            .pointerInput(tool) {
                if ((tool == Tool.Rect || tool == Tool.Lasso || tool == Tool.Move) && vectorTool == null) {
                    detectDragGestures(
                        onDragStart = { pos -> strokePoints = listOf(toDocument(pos)) },
                        onDragEnd = {
                            val points = strokePoints
                            strokePoints = emptyList()
                            if (points.size > 1) {
                                scope.launch {
                                    runCatching { engine.selectByDrag(tool, points) }
                                        .onFailure { onFailure(it.message ?: "The selection failed.") }
                                }
                            }
                        },
                        onDragCancel = { strokePoints = emptyList() },
                    ) { change, _ ->
                        strokePoints = strokePoints + toDocument(change.position)
                        change.consume()
                    }
                }
            }
            // Painting: one drag, one stroke, one History entry. A vector tool owns the drag in
            // its workspace, so painting stays out of the way even where the two map to the same
            // engine tool.
            .pointerInput(tool) {
                if (painting && vectorTool == null) {
                    detectDragGestures(
                        onDragStart = { pos -> strokePoints = listOf(toDocument(pos)) },
                        onDragEnd = {
                            val points = strokePoints
                            strokePoints = emptyList()
                            if (points.size > 1) {
                                val colour = state.value("colour")
                                val size = state.value("brushSize").toFloatOrNull() ?: 24f
                                val opacity = state.value("brushOpacity").toIntOrNull() ?: 100
                                scope.launch {
                                    runCatching { engine.stroke(points, colour, size, opacity, tool) }
                                        .onFailure { onFailure(it.message ?: "The stroke failed.") }
                                }
                            }
                        },
                        onDragCancel = { strokePoints = emptyList() },
                    ) { change, _ ->
                        strokePoints = strokePoints + toDocument(change.position)
                        change.consume()
                    }
                }
            }
            // The brush ring follows the pointer.
            .pointerInput(Unit) {
                awaitPointerEventScope {
                    while (true) {
                        cursor = awaitPointerEvent().changes.lastOrNull()?.position
                    }
                }
            },
    ) {
        val bitmap = engine.preview
        val cursorPoint = cursor
        val ring = ((state.value("brushSize").toFloatOrNull() ?: 24f) / state.docWidth.toFloat())
        Canvas(Modifier.fillMaxSize()) {
            val r = documentRect()
            if (r.width <= 0f || r.height <= 0f) return@Canvas
            if (bitmap != null) {
                drawImage(
                    image = bitmap.asImageBitmap(),
                    srcOffset = androidx.compose.ui.unit.IntOffset.Zero,
                    srcSize = androidx.compose.ui.unit.IntSize(bitmap.width, bitmap.height),
                    dstOffset = androidx.compose.ui.unit.IntOffset(r.left.toInt(), r.top.toInt()),
                    dstSize = androidx.compose.ui.unit.IntSize(
                        r.width.toInt().coerceAtLeast(1),
                        r.height.toInt().coerceAtLeast(1),
                    ),
                    filterQuality = FilterQuality.Medium,
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
            // The live stroke, in the painting colour.
            if (liveStroke.size > 1) {
                val path = Path()
                liveStroke.forEachIndexed { i, (x, y) ->
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
