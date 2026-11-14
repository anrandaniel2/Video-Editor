package io.github.artcraftandroid.filmcraft.ui.photocraft

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.artcraftandroid.filmcraft.ui.Fc
import io.github.artcraftandroid.filmcraft.ui.FcSlider
import io.github.artcraftandroid.filmcraft.ui.FcType
import io.github.artcraftandroid.filmcraft.ui.Glyph
import io.github.artcraftandroid.filmcraft.ui.GlyphIcon
import io.github.artcraftandroid.filmcraft.ui.LucideIcons
import io.github.artcraftandroid.filmcraft.ui.SelectChip
import io.github.artcraftandroid.filmcraft.ui.SliderRow
import io.github.artcraftandroid.filmcraft.ui.editorCard
import io.github.artcraftandroid.filmcraft.ui.glyph
import kotlin.math.roundToInt

/**
 * PhotoCraft's chrome, laid out the way a tablet editor lays it out: a rail of tool groups down the
 * left with the open group's tools beside it, the picture full-bleed in the middle with its own bar
 * and zoom control, and the active tool's panel as a card on the right (a sheet at the bottom on a
 * phone). Every tool and every option still comes out of [PhotoTool] and [ToolOption].
 */

/** The nine groups, down the left in landscape and across the top of the sheet in portrait. */
@Composable
internal fun GroupRail(
    active: ToolGroup,
    open: ToolGroup?,
    onGroup: (ToolGroup) -> Unit,
    vertical: Boolean,
    modifier: Modifier = Modifier,
) {
    val scroll = rememberScrollState()
    if (vertical) {
        Column(
            modifier.verticalScroll(scroll).padding(vertical = 8.dp, horizontal = 5.dp),
            verticalArrangement = Arrangement.spacedBy(3.dp),
        ) {
            for (group in ToolGroup.entries) {
                GroupButton(group, group == active, group == open, { onGroup(group) }, true, Modifier.fillMaxWidth())
            }
        }
    } else {
        Row(
            modifier.horizontalScroll(scroll).padding(horizontal = 8.dp, vertical = 4.dp),
            horizontalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            for (group in ToolGroup.entries) {
                GroupButton(group, group == active, group == open, { onGroup(group) }, false, Modifier.width(58.dp))
            }
        }
    }
}

@Composable
private fun GroupButton(
    group: ToolGroup,
    active: Boolean,
    expanded: Boolean,
    onClick: () -> Unit,
    vertical: Boolean,
    modifier: Modifier,
) {
    Column(
        modifier
            .height(if (vertical) 50.dp else 48.dp)
            .clip(RoundedCornerShape(if (vertical) 13.dp else 12.dp))
            .background(
                when {
                    active -> Fc.Accent.copy(alpha = .22f)
                    expanded -> Fc.Raised.copy(alpha = .9f)
                    else -> Color.Transparent
                },
            )
            .clickable(role = Role.Button, onClick = onClick)
            .testTag("photocraft_group_${group.name.lowercase()}")
            .semantics { contentDescription = "${group.label} tools, ${group.tools.size}" },
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        GlyphIcon(group.glyph, if (active || expanded) Fc.AccentLight else Fc.Icon, size = 19.dp)
        Spacer(Modifier.height(3.dp))
        Text(
            group.label,
            style = FcType.ToolLabel.copy(fontSize = 9.sp),
            color = if (active) Fc.AccentLight else Fc.Detail,
            maxLines = 1,
            overflow = TextOverflow.Clip,
        )
    }
}

/** The open group's tools: a second rail in landscape, a scrolling strip in the sheet. */
@Composable
internal fun ToolStrip(
    group: ToolGroup,
    current: PhotoTool,
    onTool: (PhotoTool) -> Unit,
    vertical: Boolean,
    modifier: Modifier = Modifier,
) {
    val scroll = rememberScrollState()
    if (vertical) {
        Column(
            modifier.verticalScroll(scroll).padding(vertical = 8.dp, horizontal = 4.dp),
            verticalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            for (tool in group.tools) {
                ToolButton(tool, tool == current, { onTool(tool) }, true, Modifier.fillMaxWidth())
            }
        }
    } else {
        Row(
            modifier.horizontalScroll(scroll).padding(horizontal = 8.dp, vertical = 2.dp),
            horizontalArrangement = Arrangement.spacedBy(3.dp),
        ) {
            for (tool in group.tools) {
                ToolButton(tool, tool == current, { onTool(tool) }, false, Modifier.width(56.dp))
            }
        }
    }
}

