package io.github.artcraftandroid.filmcraft.ui.editor

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.unit.dp
import io.github.artcraftandroid.filmcraft.ui.SelectChip
import kotlin.math.*

/** Lights use the same projected cone/cylinder axes and rotation rings as object gizmos. */
@Composable
internal fun LightGizmoOverlay(ed: EditorController, fw: Float, fh: Float) {
    val lights by rememberUpdatedState(ed.sceneLights.lights)
    var mode by remember { mutableStateOf(GizmoMode.Move) }
    var active by remember { mutableIntStateOf(-1) }
    fun geometry(light: SceneLight, w: Double, h: Double, pixels: Double): GizmoGeometry {
        val z = light.position[2].coerceAtLeast(1.0)
        val pose = GizmoPose(fw / 2.0 + fh * (light.position[0] - fw / 2.0) / z,
            fh / 2.0 + fh * (light.position[1] - fh / 2.0) / z, z)
        return GizmoGeometry(pose, null, fw.toDouble(), fh.toDouble(), w, h, pixels)
    }
    Box(Modifier.fillMaxSize()) {
        Canvas(Modifier.fillMaxSize().pointerInput(fw, fh, mode) {
            fun g(light: SceneLight) = geometry(light, size.width.toDouble(), size.height.toDouble(), minOf(62.dp.toPx(), minOf(size.width, size.height) * .28f).toDouble())
            awaitEachGesture {
                val down = awaitFirstDown(requireUnconsumed = false)
                val selected = lights.firstOrNull { it.id == ed.selectedLightId }
                val axis = selected?.let { g(it).hit(down.position, mode, 16.dp.toPx()) } ?: -1
                val hit = if (axis >= 0) selected else lights.minByOrNull { (g(it).centre - down.position).getDistance() }
                    ?.takeIf { (g(it).centre - down.position).getDistance() < 22.dp.toPx() }
                if (hit != null) {
                    ed.stop(); ed.selectedLightId = hit.id; down.consume(); active = axis
                    var current: SceneLight = requireNotNull(hit); var previous = down.position
                    var total = Offset.Zero; var changed = false
                    val token = "light-gizmo:${java.util.UUID.randomUUID()}"
                    while (true) {
                        val event = awaitPointerEvent()
                        if (event.changes.count { it.pressed } > 1) break
                        val change = event.changes.firstOrNull { it.id == down.id } ?: break
                        if (!change.pressed) { change.consume(); break }
                        val delta = change.position - previous; previous = change.position; total += delta
                        // The light body selects only; translation/rotation requires a real axis.
                        if (axis < 0) continue
                        if (!changed && total.getDistance() < viewConfiguration.touchSlop) continue
                        val drag = if (changed) delta else total
                        val geo = g(current)
                        if (mode == GizmoMode.Move) {
                            val tangent = geo.tangent(axis); val length = tangent.x * tangent.x + tangent.y * tangent.y
                            val amount = if (length > .0001f) ((drag.x * tangent.x + drag.y * tangent.y) / length).toDouble() else -drag.y * geo.radius / 64.0
                            val p = current.position.toMutableList(); p[axis] += amount
                            if (axis == 2) p[2] = p[2].coerceAtLeast(1.0)
                            current = current.copy(position = p)
                        } else {
                            val degrees = geo.rotationDelta(axis, change.position - drag, drag)
                            val a = Math.toRadians(current.angle); val e = Math.toRadians(current.elevation)
                            val toward = GizmoVec(cos(a) * cos(e), sin(a) * cos(e), -sin(e))
                                .rotate(if (axis == 0) degrees else 0.0, if (axis == 1) degrees else 0.0, if (axis == 2) degrees else 0.0).unit()
                            current = current.copy(angle = Math.toDegrees(atan2(toward.y, toward.x)), elevation = Math.toDegrees(asin((-toward.z).coerceIn(-1.0, 1.0))))
                        }
                        ed.setLight(current, token); changed = true; change.consume()
                    }
                    active = -1
                }
            }
        }) {
            val colors = listOf(Color(0xFFFF6575), Color(0xFF6FE295), Color(0xFF669FFF))
            val pixels = minOf(62.dp.toPx(), minOf(size.width, size.height) * .28f).toDouble()
            lights.forEach { light ->
                val geo = geometry(light, size.width.toDouble(), size.height.toDouble(), pixels)
                val centre = geo.centre; val selected = light.id == ed.selectedLightId
                drawCircle(Color.Black.copy(alpha = .8f), 12.dp.toPx(), centre)
                drawCircle(Color.White, 10.dp.toPx(), centre, style = Stroke(2.dp.toPx()))
                drawCircle(Color(light.color[0], light.color[1], light.color[2], if (light.enabled) 1f else .35f), 7.dp.toPx(), centre)
                if (selected) for (face in geo.faces(mode)) {
                    val base = colors[face.axis]; val brightness = if (face.axis == active) 1f else face.brightness
                    val color = Color(base.red * brightness, base.green * brightness, base.blue * brightness, if (mode == GizmoMode.Rotate && face.behind) .4f else 1f)
                    val path = Path().apply { moveTo(face.points[0].screen.x, face.points[0].screen.y); face.points.drop(1).forEach { lineTo(it.screen.x, it.screen.y) }; close() }
                    drawPath(path, color)
                }
            }
        }
        Row(Modifier.align(Alignment.TopStart).padding(6.dp), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            GizmoMode.entries.forEach { m -> SelectChip(m.name, mode == m, { mode = m }, tag = "light_gizmo_${m.name.lowercase()}") }
        }
    }
}
