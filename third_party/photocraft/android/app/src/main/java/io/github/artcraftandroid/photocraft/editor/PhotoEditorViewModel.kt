package io.github.artcraftandroid.photocraft.editor

import android.graphics.Bitmap
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import io.github.artcraftandroid.photocraft.core.PhotoStatus
import io.github.artcraftandroid.photocraft.engine.PhotoEngine
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import org.json.JSONObject

/** The editor's mode tabs, after the reference tablet UI. */
enum class Mode(val label: String) {
    Sketch("Sketch"),
    Vector("Vector"),
    Pixel("Pixel"),
    Layout("Layout"),
    Canva("Canva AI"),
}

/** The active tool, as the left toolbar shows it. */
enum class Tool(val label: String) {
    Move("Move"),
    Brush("Brush"),
    Pencil("Pencil"),
    Eraser("Eraser"),
    Fill("Fill"),
    Gradient("Gradient"),
    Clone("Clone"),
    Heal("Heal"),
    Lasso("Lasso"),
    Wand("Wand"),
    Rect("Rectangle"),
    Type("Type"),
    Eyedropper("Eyedropper"),
    Hand("Hand"),
    Zoom("Zoom"),
}

/** The floating panel the right toolbar opened, if any. */
enum class Panel {
    None,
    Brushes,
    Layers,
    Curves,
    Levels,
    Adjustments,
    Filters,
    Image,
    Type,
    Pages,
    History,
    Export,
    Colour,
}

/** One row in the Layers panel. */
data class LayerRow(
    val id: Long,
    val name: String,
    val kind: String,
    val visible: Boolean,
    val opacity: Float,
    val blend: String,
    val selected: Boolean,
    val active: Boolean,
    val depth: Int,
)

/** One brush preset, as the Brushes panel lists it. */
data class BrushPreset(
    val name: String,
    val builtin: Boolean,
    val size: Float,
    val hardness: Float,
)

/** One command, as the engine's own registry lists it (`command.list`). */
data class CommandRow(
    val id: String,
    val label: String,
    val menu: List<String>,
    val enabled: Boolean,
    val disabledReason: String?,
)

/**
 * The editor's state: the engine, the canvas bitmap, the panels and the tools. Every edit goes
 * through the engine's JSON commands; the canvas re-renders from the engine after each one.
 */
class PhotoEditorViewModel : ViewModel() {
    val engine = PhotoEngine()

    var mode by mutableStateOf(Mode.Pixel)
        private set
    var tool by mutableStateOf(Tool.Brush)
        private set
    var panel by mutableStateOf(Panel.None)
        private set

    var bitmap by mutableStateOf<Bitmap?>(null)
        private set
    var status by mutableStateOf<PhotoStatus?>(null)
        private set
    var layers by mutableStateOf<List<LayerRow>>(emptyList())
        private set
    var brushes by mutableStateOf<List<BrushPreset>>(emptyList())
        private set
    /** Every command the engine knows, from `command.list`; the panels filter it by menu. */
    var commands by mutableStateOf<List<CommandRow>>(emptyList())
        private set
    var busy by mutableStateOf(false)
        private set
    var error by mutableStateOf<String?>(null)
        private set

    // Canvas viewport: the document pixel at the viewport's top-left, and the zoom.
    var viewportX by mutableStateOf(0)
        private set
    var viewportY by mutableStateOf(0)
        private set
    var zoom by mutableStateOf(1f)
        private set

    // Brush and colour state, as the panels show it.
    var brushSize by mutableStateOf(32f)
        private set
    var brushHardness by mutableStateOf(0.8f)
        private set
    var brushOpacity by mutableStateOf(1f)
        private set
    var foreground by mutableStateOf(0xFF000000)
        private set
    var background by mutableStateOf(0xFFFFFFFF)

    private var renderJob: Job? = null
    /** The job that opens or creates the first document: [awaitDocument] joins it. */
    private var openJob: Job? = null

    init {
        refreshCommands()
    }

    fun setMode(m: Mode) {
        mode = m
        // The reference UI's tabs switch the tool set; Pixel is the photo editor's home.
        tool = when (m) {
            Mode.Pixel -> Tool.Brush
            Mode.Sketch -> Tool.Pencil
            Mode.Vector -> Tool.Brush
            Mode.Layout -> Tool.Move
            Mode.Canva -> Tool.Wand
        }
    }

