package io.github.artcraftandroid.filmcraft.ui.editor

import androidx.compose.ui.graphics.Color
import io.github.artcraftandroid.filmcraft.core.SceneCamera3d
import org.json.JSONObject

/**
 * The camera the preview is rendering through right now, as a [ViewProjection] for overlay
 * gizmos — the orbit view while the scene panel is open, the sequence's effective render camera
 * (active match-move pose, else manual scene camera) otherwise, `null` for the classic fixed camera.
 */
internal fun viewFor(ed: EditorController, fw: Double, fh: Double, w: Double, h: Double): ViewProjection? {
    if (ed.panel == Panel.Scene) {
        val o = ed.sceneOrbit
        val fov = ed.activeRenderCamera?.fov ?: CLASSIC_FOV
        return ViewProjection(GizmoVec(o.target[0], o.target[1], o.target[2]), o.yaw, o.pitch, 0.0, fov, fw, fh, w, h, o.distance)
    }
    return cameraProjection(ed.activeRenderCamera, fw, fh, w, h)
}

/** Effective render camera at the playhead: active match-move pose takes precedence over manual scene camera. */
internal fun effectiveRenderCamera(matchmoveCamera: SceneCamera?, manualCamera: SceneCamera?): SceneCamera? =
    matchmoveCamera ?: manualCamera

/** Build a [ViewProjection] from a resolved [SceneCamera], or `null` when using the classic fixed camera. */
internal fun cameraProjection(c: SceneCamera?, fw: Double, fh: Double, w: Double, h: Double): ViewProjection? {
    if (c == null) return null
    return ViewProjection(GizmoVec(c.position[0], c.position[1], c.position[2]), c.yaw, c.pitch, c.roll, c.fov, fw, fh, w, h)
}

/** Colour-code solved 3D landmarks by reprojection RMS error in working pixels. */
internal fun landmarkColor(rmsPx: Double): Color = when {
    rmsPx <= 1.0 -> Color(0xFF6FE295)
    rmsPx <= 2.5 -> Color(0xFFFFC947)
    else -> Color(0xFFFF6575)
}

/**
 * Convert a tap `(tapX, tapY)` in a preview container of size `(viewW, viewH)` to normalized
 * `0..1` frame coordinates for a fitted `(frameW, frameH)` video picture, or `null` if the tap
 * falls on letterbox/pillarbox bars or dimensions are non-positive.
 */
internal fun frameNormalizedAtTap(
    tapX: Float,
    tapY: Float,
    viewW: Float,
    viewH: Float,
    frameW: Float,
    frameH: Float,
): Pair<Double, Double>? {
    if (viewW <= 0f || viewH <= 0f || frameW <= 0f || frameH <= 0f) return null
    val k = kotlin.math.min(viewW / frameW, viewH / frameH)
    if (k <= 0f || !k.isFinite()) return null
    val iw = frameW * k
    val ih = frameH * k
    val ox = (viewW - iw) * 0.5f
    val oy = (viewH - ih) * 0.5f
    if (tapX < ox || tapX > ox + iw || tapY < oy || tapY > oy + ih) return null
    val nx = ((tapX - ox) / iw).toDouble().coerceIn(0.0, 1.0)
    val ny = ((tapY - oy) / ih).toDouble().coerceIn(0.0, 1.0)
    return nx to ny
}

/** The vertical field of view in degrees whose focal equals the frame height (classic camera). */
internal const val CLASSIC_FOV: Double = 53.130_102

/** Default orbit distance from the orbit-target point, in world units. 700 puts the target at the
 * classic camera's size on screen — the scene opens at the same scale the editor was showing. */
internal const val DEFAULT_ORBIT_DISTANCE: Double = 700.0

/**
 * The camera a sequence's 3D layers render through. `null` in the editor means the classic
 * fixed camera. Mirrors the engine's `SceneCamera3D` (frame pixels, frame-centred world;
 * x right, y down, z away from the viewer).
 */
