package io.github.artcraftandroid.photocraft.editor

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.artcraftandroid.photocraft.ui.GlyphIcon
import io.github.artcraftandroid.photocraft.ui.Glyphs
import io.github.artcraftandroid.photocraft.ui.Pc
import org.json.JSONObject

/**
 * The photo editor, after the reference tablet UI: a top pill of mode tabs, a left vertical
 * toolbar of tools, a right vertical toolbar of panels, the canvas in the middle, and floating
 * dark panels for brushes, layers, curves and the rest. Every control runs an engine command.
 */
@Composable
fun PhotoEditorScreen(vm: PhotoEditorViewModel, onExport: (ByteArray, String) -> Unit) {
    Box(Modifier.fillMaxSize().background(Pc.Black)) {
        // Canvas, full-bleed behind the chrome.
        CanvasView(vm)

        // Top mode tabs.
        ModeTabs(vm, Modifier.align(Alignment.TopCenter).padding(top = 12.dp))

        // Colours and history, top-left: what the painting tools paint with, and the way back.
        ColourBar(vm, Modifier.align(Alignment.TopStart).padding(start = 12.dp, top = 12.dp))

        // Left toolbar.
        LeftToolbar(vm, Modifier.align(Alignment.CenterStart).padding(start = 12.dp))

        // Right toolbar.
        RightToolbar(vm, Modifier.align(Alignment.CenterEnd).padding(end = 12.dp))

        // Floating panel.
        when (vm.panel) {
            Panel.None -> {}
            Panel.Brushes -> BrushesPanel(vm, Modifier.align(Alignment.Center).padding(end = 72.dp))
            Panel.Layers -> LayersPanel(vm, Modifier.align(Alignment.Center).padding(end = 72.dp))
            Panel.Curves -> CurvesPanel(vm, Modifier.align(Alignment.Center).padding(end = 72.dp))
            Panel.Levels -> LevelsPanel(vm, Modifier.align(Alignment.Center).padding(end = 72.dp))
            Panel.Adjustments -> AdjustmentsPanel(vm, Modifier.align(Alignment.Center).padding(end = 72.dp))
            Panel.Filters -> FiltersPanel(vm, Modifier.align(Alignment.Center).padding(end = 72.dp))
            Panel.Image -> ImagePanel(vm, Modifier.align(Alignment.Center).padding(end = 72.dp))
            Panel.Type -> TypePanel(vm, Modifier.align(Alignment.BottomCenter).padding(bottom = 24.dp))
            Panel.Pages -> PagesPanel(vm, Modifier.align(Alignment.Center).padding(end = 72.dp))
            Panel.History -> HistoryPanel(vm, Modifier.align(Alignment.Center).padding(end = 72.dp))
            Panel.Export -> ExportPanel(vm, onExport, Modifier.align(Alignment.Center).padding(end = 72.dp))
            Panel.Colour -> ColourPanel(vm, Modifier.align(Alignment.Center).padding(end = 72.dp))
        }

        // Error, bottom-centre.
        vm.error?.let { msg ->
            Box(
                Modifier.align(Alignment.BottomCenter).padding(bottom = 12.dp, start = 72.dp, end = 72.dp)
                    .background(Pc.Negative, RoundedCornerShape(8.dp))
                    .clickable { vm.dismissError() }
                    .padding(horizontal = 16.dp, vertical = 10.dp),
            ) {
                Text(msg, color = Color.White, fontSize = 13.sp)
            }
        }

        // Zoom, bottom-right: the multiplier the canvas is showing.
        Box(
            Modifier.align(Alignment.BottomEnd).padding(end = 12.dp, bottom = 44.dp)
                .background(Pc.Toolbar.copy(alpha = 0.92f), RoundedCornerShape(12.dp))
                .clickable { vm.setZoom(1f) }
                .padding(horizontal = 10.dp, vertical = 6.dp),
        ) {
            Text("${"%.1f".format(vm.zoom)}x", color = Pc.Secondary, fontSize = 12.sp)
        }

        // Busy, top-right.
        if (vm.busy) {
            Box(
                Modifier.align(Alignment.TopEnd).padding(top = 16.dp, end = 16.dp)
                    .background(Pc.Panel, RoundedCornerShape(8.dp))
                    .padding(horizontal = 12.dp, vertical = 6.dp),
            ) {
                Text("Working…", color = Pc.Secondary, fontSize = 12.sp)
            }
        }
    }
}

