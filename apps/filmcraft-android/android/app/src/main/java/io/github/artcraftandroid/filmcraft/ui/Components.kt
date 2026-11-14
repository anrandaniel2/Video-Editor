package io.github.artcraftandroid.filmcraft.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.detectVerticalDragGestures
import androidx.compose.foundation.gestures.horizontalDrag
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.requiredWidth
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsBottomHeight
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.setProgress
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties

/** A round icon button (top bar, transport row): 40 dp target, 24 dp icon. */
@Composable
fun FcIconButton(
    glyph: Glyph,
    description: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    size: Dp = FcSize.TopButton,
    iconSize: Dp = 24.dp,
    tint: Color = Fc.Icon,
) {
    Box(
        modifier
            .size(size)
            .clip(CircleShape)
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            .semantics { contentDescription = description },
        contentAlignment = Alignment.Center,
    ) {
        GlyphIcon(glyph, if (enabled) tint else Fc.Disabled, size = iconSize)
    }
}

/** A tool on the bottom tool bar: an icon in a 40 dp chip over a short label, 80 dp square. */
@Composable
fun ToolTile(
    glyph: Glyph,
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    active: Boolean = false,
    tag: String? = null,
) {
    Column(
        modifier
            .size(FcSize.ToolTile)
            .clip(RoundedCornerShape(8.dp))
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            .then(if (tag != null) Modifier.testTag(tag) else Modifier)
            .semantics { selected = active },
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        Box(
            Modifier.size(FcSize.ToolChip).clip(RoundedCornerShape(4.dp)).background(
                when {
                    active -> Fc.Accent.copy(alpha = .15f)
                    else -> Color.Transparent
                },
            ),
            contentAlignment = Alignment.Center,
        ) {
            GlyphIcon(glyph, if (!enabled) Fc.Disabled else if (active) Fc.Accent else Fc.Icon)
        }
        Spacer(Modifier.height(0.5.dp))
        Text(
            label,
            style = FcType.ToolLabel,
            color = if (!enabled) Fc.Disabled else if (active) Fc.Accent else Fc.Body,
            textAlign = TextAlign.Center,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/** A small pill action on a panel header (Apply to all, Revert): 40 × 32 dp. */
@Composable
fun PanelAction(glyph: Glyph, description: String, onClick: () -> Unit, enabled: Boolean = true, tag: String? = null) {
    Box(
        Modifier
            .size(width = 40.dp, height = 32.dp)
            .clip(CircleShape)
            // disabled: no pill, a dim icon
            .then(if (enabled) Modifier.editorCard(20.dp) else Modifier)
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            .then(if (tag != null) Modifier.testTag(tag) else Modifier)
            .semantics { contentDescription = description },
        contentAlignment = Alignment.Center,
    ) {
        GlyphIcon(glyph, if (enabled) Fc.Body else Fc.Disabled, size = 20.dp)
    }
}

/**
 * A tool's bottom panel: a dark sheet inset from the screen's sides with rounded top corners, a
 * drag handle (drag it down to close), a title row with optional actions, then the content. It
 * reaches under the navigation buttons, which it paints in its own darker band.
 */
@Composable
fun ToolPanel(
    title: String?,
    onClose: () -> Unit,
    modifier: Modifier = Modifier,
    actions: (@Composable RowScope.() -> Unit)? = null,
    tag: String? = null,
    scrollContent: Boolean = true,
    content: @Composable ColumnScope.() -> Unit,
) {
    val close by rememberUpdatedState(onClose)
    var drag by remember { mutableFloatStateOf(0f) }
    Column(
        modifier
            .fillMaxWidth()
            .padding(horizontal = FcSize.PanelInset, vertical = 8.dp)
            .clip(RoundedCornerShape(FcSize.PanelCorner))
            .editorGlass(FcSize.PanelCorner)
            .then(if (tag != null) Modifier.testTag(tag) else Modifier),
    ) {
        // the handle and the title row drag the panel down to close it
        Column(
            Modifier.fillMaxWidth().pointerInput(Unit) {
                detectVerticalDragGestures(
                    onDragStart = { drag = 0f },
                    onDragEnd = { if (drag > 48.dp.toPx()) close() },
                    onVerticalDrag = { _, dy -> drag += dy },
                )
            },
        ) {
            if (title != null) {
                Row(
                    Modifier.fillMaxWidth().padding(start = FcSize.PanelPadding, end = FcSize.PanelPadding, top = 12.dp, bottom = 4.dp).height(44.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    val toolIcon = when {
                        title.contains("3D", true) -> Glyphs.Object3D
                        title.contains("keyframe", true) -> LucideIcons.ChartSpline.glyph()
                        title.contains("speed", true) -> LucideIcons.Gauge.glyph()
                        title.contains("volume", true) || title.contains("audio", true) -> LucideIcons.Volume2.glyph()
                        title.contains("light", true) -> LucideIcons.Sun.glyph()
                        else -> LucideIcons.WandSparkles.glyph()
                    }
                    Box(Modifier.size(40.dp).clip(RoundedCornerShape(14.dp)).editorCard(18.dp), contentAlignment = Alignment.Center) { GlyphIcon(toolIcon, Fc.Icon, size = 22.dp) }
                    Spacer(Modifier.width(12.dp))
                    Text(
                        title,
                        style = FcType.GalleryTitle.copy(fontSize = 23.sp),
                        color = Fc.Title,
                        modifier = Modifier.weight(1f),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    if (actions != null) Row(horizontalArrangement = Arrangement.spacedBy(16.dp), verticalAlignment = Alignment.CenterVertically, content = actions)
                    FcIconButton(LucideIcons.X.glyph(), "Close panel", { close() })
                }
            }
        }
        // the content scrolls when the room is short (a phone held sideways)
        Column(Modifier.weight(1f, fill = false).then(if (scrollContent) Modifier.verticalScroll(rememberScrollState()) else Modifier), verticalArrangement = Arrangement.spacedBy(8.dp)) { content() }
        Spacer(Modifier.height(12.dp))
        Spacer(Modifier.fillMaxWidth().windowInsetsBottomHeight(WindowInsets.navigationBars).background(Color.Black.copy(alpha = .12f)))
    }
}

/** How a slider's track is drawn. */
enum class SliderLook { Standard, Opacity }

/**
 * FilmCraft's slider: a 4 dp track filled up to a 20 dp ringed thumb. A press anywhere moves the
 * thumb there; dragging follows the finger. [fraction] is 0..1; [onChange] gets the new fraction
 * while dragging, [onDone] when the finger lifts. [ticks] draws marks along the track.
 */
@Composable
fun FcSlider(
    fraction: Float,
    onChange: (Float) -> Unit,
    onDone: () -> Unit = {},
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    look: SliderLook = SliderLook.Standard,
    ticks: List<Float> = emptyList(),
    snapTo: Float? = null,
    description: String = "",
    tag: String? = null,
    numericRange: ClosedFloatingPointRange<Float> = 0f..100f,
    numberFromFraction: ((Float) -> Double)? = null,
    fractionFromNumber: ((Double) -> Float)? = null,
) {
    val safe = fraction.takeIf { it.isFinite() }?.coerceIn(0f, 1f) ?: 0f
    val row = LocalSliderRow.current
    val min = numericRange.start.toDouble(); val max = numericRange.endInclusive.toDouble()
    val change by rememberUpdatedState(onChange)
    val done by rememberUpdatedState(onDone)
    val number = numberFromFraction?.invoke(safe) ?: (min + safe * (max - min))
    val shown = row?.value?.takeIf { it.isNotBlank() } ?: ("%.3f".format(java.util.Locale.ROOT, number).trimEnd('0').trimEnd('.'))
    Box(modifier.then(if (tag != null) Modifier.testTag(tag) else Modifier).semantics {
        contentDescription = description
        progressBarRangeInfo = ProgressBarRangeInfo(safe, 0f..1f)
        if (enabled) setProgress { v -> change(v.coerceIn(0f, 1f)); done(); true }
    }) {
        io.github.artcraftandroid.procraft.ui.StudioSlider(
            label = row?.label ?: description.ifBlank { "Value" }, value = safe,
            shown = io.github.artcraftandroid.procraft.ui.Shown.PERCENT, enabled = enabled,
            valueText = shown, onLabelClick = row?.onLabelClick,
            numberSpec = io.github.artcraftandroid.procraft.ui.StudioNumberSpec(min, max, toFraction = {
                fractionFromNumber?.invoke(it) ?: ((it - min) / (max - min)).toFloat()
            }), onDone = { done() },
        ) { f ->
            val snap = snapTo
            change(if (snap != null && kotlin.math.abs(f - snap) < .015f) snap else f)
        }
    }
}

/** A selectable chip (export settings): outlined, filled with a check when chosen. */
@Composable
fun SelectChip(label: String, selected: Boolean, onClick: () -> Unit, tag: String? = null) {
    Row(
        Modifier
            .height(40.dp)
            .clip(CircleShape)
            .background(if (selected) Fc.Accent else Fc.Raised)
            .clickable(role = Role.RadioButton, onClick = onClick)
            .then(if (tag != null) Modifier.testTag(tag) else Modifier)
            .semantics { this.selected = selected }
            .padding(horizontal = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (selected) {
            GlyphIcon(LucideIcons.Check.glyph(), Fc.Title, size = 18.dp)
            Spacer(Modifier.width(8.dp))
        }
        Text(label, style = FcType.Label.copy(fontSize = 14.sp), color = if (selected) Fc.Title else Fc.Body)
    }
}

/** A target chip with a check when chosen (text colour: Text | Background). */
@Composable
fun ToggleChip(label: String, selected: Boolean, onClick: () -> Unit) = SelectChip(label, selected, onClick)

/** The main call to action: a full-width blue pill. */
@Composable
fun PrimaryButton(text: String, onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true, tag: String? = null) {
    Box(
        modifier
            .fillMaxWidth()
            .height(56.dp)
            .clip(RoundedCornerShape(14.dp))
            .background(if (enabled) Fc.Accent else Fc.Accent.copy(alpha = 0.4f))
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            .then(if (tag != null) Modifier.testTag(tag) else Modifier),
        contentAlignment = Alignment.Center,
    ) {
        Text(text, style = FcType.Button, color = if (enabled) Color.White else Color.White.copy(alpha = 0.4f))
    }
}

/** A plain text button in a dialog. */
@Composable
fun DialogButton(text: String, onClick: () -> Unit, filled: Boolean, destructive: Boolean = false, enabled: Boolean = true) {
    Box(
        Modifier
            .height(40.dp)
            .widthIn(min = 80.dp)
            .clip(CircleShape)
            .then(
                if (filled) Modifier.background((if (destructive) Fc.Negative else Fc.Accent).copy(alpha = if (enabled) 1f else 0.4f))
                else Modifier.border(2.dp, Fc.Divider, CircleShape),
            )
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            .padding(horizontal = 18.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(text, style = FcType.Button, color = Color.White.copy(alpha = if (enabled) 1f else 0.4f))
    }
}

/** A dialog: 28 dp corners, a title, optional content and Cancel / OK buttons. */
@Composable
fun FcDialog(
    title: String,
    confirm: String,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
    dismiss: String = "Cancel",
    destructive: Boolean = false,
    confirmEnabled: Boolean = true,
    content: @Composable ColumnScope.() -> Unit = {},
) {
    Dialog(onDismissRequest = onDismiss, properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false)) {
        ImmersiveDialog()
        Column(
            Modifier
                .padding(horizontal = 32.dp)
                .widthIn(max = 480.dp)
                .fillMaxWidth()
                .editorGlass(FcSize.PanelCorner)
                .padding(24.dp),
        ) {
            Text(title, style = FcType.DialogTitle, color = Color.White)
            Spacer(Modifier.height(16.dp))
            content()
            Spacer(Modifier.height(24.dp))
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp, Alignment.End)) {
                DialogButton(dismiss, onDismiss, filled = false)
                DialogButton(confirm, onConfirm, filled = true, destructive = destructive, enabled = confirmEnabled)
            }
        }
    }
}

/**
 * A preset tile (filters, effects, transitions): an 84 dp square with a 76 dp picture, a white
 * ring when chosen, and its name under it.
 */
@Composable
fun PresetTile(name: String, selected: Boolean, onClick: () -> Unit, tag: String? = null, picture: @Composable BoxScope.() -> Unit) {
    Column(
        Modifier
            .width(84.dp)
            .clickable(role = Role.RadioButton, onClick = onClick)
            .then(if (tag != null) Modifier.testTag(tag) else Modifier)
            .semantics { this.selected = selected },
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Box(
            Modifier
                .size(84.dp)
                .then(if (selected) Modifier.border(2.dp, Color.White, RoundedCornerShape(16.dp)) else Modifier)
                .padding(4.dp)
                .clip(RoundedCornerShape(12.dp))
                .background(Fc.SliderOff),
            contentAlignment = Alignment.Center,
            content = picture,
        )
        Spacer(Modifier.height(4.dp))
        Text(
            name,
            style = FcType.TileLabel,
            color = if (selected) Fc.Title else Fc.Title.copy(alpha = 0.56f),
            textAlign = TextAlign.Center,
            maxLines = 1,
            softWrap = false,
            overflow = TextOverflow.Ellipsis,
            // one line, allowed past the tile's edges into the gap between tiles
            modifier = Modifier.requiredWidth(96.dp),
        )
    }
}

/** A labelled slider row: name and value over the slider. */
@Composable
fun SliderRow(label: String, value: String, onLabelClick: (() -> Unit)? = null, selected: Boolean = false, content: @Composable () -> Unit) {
    Column(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 3.dp)
        .editorCard(18.dp).then(if (selected) Modifier.border(1.dp, Fc.Accent, RoundedCornerShape(18.dp)) else Modifier).padding(horizontal = 4.dp, vertical = 6.dp)) {
        androidx.compose.runtime.CompositionLocalProvider(LocalSliderRow provides SliderRowInfo(label, value, onLabelClick)) { content() }
    }
}

private data class SliderRowInfo(val label: String, val value: String, val onLabelClick: (() -> Unit)?)
private val LocalSliderRow = androidx.compose.runtime.staticCompositionLocalOf<SliderRowInfo?> { null }

/** Shared by the video editor and drawing workspace so export controls remain identical. */
@Composable
fun EditorExportButton(onClick: () -> Unit, enabled: Boolean = true, modifier: Modifier = Modifier) {
    Box(modifier.padding(end = 12.dp).clip(RoundedCornerShape(15.dp))
        .background(if (enabled) Fc.Accent else Fc.Raised)
        .clickable(enabled = enabled, role = Role.Button, onClick = onClick)
        .padding(horizontal = 14.dp, vertical = 6.dp)) {
        Text("Export", style = FcType.Button, color = Color.White)
    }
}
