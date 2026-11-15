package io.github.artcraftandroid.filmcraft.ui.photocraft

import androidx.compose.foundation.background
import androidx.compose.foundation.border
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
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
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
 * The reference's colours, read off the screenshots: a full-bleed picture with every control floating
 * over it, a near-black left rail, a light strip of panel icons down the right edge, the same
 * near-black glass for the top bar and the cards, light ink on the dark pieces and dark ink on the
 * light strip, and one blue for whatever is active.
 */
internal object PhotoUi {
    val Workspace = Color(0xFF101014)
    val Artboard = Color(0xFFE9E9EC)
    val Rail = Color(0xFF17171C)
    val Card = Color(0xFF1B1B21)
    val Light = Color(0xFFF2F2F5)
    val RailInk = Color(0xFFEDEDF2)
    val RailInkSoft = Color(0xFF8A8A94)
    val Pill = Color(0xFF1B1B21)
    val PillInk = Color(0xFFF2F2F5)
    val PillInkSoft = Color(0xFF9C9CA6)
    val PillField = Color(0xFF2A2A33)
    val Accent = Color(0xFF0A84FF)
}

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
        modifier.verticalScroll(rememberScrollState()).padding(vertical = 6.dp, horizontal = 3.dp),
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
    light: Boolean = false,
    tag: String? = null,
) {
    val ink = when {
        selected -> Color.White
        light -> PhotoUi.PillField
        else -> PhotoUi.RailInk
    }
    Box(
        Modifier
            .size(38.dp)
            .clip(RoundedCornerShape(11.dp))
            .background(
                when {
                    selected -> PhotoUi.Accent
                    expanded -> PhotoUi.Accent.copy(alpha = .22f)
                    else -> Color.Transparent
                },
            )
            .clickable(role = Role.Button, onClick = onClick)
            .then(if (tag != null) Modifier.testTag(tag) else Modifier)
            .semantics { contentDescription = description },
        contentAlignment = Alignment.Center,
    ) {
        GlyphIcon(glyph, ink, size = 19.dp)
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
    Column(modifier.editorGlass(16.dp, material = PhotoUi.Card.copy(alpha = .95f)).padding(vertical = 8.dp)) {
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
            .background(if (selected) PhotoUi.Accent else Color.Transparent)
            .clickable(role = Role.Button, onClick = onClick)
            .testTag("photocraft_tool_${tool.name.lowercase()}")
            .padding(horizontal = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(9.dp),
    ) {
        GlyphIcon(tool.glyph, if (selected) Color.White else PhotoUi.RailInk, size = 17.dp)
        Text(
            tool.label,
            style = FcType.Caption,
            color = if (selected) Color.White else PhotoUi.RailInk,
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
    val base = if (glass) modifier.editorGlass(16.dp, material = PhotoUi.Card.copy(alpha = .95f)) else modifier
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
                    .background(if (tool == current) PhotoUi.Accent else Color.Transparent)
                    .clickable(role = Role.Button, onClick = { onTool(tool) })
                    .testTag("photocraft_tool_${tool.name.lowercase()}")
                    .semantics { contentDescription = tool.label },
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.Center,
            ) {
                GlyphIcon(tool.glyph, if (tool == current) Color.White else PhotoUi.RailInk, size = 18.dp)
                Spacer(Modifier.height(2.dp))
                Text(
                    tool.label,
                    style = FcType.ToolLabel.copy(fontSize = 9.sp),
                    color = if (tool == current) Color.White else PhotoUi.RailInkSoft,
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
    onLayers: () -> Unit,
    layersOpen: Boolean,
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
        modifier.verticalScroll(rememberScrollState()).padding(vertical = 6.dp, horizontal = 3.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        RailButton(
            LucideIcons.Layers.glyph(),
            "The layer stack",
            onLayers,
            selected = layersOpen,
            light = true,
            tag = "photocraft_layers",
        )
        Box(Modifier.width(22.dp).height(1.dp).background(PhotoUi.PillField))
        for (tool in quick) {
            RailButton(
                tool.glyph,
                tool.label,
                { onTool(tool) },
                selected = tool == current,
                light = true,
                tag = "photocraft_quick_${tool.name.lowercase()}",
            )
        }
    }
}

/**
 * The layer stack, drawn the way the reference draws it: a header with the blend mode and the
 * opacity, then the rows top-down — the photo locked at the bottom, paint layers with their masks,
 * adjustment layers between them — each row showing its eye, its mask chip and its name.
 */
@Composable
internal fun LayersPanel(doc: PhotoDocument, modifier: Modifier = Modifier) {
    val active = doc.active
    Column(modifier.editorGlass(18.dp, material = PhotoUi.Card.copy(alpha = .95f))) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 14.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("Layers", style = FcType.PanelTitle.copy(fontSize = 15.sp), color = PhotoUi.PillInk)
            Spacer(Modifier.weight(1f))
            GlyphButton(LucideIcons.FilePlus.glyph(), "New layer") { doc.addPaint() }
            GlyphButton(LucideIcons.Sun.glyph(), "Adjustment layer") {
                val layer = doc.addAdjustment(AdjustmentKind.BrightnessContrast)
                doc.select(layer)
            }
            GlyphButton(LucideIcons.Copy.glyph(), "Duplicate") { active?.let { if (!it.base) doc.duplicate(it) } }
            GlyphButton(LucideIcons.ChevronUp.glyph(), "Move up") { active?.let { doc.move(it, true) } }
            GlyphButton(LucideIcons.ChevronDown.glyph(), "Move down") { active?.let { doc.move(it, false) } }
            GlyphButton(LucideIcons.Trash2.glyph(), "Delete") { active?.let { if (!it.base) doc.remove(it) } }
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(PhotoUi.PillInk.copy(alpha = .1f)))
        active?.let { layer ->
            if (layer.adjustment != null) {
                AdjustmentRow(layer) { doc.refresh() }
            } else {
                Row(
                    Modifier.fillMaxWidth().padding(horizontal = 14.dp, vertical = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text("Opacity", style = FcType.Caption, color = PhotoUi.PillInkSoft)
                    Spacer(Modifier.weight(1f))
                    Text("${(layer.opacity * 100).roundToInt()}%", style = FcType.Caption, color = PhotoUi.PillInk)
                }
                FcSlider(
                    fraction = layer.opacity,
                    onChange = { layer.opacity = it; doc.refresh() },
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp),
                    description = "Layer opacity",
                )
                Row(
                    Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 6.dp).horizontalScroll(rememberScrollState()),
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    for (mode in LayerBlend.entries) {
                        val on = layer.blend == mode
                        Box(
                            Modifier
                                .clip(RoundedCornerShape(9.dp))
                                .background(if (on) PhotoUi.Accent else PhotoUi.PillField)
                                .clickable(role = Role.Button) { layer.blend = mode; doc.refresh() }
                                .padding(horizontal = 9.dp, vertical = 6.dp),
                        ) {
                            Text(mode.label, style = FcType.Caption, color = if (on) Color.White else PhotoUi.PillInk)
                        }
                    }
                }
                Row(
                    Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 4.dp),
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    SelectChip(
                        if (layer.maskActive) "Painting the mask" else if (layer.hasMask) "Mask on" else "Add mask",
                        layer.hasMask,
                        {
                            if (!layer.hasMask) {
                                layer.hasMask = true
                                layer.maskActive = true
                            } else {
                                layer.maskActive = !layer.maskActive
                            }
                            doc.refresh()
                        },
                    )
                    if (layer.maskActive) {
                        SelectChip("Back to pixels", false, { layer.maskActive = false })
                    }
                }
            }
        }
        for (layer in doc.layers.reversed()) {
            LayerRow(layer, layer.id == doc.activeId) { doc.select(layer) }
        }
        Spacer(Modifier.height(8.dp))
    }
}

/** An adjustment layer's two sliders, labelled with the adjustment's own terms. */
@Composable
private fun AdjustmentRow(layer: PhotoLayer, onChange: () -> Unit) {
    val kind = layer.adjustment ?: return
    Column(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 6.dp)) {
        Text(kind.label.uppercase(), style = FcType.ToolLabel.copy(fontSize = 9.sp), color = PhotoUi.PillInkSoft)
        Spacer(Modifier.height(2.dp))
        Text(kind.first, style = FcType.Caption, color = PhotoUi.PillInk)
        FcSlider(
            fraction = layer.amount,
            onChange = { layer.amount = it; onChange() },
            modifier = Modifier.fillMaxWidth(),
            description = kind.first,
        )
        Text(kind.second, style = FcType.Caption, color = PhotoUi.PillInk)
        FcSlider(
            fraction = layer.second,
            onChange = { layer.second = it; onChange() },
            modifier = Modifier.fillMaxWidth(),
            description = kind.second,
        )
    }
}