    fun setTool(t: Tool) {
        tool = t
        cloneSource = null
        if (t == Tool.Type) panel = Panel.Type
    }

    fun setPanel(p: Panel) {
        panel = if (panel == p) Panel.None else p
        if (panel == Panel.Layers) refreshLayers()
        if (panel == Panel.Brushes) refreshBrushes()
    }

    fun closePanel() {
        panel = Panel.None
    }

    /** Open the given bytes as the first document. */
    fun openBytes(name: String, bytes: ByteArray) {
        val job = viewModelScope.launch {
            busy = true
            error = null
            try {
                engine.openBytes(name, bytes)
                refresh()
            } catch (e: Exception) {
                error = e.message ?: "Could not open the file."
            } finally {
                busy = false
            }
        }
        if (status?.active == null) openJob = job
    }

    /** Start a blank white canvas, as File › New does. */
    fun newDocument(width: Int, height: Int, background: String = "white") {
        openJob = viewModelScope.launch {
            busy = true
            error = null
            try {
                engine.execute("file.new", JSONObject().put("width", width).put("height", height).put("background", background))
                refresh()
            } catch (e: Exception) {
                error = e.message ?: "The document couldn't be created."
            } finally {
                busy = false
            }
        }
    }

    /**
     * Wait for the model to have a document to show (or to report why it hasn't): the host's
     * opening cover stays up until this returns.
     */
    suspend fun awaitDocument() = openJob?.join()

    /** Run one engine command, then re-render. */
    fun command(id: String, params: JSONObject = JSONObject()) {
        val job = viewModelScope.launch {
            busy = true
            error = null
            try {
                engine.execute(id, params)
                refresh()
            } catch (e: Exception) {
                error = e.message ?: "The command failed."
            } finally {
                busy = false
            }
        }
        if (status?.active == null) openJob = job
    }

    /** Run one engine command with raw JSON. */
    fun command(id: String, params: String) {
        viewModelScope.launch {
            busy = true
            error = null
            try {
                engine.execute(id, params)
                refresh()
            } catch (e: Exception) {
                error = e.message ?: "The command failed."
            } finally {
                busy = false
            }
        }
    }

    fun undo() {
        viewModelScope.launch {
            try {
                engine.undo()
                refresh()
            } catch (e: Exception) {
                error = e.message
            }
        }
    }

    fun redo() {
        viewModelScope.launch {
            try {
                engine.redo()
                refresh()
            } catch (e: Exception) {
                error = e.message
            }
        }
    }

    /** Make the document at [index] the one the editor shows (Document › Switch). */
    fun setActive(index: Int) {
        viewModelScope.launch {
            try {
                engine.setActive(index)
                refresh()
            } catch (e: Exception) {
                error = e.message ?: "The document couldn't be opened."
            }
        }
    }

    /** Close the document at [index], discarding its changes; the host's picture is kept. */
    fun close(index: Int) {
        viewModelScope.launch {
            try {
                engine.close(index)
                refresh()
            } catch (e: Exception) {
                error = e.message ?: "The document couldn't be closed."
            }
        }
    }

    // -- Editing on the canvas -------------------------------------------------------------
    //
    // The canvas reports where the finger is in document pixels; what that does is the tool's
    // business, as it is in Photoshop: the painting tools paint a stroke, Fill fills, the Wand
    // selects, the Eyedropper picks the colour, and the shapes and selections wait for a drag.

    /** Whether the current tool paints along a drag (rather than acting on a tap alone). */
    val paintsByDrag: Boolean
        get() = tool in setOf(Tool.Brush, Tool.Pencil, Tool.Eraser, Tool.Clone, Tool.Heal)

    /** The Clone Stamp's source point, picked with the first tap (the engine samples under it). */
    private var cloneSource: Pair<Float, Float>? = null

    /** Where the Clone Stamp copies from, for the canvas to mark once it is picked. */
    val cloneFrom: Pair<Float, Float>? get() = cloneSource

    /** The stroke the finger is drawing, in document pixels, shown over the preview as it goes. */
    var strokePts by mutableStateOf<List<androidx.compose.ui.geometry.Offset>>(emptyList())
        private set

