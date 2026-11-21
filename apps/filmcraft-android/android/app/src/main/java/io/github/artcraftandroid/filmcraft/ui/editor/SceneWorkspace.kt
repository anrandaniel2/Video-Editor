package io.github.artcraftandroid.filmcraft.ui.editor

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.ui.platform.LocalDensity
import kotlin.math.roundToInt
import kotlin.math.min
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.layout.ContentScale
import androidx.compose.foundation.Image
import io.github.artcraftandroid.filmcraft.core.Object3DMesh
import io.github.artcraftandroid.filmcraft.ui.*

/**
 * The 3D scene workspace: the sequence's world, editor-only, on a black surround.
 *
 * The viewport renders the scene through the render camera's editor counterpart — the
 * orbit view — with the grid and the camera frustum drawn over it. The video underneath
 * is not rendered unless the Video chip is on. Objects and lights are added from the
 * bottom bar and positioned with the fused gizmos; the camera exists by default and is
 * drawn as a frustum you can grab and drag to re-aim the render.
 */
@Composable
internal fun SceneWorkspace(ed: EditorController, close: () -> Unit) {
    val t = ed.timeline
    var cameraStrip by remember { mutableStateOf(false) }
    val sel = ed.selectedClip
    val object3d = sel?.has3d == true
    var placement by remember { mutableStateOf<io.github.artcraftandroid.filmcraft.core.Placement?>(null) }
    LaunchedEffect(sel?.id, t?.revision, ed.playhead) {
        placement = if (sel != null) ed.placementOf(sel.id) else null
    }
    LaunchedEffect(sel?.id, t?.revision, ed.playhead, ed.panel) {
        if (object3d && sel != null) ed.loadObject3D(sel.id)
    }
    LaunchedEffect(Unit) {
        ed.loadSceneCamera()
        if (ed.sceneCamera == null && !ed.sceneCameraRemoved) ed.addSceneCamera() // the render camera is there by default, unless the user took it away
        ed.loadLights()
        ed.applyScenePreviewCamera()
    }
    Box(
        Modifier.fillMaxSize().background(Fc.Black)
            .windowInsetsPadding(WindowInsets.displayCutout.union(WindowInsets.navigationBars).only(WindowInsetsSides.Horizontal))
            .statusBarsPadding().navigationBarsPadding()
            .testTag("scene_workspace"),
    ) {
        Column(Modifier.fillMaxSize()) {
            // ---- top bar
            Row(
                Modifier.fillMaxWidth().height(56.dp).padding(horizontal = 6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Box(
                    Modifier.size(44.dp).clip(RoundedCornerShape(12.dp)).clickable(role = Role.Button, onClick = close)
                        .semantics { contentDescription = "Close the 3D scene" },
                    contentAlignment = Alignment.Center,
                ) { GlyphIcon(LucideIcons.X.glyph(), Fc.Icon, size = 24.dp, description = "Close the 3D scene") }
                Text("3D Scene", style = FcType.GalleryTitle, color = Fc.Title, modifier = Modifier.padding(start = 4.dp))
                Spacer(Modifier.weight(1f))
                PanelAction(LucideIcons.Undo2.glyph(), "Undo", { ed.undo() })
                PanelAction(LucideIcons.Redo2.glyph(), "Redo", { ed.redo() })
                PanelAction(LucideIcons.RotateCcw.glyph(), "Reset view", { ed.setSceneOrbit(SceneOrbit.DEFAULT) }, tag = "scene_reset_view")
                Spacer(Modifier.width(6.dp))
                SelectChip("Video", ed.sceneShowVideo, { ed.toggleSceneShowVideo() }, tag = "scene_show_video")
            }

            // ---- viewport: the frame, fitted, on a black surround
            BoxWithConstraints(Modifier.weight(1f).fillMaxWidth().background(Color.Black).testTag("scene_viewport"), contentAlignment = Alignment.Center) {
                val aspect = if (t != null && t.height.toInt() > 0) t.width.toFloat() / t.height.toFloat() else 9f / 16f
                val boxW = min(maxWidth.value, maxHeight.value * aspect).dp
                val boxH = boxW / aspect
                val px = with(LocalDensity.current) { maxOf(boxW.toPx(), boxH.toPx()) }
                LaunchedEffect(px) { ed.previewSide = px.roundToInt() }
                Box(Modifier.size(boxW, boxH)) {
                    val pic = ed.picture
                    if (pic != null) {
                        Image(pic, contentDescription = null, modifier = Modifier.fillMaxSize(), contentScale = ContentScale.Fit)
                    } else {
                        Text(
                            "Nothing in the scene yet\nadd an object or a light below",
                            style = FcType.Body, color = Fc.Detail, textAlign = TextAlign.Center,
                            modifier = Modifier.align(Alignment.Center),
                        )
                    }
                    if (t != null) {
                        SceneGridOverlay(ed, t.width.toFloat(), t.height.toFloat(), Modifier.fillMaxSize())
                        if (object3d) ObjectGizmoOverlay(ed, t.width.toFloat(), t.height.toFloat(), placement, Modifier.fillMaxSize())
                        LightGizmoOverlay(ed, t.width.toFloat(), t.height.toFloat())
                    }
                }
            }

            // ---- camera strip (toggled from the bar)
            val cam = ed.sceneCamera
            if (cameraStrip && cam != null) {
                Column(Modifier.fillMaxWidth().heightIn(max = 264.dp).background(Fc.Panel).verticalScroll(rememberScrollState())) {
                    Row(
                        Modifier.fillMaxWidth().padding(horizontal = FcSize.PanelPadding, vertical = 8.dp),
                        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.SpaceBetween,
                    ) {
                        Text("Render camera", style = FcType.Label, color = Fc.Title)
                        SelectChip("Remove camera", false, { ed.clearSceneCamera() }, tag = "scene_remove_camera")
                    }
                    fun change(next: SceneCamera, k: String?) = ed.setSceneCamera(next, k)
                    AdvancedSlider("Camera X", cam.position[0], -2000.0, 2000.0, 1.0, "scene_cam_x") { v, k -> change(cam.copy(position = listOf(v, cam.position[1], cam.position[2])), k) }
                    AdvancedSlider("Camera Y", cam.position[1], -2000.0, 2000.0, 1.0, "scene_cam_y") { v, k -> change(cam.copy(position = listOf(cam.position[0], v, cam.position[2])), k) }
                    AdvancedSlider("Camera Z", cam.position[2], 0.0, 6000.0, 5.0, "scene_cam_z") { v, k -> change(cam.copy(position = listOf(cam.position[0], cam.position[1], v)), k) }
                    AdvancedSlider("Yaw", cam.yaw, -180.0, 180.0, 1.0, "scene_cam_yaw") { v, k -> change(cam.copy(yaw = v), k) }
                    AdvancedSlider("Pitch", cam.pitch, -180.0, 180.0, 1.0, "scene_cam_pitch") { v, k -> change(cam.copy(pitch = v), k) }
                    AdvancedSlider("Field of view", cam.fov, 10.0, 120.0, 1.0, "scene_cam_fov") { v, k -> change(cam.copy(fov = v), k) }
                    Spacer(Modifier.height(8.dp))
                }
            }

            // ---- bottom bar
            Row(
                Modifier.fillMaxWidth().height(FcSize.ToolTile + 14.dp)
                    .background(Fc.Panel)
                    .horizontalScroll(rememberScrollState())
                    .padding(horizontal = 10.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Object3DMesh.entries.filter { it != Object3DMesh.MODEL }.forEach { mesh ->
                    val label = mesh.name.lowercase().replaceFirstChar { it.uppercase() }
                    SelectChip("+ $label", false, { ed.addObjectInScene(mesh) }, tag = "scene_add_${mesh.name.lowercase()}")
                }
                Box(Modifier.width(1.dp).height(28.dp).background(Fc.Divider))
                SelectChip("+ Directional", false, { ed.addLight("directional") }, tag = "scene_add_directional")
                SelectChip("+ Point", false, { ed.addLight("point") }, tag = "scene_add_point")
                SelectChip("+ Spot", false, { ed.addLight("spot") }, tag = "scene_add_spot")
                SelectChip("Lights", false, { ed.panel = Panel.Lights }, tag = "scene_open_lights")
                Box(Modifier.width(1.dp).height(28.dp).background(Fc.Divider))
                SelectChip("Camera", cameraStrip, { cameraStrip = !cameraStrip }, tag = "scene_camera_toggle")
            }
        }
    }
}
