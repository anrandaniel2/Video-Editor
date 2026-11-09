package io.github.artcraftandroid.filmcraft.ui.editor

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import io.github.artcraftandroid.filmcraft.core.MaskPath
import io.github.artcraftandroid.filmcraft.core.MaskPoint
import io.github.artcraftandroid.filmcraft.core.MaskShape
import io.github.artcraftandroid.filmcraft.core.MaskState
import io.github.artcraftandroid.filmcraft.core.MaskTransform
import io.github.artcraftandroid.filmcraft.core.MaskVertex
import io.github.artcraftandroid.filmcraft.ui.Fc
import kotlin.math.hypot

private val MaskTransform.matrix: DoubleArray get() = doubleArrayOf(a, b, c, d, e, f)
private val MaskVertex.xy: Pair<Double, Double> get() = point.x to point.y

private fun MaskState.screen(p: MaskPoint, k: Float): Offset {
    val (x, y) = MaskGeometry.apply(transform.matrix, p.x, p.y)
    return Offset((x * k).toFloat(), (y * k).toFloat())
}

private fun MaskState.source(p: Offset, k: Float, inverse: DoubleArray): Pair<Double, Double> =
    MaskGeometry.apply(inverse, p.x.toDouble() / k, p.y.toDouble() / k)

private fun composePath(path: MaskPath, state: MaskState, k: Float): Path = Path().apply {
    val vertices = path.vertices
    if (vertices.isEmpty()) return@apply
    val first = state.screen(vertices.first().point, k)
    moveTo(first.x, first.y)
    for (i in vertices.indices) {
        val a = vertices[i]
        val b = vertices[(i + 1) % vertices.size]
        if (!path.closed && i == vertices.lastIndex) break
        val c1 = state.screen(MaskPoint(a.point.x + a.handleOut.x, a.point.y + a.handleOut.y), k)
        val c2 = state.screen(MaskPoint(b.point.x + b.handleIn.x, b.point.y + b.handleIn.y), k)
        val end = state.screen(b.point, k)
        cubicTo(c1.x, c1.y, c2.x, c2.y, end.x, end.y)
    }
    if (path.closed) close()
}

/** A modest flattening for touch hit tests; rendering and export use the engine's exact curves. */
private fun hitPolygon(path: MaskPath, state: MaskState, k: Float): List<Pair<Double, Double>> {
    val out = ArrayList<Pair<Double, Double>>()
    val vs = path.vertices
    for (i in vs.indices) {
        val a = vs[i]; val b = vs[(i + 1) % vs.size]
        val p0 = state.screen(a.point, k)
        val p1 = state.screen(MaskPoint(a.point.x + a.handleOut.x, a.point.y + a.handleOut.y), k)
        val p2 = state.screen(MaskPoint(b.point.x + b.handleIn.x, b.point.y + b.handleIn.y), k)
        val p3 = state.screen(b.point, k)
        for (j in 0 until 8) {
            val t = j / 8.0; val u = 1.0 - t
            val x = u * u * u * p0.x + 3 * u * u * t * p1.x + 3 * u * t * t * p2.x + t * t * t * p3.x
            val y = u * u * u * p0.y + 3 * u * u * t * p1.y + 3 * u * t * t * p2.y + t * t * t * p3.y
            out.add(x.toDouble() to y.toDouble())
        }
    }
    return out
}

private fun translated(path: MaskPath, dx: Double, dy: Double): MaskPath = path.copy(
    vertices = path.vertices.map { it.copy(point = MaskPoint(it.point.x + dx, it.point.y + dy)) },
)

private fun scaled(path: MaskPath, anchor: Pair<Double, Double>, factor: Double): MaskPath {
    fun point(p: MaskPoint) = MaskPoint(anchor.first + (p.x - anchor.first) * factor, anchor.second + (p.y - anchor.second) * factor)
    fun vector(p: MaskPoint) = MaskPoint(p.x * factor, p.y * factor)
    return path.copy(vertices = path.vertices.map { MaskVertex(point(it.point), vector(it.handleIn), vector(it.handleOut)) })
}