/** The foreground and background swatches, a swap, and undo/redo. */
@Composable
private fun ColourBar(vm: PhotoEditorViewModel, modifier: Modifier = Modifier) {
    Row(
        modifier.background(Pc.Toolbar.copy(alpha = 0.92f), RoundedCornerShape(16.dp))
            .padding(horizontal = 8.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        // Foreground over background, as Photoshop's swatches: the foreground is the one the
        // painting tools use.
        Box(Modifier.size(34.dp)) {
            Box(
                Modifier.size(22.dp).align(Alignment.BottomEnd)
                    .background(Color(vm.background), RoundedCornerShape(4.dp))
                    .border(1.dp, Pc.Hairline, RoundedCornerShape(4.dp))
                    // The background swatch also opens the colours, where it can be set.
                    .clickable { vm.setPanel(Panel.Colour) },
            )
            Box(
                Modifier.size(22.dp).align(Alignment.TopStart)
                    .background(Color(vm.foreground), RoundedCornerShape(4.dp))
                    .border(1.dp, Color.White.copy(alpha = 0.6f), RoundedCornerShape(4.dp))
                    // A tap on the foreground opens the colour panel.
                    .clickable { vm.setPanel(Panel.Colour) },
            )
        }
        Box(
            Modifier.size(30.dp).clip(RoundedCornerShape(8.dp)).clickable { vm.swapColors() },
            contentAlignment = Alignment.Center,
        ) {
            GlyphIcon(Glyphs.Gradient, Pc.Title, size = 18.dp, description = "Swap colours")
        }
        // Undo and redo, greyed by the engine's own history state (whether it can undo and redo).
        Text(
            "↶",
            color = if (vm.status?.canUndo == true) Pc.Title else Pc.Detail,
            fontSize = 20.sp,
            modifier = Modifier.clip(RoundedCornerShape(8.dp))
                .clickable(enabled = vm.status?.canUndo == true) { vm.undo() }
                .padding(horizontal = 6.dp, vertical = 2.dp),
        )
        Text(
            "↷",
            color = if (vm.status?.canRedo == true) Pc.Title else Pc.Disabled,
            fontSize = 20.sp,
            modifier = Modifier.clip(RoundedCornerShape(8.dp))
                .clickable(enabled = vm.status?.canRedo == true) { vm.redo() }
                .padding(horizontal = 6.dp, vertical = 2.dp),
        )
    }
}

@Composable
private fun CanvasView(vm: PhotoEditorViewModel) {
    val bmp = vm.bitmap
    val doc = vm.status?.active
    // The rectangle the document actually occupies once Compose fits it into the canvas: a tap
    // inside it is a document pixel, so painting lands under the finger whatever the zoom.
    var box by remember { mutableStateOf(androidx.compose.ui.unit.IntSize.Zero) }
    Box(
        Modifier.fillMaxSize().background(Pc.Black)
            .onSizeChanged { box = it }
            .pointerInput(vm.tool) {
                // The Hand and Zoom tools move the view: a drag pans and a pinch zooms. The
                // painting tools keep the finger for paint, as they do on a desktop, where the
                // view is moved with the space bar instead.
                if (vm.tool != Tool.Hand && vm.tool != Tool.Zoom) return@pointerInput
                detectTransformGestures { _, pan, gestureZoom, _ ->
                    if (gestureZoom != 1f) vm.setZoom(vm.zoom * gestureZoom)
                    if (pan != Offset.Zero) vm.pan(-pan.x.toInt(), -pan.y.toInt())
                }
            }
            .pointerInput(doc?.index, doc?.width, doc?.height, box, vm.tool) {
                // A drag paints: each move extends the stroke, and lifting sends it to the engine
                // as one `paint.stroke`, so the History panel has one step per stroke and the
                // canvas doesn't wait for a round trip per dab. Tools that work on a tap alone
                // (Fill, the Wand, the Eyedropper) answer to the tap instead.
                if (doc == null || vm.tool == Tool.Hand || vm.tool == Tool.Zoom) return@pointerInput
                val toDoc: (Offset) -> Pair<Float, Float>? = { o ->
                    vm.documentView()?.let { view -> documentPoint(box.width, box.height, view, o.x, o.y) }
                }
                var stroke = mutableListOf<Offset>()
                detectDragGestures(
                    onDragStart = { start -> stroke = mutableListOf(start) },
                    onDragEnd = {
                        val points = stroke.mapNotNull(toDoc)
                        stroke = mutableListOf()
                        vm.endDrag(points)
                    },
                    onDragCancel = {
                        // The gesture was taken by something else (a system swipe): drop the
                        // preview with it, so no half-stroke is left drawn on the canvas.
                        stroke = mutableListOf()
                        vm.previewStroke(emptyList())
                    },
                ) { change, _ ->
                    change.consume()
                    // The pointer's own position, not the delta: a fast drag still leaves a
                    // continuous line rather than a cluster of points at the start.
                    stroke.add(change.position)
                    vm.previewStroke(stroke.mapNotNull(toDoc))
                }
            }
            .pointerInput(doc?.index, doc?.width, doc?.height, box, vm.tool) {
                if (vm.tool == Tool.Hand) return@pointerInput
                detectTapGestures { offset ->
                    val view = vm.documentView() ?: return@detectTapGestures
                    documentPoint(box.width, box.height, view, offset.x, offset.y)
                        ?.let { (x, y) -> vm.tapAt(x, y) }
                }
            },
        contentAlignment = Alignment.Center,
    ) {
        if (bmp != null) {
            Image(
                bmp.asImageBitmap(),
                contentDescription = "Canvas",
                contentScale = ContentScale.Fit,
                modifier = Modifier.fillMaxSize(),
            )
        } else {
            Text("Open a photo to start", color = Pc.Detail, fontSize = 14.sp)
        }
        // The stroke under the finger, drawn as it goes in the document's own pixels scaled to
        // where the preview shows them, so the paint appears to come off the brush. The engine
        // has the whole stroke once the finger lifts.
        val stroke = vm.strokePts
        val view = vm.documentView()
        if (stroke.isNotEmpty() && view != null) {
            Canvas(Modifier.fillMaxSize()) {
                val scale = minOf(size.width / view.width, size.height / view.height)
                val left = (size.width - view.width * scale) / 2f
                val top = (size.height - view.height * scale) / 2f
                val path = androidx.compose.ui.graphics.Path()
                stroke.forEachIndexed { i, p ->
                    val x = left + (p.x - view.x) * scale
                    val y = top + (p.y - view.y) * scale
                    if (i == 0) path.moveTo(x, y) else path.lineTo(x, y)
                }
                drawPath(
                    path,
                    color = if (vm.tool == Tool.Eraser) Color.White.copy(alpha = 0.5f) else Color(vm.foreground),
                    style = androidx.compose.ui.graphics.drawscope.Stroke(
                        width = (vm.brushSize * scale).coerceAtLeast(1f),
                        cap = androidx.compose.ui.graphics.StrokeCap.Round,
                        join = androidx.compose.ui.graphics.StrokeJoin.Round,
                    ),
                )
            }
        }
        // The Clone Stamp's source: a cross where it will copy from, so the next drag is not a
        // guess.
        val cloneFrom = vm.cloneFrom
        val cloneView = vm.documentView()
        if (cloneFrom != null && cloneView != null) {
            Canvas(Modifier.fillMaxSize()) {
                val scale = minOf(size.width / cloneView.width, size.height / cloneView.height)
                val left = (size.width - cloneView.width * scale) / 2f
                val top = (size.height - cloneView.height * scale) / 2f
                val x = left + (cloneFrom.first - cloneView.x) * scale
                val y = top + (cloneFrom.second - cloneView.y) * scale
                val r = 10f
                drawCircle(Pc.AccentLight, radius = r, center = Offset(x, y), style = androidx.compose.ui.graphics.drawscope.Stroke(width = 2f))
                drawLine(Pc.AccentLight, Offset(x - r, y), Offset(x + r, y), 2f)
                drawLine(Pc.AccentLight, Offset(x, y - r), Offset(x, y + r), 2f)
            }
        }

        // Brush cursor: a circle at the canvas centre, sized by the brush.
        Canvas(Modifier.size(48.dp).align(Alignment.Center)) {
            drawCircle(
                color = Color.White.copy(alpha = 0.6f),
                radius = size.minDimension / 2f,
                style = androidx.compose.ui.graphics.drawscope.Stroke(width = 1.5f),
            )
        }
    }
}

@Composable
private fun ModeTabs(vm: PhotoEditorViewModel, modifier: Modifier = Modifier) {
    Row(
        modifier.horizontalScroll(rememberScrollState())
            .background(Pc.Toolbar.copy(alpha = 0.92f), RoundedCornerShape(24.dp))
            .padding(4.dp),
        horizontalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        Mode.entries.forEach { m ->
            val active = vm.mode == m
            val color = when (m) {
                Mode.Sketch -> Pc.ModeSketch
                Mode.Vector -> Pc.ModeVector
                Mode.Pixel -> Pc.ModePixel
                Mode.Layout -> Pc.ModeLayout
                Mode.Canva -> Pc.ModeCanva
            }
            Box(
                Modifier.clip(RoundedCornerShape(20.dp))
                    .background(if (active) color else Color.Transparent)
                    .clickable { vm.setMode(m) }
                    .padding(horizontal = 16.dp, vertical = 8.dp),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    m.label,
                    color = if (active) Color.Black else Pc.Title,
                    fontSize = 13.sp,
                    fontWeight = if (active) FontWeight.Bold else FontWeight.Normal,
                )
            }
        }
    }
}

