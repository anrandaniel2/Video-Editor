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
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
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
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import io.github.artcraftandroid.filmcraft.core.MatchMoveSettings
import io.github.artcraftandroid.filmcraft.ui.*
import java.util.Locale
import kotlin.math.roundToInt

@Composable
internal fun SceneBar(ed: EditorController) {
    var cameraStrip by remember { mutableStateOf(false) }
    var matchMoveStrip by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        ed.loadSceneCamera()
        ed.loadMatchMove()
        if (ed.sceneCamera == null && !ed.sceneCameraRemoved && !ed.matchmoveActive) ed.addSceneCamera()
        ed.loadLights()
    }

    val tools = listOf(
        Tool("scene_models", "3D Models", Glyphs.Object3D, active = ed.panel == Panel.Object3D) { ed.panel = Panel.Object3D },
        Tool("scene_lights", "Lights", LucideIcons.Sun.glyph(), active = ed.panel == Panel.Lights) { ed.panel = Panel.Lights },
        Tool("scene_camera_toggle", "Camera", cameraGlyph(), active = cameraStrip) {
            cameraStrip = !cameraStrip
            if (cameraStrip) matchMoveStrip = false
        },
        Tool("scene_matchmove_toggle", "Match-Move", Glyphs.Camera3D, active = matchMoveStrip || ed.matchmoveActive) {
            matchMoveStrip = !matchMoveStrip
            if (matchMoveStrip) cameraStrip = false
        },
        Tool("scene_show_video", if (ed.sceneShowVideo) "Hide video" else "Show video", LucideIcons.Film.glyph(), active = ed.sceneShowVideo) { ed.toggleSceneShowVideo() },
        Tool("scene_reset_view", "Reset view", LucideIcons.RotateCcw.glyph()) { ed.setSceneOrbit(SceneOrbit.DEFAULT) },
    )

    Column(Modifier.fillMaxWidth()) {
        HorizontalDivider(color = Fc.Divider, thickness = 1.dp)
        ToolRow(tools, back = { ed.panel = null }, deep = false, modifier = Modifier.fillMaxWidth())

        if (cameraStrip) {
            val cam = ed.sceneCamera
            Column(
                Modifier.fillMaxWidth().heightIn(max = 264.dp).background(Fc.Panel)
                    .verticalScroll(rememberScrollState()),
            ) {
                if (ed.matchmoveActive) {
                    Row(
                        Modifier.fillMaxWidth().padding(horizontal = FcSize.PanelPadding, vertical = 10.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        Text(
                            "Match-Move is driving the render camera.",
                            style = FcType.Caption, color = Fc.Secondary, modifier = Modifier.weight(1f),
                        )
                        SelectChip("Align", false, { cameraStrip = false; matchMoveStrip = true }, tag = "scene_open_matchmove")
                        SelectChip("Manual", false, {
                            ed.matchmoveState?.let { ed.setMatchMove(it.copy(enabled = false)) }
                        }, tag = "scene_disable_matchmove")
                    }
                } else {
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
                }
                Spacer(Modifier.height(8.dp))
            }
        }

        if (matchMoveStrip) {
            SceneMatchMoveStrip(ed)
        }
    }
}

