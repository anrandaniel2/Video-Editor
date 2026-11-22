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
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.unit.dp
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.asin
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.min
import kotlin.math.sin
import kotlin.math.tan

/** Dynamic cone metrics computed from a [SceneLight] for the Blender-style 3D light gizmo. */
internal data class LightConeSpec(
    val emitDir: GizmoVec,
    val basisU: GizmoVec,
    val basisV: GizmoVec,
    val beamLength: Double,
    val outerRadius: Double,
    val innerRadius: Double?,
    val hasAimTarget: Boolean,
)

/** Unit vector in world space along which light rays travel (`-toward` from `object3d.rs`). */
internal fun lightEmitDirection(angleDeg: Double, elevationDeg: Double): GizmoVec {
    val a = Math.toRadians(angleDeg)
    val e = Math.toRadians(elevationDeg)
    return GizmoVec(-cos(a) * cos(e), -sin(a) * cos(e), sin(e)).unit()
}

/** Recover `(angleDeg, elevationDeg)` from a world-space emission unit vector [emitDir]. */
internal fun lightAnglesFromEmit(emitDir: GizmoVec): Pair<Double, Double> {
    val toward = emitDir.unit() * -1.0
    val rawAngle = Math.toDegrees(atan2(toward.y, toward.x))
    val angle = if (rawAngle < 0.0) rawAngle + 360.0 else rawAngle
    val elevation = Math.toDegrees(asin((-toward.z).coerceIn(-1.0, 1.0)))
    return angle to elevation
}

/** Build dynamic Blender-style cone geometry reflecting kind, angle, elevation, cone, softness, and range. */
internal fun buildLightConeSpec(light: SceneLight, gizmoRadius: Double): LightConeSpec {
    val emit = lightEmitDirection(light.angle, light.elevation)
    val ref = if (abs(emit.y) < 0.92) GizmoVec(0.0, 1.0, 0.0) else GizmoVec(1.0, 0.0, 0.0)
    val u = emit.cross(ref).unit()
    val v = emit.cross(u).unit()
    val r = gizmoRadius.coerceAtLeast(16.0)
    val rangeFactor = if (light.range > 0.0) {
        0.65 + 1.85 * (light.range / 2000.0).coerceIn(0.05, 1.5)
    } else {
        1.42
    }
    val beamLen = when (light.kind) {
        "directional" -> r * 1.38
        else -> r * rangeFactor
    }
    return when (light.kind) {
        "spot" -> {
            val coneDeg = light.cone.coerceIn(4.0, 82.0)
            val outerR = (beamLen * tan(Math.toRadians(coneDeg))).coerceIn(r * 0.08, beamLen * 2.25)
            val soft = light.softness.coerceIn(0.0, 1.0)
            val innerR = if (soft > 0.03) {
                val innerDeg = (coneDeg * (1.0 - 0.95 * soft)).coerceAtLeast(1.0)
                (beamLen * tan(Math.toRadians(innerDeg))).coerceIn(r * 0.02, outerR)
            } else {
                null
            }
            LightConeSpec(emit, u, v, beamLen, outerR, innerR, hasAimTarget = true)
        }
        "directional" -> {
            val outerR = beamLen * tan(Math.toRadians(15.0))
            LightConeSpec(emit, u, v, beamLen, outerR, innerRadius = null, hasAimTarget = true)
        }
        else -> {
            val outerR = if (light.range > 0.0) (r * (0.35 + 0.85 * (light.range / 2000.0).coerceIn(0.05, 1.5))) else r * 0.42
            LightConeSpec(emit, u, v, beamLen, outerR, innerRadius = null, hasAimTarget = false)
        }
    }
}

private enum class LightDragKind { MoveAxis, AimTarget }

/**
 * Blender-style dynamic 3D light cone gizmo: crisp emitter dot, dynamic 3D beam cone reflecting
 * cone angle, soft-edge penumbra, falloff range, brightness & colour, draggable 3D aim target dot,
 * and XYZ translation arrows (no wireframe sphere clutter or 3-torus rotation rings).
 */