@Composable
private fun LeftToolbar(vm: PhotoEditorViewModel, modifier: Modifier = Modifier) {
    // Fifteen tools are taller than a phone held sideways: the bar scrolls rather than running
    // off the screen.
    Column(
        modifier.fillMaxHeight(0.86f).verticalScroll(rememberScrollState())
            .background(Pc.Toolbar.copy(alpha = 0.92f), RoundedCornerShape(16.dp))
            .padding(6.dp),
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        ToolButton(Tool.Move, vm, Glyphs.Move)
        ToolButton(Tool.Brush, vm, Glyphs.BrushTip)
        ToolButton(Tool.Pencil, vm, Glyphs.BrushTip)
        ToolButton(Tool.Eraser, vm, Glyphs.BrushTip)
        ToolButton(Tool.Fill, vm, Glyphs.Gradient)
        ToolButton(Tool.Gradient, vm, Glyphs.Gradient)
        ToolButton(Tool.Clone, vm, Glyphs.Clone)
        ToolButton(Tool.Heal, vm, Glyphs.Heal)
        ToolButton(Tool.Lasso, vm, Glyphs.Lasso)
        ToolButton(Tool.Wand, vm, Glyphs.Wand)
        ToolButton(Tool.Rect, vm, Glyphs.Rect)
        ToolButton(Tool.Type, vm, Glyphs.BrushTip)
        ToolButton(Tool.Eyedropper, vm, Glyphs.Eyedropper)
        ToolButton(Tool.Hand, vm, Glyphs.Hand)
        ToolButton(Tool.Zoom, vm, Glyphs.Zoom)
    }
}