/** One row: eye, thumbnail, name, and the mask chip when the layer carries one. */
@Composable
private fun LayerRow(layer: PhotoLayer, selected: Boolean, onClick: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = 8.dp, vertical = 2.dp)
            .clip(RoundedCornerShape(10.dp))
            .background(if (selected) PhotoUi.Accent.copy(alpha = .22f) else Color.Transparent)
            .clickable(role = Role.Button, onClick = onClick)
            .testTag("photocraft_layer_${layer.id}")
            .padding(horizontal = 6.dp, vertical = 5.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Box(
            Modifier.size(30.dp).clip(RoundedCornerShape(7.dp)).background(PhotoUi.PillField)
                .clickable(role = Role.Button) { layer.visible = !layer.visible },
            contentAlignment = Alignment.Center,
        ) {
            GlyphIcon(
                LucideIcons.Eye.glyph(),
                if (layer.visible) PhotoUi.PillInk else PhotoUi.PillInkSoft.copy(alpha = .5f),
                size = 15.dp,
            )
        }
        Box(
            Modifier.size(30.dp).clip(RoundedCornerShape(7.dp)).background(PhotoUi.PillField),
            contentAlignment = Alignment.Center,
        ) {
            GlyphIcon(
                when {
                    layer.base -> LucideIcons.Image.glyph()
                    layer.adjustment != null -> LucideIcons.Sun.glyph()
                    else -> LucideIcons.Paintbrush.glyph()
                },
                PhotoUi.PillInkSoft,
                size = 15.dp,
            )
        }
        Column(Modifier.weight(1f)) {
            Text(layer.name, style = FcType.Caption, color = PhotoUi.PillInk, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(
                when {
                    layer.base -> "Background"
                    layer.adjustment != null -> "Adjustment"
                    layer.hasMask -> "Layer · mask"
                    else -> "Layer"
                },
                style = FcType.ToolLabel.copy(fontSize = 9.sp),
                color = PhotoUi.PillInkSoft,
            )
        }
        if (layer.hasMask) {
            Box(
                Modifier
                    .size(24.dp)
                    .clip(RoundedCornerShape(6.dp))
                    .background(if (layer.maskActive) PhotoUi.Accent else PhotoUi.PillField)
                    .clickable(role = Role.Button) {
                        layer.maskActive = !layer.maskActive
                        if (layer.maskActive) layer.hasMask = true
                    },
                contentAlignment = Alignment.Center,
            ) {
                GlyphIcon(LucideIcons.CircleDashed.glyph(), if (layer.maskActive) Color.White else PhotoUi.PillInkSoft, size = 12.dp)
            }
        }
    }
}

