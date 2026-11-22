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
import androidx.compose.ui.unit.dp
import androidx.compose.ui.platform.testTag
import kotlin.math.*

private const val GRID_HALF = 2000.0
private const val GRID_STEP = 100.0
private const val GRID_MAJOR = 500.0
private const val GRID_CENTER_Z = 700.0
private val CAMERA = Color(0xFFFFC947)
private val SOLVED_CAMERA = Color(0xFF4CC9F0)

/**
 * The scene editor's 3D space: a floor grid through the orbit target's height and the world
 * axes. Drag empty space to orbit the view around the scene; pinch with two fingers to dolly.
 * The render camera, when the scene has one, is drawn as a frustum with a body box so it can
 * be seen — and when 3D Camera Match-Move is active, its reconstructed 3D point cloud and
 * camera trajectory path are drawn in world space as well.
 * Editor-only overlay; the orbit view never reaches an export.
 */
@Composable
internal fun SceneGridOverlay(ed: EditorController, fw: Float, fh: Float, modifier: Modifier = Modifier) {
    val orbit by rememberUpdatedState(ed.sceneOrbit)
    val camera by rememberUpdatedState(ed.activeRenderCamera)
    val matchmoveActive by rememberUpdatedState(ed.matchmoveActive)
    val sceneData by rememberUpdatedState(ed.matchmoveSceneData)
    val matchmovePose by rememberUpdatedState(ed.matchmovePose)
    fun projection(o: SceneOrbit, w: Double, h: Double): ViewProjection {
        val fov = camera?.fov ?: CLASSIC_FOV
        return ViewProjection(GizmoVec(o.target[0], o.target[1], o.target[2]), o.yaw, o.pitch, 0.0, fov, fw.toDouble(), fh.toDouble(), w, h, o.distance)
    }

    Box(modifier.fillMaxSize().testTag("scene_grid")) {
        Canvas(Modifier.fillMaxSize().pointerInput(fw, fh) {
            awaitEachGesture {
                val down = awaitFirstDown(requireUnconsumed = false)
                if (down.isConsumed) return@awaitEachGesture // an object's handle got it first
                val startOrbit = orbit
                // empty space: orbit the view; two fingers dolly toward/away
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
            // world axes: x (red) and z (blue) through the origin
            line(GizmoVec(-GRID_HALF, baseY, 0.0), GizmoVec(GRID_HALF, baseY, 0.0))?.let { (a, b) ->
                drawLine(Color(0xFFFF6575).copy(alpha = 0.55f), a, b, strokeWidth = 3f)
            }
            line(GizmoVec(0.0, baseY, GRID_CENTER_Z - GRID_HALF), GizmoVec(0.0, baseY, GRID_CENTER_Z + GRID_HALF))?.let { (a, b) ->
                drawLine(Color(0xFF669FFF).copy(alpha = 0.55f), a, b, strokeWidth = 3f)
            }

            // ---- match-move reconstructed 3D landmarks & camera path ----
            val solved = if (matchmoveActive) sceneData else null
            if (solved != null) {
                val crossArm = 3.5.dp.toPx()
                solved.points.forEach { pt ->
                    val projPt = proj.project(GizmoVec(pt.x, pt.y, pt.z)) ?: return@forEach
                    val tint = landmarkColor(pt.rmsPx).copy(alpha = 0.85f)
                    val c = projPt.screen
                    drawLine(tint, Offset(c.x - crossArm, c.y), Offset(c.x + crossArm, c.y), strokeWidth = 1.6f)
                    drawLine(tint, Offset(c.x, c.y - crossArm), Offset(c.x, c.y + crossArm), strokeWidth = 1.6f)
                }
                if (!solved.rotationOnly && solved.path.size >= 2) {
                    var prev: Offset? = null
                    solved.path.forEachIndexed { idx, sample ->
                        val eyeSample = GizmoVec(
                            sample.position.getOrElse(0) { 0.0 },
                            sample.position.getOrElse(1) { 0.0 },
                            sample.position.getOrElse(2) { 0.0 },
                        )
                        val cur = proj.project(eyeSample)?.screen
                        if (prev != null && cur != null) {
                            drawLine(SOLVED_CAMERA.copy(alpha = 0.78f), prev!!, cur, strokeWidth = 2.5f)
                        }
                        if (cur != null && (idx == 0 || idx == solved.path.lastIndex || idx % 8 == 0)) {
                            drawCircle(SOLVED_CAMERA, 2.5.dp.toPx(), cur)
                        }
                        prev = cur
                    }
                }
            }

            // ---- the render camera, as an object: a frustum with a body box ----
            val cam = camera
            if (cam != null) {
                val camColor = when {
                    !matchmoveActive -> CAMERA
                    matchmovePose?.lost == true -> Color(0xFFFF6575)
                    else -> SOLVED_CAMERA
                }
                val eye = GizmoVec(cam.position[0], cam.position[1], cam.position[2])
                val fwd = GizmoVec(0.0, 0.0, 1.0).rotate(cam.pitch, cam.yaw, cam.roll)
                val rgt = GizmoVec(1.0, 0.0, 0.0).rotate(cam.pitch, cam.yaw, cam.roll)
                val dwn = GizmoVec(0.0, 1.0, 0.0).rotate(cam.pitch, cam.yaw, cam.roll)
                val near = fw.toDouble() * 0.13
                val halfH = tan(Math.toRadians(cam.fov) / 2.0) * near
                val halfW = halfH * (fw.toDouble() / fh.toDouble())
                val corners = listOf(
                    eye + fwd * near - rgt * halfW - dwn * halfH,
                    eye + fwd * near + rgt * halfW - dwn * halfH,
                    eye + fwd * near + rgt * halfW + dwn * halfH,
                    eye + fwd * near - rgt * halfW + dwn * halfH,
                )
                fun frustumLine(a: GizmoVec, b: GizmoVec, width: Float, color: Color) {
                    val pa = proj.project(a)?.screen; val pb = proj.project(b)?.screen
                    if (pa != null && pb != null) drawLine(color, pa, pb, strokeWidth = width)
                }
                // lens lines from the eye to the near plane
                for (c in corners) frustumLine(eye, c, 2f, camColor.copy(alpha = 0.6f))
                // the near rectangle
                for (i in 0..3) frustumLine(corners[i], corners[(i + 1) % 4], 3f, camColor)
                // the body: a small box just behind the eye
                val bc = eye - fwd * (near * 0.18)
                val ex = near * 0.16; val ey = near * 0.12; val ez = near * 0.09
                fun bodyCorner(sz: Int, sy: Int, s: Int) = bc + fwd * (sz * ez) + rgt * (s * ex) + dwn * (sy * ey)
                val faceNear = listOf(bodyCorner(1, -1, -1), bodyCorner(1, -1, 1), bodyCorner(1, 1, 1), bodyCorner(1, 1, -1))
                val faceFar = listOf(bodyCorner(-1, -1, -1), bodyCorner(-1, -1, 1), bodyCorner(-1, 1, 1), bodyCorner(-1, 1, -1))
                val bodyColor = camColor.copy(alpha = 0.85f)
                for (i in 0..3) {
                    frustumLine(faceNear[i], faceNear[(i + 1) % 4], 2f, bodyColor)
                    frustumLine(faceFar[i], faceFar[(i + 1) % 4], 2f, bodyColor)
                    frustumLine(faceNear[i], faceFar[i], 2f, bodyColor)
                }
                // a dot at the pivot: the body apex, not an orbit marker
                proj.project(eye)?.screen?.let { apex ->
                    drawCircle(Color.Black.copy(alpha = 0.85f), 7.dp.toPx(), apex)
                    drawCircle(camColor, 7.dp.toPx(), apex, style = Stroke(2.dp.toPx()))
                }
            }
        }
    }
}