@Composable
private fun SceneMatchMoveStrip(ed: EditorController) {
    val running = ed.cameraSolveProgress
    val solves = ed.cameraSolves
    val mm = ed.matchmoveState
    val targetClip = ed.matchmoveTargetClip
    Column(
        Modifier.fillMaxWidth().heightIn(max = 280.dp).background(Fc.Panel)
            .verticalScroll(rememberScrollState())
            .testTag("scene_matchmove_strip"),
    ) {
        if (running != null) {
            Column(Modifier.fillMaxWidth().padding(horizontal = FcSize.PanelPadding, vertical = 10.dp)) {
                Text(running.stage, style = FcType.Label, color = Fc.Body)
                Spacer(Modifier.height(8.dp))
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Box(Modifier.weight(1f).height(6.dp).clip(CircleShape).background(Fc.SliderOff)) {
                        Box(Modifier.fillMaxWidth(running.progress.toFloat().coerceIn(0f, 1f)).height(6.dp).clip(CircleShape).background(Fc.Accent))
                    }
                    Spacer(Modifier.width(12.dp))
                    Text("${(running.progress * 100).roundToInt()}%", style = FcType.Value, color = Fc.Secondary)
                    Spacer(Modifier.width(12.dp))
                    SelectChip("Cancel", false, { ed.cancelCameraSolve() }, tag = "scene_matchmove_cancel")
                }
            }
        } else {
            Row(
                Modifier.fillMaxWidth().padding(horizontal = FcSize.PanelPadding, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Text("3D Match-Move", style = FcType.Label, color = Fc.Title, modifier = Modifier.weight(1f))
                if (targetClip != null) {
                    SelectChip("Solve clip", false, { ed.solveCamera(targetClip.id) }, tag = "scene_matchmove_solve")
                }
                SelectChip("Options…", false, { ed.panel = Panel.MatchMove }, tag = "scene_matchmove_panel")
            }
            if (solves.isEmpty()) {
                Text(
                    if (targetClip != null) "Solve '${targetClip.name}' to reconstruct its 3D camera path and point cloud."
                    else "Add a video clip on the timeline to solve 3D camera motion.",
                    style = FcType.Caption, color = Fc.Detail,
                    modifier = Modifier.padding(horizontal = FcSize.PanelPadding, vertical = 4.dp),
                )
            } else {
                Row(
                    Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = FcSize.PanelPadding, vertical = 4.dp),
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    solves.forEach { s ->
                        val label = String.format(Locale.US, "%s · %df · %.2fpx", s.name, s.frames.toInt(), s.medianRmsPx)
                        SelectChip(label, mm?.solve == s.id, {
                            val base = mm ?: MatchMoveSettings(s.id, true, 700.0, 0.0, 0.0, 0.0)
                            ed.setMatchMove(base.copy(solve = s.id, enabled = true))
                        }, tag = "scene_matchmove_solve_${s.id}")
                    }
                }
                val active = solves.firstOrNull { it.id == mm?.solve } ?: solves.last()
                val cur = mm?.takeIf { it.solve == active.id } ?: MatchMoveSettings(active.id, false, 700.0, 0.0, 0.0, 0.0)
                Row(
                    Modifier.fillMaxWidth().padding(horizontal = FcSize.PanelPadding, vertical = 2.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text("Drive 3D camera", style = FcType.Label, color = Fc.Secondary, modifier = Modifier.weight(1f))
                    Switch(
                        checked = cur.enabled,
                        onCheckedChange = { ed.setMatchMove(cur.copy(enabled = it)) },
                        colors = SwitchDefaults.colors(checkedTrackColor = Fc.Accent, uncheckedTrackColor = Fc.SliderOff),
                        modifier = Modifier.testTag("scene_matchmove_enabled"),
                    )
                }
                fun alignBase() = ed.matchmoveState?.takeIf { it.solve == active.id }
                    ?: cur.copy(solve = active.id, enabled = true)
                AdvancedSlider("World scale", cur.scalePx, 50.0, 3000.0, 5.0, "scene_matchmove_scale") { v, k ->
                    ed.setMatchMove(alignBase().copy(scalePx = v), k)
                }
                AdvancedSlider("Yaw trim", cur.yawTrim, -180.0, 180.0, 0.5, "scene_matchmove_yaw") { v, k ->
                    ed.setMatchMove(alignBase().copy(yawTrim = v), k)
                }
                AdvancedSlider("Pitch trim", cur.pitchTrim, -180.0, 180.0, 0.5, "scene_matchmove_pitch") { v, k ->
                    ed.setMatchMove(alignBase().copy(pitchTrim = v), k)
                }
                AdvancedSlider("Roll trim", cur.rollTrim, -180.0, 180.0, 0.5, "scene_matchmove_roll") { v, k ->
                    ed.setMatchMove(alignBase().copy(rollTrim = v), k)
                }
                Row(
                    Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = FcSize.PanelPadding, vertical = 4.dp),
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    if (!active.rotationOnly) {
                        SelectChip("Auto-level ground", false, { ed.levelMatchMoveGround() }, tag = "scene_matchmove_level")
                    }
                    if (cur.enabled) {
                        SelectChip("Bake frame", false, { ed.bakeMatchMoveCamera() }, tag = "scene_matchmove_bake")
                    }
                    SelectChip("Reset trims", false, {
                        ed.setMatchMove(cur.copy(scalePx = 700.0, yawTrim = 0.0, pitchTrim = 0.0, rollTrim = 0.0))
                    }, tag = "scene_matchmove_reset")
                }
            }
        }
        Spacer(Modifier.height(8.dp))
    }
}

private fun cameraGlyph() = Glyph.Drawn { c ->
    val u = size.minDimension / 24f
    drawRoundRect(c, Offset(3f*u, 7f*u), Size(18f*u, 13f*u), CornerRadius(2f*u), style = Stroke(1.8f*u))
    drawRoundRect(c, Offset(7f*u, 5f*u), Size(5f*u, 2.5f*u), CornerRadius(1f*u))
    drawCircle(c, 4f*u, Offset(12f*u, 13.5f*u), style = Stroke(1.8f*u))
    drawCircle(c, 1f*u, Offset(12f*u, 13.5f*u))
}