    fun previewStroke(points: List<Pair<Float, Float>>) {
        strokePts = points.map { (x, y) -> androidx.compose.ui.geometry.Offset(x, y) }
    }

    /** Send a finished drag to the engine as one stroke (one History step). */
    fun paintStroke(points: List<Pair<Float, Float>>) {
        strokePts = emptyList()
        if (points.isEmpty()) return
        if (tool == Tool.Clone && cloneSource == null) {
            error = "Tap the area to clone from first."
            return
        }
        viewModelScope.launch {
            try {
                engine.execute(strokeCommand(), strokeParams(points))
                refresh()
            } catch (e: Exception) {
                error = e.message ?: "The stroke couldn't be painted."
            }
        }
    }

    /** The engine's id for the current painting tool. */
    private fun strokeCommand(): String = when (tool) {
        Tool.Pencil -> "paint.pencil"
        Tool.Clone -> "paint.cloneStamp"
        Tool.Heal -> "paint.spotHealing"
        else -> "paint.stroke" // the Eraser is the brush with `erase`, as in the engine
    }

    /**
     * A stroke's params. The painting tools take the panel's hardness and opacity as 0..1
     * fractions; the retouching tools take them as 1..100 percentages, and the Clone Stamp's
     * source.
     */
    private fun strokeParams(points: List<Pair<Float, Float>>): JSONObject {
        val p = JSONObject()
        val pts = org.json.JSONArray()
        for ((x, y) in points) pts.put(org.json.JSONArray().put(x.toDouble()).put(y.toDouble()))
        p.put("points", pts)
        p.put("size", brushSize)
        if (tool == Tool.Clone || tool == Tool.Heal) {
            // The retouching commands take hardness and flow as 1..100 percentages (0 would be
            // refused, or mean a brush that paints nothing).
            p.put("hardness", Math.round(brushHardness * 100f).coerceIn(1, 100))
            p.put("opacity", Math.round(brushOpacity * 100f).coerceIn(1, 100))
            p.put("flow", 100)
            cloneSource?.let { (sx, sy) ->
                p.put("source", org.json.JSONArray().put(sx.toDouble()).put(sy.toDouble()))
            }
        } else {
            p.put("hardness", brushHardness)
            p.put("opacity", brushOpacity)
            p.put("color", hex(foreground))
            if (tool == Tool.Eraser) p.put("erase", true)
        }
        // One History step per stroke, however many dabs it takes.
        p.put("coalesce", "photocraft.stroke")
        return p
    }

    /**
     * A finished drag: the painting tools paint it, the marquee and the lasso take it as a
     * selection, and the tap-only tools act on where it started, so a drag still does something.
     */
    fun endDrag(points: List<Pair<Float, Float>>) {
        strokePts = emptyList()
        val first = points.firstOrNull() ?: return
        val last = points.last()
        when {
            paintsByDrag -> paintStroke(points)
            tool == Tool.Rect -> dragSelection(first, last)
            // The Lasso follows the finger: its path is the selection's outline, thinned to the
            // corners of the shape it drew, so a long gesture doesn't send hundreds of points.
            tool == Tool.Lasso && simplify(points).size >= 3 -> {
                val array = org.json.JSONArray()
                for ((x, y) in simplify(points)) array.put(org.json.JSONArray().put(x.toDouble()).put(y.toDouble()))
                command("select.lasso", JSONObject().put("points", array))
            }
            else -> tapAt(first.first, first.second)
        }
    }

    /** What a tap (or a drag that a tap-only tool receives) does, by tool. */
    fun tapAt(x: Float, y: Float) {
        when (tool) {
            Tool.Fill -> command("paint.bucket", JSONObject().put("x", x.toDouble()).put("y", y.toDouble()).put("tolerance", 32).put("color", hex(foreground)))
            Tool.Gradient -> command("paint.gradient", gradientParams(x, y))
            Tool.Wand -> command("select.magicWand", JSONObject().put("x", x.toDouble()).put("y", y.toDouble()).put("tolerance", 32))
            Tool.Eyedropper -> pickColour(x, y)
            Tool.Zoom -> setZoom((zoom * 1.5f).coerceAtMost(16f))
            // Move has nothing to move yet (no layer transform in this build); the marquee tools
            // need a drag, and a tap gives them a 1x1 box, which selects nothing on purpose.
            Tool.Move, Tool.Lasso, Tool.Rect -> Unit
            Tool.Type -> command(
                "type.create",
                JSONObject().put("x", x.toDouble()).put("y", y.toDouble()).put("text", "Text").put("size", 24),
            )
            Tool.Hand -> Unit
            // The Clone Stamp takes its source from the first tap (as ⌥-click does on a desktop);
            // the Healing Brush needs none, it reads the pixels around the stroke.
            Tool.Clone -> cloneSource = x to y // the next drag paints from here
            Tool.Heal -> paintStroke(listOf(x to y))
            else -> if (paintsByDrag) paintStroke(listOf(x to y)) else Unit
        }
    }

