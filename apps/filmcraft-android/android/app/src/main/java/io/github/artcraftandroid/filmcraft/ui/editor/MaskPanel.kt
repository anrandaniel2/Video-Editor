package io.github.artcraftandroid.filmcraft.ui.editor

import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableDoubleStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import io.github.artcraftandroid.filmcraft.core.MaskCombine
import io.github.artcraftandroid.filmcraft.core.MaskInfo
import io.github.artcraftandroid.filmcraft.core.MaskShape
import io.github.artcraftandroid.filmcraft.ui.Fc
import io.github.artcraftandroid.filmcraft.ui.FcSlider
import io.github.artcraftandroid.filmcraft.ui.FcSize
import io.github.artcraftandroid.filmcraft.ui.FcType
import io.github.artcraftandroid.filmcraft.ui.LucideIcons
import io.github.artcraftandroid.filmcraft.ui.PanelAction
import io.github.artcraftandroid.filmcraft.ui.SelectChip
import io.github.artcraftandroid.filmcraft.ui.SliderRow
import io.github.artcraftandroid.filmcraft.ui.ToolPanel
import io.github.artcraftandroid.filmcraft.ui.glyph
import kotlin.math.max
import kotlin.math.round

/** Clip cut-out masks: shapes, combine modes and the engine's feather/expansion/opacity controls. */
@Composable
fun MaskPanel(ed: EditorController, close: () -> Unit, modifier: Modifier = Modifier) {
    val mask = ed.selectedMask
    val state = ed.maskState
    ToolPanel(
        "Mask", close, modifier, tag = "mask_panel",
        actions = {
            if (mask != null) {
                PanelAction(
                    LucideIcons.X.glyph(), "Delete mask point", { ed.removeMaskVertex() },
                    enabled = ed.selectedMaskVertex >= 0 && mask.path.vertices.size > 3,
                    tag = "mask_delete_point_button",
                )
                PanelAction(LucideIcons.Trash2.glyph(), "Delete mask", { ed.deleteMask() }, tag = "mask_delete_button")
                if ((state?.masks?.size ?: 0) > 1) {
                    PanelAction(LucideIcons.CircleX.glyph(), "Clear masks", { ed.clearMasks() }, tag = "mask_clear_button")
                }
            }
        },
    ) {
        Text(
            "Cut out this clip. Add a shape, then drag it or its points on the preview.",
            style = FcType.Caption, color = Fc.Detail,
            modifier = Modifier.fillMaxWidth().padding(horizontal = FcSize.PanelPadding),
        )
        Spacer(Modifier.height(10.dp))
        Row(
            Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = FcSize.PanelPadding),
            horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically,
        ) {
            SelectChip("Ellipse", false, { ed.addMask(MaskShape.ELLIPSE) }, tag = "mask_add_ellipse")
            SelectChip("Rectangle", false, { ed.addMask(MaskShape.RECTANGLE) }, tag = "mask_add_rectangle")
            SelectChip("Polygon", false, { ed.addMask(MaskShape.POLYGON) }, tag = "mask_add_polygon")
            SelectChip("Draw", ed.maskDrawing, { ed.maskDrawing = !ed.maskDrawing; ed.maskAddingPoint = false }, tag = "mask_draw")
        }
        if (state != null && state.masks.isNotEmpty()) {
            Spacer(Modifier.height(8.dp))
            Row(
                Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = FcSize.PanelPadding),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                state.masks.forEach { m ->
                    SelectChip(m.name, m.index == ed.selectedMaskIndex, { ed.selectMask(m.index) }, tag = "mask_item_${m.index}")
                }
            }
        }
        if (mask == null) {
            Text(
                if (ed.maskDrawing) "Draw one closed stroke on the preview." else "Choose a shape to start. Masks reveal the layers underneath.",
                style = FcType.Body, color = Fc.Secondary,
                modifier = Modifier.padding(horizontal = FcSize.PanelPadding, vertical = 16.dp),
            )
        } else {
            MaskControls(ed, mask)
        }
        Spacer(Modifier.height(12.dp))
    }
}

@Composable
private fun MaskControls(ed: EditorController, mask: MaskInfo) {
    val shortSide = maxOf(1u, minOf(ed.maskState?.sourceWidth ?: 1080u, ed.maskState?.sourceHeight ?: 1080u)).toDouble()
    val reach = max(100.0, shortSide * 0.25)
    fun change(settings: io.github.artcraftandroid.filmcraft.core.MaskSettings, merge: String? = null) = ed.setMaskSettings(settings, merge)
    Spacer(Modifier.height(8.dp))
    Row(Modifier.fillMaxWidth().padding(horizontal = FcSize.PanelPadding), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        SelectChip("Add point", ed.maskAddingPoint, { ed.maskAddingPoint = !ed.maskAddingPoint; ed.maskDrawing = false }, tag = "mask_add_point")
        Text(
            if (mask.animated) "Keyframed · edits at playhead" else "Drag a corner to resize",
            style = FcType.Caption, color = Fc.Detail, modifier = Modifier.align(Alignment.CenterVertically),
        )
    }
    Spacer(Modifier.height(8.dp))
    HorizontalDivider(color = Fc.Divider, thickness = 1.dp)
    MaskSlider("Feather", mask.settings.feather, 0.0, reach, 0.1, "mask_feather_slider", { v, key ->
        change((ed.selectedMask?.settings ?: mask.settings).copy(feather = v), key)
    }) { "${round(it * 10.0) / 10.0} px" }
    MaskSlider("Expansion", mask.settings.expansion, -reach, reach, 0.1, "mask_expansion_slider", { v, key ->
        change((ed.selectedMask?.settings ?: mask.settings).copy(expansion = v), key)
    }) { "${round(it * 10.0) / 10.0} px" }
    MaskSlider("Mask opacity", mask.settings.opacity, 0.0, 100.0, 1.0, "mask_opacity_slider", { v, key ->
        change((ed.selectedMask?.settings ?: mask.settings).copy(opacity = v), key)
    }) { "${round(it)}%" }
    Row(
        Modifier.fillMaxWidth().padding(horizontal = FcSize.PanelPadding, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text("Invert mask", style = FcType.Label, color = Fc.Secondary, modifier = Modifier.weight(1f))
        Switch(
            checked = mask.settings.inverted,
            onCheckedChange = { change(mask.settings.copy(inverted = it)) },
            colors = SwitchDefaults.colors(checkedTrackColor = Fc.Accent, uncheckedTrackColor = Fc.SliderOff),
            modifier = Modifier.testTag("mask_invert_switch"),
        )
    }
    Text("Combine", style = FcType.Label, color = Fc.Secondary, modifier = Modifier.padding(horizontal = FcSize.PanelPadding, vertical = 4.dp))
    Row(
        Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = FcSize.PanelPadding),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        MaskCombine.entries.forEach { mode ->
            SelectChip(if (mode == MaskCombine.NONE) "Off" else mode.name.lowercase().replaceFirstChar { it.uppercase() }, mask.settings.combine == mode, {
                change(mask.settings.copy(combine = mode))
            }, tag = "mask_combine_${mode.name.lowercase()}")
        }
    }
}

/** One undo step per drag: the token changes only after the finger lifts. */
@Composable
private fun MaskSlider(
    label: String, value: Double, min: Double, max: Double, step: Double, tag: String,
    onChange: (Double, String?) -> Unit, display: (Double) -> String,
) {
    var v by remember(value) { mutableDoubleStateOf(value) }
    var token by remember { mutableStateOf(java.util.UUID.randomUUID().toString()) }
    SliderRow(label, display(v)) {
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