/** A small round icon button for the layer header. */
@Composable
private fun GlyphButton(glyph: Glyph, description: String, onClick: () -> Unit) {
    Box(
        Modifier
            .size(28.dp)
            .clip(RoundedCornerShape(8.dp))
            .clickable(role = Role.Button, onClick = onClick)
            .semantics { contentDescription = description },
        contentAlignment = Alignment.Center,
    ) {
        GlyphIcon(glyph, PhotoUi.PillInk, size = 15.dp)
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

/** Zoom and fit: the round handle the reference floats over the picture's bottom-left corner. */
@Composable
internal fun ZoomPill(view: PhotoViewState, modifier: Modifier = Modifier) {
    val fitted = view.zoom <= 1.001f
    Box(
        modifier
            .size(44.dp)
            .clip(CircleShape)
            .editorGlass(22.dp, material = PhotoUi.Card.copy(alpha = .9f))
            .clickable(role = Role.Button) { if (fitted) view.zoomTo(2f) else view.reset() }
            .semantics { contentDescription = if (fitted) "Zoom in" else "Fit the picture" },
        contentAlignment = Alignment.Center,
    ) {
        GlyphIcon(if (fitted) LucideIcons.ZoomIn.glyph() else LucideIcons.Scaling.glyph(), PhotoUi.RailInk, size = 19.dp)
    }
}

/** The ten swatches the paint tools draw with, dark to light and round the wheel. */
private val Swatches = listOf(
    Color(0xFF17171B),
    Color(0xFFF6F6F8),
    Color(0xFFE23B3B),
    Color(0xFFF08A24),
    Color(0xFFF2C511),
    Color(0xFF2FA84F),
    Color(0xFF1177FF),
    Color(0xFF7A3FF2),
    Color(0xFFE255A8),
    Color(0xFF22A9B8),
)

@Composable
private fun ColourRow(values: PhotoToolValues, second: Boolean) {
    Column(Modifier.fillMaxWidth().padding(horizontal = 14.dp, vertical = 5.dp)) {
        Text(if (second) "COLOUR · START" else "COLOUR", style = FcType.ToolLabel.copy(fontSize = 9.sp), color = Fc.Detail)
        Spacer(Modifier.height(4.dp))
        Row(
            Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()),
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            for (swatch in Swatches) {
                val chosen = if (second) values.endColor == swatch else values.color == swatch
                Box(
                    Modifier
                        .size(24.dp)
                        .clip(CircleShape)
                        .background(swatch)
                        .then(if (chosen) Modifier.border(2.dp, Color.White, CircleShape) else Modifier)
                        .clickable(role = Role.Button, onClick = { if (second) values.endColor = swatch else values.color = swatch }),
                )
            }
        }
    }
}

@Composable
private fun TypeRow(tool: PhotoTool, values: PhotoToolValues) {
    Column(Modifier.fillMaxWidth().padding(horizontal = 14.dp, vertical = 5.dp)) {
        Text("TEXT", style = FcType.ToolLabel.copy(fontSize = 9.sp), color = Fc.Detail)
        Spacer(Modifier.height(4.dp))
        Box(
            Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(10.dp))
                .background(Fc.Raised)
                .padding(horizontal = 10.dp, vertical = 9.dp),
        ) {
            BasicTextField(
                value = values.text(tool, "text", "Made with PhotoCraft"),
                onValueChange = { values.setText(tool, "text", it) },
                textStyle = FcType.Caption.copy(color = Fc.Title),
                cursorBrush = SolidColor(PhotoUi.Accent),
                modifier = Modifier.fillMaxWidth(),
            )
        }
    }
}

