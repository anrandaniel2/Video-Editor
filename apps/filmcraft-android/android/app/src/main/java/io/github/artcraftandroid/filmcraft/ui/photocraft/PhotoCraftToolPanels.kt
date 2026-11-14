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
import io.github.artcraftandroid.filmcraft.ui.editorGlass
import io.github.artcraftandroid.filmcraft.ui.glyph
import kotlin.math.roundToInt

/**
 * PhotoCraft's chrome, drawn the way the reference apps and this app's own draw mode draw theirs: a
 * narrow icon rail down the left with the open group's tools on a glass card beside it, the picture
 * full-bleed in the middle, a pill bar across the top, a strip of panel icons down the right edge and
 * the active tool's options on a floating card.
 *
 * The glass is `editorGlass(…, blurRadius = 6.dp)` with the editor's frost, the same one the draw
 * mode's rail and transport use, so the picture blurs faintly behind every control instead of being
 * hidden by an opaque panel.
 */

/** The rail down the left: the nine groups, then the all-tools and reset buttons. */
@Composable
internal fun ToolRail(
    active: ToolGroup,
    open: ToolGroup?,
    onGroup: (ToolGroup) -> Unit,
    onMore: () -> Unit,
    onReset: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier.verticalScroll(rememberScrollState()).padding(vertical = 6.dp, horizontal = 6.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        RailButton(LucideIcons.ChevronDown.glyph(), "Show or hide this group's tools", { onGroup(active) })
        for (group in ToolGroup.entries) {
            RailButton(
                group.glyph,
                "${group.label} tools, ${group.tools.size}",
                { onGroup(group) },
                selected = group == active,
                expanded = group == open,
                tag = "photocraft_group_${group.name.lowercase()}",
            )
        }
        Box(Modifier.width(24.dp).height(1.dp).background(Fc.Detail.copy(alpha = .35f)).padding(vertical = 4.dp))
        RailButton(LucideIcons.Ellipsis.glyph(), "Every tool", onMore, tag = "photocraft_all_tools")
        RailButton(LucideIcons.RotateCcw.glyph(), "Reset every edit", onReset, tag = "photocraft_reset")
    }
}

@Composable
private fun RailButton(
    glyph: Glyph,
    description: String,
    onClick: () -> Unit,
    selected: Boolean = false,
    expanded: Boolean = false,
    tag: String? = null,
) {
    Box(
        Modifier
            .size(40.dp)
            .clip(RoundedCornerShape(12.dp))
            .background(
                when {
                    selected -> Fc.Accent
                    expanded -> Fc.Accent.copy(alpha = .28f)
                    else -> Color.Transparent
                },
            )
            .clickable(role = Role.Button, onClick = onClick)
            .then(if (tag != null) Modifier.testTag(tag) else Modifier)
            .semantics { contentDescription = description },
        contentAlignment = Alignment.Center,
    ) {
        GlyphIcon(glyph, if (selected) Color.White else Fc.Icon, size = 21.dp)
    }
}

/** The floating card beside the rail: the open group's tools, then the chosen tool's own sliders. */
@Composable
internal fun ToolPickerCard(
    group: ToolGroup,
    current: PhotoTool,
    values: PhotoToolValues,
    onTool: (PhotoTool) -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier.editorGlass(20.dp).padding(vertical = 8.dp)) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 14.dp, vertical = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(group.label.uppercase(), style = FcType.ToolLabel.copy(fontSize = 9.sp), color = Fc.Detail)
            Spacer(Modifier.weight(1f))
            Text("${group.tools.size} tools", style = FcType.ToolLabel.copy(fontSize = 9.sp), color = Fc.Detail)
        }
        Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState())) {
            for (tool in group.tools) {
                ToolRow(tool, tool == current, { onTool(tool) })
            }
        }
        val sliders = current.options.filterIsInstance<ToolOption.Slider>().take(2)
        if (sliders.isNotEmpty()) {
            Box(Modifier.fillMaxWidth().padding(horizontal = 14.dp, vertical = 6.dp).height(1.dp).background(Fc.Detail.copy(alpha = .3f)))
            for (slider in sliders) OptionSlider(current, slider, values)
        }
    }
}

