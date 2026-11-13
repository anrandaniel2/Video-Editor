package io.github.artcraftandroid.filmcraft.ui.editor

import io.github.artcraftandroid.filmcraft.ui.editorGlass
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.produceState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableDoubleStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.artcraftandroid.filmcraft.core.BackgroundSettings
import io.github.artcraftandroid.filmcraft.core.FontFamily
import io.github.artcraftandroid.filmcraft.core.ModelInfo
import io.github.artcraftandroid.filmcraft.core.Object3DMesh
import io.github.artcraftandroid.filmcraft.core.Object3DSettings
import io.github.artcraftandroid.filmcraft.core.StereoLayout
import io.github.artcraftandroid.filmcraft.ui.Fc
import io.github.artcraftandroid.filmcraft.ui.FcSlider
import io.github.artcraftandroid.filmcraft.ui.FcSize
import io.github.artcraftandroid.filmcraft.ui.FcType
import io.github.artcraftandroid.filmcraft.ui.GlyphIcon
import io.github.artcraftandroid.filmcraft.ui.LucideIcons
import io.github.artcraftandroid.filmcraft.ui.PanelAction
import io.github.artcraftandroid.filmcraft.ui.SelectChip
import io.github.artcraftandroid.filmcraft.ui.SliderRow
import io.github.artcraftandroid.filmcraft.ui.ToolPanel
import io.github.artcraftandroid.filmcraft.ui.glyph
import kotlin.math.round

/** Keyframable 3D primitives and extruded text. */
@Composable
fun Object3DPanel(ed: EditorController, close: () -> Unit, modifier: Modifier = Modifier) {
    val clip = ed.selectedClip
    val state = ed.object3DState?.takeIf { it.clip == clip?.id }
    val settings = state?.settings
    val hasObject = state?.hasObject == true
    LaunchedEffect(clip?.id, ed.playhead) {
        if (clip != null) ed.loadObject3D(clip.id)
        ed.loadTracks()
    }
    ToolPanel(
        "3D Object", close, modifier, tag = "object3d_panel",
        actions = {
            if (state?.hasObject == true) {
                PanelAction(LucideIcons.ChartSpline.glyph(), "Keyframes", { ed.panel = Panel.Keyframes }, tag = "object3d_keyframe")
            }
        },
    ) {
        Text(
            "Add a 3D layer and rotate it with the sliders. Open Keyframes to animate individual properties and choose curves.",
            style = FcType.Caption, color = Fc.Detail,
            modifier = Modifier.fillMaxWidth().padding(horizontal = FcSize.PanelPadding),
        )
        Spacer(Modifier.height(10.dp))
        Row(
            Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = FcSize.PanelPadding),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Object3DMesh.entries.forEach { mesh ->
                val label = mesh.name.lowercase().replaceFirstChar { it.uppercase() }
                SelectChip(label, settings?.mesh == mesh, {
                    if (mesh == Object3DMesh.MODEL) ed.addModel()
                    else if (hasObject && settings != null) ed.setObject(settings.copy(mesh = mesh))
                    else ed.addObject(mesh)
                }, tag = "object3d_mesh_${mesh.name.lowercase()}")
            }
        }
        if (hasObject && settings != null) {
            ObjectControls(ed, settings, state?.animated == true)
        } else {
            Text(
                "Choose a shape to add a 3D overlay. Select a text clip and use Extrude to turn its style into 3D text.",
                style = FcType.Body, color = Fc.Secondary,
                modifier = Modifier.padding(horizontal = FcSize.PanelPadding, vertical = 16.dp),
            )
        }
        Spacer(Modifier.height(12.dp))
    }
}

