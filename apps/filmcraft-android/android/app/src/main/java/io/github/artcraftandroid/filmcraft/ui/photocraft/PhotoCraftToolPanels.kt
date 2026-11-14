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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
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
import io.github.artcraftandroid.filmcraft.ui.Fc
import io.github.artcraftandroid.filmcraft.ui.FcSlider
import io.github.artcraftandroid.filmcraft.ui.FcType
import io.github.artcraftandroid.filmcraft.ui.GlyphIcon
import io.github.artcraftandroid.filmcraft.ui.LucideIcons
import io.github.artcraftandroid.filmcraft.ui.SelectChip
import io.github.artcraftandroid.filmcraft.ui.SliderRow
import io.github.artcraftandroid.filmcraft.ui.editorCard
import io.github.artcraftandroid.filmcraft.ui.glyph
import kotlin.math.roundToInt

/**
 * The palette as it is drawn: a rail of the nine tool groups, a flyout with the group's tools, the
 * active tool's name and hint, and the tool's own options bar. Everything the rail shows comes out
 * of [PhotoTool], so adding a tool to the enum is the only edit needed to show it here.
 */
@Composable
internal fun ToolRail(
    tool: PhotoTool,
    onTool: (PhotoTool) -> Unit,
    modifier: Modifier = Modifier,
) {
    var openGroup by remember { mutableStateOf<ToolGroup?>(tool.group) }
    Column(modifier.fillMaxWidth()) {
        Row(
            Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 8.dp),
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            for (group in ToolGroup.entries) {
                GroupChip(
                    group = group,
                    active = group == tool.group,
                    expanded = openGroup == group,
                    onClick = { openGroup = if (openGroup == group) null else group },
                )
            }
        }
        val shown = openGroup
        if (shown != null) {
            Spacer(Modifier.height(4.dp))
            // Picking a tool closes the flyout so its options get the room the tools just used.
            ToolFlyout(shown, tool) { picked ->
                openGroup = null
                onTool(picked)
            }
        }
    }
}

@Composable
private fun GroupChip(group: ToolGroup, active: Boolean, expanded: Boolean, onClick: () -> Unit) {
    Row(
        Modifier
            .height(38.dp)
            .clip(CircleShape)
            .background(if (active) Fc.Accent.copy(alpha = .18f) else Fc.Raised)
            .clickable(role = Role.Button, onClick = onClick)
            .testTag("photocraft_group_${group.name.lowercase()}")
            .semantics { contentDescription = "${group.label} tools, ${group.tools.size}" }
            .padding(horizontal = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(5.dp),
    ) {
        GlyphIcon(group.glyph, if (active) Fc.AccentLight else Fc.Icon, size = 17.dp)
        Text(group.label, style = FcType.Caption, color = if (active) Fc.AccentLight else Fc.Body, maxLines = 1)
        Box(Modifier.size(12.dp)) {
            GlyphIcon(
                LucideIcons.ChevronsUpDown.glyph(),
                if (expanded) Fc.AccentLight else Fc.Detail,
                size = 12.dp,
                modifier = Modifier.fillMaxSize(),
            )
        }
    }
}

/** The open group's tools, four to a row so no tool ever needs scrolling to be found. */
@Composable
private fun ToolFlyout(group: ToolGroup, current: PhotoTool, onTool: (PhotoTool) -> Unit) {
    val rows = group.tools.chunked(4)
    Column(
        Modifier
            .fillMaxWidth()
            .height(if (rows.size > 3) 168.dp else (rows.size * 62).dp)
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 6.dp),
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        for (row in rows) {
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(2.dp)) {
                for (option in row) ToolCell(option, option == current) { onTool(option); }
                repeat(4 - row.size) { Spacer(Modifier.weight(1f)) }
            }
        }
    }
}

@Composable
private fun RowScope.ToolCell(tool: PhotoTool, selected: Boolean, onClick: () -> Unit) {
    Column(
        Modifier
            .weight(1f)
            .height(60.dp)
            .clip(RoundedCornerShape(12.dp))
            .background(if (selected) Fc.Accent.copy(alpha = .16f) else Color.Transparent)
            .clickable(role = Role.Button, onClick = onClick)
            .testTag("photocraft_tool_${tool.name.lowercase()}")
            .padding(horizontal = 3.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        GlyphIcon(tool.glyph, if (selected) Fc.AccentLight else Fc.Icon, size = 18.dp)
        Spacer(Modifier.height(3.dp))
        Text(
            tool.label,
            style = FcType.ToolLabel,
            color = if (selected) Fc.AccentLight else Fc.Body,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
            textAlign = TextAlign.Center,
        )
    }
}

/** The active tool's name and one line about it, above its options. */
@Composable
internal fun ToolHeader(tool: PhotoTool, modifier: Modifier = Modifier) {
    Row(
        modifier
            .fillMaxWidth()
            .padding(start = 14.dp, end = 14.dp, top = 8.dp, bottom = 2.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        GlyphIcon(tool.glyph, Fc.AccentLight, size = 17.dp)
        Text(tool.label, style = FcType.Caption, color = Fc.Title, maxLines = 1)
        Text(tool.hint, style = FcType.Caption, color = Fc.Detail, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
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
                Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 12.dp, vertical = 4.dp),
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
        Text(option.label.uppercase(), style = FcType.Caption, color = Fc.Detail)
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
            .height(56.dp)
            .clip(RoundedCornerShape(14.dp))
            .editorCard(14.dp)
            .clickable(role = Role.Button, onClick = onClick)
            .semantics { contentDescription = action.label },
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        GlyphIcon(action.glyph, Fc.Icon, size = 18.dp)
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
            Text("SAMPLES", style = FcType.Caption, color = Fc.Detail)
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
