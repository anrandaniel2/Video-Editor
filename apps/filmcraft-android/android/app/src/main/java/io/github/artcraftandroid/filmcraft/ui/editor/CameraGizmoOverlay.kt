package io.github.artcraftandroid.filmcraft.ui.editor

import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.unit.dp

/** Move/rotate handles for the scene's active render camera (manual 3D camera or solved Match-Move rig). */
@Composable
internal fun CameraGizmoOverlay(
    ed: EditorController,
    fw: Float,
    fh: Float,
    modifier: Modifier = Modifier,
    content: @Composable BoxScope.() -> Unit = {},
) {
    val camera by rememberUpdatedState(ed.activeRenderCamera)
    val matchmoveActive by rememberUpdatedState(ed.matchmoveActive)
    val matchmoveState by rememberUpdatedState(ed.matchmoveState)
    val orbit by rememberUpdatedState(ed.sceneOrbit)
    var dragCamera by remember { mutableStateOf<SceneCamera?>(null) }
    val colors = listOf(Color(0xFFFF6575), Color(0xFF6FE295), Color(0xFF669FFF))

    Box(
        modifier
            .fillMaxSize()
            .pointerInput(fw, fh) {
                fun geometry(cam: SceneCamera) = GizmoGeometry(
                    GizmoPose(cam.position[0], cam.position[1], cam.position[2], cam.pitch, cam.yaw, cam.roll),
                    null, fw.toDouble(), fh.toDouble(), size.width.toDouble(), size.height.toDouble(),
                    minOf(54.dp.toPx(), minOf(size.width, size.height) * .25f).toDouble(),
                    ViewProjection(
                        GizmoVec(orbit.target[0], orbit.target[1], orbit.target[2]),
                        orbit.yaw, orbit.pitch, 0.0, cam.fov,
                        fw.toDouble(), fh.toDouble(), size.width.toDouble(), size.height.toDouble(),
                        orbit.distance,
                    ),
                    poseIsWorld = true,
                )
                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = false)
                    if (down.isConsumed) return@awaitEachGesture
                    val startCam = camera ?: return@awaitEachGesture
                    val hit = geometry(startCam).hitGizmo(down.position, 18.dp.toPx()) ?: return@awaitEachGesture
                    ed.stop()
                    down.consume()
                    var current = startCam
                    val drivingMatchMove = matchmoveActive
                    var currentMm = matchmoveState
                    dragCamera = current
                    var previous = down.position
                    var total = Offset.Zero
                    var changed = false
                    val token = "scene-camera-gizmo:${java.util.UUID.randomUUID()}"
                    try {
                        while (true) {
                            val event = awaitPointerEvent()
                            if (event.changes.count { it.pressed } == 0) {
                                event.changes.forEach { it.consume() }
                                break
                            }
                            if (event.changes.count { it.pressed } != 1) {
                                event.changes.forEach { if (it.pressed) it.consume() }
                                continue
                            }
                            val change = event.changes.firstOrNull { it.id == down.id } ?: break
                            if (!change.pressed) {
                                change.consume()
                                break
                            }
                            val delta = change.position - previous
                            previous = change.position
                            total += delta
                            if (!changed && total.getDistance() < viewConfiguration.touchSlop) continue
                            val drag = if (changed) delta else total
                            val geo = geometry(current)
                            if (drivingMatchMove) {
                                val mm = currentMm ?: break
                                if (hit.kind == GizmoMode.Move) {
                                    val tangent = geo.tangent(hit.axis)
                                    val length2 = tangent.x * tangent.x + tangent.y * tangent.y
                                    val amount = if (length2 > .0001f) ((drag.x * tangent.x + drag.y * tangent.y) / length2).toDouble() else -drag.y.toDouble()
                                    val axis = geo.axes[hit.axis] * amount
                                    currentMm = mm.copy(
                                        offsetX = mm.offsetX + axis.x,
                                        offsetY = mm.offsetY + axis.y,
                                        offsetZ = mm.offsetZ + axis.z,
                                    )
                                    current = current.copy(
                                        position = listOf(current.position[0] + axis.x, current.position[1] + axis.y, current.position[2] + axis.z),
                                    )
                                } else {
                                    val degrees = geo.rotationDelta(hit.axis, change.position - drag, drag)
                                    val (nextMm, nextCam) = rotateMatchMoveCamera(current, mm, hit.axis, degrees)
                                    currentMm = nextMm
                                    current = nextCam
                                }
                                dragCamera = current
                                currentMm?.let { ed.setMatchMove(it, token) }
                            } else {
                                current = if (hit.kind == GizmoMode.Move) {
                                    val tangent = geo.tangent(hit.axis)
                                    val length2 = tangent.x * tangent.x + tangent.y * tangent.y
                                    val amount = if (length2 > .0001f) ((drag.x * tangent.x + drag.y * tangent.y) / length2).toDouble() else -drag.y.toDouble()
                                    val axis = geo.axes[hit.axis] * amount
                                    current.copy(position = listOf(current.position[0] + axis.x, current.position[1] + axis.y, current.position[2] + axis.z))
                                } else {
                                    val degrees = geo.rotationDelta(hit.axis, change.position - drag, drag)
                                    val (pitch, yaw, roll) = rotateEulerLocal(current.pitch, current.yaw, current.roll, hit.axis, degrees)
                                    current.copy(pitch = pitch, yaw = yaw, roll = roll)
                                }
                                dragCamera = current
                                ed.setSceneCamera(current, token)
                            }
                            changed = true
                            change.consume()
                        }
                    } finally {
                        dragCamera = null
                    }
                }
            }
            .drawBehind {
                val cam = dragCamera ?: ed.activeRenderCamera ?: return@drawBehind
                val o = ed.sceneOrbit
                if (size.width <= 0f || size.height <= 0f || fw <= 0f || fh <= 0f) return@drawBehind
                val proj = ViewProjection(
                    GizmoVec(o.target[0], o.target[1], o.target[2]),
                    o.yaw, o.pitch, 0.0, cam.fov,
                    fw.toDouble(), fh.toDouble(), size.width.toDouble(), size.height.toDouble(),
                    o.distance,
                )
                val geo = GizmoGeometry(
                    GizmoPose(cam.position[0], cam.position[1], cam.position[2], cam.pitch, cam.yaw, cam.roll),
                    null, fw.toDouble(), fh.toDouble(), size.width.toDouble(), size.height.toDouble(),
                    minOf(54.dp.toPx(), minOf(size.width, size.height) * .25f).toDouble(), proj, poseIsWorld = true,
                )
                for ((kind, face) in geo.fusedFaces()) {
                    val path = Path().apply {
                        moveTo(face.points[0].screen.x, face.points[0].screen.y)
                        face.points.drop(1).forEach { lineTo(it.screen.x, it.screen.y) }
                        close()
                    }
                    val base = colors[face.axis]
                    drawPath(
                        path,
                        Color(
                            base.red * face.brightness,
                            base.green * face.brightness,
                            base.blue * face.brightness,
                            if (kind == GizmoMode.Rotate && face.behind) .35f else .8f,
                        ),
                    )
                }
            },
        content = content,
    )
}