data class SceneCamera(
    val position: List<Double> = listOf(0.0, 0.0, 0.0),
    val yaw: Double = 0.0,
    val pitch: Double = 0.0,
    val roll: Double = 0.0,
    val fov: Double = CLASSIC_FOV,
) {
    fun toJson(merge: String? = null): String {
        val j = JSONObject()
        j.put("position", org.json.JSONArray(position))
        j.put("yaw", yaw)
        j.put("pitch", pitch)
        j.put("roll", roll)
        j.put("fov", fov)
        if (merge != null) j.put("merge", merge)
        return j.toString()
    }

    fun toNative() = SceneCamera3d(
        position = position.map { it },
        yaw = yaw,
        pitch = pitch,
        roll = roll,
        fov = fov,
    )

    companion object {
        val DEFAULT = SceneCamera()

        fun fromNative(c: SceneCamera3d) = SceneCamera(
            position = listOf(
                c.position.getOrElse(0) { 0.0 },
                c.position.getOrElse(1) { 0.0 },
                c.position.getOrElse(2) { 0.0 },
            ),
            yaw = c.yaw,
            pitch = c.pitch,
            roll = c.roll,
            fov = c.fov,
        )

        fun from(json: String): SceneCamera? {
            val j = runCatching { JSONObject(json) }.getOrNull() ?: return null
            if (j.isNull("fov") && !j.has("position")) return null
            val p = j.optJSONArray("position") ?: return null
            if (p.length() < 3) return null
            return SceneCamera(
                position = (0..2).map { p.optDouble(it, 0.0) },
                yaw = j.optDouble("yaw", 0.0),
                pitch = j.optDouble("pitch", 0.0),
                roll = j.optDouble("roll", 0.0),
                fov = j.optDouble("fov", CLASSIC_FOV),
            )
        }
    }
}

/**
 * The editor's grid view: an orbit camera around a movable target point. Editor-only — the
 * render never sees this. [target] is in the scene's world units (frame-centred pixels).
 */
data class SceneOrbit(
    val target: List<Double> = listOf(0.0, 0.0, 700.0),
    val yaw: Double = 32.0,
    val pitch: Double = -24.0,
    val distance: Double = DEFAULT_ORBIT_DISTANCE,
) {
    /** The camera the preview renders through while the scene panel is open. */
    fun toCamera(fov: Double): SceneCamera {
        val f = ViewProjection.forward(pitch, yaw, 0.0)
        return SceneCamera(
            position = listOf(target[0] - f.x * distance, target[1] - f.y * distance, target[2] - f.z * distance),
            yaw = yaw,
            pitch = pitch,
            roll = 0.0,
            fov = fov,
        )
    }

    companion object {
        val DEFAULT = SceneOrbit()
    }
}

/** Update [SceneOrbit] from a single-finger drag `(dx, dy)` in screen pixels. */
internal fun SceneOrbit.orbitBy(dx: Float, dy: Float): SceneOrbit = copy(
    yaw = yaw + dx * 0.35,
    pitch = (pitch - dy * 0.35).coerceIn(-85.0, 85.0),
)

/** Update [SceneOrbit] from a two-finger pinch [zoom] and screen-space [panX]/[panY] (view height [viewHeightPx]). */
internal fun SceneOrbit.dollyAndPanBy(
    zoom: Float,
    panX: Float,
    panY: Float,
    fov: Double,
    viewHeightPx: Float,
): SceneOrbit {
    val nextDist = if (zoom > 0f && zoom.isFinite()) {
        (distance / zoom).coerceIn(150.0, 6000.0)
    } else {
        distance
    }
    val vh = viewHeightPx.coerceAtLeast(1f).toDouble()
    val worldPerPx = (2.0 * nextDist * kotlin.math.tan(Math.toRadians(fov) / 2.0)) / vh
    val right = GizmoVec(1.0, 0.0, 0.0).rotate(pitch, yaw, 0.0)
    val downAxis = GizmoVec(0.0, 1.0, 0.0).rotate(pitch, yaw, 0.0)
    val shift = right * (-panX * worldPerPx) + downAxis * (-panY * worldPerPx)
    return copy(
        target = listOf(
            (target[0] + shift.x).coerceIn(-4000.0, 4000.0),
            (target[1] + shift.y).coerceIn(-4000.0, 4000.0),
            (target[2] + shift.z).coerceIn(-4000.0, 6000.0),
        ),
        distance = nextDist,
    )
}

