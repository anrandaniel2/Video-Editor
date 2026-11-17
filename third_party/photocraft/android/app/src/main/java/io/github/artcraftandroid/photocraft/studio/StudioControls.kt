package io.github.artcraftandroid.photocraft.studio

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * The reference's controls, one for one: its icon button (30 px, 7 px radius, blue when active), its
 * pill number field (26 px, `#20211f`), its select field, its panel frame (17 px radius, the
 * `#30312ffb → #262725fb` gradient) and its sliders.
 */

/** `.icon-button`: 30×30, or 26×26 small; blue when [active]. */
@Composable
fun IconButton(
    icon: String,
    label: String,
    modifier: Modifier = Modifier,
    active: Boolean = false,
    small: Boolean = false,
    iconSize: Dp? = null,
    tint: Color? = null,
    onClick: (() -> Unit)? = null,
) {
    val box = if (small) 26.dp else 30.dp
    Box(
        (if (onClick != null) modifier.clickable(onClick = onClick) else modifier)
            .size(box)
            .clip(RoundedCornerShape(if (small) 5.dp else 7.dp))
            .background(if (active) Sc.Blue else Color.Transparent),
        contentAlignment = Alignment.Center,
    ) {
        StudioIcons.Glyph(
            icon,
            tint ?: if (active) Color.White else Sc.IconInk,
            iconSize ?: if (small) 16.dp else 19.dp,
        )
    }
}

