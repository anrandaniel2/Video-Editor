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
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import kotlin.math.*

private const val GRID_HALF = 2000.0
private const val GRID_STEP = 100.0
private const val GRID_MAJOR = 500.0
private const val GRID_CENTER_Z = 700.0
private val CAMERA = Color(0xFFFFC947)

/**
 * The 3D scene workspace's viewport: a floor grid, the world axes, and the scene camera
 * drawn as a frustum you can see and grab. Drag empty space to orbit the camera around the
 * scene; pinch with two fingers to dolly; drag the frustum to move the render camera.
 * The grid plane sits at the target's height. Editor-only overlay; exports never see it.
 */
@Composable
internal fun SceneGridOverlay(ed: EditorController, fw: Float, fh: Float, modifier: Modifier = Modifier) {
    val orbit by rememberUpdatedState(ed.sceneOrbit)
    val camera by rememberUpdatedState(ed.sceneCamera)
    fun projection(o: SceneOrbit, w: Double, h: Double): ViewProjection {
        val fov = camera?.fov ?: CLASSIC_FOV
        return ViewProjection(GizmoVec(o.target[0], o.target[1], o.target[2]), o.yaw, o.pitch, 0.0, fov, fw.toDouble(), fh.toDouble(), w, h, o.distance)
    }
    fun camRight(c: SceneCamera) = GizmoVec(1.0, 0.0, 0.0).rotate(c.pitch, c.yaw, c.roll)
    fun camDown(c: SceneCamera) = GizmoVec(0.0, 1.0, 0.0).rotate(c.pitch, c.yaw, c.roll)
    fun camForward(c: SceneCamera) = GizmoVec(0.0, 0.0, 1.0).rotate(c.pitch, c.yaw, c.roll)
    fun camEye(c: SceneCamera) = GizmoVec(c.position[0], c.position[1], c.position[2])

    Box(modifier.fillMaxSize().testTag("scene_grid")) {
        Canvas(Modifier.fillMaxSize().pointerInput(fw, fh) {
            awaitEachGesture {
                val down = awaitFirstDown(requireUnconsumed = false)
                if (down.isConsumed) return@awaitEachGesture // an object's handle got it first
                val w = size.width.toDouble(); val h = size.height.toDouble()
                val startOrbit = orbit
                val startCam = camera
                val proj = projection(startOrbit, w, h)
                val camPos0 = startCam?.let { camEye(it) }

                // ---- grabbing the render camera? ----
                var camGrab = false
                if (startCam != null && camPos0 != null) {
                    val eye = camPos0
                    val fwd = camForward(startCam); val rgt = camRight(startCam); val dwn = camDown(startCam)
                    val fovRad = Math.toRadians(startCam.fov)
                    val near = fw.toDouble() * 0.13
                    val hh = tan(fovRad / 2.0) * near
                    val hw = hh * (fw.toDouble() / fh.toDouble())
                    val corners = listOf(
                        eye + fwd * near - rgt * hw - dwn * hh,
                        eye + fwd * near + rgt * hw - dwn * hh,
                        eye + fwd * near + rgt * hw + dwn * hh,
                        eye + fwd * near - rgt * hw + dwn * hh,
                    )
                    val segments = mutableListOf<Pair<GizmoVec, GizmoVec>>()
                    for (c in corners) segments += eye to c
                    for (i in 0..3) segments += corners[i] to corners[(i + 1) % 4]
                    // the body box behind the eye
                    val bc = eye - fwd * (near * 0.18)
                    val ex = near * 0.16; val ey = near * 0.12; val ez = near * 0.09
                    fun bodyCorner(sz: Int, sy: Int, s: Int) = bc + fwd * (sz * ez) + rgt * (s * ex) + dwn * (sy * ey)
                    val faceNear = listOf(bodyCorner(1, -1, -1), bodyCorner(1, -1, 1), bodyCorner(1, 1, 1), bodyCorner(1, 1, -1))
                    val faceFar = listOf(bodyCorner(-1, -1, -1), bodyCorner(-1, -1, 1), bodyCorner(-1, 1, 1), bodyCorner(-1, 1, -1))
                    for (face in listOf(faceNear, faceFar)) for (i in 0..3) segments += face[i] to face[(i + 1) % 4]
                    for (i in 0..3) segments += faceNear[i] to faceFar[i]
                    camGrab = segments.any { (a, b) ->
                        val pa = proj.project(a)?.screen; val pb = proj.project(b)?.screen
                        pa != null && pb != null && segmentDistance(down.position, pa, pb) < 16.dp.toPx()
                    }
                }

                if (camGrab && startCam != null && camPos0 != null) {
                    ed.stop()
                    down.consume()
                    val rgt = GizmoVec(1.0, 0.0, 0.0).rotate(startOrbit.pitch, startOrbit.yaw, 0.0)
                    val dwn = GizmoVec(0.0, 1.0, 0.0).rotate(startOrbit.pitch, startOrbit.yaw, 0.0)
                    fun tangentOf(axis: GizmoVec, at: GizmoVec): Offset {
                        val p = proj.project(at)?.screen ?: return Offset.Zero
                        val q = proj.project(at + axis * 120.0)?.screen ?: return Offset.Zero
                        return Offset((q.x - p.x) / 120.0f, (q.y - p.y) / 120.0f)
                    }

                    fun amountOf(drag: Offset, t: Offset): Double {
                        val len = t.x * t.x + t.y * t.y
                        return if (len > 0.0001f) ((drag.x * t.x + drag.y * t.y) / len).toDouble() else 0.0
                    }
                    var pos = camPos0
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
                        val drag = if (changed) delta else total
                        pos = pos + rgt * amountOf(drag, tangentOf(rgt, pos)) + dwn * amountOf(drag, tangentOf(dwn, pos))
                        ed.setSceneCamera(startCam.copy(position = listOf(pos.x, pos.y, pos.z)), "scene-camera-drag")
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
                                val next = (orbit.distance / zoom).coerceIn(150.0, 4200.0)
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
            val baseY = o.target[1]

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
            // world axes: x (red) and z (blue) through the origin, at the grid plane
            line(GizmoVec(-GRID_HALF, baseY, 0.0), GizmoVec(GRID_HALF, baseY, 0.0))?.let { (a, b) ->
                drawLine(Color(0xFFFF6575).copy(alpha = 0.55f), a, b, strokeWidth = 3f)
            }
            line(GizmoVec(0.0, baseY, GRID_CENTER_Z - GRID_HALF), GizmoVec(0.0, baseY, GRID_CENTER_Z + GRID_HALF))?.let { (a, b) ->
                drawLine(Color(0xFF669FFF).copy(alpha = 0.55f), a, b, strokeWidth = 3f)
            }

            // ---- the scene camera: a frustum you can see and grab ----
            val cam = camera
            if (cam != null) {
                val eye = camEye(cam)
                val fwd = camForward(cam); val rgt = camRight(cam); val dwn = camDown(cam)
                val fovRad = Math.toRadians(cam.fov)
                val near = fw.toDouble() * 0.13
                val hh = tan(fovRad / 2.0) * near
                val hw = hh * (fw.toDouble() / fh.toDouble())
                val corners = listOf(
                    eye + fwd * near - rgt * hw - dwn * hh,
                    eye + fwd * near + rgt * hw - dwn * hh,
                    eye + fwd * near + rgt * hw + dwn * hh,
                    eye + fwd * near - rgt * hw + dwn * hh,
                )
                val pts = corners.map { proj.project(it)?.screen }
                val apex = proj.project(eye)?.screen
                apex?.let { a ->
                    for (p in pts) p?.let { b -> drawLine(CAMERA.copy(alpha = 0.6f), a, b, strokeWidth = 2f) }
                }
                for (i in 0..3) {
                    val a = pts[i] ?: continue; val b = pts[(i + 1) % 4] ?: continue
                    drawLine(CAMERA, a, b, strokeWidth = 3f)
                }
                // body: a small box just behind the eye
                val bc = eye - fwd * (near * 0.18)
                val ex = near * 0.16; val ey = near * 0.12; val ez = near * 0.09
                fun bodyCorner(sz: Int, sy: Int, s: Int) = bc + fwd * (sz * ez) + rgt * (s * ex) + dwn * (sy * ey)
                val faceNear = listOf(bodyCorner(1, -1, -1), bodyCorner(1, -1, 1), bodyCorner(1, 1, 1), bodyCorner(1, 1, -1)).map { proj.project(it)?.screen }
                val faceFar = listOf(bodyCorner(-1, -1, -1), bodyCorner(-1, -1, 1), bodyCorner(-1, 1, 1), bodyCorner(-1, 1, -1)).map { proj.project(it)?.screen }
                val boxAlpha = CAMERA.copy(alpha = 0.85f)
                for (face in listOf(faceNear, faceFar)) {
                    for (i in 0..3) {
                        val a = face[i] ?: continue; val b = face[(i + 1) % 4] ?: continue
                        drawLine(boxAlpha, a, b, strokeWidth = 2f)
                    }
                }
                for (i in 0..3) {
                    val a = faceNear[i] ?: continue; val b = faceFar[i] ?: continue
                    drawLine(boxAlpha, a, b, strokeWidth = 2f)
                }
                // apex pivot dot
                apex?.let { c ->
                    drawCircle(Color.Black.copy(alpha = 0.8f), 7.dp.toPx(), c)
                    drawCircle(CAMERA, 7.dp.toPx(), c, style = Stroke(2.dp.toPx()))
                }
            }
        }
    }
}