@Composable
private fun ToolRow(tool: PhotoTool, selected: Boolean, onClick: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .height(38.dp)
            .padding(horizontal = 6.dp)
            .clip(RoundedCornerShape(9.dp))
            .background(if (selected) Fc.Accent else Color.Transparent)
            .clickable(role = Role.Button, onClick = onClick)
            .testTag("photocraft_tool_${tool.name.lowercase()}")
            .padding(horizontal = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(9.dp),
    ) {
        GlyphIcon(tool.glyph, if (selected) Color.White else Fc.Icon, size = 17.dp)
        Text(
            tool.label,
            style = FcType.Caption,
            color = if (selected) Color.White else Fc.Body,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/** Every tool in every group: what the rail's "…" and the bar's menu open. */
@Composable
internal fun AllToolsCard(
    current: PhotoTool,
    onTool: (PhotoTool) -> Unit,
    modifier: Modifier = Modifier,
    glass: Boolean = true,
) {
    // Inside the phone's sheet the card is already on glass: a second haze pane would nest effects.
    val base = if (glass) modifier.editorGlass(20.dp) else modifier
    Column(base.verticalScroll(rememberScrollState()).padding(vertical = 8.dp)) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 14.dp, vertical = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("ALL TOOLS", style = FcType.ToolLabel.copy(fontSize = 9.sp), color = Fc.Detail)
            Spacer(Modifier.weight(1f))
            Text("${PhotoTool.entries.size}", style = FcType.ToolLabel.copy(fontSize = 9.sp), color = Fc.Detail)
        }
        for (group in ToolGroup.entries) {
            Text(
                group.label.uppercase(),
                style = FcType.ToolLabel.copy(fontSize = 9.sp),
                color = Fc.AccentLight,
                modifier = Modifier.padding(start = 14.dp, top = 6.dp, bottom = 2.dp),
            )
            for (tool in group.tools) ToolRow(tool, tool == current, { onTool(tool) })
        }
    }
}

/** The tools of the open group as a strip, for the sheet at the bottom of a phone. */
@Composable
internal fun ToolStrip(
    group: ToolGroup,
    current: PhotoTool,
    onTool: (PhotoTool) -> Unit,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier.horizontalScroll(rememberScrollState()).padding(horizontal = 8.dp, vertical = 4.dp),
        horizontalArrangement = Arrangement.spacedBy(3.dp),
    ) {
        for (tool in group.tools) {
            Column(
                Modifier
                    .width(54.dp)
                    .height(50.dp)
                    .clip(RoundedCornerShape(10.dp))
                    .background(if (tool == current) Fc.Accent else Color.Transparent)
                    .clickable(role = Role.Button, onClick = { onTool(tool) })
                    .testTag("photocraft_tool_${tool.name.lowercase()}")
                    .semantics { contentDescription = tool.label },
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.Center,
            ) {
                GlyphIcon(tool.glyph, if (tool == current) Color.White else Fc.Body, size = 18.dp)
                Spacer(Modifier.height(2.dp))
                Text(
                    tool.label,
                    style = FcType.ToolLabel.copy(fontSize = 9.sp),
                    color = if (tool == current) Color.White else Fc.Detail,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    textAlign = TextAlign.Center,
                )
            }
        }
    }
}

/** The panel shortcuts down the right edge, like the reference's stack of panel icons. */
@Composable
internal fun PanelStrip(
    current: PhotoTool,
    onTool: (PhotoTool) -> Unit,
    modifier: Modifier = Modifier,
) {
    val quick = listOf(
        PhotoTool.Adjust,
        PhotoTool.Filters,
        PhotoTool.Crop,
        PhotoTool.Eyedropper,
        PhotoTool.SpotHealing,
        PhotoTool.Brush,
        PhotoTool.HorizontalType,
        PhotoTool.Zoom,
    )
    Column(
        modifier.verticalScroll(rememberScrollState()).padding(vertical = 6.dp, horizontal = 6.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        for (tool in quick) {
            RailButton(
                tool.glyph,
                tool.label,
                { onTool(tool) },
                selected = tool == current,
                tag = "photocraft_quick_${tool.name.lowercase()}",
            )
        }
    }
}

/** The card's title: the tool, its group and the picture's real pixel size. */
@Composable
internal fun PanelHeader(tool: PhotoTool, width: Int, height: Int, modifier: Modifier = Modifier) {
    Column(modifier.fillMaxWidth()) {
        Row(
            Modifier.fillMaxWidth().padding(start = 14.dp, end = 12.dp, top = 10.dp, bottom = 2.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            GlyphIcon(tool.glyph, Fc.AccentLight, size = 16.dp)
            Text(tool.label, style = FcType.Caption, color = Fc.Title, maxLines = 1)
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
        Box(Modifier.fillMaxWidth().padding(horizontal = 14.dp, vertical = 6.dp).height(1.dp).background(Fc.Detail.copy(alpha = .3f)))
    }
}

/** Zoom and Fit, floating over the bottom-left corner of the picture. */
@Composable
internal fun ZoomPill(view: PhotoViewState, modifier: Modifier = Modifier) {
    Row(
        modifier.height(36.dp).editorGlass(18.dp).padding(horizontal = 4.dp),
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
        if (switches.isNotEmpty()) {
            Row(
                Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 4.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                for (option in switches) {
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
internal fun OptionSlider(tool: PhotoTool, option: ToolOption.Slider, values: PhotoToolValues) {
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
private fun RowScope.ToolActionButton(action: ToolAction, modifier: Modifier, onClick: () -> Unit) {
    Column(
        modifier
            .height(50.dp)
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
