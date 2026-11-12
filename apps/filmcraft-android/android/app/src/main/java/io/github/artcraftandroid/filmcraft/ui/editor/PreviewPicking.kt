package io.github.artcraftandroid.filmcraft.ui.editor

import androidx.compose.ui.geometry.Offset
import io.github.artcraftandroid.filmcraft.core.Placement
import kotlin.math.*

/** Inverse layer transform: rotated/scaled text and overlays keep their own hit rectangle. */
internal fun placementContains(p: Placement, point: Offset, tolerance: Double): Boolean {
    if (!listOf(p.x, p.y, p.width, p.height, p.scale, p.rotation).all { it.isFinite() }) return false
    val a = Math.toRadians(p.rotation)
    val dx = point.x - p.x; val dy = point.y - p.y
    val x = dx * cos(a) + dy * sin(a)
    val y = -dx * sin(a) + dy * cos(a)
    return abs(x) <= abs(p.width * p.scale) / 2 + tolerance && abs(y) <= abs(p.height * p.scale) / 2 + tolerance
}

/** Only runs on a tap, not every preview frame. Higher video lanes composite on top. */
internal suspend fun EditorController.pickPreview(point: Offset, tolerance: Double): ULong? {
    val snapshot = timeline ?: return null
    val time = playhead
    for (lane in snapshot.lanes.filter { it.index >= 0 }.sortedByDescending { it.index }) {
        for (clip in lane.clips.asReversed()) {
            if (time < clip.start || time - clip.start >= clip.duration || clip.opacity <= 0.0) continue
            val p = placementOf(clip.id) ?: continue
            if (!clip.has3d) {
                if (placementContains(p, point, tolerance)) return clip.id
                continue
            }
            // Do not use the full backing layer as a 3D object's hit region.
            val state = previewObjectSettings(clip.id) ?: continue
            val g = GizmoGeometry(state, p, snapshot.width.toDouble(), snapshot.height.toDouble(), snapshot.width.toDouble(), snapshot.height.toDouble(), 1.0)
            val half = state.size * state.scale / 200.0
            val halfDepth = if (state.depth > 0) state.depth * state.scale / 200.0 else half
            val halfWidth = if (state.text.isNotBlank()) half * state.text.lines().maxOf { it.length }.coerceAtLeast(1) * .7 else half
            val halfHeight = if (state.text.isNotBlank()) half * state.text.lines().size else half
            val points = buildList {
                for (x in listOf(-halfWidth, halfWidth)) for (y in listOf(-halfHeight, halfHeight)) for (z in listOf(-halfDepth, halfDepth)) {
                    g.local(GizmoVec(x, y, z))?.screen?.let { add(it) }
                }
            }
            if (points.isNotEmpty() && point.x >= points.minOf { it.x } - tolerance && point.x <= points.maxOf { it.x } + tolerance &&
                point.y >= points.minOf { it.y } - tolerance && point.y <= points.maxOf { it.y } + tolerance) return clip.id
        }
    }
    return null
}