@Composable
private fun ToolButton(tool: Tool, vm: PhotoEditorViewModel, glyph: io.github.artcraftandroid.photocraft.ui.Glyph) {
    val active = vm.tool == tool
    Box(
        Modifier.size(44.dp).clip(RoundedCornerShape(10.dp))
            .background(if (active) Pc.Accent else Color.Transparent)
            .clickable { vm.setTool(tool) },
        contentAlignment = Alignment.Center,
    ) {
        GlyphIcon(glyph, if (active) Color.White else Pc.Title, size = 22.dp, description = tool.label)
    }
}

@Composable
private fun RightToolbar(vm: PhotoEditorViewModel, modifier: Modifier = Modifier) {
    Column(
        modifier.fillMaxHeight(0.86f).verticalScroll(rememberScrollState())
            .background(Pc.Toolbar.copy(alpha = 0.92f), RoundedCornerShape(16.dp))
            .padding(6.dp),
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        PanelButton(Panel.Brushes, vm, Glyphs.BrushTip, "Brushes")
        PanelButton(Panel.Colour, vm, Glyphs.Eyedropper, "Colour")
        PanelButton(Panel.Layers, vm, Glyphs.BrushTip, "Layers")
        PanelButton(Panel.Curves, vm, Glyphs.Curve, "Curves")
        PanelButton(Panel.Levels, vm, Glyphs.Histogram, "Levels")
        PanelButton(Panel.Adjustments, vm, Glyphs.Sun, "Adjustments")
        PanelButton(Panel.Filters, vm, Glyphs.Droplet, "Filters")
        PanelButton(Panel.Image, vm, Glyphs.Crop, "Image")
        PanelButton(Panel.Type, vm, Glyphs.BrushTip, "Type")
        PanelButton(Panel.Pages, vm, Glyphs.Frame, "Pages")
        PanelButton(Panel.History, vm, Glyphs.BrushTip, "History")
        PanelButton(Panel.Export, vm, Glyphs.BrushTip, "Export")
    }
}

@Composable
private fun PanelButton(panel: Panel, vm: PhotoEditorViewModel, glyph: io.github.artcraftandroid.photocraft.ui.Glyph, label: String) {
    val active = vm.panel == panel
    Box(
        Modifier.size(44.dp).clip(RoundedCornerShape(10.dp))
            .background(if (active) Pc.Accent else Color.Transparent)
            .clickable { vm.setPanel(panel) },
        contentAlignment = Alignment.Center,
    ) {
        GlyphIcon(glyph, if (active) Color.White else Pc.Title, size = 22.dp, description = label)
    }
}

@Composable
private fun PanelShell(title: String, onClose: () -> Unit, modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    Column(
        modifier.width(320.dp).fillMaxHeight(0.9f)
            .background(Pc.Panel.copy(alpha = 0.96f), RoundedCornerShape(16.dp))
            .border(1.dp, Pc.Hairline, RoundedCornerShape(16.dp))
            .padding(16.dp),
    ) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
            Text(title, color = Pc.Title, fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
            Text("Done", color = Pc.AccentLight, fontSize = 13.sp, modifier = Modifier.clickable(onClick = onClose))
        }
        Spacer(Modifier.height(12.dp))
        // The panel's own content scrolls when it is taller than the space the panel is given.
        Column(Modifier.verticalScroll(rememberScrollState())) { content() }
    }
}

@Composable
private fun SliderRow(
    label: String,
    value: Float,
    range: ClosedFloatingPointRange<Float>,
    onChange: (Float) -> Unit,
    // Runs once the finger lifts. A slider that drives the engine sends its command here: one
    // command per gesture, not one per pixel of movement.
    onFinished: (() -> Unit)? = null,
) {
    Column(Modifier.fillMaxWidth()) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
            Text(label, color = Pc.Secondary, fontSize = 12.sp)
            Text("%.0f".format(value), color = Pc.Title, fontSize = 12.sp)
        }
        Slider(
            value = value,
            onValueChange = onChange,
            onValueChangeFinished = onFinished,
            valueRange = range,
            colors = SliderDefaults.colors(thumbColor = Pc.ThumbRing, activeTrackColor = Pc.SliderOn, inactiveTrackColor = Pc.SliderOff),
            modifier = Modifier.fillMaxWidth().height(28.dp),
        )
    }
}