@Composable
internal fun LightGizmoOverlay(
    ed: EditorController,
    fw: Float,
    fh: Float,
    modifier: Modifier = Modifier,
    content: @Composable BoxScope.() -> Unit = {},
) {
    val lights by rememberUpdatedState(ed.sceneLights.lights)
    var activeAxis by remember { mutableStateOf<Int?>(null) }
    var aimingLight by remember { mutableStateOf(false) }

    fun geometry(light: SceneLight, w: Double, h: Double, pixels: Double): GizmoGeometry {
        val view = viewFor(ed, fw.toDouble(), fh.toDouble(), w, h)
        if (view != null) {
            val pose = GizmoPose(light.position[0] - fw / 2.0, light.position[1] - fh / 2.0, light.position[2])
            return GizmoGeometry(pose, null, fw.toDouble(), fh.toDouble(), w, h, pixels, view, poseIsWorld = true)
        }
        val z = light.position[2].coerceAtLeast(20.0)
        val pose = GizmoPose(
            fw / 2.0 + fh * (light.position[0] - fw / 2.0) / z,
            fh / 2.0 + fh * (light.position[1] - fh / 2.0) / z,
            z,
        )
        return GizmoGeometry(pose, null, fw.toDouble(), fh.toDouble(), w, h, pixels)
    }

    Box(
        modifier
            .fillMaxSize()
            .pointerInput(fw, fh) {
                fun g(light: SceneLight) = geometry(
                    light,
                    size.width.toDouble(),
                    size.height.toDouble(),
                    min(62.dp.toPx(), min(size.width, size.height) * .28f).toDouble(),
                )
                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = false)
                    if (down.isConsumed || lights.isEmpty()) return@awaitEachGesture
                    val selected = lights.firstOrNull { it.id == ed.selectedLightId }

                    var dragKind: LightDragKind? = null
                    var hitMoveAxis = -1
                    if (selected != null) {
                        val geo = g(selected)
                        val spec = buildLightConeSpec(selected, geo.radius)
                        if (spec.hasAimTarget) {
                            val aimPt = geo.project(geo.origin + spec.emitDir * spec.beamLength)?.screen
                            if (aimPt != null && (aimPt - down.position).getDistance() <= 18.dp.toPx()) {
                                dragKind = LightDragKind.AimTarget
                            }
                        }
                        if (dragKind == null) {
                            val moveHit = geo.hitGizmo(down.position, 16.dp.toPx(), setOf(GizmoMode.Move))
                            if (moveHit != null) {
                                dragKind = LightDragKind.MoveAxis
                                hitMoveAxis = moveHit.axis
                            }
                        }
                    }

                    if (dragKind != null && selected != null) {
                        ed.stop()
                        ed.selectedLightId = selected.id
                        down.consume()
                        activeAxis = if (dragKind == LightDragKind.MoveAxis) hitMoveAxis else null
                        aimingLight = dragKind == LightDragKind.AimTarget
                        var current: SceneLight = selected
                        var previous = down.position
                        var total = Offset.Zero
                        var changed = false
                        val token = "light-gizmo:${java.util.UUID.randomUUID()}"
                        try {
                            while (true) {
                                val event = awaitPointerEvent()
                                if (event.changes.count { it.pressed } > 1) break
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
                                val geo = g(current)
                                if (dragKind == LightDragKind.MoveAxis) {
                                    val tangent = geo.tangent(hitMoveAxis)
                                    val length = tangent.x * tangent.x + tangent.y * tangent.y
                                    val amount = if (length > .0001f) {
                                        ((drag.x * tangent.x + drag.y * tangent.y) / length).toDouble()
                                    } else {
                                        -drag.y * geo.radius / 64.0
                                    }
                                    val p = current.position.toMutableList()
                                    p[hitMoveAxis] += amount
                                    val in3dView = viewFor(ed, fw.toDouble(), fh.toDouble(), size.width.toDouble(), size.height.toDouble()) != null
                                    if (hitMoveAxis == 2) {
                                        p[2] = if (in3dView) p[2].coerceIn(-4000.0, 10000.0) else p[2].coerceAtLeast(20.0)
                                    }
                                    current = current.copy(position = p)
                                } else {
                                    val spec = buildLightConeSpec(current, geo.radius)
                                    val baseWorld = geo.origin + spec.emitDir * spec.beamLength
                                    val baseScreen = geo.project(baseWorld)?.screen
                                    val eps = (spec.beamLength * 0.02).coerceAtLeast(1.0)
                                    val uScreen = geo.project(baseWorld + spec.basisU * eps)?.screen
                                    val vScreen = geo.project(baseWorld + spec.basisV * eps)?.screen
                                    if (baseScreen != null && uScreen != null && vScreen != null) {
                                        val tu = (uScreen - baseScreen) / eps.toFloat()
                                        val tv = (vScreen - baseScreen) / eps.toFloat()
                                        val det = tu.x * tv.y - tu.y * tv.x
                                        val (du, dv) = if (abs(det) > 1e-5f) {
                                            val invDet = 1f / det
                                            ((drag.x * tv.y - drag.y * tv.x) * invDet).toDouble() to
                                                ((tu.x * drag.y - tu.y * drag.x) * invDet).toDouble()
                                        } else {
                                            val lu = tu.x * tu.x + tu.y * tu.y
                                            val lv = tv.x * tv.x + tv.y * tv.y
                                            val stepU = if (lu > 1e-5f) ((drag.x * tu.x + drag.y * tu.y) / lu).toDouble() else 0.0
                                            val stepV = if (lv > 1e-5f) ((drag.x * tv.x + drag.y * tv.y) / lv).toDouble() else 0.0
                                            stepU to stepV
                                        }
                                        val nextTarget = baseWorld + spec.basisU * du + spec.basisV * dv
                                        val nextEmit = (nextTarget - geo.origin).unit()
                                        val (ang, elev) = lightAnglesFromEmit(nextEmit)
                                        current = current.copy(angle = ang, elevation = elev)
                                    }
                                }
                                ed.setLight(current, token)
                                changed = true
                                change.consume()
                            }
                        } finally {
                            activeAxis = null
                            aimingLight = false
                        }
                    } else {
                        val hitBody = lights.minByOrNull { (g(it).centre - down.position).getDistance() }
                            ?.takeIf { (g(it).centre - down.position).getDistance() < 22.dp.toPx() }
                            ?: return@awaitEachGesture
                        down.consume()
                        var previous = down.position
                        var total = Offset.Zero
                        while (true) {
                            val event = awaitPointerEvent()
                            if (event.changes.count { it.pressed } > 1) break
                            val change = event.changes.firstOrNull { it.id == down.id } ?: break
                            if (!change.pressed) {
                                if (total.getDistance() < viewConfiguration.touchSlop) {
                                    ed.stop()
                                    ed.selectedLightId = hitBody.id
                                }
                                change.consume()
                                break
                            }
                            total += change.position - previous
                            previous = change.position
                            change.consume()
                            if (total.getDistance() >= viewConfiguration.touchSlop) break
                        }
                    }
                }
            }
            .drawBehind {
                if (size.width <= 0f || size.height <= 0f || fw <= 0f || fh <= 0f) return@drawBehind
                val axisColors = listOf(Color(0xFFFF6575), Color(0xFF6FE295), Color(0xFF669FFF))
                val pixels = min(62.dp.toPx(), min(size.width, size.height) * .28f).toDouble()
                for (light in ed.sceneLights.lights) {
                    val geo = geometry(light, size.width.toDouble(), size.height.toDouble(), pixels)
                    val isSelected = light.id == ed.selectedLightId
                    drawBlenderLightCone(light, geo, isSelected, aimingLight && isSelected)
                    if (isSelected) {
                        for (face in geo.faces(GizmoMode.Move)) {
                            val path = Path().apply {
                                moveTo(face.points[0].screen.x, face.points[0].screen.y)
                                face.points.drop(1).forEach { lineTo(it.screen.x, it.screen.y) }
                                close()
                            }
                            val base = axisColors[face.axis]
                            val highlight = activeAxis == face.axis
                            drawPath(
                                path,
                                Color(
                                    (base.red * face.brightness + if (highlight) .2f else 0f).coerceAtMost(1f),
                                    (base.green * face.brightness + if (highlight) .2f else 0f).coerceAtMost(1f),
                                    (base.blue * face.brightness + if (highlight) .2f else 0f).coerceAtMost(1f),
                                    if (highlight) 1f else .88f,
                                ),
                            )
                        }
                    }
                }
            },
        content = content,
    )
}