/** A tool's options: the actions the pipeline runs now, then its sliders, choices and switches. */
@Composable
internal fun ToolOptionsPanel(tool: PhotoTool, values: PhotoToolValues, onAction: (ToolAction) -> Unit) {
    val switches = tool.options.filterIsInstance<ToolOption.Switch>()
    val kind = tool.paintKind()
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(bottom = 10.dp)) {
        val inked = kind == PaintKind.Stroke || kind == PaintKind.Pen || kind == PaintKind.Shape ||
            kind == PaintKind.Text || kind == PaintKind.Gradient || kind == PaintKind.Bucket
        if (inked) ColourRow(values, second = false)
        if (kind == PaintKind.Selection) {
            Text(
                "Drag to select. Paint after this stays inside the outline; tap to clear it.",
                color = PhotoUi.PillInkSoft,
                style = FcType.Caption,
                modifier = Modifier.fillMaxWidth().padding(horizontal = 14.dp, vertical = 6.dp),
            )
        }
        if (kind == PaintKind.Retouch) {
            Text(
                when (retouchMode(tool)) {
                    RetouchMode.Heal -> "Paint over a blemish: the pixels under the brush are smoothed into their surroundings."
                    RetouchMode.Clone -> "Paint the copy: the brush takes what is two brush-widths to its left."
                    RetouchMode.Blur -> "Paint to blur the pixels under the brush."
                    RetouchMode.Sharpen -> "Paint to sharpen the edges under the brush."
                    RetouchMode.Smudge -> "Drag to drag the pixels along with the brush."
                    RetouchMode.Dodge -> "Paint to lighten."
                    RetouchMode.Burn -> "Paint to darken."
                    RetouchMode.Sponge -> "Paint to take the colour out."
                    RetouchMode.RedEye -> "Tap a pupil: red turns grey."
                    null -> ""
                },
                color = PhotoUi.PillInkSoft,
                style = FcType.Caption,
                modifier = Modifier.fillMaxWidth().padding(horizontal = 14.dp, vertical = 6.dp),
            )
        }
        if (kind == PaintKind.Navigate) {
            Text(
                if (tool == PhotoTool.Hand) "Drag the picture to move it. Double-tap to fit."
                else "Tap to zoom in; double-tap to fit.",
                color = PhotoUi.PillInkSoft,
                style = FcType.Caption,
                modifier = Modifier.fillMaxWidth().padding(horizontal = 14.dp, vertical = 6.dp),
            )
        }
        if (kind == PaintKind.Gradient) ColourRow(values, second = true)
        if (kind == PaintKind.Text) TypeRow(tool, values)
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
            "Tone, crop, rotate, flip, sampling, every brush and pen stroke, the shapes, type, gradients, the " +
                "paint bucket, the marquees and the retouch brushes act on this picture now, and they are in " +
                "the saved photo.",
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
