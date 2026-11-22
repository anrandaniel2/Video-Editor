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
    val r = gizmoRadius.coerceAtLeast(24.0)
    val rangeFactor = if (light.range > 0.0) {
        1.65 + 2.55 * (light.range / 2000.0).coerceIn(0.05, 1.5)
    } else {
        2.85
    }
    val beamLen = when (light.kind) {
        "directional" -> r * 2.55
        else -> r * rangeFactor
    }
    return when (light.kind) {
        "spot" -> {
            val coneDeg = light.cone.coerceIn(4.0, 82.0)
            val outerR = (beamLen * tan(Math.toRadians(coneDeg))).coerceIn(r * 0.16, beamLen * 2.25)
            val soft = light.softness.coerceIn(0.0, 1.0)
            val innerR = if (soft > 0.03) {
                val innerDeg = (coneDeg * (1.0 - 0.92 * soft)).coerceAtLeast(1.0)
                (beamLen * tan(Math.toRadians(innerDeg))).coerceIn(r * 0.04, outerR * 0.97)
            } else {
                null
            }
            LightConeSpec(emit, u, v, beamLen, outerR, innerR, hasAimTarget = true)
        }
        "directional" -> {
            val outerR = beamLen * tan(Math.toRadians(20.0))
            LightConeSpec(emit, u, v, beamLen, outerR, innerRadius = null, hasAimTarget = true)
        }
        else -> {
            val outerR = if (light.range > 0.0) (r * (0.65 + 1.45 * (light.range / 2000.0).coerceIn(0.05, 1.5))) else r * 0.85
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
    val strength = if (!light.enabled) 0.28f else (0.55f + 0.45f * (light.intensity / 2.5).coerceIn(0.0, 1.0).toFloat())
    val strokeAlpha = (if (selected) 0.98f else 0.78f) * strength
    val secondaryAlpha = (if (selected) 0.68f else 0.46f) * strength
    val tint = rawTint.copy(alpha = strokeAlpha.coerceIn(0.24f, 1f))
    val softTint = rawTint.copy(alpha = secondaryAlpha.coerceIn(0.16f, 0.9f))
    val haloColor = Color.Black.copy(alpha = if (selected) 0.62f else 0.42f)
    val dash = PathEffect.dashPathEffect(floatArrayOf(6.dp.toPx(), 4.dp.toPx()))
    val lineStroke = if (selected) 2.5.dp.toPx() else 1.8.dp.toPx()

    val spec = buildLightConeSpec(light, geo.radius)
    val baseCenter = geo.origin + spec.emitDir * spec.beamLength

    fun projectRingPoints(center: GizmoVec, u: GizmoVec, v: GizmoVec, radius: Double, segments: Int = 36): List<GizmoPoint?> =
        (0 until segments).map { i ->
            val th = i * 2.0 * PI / segments
            geo.project(center + (u * cos(th) + v * sin(th)) * radius)
        }

    fun drawProjectedRing(pts: List<GizmoPoint?>, color: Color, widthPx: Float, dashed: Boolean = false, withHalo: Boolean = false, fillAlpha: Float = 0f) {
        val path = Path()
        var started = false
        var validCount = 0
        for (i in 0..pts.size) {
            val pt = pts[i % pts.size]?.screen
            if (pt != null) {
                validCount++
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
        if (validCount >= 3) {
            if (fillAlpha > 0f) {
                drawPath(path, rawTint.copy(alpha = fillAlpha))
            }
            if (withHalo) {
                drawPath(path, haloColor, style = Stroke(width = widthPx + 2.4.dp.toPx(), pathEffect = if (dashed) dash else null))
            }
            drawPath(path, color, style = Stroke(width = widthPx, pathEffect = if (dashed) dash else null))
        }
    }

    // Helper: draw true 2D perspective tangent silhouette lines from apexScreen to a projected 3D ring.
    fun drawConeTangentEdges(apexScreen: Offset, pts: List<GizmoPoint?>, color: Color, widthPx: Float, dashed: Boolean = false, withHalo: Boolean = false) {
        val screens = pts.mapNotNull { it?.screen }
        if (screens.size < 6) return
        val center = screens.fold(Offset.Zero) { acc, p -> acc + p } / screens.size.toFloat()
        val refVec = center - apexScreen
        val refLen = refVec.getDistance()
        if (refLen < 4f) {
            // Looking nearly straight down the cone barrel: draw 4 clean symmetric generator spokes.
            for (k in listOf(0, pts.size / 4, pts.size / 2, (pts.size * 3) / 4)) {
                pts.getOrNull(k)?.screen?.let { rim ->
                    if (withHalo) drawLine(haloColor, apexScreen, rim, strokeWidth = widthPx + 2.2.dp.toPx(), cap = StrokeCap.Round)
                    drawLine(color, apexScreen, rim, strokeWidth = widthPx, cap = StrokeCap.Round, pathEffect = if (dashed) dash else null)
                }
            }
            return
        }
        val refDir = refVec / refLen
        var minAng = Double.POSITIVE_INFINITY
        var maxAng = Double.NEGATIVE_INFINITY
        var leftPt: Offset? = null
        var rightPt: Offset? = null
        for (p in screens) {
            val d = p - apexScreen
            if (d.getDistance() < 2f) continue
            val cross = (refDir.x * d.y - refDir.y * d.x).toDouble()
            val dot = (refDir.x * d.x + refDir.y * d.y).toDouble()
            val ang = atan2(cross, dot)
            if (ang < minAng) { minAng = ang; leftPt = p }
            if (ang > maxAng) { maxAng = ang; rightPt = p }
        }
        for (edge in listOfNotNull(leftPt, rightPt)) {
            if (withHalo) {
                drawLine(haloColor, apexScreen, edge, strokeWidth = widthPx + 2.4.dp.toPx(), cap = StrokeCap.Round, pathEffect = if (dashed) dash else null)
            }
            drawLine(color, apexScreen, edge, strokeWidth = widthPx, cap = StrokeCap.Round, pathEffect = if (dashed) dash else null)
        }
    }

    if (light.kind == "spot" || light.kind == "directional") {
        val segs = 32
        val outerRingPts = projectRingPoints(baseCenter, spec.basisU, spec.basisV, spec.outerRadius, segs)

        // 1. Volumetric back-to-front shaded 3D beam cone facets so the cone volume is unmistakable.
        if (light.enabled) {
            data class ConeFacet(val a: Offset, val b: Offset, val depth: Double, val shade: Float)
            val facets = ArrayList<ConeFacet>(segs)
            val originPt = geo.project(geo.origin)
            if (originPt != null) {
                for (i in 0 until segs) {
                    val j = (i + 1) % segs
                    val pa = outerRingPts[i] ?: continue
                    val pb = outerRingPts[j] ?: continue
                    val thMid = (i + 0.5) * 2.0 * PI / segs
                    val radial = spec.basisU * cos(thMid) + spec.basisV * sin(thMid)
                    val shade = (0.45 + 0.55 * abs(radial.y)).toFloat()
                    facets.add(ConeFacet(pa.screen, pb.screen, (originPt.depth + pa.depth + pb.depth) / 3.0, shade))
                }
                facets.sortByDescending { it.depth }
                val baseWash = if (selected) 0.13f else 0.075f
                for (f in facets) {
                    val triPath = Path().apply {
                        moveTo(originScreen.x, originScreen.y)
                        lineTo(f.a.x, f.a.y)
                        lineTo(f.b.x, f.b.y)
                        close()
                    }
                    drawPath(triPath, rawTint.copy(alpha = (baseWash * f.shade * strength).coerceIn(0.02f, 0.22f)))
                }
            }
        }

        // 2. Subtle 4-quadrant longitudinal ribs so 3D rotation/perspective of the cone reads clearly.
        for (q in listOf(0, segs / 4, segs / 2, (segs * 3) / 4)) {
            val rim = outerRingPts[q]?.screen ?: continue
            drawLine(softTint.copy(alpha = (softTint.alpha * 0.55f).coerceAtLeast(0.12f)), originScreen, rim, strokeWidth = 1.1.dp.toPx(), cap = StrokeCap.Round, pathEffect = if (!light.enabled) dash else null)
        }

        // 3. True 2D perspective tangent silhouette sides + projected 3D base ellipse with contrast halo.
        drawConeTangentEdges(originScreen, outerRingPts, tint, lineStroke, dashed = !light.enabled, withHalo = true)
        drawProjectedRing(
            outerRingPts,
            tint,
            lineStroke,
            dashed = !light.enabled,
            withHalo = true,
            fillAlpha = if (light.enabled) ((if (selected) 0.10f else 0.055f) * strength) else 0f,
        )

        // 4. Inner softness penumbra 3D cone (updates live with `softness` & `cone`!).
        if (spec.innerRadius != null) {
            val innerPts = projectRingPoints(baseCenter, spec.basisU, spec.basisV, spec.innerRadius, segs)
            drawConeTangentEdges(originScreen, innerPts, softTint, (lineStroke * 0.72f).coerceAtLeast(1.1f), dashed = true, withHalo = false)
            drawProjectedRing(innerPts, softTint, (lineStroke * 0.8f).coerceAtLeast(1.2f), dashed = true)
        }

        // 5. Parallel sun-ray streaks around directional cones so sun vs spot stays clear.
        if (light.kind == "directional") {
            val rayLen = spec.beamLength * 0.45
            for (dir in listOf(spec.basisU, spec.basisV, spec.basisU * -1.0, spec.basisV * -1.0)) {
                val r0 = geo.project(geo.origin + dir * (spec.outerRadius * 0.72))?.screen
                val r1 = geo.project(geo.origin + dir * (spec.outerRadius * 0.72) + spec.emitDir * rayLen)?.screen
                if (r0 != null && r1 != null) {
                    drawLine(haloColor, r0, r1, strokeWidth = lineStroke + 1.8.dp.toPx(), cap = StrokeCap.Round)
                    drawLine(softTint, r0, r1, strokeWidth = lineStroke * 0.85f, cap = StrokeCap.Round)
                }
            }
        }

        // 6. Solid 3D shaded Lamp Housing Cone Cap right at the emitter (`origin -> lampRim`)!
        val capSegs = 18
        val capLen = (geo.radius * 0.46).coerceAtMost(spec.beamLength * 0.32)
        val capRad = (capLen * tan(Math.toRadians(if (light.kind == "spot") light.cone.coerceIn(16.0, 52.0) else 26.0))).coerceIn(geo.radius * 0.16, geo.radius * 0.42)
        val capCenter = geo.origin + spec.emitDir * capLen
        val capRingPts = projectRingPoints(capCenter, spec.basisU, spec.basisV, capRad, capSegs)
        val originProj = geo.project(geo.origin)
        if (originProj != null) {
            data class CapTri(val a: Offset, val b: Offset, val depth: Double, val lit: Float)
            val capTris = ArrayList<CapTri>(capSegs)
            for (i in 0 until capSegs) {
                val j = (i + 1) % capSegs
                val pa = capRingPts[i] ?: continue
                val pb = capRingPts[j] ?: continue
                val th = (i + 0.5) * 2.0 * PI / capSegs
                val n = (spec.basisU * cos(th) + spec.basisV * sin(th) - spec.emitDir * 0.35).unit()
                val lit = (0.35 + 0.65 * ((-n.y) * 0.5 + 0.5)).toFloat().coerceIn(0.25f, 1f)
                capTris.add(CapTri(pa.screen, pb.screen, (originProj.depth + pa.depth + pb.depth) / 3.0, lit))
            }
            capTris.sortByDescending { it.depth }
            for (ct in capTris) {
                val p = Path().apply {
                    moveTo(originScreen.x, originScreen.y)
                    lineTo(ct.a.x, ct.a.y)
                    lineTo(ct.b.x, ct.b.y)
                    close()
                }
                val bodyCol = Color(
                    (0.16f + rawTint.red * 0.55f) * ct.lit,
                    (0.16f + rawTint.green * 0.55f) * ct.lit,
                    (0.18f + rawTint.blue * 0.55f) * ct.lit,
                    if (selected) 0.90f else 0.76f,
                )
                drawPath(p, bodyCol)
            }
            // Bright glowing bulb lens face across the open mouth of the 3D lamp cone!
            drawProjectedRing(
                capRingPts,
                tint,
                (lineStroke * 0.9f).coerceAtLeast(1.4f),
                withHalo = true,
                fillAlpha = if (light.enabled) (0.48f * strength).coerceIn(0.18f, 0.72f) else 0.12f,
            )
            drawConeTangentEdges(originScreen, capRingPts, tint, (lineStroke * 0.95f).coerceAtLeast(1.5f), withHalo = true)
        }

        // 7. Central aim axis + draggable Blender-style aim target handle at the cone base center.
        val aimScreen = geo.project(baseCenter)?.screen
        if (aimScreen != null) {
            drawLine(haloColor, originScreen, aimScreen, strokeWidth = 2.8.dp.toPx(), pathEffect = dash)
            drawLine(softTint, originScreen, aimScreen, strokeWidth = 1.4.dp.toPx(), pathEffect = dash)
            val handleRadius = if (aiming) 7.5.dp.toPx() else if (selected) 6.0.dp.toPx() else 4.2.dp.toPx()
            drawCircle(Color.Black.copy(alpha = 0.68f), handleRadius + 2.dp.toPx(), aimScreen)
            drawCircle(if (aiming) Color.White else tint, handleRadius, aimScreen)
            if (selected) {
                drawCircle(Color.White.copy(alpha = 0.92f), handleRadius + 2.8.dp.toPx(), aimScreen, style = Stroke(1.5.dp.toPx()))
            }
        }
    } else {
        // Point lamp: bright 3D radial starburst + 3D equatorial falloff ring.
        val tickInner = 10.dp.toPx()
        val tickOuter = (16.dp.toPx() + 5.dp.toPx() * (light.intensity / 3.0).coerceIn(0.0, 1.0).toFloat())
        for (i in 0 until 8) {
            val th = (i * PI / 4.0).toFloat()
            val dir = Offset(cos(th), sin(th))
            drawLine(haloColor, originScreen + dir * tickInner, originScreen + dir * tickOuter, strokeWidth = 3.2.dp.toPx(), cap = StrokeCap.Round)
            drawLine(tint, originScreen + dir * tickInner, originScreen + dir * tickOuter, strokeWidth = 1.8.dp.toPx(), cap = StrokeCap.Round)
        }
        val ringPts = projectRingPoints(geo.origin, GizmoVec(1.0, 0.0, 0.0), GizmoVec(0.0, 0.0, 1.0), spec.outerRadius, 36)
        drawProjectedRing(ringPts, softTint, 1.5.dp.toPx(), dashed = true, withHalo = true)
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
