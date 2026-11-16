package io.github.artcraftandroid.photocraft.studio
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
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
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.launch
/**
 * The studio's panels: the reference's seven panel bodies and its secondary ones, in its own
 * layout and order.
 *
 * The reference's notes are replaced by what the studio really does. Where it said "document editing
 * is not enabled", the toolbar runs the engine; where it said "the portrait stays unchanged", the
 * curve becomes an adjustment layer; where it said export "is not included", the button exports.
 */
class StudioActions(
    val engine: StudioEngine,
    val state: StudioState,
    val accent: Color,
    val feedback: (String) -> Unit,
    val export: () -> Unit,
    val openDocument: () -> Unit,
    val saveDocument: () -> Unit,
    /** The classic PhotoCraft editor, for the panels the studio doesn't show. */
    val classicEditor: () -> Unit,
)
/** The panel for [name], with the frame the reference wraps every one in. */
@Composable
fun StudioPanel(name: String, a: StudioActions, onPin: () -> Unit, onClose: () -> Unit, modifier: Modifier = Modifier) {
    val s = a.state
    val pinned = s.isPinned(name)
    // The reference widens Curves and narrows Brushes and Pages.
    val width = when (name) {
        "Curves" -> 266.dp
        "Brushes" -> 210.dp
        "Pages" -> 235.dp
        else -> Sc.PanelWidth
    }
    PanelFrame(name, pinned = pinned, onPin = onPin, onClose = onClose, modifier = modifier.width(width)) {
        when (name) {
            "Layers" -> LayersBody(a)
            "Curves" -> CurvesBody(a)
            "Pages" -> PagesBody(a)
            "Brushes" -> BrushesBody(a)
            else -> SecondaryBody(name, a)
        }
    }
}
// ---------------------------------------------------------------------------- Layers
@Composable
private fun LayersBody(a: StudioActions) {
    val s = a.state
    val e = a.engine
    val scope = rememberCoroutineScope()
    val live = e.layers.isNotEmpty()
    PanelToolbar {
        IconButton("more", "Layer actions", small = true) {
            if (live) a.feedback("${e.layers.size} layers in this document.") else a.feedback("Open a picture to inspect its layers.")
        }
        IconButton("mask", "Add mask", small = true) {
            scope.engine { e.addMask(); a.feedback("A layer mask was added.") }
        }
        IconButton("duplicate", "Duplicate layer", small = true) {
            scope.engine { e.duplicateLayer(); a.feedback("Layer duplicated.") }
        }
        IconButton("layers", "Add layer group", small = true) {
            scope.engine { e.addGroup(); a.feedback("Layer group added.") }
        }
        IconButton("trash", "Delete layer", small = true) {
            scope.engine { e.deleteLayer(); a.feedback("Layer deleted.") }
        }
    }
    // `.layer-options`: opacity pill, blend select, options.
    Row(
        Modifier.fillMaxWidth().background(Color(0xFF222321)).padding(horizontal = 7.dp, vertical = 7.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(5.dp),
    ) {
        val active = e.layers.firstOrNull { it.active }
        NumberField(
            value = active?.let { (it.opacity * 100).toInt().toString() } ?: s.value("opacity"),
            label = "Layer opacity",
            suffix = "%",
        ) { text ->
            s.setValue("opacity", text)
            text.toIntOrNull()?.let { pct -> scope.engine { e.setOpacity(pct) } }
        }
        SelectField(
            label = "Blend mode",
            value = active?.blend?.takeIf { it.isNotEmpty() } ?: s.value("blend"),
            options = StudioBlends,
            modifier = Modifier.weight(1f),
        ) { mode ->
            s.setValue("blend", mode)
            scope.engine { e.setBlend(mode) }
        }
        IconButton("settings", "Layer options", small = true) { a.feedback("Layer locks follow the document.") }
    }
    PanelDivider()
    Column(Modifier.fillMaxWidth().heightIn(max = 420.dp).verticalScroll(rememberScrollState())) {
        if (live) {
            e.layers.forEach { row -> LiveLayerRow(row, a) }
        } else {
            MockLayers.filter { row ->
                if (row.parent != null && row.parent in s.collapsed) return@filter false
                if (row.id in setOf("stem", "highlights", "shadows", "stripes", "gradient", "mushroom", "flower") &&
                    "illustration" in s.collapsed
                ) return@filter false
                true
            }.forEach { row -> MockLayerRow(row, a) }
        }
    }
    PanelFooter(if (live) "${e.layers.size} layers" else "${MockLayers.size} layers", if (live) "PhotoCraft" else "Reference")
}
@Composable
private fun LiveLayerRow(row: StudioLayer, a: StudioActions) {
    val s = a.state
    val scope = rememberCoroutineScope()
    val selected = row.active
    val hidden = !row.visible
    val collapsed = row.name in s.collapsed
    Row(
        Modifier.fillMaxWidth().height(34.dp)
            .background(if (selected) Sc.Blue else Color.Transparent)
            .clickable { scope.engine { a.engine.selectLayer(row.id) } },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.width((7 + row.depth * 5).dp).height(34.dp).background(Color(0x17181566)))
        if (row.group) {
            Box(
                Modifier.size(width = 12.dp, height = 26.dp).clickable {
                    if (collapsed) s.collapsed.remove(row.name) else s.collapsed.add(row.name)
            },
                contentAlignment = Alignment.Center,
            ) {
                StudioIcons.Glyph("chevron", if (selected) Color.White else Sc.IconInk, 11.dp, Modifier.rotate(if (collapsed) 0f else 90f))
        }
        } else {
            Spacer(Modifier.width(7.dp))
        }
        Row(Modifier.weight(1f), verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.size(width = 26.dp, height = 29.dp), contentAlignment = Alignment.Center) {
                StudioIcons.Glyph(
                    when {
                        row.group -> "layers"
                        row.kind.contains("text", true) -> "text"
                        row.kind.contains("adjust", true) -> "contrast"
                        else -> "image"
                },
                    if (selected) Color.White else Sc.ThumbInk,
                    21.dp,
                )
        }
            Text(
                row.name,
                fontSize = 10.sp,
                color = if (selected) Color.White else Sc.Title,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.alpha(if (hidden) 0.3f else 1f),
            )
        }
        Box(
            Modifier.size(width = 17.dp, height = 27.dp).clickable {
                scope.engine { a.engine.setLocked(row.id, row.name !in s.locked) }
                if (row.name in s.locked) s.locked.remove(row.name) else s.locked.add(row.name)
        },
            contentAlignment = Alignment.Center,
        ) {
            StudioIcons.Glyph(if (row.name in s.locked) "lock" else "unlock", Sc.IconInk.copy(alpha = 0.6f), 11.dp)
        }
        Box(
            Modifier.size(width = 24.dp, height = 27.dp).clickable {
                scope.engine { a.engine.setVisible(row.id, !row.visible) }
        },
            contentAlignment = Alignment.Center,
        ) {
            StudioIcons.Glyph(if (row.visible) "eye" else "hidden", Color(0xFFECEEE6), 13.dp)
        }
    }
    Box(Modifier.fillMaxWidth().height(1.dp).background(Color(0x88171815)))
}
@Composable
private fun MockLayerRow(row: MockLayer, a: StudioActions) {
    val s = a.state
    val selected = s.selectedLayer == row.id
    val hidden = row.id in s.hidden
    val collapsed = row.id in s.collapsed
    Row(
        Modifier.fillMaxWidth().height(34.dp)
            .background(if (selected) Sc.Blue else Color.Transparent)
            .clickable { s.selectedLayer = row.id },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.width((7 + row.depth * 5).dp).height(34.dp).background(Color(0x17181566)))
        if (row.group) {
            Box(
                Modifier.size(width = 12.dp, height = 26.dp).clickable {
                    if (collapsed) s.collapsed.remove(row.id) else s.collapsed.add(row.id)
            },
                contentAlignment = Alignment.Center,
            ) {
                StudioIcons.Glyph("chevron", if (selected) Color.White else Sc.IconInk, 11.dp, Modifier.rotate(if (collapsed) 0f else 90f))
        }
        } else {
            Spacer(Modifier.width(7.dp))
        }
        Row(Modifier.weight(1f), verticalAlignment = Alignment.CenterVertically) {
            Thumb(row, selected)
            Text(
                row.name,
                fontSize = 10.sp,
                color = if (selected) Color.White else Sc.Title,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.alpha(if (hidden) 0.3f else 1f),
            )
        }
        Box(
            Modifier.size(width = 17.dp, height = 27.dp).clickable {
                if (row.id in s.locked) s.locked.remove(row.id) else s.locked.add(row.id)
        },
            contentAlignment = Alignment.Center,
        ) {
            StudioIcons.Glyph(
                if (row.id in s.locked) "lock" else "unlock",
                Sc.IconInk.copy(alpha = if (row.id in s.locked) 0.6f else 0.15f),
                11.dp,
            )
        }
        Box(
            Modifier.size(width = 24.dp, height = 27.dp).clickable {
                if (hidden) s.hidden.remove(row.id) else s.hidden.add(row.id)
        },
            contentAlignment = Alignment.Center,
        ) {
            StudioIcons.Glyph(if (hidden) "hidden" else "eye", Color(0xFFECEEE6), 13.dp)
        }
    }
    Box(Modifier.fillMaxWidth().height(1.dp).background(Color(0x88171815)))
}
/** The reference's `.layer-thumb` variants: each row's own little picture. */
@Composable
private fun Thumb(row: MockLayer, selected: Boolean) {
    val size = Modifier.size(width = 26.dp, height = 29.dp)
    when (row.id) {
        "illustration" -> Box(
            size.clip(RoundedCornerShape(14.dp)).background(
                androidx.compose.ui.graphics.Brush.sweepGradient(
                    listOf(Color(0xFF10DF68), Color(0xFFEBB3FF), Color(0xFFFFDF19), Color(0xFFFF3068), Color(0xFF10DF68)),
                ),
            ),
            contentAlignment = Alignment.Center,
        ) { StudioIcons.Glyph(row.kind, Color(0xFFFF7797), 21.dp) }
        "stem", "flower" -> Box(
            size.background(
                androidx.compose.ui.graphics.Brush.linearGradient(
                    listOf(Color.Transparent, Color(0xFFFF387B), Color(0xFFFFB333), Color(0xFFFF296B), Color.Transparent),
                    start = Offset(0f, 0f),
                    end = Offset(26f, 29f),
                ),
            ),
            contentAlignment = Alignment.Center,
        ) { StudioIcons.Glyph(row.kind, Color(0xFFFFB336), 21.dp) }
        "mushroom" -> Box(
            size.background(
                androidx.compose.ui.graphics.Brush.radialGradient(
                    listOf(Color(0xFFFFE931), Color(0xFF14E564)),
                    radius = 16f,
                ),
            ),
            contentAlignment = Alignment.Center,
        ) { StudioIcons.Glyph(row.kind, Color(0xFF112D19), 21.dp) }
        "mask" -> Box(
            size.background(
                androidx.compose.ui.graphics.Brush.horizontalGradient(
                    0f to Color.Black, 0.42f to Color.Black, 0.46f to Color.White,
                    0.7f to Color.White, 0.75f to Color.Black, 1f to Color.Black,
                ),
            ),
        )
        else -> Box(size, contentAlignment = Alignment.Center) {
            StudioIcons.Glyph(
                row.kind,
                if (row.id == "pixel") Color.White else if (row.id == "gradient") Color(0xFFFF1773) else if (selected) Color.White else Sc.ThumbInk,
                21.dp,
            )
        }
    }
}
// ---------------------------------------------------------------------------- Curves
@Composable
private fun CurvesBody(a: StudioActions) {
    val s = a.state
    val e = a.engine
    val scope = rememberCoroutineScope()
    var selected by remember { mutableStateOf(1) }
    // `.curves-options`: two selects over two dials.
    Row(Modifier.fillMaxWidth().padding(start = 10.dp, end = 10.dp, top = 10.dp, bottom = 5.dp), horizontalArrangement = Arrangement.spacedBy(7.dp)) {
        Column(Modifier.width(77.dp)) {
            SelectField(
                label = "Colour space",
                value = s.value("colourSpace"),
                options = listOf("RGB", "CMYK", "LAB", "Greyscale"),
                height = 22.dp,
                fontSize = 10,
                onSelect = { s.setValue("colourSpace", it) },
            )
            Text("Colour space", fontSize = 9.sp, color = Sc.Note, modifier = Modifier.fillMaxWidth().padding(vertical = 3.dp), textAlign = androidx.compose.ui.text.style.TextAlign.Center)
            SelectField(
                label = "Curve channel",
                value = s.value("channel"),
                options = listOf("Master", "Red", "Green", "Blue", "Alpha"),
                height = 22.dp,
                fontSize = 10,
                onSelect = { s.setValue("channel", it) },
            )
            Text("Channels", fontSize = 9.sp, color = Sc.Note, modifier = Modifier.fillMaxWidth().padding(vertical = 3.dp), textAlign = androidx.compose.ui.text.style.TextAlign.Center)
        }
        Dial((s.points.getOrNull(1)?.x ?: 0f).toInt().toString(), "Input minimum")
        Dial(s.value("opacity"), "Opacity", editable = true) { s.setValue("opacity", it) }
    }
    Row(Modifier.fillMaxWidth().padding(end = 10.dp, bottom = 7.dp), horizontalArrangement = Arrangement.spacedBy(9.dp, Alignment.End)) {
        IconButton("reset", "Reset adjustment", small = true) {
            s.points = DefaultPoints
            s.setValue("opacity", "100")
            selected = 1
        }
        IconButton("eye", "Preview adjustment", small = true, active = s.isOn("curvePreview")) { s.toggle("curvePreview") }
        IconButton("trash", "Remove adjustment", small = true) {
            scope.engine { e.deleteLayer(); a.feedback("The adjustment layer was removed.") }
        }
        IconButton("plus", "Add adjustment", small = true) {
            scope.engine {
                e.applyCurves(s.points, s.value("channel"), s.value("opacity").toIntOrNull() ?: 100)
                a.feedback("The curve was added as an adjustment layer.")
        }
        }
    }
    CurveEditor(s, selected, onSelect = { selected = it })
    PanelNote("The curve is applied to the document as an adjustment layer.")
}
/** The reference's `.dial-control`: a 42 px ring with its reading. */
@Composable
private fun Dial(value: String, caption: String, editable: Boolean = false, onChange: ((String) -> Unit)? = null) {
    Column(
        Modifier.width(74.dp).height(83.dp).clip(RoundedCornerShape(8.dp)).background(Color(0xFF1D1E1B)),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        Box(
            Modifier.size(42.dp).clip(RoundedCornerShape(21.dp))
                .border(
                    if (editable) 2.dp else 2.dp,
                    if (editable) Sc.Blue else Color(0xFF43483E),
                    RoundedCornerShape(21.dp),
                ),
            contentAlignment = Alignment.Center,
        ) {
            if (editable && onChange != null) {
                BasicNumber(value, onChange, width = 30.dp)
            } else {
                Text(value, fontSize = 10.sp, color = Color(0xFFB4B8AD))
            }
        }
        Spacer(Modifier.height(10.dp))
        Text(caption, fontSize = 8.sp, color = Sc.Title, maxLines = 1)
    }
}
@Composable
private fun BasicNumber(value: String, onChange: (String) -> Unit, width: androidx.compose.ui.unit.Dp) {
    androidx.compose.foundation.text.BasicTextField(
        value = value,
        onValueChange = onChange,
        singleLine = true,
        textStyle = androidx.compose.ui.text.TextStyle(
            fontSize = 9.sp,
            color = Color(0xFFB4B8AD),
            textAlign = androidx.compose.ui.text.style.TextAlign.Center,
        ),
        cursorBrush = androidx.compose.ui.graphics.SolidColor(Sc.Blue),
        modifier = Modifier.width(width),
    )
}
/** The reference's `.curve-editor`: graph, side fields, hint. */
@Composable
private fun CurveEditor(s: StudioState, selected: Int, onSelect: (Int) -> Unit) {
    // `.curve-editor`: 266 wide, hung 87 px to the left of the panel, so its graph sits over the
    // document rather than inside the panel's own column.
    Column(
        Modifier.width(266.dp).offset(x = (-87).dp)
            .clip(RoundedCornerShape(11.dp)).background(Color(0xFF20211F))
            .border(1.dp, Color(0xFF111111), RoundedCornerShape(11.dp))
            .padding(start = 8.dp, end = 8.dp, top = 9.dp, bottom = 6.dp),
    ) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(7.dp)) {
            CurveGraph(
                points = s.points,
                selected = selected,
                modifier = Modifier.weight(1f),
                onSelect = onSelect,
                onMove = { index, x, y ->
                    // The reference clamps each point between its neighbours.
                    val pts = s.points
                    val low = if (index == 0) 0f else pts[index - 1].x + 1f
                    val high = if (index == pts.lastIndex) 100f else pts[index + 1].x - 1f
                    s.points = pts.mapIndexed { i, p ->
                        if (i == index) CurvePoint(x.coerceIn(low, high), y.coerceIn(0f, 100f)) else p
                }
            },
            )
            Column(Modifier.width(35.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                IconButton("reset", "Reset curve", small = true) {
                    s.points = DefaultPoints
                    onSelect(1)
            }
                val point = s.points.getOrNull(selected) ?: DefaultPoints.last()
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                    Text("X", fontSize = 8.sp, color = Sc.Footer)
                    Box(
                        Modifier.width(33.dp).height(22.dp).clip(RoundedCornerShape(11.dp)).background(Color(0xFF1B1C19)),
                        contentAlignment = Alignment.Center,
                    ) {
                        BasicNumber(point.x.toInt().toString(), { v ->
                            v.toFloatOrNull()?.let { n ->
                                s.points = s.points.mapIndexed { i, p -> if (i == selected) p.copy(x = n.coerceIn(0f, 100f)) else p }
                        }
                    }, width = 29.dp)
                }
            }
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                    Text("Y", fontSize = 8.sp, color = Sc.Footer)
                    Box(
                        Modifier.width(33.dp).height(22.dp).clip(RoundedCornerShape(11.dp)).background(Color(0xFF1B1C19)),
                        contentAlignment = Alignment.Center,
                    ) {
                        BasicNumber((100f - point.y).toInt().toString(), { v ->
                            v.toFloatOrNull()?.let { n ->
                                s.points = s.points.mapIndexed { i, p -> if (i == selected) p.copy(y = 100f - n.coerceIn(0f, 100f)) else p }
                        }
                    }, width = 29.dp)
                }
            }
        }
        }
        Text(
            "Drag a point to shape the curve",
            fontSize = 8.sp,
            color = Color(0xFF777C71),
            modifier = Modifier.fillMaxWidth().padding(top = 6.dp),
            textAlign = androidx.compose.ui.text.style.TextAlign.Center,
        )
    }
}
/** The 199 px curve graph: the reference's grid, its three histograms and its draggable points. */
@Composable
private fun CurveGraph(
    points: List<CurvePoint>,
    selected: Int,
    modifier: Modifier = Modifier,
    onSelect: (Int) -> Unit,
    onMove: (Int, Float, Float) -> Unit,
) {
    var dragging by remember { mutableStateOf(-1) }
    var boxSize by remember { mutableStateOf(IntSize.Zero) }
    Canvas(
        modifier.height(199.dp).border(1.dp, Color(0xFF65685E))
            .onSizeChanged { boxSize = it }
            .pointerInput(points) {
                val w = boxSize.width.toFloat().coerceAtLeast(1f)
                val h = boxSize.height.toFloat().coerceAtLeast(1f)
                detectDragGestures(
                    onDragStart = { pos ->
                        val near = points.indices.minByOrNull { i ->
                            val px = points[i].x / 100f * w
                            val py = points[i].y / 100f * h
                            (px - pos.x) * (px - pos.x) + (py - pos.y) * (py - pos.y)
                        } ?: -1
                        dragging = near
                        if (near >= 0) onSelect(near)
                    },
                    onDragEnd = { dragging = -1 },
                    onDragCancel = { dragging = -1 },
                ) { change, _ ->
                    if (dragging >= 0) {
                        onMove(dragging, change.position.x / w * 100f, change.position.y / h * 100f)
                    }
                    change.consume()
                }
            },
    ) {
        val w = size.width
        val h = size.height
        drawRect(Color(0xFF191B1A))
        // The reference's grid: its 200-unit view box divided in eight.
        for (n in 1..7) {
            val x = w * n / 8f
            val y = h * n / 8f
            drawLine(Color(0xFF4E514F), Offset(x, 0f), Offset(x, h), 0.5f)
            drawLine(Color(0xFF4E514F), Offset(0f, y), Offset(w, y), 0.5f)
        }
        // The three histograms: red, green, blue.
        histogram(w, h, seed = 72f, fill = Color(0xFFC73536).copy(alpha = 0.6f), stroke = Color(0xFFE84949))
        histogram(w, h, seed = 55f, fill = Color(0xFF2D9240).copy(alpha = 0.45f), stroke = Color(0xFF6BB047))
        histogram(w, h, seed = 43f, fill = Color(0xFF2743B8).copy(alpha = 0.55f), stroke = Color(0xFF4361E1))
        // The identity line, then the curve through the points.
        drawLine(Color(0xFF747674), Offset(0f, 0f), Offset(w, h), 0.7f)
        val path = Path()
        points.forEachIndexed { i, p ->
            val px = p.x / 100f * w
            val py = p.y / 100f * h
            if (i == 0) path.moveTo(px, py) else path.lineTo(px, py)
        }
        drawPath(path, Color(0xFFFAFAFA), style = Stroke(width = 1.4f))
        points.forEachIndexed { i, p ->
            val px = p.x / 100f * w
            val py = p.y / 100f * h
            val r = if (i == selected) 8f else 6f
            drawCircle(Color.White, r, Offset(px, py), style = Stroke(1.4f))
            if (i == selected) drawCircle(Sc.Blue.copy(alpha = 0.9f), r - 1.5f, Offset(px, py))
        }
    }
}
/** The reference's histogram path, at the canvas' size. */
private fun androidx.compose.ui.graphics.drawscope.DrawScope.histogram(
    w: Float,
    h: Float,
    seed: Float,
    fill: Color,
    stroke: Color,
) {
    val path = Path()
    path.moveTo(0f, h)
    for (i in 0..100) {
        val peak = kotlin.math.exp(-((i - seed) * (i - seed)) / 300f) * 65f +
            kotlin.math.exp(-((i - 76f) * (i - 76f)) / 30f) * 85f
        val n = ((i * 17 + seed.toInt() * 13) % 29) / 29f
        val x = i / 100f * w
        val y = h - (peak * (0.1f + n)) / 200f * h
        path.lineTo(x, y)
    }
    path.lineTo(w, h)
    path.close()
    drawPath(path, fill)
    drawPath(path, stroke, style = Stroke(width = 0.7f))
}
// ---------------------------------------------------------------------------- Pages
@Composable
private fun PagesBody(a: StudioActions) {
    val s = a.state
    val scope = rememberCoroutineScope()
    val spreads = listOf(
        Triple("studio/reference/pages-2.webp", 2, "Nora Toth"),
        Triple("studio/reference/pages-4.webp", 4, "Paloma Rincon"),
        Triple("studio/reference/pages-6.webp", 6, "Marrak"),
        Triple("studio/reference/layout.webp", 8, "Isabela Humphrey"),
    )
    // `.pages-navigation`: the two chevrons and the page count.
    Row(
        Modifier.fillMaxWidth().padding(horizontal = 7.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        IconButton("chevron", "Previous spread", small = true) { s.page = (s.page - 2).coerceAtLeast(2) }
        Text("Pages", fontSize = 11.sp, color = Sc.Title)
        IconButton("chevron", "Next spread", small = true) { s.page = (s.page + 2).coerceAtMost(8) }
    }
    // The document's own artboards, read once for the toolbar and the list below.
    val pages = a.engine.artboards
    Row(Modifier.fillMaxWidth().heightIn(min = 30.dp).padding(horizontal = 11.dp), verticalAlignment = Alignment.CenterVertically) {
        // A page is an artboard: the + puts the next one beside the active one, as the reference's
        // page grid would, and the duplicate copies it with its contents.
        IconButton("plus", "Add page", small = true) {
            scope.engine {
                val made = runCatching { a.engine.newArtboard() }.getOrNull()
                if (made != null) a.feedback("A page was added beside the active one.") else a.feedback("No document to add a page to.")
            }
        }
        Spacer(Modifier.width(15.dp))
        IconButton("duplicate", "Duplicate page", small = true) {
            scope.engine {
                runCatching { a.engine.addArtboardBeside() }
                    .onSuccess { a.feedback("The page was duplicated beside itself.") }
                    .onFailure { a.feedback(it.message ?: "Nothing to duplicate.") }
            }
        }
        Spacer(Modifier.width(15.dp))
        IconButton("trash", "Delete page", small = true) {
            scope.engine {
                // A page is the active artboard's group layer: the engine deletes what is active,
                // so the notice says which page went rather than claiming it was always one.
                val page = pages.firstOrNull { board -> board.id == a.engine.activeLayer }
                runCatching { a.engine.deleteLayer() }
                    .onSuccess {
                        a.feedback(
                            if (page != null) "“${page.name}” was deleted." else "The active layer was deleted.",
                        )
                    }
                    .onFailure { a.feedback(it.message ?: "Nothing to delete.") }
            }
        }
        Spacer(Modifier.weight(1f))
        IconButton("more", "Page options", small = true) { s.toggle("managePages") }
    }
    // The document's own artboards first: a page in this studio is an artboard, and the reference's
    // spreads stand in only while the document has none.
    if (pages.isNotEmpty()) {
        Column(Modifier.fillMaxWidth().heightIn(max = 140.dp).verticalScroll(rememberScrollState()).padding(horizontal = 11.dp)) {
            pages.forEach { board ->
                Row(
                    Modifier.fillMaxWidth().padding(vertical = 3.dp).clickable {
                        scope.engine {
                            a.engine.selectArtboard(board.id)
                            a.feedback("“${board.name}” is the active page.")
                        }
                    },
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    StudioIcons.Glyph("pages", Sc.Detail, 12.dp)
                    Spacer(Modifier.width(6.dp))
                    Text(board.name, fontSize = 10.sp, color = Sc.Title, maxLines = 1)
                    Spacer(Modifier.weight(1f))
                    Text("${board.w}×${board.h}", fontSize = 9.sp, color = Sc.Detail)
                }
            }
        }
        PanelDivider()
    }
    Column(Modifier.fillMaxWidth().heightIn(max = 320.dp).verticalScroll(rememberScrollState()).padding(horizontal = 11.dp)) {
        spreads.forEach { (path, page, title) ->
            Column(
                Modifier.fillMaxWidth().padding(top = 2.dp, bottom = 12.dp)
                    .clickable { s.page = page },
            ) {
                ReferenceAsset(
                    path = path,
                    modifier = Modifier.fillMaxWidth().height(96.dp)
                        .border(2.dp, if (s.page == page) Color(0xFF3597FF) else Color.Transparent),
                    description = "$title editorial spread",
                    contentScale = androidx.compose.ui.layout.ContentScale.Crop,
                )
                Text(
                    "Pages $page,${page + 1}",
                    fontSize = 9.sp,
                    color = Color(0xFFB8BBAE),
                    modifier = Modifier.fillMaxWidth().padding(top = 8.dp),
                    textAlign = androidx.compose.ui.text.style.TextAlign.Center,
                )
        }
        }
    }
    DisclosureRow("Manage Pages", s.isOn("managePages")) { s.toggle("managePages") }
    if (s.isOn("managePages")) ExpandedSection("8 pages · 4 spreads", "Facing pages · Portrait")
    DisclosureRow("Sections", s.isOn("sections")) { s.toggle("sections") }
    if (s.isOn("sections")) ExpandedSection("Section 1 — Showcase", "Pages 2–9")
}
// ---------------------------------------------------------------------------- Brushes
@Composable
private fun BrushesBody(a: StudioActions) {
    val s = a.state
    val e = a.engine
    val scope = rememberCoroutineScope()
    // The drawing app's shelf: its sets, each with its brushes. The engine's preset list is the
    // library, so a brush saved, renamed or deleted in the studio is in the next shelf.
    val sets = e.brushSets
    val shipped = remember(sets) { sets.map { it.name } }
    val categories = shipped.ifEmpty { listOf("Sketching", "Inking", "Painting", "All Brushes") } + "All Brushes"
    // Where a preset with no set of its own is filed: the library's group, or "Custom".
    val all = remember(sets) { sets.flatMap { set -> set.brushes.map { it to set.name } } }
    val shown = all.filter { (brush, set) ->
        val category = s.category
        val inSet = category == "All Brushes" || category == set
        val named = s.search.isEmpty() || brush.name.contains(s.search, ignoreCase = true)
        inSet && named
    }
    // `.brush-search`
    Row(
        Modifier.fillMaxWidth().padding(horizontal = 9.dp, vertical = 8.dp).height(27.dp)
            .clip(RoundedCornerShape(5.dp)).background(Color(0xFF1F201E)).padding(horizontal = 7.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        StudioIcons.Glyph("search", Color(0xFF989C8F), 13.dp)
        Spacer(Modifier.width(5.dp))
        androidx.compose.foundation.text.BasicTextField(
            value = s.search,
            onValueChange = { s.search = it },
            singleLine = true,
            textStyle = androidx.compose.ui.text.TextStyle(fontSize = 10.sp, color = Sc.Title),
            cursorBrush = androidx.compose.ui.graphics.SolidColor(Sc.Blue),
            modifier = Modifier.weight(1f),
            decorationBox = { inner ->
                if (s.search.isEmpty()) Text("Find a brush", fontSize = 10.sp, color = Color(0xFF84887D))
                inner()
        },
        )
        if (s.search.isNotEmpty()) {
            Box(Modifier.clickable { s.search = "" }) { StudioIcons.Glyph("close", Color(0xFF989C8F), 12.dp) }
        }
    }
    SelectField(
        label = "Brush category",
        value = s.category,
        options = categories,
        modifier = Modifier.fillMaxWidth().padding(horizontal = 9.dp).height(25.dp),
    ) { s.category = it }
    Column(Modifier.fillMaxWidth().heightIn(max = 355.dp).verticalScroll(rememberScrollState())) {
        shown.forEach { (brush, set) ->
            val on = s.brush == brush.name
            Column(
                Modifier.fillMaxWidth().height(49.dp)
                    .background(if (on) Color(0x07FFFFFF) else Color.Transparent)
                    .border(1.dp, if (on) Color(0xFF358ED9) else Color.Transparent)
                    .clickable {
                        s.brush = brush.name
                        s.setValue("brushSize", brush.size.toInt().coerceAtLeast(1).toString())
                        scope.engine {
                            // The preset brings its own tip, dynamics and stabilisation: the
                            // studio's brush is the drawing app's brush.
                            e.setBrush(preset = brush.name)
                            s.setValue("brushSize", (e.brushValue("size").toFloatOrNull() ?: brush.size).toInt().toString())
                        }
                }
                    .padding(horizontal = 10.dp),
                verticalArrangement = Arrangement.Center,
            ) {
                // The engine's own stroke with this brush, not a stand-in line.
                val preview = remember(brush.name) { mutableStateOf<android.graphics.Bitmap?>(null) }
                LaunchedEffect(brush.name) {
                    val bmp = e.brushPreview(brush.name, 180, 34)
                    if (bmp != null) preview.value = bmp
                }
                val bmp = preview.value
                if (bmp != null) {
                    androidx.compose.foundation.Image(
                        bitmap = bmp.asImageBitmap(),
                        contentDescription = null,
                        modifier = Modifier.fillMaxWidth().height(31.dp),
                        contentScale = androidx.compose.ui.layout.ContentScale.Fit,
                    )
                } else {
                    BrushStrokeSample(0, Modifier.fillMaxWidth().height(31.dp))
                }
                Text(brush.name, fontSize = 9.sp, color = Color(0xFFB9BEAF), maxLines = 1)
        }
        }
        if (shown.isEmpty()) {
            Column(
                Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 26.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                Text(
                    if (s.search.isNotEmpty()) "No brushes match \u201C${s.search}\u201D." else "This set is empty.",
                    fontSize = 11.sp,
                    color = Color(0xFFA8AF9A),
                )
                Spacer(Modifier.height(8.dp))
                Text(
                    "Show all brushes",
                    fontSize = 11.sp,
                    color = Color(0xFF7AB8FF),
                    modifier = Modifier.clickable { s.search = ""; s.category = "All Brushes" },
                )
        }
        }
    }
    // `.brush-controls`: width and opacity, pill and slider.
    Column(
        Modifier.fillMaxWidth().background(Color.Transparent)
            .border(width = 0.dp, color = Color.Transparent)
            .padding(start = 12.dp, end = 12.dp, top = 10.dp, bottom = 11.dp),
    ) {
        // `.brush-controls > label` is 9 px, and its number field 22 px tall.
        FieldRow("Width", labelSize = 9) {
            NumberField(s.value("brushSize"), "Brush width", suffix = "px", height = 22.dp) { v -> s.setValue("brushSize", v) }
        }
        StudioSlider(
            fraction = ((s.value("brushSize").toFloatOrNull() ?: 24f) - 1f) / 499f,
            modifier = Modifier.fillMaxWidth(),
        ) { f ->
            val px = (1f + f * 499f).toInt()
            s.setValue("brushSize", px.toString())
            scope.engine { e.setBrush(size = px.toFloat()) }
        }
        FieldRow("Opacity", labelSize = 9) {
            NumberField(s.value("brushOpacity"), "Brush opacity", suffix = "%", height = 22.dp) { v -> s.setValue("brushOpacity", v) }
        }
        StudioSlider(
            fraction = (s.value("brushOpacity").toFloatOrNull() ?: 100f) / 100f,
            modifier = Modifier.fillMaxWidth(),
        ) { f ->
            val pct = (f * 100f).toInt()
            s.setValue("brushOpacity", pct.toString())
            scope.engine { e.setBrush(opacity = pct / 100f) }
        }
    }
}
/**
 * `.brush-preset svg`: a stroke sample drawn in Compose, for the moment before the engine's own
 * preview answers (or when it cannot be drawn). The shelf's thumbnails are the engine's strokes.
 */
@Composable
private fun BrushStrokeSample(index: Int, modifier: Modifier = Modifier) {
    Canvas(modifier) {
        val w = size.width
        val h = size.height
        val width = 2f + (index % 5) * 2f
        val path = Path().apply {
            moveTo(w * 0.015f, h * 0.72f)
            quadraticBezierTo(w * 0.26f, h * 0.15f, w * 0.5f, h * 0.47f)
            quadraticBezierTo(w * 0.75f, h * 0.79f, w * 0.985f, h * 0.28f)
        }
        drawPath(
            path,
            Color(0xFFEEEEEE).copy(alpha = 0.85f),
            style = Stroke(width = width, cap = StrokeCap.Round),
        )
        if (index < 5) {
            val ghost = Path().apply {
                moveTo(w * 0.03f, h * 0.81f)
                quadraticBezierTo(w * 0.26f, h * 0.22f, w * 0.52f, h * 0.59f)
                quadraticBezierTo(w * 0.76f, h * 0.96f, w * 0.985f, h * 0.37f)
            }
            drawPath(ghost, Color(0xFFAAAAAA), style = Stroke(width = 0.6f))
        }
    }
}
// ---------------------------------------------------------------------------- Colour, Text, secondary
@Composable
private fun SecondaryBody(name: String, a: StudioActions) {
    val s = a.state
    when (name) {
        "Colour", "Swatches" -> ColourBody(a)
        "Text", "Paragraph" -> SecondaryContent {
            TextControls(a, compact = false)
            Text("Text formatting goes to the text layer.", fontSize = 10.sp, lineHeight = 16.sp, color = Sc.Note)
        }
        "Stroke" -> StrokeBody(a)
        "Effects" -> EffectsBody(a)
        "Transform" -> TransformBody(a)
        "Navigator" -> NavigatorBody(a)
        "History" -> HistoryBody(a)
        "Export" -> ExportBody(a)
        else -> SecondaryContent {
            Text("No assets in this document.", fontSize = 10.sp, lineHeight = 16.sp, color = Sc.Note)
            StandardButton("Import assets…") { a.feedback("Pictures are placed with Place Image.") }
        }
    }
}
/** `.colour-content`: the wheel, the value row and the twelve swatches. */
@Composable
private fun ColourBody(a: StudioActions) {
    val s = a.state
    val scope = rememberCoroutineScope()
    Column(Modifier.fillMaxWidth().padding(start = 12.dp, end = 12.dp, top = 15.dp)) {
        Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
            ColourWheel(parseColour(s.value("colour")))
        }
        Spacer(Modifier.height(16.dp))
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            // The reference's colour input, as a tappable well.
            Box(
                Modifier.size(width = 30.dp, height = 27.dp).clip(RoundedCornerShape(4.dp))
                    .background(parseColour(s.value("colour")))
                    .border(1.dp, Color(0x33FFFFFF), RoundedCornerShape(4.dp))
                    .clickable { scope.engine { a.engine.setColours(foreground = s.value("colour")) } },
            )
            Text(s.value("colour").uppercase(), fontSize = 11.sp, color = Sc.Title)
            Text("RGB/8", fontSize = 9.sp, color = Sc.Faded, modifier = Modifier.padding(start = 10.dp))
        }
        Spacer(Modifier.height(14.dp))
        // `.colour-swatches`: six across, round.
        StudioSwatches.chunked(6).forEach { row ->
            Row(Modifier.fillMaxWidth().padding(bottom = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                row.forEach { c ->
                    val on = s.value("colour") == c
                    Box(
                        Modifier.weight(1f).height(26.dp).clip(RoundedCornerShape(13.dp)).background(parseColour(c))
                            .border(1.dp, Color(0x26FFFFFF), RoundedCornerShape(13.dp))
                            .clickable {
                                s.setValue("colour", c)
                                scope.engine { a.engine.setColours(foreground = c) }
                        },
                        contentAlignment = Alignment.Center,
                    ) {
                        if (on) {
                            StudioIcons.Glyph(
                                "check",
                                if (c in setOf("#121313", "#278aff", "#8965ef")) Color.White else Color(0xFF222222),
                                15.dp,
                            )
                    }
                }
            }
        }
        }
        Text(
            "Double-tap a swatch to paint with it.",
            fontSize = 10.sp,
            lineHeight = 16.sp,
            color = Sc.Note,
            modifier = Modifier.padding(bottom = 12.dp),
        )
    }
}
/** `.text-controls`: font, style, size, the three styles, the colour and the alignment. */
@Composable
private fun TextControls(a: StudioActions, compact: Boolean) {
    val s = a.state
    Row(
        Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(if (compact) 8.dp else 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        SelectField(
            "Font family", s.value("font"),
            listOf("Instrument Serif", "Arial", "Georgia", "Helvetica", "Times New Roman"),
            modifier = Modifier.width(if (compact) 127.dp else 108.dp),
        ) { s.setValue("font", it) }
        SelectField(
            "Font style", s.value("fontStyle"),
            listOf("Regular", "Medium", "Bold", "Italic"),
            modifier = Modifier.width(if (compact) 85.dp else 82.dp),
        ) { s.setValue("fontStyle", it) }
        NumberField(s.value("size"), "Font size", suffix = "pt", minWidth = 61.dp) { s.setValue("size", it) }
        listOf("bold", "italic", "underline").forEach { icon ->
            IconButton(icon, "${icon.replaceFirstChar { it.uppercase() }} text", small = false, active = s.isOn(icon)) { s.toggle(icon) }
        }
        // The text colour, as the reference's round colour input.
        Box(
            Modifier.size(26.dp).clip(RoundedCornerShape(13.dp)).background(parseColour(s.value("colour")))
                .border(1.dp, Color(0x33FFFFFF), RoundedCornerShape(13.dp))
                .clickable { a.feedback("The text colour is the foreground colour.") },
        )
        listOf("align", "center", "right", "list").forEach { icon ->
            IconButton(icon, icon, small = false, active = s.value("alignment") == icon) { s.setValue("alignment", icon) }
        }
    }
}
@Composable
private fun StrokeBody(a: StudioActions) {
    val s = a.state
    SecondaryContent {
        FieldRow("Width") {
            NumberField(s.value("strokeWidth"), "Stroke width", suffix = "pt") { s.setValue("strokeWidth", it) }
        }
        SelectField("Stroke style", s.value("strokeStyle").ifEmpty { "Solid" }, listOf("Solid", "Dashed", "Dotted", "None")) {
            s.setValue("strokeStyle", it)
        }
        // `.stroke-preview`
        val style = s.value("strokeStyle").ifEmpty { "Solid" }
        val width = (s.value("strokeWidth").toFloatOrNull() ?: 0.7f).coerceIn(1f, 12f)
        Box(
            Modifier.fillMaxWidth().padding(vertical = 14.dp).height(width.coerceAtLeast(1f).dp)
                .background(
                    if (style == "None") Color.Transparent else Color(0xFFE0E4D7),
                )
                .alpha(if (style == "None") 0f else 1f),
        )
        FieldRow("Pressure") {
            IconButton("curve", "Pressure profile", active = s.isOn("pressure")) { s.toggle("pressure") }
        }
        Text("Stroke width goes to the shape's stroke.", fontSize = 10.sp, lineHeight = 16.sp, color = Sc.Note)
    }
}
@Composable
private fun EffectsBody(a: StudioActions) {
    val s = a.state
    SecondaryContent {
        listOf("Gaussian Blur", "Outer Shadow", "Inner Shadow", "Outer Glow", "Bevel / Emboss", "Colour Overlay").forEach { effect ->
            CheckRow(effect, s.isOn(effect)) { s.toggle(effect) }
        }
        Text("Layer effects are applied by the document's layer style.", fontSize = 10.sp, lineHeight = 16.sp, color = Sc.Note)
    }
}
@Composable
private fun TransformBody(a: StudioActions) {
    val s = a.state
    SecondaryContent {
        listOf("X", "Y", "Width", "Height", "Rotation", "Shear").forEachIndexed { i, label ->
            FieldRow(label) {
                NumberField(
                    value = s.value("transform$label").ifEmpty {
                        when (i) {
                            2 -> s.docWidth.toString()
                            3 -> s.docHeight.toString()
                            else -> "0"
                    }
                },
                    label = "Transform $label",
                    minWidth = 62.dp,
                    suffix = if (i > 3) "°" else "px",
                ) { s.setValue("transform$label", it) }
        }
        }
        Text("Transforms apply to the active layer.", fontSize = 10.sp, lineHeight = 16.sp, color = Sc.Note)
    }
}
@Composable
private fun NavigatorBody(a: StudioActions) {
    val s = a.state
    SecondaryContent {
        WorkspaceArtwork(s.workspace, Modifier.fillMaxWidth().height(120.dp))
        FieldRow("Zoom") {
            Text("${(s.zoom * 100).toInt()}%", fontSize = 11.sp, color = Sc.Title)
        }
        StudioSlider(fraction = (s.zoom * 100f - 75f) / 175f, modifier = Modifier.fillMaxWidth()) { f ->
            s.zoom = (75f + f * 175f) / 100f
        }
        StandardButton("Fit document") { s.fit() }
    }
}
@Composable
private fun HistoryBody(a: StudioActions) {
    val e = a.engine
    val scope = rememberCoroutineScope()
    SecondaryContent {
        if (e.history.isEmpty()) {
            Row(Modifier.fillMaxWidth().padding(vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                StudioIcons.Glyph("history", Sc.IconInk, 16.dp)
                Spacer(Modifier.width(8.dp))
                Text("Opened document", fontSize = 10.sp, color = Sc.Title, modifier = Modifier.weight(1f))
                StudioIcons.Glyph("check", Sc.IconInk, 14.dp)
        }
        } else {
            Column(Modifier.fillMaxWidth().heightIn(max = 190.dp).verticalScroll(rememberScrollState())) {
                e.history.forEachIndexed { i, label ->
                    Row(Modifier.fillMaxWidth().clickable { }, verticalAlignment = Alignment.CenterVertically) {
                        StudioIcons.Glyph(if (i == 0) "check" else "history", if (i == 0) Sc.BlueLight else Sc.IconInk, 16.dp)
                        Spacer(Modifier.width(8.dp))
                        Text(label, fontSize = 10.sp, color = if (i == 0) Sc.Title else Sc.Note, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(vertical = 4.dp))
                }
            }
        }
        }
        Text("Every engine command is one entry.", fontSize = 10.sp, lineHeight = 16.sp, color = Sc.Note)
        StandardButton("Undo") { scope.engine { e.undo() } }
        StandardButton("Redo") { scope.engine { e.redo() } }
    }
}
@Composable
private fun ExportBody(a: StudioActions) {
    val s = a.state
    SecondaryContent {
        SelectField("Export format", s.value("exportFormat").ifEmpty { "PNG" }, listOf("PNG", "JPEG", "WebP", "PSD", "TIFF")) {
            s.setValue("exportFormat", it)
        }
        Text(
            "PhotoCraft encodes the document; FilmCraft adds it to the timeline.",
            fontSize = 10.sp,
            lineHeight = 16.sp,
            color = Sc.Note,
        )
        StandardButton("Export…", primary = true) { a.export() }
        StandardButton("Classic editor…") { a.classicEditor() }
    }
}
/** Runs an engine call off the frame, reporting a refusal to the interface. */
internal fun kotlinx.coroutines.CoroutineScope.engine(block: suspend () -> Unit) {
    launch(kotlinx.coroutines.Dispatchers.Main.immediate) {
        try {
            block()
        } catch (e: kotlinx.coroutines.CancellationException) {
            throw e
        } catch (e: Exception) {
            StudioErrors.report(e.message ?: "The engine refused that.")
        }
    }
}
/** Where engine failures go: the shell shows them as its toast. */
object StudioErrors {
    var sink: ((String) -> Unit)? = null
    fun report(message: String) {
        sink?.invoke(message)
    }
}
