package io.github.artcraftandroid.filmcraft.ui.editor

import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.width
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import io.github.artcraftandroid.filmcraft.core.Object3DMesh
import io.github.artcraftandroid.filmcraft.ui.*

/**
 * The scene bar: the bar that rises above the bottom toolbar when the 3D Scene tool is
 * active. Everything the old floating panel did is here as one scrollable row of actions —
 * add an object, add a light, open the light editor, tweak the render camera, toggle the
 * video back in, reset the view — plus a close action. The editor underneath is untouched:
 * the preview keeps the grid overlay and the fused gizmos.
 *
 * The render camera exists by default (auto-added the first time the bar rises, unless the
 * user removed it this session) and is drawn in the viewport as a grabbable frustum. Exports
 * always render through it; the orbit view never exports.
 */
@Composable
internal fun SceneBar(ed: EditorController) {
    var cameraStrip by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        ed.loadSceneCamera()
        if (ed.sceneCamera == null && !ed.sceneCameraRemoved) {
            // The render camera is there by default; its defaults reproduce the classic view.
            ed.addSceneCamera()
        }
        ed.loadLights()
    }
    Column(Modifier.fillMaxWidth().background(Fc.Panel)) {
        HorizontalDivider(color = Fc.Divider, thickness = 1.dp)
        Row(
            Modifier.fillMaxWidth().height(64.dp).horizontalScroll(rememberScrollState())
                .padding(horizontal = 10.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            PanelAction(LucideIcons.X.glyph(), "Close the 3D scene", { ed.panel = null }, tag = "scene_close")
            Object3DMesh.entries.filter { it != Object3DMesh.MODEL }.forEach { mesh ->
                val label = mesh.name.lowercase().replaceFirstChar { it.uppercase() }
                SelectChip("+ $label", false, { ed.addObjectInScene(mesh) }, tag = "scene_bar_add_${mesh.name.lowercase()}")
            }
            BarDivider()
            SelectChip("+ Directional", false, { ed.addLight("directional") }, tag = "scene_bar_add_directional")
            SelectChip("+ Point", false, { ed.addLight("point") }, tag = "scene_bar_add_point")
            SelectChip("+ Spot", false, { ed.addLight("spot") }, tag = "scene_bar_add_spot")
            SelectChip("Lights", false, { ed.panel = Panel.Lights }, tag = "scene_open_lights")
            BarDivider()
            SelectChip("Camera", cameraStrip, { cameraStrip = !cameraStrip }, tag = "scene_camera_toggle")
            SelectChip("Video", ed.sceneShowVideo, { ed.toggleSceneShowVideo() }, tag = "scene_show_video")
            PanelAction(LucideIcons.RotateCcw.glyph(), "Reset view", { ed.setSceneOrbit(SceneOrbit.DEFAULT) }, tag = "scene_reset_view")
        }

        // ---- render camera strip (toggled from the bar)
        val cam = ed.sceneCamera
        if (cameraStrip) {
            Column(
                Modifier.fillMaxWidth().heightIn(max = 264.dp).background(Fc.Panel)
                    .verticalScroll(rememberScrollState()),
            ) {
                Row(
                    Modifier.fillMaxWidth().padding(horizontal = FcSize.PanelPadding, vertical = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.SpaceBetween,
                ) {
                    Text("Render camera", style = FcType.Label, color = Fc.Title)
                    if (cam != null) {
                        SelectChip("Remove camera", false, { ed.clearSceneCamera() }, tag = "scene_remove_camera")
                    } else {
                        SelectChip("Add camera", false, { ed.addSceneCamera() }, tag = "scene_add_camera")
                    }
                }
                if (cam != null) {
                    fun change(next: SceneCamera, k: String?) = ed.setSceneCamera(next, k)
                    AdvancedSlider("Camera X", cam.position[0], -2000.0, 2000.0, 1.0, "scene_cam_x") { v, k -> change(cam.copy(position = listOf(v, cam.position[1], cam.position[2])), k) }
                    AdvancedSlider("Camera Y", cam.position[1], -2000.0, 2000.0, 1.0, "scene_cam_y") { v, k -> change(cam.copy(position = listOf(cam.position[0], v, cam.position[2])), k) }
                    AdvancedSlider("Camera Z", cam.position[2], 0.0, 6000.0, 5.0, "scene_cam_z") { v, k -> change(cam.copy(position = listOf(cam.position[0], cam.position[1], v)), k) }
                    AdvancedSlider("Yaw", cam.yaw, -180.0, 180.0, 1.0, "scene_cam_yaw") { v, k -> change(cam.copy(yaw = v), k) }
                    AdvancedSlider("Pitch", cam.pitch, -180.0, 180.0, 1.0, "scene_cam_pitch") { v, k -> change(cam.copy(pitch = v), k) }
                    AdvancedSlider("Roll", cam.roll, -180.0, 180.0, 1.0, "scene_cam_roll") { v, k -> change(cam.copy(roll = v), k) }
                    AdvancedSlider("Field of view", cam.fov, 10.0, 120.0, 1.0, "scene_cam_fov") { v, k -> change(cam.copy(fov = v), k) }
                }
                Spacer(Modifier.height(8.dp))
            }
        }
    }
}

@Composable
private fun BarDivider() {
    Box(Modifier.width(1.dp).height(28.dp).background(Fc.Divider))
}