    /** The Gradient tool's tap: a foreground to background sweep that ends at the canvas centre. */
    private fun gradientParams(x: Float, y: Float): JSONObject {
        val w = status?.active?.width?.toInt() ?: 1000
        val h = status?.active?.height?.toInt() ?: 1000
        return JSONObject()
            .put("from", org.json.JSONArray().put(x.toDouble()).put(y.toDouble()))
            .put("to", org.json.JSONArray().put((w / 2).toDouble()).put((h / 2).toDouble()))
            .put("colors", org.json.JSONArray().put(hex(foreground)).put(hex(background)))
    }

    /** The Eyedropper: the composite pixel under the finger becomes the foreground. */
    private fun pickColour(x: Float, y: Float) {
        viewModelScope.launch {
            try {
                val rgba = org.json.JSONArray(
                    engine.execute("document.sampleColor", JSONObject().put("x", x.toDouble()).put("y", y.toDouble())),
                )
                val channel = { i: Int -> Math.round(rgba.optDouble(i, 0.0) * 255.0).toInt().coerceIn(0, 255) }
                foreground = (0xFF shl 24) or (channel(0) shl 16) or (channel(1) shl 8) or channel(2)
            } catch (e: Exception) {
                error = e.message ?: "The colour couldn't be sampled."
            }
        }
    }

    /** A drag for the shape and lasso tools, from where it started to where it ended. */
    fun dragSelection(from: Pair<Float, Float>, to: Pair<Float, Float>) {
        val (x0, y0) = from
        val (x1, y1) = to
        val left = minOf(x0, x1)
        val top = minOf(y0, y1)
        val w = kotlin.math.abs(x1 - x0)
        val h = kotlin.math.abs(y1 - y0)
        if (w < 1f || h < 1f) return
        command(
            "select.rect",
            JSONObject()
                .put("x", left.toInt())
                .put("y", top.toInt())
                .put("width", w.toInt())
                .put("height", h.toInt())
                .put("ellipse", false),
        )
    }

    /**
     * A path's points, thinned: one every few pixels of travel. The engine needs the shape, and
     * the finger's hundreds of samples describe the same one.
     */
    private fun simplify(points: List<Pair<Float, Float>>, step: Float = 3f): List<Pair<Float, Float>> {
        val kept = mutableListOf<Pair<Float, Float>>()
        var last = points.first()
        kept += last
        for (p in points.drop(1)) {
            if (kotlin.math.abs(p.first - last.first) + kotlin.math.abs(p.second - last.second) >= step) {
                kept += p
                last = p
            }
        }
        // The end of the path is part of its shape, even when the last sample was close.
        if (kept.last() != points.last()) kept += points.last()
        // The engine builds a polygon from these points: it needs a triangle at least. A shorter
        // path made no gesture worth selecting, which `endDrag` doesn't send.
        return if (kept.size >= 3) kept else points
        return kept
    }

    /** A stroke's colour as `#rrggbb`, the engine's colour string. */
    private fun hex(c: Int): String = String.format("#%06X", c and 0xFFFFFF)