@Composable
private fun ObjectControls(ed: EditorController, st: Object3DSettings, animated: Boolean) {
    fun change(next: Object3DSettings, merge: String? = null) = ed.setObject(next, merge)
    Spacer(Modifier.height(8.dp))
    Text(
        if (animated) "Keyframed · edits at playhead" else "Static · enable keyframes to animate",
        style = FcType.Caption, color = Fc.Detail,
        modifier = Modifier.padding(horizontal = FcSize.PanelPadding),
    )
    if (st.mesh == Object3DMesh.MODEL) {
        val info by produceState<ModelInfo?>(null, st.modelPath) {
            value = if (st.modelPath.isBlank()) null else runCatching { ed.engine.command { modelInfo(st.modelPath) } }.getOrNull()
        }
        Row(
            Modifier.fillMaxWidth().padding(horizontal = FcSize.PanelPadding, vertical = 4.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                st.modelPath.substringAfterLast('/').ifBlank { "No model imported" },
                style = FcType.Label, color = Fc.Secondary, maxLines = 1, overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            SelectChip("Import GLB", false, { ed.addModel() }, tag = "object3d_import_model")
        }
        info?.let { m ->
            Text(
                "${m.triangles} triangles · ${m.textures} textures · ${m.skins} rigs · ${m.morphTargets} morph targets",
                style = FcType.Caption, color = Fc.Detail,
                modifier = Modifier.padding(horizontal = FcSize.PanelPadding, vertical = 2.dp),
            )
            val labels = listOf("Rest pose") + m.animations.map { it.name }
            val current = if (st.modelAnim < 0) "Rest pose" else m.animations.getOrNull(st.modelAnim)?.name ?: "Animation ${st.modelAnim + 1}"
            PickerRow("Animation", current, labels, "object3d_model_anim") { picked ->
                val idx = labels.indexOf(picked) - 1
                change((ed.object3DState?.settings ?: st).copy(modelAnim = idx, modelTime = 0.0))
            }
            val duration = m.animations.getOrNull(st.modelAnim)?.duration ?: 0.0
            AdvancedSlider("Animation time", st.modelTime, 0.0, duration.coerceAtLeast(0.01), 0.01, "object3d_model_time") { v, k ->
                change((ed.object3DState?.settings ?: st).copy(modelTime = v), k)
            }
            Row(Modifier.fillMaxWidth().padding(horizontal = FcSize.PanelPadding), verticalAlignment = Alignment.CenterVertically) {
                Text("Play with clip", style = FcType.Label, color = Fc.Secondary, modifier = Modifier.weight(1f))
                Switch(
                    checked = st.modelPlay,
                    onCheckedChange = { change((ed.object3DState?.settings ?: st).copy(modelPlay = it)) },
                    colors = SwitchDefaults.colors(checkedTrackColor = Fc.Accent, uncheckedTrackColor = Fc.SliderOff),
                    modifier = Modifier.testTag("object3d_model_play"),
                )
            }
            Row(Modifier.fillMaxWidth().padding(horizontal = FcSize.PanelPadding), verticalAlignment = Alignment.CenterVertically) {
                Text("Loop", style = FcType.Label, color = Fc.Secondary, modifier = Modifier.weight(1f))
                Switch(
                    checked = st.modelLoop,
                    onCheckedChange = { change((ed.object3DState?.settings ?: st).copy(modelLoop = it)) },
                    colors = SwitchDefaults.colors(checkedTrackColor = Fc.Accent, uncheckedTrackColor = Fc.SliderOff),
                    modifier = Modifier.testTag("object3d_model_loop"),
                )
            }
            AdvancedSlider("Animation speed", st.modelSpeed, 0.0, 400.0, 1.0, "object3d_model_speed") { v, k ->
                change((ed.object3DState?.settings ?: st).copy(modelSpeed = v), k)
            }
        }
    } else if (st.mesh == Object3DMesh.TEXT) {
        OutlinedTextField(
            value = st.text,
            onValueChange = { change((ed.object3DState?.settings ?: st).copy(text = it)) },
            label = { Text("Text") },
            modifier = Modifier.fillMaxWidth().padding(horizontal = FcSize.PanelPadding, vertical = 8.dp).testTag("object3d_text"),
        )
        val families by produceState<List<FontFamily>>(emptyList()) {
            value = runCatching { ed.engine.command { fontFamilies() } }.getOrDefault(emptyList())
                .ifEmpty { listOf(FontFamily("Inter", listOf("Regular", "Bold"))) }
        }
        val styles = families.firstOrNull { it.name == st.font }?.styles ?: listOf(st.fontStyle.ifBlank { "Regular" })
        PickerRow("Font", st.font, families.map { it.name }.ifEmpty { listOf(st.font) }, "object3d_font") { picked ->
            val next = families.firstOrNull { it.name == picked }?.styles ?: emptyList()
            // Keep the style when the new family has it, else fall back to Regular/its first.
            val style = when {
                next.contains(st.fontStyle) -> st.fontStyle
                next.contains("Regular") -> "Regular"
                else -> next.firstOrNull() ?: "Regular"
            }
            change((ed.object3DState?.settings ?: st).copy(font = picked, fontStyle = style))
        }
        PickerRow("Style", st.fontStyle, styles, "object3d_font_style") { picked ->
            change((ed.object3DState?.settings ?: st).copy(fontStyle = picked))
        }
        Row(
            Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = FcSize.PanelPadding, vertical = 6.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            SelectChip("Bold", st.fauxBold, { change((ed.object3DState?.settings ?: st).copy(fauxBold = !st.fauxBold)) }, tag = "object3d_bold")
            SelectChip("Italic", st.fauxItalic, { change((ed.object3DState?.settings ?: st).copy(fauxItalic = !st.fauxItalic)) }, tag = "object3d_italic")
            SelectChip("Left", st.align == 0u, { change((ed.object3DState?.settings ?: st).copy(align = 0u)) }, tag = "object3d_align_left")
            SelectChip("Center", st.align == 1u, { change((ed.object3DState?.settings ?: st).copy(align = 1u)) }, tag = "object3d_align_center")
            SelectChip("Right", st.align == 2u, { change((ed.object3DState?.settings ?: st).copy(align = 2u)) }, tag = "object3d_align_right")
        }
        AdvancedSlider("Font size", st.size, 4.0, 600.0, 1.0, "object3d_size") { v, k -> change((ed.object3DState?.settings ?: st).copy(size = v), k) }
        AdvancedSlider("Extrusion", st.depth, 0.0, 500.0, 1.0, "object3d_depth") { v, k -> change((ed.object3DState?.settings ?: st).copy(depth = v), k) }
        AdvancedSlider("Bevel", st.bevel, 0.0, 100.0, 0.5, "object3d_bevel") { v, k -> change((ed.object3DState?.settings ?: st).copy(bevel = v), k) }
    }
    HorizontalDivider(color = Fc.Divider, thickness = 1.dp, modifier = Modifier.padding(vertical = 6.dp))
    AdvancedSlider("Rotate X", st.rotationX, -180.0, 180.0, 1.0, "object3d_rx") { v, k -> change((ed.object3DState?.settings ?: st).copy(rotationX = v), k) }
    AdvancedSlider("Rotate Y", st.rotationY, -180.0, 180.0, 1.0, "object3d_ry") { v, k -> change((ed.object3DState?.settings ?: st).copy(rotationY = v), k) }
    AdvancedSlider("Rotate Z", st.rotationZ, -180.0, 180.0, 1.0, "object3d_rz") { v, k -> change((ed.object3DState?.settings ?: st).copy(rotationZ = v), k) }
    AdvancedSlider("Scale", st.scale, 0.0, 400.0, 1.0, "object3d_scale") { v, k -> change((ed.object3DState?.settings ?: st).copy(scale = v), k) }
    AdvancedSlider("Camera distance", st.z, 50.0, 3000.0, 5.0, "object3d_z") { v, k -> change((ed.object3DState?.settings ?: st).copy(z = v), k) }
    AdvancedSlider("X", st.x, -2000.0, 4000.0, 1.0, "object3d_x") { v, k -> change((ed.object3DState?.settings ?: st).copy(x = v), k) }
    AdvancedSlider("Y", st.y, -2000.0, 4000.0, 1.0, "object3d_y") { v, k -> change((ed.object3DState?.settings ?: st).copy(y = v), k) }
    HorizontalDivider(color = Fc.Divider, thickness = 1.dp, modifier = Modifier.padding(vertical = 6.dp))
    ColorPicker("Base color", st.color.toInt()) { c -> change((ed.object3DState?.settings ?: st).copy(color = c.toUInt(), sideColor = c.toUInt())) }
    if (st.mesh != Object3DMesh.MODEL) {
        AdvancedSlider("Metalness", st.metalness, 0.0, 100.0, 1.0, "object3d_metal") { v, k -> change((ed.object3DState?.settings ?: st).copy(metalness = v), k) }
        AdvancedSlider("Roughness", st.roughness, 0.0, 100.0, 1.0, "object3d_rough") { v, k -> change((ed.object3DState?.settings ?: st).copy(roughness = v), k) }
    }
    AdvancedSlider("Opacity", st.opacity, 0.0, 100.0, 1.0, "object3d_opacity") { v, k -> change((ed.object3DState?.settings ?: st).copy(opacity = v), k) }
    // Lighting is the scene's light objects (coloured, multiple) — not a per-object angle.
    LaunchedEffect(Unit) { ed.loadLights() }
    Row(
        Modifier.fillMaxWidth().padding(horizontal = FcSize.PanelPadding),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        val n = ed.sceneLights.lights.size
        Text(
            if (n == 0) "Lighting: classic light" else if (n == 1) "Lighting: 1 light" else "Lighting: $n lights",
            style = FcType.Caption, color = Fc.Secondary,
        )
        SelectChip("Lights", ed.panel == Panel.Lights, { ed.panel = Panel.Lights }, tag = "object3d_lights")
    }
    HorizontalDivider(color = Fc.Divider, thickness = 1.dp, modifier = Modifier.padding(vertical = 6.dp))
    Row(Modifier.fillMaxWidth().padding(horizontal = FcSize.PanelPadding), verticalAlignment = Alignment.CenterVertically) {
        Text("Follow motion track", style = FcType.Label, color = Fc.Secondary, modifier = Modifier.weight(1f))
        Switch(
            checked = st.followTrack,
            onCheckedChange = { ed.followTrack(ed.selectedTrackId, it) },
            enabled = ed.motionTracks.isNotEmpty(),
            colors = SwitchDefaults.colors(checkedTrackColor = Fc.Accent, uncheckedTrackColor = Fc.SliderOff),
            modifier = Modifier.testTag("object3d_follow"),
        )
    }
    if (ed.motionTracks.isNotEmpty()) {
        Row(
            Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = FcSize.PanelPadding),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            ed.motionTracks.forEach { t ->
                SelectChip(t.name, ed.selectedTrackId == t.id, {
                    ed.selectedTrackId = t.id
                    if (st.followTrack) ed.followTrack(t.id, true)
                }, tag = "object3d_track_${t.id}")
            }
        }
        AdvancedSlider("Track depth", st.trackDepth, -1000.0, 1000.0, 5.0, "object3d_track_depth") { v, k ->
            change((ed.object3DState?.settings ?: st).copy(trackDepth = v), k)
        }
    }
    // Shape geometry sliders live at the bottom. A 3D model has no size/extrusion sliders —
    // its Scale in the transform group above is the only size control.
    if (st.mesh != Object3DMesh.MODEL && st.mesh != Object3DMesh.TEXT) {
        HorizontalDivider(color = Fc.Divider, thickness = 1.dp, modifier = Modifier.padding(vertical = 6.dp))
        AdvancedSlider("Size", st.size, 10.0, 1200.0, 1.0, "object3d_size") { v, k -> change((ed.object3DState?.settings ?: st).copy(size = v), k) }
        AdvancedSlider("Depth", st.depth, 0.0, 500.0, 1.0, "object3d_depth") { v, k -> change((ed.object3DState?.settings ?: st).copy(depth = v), k) }
        if (st.mesh != Object3DMesh.SPHERE) {
            AdvancedSlider("Edge bevel", st.bevel, 0.0, 100.0, 0.5, "object3d_bevel") { v, k -> change((ed.object3DState?.settings ?: st).copy(bevel = v), k) }
        }
    }
}