/** `.number-field`: a pill with the value centred and an optional suffix. */
@Composable
fun NumberField(
    value: String,
    label: String,
    modifier: Modifier = Modifier,
    suffix: String = "",
    minWidth: Dp = 52.dp,
    min: Int = 0,
    max: Int = 100,
    onChange: (String) -> Unit,
) {
    Row(
        modifier.height(26.dp).width(minWidth).clip(RoundedCornerShape(13.dp)).background(Sc.Field)
            .padding(horizontal = 7.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.Center,
    ) {
        BasicTextField(
            value = value,
            onValueChange = onChange,
            singleLine = true,
            textStyle = TextStyle(fontSize = 11.sp, color = Sc.Title, textAlign = TextAlign.Center),
            cursorBrush = SolidColor(Sc.Blue),
            modifier = Modifier.width(32.dp),
        )
        if (suffix.isNotEmpty()) Text(suffix, fontSize = 10.sp, color = Sc.FieldInk, modifier = Modifier.padding(start = 3.dp))
    }
}

/** `.select-field`: the pill with the chevron, opened as a menu. */
@Composable
fun SelectField(
    label: String,
    value: String,
    options: List<String>,
    modifier: Modifier = Modifier,
    height: Dp = 26.dp,
    fontSize: Int = 11,
    onSelect: (String) -> Unit,
) {
    var open by remember { mutableStateOf(false) }
    Box(modifier) {
        Row(
            Modifier.fillMaxWidth().height(height).clip(RoundedCornerShape(height / 2)).background(Sc.Field)
                .clickable { open = true }
                .padding(start = 12.dp, end = 23.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(value, fontSize = fontSize.sp, color = Sc.Title, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
        }
        Box(Modifier.align(Alignment.CenterEnd).padding(end = 7.dp)) {
            StudioIcons.Glyph("down", Sc.FieldIcon, 12.dp)
        }
        if (open) {
            // The menu the reference's native select opens: its own options, on the chrome.
            Box(Modifier.fillMaxWidth().heightIn(min = 1.dp))
            androidx.compose.ui.window.Popup(
                alignment = Alignment.TopStart,
                onDismissRequest = { open = false },
            ) {
                Column(
                    Modifier.width(150.dp).clip(RoundedCornerShape(10.dp)).background(Sc.surfaceBrush)
                        .border(1.dp, Sc.Hairline, RoundedCornerShape(10.dp)).padding(4.dp),
                ) {
                    options.forEach { option ->
                        Row(
                            Modifier.fillMaxWidth().height(26.dp).clip(RoundedCornerShape(6.dp))
                                .clickable { onSelect(option); open = false }
                                .padding(horizontal = 8.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Text(
                                option,
                                fontSize = 11.sp,
                                color = if (option == value) Sc.BlueLight else Sc.Title,
                                maxLines = 1,
                            )
                        }
                    }
                }
            }
        }
    }
}

/** `.panel-header` with its grip row, pin and close; the body is the panel's own. */
@Composable
fun PanelFrame(
    name: String,
    modifier: Modifier = Modifier,
    pinned: Boolean = false,
    actions: (@Composable () -> Unit)? = null,
    onPin: () -> Unit,
    onClose: () -> Unit,
    body: @Composable () -> Unit,
) {
    // The grip drags the panel, as the reference's does; arrow keys nudge it.
    var dx by remember { mutableStateOf(0f) }
    var dy by remember { mutableStateOf(0f) }
    Column(
        // The panel takes the width its caller gives it (the reference's 238, or 266 for Curves).
        // Its contents may hang outside it, as the reference's curve editor does with its negative
        // margin, so the frame is not clipped: the rounded surface is drawn inside it instead.
        modifier
            .offset { androidx.compose.ui.unit.IntOffset(dx.toInt(), dy.toInt()) }
            .background(Sc.surfaceBrush, RoundedCornerShape(Sc.PanelRadius))
            .border(1.dp, Sc.Hairline, RoundedCornerShape(Sc.PanelRadius)),
    ) {
        Box(Modifier.fillMaxWidth().height(17.dp), contentAlignment = Alignment.Center) {
            Box(
                Modifier.width(70.dp).height(17.dp).clip(RoundedCornerShape(6.dp))
                    .pointerInput(Unit) {
                        detectDragGestures { change, drag ->
                            dx += drag.x
                            dy += drag.y
                            change.consume()
                        }
                    },
                contentAlignment = Alignment.Center,
            ) {
                Box(Modifier.width(38.dp).height(5.dp).clip(RoundedCornerShape(5.dp)).background(Sc.Grip))
            }
        }
        Row(
            Modifier.fillMaxWidth().height(34.dp).padding(start = 12.dp, end = 9.dp, bottom = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                if (name == "Curves") "Curves Adjustment" else name,
                fontSize = 12.sp,
                color = Sc.Title,
                fontWeight = FontWeight.Medium,
                modifier = Modifier.weight(1f),
                maxLines = 1,
            )
            actions?.invoke()
            HeaderButton("pin", "Pin $name panel", active = pinned, onClick = onPin)
            Spacer(Modifier.width(1.dp))
            HeaderButton("close", "Close $name panel", onClick = onClose)
        }
        PanelDivider()
        body()
    }
}

/** A header action: 23×23, tinted light blue when active, as `.panel-header-actions` draws it. */
@Composable
private fun HeaderButton(icon: String, label: String, active: Boolean = false, onClick: () -> Unit) {
    Box(
        Modifier.size(23.dp).clip(RoundedCornerShape(5.dp))
            .background(if (active) Color(0x17FFFFFF) else Color.Transparent)
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        StudioIcons.Glyph(icon, if (active) Sc.BlueLight else Sc.IconInk, 14.dp)
    }
}

/** `.panel-toolbar`: a row of small actions, spaced around or from the start. */
@Composable
fun PanelToolbar(modifier: Modifier = Modifier, spaceAround: Boolean = true, content: @Composable androidx.compose.foundation.layout.RowScope.() -> Unit) {
    Row(
        modifier.fillMaxWidth().heightIn(min = 36.dp).padding(horizontal = 7.dp, vertical = 3.dp),
        horizontalArrangement = if (spaceAround) Arrangement.SpaceAround else Arrangement.spacedBy(15.dp),
        verticalAlignment = Alignment.CenterVertically,
        content = content,
    )
}

/** `.panel-note`: the grey caption under a panel's controls. */
@Composable
fun PanelNote(text: String, modifier: Modifier = Modifier) {
    Text(
        text,
        fontSize = 10.sp,
        lineHeight = 16.sp,
        color = Sc.Note,
        modifier = modifier.fillMaxWidth().padding(start = 12.dp, end = 12.dp, top = 10.dp, bottom = 12.dp),
    )
}

/** `.standard-button` / `.primary-button`. */
@Composable
fun StandardButton(label: String, modifier: Modifier = Modifier, primary: Boolean = false, onClick: () -> Unit) {
    Box(
        modifier.fillMaxWidth().heightIn(min = 34.dp).clip(RoundedCornerShape(7.dp))
            .background(if (primary) Sc.ButtonPrimary else Color(0x0BFFFFFF))
            .border(1.dp, if (primary) Sc.ButtonPrimaryEdge else Color(0x15FFFFFF), RoundedCornerShape(7.dp))
            .clickable(onClick = onClick)
            .padding(horizontal = 13.dp, vertical = 8.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(label, fontSize = 11.sp, color = if (primary) Color.White else Color(0xFFDDDFD7), maxLines = 1)
    }
}

/** A slider in the reference's accent blue, with its own thumb. */
@Composable
fun StudioSlider(
    fraction: Float,
    modifier: Modifier = Modifier,
    accent: Color = Sc.Blue,
    onFraction: (Float) -> Unit,
) {
    Box(
        modifier.height(16.dp).pointerInput(Unit) {
            detectDragGestures(
                onDragStart = { onFraction((it.x / size.width).coerceIn(0f, 1f)) },
            ) { change, _ ->
                onFraction((change.position.x / size.width).coerceIn(0f, 1f))
                change.consume()
            }
        }.pointerInput(Unit) {
            detectTapGestures { onFraction((it.x / size.width).coerceIn(0f, 1f)) }
        },
    ) {
        Canvas(Modifier.fillMaxWidth().height(16.dp)) {
            val cy = size.height / 2f
            val track = 3.dp.toPx()
            drawRoundRect(
                Sc.SliderTrack,
                Offset(0f, cy - track / 2f),
                Size(size.width, track),
                androidx.compose.ui.geometry.CornerRadius(track / 2f),
            )
            val x = fraction.coerceIn(0f, 1f) * size.width
            drawRoundRect(
                accent,
                Offset(0f, cy - track / 2f),
                Size(x, track),
                androidx.compose.ui.geometry.CornerRadius(track / 2f),
            )
            drawCircle(Color(0xFFE9EDEA), 6.dp.toPx(), Offset(x, cy))
        }
    }
}

/**
 * A slider laid out the way Procraft's brush controls are: the name at the left, its value in a
 * rounded pill at the right, and the track underneath.
 *
 * The pill is the point of it. Procraft lets you tap the value to type a number instead of
 * dragging, which is how you set an exact 37px brush; a track alone cannot express that. Tapping
 * here focuses a small field the same way.
 */
@Composable
fun StudioValueSlider(
    label: String,
    fraction: Float,
    valueText: String,
    modifier: Modifier = Modifier,
    accent: Color = Sc.Blue,
    onFraction: (Float) -> Unit,
    onValue: ((String) -> Unit)? = null,
) {
    var typing by remember { mutableStateOf(false) }
    var typed by remember { mutableStateOf(valueText) }
    Column(modifier.fillMaxWidth()) {
        Row(
            Modifier.fillMaxWidth().height(26.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Text(label, fontSize = 11.sp, color = Sc.Title, maxLines = 1)
            if (typing && onValue != null) {
                androidx.compose.foundation.text.BasicTextField(
                    value = typed,
                    onValueChange = { typed = it },
                    singleLine = true,
                    textStyle = androidx.compose.ui.text.TextStyle(
                        fontSize = 11.sp,
                        color = Color.White,
                        textAlign = androidx.compose.ui.text.style.TextAlign.Center,
                    ),
                    modifier = Modifier
                        .width(73.dp).height(24.dp)
                        .clip(RoundedCornerShape(12.dp)).background(accent)
                        .padding(horizontal = 10.dp),
                )
            } else {
                Box(
                    Modifier.height(24.dp).widthIn(min = 73.dp)
                        .clip(RoundedCornerShape(12.dp)).background(Color(0xFF2A2C28))
                        .then(if (onValue != null) Modifier.clickable { typed = valueText; typing = true } else Modifier)
                        .padding(horizontal = 13.dp),
                    contentAlignment = Alignment.Center,
                ) {
                    Text(valueText, fontSize = 11.sp, color = Color(0xFFE9EDEA), maxLines = 1)
                }
            }
        }
        StudioSlider(fraction = fraction, accent = accent) { f ->
            // Leaving the field open while dragging would show a stale number.
            if (typing) {
                typing = false
                onValue?.invoke(typed)
            }
            onFraction(f)
        }
    }
}

/** A labelled row: the reference's `.field-row` (label left, control right). */
@Composable
fun FieldRow(label: String, modifier: Modifier = Modifier, trailing: @Composable () -> Unit) {
    Row(
        modifier.fillMaxWidth().heightIn(min = 26.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        Text(label, fontSize = 11.sp, color = Sc.Title, maxLines = 1)
        trailing()
    }
}

/** A checkbox row: the reference's `.effect-row`. */
@Composable
fun CheckRow(label: String, checked: Boolean, onChange: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 32.dp).clickable(onClick = onChange)
            .padding(bottom = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        Text(label, fontSize = 11.sp, color = Sc.Title, maxLines = 1)
        Box(
            Modifier.size(15.dp).clip(RoundedCornerShape(3.dp))
                .background(if (checked) Sc.Blue else Sc.Field)
                .border(1.dp, if (checked) Sc.Blue else Color(0x33FFFFFF), RoundedCornerShape(3.dp)),
            contentAlignment = Alignment.Center,
        ) {
            if (checked) StudioIcons.Glyph("check", Color.White, 10.dp)
        }
    }
}

/** `.disclosure-row`: a full-width row that expands a section. */
@Composable
fun DisclosureRow(label: String, expanded: Boolean, onToggle: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 35.dp).clickable(onClick = onToggle)
            .padding(horizontal = 12.dp, vertical = 11.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        Text(label, fontSize = 11.sp, color = Sc.Title, maxLines = 1)
        StudioIcons.Glyph("chevron", Sc.IconInk, 14.dp, Modifier.rotate(if (expanded) 90f else 0f))
    }
}

/** `.expanded-section`: the sunken block a disclosure opens onto. */
@Composable
fun ExpandedSection(main: String, detail: String) {
    Column(Modifier.fillMaxWidth().background(Sc.Field).padding(horizontal = 12.dp, vertical = 10.dp)) {
        Text(main, fontSize = 10.sp, lineHeight = 18.sp, color = Sc.Title)
        Text(detail, fontSize = 10.sp, lineHeight = 18.sp, color = Sc.Note)
    }
}

/** `.standard-button` width rule inside `.secondary-content`. */
@Composable
fun SecondaryContent(modifier: Modifier = Modifier, content: @Composable androidx.compose.foundation.layout.ColumnScope.() -> Unit) {
    Column(modifier.fillMaxWidth().padding(12.dp), verticalArrangement = Arrangement.spacedBy(10.dp), content = content)
}

/** `.panel-footer`: the row of small grey facts under a list. */
@Composable
fun PanelFooter(start: String, end: String) {
    Row(
        Modifier.fillMaxWidth().padding(horizontal = 11.dp, vertical = 8.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        Text(start, fontSize = 9.sp, color = Sc.Footer)
        Text(end, fontSize = 9.sp, color = Sc.Footer)
    }
}

/** `.panel-divider`: the hairline under a header. */
@Composable
fun PanelDivider(modifier: Modifier = Modifier) {
    Box(modifier.fillMaxWidth().height(1.dp).background(Color(0x70141513)))
}

/** A tiny inline spacer. */
@Composable
fun VSpace(dp: Dp) = Spacer(Modifier.height(dp))

/** `#rgb` / `#rrggbb` / `#aarrggbb` as a Compose colour. */
fun parseColour(hex: String): Color {
    val s = hex.trim().removePrefix("#")
    return runCatching {
        when (s.length) {
            3 -> Color(
                (0xFF shl 24) or
                    ((s[0].digitToInt(16) * 17) shl 16) or
                    ((s[1].digitToInt(16) * 17) shl 8) or
                    (s[2].digitToInt(16) * 17),
            )
            6 -> Color(0xFF000000L.toInt() or s.toLong(16).toInt())
            8 -> Color(s.toLong(16).toInt())
            else -> Sc.IconInk
        }
    }.getOrDefault(Sc.IconInk)
}

/** `.colour-wheel`: the conic wheel with the chosen colour in its middle. */
@Composable
fun ColourWheel(colour: Color, modifier: Modifier = Modifier) {
    Box(modifier.size(185.dp), contentAlignment = Alignment.Center) {
        Canvas(Modifier.size(185.dp)) {
            val hues = listOf(
                Color.Red, Color.Yellow, Color.Green, Color.Cyan, Color.Blue, Color.Magenta, Color.Red,
            )
            val r = size.minDimension / 2f
            for (i in 0 until 360) {
                val a0 = i.toFloat()
                val c = lerpHue(hues, i / 360f)
                drawArc(
                    color = c,
                    startAngle = a0 - 90f,
                    sweepAngle = 1.8f,
                    useCenter = true,
                    topLeft = Offset(0f, 0f),
                    size = Size(size.width, size.height),
                )
            }
            drawCircle(Color(0x33000000), r, Offset(r, r), style = Stroke(1f))
        }
        // The chosen colour, inset in the dark ring the reference draws.
        Box(
            Modifier.size(126.dp).clip(RoundedCornerShape(63.dp)).background(colour)
                .border(12.dp, Color(0xFF2A2C27), RoundedCornerShape(63.dp)),
        )
    }
}

private fun lerpHue(hues: List<Color>, t: Float): Color {
    val scaled = (t.coerceIn(0f, 0.9999f)) * (hues.size - 1)
    val i = scaled.toInt()
    val f = scaled - i
    val a = hues[i]
    val b = hues[i + 1]
    return Color(
        red = a.red + (b.red - a.red) * f,
        green = a.green + (b.green - a.green) * f,
        blue = a.blue + (b.blue - a.blue) * f,
        alpha = 1f,
    )
}

/** `.mini-grip`: the handle at the top of the node toolbar and the quick controls. */
@Composable
fun MiniGrip(width: Dp = 38.dp, height: Dp = 5.dp) {
    Box(Modifier.width(width).height(height).clip(RoundedCornerShape(5.dp)).background(Sc.Grip))
}