    /**
     * The part of the document the canvas shows at the current zoom and pan, capped at the size
     * the engine composites in one go (2048px a side): the preview is exactly this region, and
     * every pointer position is mapped through it, so what is drawn and what is painted agree.
     */
    fun documentView(): DocView? {
        val size = status?.active ?: return null
        val w = size.width.toFloat().coerceAtLeast(1f)
        val h = size.height.toFloat().coerceAtLeast(1f)
        var vw = (w / zoom).coerceIn(1f, w)
        var vh = (h / zoom).coerceIn(1f, h)
        // A window bigger than the preview cap is shown whole, scaled down to it.
        val over = maxOf(vw, vh) / 2048f
        if (over > 1f) {
            vw /= over
            vh /= over
        }
        // Pan is clamped to the document, so the picture never leaves the screen.
        return DocView(
            x = viewportX.toFloat().coerceIn(0f, (w - vw).coerceAtLeast(0f)),
            y = viewportY.toFloat().coerceIn(0f, (h - vh).coerceAtLeast(0f)),
            width = vw,
            height = vh,
        )
    }

    fun setZoom(z: Float) {
        // The canvas shows the document fitted, so zooming out past 100 % shows nothing new; the
        // range runs from the whole picture to 16x into it.
        zoom = z.coerceIn(1f, 16f)
        renderViewport()
    }

    fun pan(dx: Int, dy: Int) {
        // A drag pans what is visible, so the step is in the document's pixels, not the screen's:
        // at 4x zoom a finger travelling 100 screen pixels moves the window 25.
        val view = documentView() ?: return
        val w = status?.active?.width?.toInt() ?: return
        val h = status?.active?.height?.toInt() ?: return
        viewportX = (viewportX.toFloat() + dx / zoom).toInt().coerceIn(0, (w - view.width.toInt()).coerceAtLeast(0))
        viewportY = (viewportY.toFloat() + dy / zoom).toInt().coerceIn(0, (h - view.height.toInt()).coerceAtLeast(0))
        renderViewport()
    }

    fun setViewport(x: Int, y: Int) {
        viewportX = x
        viewportY = y
        renderViewport()
    }

    fun setBrushSize(s: Float) {
        brushSize = s.coerceIn(1f, 512f)
    }

    fun setBrushHardness(h: Float) {
        brushHardness = h.coerceIn(0f, 1f)
    }

    fun setBrushOpacity(o: Float) {
        brushOpacity = o.coerceIn(0f, 1f)
    }

    /**
     * Send the panel's size, hardness and opacity to the engine as its current brush, once the
     * finger lifts: the next stroke is painted with them.
     */
    fun commitBrush() {
        viewModelScope.launch {
            try {
                engine.execute(
                    "tools.setBrush",
                    JSONObject()
                        .put("size", brushSize)
                        .put("hardness", brushHardness)
                        .put("opacity", brushOpacity),
                )
            } catch (e: Exception) {
                error = e.message ?: "The brush couldn't be set."
            }
        }
    }

    fun setForeground(c: Int) {
        foreground = c
        // The engine's own foreground: the painting tools and Fill default to it.
        viewModelScope.launch {
            runCatching { engine.execute("tools.setColors", JSONObject().put("foreground", hex(c))) }
        }
    }

    fun setBackground(c: Int) {
        background = c
        viewModelScope.launch {
            runCatching { engine.execute("tools.setColors", JSONObject().put("background", hex(c))) }
        }
    }

    fun swapColors() {
        val t = foreground
        foreground = background
        background = t
        viewModelScope.launch {
            try {
                engine.execute("tools.swapColors", "{}")
            } catch (_: Exception) {
            }
        }
    }

    fun dismissError() {
        error = null
    }

    /** Report a failure the shell hit (a file that couldn't be written), as the editor's own. */
    fun showError(message: String) {
        error = message
    }

    /** Export the active document (a flat file, the layers kept in PSD/TIFF); the bytes go to [onBytes]. */
    fun export(format: String, quality: Int = 90, onBytes: (ByteArray, String) -> Unit) {
        viewModelScope.launch {
            busy = true
            error = null
            try {
                onBytes(engine.exportBytes(format, quality), format)
            } catch (e: Exception) {
                error = e.message ?: "The export failed."
            } finally {
                busy = false
            }
        }
    }

    private fun refresh() {
        viewModelScope.launch {
            try {
                status = engine.status()
                renderViewport()
                if (panel == Panel.Layers) refreshLayers()
            } catch (e: Exception) {
                error = e.message
            }
        }
    }