private sealed interface Hit {
    data class Vertex(val index: Int) : Hit
    data class Corner(val index: Int) : Hit
    data object Body : Hit
    data object None : Hit
}

private fun corners(bounds: DoubleArray): List<Offset> = listOf(
    Offset(bounds[0].toFloat(), bounds[1].toFloat()), Offset(bounds[2].toFloat(), bounds[1].toFloat()),
    Offset(bounds[2].toFloat(), bounds[3].toFloat()), Offset(bounds[0].toFloat(), bounds[3].toFloat()),
)

private fun hitTest(path: MaskPath, state: MaskState, k: Float, p: Offset, radius: Float): Hit {
    val vs = path.vertices.map { state.screen(it.point, k) }
    vs.forEachIndexed { i, q -> if ((q - p).getDistance() <= radius) return Hit.Vertex(i) }
    val poly = hitPolygon(path, state, k)
    val box = MaskGeometry.bounds(poly) ?: return Hit.None
    corners(box).forEachIndexed { i, q -> if ((q - p).getDistance() <= radius) return Hit.Corner(i) }
    return if (MaskGeometry.contains(poly, p.x.toDouble(), p.y.toDouble())) Hit.Body else Hit.None
}

/** Transparent touch layer over the program picture. All committed geometry goes back to Rust. */
@Composable
fun MaskOverlay(ed: EditorController, state: MaskState, frameWidth: Float, modifier: Modifier = Modifier) {
    var stroke by remember { mutableStateOf<List<Offset>>(emptyList()) }
    Canvas(
        modifier
            .testTag("mask_overlay")
            .semantics { contentDescription = "Mask editor preview" }
            .pointerInput(ed.selectedMaskIndex, ed.maskDrawing, ed.maskAddingPoint, state.revision, state.time, state.transform, frameWidth) {
                val k = size.width / frameWidth.coerceAtLeast(1f)
                val inverse = MaskGeometry.invert(state.transform.matrix) ?: return@pointerInput
                val radius = 20.dp.toPx()
                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = false)
                    val start = down.position
                    val mask = state.masks.firstOrNull { it.index == ed.selectedMaskIndex }
                    val original = mask?.path
                    val polygon = original?.let { hitPolygon(it, state, k) }.orEmpty()
                    val box = MaskGeometry.bounds(polygon)
                    val hit = if (original != null && !ed.maskDrawing && !ed.maskAddingPoint) hitTest(original, state, k, start, radius) else Hit.None
                    var moved = false
                    val token = ed.newMaskGesture()
                    ed.beginMaskGesture()
                    try {
                        while (true) {
                            val event = awaitPointerEvent()
                            if (event.changes.none { it.pressed }) break
                            val current = event.changes.firstOrNull { it.pressed }?.position ?: event.changes.first().position
                            if ((current - start).getDistance() > viewConfiguration.touchSlop / 2) moved = true
                            if (ed.maskDrawing) {
                                if (moved) stroke = if (stroke.isEmpty()) listOf(start, current) else stroke + current
                                event.changes.forEach { it.consume() }
                                continue
                            }
                            if (moved && original != null && mask != null) {
                                val a = state.source(start, k, inverse)
                                val b = state.source(current, k, inverse)
                                val next = when (hit) {
                                    is Hit.Vertex -> original.copy(vertices = original.vertices.mapIndexed { i, v ->
                                        if (i == hit.index) v.copy(point = MaskPoint(v.point.x + b.first - a.first, v.point.y + b.second - a.second)) else v
                                    })
                                    is Hit.Corner -> {
                                        val cs = box?.let { corners(it) }.orEmpty()
                                        val anchor = cs.getOrNull((hit.index + 2) % 4) ?: start
                                        val from = state.source(anchor, k, inverse)
                                        val d0 = hypot((start.x - anchor.x).toDouble(), (start.y - anchor.y).toDouble()).coerceAtLeast(1.0)
                                        val d1 = hypot((current.x - anchor.x).toDouble(), (current.y - anchor.y).toDouble())
                                        scaled(original, from, (d1 / d0).coerceIn(0.02, 50.0))
                                    }
                                    Hit.Body -> translated(original, b.first - a.first, b.second - a.second)
                                    Hit.None -> original
                                }
                                if (next != original) ed.setMaskPath(next, token)
                                event.changes.forEach { it.consume() }
                            }
                        }
                        if (ed.maskDrawing) {
                            val simple = MaskGeometry.simplify(stroke.map { it.x.toDouble() to it.y.toDouble() }, 3.0)
                            if (simple.size >= 3 && MaskGeometry.area(simple) > 1.0) {
                                ed.addMask(MaskShape.POLYGON, simple.map { state.source(Offset(it.first.toFloat(), it.second.toFloat()), k, inverse) })
                            } else if (stroke.isNotEmpty()) {
                                ed.tellUser("Draw a larger closed shape for the mask.")
                            }
                            stroke = emptyList()
                        } else if (!moved && mask != null && original != null) {
                            if (ed.maskAddingPoint) {
                                val p = state.source(start, k, inverse)
                                val edge = MaskGeometry.nearestEdge(original.vertices.map { it.xy }, p.first, p.second)
                                if (edge >= 0) ed.addMaskVertex(edge, p)
                            } else if (hit is Hit.Vertex) {
                                ed.selectedMaskVertex = hit.index
                            } else {
                                // A tap can also select another mask beneath the finger.
                                state.masks.lastOrNull { MaskGeometry.contains(hitPolygon(it.path, state, k), start.x.toDouble(), start.y.toDouble()) }
                                    ?.let { ed.selectMask(it.index) }
                            }
                        }
                    } finally {
                        stroke = emptyList()
                        ed.endMaskGesture()
                    }
                }
            },
    ) {
        val k = size.width / frameWidth.coerceAtLeast(1f)
        state.masks.forEach { mask ->
            val active = mask.index == ed.selectedMaskIndex
            val path = composePath(mask.path, state, k)
            drawPath(path, Color.Black.copy(alpha = 0.65f), style = Stroke(4.dp.toPx()))
            drawPath(path, if (active) Color.White else Fc.Detail, style = Stroke(if (active) 2.dp.toPx() else 1.dp.toPx()))
            if (active) {
                val points = hitPolygon(mask.path, state, k)
                MaskGeometry.bounds(points)?.let { bounds ->
                    corners(bounds).forEach { p ->
                        drawRect(Fc.Panel, Offset(p.x - 5.dp.toPx(), p.y - 5.dp.toPx()), Size(10.dp.toPx(), 10.dp.toPx()))
                        drawRect(Color.White, Offset(p.x - 5.dp.toPx(), p.y - 5.dp.toPx()), Size(10.dp.toPx(), 10.dp.toPx()), style = Stroke(1.5.dp.toPx()))
                    }
                }
                mask.path.vertices.forEachIndexed { i, v ->
                    val p = state.screen(v.point, k)
                    drawCircle(if (i == ed.selectedMaskVertex) Fc.Accent else Fc.Panel, 5.dp.toPx(), p)
                    drawCircle(Color.White, 5.dp.toPx(), p, style = Stroke(1.5.dp.toPx()))
                }
            }
        }
        if (stroke.size > 1) {
            val draft = Path().apply {
                moveTo(stroke.first().x, stroke.first().y)
                stroke.drop(1).forEach { lineTo(it.x, it.y) }
            }
            drawPath(draft, Fc.AccentLight, style = Stroke(2.5.dp.toPx()))
        }
    }
}
