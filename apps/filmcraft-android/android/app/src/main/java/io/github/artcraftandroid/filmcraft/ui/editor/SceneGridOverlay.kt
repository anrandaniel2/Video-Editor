package io.github.artcraftandroid.filmcraft.ui.editor

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.calculateZoom
import androidx.compose.foundation.layout.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import kotlin.math.*

private val axisColors = listOf(Color(0xFFFF6575), Color(0xFF6FE295), Color(0xFF669FFF))
private const val GRID_HALF = 2000.0
private const val GRID_STEP = 100.0
private const val GRID_MAJOR = 500.0
private const val GRID_CENTER_Z = 700.0

/**
 * The scene editor's 3D space: a floor grid through the orbit target's height, the world
 * axes, and the orbit-target point (a dot with the fused move gizmo). Drag empty space to
 * orbit the camera around it, pinch to dolly, drag the dot's shafts to move it. The camera
 * never moves unless the user touches the screen. Editor-only overlay; hit-testing uses the
 * same projection as the render.
 */
@Composable
internal fun SceneGridOverlay(ed: EditorController, fw: Float, fh: Float, modifier: Modifier = Modifier) {
    val orbit by rememberUpdatedState(ed.sceneOrbit)
    val camera by rememberUpdatedState(ed.sceneCamera)
    fun projection(o: SceneOrbit, w: Double, h: Double): ViewProjection {
        val fov = camera?.fov ?: CLASSIC_FOV
        return ViewProjection(GizmoVec(o.target[0], o.target[1], o.target[2]), o.yaw, o.pitch, 0.0, fov, fw.toDouble(), fh.toDouble(), w, h, o.distance)
    }

    fun targetGeometry(o: SceneOrbit, w: Double, h: Double, px: Double): GizmoGeometry =
        GizmoGeometry(GizmoPose(o.target[0], o.target[1], o.target[2]), null, fw.toDouble(), fh.toDouble(), w, h, px, projection(o, w, h), poseIsWorld = true)

    Box(modifier.fillMaxSize().testTag("scene_grid")) {
        Canvas(Modifier.fillMaxSize().pointerInput(fw, fh) {
            awaitEachGesture {
                val down = awaitFirstDown(requireUnconsumed = false)
                if (down.isConsumed) return@awaitEachGesture // an object's handle got it first
                val w = size.width.toDouble(); val h = size.height.toDouble()
                val px = minOf(62.dp.toPx(), minOf(size.width, size.height) * 0.28f).toDouble()
                val startOrbit = orbit
                val geo = targetGeometry(startOrbit, w, h, px)
                val handle = geo.hitGizmo(down.position, 16.dp.toPx(), setOf(GizmoMode.Move))
                if (handle != null) {
                    ed.stop()
                    down.consume()
                    var previous = down.position
                    var total = Offset.Zero
                    var changed = false
                    while (true) {
                        val event = awaitPointerEvent()
                        if (event.changes.count { it.pressed } > 1) break
                        val change = event.changes.firstOrNull { it.id == down.id } ?: break
                        if (!change.pressed) { change.consume(); break }
                        val delta = change.position - previous; previous = change.position; total += delta
                        if (!changed && total.getDistance() < viewConfiguration.touchSlop) continue
                        val tangent = geo.tangent(handle.axis)
                        val len = tangent.x * tangent.x + tangent.y * tangent.y
                        val drag = if (changed) delta else total
                        val amount = if (len > 0.0001f) ((drag.x * tangent.x + drag.y * tangent.y) / len).toDouble() else -drag.y * geo.radius / 64.0
                        val p = orbit.target.toMutableList()
                        p[handle.axis] += amount
                        if (handle.axis == 2) p[2] = p[2].coerceAtLeast(1.0)
                        ed.setSceneOrbit(orbit.copy(target = p))
                        changed = true
                        change.consume()
                    }
                    ed.applyScenePreviewCamera()
                } else {
                    // empty space: orbit the camera; two fingers dolly toward/away
                    down.consume()
                    var previous = down.position
                    var multi = false
                    var totalDx = 0f
                    var totalDy = 0f
                    while (true) {
                        val event = awaitPointerEvent()
                        val fingers = event.changes.count { it.pressed }
                        if (fingers == 0) break
                        if (fingers >= 2) {
                            multi = true
                            val zoom = event.calculateZoom()
                            if (zoom != 1f) {
                                val next = (orbit.distance / zoom).coerceIn(300.0, 4200.0)
                                ed.setSceneOrbit(orbit.copy(distance = next))
                            }
                            event.changes.forEach { it.consume() }
                            event.changes.firstOrNull { it.pressed }?.let { previous = it.position }
                            continue
                        }
                        if (multi) continue
                        val change = event.changes.firstOrNull { it.id == down.id } ?: break
                        if (!change.pressed) { change.consume(); break }
                        val delta = change.position - previous
                        previous = change.position
                        totalDx += delta.x; totalDy += delta.y
                        ed.setSceneOrbit(orbit.copy(
                            yaw = startOrbit.yaw + totalDx * 0.35,
                            pitch = (startOrbit.pitch - totalDy * 0.35).coerceIn(-85.0, 85.0),
                        ))
                        change.consume()
                    }
                    ed.applyScenePreviewCamera()
                }
            }
        }) {
            if (size.width <= 0f || size.height <= 0f || fw <= 0f || fh <= 0f) return@Canvas
            val o = orbit
            val proj = projection(o, size.width.toDouble(), size.height.toDouble())
            val target = GizmoVec(o.target[0], o.target[1], o.target[2])
            val baseY = target.y

            // ---- floor grid ----
            val steps = (GRID_HALF * 2 / GRID_STEP).toInt()
            fun line(a: GizmoVec, b: GizmoVec): Pair<Offset, Offset>? {
                val pa = proj.project(a) ?: return null
                val pb = proj.project(b) ?: return null
                return pa.screen to pb.screen
            }
            for (i in 0..steps) {
                val x = -GRID_HALF + i * GRID_STEP
                val isMajor = abs(x % GRID_MAJOR) < 1.0
                val alpha = when { x == 0.0 -> 0f; isMajor -> 0.26f; else -> 0.12f }
                if (alpha > 0f) line(GizmoVec(x, baseY, GRID_CENTER_Z - GRID_HALF), GizmoVec(x, baseY, GRID_CENTER_Z + GRID_HALF))?.let { (a, b) ->
                    drawLine(Color.White.copy(alpha = alpha), a, b, strokeWidth = if (isMajor) 2f else 1f)
                }
                val z = GRID_CENTER_Z - GRID_HALF + i * GRID_STEP
                val isMajorZ = abs(z % GRID_MAJOR) < 1.0
                val alphaZ = when { z - GRID_CENTER_Z == 0.0 -> 0f; isMajorZ -> 0.26f; else -> 0.12f }
                if (alphaZ > 0f) line(GizmoVec(-GRID_HALF, baseY, z), GizmoVec(GRID_HALF, baseY, z))?.let { (a, b) ->
                    drawLine(Color.White.copy(alpha = alphaZ), a, b, strokeWidth = if (isMajorZ) 2f else 1f)
                }
            }
            // world axes: x (red) and z (blue) through the origin, y (green) vertical through the target
            line(GizmoVec(-GRID_HALF, baseY, 0.0), GizmoVec(GRID_HALF, baseY, 0.0))?.let { (a, b) ->
                drawLine(Color(0xFFFF6575).copy(alpha = 0.55f), a, b, strokeWidth = 3f)
            }
            line(GizmoVec(0.0, baseY, GRID_CENTER_Z - GRID_HALF), GizmoVec(0.0, baseY, GRID_CENTER_Z + GRID_HALF))?.let { (a, b) ->
                drawLine(Color(0xFF669FFF).copy(alpha = 0.55f), a, b, strokeWidth = 3f)
            }
            line(GizmoVec(target.x, baseY - 1200.0, target.z), GizmoVec(target.x, baseY + 1200.0, target.z))?.let { (a, b) ->
                drawLine(Color(0xFF6FE295).copy(alpha = 0.35f), a, b, strokeWidth = 2f)
            }

            // ---- orbit target: shafts + dot ----
            val px = minOf(62.dp.toPx(), minOf(size.width, size.height) * 0.28f).toDouble()
            val geo = targetGeometry(o, size.width.toDouble(), size.height.toDouble(), px)
            for ((kind, face) in geo.fusedFaces()) {
                if (kind != GizmoMode.Move) continue
                val base = axisColors[face.axis]
                val brightness = face.brightness.takeIf { it.isFinite() }?.coerceIn(0f, 1f) ?: 0.4f
                val color = Color(base.red * brightness, base.green * brightness, base.blue * brightness, 1f)
                val path = Path().apply {
                    moveTo(face.points[0].screen.x, face.points[0].screen.y)
                    face.points.drop(1).forEach { lineTo(it.screen.x, it.screen.y) }
                    close()
                }
                drawPath(path, color)
            }
            proj.project(target)?.let { (screen, _) ->
                drawCircle(Color.Black.copy(alpha = 0.75f), 9.dp.toPx(), screen)
                drawCircle(Color.White, 9.dp.toPx(), screen, style = Stroke(2.dp.toPx()))
                drawCircle(Color.White, 3.dp.toPx(), screen)
            }
        }
    }
}
