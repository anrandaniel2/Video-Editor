package io.github.artcraftandroid.filmcraft.ui.editor.photo

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.withTransform
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.unit.toSize
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlin.math.roundToInt

/**
 * The photo editor's screen.
 *
 * Not the drawing canvas. A photo is edited by adjusting it — exposure, contrast, colour, curves —
 * with a brush available for touching up, which is what this offers and what the drawing canvas
 * does not. The rail on the left is the tools; the panel on the right belongs to the tool that
 * opened it and sits beside it, as the reference UI's workspaces do.
 *
 * Everything numeric happens in Rust ([PhotoDocument]): the screen asks for a region to draw and
 * gets bytes back. It never touches a whole picture at once, so a large photo opens in the time it
 * takes to decode it, and a slider moves at the speed of one thumbnail.
 */
@Composable
internal fun PhotoEditorScreen(
    session: PhotoSession,
    title: String,
    onExport: () -> Unit,
    onClose: () -> Unit,
) {
    val photo = session.document ?: run {
        Box(Modifier.fillMaxSize().background(Color.Black), contentAlignment = Alignment.Center) {
            Text(session.status ?: "Opening the photo…", color = Color.White, fontSize = 14.sp)
        }
        return
    }

    Row(Modifier.fillMaxSize().background(Color(0xFF1B1C1A))) {
        PhotoRail(
            tool = session.tool,
            canUndo = session.info?.can_undo == true,
            canRedo = session.info?.can_redo == true,
            onTool = { session.choose(it) },
            onUndo = { session.undo() },
            onRedo = { session.redo() },
            onDone = onClose,
        )
        // The picture, and beside it the panel of whatever tool is open.
        BoxWithConstraints(Modifier.weight(1f).fillMaxHeight()) {
            Column(Modifier.fillMaxSize()) {
                PhotoHeader(title, onExport, onClose)
                PhotoCanvas(session, Modifier.weight(1f).fillMaxWidth())
                PhotoToolBar(session)
            }
            // The panel hangs beside the rail it belongs to, not over the middle of the picture.
            session.tool.panel?.let { panel ->
                PhotoPanel(session, panel, Modifier.align(Alignment.CenterStart).padding(start = 12.dp))
            }
        }
    }
    LaunchedEffect(photo) { session.loadRegion() }
}

/** The tools a photo is edited with. The order is the reference UI's Pixel workspace. */
internal enum class PhotoTool(val label: String, val panel: PhotoPanelKind?) {
    Move("Move", null),
    Crop("Crop", PhotoPanelKind.Crop),
    Select("Select", PhotoPanelKind.None),
    Brush("Paint", PhotoPanelKind.Brush),
    Erase("Erase", PhotoPanelKind.Brush),
    Heal("Heal", PhotoPanelKind.Retouch),
    Clone("Clone", PhotoPanelKind.Retouch),
    Fill("Fill", null),
    Adjust("Adjust", PhotoPanelKind.Adjust),
    Curves("Curves", PhotoPanelKind.Curves),
    Colour("Colour", PhotoPanelKind.Colour),
    Layers("Layers", PhotoPanelKind.Layers),
}

internal enum class PhotoPanelKind { Adjust, Curves, Colour, Brush, Retouch, Layers, Crop, None }

private val RailWidth = 56.dp
private val PanelWidth = 264.dp