@Composable
private fun ActionButton(label: String, modifier: Modifier = Modifier, onClick: () -> Unit) {
    Box(
        modifier.clip(RoundedCornerShape(8.dp)).background(Pc.PillButton).clickable(onClick = onClick)
            .padding(horizontal = 14.dp, vertical = 10.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(label, color = Pc.Title, fontSize = 13.sp)
    }
}

@Composable
private fun BrushesPanel(vm: PhotoEditorViewModel, modifier: Modifier = Modifier) {
    PanelShell("Brushes", vm::closePanel, modifier) {
        SliderRow("Size", vm.brushSize, 1f..512f, vm::setBrushSize) { vm.commitBrush() }
        SliderRow("Hardness", vm.brushHardness * 100f, 0f..100f, { vm.setBrushHardness(it / 100f) }) { vm.commitBrush() }
        SliderRow("Opacity", vm.brushOpacity * 100f, 0f..100f, { vm.setBrushOpacity(it / 100f) }) { vm.commitBrush() }
        Spacer(Modifier.height(8.dp))
        Text("Presets", color = Pc.Secondary, fontSize = 12.sp)
        LazyColumn(Modifier.fillMaxWidth().height(220.dp)) {
            items(vm.brushes) { b ->
                Row(
                    Modifier.fillMaxWidth().clickable {
                        val p = JSONObject()
                        p.put("preset", b.name)
                        vm.command("tools.setBrush", p)
                    }.padding(vertical = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Box(Modifier.size(24.dp).background(Pc.Raised, CircleShape))
                    Spacer(Modifier.width(10.dp))
                    Text(b.name, color = Pc.Title, fontSize = 13.sp)
                }
            }
        }
        Spacer(Modifier.height(8.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            ActionButton("New layer") { vm.command("layer.new.layer", JSONObject().put("name", "Brush")) }
            ActionButton("Clear") { vm.command("layer.delete", JSONObject()) }
        }
    }
}

@Composable
private fun LayersPanel(vm: PhotoEditorViewModel, modifier: Modifier = Modifier) {
    PanelShell("Layers", vm::closePanel, modifier) {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            ActionButton("New") { vm.command("layer.new.layer", JSONObject()) }
            ActionButton("Duplicate") { vm.command("layer.duplicate", JSONObject()) }
            ActionButton("Delete") { vm.command("layer.delete", JSONObject()) }
        }
        Spacer(Modifier.height(8.dp))
        LazyColumn(Modifier.fillMaxWidth().height(280.dp)) {
            items(vm.layers) { l ->
                Row(
                    Modifier.fillMaxWidth()
                        .background(if (l.active) Pc.Raised else Color.Transparent, RoundedCornerShape(8.dp))
                        .clickable {
                            val p = JSONObject()
                            p.put("layer", l.id)
                            vm.command("layer.select", p)
                        }
                        .padding(horizontal = 8.dp, vertical = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    GlyphIcon(
                        if (l.visible) Glyphs.Eye else Glyphs.EyeOff,
                        Pc.Secondary,
                        size = 18.dp,
                        modifier = Modifier.clickable {
                            val p = JSONObject()
                            p.put("layer", l.id)
                            p.put("visible", !l.visible)
                            vm.command("layer.setProps", p)
                        },
                    )
                    Spacer(Modifier.width(8.dp))
                    // Not weight(): the panel's content scrolls, and a weighted row inside a
                    // scrolling column has no bounded height to divide.
                    Column(Modifier.width(220.dp)) {
                        Text(l.name, color = Pc.Title, fontSize = 13.sp)
                        Text("${l.kind} · ${l.blend} · ${"%.0f".format(l.opacity * 100f)}%", color = Pc.Detail, fontSize = 11.sp)
                    }
                    if (l.active) {
                        Text("●", color = Pc.AccentLight, fontSize = 10.sp)
                    }
                }
            }
        }
        Spacer(Modifier.height(8.dp))
        // The active layer's opacity, held locally while the finger is down and sent once on
        // release: one command, one history step, no round trip per frame of the drag.
        val active = vm.layers.firstOrNull { it.active }
        var dragged by remember(active?.id) { mutableStateOf<Float?>(null) }
        val shown = dragged ?: (active?.opacity ?: 1f) * 100f
        SliderRow("Opacity", shown, 0f..100f, { dragged = it }) {
            val l = active ?: return@SliderRow
            val v = dragged ?: return@SliderRow
            dragged = null
            val p = JSONObject()
            p.put("layer", l.id)
            p.put("opacity", v / 100f)
            vm.command("layer.setProps", p)
            vm.refreshLayersNow()
        }
    }
}

@Composable
private fun CurvesPanel(vm: PhotoEditorViewModel, modifier: Modifier = Modifier) {
    PanelShell("Curves", vm::closePanel, modifier) {
        // The curve is the adjustment: two points (a black point and a white point) that the
        // sliders move, sent to the engine as `image.adjustments.curves`.
        var black by remember { mutableStateOf(0f) }
        var white by remember { mutableStateOf(255f) }
        // One history step per drag, not per pixel: `coalesce` names the gesture, and the engine
        // merges consecutive calls that carry the same name.
        fun apply(b: Float, w: Float) {
            if (w - b < 2f) return
            val p = JSONObject()
            p.put("points", org.json.JSONArray().put(org.json.JSONArray().put(0).put(b.toInt())).put(org.json.JSONArray().put(255).put(w.toInt())))
            p.put("coalesce", "photocraft.curves")
            vm.command("image.adjustments.curves", p)
        }
        Box(Modifier.fillMaxWidth().height(180.dp).background(Pc.Lane, RoundedCornerShape(8.dp))) {
            Canvas(Modifier.fillMaxSize().padding(8.dp)) {
                for (i in 1 until 4) {
                    drawLine(Color.White.copy(alpha = 0.08f), Offset(size.width * i / 4f, 0f), Offset(size.width * i / 4f, size.height), 1f)
                    drawLine(Color.White.copy(alpha = 0.08f), Offset(0f, size.height * i / 4f), Offset(size.width, size.height * i / 4f), 1f)
                }
                drawLine(Color.White.copy(alpha = 0.25f), Offset(0f, size.height), Offset(size.width, 0f), 1f)
                // The curve the two points describe: in 0..255, out the slider's value.
                val x0 = size.width * (0f)
                val y0 = size.height * (1f - black / 255f)
                val x1 = size.width
                val y1 = size.height * (1f - white / 255f)
                drawLine(Pc.AccentLight, Offset(x0, y0), Offset(x1, y1), strokeWidth = 2f)
            }
        }
        Spacer(Modifier.height(12.dp))
        SliderRow("Black point", black, 0f..253f, { black = it }) { apply(black, white) }
        SliderRow("White point", white, 2f..255f, { white = it }) { apply(black, white) }
        Spacer(Modifier.height(8.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            ActionButton("Apply as layer") {
                val p = JSONObject()
                p.put("points", org.json.JSONArray().put(org.json.JSONArray().put(0).put(black.toInt())).put(org.json.JSONArray().put(255).put(white.toInt())))
                vm.command("layer.newAdjustmentLayer.curves", p)
            }
            ActionButton("Reset") {
                black = 0f
                white = 255f
                // A key of its own, so Reset is a state the user can undo back from.
                val p = JSONObject()
                p.put("points", org.json.JSONArray().put(org.json.JSONArray().put(0).put(0)).put(org.json.JSONArray().put(255).put(255)))
                p.put("coalesce", "photocraft.curves.reset")
                vm.command("image.adjustments.curves", p)
            }
        }
    }
}

@Composable
private fun LevelsPanel(vm: PhotoEditorViewModel, modifier: Modifier = Modifier) {
    PanelShell("Levels", vm::closePanel, modifier) {
        var inBlack by remember { mutableStateOf(0f) }
        var gamma by remember { mutableStateOf(1f) }
        var inWhite by remember { mutableStateOf(255f) }
        // One history step per drag: see CurvesPanel.
        fun apply() {
            val p = JSONObject()
            p.put("inBlack", inBlack.toInt())
            p.put("gamma", gamma)
            p.put("inWhite", inWhite.toInt())
            p.put("coalesce", "photocraft.levels")
            vm.command("image.adjustments.levels", p)
        }
        Box(Modifier.fillMaxWidth().height(120.dp).background(Pc.Lane, RoundedCornerShape(8.dp))) {
            Canvas(Modifier.fillMaxSize().padding(8.dp)) {
                // The tonal range the three inputs describe, black to white.
                drawRect(Pc.AccentLight.copy(alpha = 0.25f), Offset(0f, 0f), androidx.compose.ui.geometry.Size(size.width * (inBlack / 255f), size.height))
                drawRect(Pc.AccentLight.copy(alpha = 0.7f), Offset(size.width * (inBlack / 255f), 0f), androidx.compose.ui.geometry.Size(size.width * ((inWhite - inBlack) / 255f), size.height))
                drawRect(Pc.AccentLight.copy(alpha = 0.25f), Offset(size.width * (inWhite / 255f), 0f), androidx.compose.ui.geometry.Size(size.width * (1f - inWhite / 255f), size.height))
            }
        }
        Spacer(Modifier.height(12.dp))
        SliderRow("Input black", inBlack, 0f..253f, { inBlack = it }) { apply() }
        SliderRow("Gamma", gamma, 0.1f..9.99f, { gamma = it }) { apply() }
        SliderRow("Input white", inWhite, 2f..255f, { inWhite = it }) { apply() }
        Spacer(Modifier.height(8.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            ActionButton("Apply as layer") {
                val p = JSONObject()
                p.put("inBlack", inBlack.toInt())
                p.put("gamma", gamma)
                p.put("inWhite", inWhite.toInt())
                vm.command("layer.newAdjustmentLayer.levels", p)
            }
            ActionButton("Reset") {
                inBlack = 0f
                gamma = 1f
                inWhite = 255f
                val p = JSONObject()
                p.put("inBlack", 0)
                p.put("gamma", 1.0)
                p.put("inWhite", 255)
                p.put("coalesce", "photocraft.levels.reset")
                vm.command("image.adjustments.levels", p)
            }
        }
    }
}

@Composable
private fun AdjustmentsPanel(vm: PhotoEditorViewModel, modifier: Modifier = Modifier) {
    PanelShell("Adjustments", vm::closePanel, modifier) {
        // The engine's own adjustment commands: the destructive ones (Image › Adjustments) and,
        // below them, the adjustment layers (Layer › New Adjustment Layer), which stay editable.
        val destructive = vm.commandsUnder("Image", "Adjustments")
        val layers = vm.commandsUnder("Layer", "New Adjustment Layer")
        CommandList(
            sections = listOf("Apply to the image" to destructive, "As an adjustment layer" to layers),
            onRun = { vm.command(it, JSONObject()) },
        )
    }
}

@Composable
private fun FiltersPanel(vm: PhotoEditorViewModel, modifier: Modifier = Modifier) {
    PanelShell("Filters", vm::closePanel, modifier) {
        // Grouped by the engine's own menu placement (Filter › Blur, Filter › Distort, …).
        val groups = vm.commandsUnder("Filter")
            .groupBy { it.menu.getOrNull(1) ?: "Other" }
            .toSortedMap()
        CommandList(
            sections = groups.map { (group, commands) -> group to commands },
            onRun = { vm.command(it, JSONObject()) },
        )
    }
}

@Composable
private fun ImagePanel(vm: PhotoEditorViewModel, modifier: Modifier = Modifier) {
    PanelShell("Image", vm::closePanel, modifier) {
        val groups = vm.commandsStartingWith("image.")
            .filter { !it.id.startsWith("image.adjustments") }
            .groupBy { it.menu.getOrNull(1) ?: "Image" }
            .toSortedMap()
        CommandList(
            sections = groups.map { (group, commands) -> group to commands },
            onRun = { vm.command(it, JSONObject()) },
        )
    }
}

/** One engine command per row: its label, greyed with its reason when it can't run now. */
@Composable
private fun CommandList(
    sections: List<Pair<String, List<CommandRow>>>,
    onRun: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val sections = sections.filter { it.second.isNotEmpty() }
    if (sections.isEmpty()) {
        Text("The engine listed no commands for this panel.", color = Pc.Detail, fontSize = 12.sp)
        return
    }
    LazyColumn(modifier.fillMaxWidth().height(340.dp)) {
        sections.forEach { (title, commands) ->
            item(key = "section-$title") {
                Text(
                    title,
                    color = Pc.Secondary,
                    fontSize = 11.sp,
                    fontWeight = FontWeight.Bold,
                    modifier = Modifier.padding(top = 12.dp, bottom = 4.dp),
                )
            }
            items(commands, key = { it.id }) { c ->
                Column(
                    Modifier.fillMaxWidth()
                        .clickable(enabled = c.enabled) { onRun(c.id) }
                        .padding(vertical = 8.dp),
                ) {
                    Text(
                        c.label,
                        color = if (c.enabled) Pc.Title else Pc.Disabled,
                        fontSize = 13.sp,
                    )
                    if (!c.enabled && c.disabledReason != null) {
                        Text(c.disabledReason, color = Pc.Detail, fontSize = 11.sp)
                    }
                }
            }
        }
    }
}

/** Foreground and background colours: swatches, RGB sliders and a hex field's worth of presets. */
@Composable
private fun ColourPanel(vm: PhotoEditorViewModel, modifier: Modifier = Modifier) {
    PanelShell("Colour", vm::closePanel, modifier) {
        var fg by remember { mutableStateOf(vm.foreground) }
        Column(Modifier.fillMaxWidth()) {
            // The red, green and blue of the colour being picked, as 0..255 each. The swatch
            // follows the finger; the engine hears it once, when the finger lifts.
            SliderRow("Red", android.graphics.Color.red(fg).toFloat(), 0f..255f, { v ->
                fg = android.graphics.Color.rgb(v.toInt(), android.graphics.Color.green(fg), android.graphics.Color.blue(fg))
            }) { vm.setForeground(fg) }
            SliderRow("Green", android.graphics.Color.green(fg).toFloat(), 0f..255f, { v ->
                fg = android.graphics.Color.rgb(android.graphics.Color.red(fg), v.toInt(), android.graphics.Color.blue(fg))
            }) { vm.setForeground(fg) }
            SliderRow("Blue", android.graphics.Color.blue(fg).toFloat(), 0f..255f, { v ->
                fg = android.graphics.Color.rgb(android.graphics.Color.red(fg), android.graphics.Color.green(fg), v.toInt())
            }) { vm.setForeground(fg) }
            Spacer(Modifier.height(8.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                // A row of plain colours, then the two swatches the tools use.
                // Ints (0xFF000000 alone would be a Long, which Color and setForeground reject).
                listOf(
                    0xFF000000.toInt(), 0xFFFFFFFF.toInt(), 0xFFE53935.toInt(), 0xFFFB8C00.toInt(),
                    0xFFFDD835.toInt(), 0xFF43A047.toInt(), 0xFF1E88E5.toInt(), 0xFF8E24AA.toInt(),
                )
                    .forEach { c ->
                        Box(
                            Modifier.size(28.dp).clip(RoundedCornerShape(6.dp)).background(Color(c))
                                .border(1.dp, Pc.Hairline, RoundedCornerShape(6.dp))
                                .clickable { fg = c; vm.setForeground(c) },
                        )
                    }
            }
            Spacer(Modifier.height(12.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                ActionButton("Foreground") { vm.setForeground(fg) }
                ActionButton("Background") { vm.setBackground(fg) }
                ActionButton("Swap") { vm.swapColors(); fg = vm.foreground }
            }
            Spacer(Modifier.height(8.dp))
            Text("The Eye dropper and the painting tools use the foreground.", color = Pc.Detail, fontSize = 11.sp)
        }
    }
}

@Composable
private fun TypePanel(vm: PhotoEditorViewModel, modifier: Modifier = Modifier) {
    // The bottom type bar, after the reference UI: font, size, B/I/U, alignment.
    Row(
        modifier.background(Pc.Toolbar.copy(alpha = 0.96f), RoundedCornerShape(16.dp))
            .border(1.dp, Pc.Hairline, RoundedCornerShape(16.dp))
            .padding(horizontal = 16.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("Instrument Serif", color = Pc.Title, fontSize = 14.sp)
        Text("14 pt", color = Pc.Secondary, fontSize = 12.sp)
        Text("B", color = Pc.Title, fontSize = 14.sp, fontWeight = FontWeight.Bold)
        Text("I", color = Pc.Title, fontSize = 14.sp, fontWeight = FontWeight.Light)
        Text("U", color = Pc.Title, fontSize = 14.sp)
        Spacer(Modifier.width(8.dp))
        ActionButton("Add text") {
            val p = JSONObject()
            p.put("x", 100)
            p.put("y", 100)
            p.put("text", "Text")
            p.put("size", 14)
            vm.command("type.create", p)
        }
        ActionButton("Done", vm::closePanel)
    }
}

@Composable
private fun PagesPanel(vm: PhotoEditorViewModel, modifier: Modifier = Modifier) {
    PanelShell("Pages", vm::closePanel, modifier) {
        val docs = vm.status?.documents ?: emptyList()
        LazyRow(Modifier.fillMaxWidth().height(120.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            items(docs) { d ->
                Column(
                    Modifier.width(96.dp).clip(RoundedCornerShape(8.dp))
                        .background(if (vm.status?.active?.index == d.index) Pc.Raised else Pc.Lane)
                        .clickable { vm.setActive(d.index.toInt()) }
                        .padding(8.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    Box(Modifier.fillMaxWidth().height(64.dp).background(Pc.Black, RoundedCornerShape(4.dp)))
                    Spacer(Modifier.height(6.dp))
                    Text(d.name, color = Pc.Title, fontSize = 11.sp, maxLines = 1)
                    Text("${d.width.toInt()}×${d.height.toInt()}", color = Pc.Detail, fontSize = 10.sp)
                }
            }
        }
        Spacer(Modifier.height(8.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            ActionButton("New") { vm.command("file.new", JSONObject().put("width", 1920).put("height", 1080)) }
            ActionButton("Close") { vm.status?.active?.let { vm.close(it.index.toInt()) } }
        }
    }
}

@Composable
private fun HistoryPanel(vm: PhotoEditorViewModel, modifier: Modifier = Modifier) {
    PanelShell("History", vm::closePanel, modifier) {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            ActionButton("Undo ${vm.status?.undoLabel ?: ""}") { vm.undo() }
            ActionButton("Redo ${vm.status?.redoLabel ?: ""}") { vm.redo() }
        }
        Spacer(Modifier.height(8.dp))
        Text(
            "Revision ${vm.status?.revision ?: 0} · ${if (vm.status?.dirty == true) "Unsaved changes" else "Saved"}",
            color = Pc.Secondary,
            fontSize = 12.sp,
        )
    }
}

@Composable
private fun ExportPanel(vm: PhotoEditorViewModel, onExport: (ByteArray, String) -> Unit, modifier: Modifier = Modifier) {
    PanelShell("Export", vm::closePanel, modifier) {
        // The shell owns the file system: ask the engine for the bytes and hand them to whoever
        // hosts the editor (the standalone app shares them; FilmCraft puts them on the timeline).
        val formats = listOf("png", "jpeg", "webp", "tiff", "psd", "bmp", "gif")
        LazyRow(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            items(formats) { f ->
                ActionButton(f.uppercase()) { vm.export(f) { bytes, format -> onExport(bytes, format) } }
            }
        }
        Spacer(Modifier.height(8.dp))
        Text(
            "Exports the active document as a flat file. Layered formats (PSD, TIFF) keep the layers.",
            color = Pc.Detail,
            fontSize = 11.sp,
        )
    }
}