private fun DrawScope.drawBlenderLightCone(
    light: SceneLight,
    geo: GizmoGeometry,
    selected: Boolean,
    aiming: Boolean,
) {
    val originScreen = geo.project(geo.origin)?.screen ?: return
    val rawTint = Color(
        light.color.getOrElse(0) { 1f }.coerceIn(0f, 1f),
        light.color.getOrElse(1) { 1f }.coerceIn(0f, 1f),
        light.color.getOrElse(2) { 1f }.coerceIn(0f, 1f),
    )
    val strength = if (!light.enabled) 0.25f else (0.45f + 0.55f * (light.intensity / 2.5).coerceIn(0.0, 1.0).toFloat())
    val strokeAlpha = (if (selected) 0.95f else 0.62f) * strength
    val secondaryAlpha = (if (selected) 0.55f else 0.34f) * strength
    val tint = rawTint.copy(alpha = strokeAlpha.coerceIn(0.18f, 1f))
    val softTint = rawTint.copy(alpha = secondaryAlpha.coerceIn(0.12f, 0.85f))
    val dash = PathEffect.dashPathEffect(floatArrayOf(5.dp.toPx(), 4.dp.toPx()))
    val lineStroke = if (selected) 2.1.dp.toPx() else 1.4.dp.toPx()

    val spec = buildLightConeSpec(light, geo.radius)
    val baseCenter = geo.origin + spec.emitDir * spec.beamLength

    fun drawProjectedRing(center: GizmoVec, u: GizmoVec, v: GizmoVec, radius: Double, color: Color, widthPx: Float, dashed: Boolean = false) {
        val segments = 48
        val path = Path()
        var started = false
        for (i in 0..segments) {
            val th = i * 2.0 * PI / segments
            val pt = geo.project(center + (u * cos(th) + v * sin(th)) * radius)?.screen
            if (pt != null) {
                if (!started) {
                    path.moveTo(pt.x, pt.y)
                    started = true
                } else {
                    path.lineTo(pt.x, pt.y)
                }
            } else {
                started = false
            }
        }
        if (started) {
            drawPath(
                path,
                color,
                style = Stroke(width = widthPx, pathEffect = if (dashed) dash else null),
            )
        }
    }

    if (light.kind == "spot" || light.kind == "directional") {
        // 4 clean Blender cone silhouette generators + subtle translucent cone wash when selected.
        val cardinals = listOf(spec.basisU, spec.basisV, spec.basisU * -1.0, spec.basisV * -1.0)
        val rimPts = cardinals.map { dir -> geo.project(baseCenter + dir * spec.outerRadius)?.screen }
        if (selected && light.enabled) {
            for (k in 0 until 4) {
                val a = rimPts[k]
                val b = rimPts[(k + 1) % 4]
                if (a != null && b != null) {
                    val wash = Path().apply {
                        moveTo(originScreen.x, originScreen.y)
                        lineTo(a.x, a.y)
                        lineTo(b.x, b.y)
                        close()
                    }
                    drawPath(wash, rawTint.copy(alpha = (0.09f * strength).coerceIn(0.03f, 0.14f)))
                }
            }
        }
        for (rim in rimPts) {
            if (rim != null) {
                drawLine(
                    tint,
                    originScreen,
                    rim,
                    strokeWidth = lineStroke,
                    cap = StrokeCap.Round,
                    pathEffect = if (!light.enabled) dash else null,
                )
            }
        }

        // Outer cone base circle.
        drawProjectedRing(baseCenter, spec.basisU, spec.basisV, spec.outerRadius, tint, lineStroke, dashed = !light.enabled)

        // Inner softness penumbra circle for spotlights (reflects `softness` dynamically).
        if (spec.innerRadius != null) {
            drawProjectedRing(baseCenter, spec.basisU, spec.basisV, spec.innerRadius, softTint, (lineStroke * 0.78f).coerceAtLeast(1f), dashed = true)
        }

        // Extra parallel sun rays for directional lights so sun vs spot is immediately distinct.
        if (light.kind == "directional") {
            val rayLen = spec.beamLength * 0.42
            for (dir in cardinals) {
                val r0 = geo.project(geo.origin + dir * (spec.outerRadius * 0.65))?.screen
                val r1 = geo.project(geo.origin + dir * (spec.outerRadius * 0.65) + spec.emitDir * rayLen)?.screen
                if (r0 != null && r1 != null) {
                    drawLine(softTint, r0, r1, strokeWidth = (lineStroke * 0.8f).coerceAtLeast(1f), cap = StrokeCap.Round)
                }
            }
        }

        // Central aim axis + draggable Blender-style aim target dot at the cone base center.
        val aimScreen = geo.project(baseCenter)?.screen
        if (aimScreen != null) {
            drawLine(softTint, originScreen, aimScreen, strokeWidth = 1.2.dp.toPx(), pathEffect = dash)
            val handleRadius = if (aiming) 6.5.dp.toPx() else if (selected) 5.2.dp.toPx() else 3.4.dp.toPx()
            drawCircle(Color.Black.copy(alpha = 0.55f), handleRadius + 1.6.dp.toPx(), aimScreen)
            drawCircle(if (aiming) Color.White else tint, handleRadius, aimScreen)
            if (selected) {
                drawCircle(Color.White.copy(alpha = 0.9f), handleRadius + 2.5.dp.toPx(), aimScreen, style = Stroke(1.3.dp.toPx()))
            }
        }
    } else {
        // Point lamp: clean single screen-space radial ticks + optional single falloff circle (no wireframe sphere!).
        val tickInner = 9.dp.toPx()
        val tickOuter = (13.dp.toPx() + 4.dp.toPx() * (light.intensity / 3.0).coerceIn(0.0, 1.0).toFloat())
        for (i in 0 until 8) {
            val th = (i * PI / 4.0).toFloat()
            val dir = Offset(cos(th), sin(th))
            drawLine(softTint, originScreen + dir * tickInner, originScreen + dir * tickOuter, strokeWidth = 1.5.dp.toPx(), cap = StrokeCap.Round)
        }
        if (light.range > 0.0) {
            val edge = geo.project(geo.origin + GizmoVec(spec.outerRadius, 0.0, 0.0))?.screen
                ?: geo.project(geo.origin + GizmoVec(0.0, spec.outerRadius, 0.0))?.screen
            if (edge != null) {
                val screenR = (edge - originScreen).getDistance().coerceIn(16.dp.toPx(), min(size.width, size.height) * 0.48f)
                drawCircle(softTint, screenR, originScreen, style = Stroke(1.3.dp.toPx(), pathEffect = dash))
            }
        }
    }

    // Crisp Blender-style light emitter dot at origin.
    val glowRadius = (9.dp.toPx() + 6.dp.toPx() * (light.intensity / 3.0).coerceIn(0.0, 1.5).toFloat())
    if (light.enabled && light.intensity > 0.01) {
        drawCircle(rawTint.copy(alpha = (0.20f * strength).coerceIn(0.05f, 0.28f)), glowRadius, originScreen)
    }
    val outerDot = if (selected) 7.dp.toPx() else 5.8.dp.toPx()
    drawCircle(Color.Black.copy(alpha = 0.65f), outerDot + 1.4.dp.toPx(), originScreen)
    drawCircle(tint, outerDot, originScreen, style = Stroke(if (selected) 2.1.dp.toPx() else 1.6.dp.toPx()))
    drawCircle(if (light.enabled) rawTint else Color.Gray, 2.8.dp.toPx(), originScreen)
    if (light.animated) {
        // Subtle diamond indicator above keyframed lights.
        val d = 3.5.dp.toPx()
        val top = Offset(originScreen.x, originScreen.y - outerDot - 6.dp.toPx())
        val diamond = Path().apply {
            moveTo(top.x, top.y - d)
            lineTo(top.x + d, top.y)
            lineTo(top.x, top.y + d)
            lineTo(top.x - d, top.y)
            close()
        }
        drawPath(diamond, Color(0xFFFFC947))
    }
}
