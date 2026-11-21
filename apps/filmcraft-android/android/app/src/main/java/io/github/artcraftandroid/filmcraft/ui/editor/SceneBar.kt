package io.github.artcraftandroid.filmcraft.ui.editor

import androidx.compose.foundation.background
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
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.unit.dp
import androidx.compose.ui.platform.testTag
import io.github.artcraftandroid.filmcraft.core.Object3DMesh
import io.github.artcraftandroid.filmcraft.ui.*

@Composable
internal fun SceneBar(ed: EditorController) {
    var cameraStrip by remember { mutableStateOf(false) }
    var modelsMenu by remember { mutableStateOf(false) }
    var lightsMenu by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        ed.loadSceneCamera()
        if (ed.sceneCamera == null && !ed.sceneCameraRemoved) ed.addSceneCamera()
        ed.loadLights()
    }

    val tools = listOf(
        Tool("scene_models", "3D Models", Glyphs.Object3D, active = modelsMenu) { modelsMenu = true; lightsMenu = false },
        Tool("scene_lights", "Lights", LucideIcons.Sun.glyph(), active = lightsMenu) { lightsMenu = true; modelsMenu = false },
        Tool("scene_camera_toggle", "Camera", cameraGlyph(), active = cameraStrip) { cameraStrip = !cameraStrip; modelsMenu = false; lightsMenu = false },
        Tool("scene_show_video", if (ed.sceneShowVideo) "Hide video" else "Show video", LucideIcons.Film.glyph(), active = ed.sceneShowVideo) { ed.toggleSceneShowVideo() },
        Tool("scene_reset_view", "Reset view", LucideIcons.RotateCcw.glyph()) { ed.setSceneOrbit(SceneOrbit.DEFAULT) },
    )

    Column(Modifier.fillMaxWidth()) {
        HorizontalDivider(color = Fc.Divider, thickness = 1.dp)
        Box(Modifier.fillMaxWidth()) {
            ToolRow(tools, back = { ed.panel = null }, deep = false, modifier = Modifier.fillMaxWidth())
            DropdownMenu(expanded = modelsMenu, onDismissRequest = { modelsMenu = false }) {
                Object3DMesh.entries.forEach { mesh ->
                    val label = mesh.name.lowercase().replaceFirstChar { it.uppercase() }
                    DropdownMenuItem(
                        text = { Text(if (mesh == Object3DMesh.MODEL) "Import 3D model…" else label) },
                        onClick = {
                            modelsMenu = false
                            if (mesh == Object3DMesh.MODEL) ed.addModel() else ed.addObjectInScene(mesh)
                        },
                        modifier = Modifier.testTag("scene_add_${mesh.name.lowercase()}"),
                    )
                }
            }
            DropdownMenu(expanded = lightsMenu, onDismissRequest = { lightsMenu = false }) {
                listOf("directional", "point", "spot").forEach { kind ->
                    DropdownMenuItem(
                        text = { Text("Add ${kind.replaceFirstChar { it.uppercase() }} light") },
                        onClick = { lightsMenu = false; ed.addLight(kind) { ed.selectedLightId = it } },
                        modifier = Modifier.testTag("scene_bar_add_$kind"),
                    )
                }
                DropdownMenuItem(
                    text = { Text("Light settings…") },
                    onClick = { lightsMenu = false; ed.panel = Panel.Lights },
                    modifier = Modifier.testTag("scene_open_lights"),
                )
            }
        }

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
                    if (cam != null) SelectChip("Remove camera", false, { ed.clearSceneCamera() }, tag = "scene_remove_camera")
                    else SelectChip("Add camera", false, { ed.addSceneCamera() }, tag = "scene_add_camera")
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

private fun cameraGlyph() = Glyph.Drawn { c ->
    val u = size.minDimension / 24f
    drawRoundRect(c, Offset(3f*u, 7f*u), Size(18f*u, 13f*u), CornerRadius(2f*u), style = Stroke(1.8f*u))
    drawRoundRect(c, Offset(7f*u, 5f*u), Size(5f*u, 2.5f*u), CornerRadius(1f*u))
    drawCircle(c, 4f*u, Offset(12f*u, 13.5f*u), style = Stroke(1.8f*u))
    drawCircle(c, 1f*u, Offset(12f*u, 13.5f*u))
}