/**
 * A labelled dropdown. Long lists (the phone can have hundreds of fonts) get a search field and
 * scroll; the current value stays visible on the closed row. Shared by the 3D panel and the
 * text strip where text is added.
 */
@Composable
internal fun PickerRow(label: String, value: String, options: List<String>, tag: String, modifier: Modifier = Modifier, onPick: (String) -> Unit) {
    var showList by remember { mutableStateOf(false) }
    var query by remember { mutableStateOf("") }
    Column(modifier.fillMaxWidth().padding(horizontal = FcSize.PanelPadding, vertical = 4.dp)) {
        Text(label, style = FcType.Caption, color = Fc.Detail)
        Spacer(Modifier.height(4.dp))
        Box {
            Row(
                Modifier.fillMaxWidth().height(48.dp).clip(RoundedCornerShape(10.dp)).editorGlass(10.dp, layer = 3f, captureSurface = false)
                    .clickable(onClick = { showList = true })
                    .padding(horizontal = 12.dp)
                    .testTag(tag),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    value.ifBlank { "—" },
                    style = FcType.Body,
                    color = Fc.Title,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                GlyphIcon(LucideIcons.ChevronDown.glyph(), Fc.Detail, size = 18.dp)
            }
            DropdownMenu(
                expanded = showList,
                onDismissRequest = { showList = false; query = "" },
                modifier = Modifier.heightIn(max = 360.dp).editorGlass(12.dp, layer = 4f).testTag("${tag}_menu"),
                containerColor = Color.Transparent,
                tonalElevation = 0.dp,
                shadowElevation = 0.dp,
            ) {
                if (options.size > 10) {
                    OutlinedTextField(
                        value = query,
                        onValueChange = { query = it },
                        singleLine = true,
                        label = { Text("Search") },
                        modifier = Modifier.fillMaxWidth().padding(horizontal = 8.dp).testTag("${tag}_search"),
                    )
                }
                val shown = options.filter { query.isBlank() || it.contains(query.trim(), ignoreCase = true) }
                if (shown.isEmpty()) {
                    DropdownMenuItem(text = { Text("No match", color = Fc.Detail) }, onClick = { }, enabled = false)
                }
                shown.forEachIndexed { i, option ->
                    DropdownMenuItem(
                        text = { Text(option, color = if (option == value) Fc.Accent else Fc.Title, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                        onClick = { showList = false; query = ""; onPick(option) },
                        modifier = Modifier.testTag("${tag}_item_$i"),
                    )
                }
            }
        }
    }
}

private val COLORS = listOf(0xFFEB7333uL, 0xFFF2C14EuL, 0xFF4CC9F0uL, 0xFF80ED99uL, 0xFFF72585uL, 0xFFFFFFFFuL, 0xFF222222uL)

@Composable
private fun ColorRow(label: String, selected: ULong, onPick: (ULong) -> Unit) {
    Row(Modifier.fillMaxWidth().padding(horizontal = FcSize.PanelPadding, vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(label, style = FcType.Label, color = Fc.Secondary, modifier = Modifier.weight(1f))
        COLORS.forEach { c ->
            Box(
                Modifier.padding(start = 8.dp).size(24.dp).clip(CircleShape).background(Color(c.toInt()))
                    .clickable { onPick(c) }
                    .testTag("object3d_color_${c.toString(16)}"),
            )
        }
    }
}

/** Analyse footage and manage the tracks 3D objects can follow. */
@Composable
fun TrackingPanel(ed: EditorController, close: () -> Unit, modifier: Modifier = Modifier) {
    val clip = ed.selectedClip
    LaunchedEffect(clip?.id) { ed.loadTracks() }
    val stereo = ed.trackingLayout
    var baseline by remember { mutableDoubleStateOf(64.0) }
    var seconds by remember { mutableDoubleStateOf(10.0) }
    ToolPanel(
        "3D Motion Tracking", close, modifier, tag = "tracking_panel",
        actions = {
            if (ed.selectedTrackId != 0uL) {
                PanelAction(LucideIcons.Trash2.glyph(), "Delete track", { ed.deleteTrack(ed.selectedTrackId) }, tag = "tracking_delete")
            }
        },
    ) {
        Text(
            "Tap the preview to choose what to follow, then track from the playhead. Side-by-side or top/bottom footage also estimates depth.",
            style = FcType.Caption, color = Fc.Detail,
            modifier = Modifier.fillMaxWidth().padding(horizontal = FcSize.PanelPadding),
        )
        Spacer(Modifier.height(8.dp))
        Text(
            ed.trackingTarget?.let { "Target: ${round(it.first * 100)}%, ${round(it.second * 100)}%" } ?: "Target: centre (tap the preview to move it)",
            style = FcType.Body, color = Fc.Secondary,
            modifier = Modifier.padding(horizontal = FcSize.PanelPadding),
        )
        Spacer(Modifier.height(8.dp))
        Row(
            Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = FcSize.PanelPadding),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            SelectChip("Mono", stereo == StereoLayout.MONO, { ed.trackingLayout = StereoLayout.MONO }, tag = "tracking_mono")
            SelectChip("Side by side", stereo == StereoLayout.SIDE_BY_SIDE, { ed.trackingLayout = StereoLayout.SIDE_BY_SIDE }, tag = "tracking_sbs")
            SelectChip("Top / bottom", stereo == StereoLayout.TOP_BOTTOM, { ed.trackingLayout = StereoLayout.TOP_BOTTOM }, tag = "tracking_tb")
        }
        AdvancedSlider("Depth sensitivity", baseline, 4.0, 256.0, 1.0, "tracking_baseline") { v, _ -> baseline = v }
        AdvancedSlider("Seconds to track", seconds, 1.0, 30.0, 1.0, "tracking_seconds") { v, _ -> seconds = v }
        Row(Modifier.fillMaxWidth().padding(horizontal = FcSize.PanelPadding, vertical = 8.dp)) {
            SelectChip(if (ed.trackingBusy) "Tracking…" else "Track from playhead", false, {
                if (!ed.trackingBusy && clip != null) ed.trackMotion(stereo, baseline, seconds)
            }, tag = "tracking_run")
        }
        if (ed.motionTracks.isNotEmpty()) {
            HorizontalDivider(color = Fc.Divider, thickness = 1.dp, modifier = Modifier.padding(vertical = 6.dp))
            Row(
                Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = FcSize.PanelPadding),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                ed.motionTracks.forEach { t ->
                    SelectChip("${t.name} · ${t.samples}f · ${t.quality.toInt()}%", ed.selectedTrackId == t.id, { ed.selectedTrackId = t.id }, tag = "tracking_item_${t.id}")
                }
            }
            Text(
                "Select a 3D object and turn on Follow motion track in the 3D panel to attach it.",
                style = FcType.Caption, color = Fc.Detail,
                modifier = Modifier.padding(horizontal = FcSize.PanelPadding, vertical = 8.dp),
            )
        }
        Spacer(Modifier.height(12.dp))
    }
}

/** One-click background removal refined with keep/remove points on the preview. */
@Composable
fun BackgroundPanel(ed: EditorController, close: () -> Unit, modifier: Modifier = Modifier) {
    val clip = ed.selectedClip
    val state = ed.backgroundState
    LaunchedEffect(clip?.id, ed.playhead) { if (clip != null) ed.loadBackground(clip.id) }
    ToolPanel(
        "Background Removal", close, modifier, tag = "background_panel",
        actions = {
            val active = state?.takeIf { it.enabled }
            if (active != null) {
                PanelAction(LucideIcons.CircleX.glyph(), "Clear points", {
                    ed.setBackground(active.settings.copy(keepPoints = emptyList(), removePoints = emptyList()))
                }, tag = "background_clear_points")
            }
        },
    ) {
        val st = state?.settings
        if (state == null || !state.enabled || st == null) {
            Text(
                "Remove the background in one click, then tap the preview with Keep or Remove to refine what stays.",
                style = FcType.Caption, color = Fc.Detail,
                modifier = Modifier.fillMaxWidth().padding(horizontal = FcSize.PanelPadding),
            )
            Row(Modifier.fillMaxWidth().padding(horizontal = FcSize.PanelPadding, vertical = 12.dp)) {
                SelectChip("Remove background", false, { ed.removeBackground() }, tag = "background_run")
            }
        } else {
            fun change(next: BackgroundSettings, merge: String? = null) = ed.setBackground(next, merge)
            Row(
                Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = FcSize.PanelPadding),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                SelectChip("Keep points", ed.backgroundPointMode == EditorController.BackgroundPointMode.Keep, {
                    ed.backgroundPointMode = EditorController.BackgroundPointMode.Keep
                }, tag = "background_keep_mode")
                SelectChip("Remove points", ed.backgroundPointMode == EditorController.BackgroundPointMode.Remove, {
                    ed.backgroundPointMode = EditorController.BackgroundPointMode.Remove
                }, tag = "background_remove_mode")
                SelectChip("Done", ed.backgroundPointMode == EditorController.BackgroundPointMode.None, {
                    ed.backgroundPointMode = EditorController.BackgroundPointMode.None
                }, tag = "background_done_mode")
            }
            Text(
                "Keep: ${st.keepPoints.size} · Remove: ${st.removePoints.size}. Tap the preview to add points.",
                style = FcType.Caption, color = Fc.Detail,
                modifier = Modifier.padding(horizontal = FcSize.PanelPadding, vertical = 6.dp),
            )
            AdvancedSlider("Threshold", st.threshold, 1.0, 100.0, 1.0, "background_threshold") { v, k ->
                change((ed.backgroundState?.settings ?: st).copy(threshold = v), k)
            }
            AdvancedSlider("Feather", st.feather, 0.0, 100.0, 1.0, "background_feather") { v, k ->
                change((ed.backgroundState?.settings ?: st).copy(feather = v), k)
            }
            AdvancedSlider("Edge smooth", st.smooth, 0.0, 20.0, 0.5, "background_smooth") { v, k ->
                change((ed.backgroundState?.settings ?: st).copy(smooth = v), k)
            }
            AdvancedSlider("Point radius", st.pointRadius, 0.0, 50.0, 0.5, "background_point_radius") { v, k ->
                change((ed.backgroundState?.settings ?: st).copy(pointRadius = v), k)
            }
            AdvancedSlider("Point strength", st.pointStrength, 0.0, 100.0, 1.0, "background_point_strength") { v, k ->
                change((ed.backgroundState?.settings ?: st).copy(pointStrength = v), k)
            }
            Row(Modifier.fillMaxWidth().padding(horizontal = FcSize.PanelPadding, vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                Text("Invert", style = FcType.Label, color = Fc.Secondary, modifier = Modifier.weight(1f))
                Switch(
                    checked = st.invert,
                    onCheckedChange = { change((ed.backgroundState?.settings ?: st).copy(invert = it)) },
                    colors = SwitchDefaults.colors(checkedTrackColor = Fc.Accent, uncheckedTrackColor = Fc.SliderOff),
                    modifier = Modifier.testTag("background_invert"),
                )
            }
        }
        Spacer(Modifier.height(12.dp))
    }
}

@Composable
private fun AdvancedSlider(
    label: String, value: Double, min: Double, max: Double, step: Double, tag: String,
    onChange: (Double, String?) -> Unit,
) {
    var v by remember(value) { mutableDoubleStateOf(value) }
    var token by remember { mutableStateOf(java.util.UUID.randomUUID().toString()) }
    SliderRow(label, "${round(v * 10.0) / 10.0}") {
        FcSlider(
            fraction = ((v - min) / (max - min)).toFloat(),
            onChange = { f ->
                val raw = min + f.coerceIn(0f, 1f).toDouble() * (max - min)
                v = round(raw / step) * step
                onChange(v, token)
            },
            onDone = { token = java.util.UUID.randomUUID().toString() },
            description = label, tag = tag,
        )
    }
}

// ------------------------------------------------------------------ animation frame bar

/** Whether the floating model-animation frame bar should be on screen. */
fun frameBarVisible(ed: EditorController): Boolean {
    val state = ed.object3DState ?: return false
    val clip = ed.selectedClip ?: return false
    val st = state.settings
    return state.clip == clip.id && st.modelPath.isNotBlank() && st.modelAnim >= 0
}

/**
 * The animation frame browser: a floating bar across the bottom with every frame of the selected
 * model animation, keyframe diamonds and scrubbing. The editor lifts the preview above it (or
 * shrinks it when there is no room) so the two never overlap — see EditorScreen.
 */
@Composable
fun AnimationFrameBar(ed: EditorController, modifier: Modifier = Modifier) {
    val st = ed.object3DState?.settings ?: return
    if (st.modelPath.isBlank() || st.modelAnim < 0) return
    val info by produceState<ModelInfo?>(null, st.modelPath) {
        value = runCatching { ed.engine.command { modelInfo(st.modelPath) } }.getOrNull()
    }
    val anim = info?.animations?.getOrNull(st.modelAnim) ?: return
    val fps = if (ed.rateDen != 0L) ed.rateNum.toDouble() / ed.rateDen else 30.0
    val frames = (kotlin.math.ceil(anim.duration * fps).toInt() + 1).coerceIn(1, 480)
    val current = (st.modelTime * fps).toInt().coerceIn(0, frames - 1)
    val keyTimes = remember(anim) { anim.keys.sorted() }
    val keyFrames = remember(keyTimes, fps) { keyTimes.map { (it * fps).toInt() }.toSet() }
    fun setFrame(f: Int) {
        val s = ed.object3DState?.settings ?: return
        ed.setObject(s.copy(modelTime = f.coerceIn(0, frames - 1) / fps), null)
    }
    fun jumpKey(dir: Int) {
        val t = ed.object3DState?.settings?.modelTime ?: return
        val next = if (dir > 0) {
            keyTimes.firstOrNull { it > t + 1e-3 } ?: keyTimes.lastOrNull()
        } else {
            keyTimes.lastOrNull { it < t - 1e-3 } ?: keyTimes.firstOrNull()
        }
        if (next != null) setFrame((next * fps).toInt())
    }
    val listState = rememberLazyListState()
    // Keep the playing frame on screen; user scrolling is never fought (the list only moves
    // when the current frame changes).
    LaunchedEffect(current) { runCatching { listState.scrollToItem(maxOf(0, current - 3)) } }
    Row(
        modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(18.dp))
            .editorGlass(18.dp, layer = 2f)
            .padding(horizontal = 8.dp, vertical = 8.dp)
            .testTag("frame_bar"),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        SelectChip("◀|", false, { jumpKey(-1) }, tag = "frame_bar_prev_key")
        SelectChip("◀", false, { setFrame(current - 1) }, tag = "frame_bar_step_back")
        SelectChip("▶", false, { setFrame(current + 1) }, tag = "frame_bar_step_fwd")
        SelectChip("|▶", false, { jumpKey(1) }, tag = "frame_bar_next_key")
        LazyRow(
            state = listState,
            modifier = Modifier.weight(1f).height(52.dp).testTag("frame_bar_strip"),
            horizontalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            items(frames) { f ->
                val active = f == current
                val key = f in keyFrames
                Column(
                    Modifier
                        .width(28.dp)
                        .fillMaxHeight()
                        .clip(RoundedCornerShape(8.dp))
                        .background(if (active) Fc.Accent.copy(alpha = 0.28f) else Color.Transparent)
                        .clickable { setFrame(f) }
                        .testTag("frame_bar_cell_$f"),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.Center,
                ) {
                    Text(if (key) "◆" else "·", style = FcType.Caption, color = if (key) Fc.Accent else Fc.Detail, fontSize = 9.sp)
                    Text(f.toString(), style = FcType.Caption, color = if (active) Fc.Title else Fc.Detail, fontSize = 9.sp, maxLines = 1)
                }
            }
        }
        Text(
            "%.2fs".format(st.modelTime),
            style = FcType.Caption,
            color = Fc.Secondary,
            maxLines = 1,
            modifier = Modifier.padding(start = 2.dp).testTag("frame_bar_time"),
        )
    }
}

