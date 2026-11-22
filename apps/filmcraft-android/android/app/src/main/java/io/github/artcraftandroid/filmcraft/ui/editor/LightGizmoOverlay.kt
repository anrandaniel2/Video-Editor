package io.github.artcraftandroid.filmcraft.ui.editor

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.unit.dp
import kotlin.math.*

/** Lights use the same projected cone/cylinder axes and rotation rings as object gizmos. */
@Composable
internal fun LightGizmoOverlay(ed: EditorController, fw: Float, fh: Float) {
    val lights by rememberUpdatedState(ed.sceneLights.lights)
    var active by remember { mutableStateOf<GizmoHit?>(null) }
    fun geometry(light: SceneLight, w: Double, h: Double, pixels: Double): GizmoGeometry {
        val view = viewFor(ed, fw.toDouble(), fh.toDouble(), w, h)
        if (view != null) {
            val pose = GizmoPose(light.position[0] - fw / 2.0, light.position[1] - fh / 2.0, light.position[2])
            return GizmoGeometry(pose, null, fw.toDouble(), fh.toDouble(), w, h, pixels, view, poseIsWorld = true)
        }
        val z = light.position[2].coerceAtLeast(1.0)
        val pose = GizmoPose(fw / 2.0 + fh * (light.position[0] - fw / 2.0) / z,
            fh / 2.0 + fh * (light.position[1] - fh / 2.0) / z, z)
        return GizmoGeometry(pose, null, fw.toDouble(), fh.toDouble(), w, h, pixels)
    }
    Box(Modifier.fillMaxSize()) {
        Canvas(Modifier.fillMaxSize().pointerInput(fw, fh) {
            fun g(light: SceneLight) = geometry(light, size.width.toDouble(), size.height.toDouble(), minOf(62.dp.toPx(), minOf(size.width, size.height) * .28f).toDouble())
            awaitEachGesture {
                val down = awaitFirstDown(requireUnconsumed = false)
                if (down.isConsumed || lights.isEmpty()) return@awaitEachGesture
                val selected = lights.firstOrNull { it.id == ed.selectedLightId }
                val hitAxis = selected?.let { g(it).hitGizmo(down.position, 16.dp.toPx()) }
                if (hitAxis != null && selected != null) {
                    ed.stop(); ed.selectedLightId = selected.id; down.consume(); active = hitAxis
                    var current: SceneLight = selected; var previous = down.position
                    var total = Offset.Zero; var changed = false
                    val token = "light-gizmo:${java.util.UUID.randomUUID()}"
                    while (true) {
                        val event = awaitPointerEvent()
                        if (event.changes.count { it.pressed } > 1) break
                        val change = event.changes.firstOrNull { it.id == down.id } ?: break
                        if (!change.pressed) { change.consume(); break }
                        val delta = change.position - previous; previous = change.position; total += delta
                        if (!changed && total.getDistance() < viewConfiguration.touchSlop) continue
                        val drag = if (changed) delta else total
                        val geo = g(current)
                        if (hitAxis.kind == GizmoMode.Move) {
                            val tangent = geo.tangent(hitAxis.axis); val length = tangent.x * tangent.x + tangent.y * tangent.y
                            val amount = if (length > .0001f) ((drag.x * tangent.x + drag.y * tangent.y) / length).toDouble() else -drag.y * geo.radius / 64.0
                            val p = current.position.toMutableList(); p[hitAxis.axis] += amount
                            if (hitAxis.axis == 2) p[hitAxis.axis] = p[hitAxis.axis].coerceAtLeast(1.0)
                            current = current.copy(position = p)
                        } else {
                            val degrees = geo.rotationDelta(hitAxis.axis, change.position - drag, drag)
                            val a = Math.toRadians(current.angle); val e = Math.toRadians(current.elevation)
                            val toward = GizmoVec(cos(a) * cos(e), sin(a) * cos(e), -sin(e))
                                .rotate(if (hitAxis.axis == 0) degrees else 0.0, if (hitAxis.axis == 1) degrees else 0.0, if (hitAxis.axis == 2) degrees else 0.0).unit()
                            current = current.copy(angle = Math.toDegrees(atan2(toward.y, toward.x)), elevation = Math.toDegrees(asin((-toward.z).coerceIn(-1.0, 1.0))))
                        }
                        ed.setLight(current, token); changed = true; change.consume()
                    }
                    active = null
                } else {
                    val hitBody = lights.minByOrNull { (g(it).centre - down.position).getDistance() }
                        ?.takeIf { (g(it).centre - down.position).getDistance() < 22.dp.toPx() }
                        ?: return@awaitEachGesture
                    var previous = down.position
                    var total = Offset.Zero
                    while (true) {
                        val event = awaitPointerEvent()
                        if (event.changes.count { it.pressed } > 1) break
                        val change = event.changes.firstOrNull { it.id == down.id } ?: break
                        if (!change.pressed) {
                            ed.stop()
                            ed.selectedLightId = hitBody.id
                            change.consume()
                            break
                        }
                        total += change.position - previous
                        previous = change.position
                        if (total.getDistance() >= viewConfiguration.touchSlop) break
                    }
                }
            }
        }) {
            val colors = listOf(Color(0xFFFF6575), Color(0xFF6FE295), Color(0xFF669FFF))
            val pixels = minOf(62.dp.toPx(), minOf(size.width, size.height) * .28f).toDouble()
            lights.forEach { light ->
                val geo = geometry(light, size.width.toDouble(), size.height.toDouble(), pixels)
                val centre = geo.centre; val selected = light.id == ed.selectedLightId
                val tint = Color(light.color[0], light.color[1], light.color[2], if (light.enabled) .78f else .3f)
                val origin = GizmoVec(light.position[0] - fw.toDouble() / 2.0, light.position[1] - fh.toDouble() / 2.0, light.position[2])
                fun screen(p: GizmoVec) = viewFor(ed, fw.toDouble(), fh.toDouble(), size.width.toDouble(), size.height.toDouble())?.project(p)?.screen
                    ?: run {
                        if (p.z <= 1.0) null else Offset(((fw.toDouble() / 2 + fh.toDouble() * p.x / p.z) * size.width / fw).toFloat(), ((fh.toDouble() / 2 + fh.toDouble() * p.y / p.z) * size.height / fh).toFloat())
                    }
                if (light.kind == "point") {
                    val r = (light.range.takeIf { it > 0.0 } ?: 180.0).coerceIn(30.0, 2000.0)
                    val bases = listOf(GizmoVec(1.0,0.0,0.0) to GizmoVec(0.0,1.0,0.0), GizmoVec(1.0,0.0,0.0) to GizmoVec(0.0,0.0,1.0), GizmoVec(0.0,1.0,0.0) to GizmoVec(0.0,0.0,1.0))
                    for ((u, v) in bases) {
                        var prev: Offset? = null
                        for (i in 0..32) {
                            val a = i * 2.0 * Math.PI / 32.0
                            val next = screen(origin + u * (cos(a) * r) + v * (sin(a) * r))
                            if (prev != null && next != null) drawLine(tint.copy(alpha = .6f), prev!!, next, strokeWidth = 1.5f)
                            prev = next
                        }
                    }
                } else {
                    val angle = Math.toRadians(light.angle)
                    val elev = Math.toRadians(light.elevation)
                    val axis = GizmoVec(cos(angle) * cos(elev), sin(angle) * cos(elev), -sin(elev)).unit()
                    val length = (light.range.takeIf { it > 0.0 } ?: 600.0).coerceIn(80.0, 3000.0)
                    val halfAngle = Math.toRadians(if (light.kind == "spot") light.cone.coerceIn(1.0, 89.0) else 8.0)
                    val radius = tan(halfAngle) * length
                    val reference = if (abs(axis.y) < .9) GizmoVec(0.0,1.0,0.0) else GizmoVec(1.0,0.0,0.0)
                    val u = axis.cross(reference).unit(); val v = axis.cross(u).unit()
                    var prev: Offset? = null
                    for (i in 0..32) {
                        val a = i * 2.0 * Math.PI / 32.0
                        val ring = origin + axis * length + u * (cos(a) * radius) + v * (sin(a) * radius)
                        val next = screen(ring)
                        if (prev != null && next != null) drawLine(tint.copy(alpha = .65f), prev!!, next, strokeWidth = 1.5f)
                        prev = next
                    }
                    for (i in 0 until 8) {
                        val a = i * 2.0 * Math.PI / 8.0
                        val edge = origin + axis * length + u * (cos(a) * radius) + v * (sin(a) * radius)
                        val p0 = screen(origin); val p1 = screen(edge)
                        if (p0 != null && p1 != null) drawLine(tint.copy(alpha = .48f), p0, p1, strokeWidth = 1.25f)
                    }
                }
                // A tiny 3D wireframe body marks the source; unlike the old flat dot, the beam/range
                // volume is the primary light affordance and updates live with range/cone settings.
                val bodyR = 10.0
                val body = listOf(GizmoVec(1.0,0.0,0.0) to GizmoVec(0.0,1.0,0.0), GizmoVec(1.0,0.0,0.0) to GizmoVec(0.0,0.0,1.0), GizmoVec(0.0,1.0,0.0) to GizmoVec(0.0,0.0,1.0))
                body.forEach { (u, v) ->
                    var prev: Offset? = null
                    for (i in 0..16) {
                        val a = i * 2.0 * Math.PI / 16.0
                        val next = screen(origin + u * (cos(a) * bodyR) + v * (sin(a) * bodyR))
                        if (prev != null && next != null) drawLine(tint, prev!!, next, strokeWidth = 2f)
                        prev = next
                    }
                }
                if (selected) for ((kind, face) in geo.fusedFaces()) {
                    val base = colors[face.axis]; val brightness = if (active?.kind == kind && active?.axis == face.axis) 1f else face.brightness
                    val color = Color(base.red * brightness, base.green * brightness, base.blue * brightness, if (kind == GizmoMode.Rotate && face.behind) .4f else 1f)
                    val path = Path().apply { moveTo(face.points[0].screen.x, face.points[0].screen.y); face.points.drop(1).forEach { lineTo(it.screen.x, it.screen.y) }; close() }
                    drawPath(path, color)
                }
            }
        }
    }
}