@Composable
private fun PhotoRail(
    tool: PhotoTool,
    canUndo: Boolean,
    canRedo: Boolean,
    onTool: (PhotoTool) -> Unit,
    onUndo: () -> Unit,
    onRedo: () -> Unit,
    onDone: () -> Unit,
) {
    Column(
        Modifier.width(RailWidth).fillMaxHeight().background(Color(0xFF232420)).padding(vertical = 8.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        RailButton("✓", "Done", false, onDone)
        Spacer(Modifier.height(10.dp))
        PhotoTool.entries.forEach { entry ->
            RailButton(GLYPHS[entry] ?: "•", entry.label, entry == tool) { onTool(entry) }
        }
        Spacer(Modifier.weight(1f))
        RailButton("↶", "Undo", false, enabled = canUndo, onClick = onUndo)
        RailButton("↷", "Redo", false, enabled = canRedo, onClick = onRedo)
    }
}

/**
 * Glyphs are text for now: the reference draws each tool as an icon, but a wrong icon reads as a
 * bug, and a character that is obviously the right shape is better than an approximate icon. This
 * is the one place a real icon set should replace it.
 */
private val GLYPHS = mapOf(
    PhotoTool.Move to "✥", PhotoTool.Crop to "⧉", PhotoTool.Select to "⬚",
    PhotoTool.Brush to "✎", PhotoTool.Erase to "⌫", PhotoTool.Heal to "✚",
    PhotoTool.Clone to "⧉", PhotoTool.Fill to "◧", PhotoTool.Adjust to "◐",
    PhotoTool.Curves to "∿", PhotoTool.Colour to "◉", PhotoTool.Layers to "▤",
)

@Composable
private fun RailButton(glyph: String, label: String, selected: Boolean, enabled: Boolean = true, onClick: () -> Unit) {
    Box(
        Modifier
            .size(46.dp, 42.dp)
            .clip(RoundedCornerShape(10.dp))
            .background(if (selected) Color(0xFF328FFF) else Color.Transparent)
            .clickable(enabled = enabled, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            glyph,
            color = when {
                !enabled -> Color(0xFF5A5C55)
                selected -> Color.White
                else -> Color(0xFFD5D8CC)
            },
            fontSize = 17.sp,
            fontWeight = if (selected) FontWeight.SemiBold else FontWeight.Normal,
        )
    }
}

@Composable
private fun PhotoHeader(title: String, onExport: () -> Unit, onClose: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().height(46.dp).background(Color(0xFF232420)).padding(horizontal = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        Text(title, color = Color(0xFFE8EAE0), fontSize = 14.sp, fontWeight = FontWeight.Medium, maxLines = 1, modifier = Modifier.weight(1f))
        Text("Export", color = Color(0xFF7AB8FF), fontSize = 13.sp, modifier = Modifier.clickable(onClick = onExport))
        Text("Done", color = Color(0xFF7AB8FF), fontSize = 13.sp, modifier = Modifier.clickable(onClick = onClose))
    }
}

/**
 * The picture, and the work on it.
 *
 * The canvas draws the region Rust hands it. It does not keep the whole photo in memory or in a
 * Compose bitmap: what is on the glass is what is asked for, which is what keeps a large file usable.
 */
@Composable
private fun PhotoCanvas(session: PhotoSession, modifier: Modifier) {
    val bitmap = session.region
    Box(modifier.background(Color(0xFF141513)), contentAlignment = Alignment.Center) {
        if (bitmap != null) {
            val image = remember(bitmap) { bitmap.asImageBitmap() }
            Canvas(
                Modifier.fillMaxSize()
                    .onSizeChanged { session.canvasSize(Size(it.width.toFloat(), it.height.toFloat())) }
                    .pointerInput(session.tool, session.brushSize) {
                    // A stroke is where the finger goes, in picture coordinates, with the pressure
                    // the screen reports — the same points the drawing canvas would send.
                    awaitEachGesture {
                        val down = awaitFirstDown(requireUnconsumed = false)
                        session.beginStroke(down.position, this@pointerInput.size.toSize(), down.pressure)
                        while (true) {
                            val change = awaitPointerEvent().changes.firstOrNull() ?: break
                            if (!change.pressed) break
                            session.strokePoint(change.position, this@pointerInput.size.toSize(), change.pressure)
                            change.consume()
                        }
                        session.endStroke()
                    }
                },
            ) {
                withTransform({
                    val scale = minOf(size.width / image.width, size.height / image.height)
                    translate((size.width - image.width * scale) / 2f, (size.height - image.height * scale) / 2f)
                    scale(scale, scale, androidx.compose.ui.geometry.Offset.Zero)
                }) {
                    drawImage(image, contentScale = ContentScale.FillBounds, filterQuality = androidx.compose.ui.graphics.FilterQuality.Medium)
                }
            }
        } else {
            Text("Rendering…", color = Color(0xFF8A8F80), fontSize = 13.sp)
        }
    }
}

/** The bar under the picture: the tool's primary control, where every workspace in the reference has one. */
@Composable
private fun PhotoToolBar(session: PhotoSession) {
    Row(
        Modifier.fillMaxWidth().height(52.dp).background(Color(0xFF232420)).padding(horizontal = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        when (session.tool) {
            PhotoTool.Brush, PhotoTool.Erase -> {
                Text("Size", color = Color(0xFFB9BEAF), fontSize = 11.sp)
                PhotoSlider(
                    position = (session.brushSize - 1f) / 199f,
                    modifier = Modifier.weight(1f),
                    onChange = { session.setBrushSize(1f + it * 199f) },
                )
                Text("${session.brushSize.roundToInt()}", color = Color(0xFFE8EAE0), fontSize = 11.sp, modifier = Modifier.width(34.dp))
            }
            PhotoTool.Adjust, PhotoTool.Curves, PhotoTool.Colour -> {
                val adjustment = session.activeAdjustment
                // The slider follows the value on screen, not the adjustment's neutral: a control
                // that springs back to zero the moment you look at it is worse than no control.
                val value = session.values[adjustment] ?: adjustment.neutral
                Text(adjustment.label, color = Color(0xFFB9BEAF), fontSize = 11.sp)
                PhotoSlider(
                    position = adjustment.fraction(value),
                    modifier = Modifier.weight(1f),
                    onChange = { session.setAdjustment(adjustment.fromFraction(it)) },
                    onRelease = { session.commitAdjustments() },
                )
                Text(
                    "${(value * 100).roundToInt() / 100f}",
                    color = Color(0xFFE8EAE0),
                    fontSize = 11.sp,
                    modifier = Modifier.width(42.dp),
                )
            }
            else -> Text(session.tool.label, color = Color(0xFFB9BEAF), fontSize = 11.sp)
        }
    }
}