    private fun renderViewport() {
        renderJob?.cancel()
        renderJob = viewModelScope.launch {
            try {
                val size = engine.documentSize() ?: return@launch
                // The region the canvas shows: the whole document at 100 %, a window of it as
                // the zoom goes up. Rendering is capped at 2048px on a side, since the engine
                // composites on the CPU and a large window would preview slowly.
                val view = documentView() ?: return@launch
                val rw = view.width.toInt().coerceAtLeast(1)
                val rh = view.height.toInt().coerceAtLeast(1)
                bitmap = engine.renderRegion(view.x.toInt(), view.y.toInt(), rw, rh)
            } catch (_: Exception) {
                // No document yet: the canvas stays blank.
            }
        }
    }

    /**
     * The panel closed: drop the session, and with it the engine's documents and their pixels.
     * The Rust side owns the memory, so this is what actually frees it.
     */
    override fun onCleared() {
        super.onCleared()
        engine.release()
    }

    /** Reload the Layers panel now: after a layer command the panels don't cover. */
    fun refreshLayersNow() = refreshLayers()

    /**
     * The Layers panel's rows, flattened depth-first (groups included) from the engine's own
     * `document.inspect`: ids, names, kinds, visibility, opacity and blend mode.
     */
    private fun refreshLayers() {
        viewModelScope.launch {
            try {
                val doc = JSONObject(engine.execute("document.inspect", "{}"))
                val active = if (doc.isNull("activeLayer")) -1L else doc.optLong("activeLayer", -1L)
                val rows = ArrayList<LayerRow>()
                fun walk(nodes: org.json.JSONArray?, depth: Int) {
                    if (nodes == null) return
                    for (i in 0 until nodes.length()) {
                        val o = nodes.getJSONObject(i)
                        val id = o.optLong("id", -1L)
                        rows += LayerRow(
                            id = id,
                            name = o.optString("name", "Layer"),
                            kind = o.optString("kind", "raster"),
                            visible = o.optBoolean("visible", true),
                            opacity = o.optDouble("opacity", 1.0).toFloat(),
                            blend = o.optString("blend", "Normal"),
                            selected = o.optBoolean("selected", false),
                            active = id == active,
                            depth = depth,
                        )
                        walk(o.optJSONArray("children"), depth + 1)
                    }
                }
                walk(doc.optJSONArray("layers"), 0)
                layers = rows
            } catch (_: Exception) {
                // No document yet: the panel stays as it was.
            }
        }
    }

    private fun refreshCommands() {
        viewModelScope.launch {
            try {
                val arr = org.json.JSONArray(engine.execute("command.list", "{}"))
                val list = ArrayList<CommandRow>(arr.length())
                for (i in 0 until arr.length()) {
                    val o = arr.getJSONObject(i)
                    val menu = ArrayList<String>()
                    o.optJSONArray("menu")?.let { m -> for (k in 0 until m.length()) menu += m.getString(k) }
                    list += CommandRow(
                        id = o.optString("id"),
                        label = o.optString("label"),
                        menu = menu,
                        enabled = o.optBoolean("enabled", true),
                        disabledReason = if (o.isNull("disabledReason")) null else o.optString("disabledReason"),
                    )
                }
                commands = list
            } catch (_: Exception) {
                // No registry (an engine older than command.list): the panels stay empty.
            }
        }
    }

    /** The commands under a menu path, e.g. `["Image", "Adjustments"]`. */
    fun commandsUnder(vararg menu: String): List<CommandRow> =
        commands.filter { it.menu.size >= menu.size && it.menu.subList(0, menu.size) == menu.toList() }

    /** The commands whose id starts with [prefix] (the adjustment and filter families). */
    fun commandsStartingWith(prefix: String): List<CommandRow> = commands.filter { it.id.startsWith(prefix) }

    private fun refreshBrushes() {
        viewModelScope.launch {
            try {
                val json = engine.execute("brush.presets.list", "{}")
                val arr = JSONObject(json).optJSONArray("presets") ?: return@launch
                val list = mutableListOf<BrushPreset>()
                for (i in 0 until arr.length()) {
                    val o = arr.getJSONObject(i)
                    list += BrushPreset(
                        name = o.optString("name", "Brush"),
                        builtin = o.optBoolean("builtin", false),
                        size = o.optDouble("size", 32.0).toFloat(),
                        hardness = o.optDouble("hardness", 0.8).toFloat(),
                    )
                }
                brushes = list
            } catch (_: Exception) {
            }
        }
    }
}
