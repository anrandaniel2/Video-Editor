package io.github.artcraftandroid.filmcraft.ui.editor.photo

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlin.math.roundToInt

/**
 * The panels, each anchored to the rail that opened it.
 *
 * They are 264 dp wide — the reference's panel width, and narrow enough to sit beside the rail on a
 * phone rather than covering the picture it belongs to. Their content scrolls, so a tall
 * adjustments stack does not run off the bottom on a short screen.
 */
@Composable
internal fun PhotoPanel(session: PhotoSession, kind: PhotoPanelKind, modifier: Modifier) {
    Column(
        modifier
            .width(PanelWidth)
            .heightIn(max = 520.dp)
            .clip(RoundedCornerShape(12.dp))
            .background(Color(0xFF282926))
            .padding(vertical = 8.dp),
    ) {
        when (kind) {
            PhotoPanelKind.Adjust, PhotoPanelKind.Curves -> AdjustPanel(session)
            PhotoPanelKind.Colour -> ColourPanel(session)
            PhotoPanelKind.Brush -> BrushPanel(session)
            PhotoPanelKind.Layers -> LayersPanel(session)
        }
    }
}

private val PanelWidth = 264.dp
private val PanelHeader = Color(0xFFE8EAE0)
private val PanelBody = Color(0xFFB9BEAF)
private val PanelMuted = Color(0xFF7E8478)
private val RowSelected = Color(0x33328FFF)

@Composable
private fun PanelTitle(text: String) {
    Text(
        text,
        color = PanelHeader,
        fontSize = 12.sp,
        fontWeight = FontWeight.SemiBold,
        modifier = Modifier.padding(start = 14.dp, end = 14.dp, top = 4.dp, bottom = 6.dp),
    )
}

/**
 * The adjustments.
 *
 * A slider drags the *preview*; lifting the finger commits the whole stack to Rust as one step, so
 * one undo takes back one adjustment rather than one per pixel of thumb travel.
 */
@Composable
private fun AdjustPanel(session: PhotoSession) {
    PanelTitle("Adjust")
    Column(Modifier.verticalScroll(rememberScrollState()).padding(horizontal = 14.dp)) {
        Adjustment.entries.forEach { adjustment ->
            val value = session.values[adjustment] ?: adjustment.neutral
            val active = adjustment == session.activeAdjustment
            Column(Modifier.fillMaxWidth().padding(vertical = 3.dp)) {
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        adjustment.label,
                        color = if (active) Color.White else PanelBody,
                        fontSize = 11.sp,
                        modifier = Modifier.weight(1f).clickable { session.selectAdjustment(adjustment) },
                    )
                    Text(shows(adjustment, value), color = PanelMuted, fontSize = 10.sp)
                }
                PhotoSlider(
                    position = adjustment.fraction(value),
                    modifier = Modifier.fillMaxWidth(),
                    onChange = {
                        session.selectAdjustment(adjustment)
                        session.setAdjustment(adjustment.fromFraction(it))
                    },
                    onRelease = { session.commitAdjustments() },
                )
            }
        }
        Spacer(Modifier.height(8.dp))
        Text(
            "Changes apply to the whole picture. Undo takes back one adjustment.",
            color = PanelMuted,
            fontSize = 10.sp,
            modifier = Modifier.padding(bottom = 8.dp),
        )
    }
}

/** How an adjustment prints: gains as percentages, the rest as plain amounts. */
private fun shows(adjustment: Adjustment, value: Float): String = when (adjustment) {
    Adjustment.Contrast, Adjustment.Saturation, Adjustment.Gamma,
    Adjustment.Temperature, Adjustment.Tint -> "${(value * 100).roundToInt()}%"
    Adjustment.Hue -> "${value.roundToInt()}°"
    Adjustment.Exposure -> if (value >= 0f) "+${round2(value)}" else round2(value)
    else -> round2(value)
}

private fun round2(value: Float): String = "${(value * 100).roundToInt() / 100f}"

