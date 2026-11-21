package io.github.artcraftandroid.filmcraft.ui.editor

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
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
import androidx.compose.ui.unit.dp
import io.github.artcraftandroid.filmcraft.core.Object3DMesh
import io.github.artcraftandroid.filmcraft.ui.*

/** The scene-mode replacement for the bottom toolbar, laid out by the same ToolRow as Audio. */
@Composable
internal fun SceneBar(ed: EditorController) {
    var cameraStrip by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        ed.loadSceneCamera()
        if (ed.sceneCamera == null && !ed.sceneCameraRemoved) ed.addSceneCamera()
        ed.loadLights()
    }

    val tools = buildList {
        Object3DMesh.entries.filter { it != Object3DMesh.MODEL }.forEach { mesh ->
            val label = mesh.name.lowercase().replaceFirstChar { it.uppercase() }
            add(Tool("scene_bar_add_${mesh.name.lowercase()}", "+ $label", Glyphs.Object3D) { ed.addObjectInScene(mesh) })
        }
        add(Tool("scene_bar_add_directional", "+ Directional", LucideIcons.Sun.glyph()) { ed.addLight("directional") })
        add(Tool("scene_bar_add_point", "+ Point", LucideIcons.Circle.glyph()) { ed.addLight("point") })
        add(Tool("scene_bar_add_spot", "+ Spot", LucideIcons.Flashlight.glyph()) { ed.addLight("spot") })
        add(Tool("scene_open_lights", "Lights", LucideIcons.Sun.glyph()) { ed.panel = Panel.Lights })
        add(Tool("scene_camera_toggle", "Camera", Glyphs.Object3D, active = cameraStrip) { cameraStrip = !cameraStrip })
        add(Tool("scene_show_video", if (ed.sceneShowVideo) "Hide video" else "Show video", LucideIcons.Film.glyph(), active = ed.sceneShowVideo) { ed.toggleSceneShowVideo() })
        add(Tool("scene_reset_view", "Reset view", LucideIcons.RotateCcw.glyph()) { ed.setSceneOrbit(SceneOrbit.DEFAULT) })
    }
    Column(Modifier.fillMaxWidth()) {
        HorizontalDivider(color = Fc.Divider, thickness = 1.dp)
        ToolRow(tools, back = { ed.panel = null }, deep = false, modifier = Modifier.fillMaxWidth())
        if (cameraStrip) {
            val cam = ed.sceneCamera
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
