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
import androidx.compose.foundation.layout.fillMaxHeight
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
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.launch
import org.json.JSONObject
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
        // The lock is the layer's own, and the engine refuses to paint on a locked layer.
        IconButton(
            "lock",
            if (active?.let { e.isLocked(it.id) } == true) "Unlock layer" else "Lock layer",
            small = true,
            active = active?.let { e.isLocked(it.id) } == true,
        ) {
            val id = active?.id ?: return@IconButton
            scope.engine { e.setLocked(id, !e.isLocked(id)) }
        }
    }
    PanelDivider()
    Column(Modifier.fillMaxWidth().heightIn(max = 420.dp).verticalScroll(rememberScrollState())) {
        // Only the document's own layers. There used to be a hardcoded tree of the reference's
        // fifteen rows shown "at rest", but they were not the document: tapping one asked the
        // engine to select a layer that did not exist, which is where "select a layer first" came
        // from. A panel that lists layers you cannot select is worse than an empty one.
        if (live) {
            e.layers.forEach { row -> LiveLayerRow(row, a) }
        } else {
            Column(
                Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 22.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                Text("No layers yet.", fontSize = 11.sp, color = Color(0xFFA8AF9A))
            }
        }
    }
    PanelFooter(
        if (live) "${e.layers.size} ${if (e.layers.size == 1) "layer" else "layers"}" else "No document",
        if (live) "PhotoCraft" else "",
    )
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
            // One option, because it is the one the engine works in. A list of four with three
            // that change nothing is the dead-control pattern in a different shape.
            SelectField(
                label = "Colour space",
                value = "RGB",
                options = listOf("RGB"),
                height = 22.dp,
                fontSize = 10,
                onSelect = {},
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
    val e = a.engine
    val scope = rememberCoroutineScope()
    val pages = e.pages
    val active = e.activePage
    fun go(block: suspend () -> Unit) {
        scope.launch {
            runCatching { block() }.onFailure { a.feedback(it.message ?: "The page could not change.") }
        }
    }
    // `.pages-navigation`: the two chevrons and the page count.
    Row(
        Modifier.fillMaxWidth().padding(horizontal = 7.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        IconButton("chevron", "Previous page", small = true) {
            if (active > 0) go { e.run("page.select", JSONObject().put("page", active - 1)) }
        }
        Text("Pages", fontSize = 11.sp, color = Sc.Title)
        IconButton("chevron", "Next page", small = true) {
            if (active < pages.lastIndex) go { e.run("page.select", JSONObject().put("page", active + 1)) }
        }
    }
    Row(Modifier.fillMaxWidth().heightIn(min = 30.dp).padding(horizontal = 11.dp), verticalAlignment = Alignment.CenterVertically) {
        IconButton("plus", "Add page", small = true) { go { e.run("page.add") } }
        Spacer(Modifier.width(15.dp))
        IconButton("duplicate", "Duplicate page", small = true) { go { e.run("page.duplicate") } }
        Spacer(Modifier.width(15.dp))
        // A document keeps at least one page, so the control says why when it cannot act rather
        // than emptying the document.
        IconButton("trash", "Delete page", small = true) {
            if (pages.size <= 1) {
                a.feedback("A document needs at least one page.")
            } else {
                go { e.run("page.delete") }
            }
        }
        Spacer(Modifier.weight(1f))
        IconButton("more", "Page options", small = true) { s.toggle("managePages") }
    }
    if (pages.isEmpty()) {
        Text(
            "No document is open.",
            fontSize = 10.sp,
            color = Color(0xFF8E9287),
            modifier = Modifier.fillMaxWidth().padding(horizontal = 11.dp, vertical = 9.dp),
        )
        return
    }
    // The pages that exist. A row is the page's own name, its size and its layer count, so an
    // empty page reads as empty rather than as a missing spread.
    Column(Modifier.fillMaxWidth().heightIn(max = 320.dp).verticalScroll(rememberScrollState()).padding(horizontal = 11.dp)) {
        pages.forEach { page ->
            Row(
                Modifier.fillMaxWidth().padding(vertical = 3.dp)
                    .clip(RoundedCornerShape(5.dp))
                    .background(if (page.active) Color(0xFF2B2C29) else Color.Transparent)
                    .border(
                        1.dp,
                        if (page.active) Color(0xFF3597FF) else Color(0xFF2E2F2B),
                        RoundedCornerShape(5.dp),
                    )
                    .clickable { if (!page.active) go { e.run("page.select", JSONObject().put("page", page.index)) } }
                    .padding(horizontal = 7.dp, vertical = 7.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                StudioIcons.Glyph("page", if (page.active) Color(0xFF3597FF) else Color(0xFF989C8F), 14.dp)
                Spacer(Modifier.width(7.dp))
                Column(Modifier.weight(1f)) {
                    Text(
                        page.name,
                        fontSize = 10.sp,
                        color = if (page.active) Sc.Title else Color(0xFFB8BBAE),
                        maxLines = 1,
                    )
                    Text(
                        "${page.width} × ${page.height} · ${page.layers} ${if (page.layers == 1) "layer" else "layers"}",
                        fontSize = 8.5.sp,
                        color = Color(0xFF8E9287),
                    )
                }
            }
        }
        Spacer(Modifier.height(4.dp))
    }
    DisclosureRow("Manage Pages", s.isOn("managePages")) { s.toggle("managePages") }
    if (s.isOn("managePages")) {
        ExpandedSection(
            "${pages.size} ${if (pages.size == 1) "page" else "pages"}",
            pages.getOrNull(active)?.let { "${it.width} × ${it.height}" } ?: "",
        )
    }
    DisclosureRow("Sections", s.isOn("sections")) { s.toggle("sections") }
    if (s.isOn("sections")) ExpandedSection("Section 1 — Showcase", "Pages 1–${pages.size}")
}
// ---------------------------------------------------------------------------- Brushes
@Composable
private fun BrushesBody(a: StudioActions) {
    val s = a.state
    val e = a.engine
    val scope = rememberCoroutineScope()
    // The search text narrows the list; the set does too, by which brushes belong to it.
    // The brushes with the settings that draw them, so a row's sample is the stroke the canvas
    // would make rather than the same squiggle eight times.
    val all = if (e.brushLibrary.isEmpty()) StudioBrushes.map { BrushInfo(name = it) } else e.brushLibrary
    val brushes = all
        .filter { it.name.lowercase().contains(s.search.lowercase()) && StudioBrushSets.filters(s.category, it.name) }
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
    // The set is switched with an arrow either side, as the reference's brush strip does, rather
    // than dropped down: the name stays visible while the collection changes under it.
    val sets = StudioBrushSets.names
    Row(
        Modifier.fillMaxWidth().padding(horizontal = 9.dp).height(28.dp)
            .clip(RoundedCornerShape(6.dp)).background(Color(0xFF1F201E)),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        val index = sets.indexOf(s.category).coerceAtLeast(0)
        Box(
            Modifier.fillMaxHeight().width(30.dp).clickable {
                s.category = sets[(index - 1 + sets.size) % sets.size]
            },
            contentAlignment = Alignment.Center,
        ) {
            StudioIcons.Glyph("chevron-left", Color(0xFFB9BEAF), 13.dp)
        }
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            Text(sets[index], fontSize = 11.sp, color = Sc.Title, maxLines = 1)
            Text("Set ${index + 1} of ${sets.size}", fontSize = 8.sp, color = Color(0xFF84887D), maxLines = 1)
        }
        Box(
            Modifier.fillMaxHeight().width(30.dp).clickable {
                s.category = sets[(index + 1) % sets.size]
            },
            contentAlignment = Alignment.Center,
        ) {
            StudioIcons.Glyph("chevron-right", Color(0xFFB9BEAF), 13.dp)
        }
    }
    Column(Modifier.fillMaxWidth().heightIn(max = 355.dp).verticalScroll(rememberScrollState())) {
        brushes.forEachIndexed { _, info ->
            val name = info.name
            val on = s.brush == name
            Column(
                Modifier.fillMaxWidth().height(49.dp)
                    .background(if (on) Color(0x07FFFFFF) else Color.Transparent)
                    .border(1.dp, if (on) Color(0xFF358ED9) else Color.Transparent)
                    .clickable {
                        s.brush = name
                        scope.engine {
                            e.setBrush(preset = name)
                            // The engine replies with the brush it stored; the panel's own
                            // controls then edit that, not a stale copy.
                            e.refreshBrushes()
                        }
                }
                    .padding(horizontal = 10.dp),
                verticalArrangement = Arrangement.Center,
            ) {
                BrushStrokeSample(info, Modifier.fillMaxWidth().height(31.dp))
                Text(name, fontSize = 9.sp, color = Color(0xFFB9BEAF), maxLines = 1)
        }
        }
        if (brushes.isEmpty()) {
            Column(
                Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 26.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                Text("No brushes match “${s.search}”.", fontSize = 11.sp, color = Color(0xFFA8AF9A))
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
        // Name at the left, value in a pill at the right, track below -- Procraft's own brush
        // control. The pill takes a typed number, so an exact width does not depend on hitting a
        // track position with a thumb.
        StudioValueSlider(
            label = "Width",
            fraction = ((s.value("brushSize").toFloatOrNull() ?: 24f) - 1f) / 499f,
            valueText = "${s.value("brushSize").toIntOrNull() ?: 24}px",
            modifier = Modifier.fillMaxWidth(),
            onValue = { v ->
                v.filter(Char::isDigit).toIntOrNull()?.let { px ->
                    val clamped = px.coerceIn(1, 500)
                    s.setValue("brushSize", clamped.toString())
                    scope.engine { e.setBrush(size = clamped.toFloat()) }
                }
            },
        ) { f ->
            val px = (1f + f * 499f).toInt()
            s.setValue("brushSize", px.toString())
            scope.engine { e.setBrush(size = px.toFloat()) }
        }
        Spacer(Modifier.height(8.dp))
        StudioValueSlider(
            label = "Opacity",
            fraction = (s.value("brushOpacity").toFloatOrNull() ?: 100f) / 100f,
            valueText = "${s.value("brushOpacity").toIntOrNull() ?: 100}%",
            modifier = Modifier.fillMaxWidth(),
            onValue = { v ->
                v.filter(Char::isDigit).toIntOrNull()?.let { pct ->
                    val clamped = pct.coerceIn(0, 100)
                    s.setValue("brushOpacity", clamped.toString())
                    scope.engine { e.setBrush(opacity = clamped / 100f) }
                }
            },
        ) { f ->
            val pct = (f * 100f).toInt()
            s.setValue("brushOpacity", pct.toString())
            scope.engine { e.setBrush(opacity = pct / 100f) }
        }
        // The brush's shape: the settings that make one brush unlike another. These edit the
        // brush in hand (`brush.set`) rather than picking a preset, so the next stroke follows
        // the slider that was just moved.
        val brush = e.brush
        val edit: (org.json.JSONObject) -> Unit = { o -> scope.engine { e.setBrushParams(o) } }
        Spacer(Modifier.height(8.dp))
        StudioValueSlider(
            label = "Hardness",
            fraction = brush.hardness,
            valueText = "${(brush.hardness * 100).toInt()}%",
            modifier = Modifier.fillMaxWidth(),
        ) { f ->
            // The hardness is part of the brush *and* of the tool state the panels read, so both
            // are told: otherwise the slider and the preset list drift apart.
            scope.engine { e.setBrush(hardness = f) }
            edit(org.json.JSONObject().put("hardness", f.toDouble()))
        }
        Spacer(Modifier.height(8.dp))
        StudioValueSlider(
            label = "Grain",
            fraction = brush.grain,
            valueText = "${(brush.grain * 100).toInt()}%",
            modifier = Modifier.fillMaxWidth(),
        ) { f -> edit(org.json.JSONObject().put("grain", f.toDouble())) }
        Spacer(Modifier.height(8.dp))
        StudioValueSlider(
            label = "Spacing",
            fraction = ((brush.spacing - 0.02f) / 0.58f).coerceIn(0f, 1f),
            valueText = "${(brush.spacing * 100).toInt()}%",
            modifier = Modifier.fillMaxWidth(),
        ) { f -> edit(org.json.JSONObject().put("spacing", (0.02f + f * 0.58f).toDouble())) }
        Spacer(Modifier.height(8.dp))
        StudioValueSlider(
            label = "Flow",
            fraction = brush.flow,
            valueText = "${(brush.flow * 100).toInt()}%",
            modifier = Modifier.fillMaxWidth(),
        ) { f -> edit(org.json.JSONObject().put("flow", f.toDouble())) }
        Spacer(Modifier.height(8.dp))
        StudioValueSlider(
            label = "Taper",
            fraction = (brush.taper / 0.5f).coerceIn(0f, 1f),
            valueText = "${(brush.taper * 100).toInt()}%",
            modifier = Modifier.fillMaxWidth(),
        ) { f -> edit(org.json.JSONObject().put("taper", (f * 0.5f).toDouble())) }
        // The tip and the grain's mode are choices, not positions on a track: two rows of pills.
        Spacer(Modifier.height(8.dp))
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            for (tip in listOf("round", "chisel", "flat", "ragged", "cloud")) {
                BrushChoicePill(
                    text = tip.replaceFirstChar(Char::uppercase),
                    on = brush.tip == tip,
                    modifier = Modifier.weight(1f),
                ) { edit(org.json.JSONObject().put("tip", tip)) }
            }
        }
        Spacer(Modifier.height(4.dp))
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            for ((mode, label) in listOf("canvas" to "Canvas grain", "stamp" to "Travelling grain")) {
                BrushChoicePill(
                    text = label,
                    on = brush.grainMode == mode,
                    modifier = Modifier.weight(2f),
                ) { edit(org.json.JSONObject().put("grainMode", mode)) }
            }
        }
    }
}

/** One of the brush's either-or settings: the tip, or where its grain sits. */
@Composable
private fun BrushChoicePill(
    text: String,
    on: Boolean,
    modifier: Modifier = Modifier,
    onClick: () -> Unit,
) {
    Box(
        modifier
            .height(24.dp)
            .clip(RoundedCornerShape(5.dp))
            .background(if (on) Color(0xFF358ED9) else Color(0xFF1F201E))
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Text(text, fontSize = 8.sp, maxLines = 1, color = if (on) Color.White else Color(0xFFB9BEAF))
    }
}
/**
 * `.brush-preset`: the brush's own stroke, drawn from the settings the canvas will use.
 *
 * The list used to show eight copies of one squiggle, which was honest about the engine and
 * useless as a chooser. Here the width comes from the hardness, the edge is broken by the grain,
 * the line thins where the brush tapers, and a bristle brush is drawn as several hairs. Two rows
 * that look alike are two brushes that paint alike.
 */
@Composable
private fun BrushStrokeSample(info: BrushInfo, modifier: Modifier = Modifier) {
    Canvas(modifier) {
        val w = size.width
        val h = size.height
        val path = Path().apply {
            moveTo(w * 0.015f, h * 0.72f)
            quadraticBezierTo(w * 0.26f, h * 0.15f, w * 0.5f, h * 0.47f)
            quadraticBezierTo(w * 0.75f, h * 0.79f, w * 0.985f, h * 0.28f)
        }
        // A bristle brush draws several hairs at once; the rest draw one body. This is the one
        // thing that separates a rake from a round brush at thumbnail size.
        val hairs = when (info.tip) {
            "flat" -> 3
            "chisel" -> 2
            else -> if (info.scatter > 0.35f) info.count.coerceAtLeast(2) else 1
        }
        val body = (1.5f + info.hardness * 3.5f + (1f - info.roundness) * 3f).coerceIn(1f, 9f)
        val colour = Color(0xFFEEEEEE)
        for (hair in 0 until hairs) {
            // The hairs sit side by side across the stroke, so its edge looks broken rather than
            // soft -- which is what a bristle and a flat brush are.
            val shift = if (hairs == 1) 0f else (hair / (hairs - 1f) - 0.5f) * body * 0.9f
            val alpha = (1f - info.grain * 0.4f * (hair % 2)) * 0.9f
            translate(top = shift) {
                drawPath(
                    path,
                    colour.copy(alpha = alpha.coerceIn(0.15f, 1f)),
                    style = Stroke(
                        width = if (hairs == 1) body else body / hairs,
                        cap = if (info.tip == "ragged" || info.tip == "cloud") {
                            StrokeCap.Butt
                        } else {
                            StrokeCap.Round
                        },
                    ),
                )
            }
        }
        // A tapered brush starts and finishes thin; drawing the ends at full width would say the
        // taper setting does nothing.
        if (info.taper > 0.01f) {
            val thin = (1f - info.taper.coerceIn(0f, 0.5f) * 1.6f).coerceAtLeast(0.25f)
            val ends = listOf(
                Path().apply {
                    moveTo(w * 0.015f, h * 0.72f)
                    quadraticBezierTo(w * 0.09f, h * 0.55f, w * 0.19f, h * 0.42f)
                },
                Path().apply {
                    moveTo(w * 0.80f, h * 0.36f)
                    quadraticBezierTo(w * 0.91f, h * 0.31f, w * 0.985f, h * 0.28f)
                },
            )
            for (fade in ends) {
                drawPath(
                    fade,
                    Color(0xFF1B1C1A).copy(alpha = (1f - thin).coerceIn(0f, 1f)),
                    style = Stroke(width = body * 0.95f, cap = StrokeCap.Round),
                )
            }
        }
        // The grain: tooth along the stroke, as many speckles as its depth says. This is what
        // makes charcoal and chalk look like themselves in the list.
        if (info.grain > 0.02f) {
            val n = (info.grain * 26).toInt().coerceAtLeast(1)
            for (k in 0 until n) {
                val t = (k + 0.5f) / n
                val x = w * (0.03f + 0.94f * t)
                // The same curve the path follows, so the speckles sit on the stroke.
                val y = h * (0.72f - 0.44f * kotlin.math.sin(t * 3.14159f))
                drawCircle(
                    Color(0xFF1B1C1A).copy(alpha = 0.5f + 0.3f * (k % 3) / 2f),
                    radius = 0.4f + info.grain * 1.5f,
                    center = Offset(x, y),
                )
            }
        }
    }
}
// ---------------------------------------------------------------------------- Colour, Text, secondary
@Composable
private fun SecondaryBody(name: String, a: StudioActions) {
    val s = a.state
    when (name) {
        "Colour", "Swatches" -> ColourBody(a)
        "Text" -> TextBody(a)
        "Paragraph" -> ParagraphBody(a)
        "Stroke" -> StrokeBody(a)
        "Effects" -> EffectsBody(a)
        "Transform" -> TransformBody(a)
        "Navigator" -> NavigatorBody(a)
        "History" -> HistoryBody(a)
        "Export" -> ExportBody(a)
        // The rail's thirteen studios are all handled above, so this is a panel name the model
        // does not have. It says so rather than pretending to be an Assets browser with nothing
        // in it: a button that only explains itself is the kind of dead control this is meant to
        // be free of.
        else -> SecondaryContent {
            Text(
                "There is no panel called \"$name\".",
                fontSize = 10.sp, lineHeight = 16.sp, color = Sc.Note,
            )
        }
    }
}
/** The Text studio: the face, the size and the colour of the selected run. */
@Composable
private fun TextBody(a: StudioActions) {
    val s = a.state
    val e = a.engine
    val scope = rememberCoroutineScope()
    val active = e.layers.firstOrNull { it.active }
    val isText = active?.kind == "text"
    // Every field writes to the run itself, so what the panel shows is what the drawing carries.
    fun set(params: JSONObject) {
        val id = active?.id ?: return
        params.put("layer", id)
        scope.engine { e.run("type.setProps", params) }
    }
    SecondaryContent {
        if (!isText) {
            Text(
                if (active == null) "Select a text layer to format it." else "The selected layer is not text.",
                fontSize = 10.sp, lineHeight = 16.sp, color = Sc.Note,
            )
        }
        TextControls(a, compact = false)
        Row(Modifier.fillMaxWidth().padding(top = 6.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            listOf("Regular", "Italic", "Bold", "Bold Italic").forEach { face ->
                StandardButton(face) { if (isText) set(JSONObject().put("style", face)) }
            }
        }
        Row(Modifier.fillMaxWidth().padding(top = 6.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            listOf(24, 48, 96, 200).forEach { size ->
                StandardButton("${size}px") { if (isText) set(JSONObject().put("size", size.toDouble())) }
            }
        }
        if (isText) {
            Row(Modifier.fillMaxWidth().padding(top = 6.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                StandardButton("Colour it") { set(JSONObject().put("colour", s.value("colour"))) }
            }
        }
    }
}

/** The Paragraph studio: how the run's lines line up, and how far apart they sit. */
@Composable
private fun ParagraphBody(a: StudioActions) {
    val s = a.state
    val e = a.engine
    val scope = rememberCoroutineScope()
    val active = e.layers.firstOrNull { it.active }
    val isText = active?.kind == "text"
    fun set(params: JSONObject) {
        val id = active?.id ?: return
        params.put("layer", id)
        scope.engine { e.run("type.setProps", params) }
    }
    SecondaryContent {
        if (!isText) {
            Text(
                if (active == null) "Select a text layer to align it." else "The selected layer is not text.",
                fontSize = 10.sp, lineHeight = 16.sp, color = Sc.Note,
            )
        }
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            // Glyph names the studio actually has: an unknown name draws the placeholder
            // rectangle, which would make four identical buttons.
            listOf("align" to "left", "center" to "center", "list" to "right", "right" to "justify")
                .forEach { (icon, value) ->
                    IconButton(icon, "Align ${value}", small = true, active = isText) {
                        if (isText) set(JSONObject().put("align", value))
                    }
                }
        }
        FieldRow("Leading") {
            NumberField(s.value("leading"), "Leading", suffix = "×") { text ->
                s.setValue("leading", text)
                text.toDoubleOrNull()?.let { if (isText) set(JSONObject().put("leading", it)) }
            }
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
    val e = a.engine
    val scope = rememberCoroutineScope()
    val active = e.layers.firstOrNull { it.active }
    val isText = active?.kind == "text"
    // Every control edits the selected run. With nothing text selected the controls still show
    // the values they will apply, and say so rather than writing into a copy of their own.
    fun set(block: JSONObject.() -> Unit) {
        val id = active?.id ?: return
        if (!isText) return
        val params = JSONObject().apply { put("layer", id); block() }
        scope.engine { e.run("type.setProps", params) }
    }
    Column(Modifier.fillMaxWidth()) {
        Row(
            Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(if (compact) 8.dp else 6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            SelectField(
                "Font family", s.value("font"),
                listOf("Inter", "Instrument Serif", "Arial", "Georgia", "Helvetica", "Times New Roman"),
                modifier = Modifier.width(if (compact) 127.dp else 108.dp),
            ) { family -> s.setValue("font", family); set { put("family", family) } }
            SelectField(
                "Font style", s.value("fontStyle"),
                listOf("Regular", "Italic", "Bold", "Bold Italic"),
                modifier = Modifier.width(if (compact) 85.dp else 82.dp),
            ) { face -> s.setValue("fontStyle", face); set { put("style", face) } }
            NumberField(s.value("size"), "Font size", suffix = "px", minWidth = 61.dp) { text ->
                s.setValue("size", text)
                text.toDoubleOrNull()?.let { size -> set { put("size", size) } }
            }
            listOf("bold" to "Bold", "italic" to "Italic").forEach { (icon, face) ->
                IconButton(icon, "$face text", small = false, active = s.value("fontStyle") == face) {
                    val next = if (s.value("fontStyle") == face) "Regular" else face
                    s.setValue("fontStyle", next)
                    set { put("style", next) }
                }
            }
            // The text colour, as the reference's round colour input: it colours the run.
            Box(
                Modifier.size(26.dp).clip(RoundedCornerShape(13.dp)).background(parseColour(s.value("colour")))
                    .border(1.dp, Color(0x33FFFFFF), RoundedCornerShape(13.dp))
                    .clickable {
                        if (isText) set { put("colour", s.value("colour")) }
                        else a.feedback("Select a text layer to colour it.")
                    },
            )
            listOf("align" to "left", "center" to "center", "right" to "right").forEach { (icon, align) ->
                IconButton(icon, "Align $align", small = false, active = s.value("alignment") == icon) {
                    s.setValue("alignment", icon)
                    set { put("align", align) }
                }
            }
        }
        if (!isText) {
            Text(
                if (active == null) "Select a text layer to format it." else "The selected layer is not text.",
                fontSize = 9.5.sp, lineHeight = 15.sp, color = Sc.Note,
                modifier = Modifier.padding(top = 4.dp),
            )
        }
    }
}
@Composable
private fun StrokeBody(a: StudioActions) {
    val s = a.state
    val e = a.engine
    val scope = rememberCoroutineScope()
    val active = e.layers.firstOrNull { it.active }
    // The stroke of a vector shape is geometry, so the panel edits the selected shape. With a
    // pixel layer selected there is no stroke to change, and the panel says so rather than
    // writing a number that nothing reads.
    val isShape = active?.kind == "shape"
    fun applyStroke(width: Float) {
        val id = active?.id ?: return
        s.setValue("shapeStrokeWidth", width.toString())
        scope.launch {
            runCatching {
                e.run(
                    "shape.setProps",
                    JSONObject().put("layer", id).put("strokeWidth", width.toDouble()),
                )
            }.onFailure { a.feedback(it.message ?: "The stroke could not change.") }
        }
    }
    SecondaryContent {
        if (!isShape) {
            Text(
                if (active == null) "Select a shape to give it a stroke." else "The selected layer is not a shape.",
                fontSize = 10.sp,
                lineHeight = 16.sp,
                color = Sc.Note,
            )
        }
        FieldRow("Width") {
            NumberField(s.value("shapeStrokeWidth"), "Stroke width", suffix = "pt") {
                it.toFloatOrNull()?.let { w -> applyStroke(w) }
            }
        }
        Row(Modifier.fillMaxWidth().padding(top = 4.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            listOf(0f, 1f, 2f, 4f, 8f).forEach { width ->
                StandardButton(if (width == 0f) "None" else "${width.toInt()}") {
                    if (isShape) applyStroke(width) else a.feedback("Select a shape first.")
                }
            }
        }
        // `.stroke-preview`
        val width = (s.value("shapeStrokeWidth").toFloatOrNull() ?: 0f).coerceIn(0f, 12f)
        Box(
            Modifier.fillMaxWidth().padding(vertical = 14.dp).height(width.coerceAtLeast(1f).dp)
                .background(if (width == 0f) Color.Transparent else Color(0xFFE0E4D7))
                .alpha(if (width == 0f) 0f else 1f),
        )
        if (isShape) {
            Row(Modifier.fillMaxWidth().padding(top = 2.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                StandardButton("No stroke") {
                    val id = active?.id ?: return@StandardButton
                    scope.launch {
                        runCatching {
                            e.run(
                                "shape.setProps",
                                JSONObject().put("layer", id).put("stroke", JSONObject.NULL),
                            )
                        }.onFailure { a.feedback(it.message ?: "The stroke could not change.") }
                    }
                }
                StandardButton("Outline in colour") {
                    val id = active?.id ?: return@StandardButton
                    scope.launch {
                        runCatching {
                            e.run(
                                "shape.setProps",
                                JSONObject().put("layer", id)
                                    .put("stroke", s.value("colour"))
                                    .put("strokeWidth", 2.0),
                            )
                        }.onFailure { a.feedback(it.message ?: "The stroke could not change.") }
                    }
                }
            }
        }
    }
}
@Composable
private fun EffectsBody(a: StudioActions) {
    val s = a.state
    val e = a.engine
    val scope = rememberCoroutineScope()
    val active = e.layers.firstOrNull { it.active }
    val effects = listOf(
        "Gaussian Blur", "Outer Shadow", "Inner Shadow", "Outer Glow", "Bevel / Emboss", "Colour Overlay",
    )
    fun set(effect: String, on: Boolean) {
        val id = active?.id ?: return
        s.toggle(effect)
        scope.launch {
            runCatching {
                e.run(
                    "layer.style",
                    JSONObject().put("layer", id).put("effect", effect).put("on", on)
                        .put("colour", s.value("colour"))
                        .put("radius", (s.value("effectRadius").toDoubleOrNull() ?: 8.0)),
                )
            }.onFailure { a.feedback(it.message ?: "The effect could not change.") }
        }
    }
    SecondaryContent {
        if (active == null) {
            Text("Select a layer to give it an effect.", fontSize = 10.sp, lineHeight = 16.sp, color = Sc.Note)
        }
        effects.forEach { effect ->
            // The row reports a click, not the new value; the panel owns the toggle, so the two
            // cannot disagree.
            CheckRow(effect, s.isOn(effect)) { set(effect, !s.isOn(effect)) }
        }
        Spacer(Modifier.height(8.dp))
        FieldRow("Radius") {
            NumberField(s.value("effectRadius"), "Effect radius", suffix = "px") {
                s.setValue("effectRadius", it)
                // A change to the radius applies to whatever is already on, so the field is not
                // a setting that only takes effect the next time a box is ticked.
                effects.filter { s.isOn(it) }.forEach { on -> set(on, true) }
            }
        }
    }
}
@Composable
private fun TransformBody(a: StudioActions) {
    val s = a.state
    val e = a.engine
    val scope = rememberCoroutineScope()
    val active = e.layers.firstOrNull { it.active }
    val movable = active?.kind == "shape" || active?.kind == "picture"
    // The numbers come from the engine's own shape, so they show where the shape is rather than
    // the last thing typed into the field.
    // `engine.execute` is the suspend wrapper the rest of the studio uses; the inspect itself is
    // read from a background scope so the panel never blocks a frame on the engine's mutex.
    var box by remember(active?.id, e.status?.revision) { mutableStateOf<JSONObject?>(null) }
    LaunchedEffect(active?.id, e.status?.revision) {
        box = runCatching {
            val doc = JSONObject(e.run("document.inspect", refresh = false))
            val arr = doc.optJSONArray("layers")
            fun find(nodes: org.json.JSONArray?, id: Long): org.json.JSONObject? {
                if (nodes == null) return null
                for (i in 0 until nodes.length()) {
                    val o = nodes.optJSONObject(i) ?: continue
                    if (o.optLong("id", -1L) == id) return o
                    find(o.optJSONArray("children"), id)?.let { return it }
                }
                return null
            }
            find(arr, active?.id ?: -1L)
        }.getOrNull()
    }
    val shape = box?.optJSONObject("shape") ?: box?.optJSONObject("picture")
    fun apply(key: String, value: Double) {
        val id = active?.id ?: return
        // A shape and a picture both have frames, but they are different commands: sending a
        // picture's id to the shape command would be refused as "not a shape".
        val command = if (active?.kind == "picture") "picture.setProps" else "shape.setProps"
        scope.launch {
            runCatching {
                e.run(command, JSONObject().put("layer", id).put(key, value))
            }.onFailure { a.feedback(it.message ?: "The transform could not be applied.") }
        }
    }
    SecondaryContent {
        if (!movable) {
            Text(
                if (active == null) "Select a shape or a picture to transform it."
                else "Only a shape or a picture can be transformed from here.",
                fontSize = 10.sp, lineHeight = 16.sp, color = Sc.Note,
            )
        }
        listOf("X" to "x", "Y" to "y", "Width" to "width", "Height" to "height", "Rotation" to "rotation").forEach { (label, key) ->
            FieldRow(label) {
                NumberField(
                    value = s.value("transform$label").ifEmpty {
                        val fromEngine = shape?.optDouble(key, Double.NaN)?.takeIf { !it.isNaN() }
                        when {
                            fromEngine != null -> if (key == "rotation") fromEngine.toInt().toString() else fromEngine.toInt().toString()
                            label == "Width" -> s.docWidth.toString()
                            label == "Height" -> s.docHeight.toString()
                            else -> "0"
                        }
                    },
                    label = "Transform $label",
                    minWidth = 62.dp,
                    suffix = if (key == "rotation") "°" else "px",
                ) {
                    s.setValue("transform$label", it)
                    it.toDoubleOrNull()?.let { v -> if (movable) apply(key, v) }
                }
            }
        }
        if (movable) {
            Row(Modifier.fillMaxWidth().padding(top = 2.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                StandardButton("Centre") {
                    val w = shape?.optDouble("width", 0.0) ?: 0.0
                    val h = shape?.optDouble("height", 0.0) ?: 0.0
                    apply("x", (s.docWidth - w) / 2.0)
                    apply("y", (s.docHeight - h) / 2.0)
                }
                StandardButton("Fit page") {
                    apply("x", 0.0)
                    apply("y", 0.0)
                    apply("width", s.docWidth.toDouble())
                    apply("height", s.docHeight.toDouble())
                }
                StandardButton("Reset rotation") { apply("rotation", 0.0) }
            }
        }
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