@Composable
private fun ColourPanel(session: PhotoSession) {
    PanelTitle("Colour")
    Column(Modifier.verticalScroll(rememberScrollState()).padding(horizontal = 14.dp)) {
        listOf(Adjustment.Hue, Adjustment.Saturation, Adjustment.Temperature, Adjustment.Tint).forEach { adjustment ->
            val value = session.values[adjustment] ?: adjustment.neutral
            Row(Modifier.fillMaxWidth().padding(top = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(adjustment.label, color = PanelBody, fontSize = 11.sp, modifier = Modifier.weight(1f))
                Text(shows(adjustment, value), color = PanelMuted, fontSize = 10.sp)
            }
            PhotoSlider(
                position = adjustment.fraction(value),
                modifier = Modifier.fillMaxWidth(),
                onChange = {
                    session.selectAdjustment(adjustment)
                    session.setAdjustment(adjustment.fromFraction(it))
                },
                onRelease = { session.commitAdjustments() },
            )
        }
    }
}

@Composable
private fun BrushPanel(session: PhotoSession) {
    PanelTitle("Brush")
    Column(Modifier.verticalScroll(rememberScrollState()).padding(horizontal = 14.dp)) {
        Row(Modifier.fillMaxWidth().padding(top = 4.dp)) {
            Text("Size", color = PanelBody, fontSize = 11.sp, modifier = Modifier.weight(1f))
            Text("${session.brushSize.roundToInt()} px", color = PanelMuted, fontSize = 10.sp)
        }
        PhotoSlider(
            position = (session.brushSize - 1f) / 199f,
            modifier = Modifier.fillMaxWidth(),
            onChange = { session.changeBrushSize(1f + it * 199f) },
        )
        Row(Modifier.fillMaxWidth().padding(top = 6.dp)) {
            Text("Opacity", color = PanelBody, fontSize = 11.sp, modifier = Modifier.weight(1f))
            Text("${(session.brushOpacity * 100).roundToInt()}%", color = PanelMuted, fontSize = 10.sp)
        }
        PhotoSlider(
            position = session.brushOpacity,
            modifier = Modifier.fillMaxWidth(),
            onChange = { session.changeBrushOpacity(it) },
        )
        Row(Modifier.fillMaxWidth().padding(top = 6.dp)) {
            Text("Hardness", color = PanelBody, fontSize = 11.sp, modifier = Modifier.weight(1f))
            Text("${(session.brushHardness * 100).roundToInt()}%", color = PanelMuted, fontSize = 10.sp)
        }
        PhotoSlider(
            position = session.brushHardness,
            modifier = Modifier.fillMaxWidth(),
            onChange = { session.changeBrushHardness(it) },
        )
        Spacer(Modifier.height(10.dp))
        Text(
            "A stroke is painted by the drawing engine, so it behaves exactly as it does there.",
            color = PanelMuted,
            fontSize = 10.sp,
            modifier = Modifier.padding(bottom = 8.dp),
        )
    }
}

@Composable
private fun LayersPanel(session: PhotoSession) {
    val layers = session.info?.layers.orEmpty()
    PanelTitle("Layers")
    LazyColumn(Modifier.fillMaxWidth().heightIn(max = 300.dp)) {
        items(layers, key = { it.id }) { layer ->
            val active = layer.id == session.info?.activeLayer
            Row(
                Modifier
                    .fillMaxWidth()
                    .background(if (active) RowSelected else Color.Transparent)
                    .clickable { session.selectLayer(layer.id) }
                    .padding(horizontal = 14.dp, vertical = 9.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(layer.name, color = if (active) Color.White else PanelBody, fontSize = 11.sp, modifier = Modifier.weight(1f))
                Text(
                    "${(layer.opacity * 100).roundToInt()}%",
                    color = PanelMuted,
                    fontSize = 10.sp,
                    modifier = Modifier.clickable { session.setLayer(layer.id, layer.name, !layer.visible, layer.opacity) },
                )
            }
        }
    }
    Spacer(Modifier.height(6.dp))
    Text(
        "+ Add layer",
        color = Color(0xFF7AB8FF),
        fontSize = 11.sp,
        modifier = Modifier.padding(start = 14.dp, bottom = 10.dp).clickable { session.addLayer() },
    )
}

/**
 * A slider that reports both the drag and its end.
 *
 * The end matters: dragging only moves the preview, and lifting the finger is what commits the
 * change to the engine, so one drag is one undoable step instead of hundreds.
 */
@Composable
internal fun PhotoSlider(
    position: Float,
    modifier: Modifier = Modifier,
    min: Float? = null,
    max: Float? = null,
    onChange: (Float) -> Unit,
    onRelease: (() -> Unit)? = null,
) {
    val range = min != null && max != null
    BoxWithConstraints(
        modifier
            .height(22.dp)
            .pointerInput(range, min, max) {
                detectHorizontalDragGestures(
                    onDragStart = { offset ->
                        onChange(if (range) min!! + (max!! - min) * (offset.x / size.width).coerceIn(0f, 1f) else (offset.x / size.width).coerceIn(0f, 1f))
                    },
                    onDragEnd = { onRelease?.invoke() },
                    onDragCancel = { onRelease?.invoke() },
                ) { change, _ ->
                    val x = change.position.x
                    onChange(if (range) min!! + (max!! - min) * (x / size.width).coerceIn(0f, 1f) else (x / size.width).coerceIn(0f, 1f))
                    change.consume()
                }
            },
        contentAlignment = Alignment.CenterStart,
    ) {
        val travel = (maxWidth - 12.dp).coerceAtLeast(0.dp)
        Box(Modifier.fillMaxWidth().height(3.dp).clip(RoundedCornerShape(2.dp)).background(Color(0xFF3A3C36)))
        Box(Modifier.fillMaxWidth(position.coerceIn(0f, 1f)).height(3.dp).clip(RoundedCornerShape(2.dp)).background(Color(0xFF328FFF)))
        // The thumb's centre follows the drag, and it travels the slider's real width.
        Box(
            Modifier
                .offset(x = travel * position.coerceIn(0f, 1f))
                .size(12.dp)
                .clip(RoundedCornerShape(6.dp))
                .background(Color.White),
        )
    }
}
