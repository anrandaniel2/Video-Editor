package io.github.artcraftandroid.filmcraft.ui.editor

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.unit.dp
import kotlin.math.*

/** Editor-only handles, drawn after the image; they are never included in exported frames. */
@Composable
internal fun LightGizmoOverlay(ed: EditorController, fw: Float, fh: Float) {
    val lights by rememberUpdatedState(ed.sceneLights.lights)
    fun centre(l: SceneLight, w: Float, h: Float): Offset = Offset(
        (l.position[0] / fw * w).toFloat().coerceIn(28f, (w - 28f).coerceAtLeast(28f)),
        (l.position[1] / fh * h).toFloat().coerceIn(28f, (h - 28f).coerceAtLeast(28f)))
    Canvas(Modifier.fillMaxSize().pointerInput(fw, fh) {
        awaitEachGesture {
            val down = awaitFirstDown(requireUnconsumed = false)
            val length = 48.dp.toPx()
            val directions = listOf(Offset(1f, 0f), Offset(0f, -1f), Offset(-.7f, .7f))
            val candidates = lights.flatMap { l ->
                val c = centre(l, size.width.toFloat(), size.height.toFloat())
                listOf(Triple(l, -1, c)) + if (l.id == ed.selectedLightId) directions.mapIndexed { i, d -> Triple(l, i, c + d * length) } else emptyList()
            }
            val hit = candidates.minByOrNull { (it.third - down.position).getDistance() }
                ?.takeIf { (it.third - down.position).getDistance() <= 24.dp.toPx() }
            if (hit != null) {
                ed.stop(); ed.selectedLightId = hit.first.id; down.consume()
                var current = hit.first
                var previous = down.position
                val token = "light-gizmo:${java.util.UUID.randomUUID()}"
                while (true) {
                    val event = awaitPointerEvent()
                    val change = event.changes.firstOrNull { it.id == down.id } ?: break
                    if (!change.pressed) { change.consume(); break }
                    val delta = change.position - previous; previous = change.position
                    if (delta != Offset.Zero) {
                        current = if (current.kind == "directional" && hit.second == -1) {
                            current.copy(angle = current.angle + delta.x * .5, elevation = (current.elevation - delta.y * .5).coerceIn(-90.0, 90.0))
                        } else {
                            val p = current.position.toMutableList()
                            when (hit.second) {
                                0 -> p[0] += delta.x * fw / size.width
                                1 -> p[1] += delta.y * fh / size.height
                                2 -> p[2] = (p[2] + (-delta.x + delta.y) * fh / size.height).coerceAtLeast(1.0)
                                else -> { p[0] += delta.x * fw / size.width; p[1] += delta.y * fh / size.height }
                            }
                            current.copy(position = p)
                        }
                        ed.setLight(current, token)
                    }
                    change.consume()
                }
            }
        }
    }) {
        val axisColors = listOf(Color(0xFFFF6575), Color(0xFF6FE295), Color(0xFF669FFF))
        val directions = listOf(Offset(1f, 0f), Offset(0f, -1f), Offset(-.7f, .7f))
        lights.forEach { light ->
            val c = centre(light, size.width, size.height)
            val selected = light.id == ed.selectedLightId
            val r = if (selected) 12.dp.toPx() else 9.dp.toPx()
            drawCircle(Color.Black.copy(alpha = .8f), r + 3.dp.toPx(), c)
            drawCircle(Color.White, r, c, style = Stroke(2.dp.toPx()))
            drawCircle(Color(light.color[0], light.color[1], light.color[2], if (light.enabled) 1f else .35f), r - 3.dp.toPx(), c)
            for (i in 0..7) {
                val a = i * PI / 4
                val d = Offset(cos(a).toFloat(), sin(a).toFloat())
                drawLine(Color.White, c + d * (r + 4.dp.toPx()), c + d * (r + 8.dp.toPx()), 2.dp.toPx())
            }
            if (selected) directions.forEachIndexed { i, d ->
                val end = c + d * 48.dp.toPx()
                drawLine(Color.Black, c, end, 5.dp.toPx())
                drawLine(axisColors[i], c, end, 3.dp.toPx())
                drawCircle(axisColors[i], 5.dp.toPx(), end)
            }
        }
    }
}