// ------------------------------------------------------------------ scene lights

/** The scene's light objects: coloured lights placed in 3D that light every 3D layer. */
@Composable
fun LightsPanel(ed: EditorController, close: () -> Unit, modifier: Modifier = Modifier) {
    val lighting = ed.sceneLights
    LaunchedEffect(Unit) { ed.loadLights() }
    val light = lighting.lights.firstOrNull { it.id == ed.selectedLightId } ?: lighting.lights.firstOrNull()
    ToolPanel(
        "Lights", close, modifier, tag = "lights_panel",
        actions = {
            if (light != null) {
                PanelAction(LucideIcons.Trash2.glyph(), "Delete light", { ed.removeLight(light.id); ed.selectedLightId = 0L }, tag = "lights_delete")
            }
        },
    ) {
        Text(
            "Coloured light objects that light every 3D object and text. Add as many as you like.",
            style = FcType.Caption, color = Fc.Detail,
            modifier = Modifier.fillMaxWidth().padding(horizontal = FcSize.PanelPadding),
        )
        Spacer(Modifier.height(10.dp))
        Text("Ambient fill", style = FcType.Label, color = Fc.Title, modifier = Modifier.padding(horizontal = FcSize.PanelPadding))
        val amb = lighting.ambientColor
        ColorPicker("Ambient color", android.graphics.Color.rgb(amb[0], amb[1], amb[2])) { c ->
            ed.setAmbient(listOf(android.graphics.Color.red(c) / 255f, android.graphics.Color.green(c) / 255f, android.graphics.Color.blue(c) / 255f), lighting.ambientLevel, "lights_ambient")
        }
        AdvancedSlider("Ambient level", lighting.ambientLevel, 0.0, 3.0, 0.05, "lights_ambient_level") { v, k ->
            ed.setAmbient(amb, v, k)
        }
        Spacer(Modifier.height(6.dp))
        Row(
            Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = FcSize.PanelPadding),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            SelectChip("+ Directional", false, { ed.addLight("directional") { ed.selectedLightId = it } }, tag = "lights_add_directional")
            SelectChip("+ Point", false, { ed.addLight("point") { ed.selectedLightId = it } }, tag = "lights_add_point")
            SelectChip("+ Spot", false, { ed.addLight("spot") { ed.selectedLightId = it } }, tag = "lights_add_spot")
        }
        if (lighting.lights.isEmpty()) {
            Text(
                "No lights yet: 3D objects use their classic light. Add one to light the scene yourself.",
                style = FcType.Body, color = Fc.Secondary,
                modifier = Modifier.padding(horizontal = FcSize.PanelPadding, vertical = 12.dp),
            )
        }
        lighting.lights.forEach { l ->
            Row(
                Modifier
                    .fillMaxWidth()
                    .padding(horizontal = FcSize.PanelPadding, vertical = 2.dp)
                    .clip(RoundedCornerShape(10.dp))
                    .background(if (l.id == light?.id) Fc.Accent.copy(alpha = 0.18f) else Color.Transparent)
                    .clickable { ed.selectedLightId = l.id }
                    .testTag("lights_row_${l.id}"),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Box(Modifier.padding(start = 6.dp).size(18.dp).clip(CircleShape).background(Color(l.color[0], l.color[1], l.color[2])))
                Text(
                    "${l.name} · ${l.kind}",
                    style = FcType.Body, color = if (l.enabled) Fc.Title else Fc.Detail,
                    maxLines = 1, overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f).padding(start = 8.dp),
                )
                Switch(
                    checked = l.enabled,
                    onCheckedChange = { ed.setLight(l.copy(enabled = it)) },
                    colors = SwitchDefaults.colors(checkedTrackColor = Fc.Accent, uncheckedTrackColor = Fc.SliderOff),
                    modifier = Modifier.testTag("lights_enabled_${l.id}"),
                )
            }
        }
        if (light != null) {
            Spacer(Modifier.height(8.dp))
            Text("Selected light", style = FcType.Label, color = Fc.Title, modifier = Modifier.padding(horizontal = FcSize.PanelPadding))
            Row(
                Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = FcSize.PanelPadding),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                for (kind in listOf("directional", "point", "spot")) {
                    SelectChip(kind.replaceFirstChar { it.uppercase() }, light.kind == kind, {
                        ed.setLight(light.copy(kind = kind))
                    }, tag = "lights_kind_$kind")
                }
            }
            Spacer(Modifier.height(6.dp))
            ColorPicker("Light color", android.graphics.Color.rgb(light.color[0], light.color[1], light.color[2])) { c ->
                ed.setLight(light.copy(color = listOf(android.graphics.Color.red(c) / 255f, android.graphics.Color.green(c) / 255f, android.graphics.Color.blue(c) / 255f)), "lights_color")
            }
            AdvancedSlider("Brightness", light.intensity, 0.0, 8.0, 0.05, "lights_intensity") { v, k ->
                ed.setLight(light.copy(intensity = v), k)
            }
            if (light.kind != "directional") {
                AdvancedSlider("X", light.position[0], -2000.0, 4000.0, 1.0, "lights_x") { v, k ->
                    ed.setLight(light.copy(position = listOf(v, light.position[1], light.position[2])), k)
                }
                AdvancedSlider("Y", light.position[1], -2000.0, 4000.0, 1.0, "lights_y") { v, k ->
                    ed.setLight(light.copy(position = listOf(light.position[0], v, light.position[2])), k)
                }
                AdvancedSlider("Z", light.position[2], 0.0, 3000.0, 5.0, "lights_z") { v, k ->
                    ed.setLight(light.copy(position = listOf(light.position[0], light.position[1], v)), k)
                }
                AdvancedSlider("Falloff", light.range, 0.0, 4000.0, 10.0, "lights_range") { v, k ->
                    ed.setLight(light.copy(range = v), k)
                }
            }
            if (light.kind != "point") {
                AdvancedSlider("Angle", light.angle, 0.0, 360.0, 1.0, "lights_angle") { v, k ->
                    ed.setLight(light.copy(angle = v), k)
                }
                AdvancedSlider("Elevation", light.elevation, -90.0, 90.0, 1.0, "lights_elevation") { v, k ->
                    ed.setLight(light.copy(elevation = v), k)
                }
            }
            if (light.kind == "spot") {
                AdvancedSlider("Cone", light.cone, 1.0, 89.0, 1.0, "lights_cone") { v, k ->
                    ed.setLight(light.copy(cone = v), k)
                }
                AdvancedSlider("Soft edge", light.softness, 0.0, 1.0, 0.05, "lights_softness") { v, k ->
                    ed.setLight(light.copy(softness = v), k)
                }
            }
        }
        Spacer(Modifier.height(12.dp))
    }
}