@Composable
private fun ToolButton(tool: PhotoTool, selected: Boolean, onClick: () -> Unit, vertical: Boolean, modifier: Modifier) {
    Column(
        modifier
            .height(if (vertical) 40.dp else 52.dp)
            .clip(RoundedCornerShape(10.dp))
            .background(if (selected) Fc.Accent.copy(alpha = .85f) else Color.Transparent)
            .clickable(role = Role.Button, onClick = onClick)
            .testTag("photocraft_tool_${tool.name.lowercase()}")
            .semantics { contentDescription = tool.label },
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        GlyphIcon(tool.glyph, if (selected) Fc.Title else Fc.Body, size = if (vertical) 18.dp else 17.dp)
        if (!vertical) {
            Spacer(Modifier.height(2.dp))
            Text(
                tool.label,
                style = FcType.ToolLabel.copy(fontSize = 9.sp),
                color = if (selected) Fc.Title else Fc.Detail,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                textAlign = TextAlign.Center,
            )
        }
    }
}

/** The card's title: the tool, the group it came from, and the picture's real size. */
@Composable
internal fun PanelHeader(tool: PhotoTool, width: Int, height: Int, modifier: Modifier = Modifier) {
    Column(modifier.fillMaxWidth()) {
        Row(
            Modifier.fillMaxWidth().padding(start = 14.dp, end = 14.dp, top = 12.dp, bottom = 2.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            GlyphIcon(tool.glyph, Fc.AccentLight, size = 17.dp)
            Text(tool.label, style = FcType.Caption, color = Fc.Title, maxLines = 1)
            Text(tool.group.label.uppercase(), style = FcType.ToolLabel.copy(fontSize = 9.sp), color = Fc.Detail, maxLines = 1)
            Spacer(Modifier.weight(1f))
            if (width > 0 && height > 0) {
                Text("$width × $height", style = FcType.ToolLabel.copy(fontSize = 10.sp), color = Fc.Detail, maxLines = 1)
            }
        }
        Text(
            tool.hint,
            style = FcType.Caption,
            color = Fc.Detail,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.fillMaxWidth().padding(horizontal = 14.dp, vertical = 2.dp),
        )
        Box(Modifier.fillMaxWidth().padding(horizontal = 14.dp, vertical = 6.dp).height(1.dp).background(Fc.Raised))
    }
}

/** Zoom and Fit, floating over the bottom-left corner of the picture. */
@Composable
internal fun ZoomPill(view: PhotoViewState, modifier: Modifier = Modifier) {
    Row(
        modifier
            .height(36.dp)
            .clip(CircleShape)
            .background(Fc.Panel.copy(alpha = .92f))
            .padding(horizontal = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        PillButton(LucideIcons.ZoomOut.glyph(), "Zoom out") { view.zoomTo(view.zoom / 1.5f) }
        Text(
            "${(view.zoom * 100).roundToInt()}%",
            style = FcType.Caption,
            color = Fc.Title,
            maxLines = 1,
            textAlign = TextAlign.Center,
            modifier = Modifier.width(46.dp),
        )
        PillButton(LucideIcons.ZoomIn.glyph(), "Zoom in") { view.zoomTo(view.zoom * 1.5f) }
        PillButton(LucideIcons.Scaling.glyph(), "Fit the picture") { view.reset() }
    }
}

@Composable
private fun PillButton(glyph: Glyph, description: String, onClick: () -> Unit) {
    Box(
        Modifier
            .size(28.dp)
            .clip(CircleShape)
            .clickable(role = Role.Button, onClick = onClick)
            .semantics { contentDescription = description },
        contentAlignment = Alignment.Center,
    ) {
        GlyphIcon(glyph, Fc.Body, size = 16.dp)
    }
}

/** A tool's options: the actions the pipeline runs now, then its sliders, choices and switches. */
@Composable
internal fun ToolOptionsPanel(tool: PhotoTool, values: PhotoToolValues, onAction: (ToolAction) -> Unit) {
    val switches = tool.options.filterIsInstance<ToolOption.Switch>()
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(bottom = 10.dp)) {
        if (tool.actions.isNotEmpty()) {
            Row(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 6.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                for (action in tool.actions) ToolActionButton(action, Modifier.weight(1f)) { onAction(action) }
            }
        }
        for (option in tool.options) {
            when (option) {
                is ToolOption.Slider -> OptionSlider(tool, option, values)
                is ToolOption.Choice -> OptionChoice(tool, option, values)
                is ToolOption.Switch -> Unit
            }
        }
        for (pair in switches.chunked(2)) {
            Row(
                Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 4.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                for (option in pair) {
                    SelectChip(option.label, values.switch(tool, option), { values.setSwitch(tool, option, !values.switch(tool, option)) })
                }
            }
        }
        Text(
            "The palette and its options are complete. Tone, crop, rotate, flip and sampling act on this " +
                "picture now; strokes, selections and vector edits are applied by the vendored PhotoCraft engine.",
            style = FcType.Caption,
            color = Fc.Detail,
            modifier = Modifier.fillMaxWidth().padding(horizontal = 14.dp, vertical = 8.dp),
        )
    }
}

@Composable
private fun OptionSlider(tool: PhotoTool, option: ToolOption.Slider, values: PhotoToolValues) {
    val value = values.number(tool, option)
    val span = option.max - option.min
    val fraction = if (span > 0f) ((value - option.min) / span).coerceIn(0f, 1f) else 0f
    SliderRow(option.label, "${value.roundToInt()}${option.suffix}") {
        FcSlider(
            fraction = fraction,
            onChange = { position -> values.setNumber(tool, option, option.min + position * span) },
            description = option.label,
            numericRange = option.min..option.max,
        )
    }
}

@Composable
private fun OptionChoice(tool: PhotoTool, option: ToolOption.Choice, values: PhotoToolValues) {
    val selected = values.choice(tool, option)
    Column(Modifier.fillMaxWidth().padding(horizontal = 14.dp, vertical = 5.dp)) {
        Text(option.label.uppercase(), style = FcType.ToolLabel.copy(fontSize = 9.sp), color = Fc.Detail)
        Spacer(Modifier.height(4.dp))
        Row(
            Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            option.choices.forEachIndexed { index, label ->
                SelectChip(label, selected == index, { values.setChoice(tool, option, index) })
            }
        }
    }
}

@Composable
private fun ToolActionButton(action: ToolAction, modifier: Modifier = Modifier, onClick: () -> Unit) {
    Column(
        modifier
            .height(52.dp)
            .clip(RoundedCornerShape(13.dp))
            .editorCard(13.dp)
            .clickable(role = Role.Button, onClick = onClick)
            .semantics { contentDescription = action.label },
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        GlyphIcon(action.glyph, Fc.Icon, size = 17.dp)
        Spacer(Modifier.height(3.dp))
        Text(action.label, style = FcType.ToolLabel, color = Fc.Body, maxLines = 1)
    }
}

/** The eyedropper and the colour sampler: what the last tap picked, and the samples kept so far. */
@Composable
internal fun SamplePanel(tool: PhotoTool, values: PhotoToolValues, view: PhotoViewState) {
    val last = view.samples.firstOrNull()
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = 14.dp)) {
        Row(Modifier.fillMaxWidth().padding(vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Box(
                Modifier
                    .size(52.dp)
                    .clip(RoundedCornerShape(12.dp))
                    .background(last ?: Fc.Raised),
            )
            Spacer(Modifier.width(12.dp))
            Column {
                Text(last?.hex() ?: "No sample yet", style = FcType.Caption, color = Fc.Title)
                Text(
                    if (last == null) "Tap the picture to read its colour."
                    else {
                        val option = tool.options.filterIsInstance<ToolOption.Choice>().firstOrNull()
                        val size = if (option == null) "one point" else option.choices.getOrNull(values.choice(tool, option)) ?: "one point"
                        "Sampled $size at the tap."
                    },
                    style = FcType.Caption,
                    color = Fc.Detail,
                )
            }
        }
        if (view.samples.size > 1) {
            Text("SAMPLES", style = FcType.ToolLabel.copy(fontSize = 9.sp), color = Fc.Detail)
            Row(
                Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(vertical = 6.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                for (sample in view.samples) {
                    Box(Modifier.size(34.dp).clip(RoundedCornerShape(9.dp)).background(sample))
                }
            }
        }
        Row(Modifier.fillMaxWidth().padding(vertical = 6.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            for (option in tool.options.filterIsInstance<ToolOption.Switch>()) {
                SelectChip(option.label, values.switch(tool, option), { values.setSwitch(tool, option, !values.switch(tool, option)) })
            }
            SelectChip("Clear samples", false, { view.samples.clear() })
        }
    }
}

/** The Hand and Zoom tools: the same view state their gestures change. */
@Composable
internal fun ViewPanel(view: PhotoViewState) {
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = 14.dp)) {
        Text(
            "Zoom ${(view.zoom * 100).roundToInt()}%",
            style = FcType.Caption,
            color = Fc.Title,
            modifier = Modifier.padding(vertical = 8.dp),
        )
        Row(
            Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            for (step in listOf(1f, 2f, 4f, 8f)) {
                SelectChip(if (step == 1f) "Fit" else "${(step * 100).roundToInt()}%", kotlin.math.abs(view.zoom - step) < 0.01f, { view.zoomTo(step) })
            }
            SelectChip("Reset view", false, { view.reset() })
        }
        Text(
            "Drag with one finger to pan, pinch to zoom, double-tap to fit. The picture itself is untouched.",
            style = FcType.Caption,
            color = Fc.Detail,
            modifier = Modifier.padding(vertical = 8.dp),
        )
    }
}

private fun Color.hex(): String {
    fun channel(value: Float): String = ((value * 255f).roundToInt().coerceIn(0, 255)).toString(16).padStart(2, '0')
    return "#${channel(red)}${channel(green)}${channel(blue)}".uppercase()
}

/** The one place a tool action reaches the controller. */
internal fun runToolAction(ed: PhotoCraftController, action: ToolAction) {
    when (action) {
        ToolAction.RotateLeft -> ed.rotate(false)
        ToolAction.RotateRight -> ed.rotate(true)
        ToolAction.FlipHorizontal -> ed.flip(true)
        ToolAction.FlipVertical -> ed.flip(false)
        ToolAction.AutoTone -> ed.autoAdjust()
    }
}
