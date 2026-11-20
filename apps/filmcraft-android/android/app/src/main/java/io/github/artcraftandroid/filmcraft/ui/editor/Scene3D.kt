package io.github.artcraftandroid.filmcraft.ui.editor

import io.github.artcraftandroid.filmcraft.core.SceneCamera3d
import org.json.JSONObject

/**
 * The camera the preview is rendering through right now, as a [ViewProjection] for overlay
 * gizmos — the orbit view while the scene panel is open, the sequence's scene camera
 * otherwise, `null` for the classic fixed camera.
 */
internal fun viewFor(ed: EditorController, fw: Double, fh: Double, w: Double, h: Double): ViewProjection? {
    if (ed.panel == Panel.Scene) {
        val o = ed.sceneOrbit
        val fov = ed.sceneCamera?.fov ?: CLASSIC_FOV
        return ViewProjection(GizmoVec(o.target[0], o.target[1], o.target[2]), o.yaw, o.pitch, 0.0, fov, fw, fh, w, h, o.distance)
    }
    val c = ed.sceneCamera ?: return null
    return ViewProjection(GizmoVec(c.position[0], c.position[1], c.position[2]), c.yaw, c.pitch, c.roll, c.fov, fw, fh, w, h)
}

/** The vertical field of view in degrees whose focal equals the frame height (classic camera). */
internal const val CLASSIC_FOV: Double = 53.130_102

/** Default orbit distance from the orbit-target point, in world units. */
internal const val DEFAULT_ORBIT_DISTANCE: Double = 1700.0

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