/** Quick colour choices; RGB sliders below give full control. */
@Composable
private fun SwatchRow(color: List<Float>, onPick: (Float, Float, Float) -> Unit) {
    val presets = listOf(
        listOf(1f, 1f, 1f), listOf(1f, 0.9f, 0.75f), listOf(1f, 0.35f, 0.25f), listOf(1f, 0.65f, 0.25f),
        listOf(1f, 0.95f, 0.4f), listOf(0.45f, 1f, 0.5f), listOf(0.35f, 0.9f, 1f), listOf(0.4f, 0.55f, 1f),
        listOf(0.8f, 0.45f, 1f), listOf(1f, 0.5f, 0.8f),
    )
    Row(
        Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = FcSize.PanelPadding, vertical = 4.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        presets.forEach { p ->
            val on = color[0] == p[0] && color[1] == p[1] && color[2] == p[2]
            Box(
                Modifier
                    .size(26.dp)
                    .clip(CircleShape)
                    .background(Color(p[0], p[1], p[2]))
                    .border(if (on) 2.dp else 1.dp, if (on) Fc.Accent else Fc.SliderOff, CircleShape)
                    .clickable { onPick(p[0], p[1], p[2]) }
                    .testTag("lights_swatch_${p[0]}_${p[1]}_${p[2]}"),
            )
        }
    }
}
