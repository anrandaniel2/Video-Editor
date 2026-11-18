package io.github.artcraftandroid.photocraft.studio

import android.graphics.Bitmap
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import io.github.artcraftandroid.filmcraft.core.PhotoStatus
import io.github.artcraftandroid.photocraft.editor.Tool
import io.github.artcraftandroid.photocraft.engine.PhotoEngine
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject

/**
 * The studio's link to PhotoCraft.
 *
 * The reference UI was a mock: its layer list was a literal, its Export button said file generation
 * "is not included in this UI replica", and its Curves note said "the portrait stays unchanged".
 * Here the same rows and buttons run PhotoCraft's own commands: the Layers panel lists the
 * document's real tree (and keeps the reference's rows for the mock document when there is none),
 * the sliders are `layer.setProps`, the curve is an adjustment layer, and Export returns the
 * engine's encoded bytes to the host.
 */
class StudioEngine(val engine: PhotoEngine = PhotoEngine()) {
    var status: PhotoStatus? by mutableStateOf(null)
        private set

    /** The composited document, as the canvas paints it. */
    var preview: Bitmap? by mutableStateOf(null)
        private set

    /** The document's layers, top first, flattened from the engine's tree. */
    var layers: List<StudioLayer> by mutableStateOf(emptyList())
        private set

    /** History labels, newest first. */
    var history: List<String> by mutableStateOf(emptyList())
        private set

    /** The document's pages, in the order the Pages panel lists them. */
    var pages: List<StudioPage> by mutableStateOf(emptyList())
        private set

    /** Which page the canvas, the layers and every tool act on. */
    var activePage: Int by mutableStateOf(0)
        private set

    /**
     * Which document this is.
     *
     * The revision only moves when the document changes; two different documents both start at
     * revision 1. So a canvas watching the revision cannot tell that the picture it was showing
     * has been replaced by another document -- which is how a studio that opened while a picture
     * was still decoding kept showing its own blank canvas.
     */
    var serial: Long by mutableStateOf(0L)
        private set

    var brushPresets: List<String> by mutableStateOf(emptyList())
        private set

    /**
     * The brushes with the settings that draw them.
     *
     * The names alone were enough for a list that only had to list; the panel now draws each
     * brush's own stroke and shows its tip, so the settings have to travel with the name.
     */
    var brushLibrary: List<BrushInfo> by mutableStateOf(emptyList())
        private set

    /** The brush in hand: what the panel's hardness, spacing, flow and taper controls edit. */
    var brush: BrushInfo by mutableStateOf(BrushInfo.fallback)
        private set

    val hasDocument: Boolean get() = status?.active != null

    /** UniFFI lifts Rust's `u64` as `ULong`; the engine's ids are signed here. */
    val activeLayer: Long? get() = status?.activeLayer?.toLong()

    val width: Int get() = status?.active?.width?.toInt()?.coerceAtLeast(1) ?: 1
    val height: Int get() = status?.active?.height?.toInt()?.coerceAtLeast(1) ?: 1

    // ---------------------------------------------------------------- documents

    suspend fun newDocument(width: Int, height: Int, background: String = "white") = withContext(Dispatchers.IO) {
        engine.execute(
            "file.new",
            JSONObject().put("width", width).put("height", height).put("background", background),
        )
        refreshAll()
    }

    suspend fun open(name: String, bytes: ByteArray) = withContext(Dispatchers.IO) {
        engine.openBytes(name, bytes)
        refreshAll()
    }

    suspend fun place(name: String, bytes: ByteArray) = withContext(Dispatchers.IO) {
        engine.placeBytes(name, bytes)
        refreshAll()
    }

    /** Run any engine command; [refresh] re-reads the document afterwards. */
    suspend fun run(id: String, params: JSONObject = JSONObject(), refresh: Boolean = true): String =
        withContext(Dispatchers.IO) {
            val out = engine.execute(id, params)
            if (refresh) refreshAll()
            out
        }

    suspend fun isEnabled(id: String): Boolean = withContext(Dispatchers.IO) { engine.isEnabled(id) }

    suspend fun undo(): Boolean = withContext(Dispatchers.IO) {
        val ok = runCatching { engine.undo() }.getOrDefault(false)
        refreshAll()
        ok
    }

    suspend fun redo(): Boolean = withContext(Dispatchers.IO) {
        val ok = runCatching { engine.redo() }.getOrDefault(false)
        refreshAll()
        ok
    }

    // ---------------------------------------------------------------- layers

    suspend fun selectLayer(id: Long) = withContext(Dispatchers.IO) {
        engine.execute("layer.select", JSONObject().put("layer", id))
        refreshAll()
    }

    suspend fun setVisible(id: Long, visible: Boolean) = withContext(Dispatchers.IO) {
        engine.execute("layer.setProps", JSONObject().put("layer", id).put("visible", visible))
        refreshAll()
    }

    /** Whether the layer is locked, from the list the panel already holds. */
    fun isLocked(id: Long): Boolean = layers.firstOrNull { it.id == id }?.locked == true

    suspend fun setLocked(id: Long, locked: Boolean) = withContext(Dispatchers.IO) {
        engine.execute("layer.setProps", JSONObject().put("layer", id).put("locked", locked))
        refreshAll()
    }

    suspend fun setOpacity(percent: Int) = withContext(Dispatchers.IO) {
        val id = activeLayer ?: return@withContext
        engine.execute(
            "layer.setProps",
            JSONObject().put("layer", id).put("opacity", (percent.coerceIn(0, 100)) / 100.0),
        )
        refreshAll()
    }

    suspend fun setBlend(mode: String) = withContext(Dispatchers.IO) {
        val id = activeLayer ?: return@withContext
        engine.execute("layer.setProps", JSONObject().put("layer", id).put("blend", mode))
        refreshAll()
    }

    suspend fun newLayer() = withContext(Dispatchers.IO) {
        engine.execute("layer.new.layer", JSONObject())
        refreshAll()
    }

    suspend fun duplicateLayer() = withContext(Dispatchers.IO) {
        engine.execute("layer.duplicate", JSONObject())
        refreshAll()
    }

    suspend fun deleteLayer() = withContext(Dispatchers.IO) {
        engine.execute("layer.delete", JSONObject())
        refreshAll()
    }

    suspend fun addMask() = withContext(Dispatchers.IO) {
        engine.execute("layer.layerMask.revealAll", JSONObject())
        refreshAll()
    }

    suspend fun addGroup() = withContext(Dispatchers.IO) {
        engine.execute("layer.new.group", JSONObject())
        refreshAll()
    }

    /** Delete the selection: the reference's Delete selection, made real. */
    suspend fun clearSelection() = withContext(Dispatchers.IO) {
        engine.execute("select.deselect", JSONObject())
        refreshAll()
    }

    // ---------------------------------------------------------------- paint

    /** The brush every painting tool uses: one command carries every setting the panels edit. */
    suspend fun setBrush(size: Float? = null, hardness: Float? = null, opacity: Float? = null, preset: String? = null) =
        withContext(Dispatchers.IO) {
            val p = JSONObject()
            if (size != null) p.put("size", size.toDouble().coerceIn(0.5, 5000.0))
            if (hardness != null) p.put("hardness", hardness.toDouble().coerceIn(0.0, 1.0))
            if (opacity != null) p.put("opacity", opacity.toDouble().coerceIn(0.0, 1.0))
            if (preset != null) p.put("preset", preset)
            if (p.length() == 0) return@withContext
            // The reply is the brush as the engine now holds it, named. Taking it means picking a
            // brush and the panel's own sliders can never describe different brushes.
            val reply = runCatching { JSONObject(engine.execute("tools.setBrush", p)) }.getOrNull()
            if (reply != null) brush = BrushInfo.from(reply)
        }

    /**
     * The colour well: the tools' colours, and the fill of the selected shape.
     *
     * A shape keeps its fill as text, so recolouring one is a change to its geometry -- and while
     * a shape is selected, picking a colour and having nothing happen would be the dead control
     * this is meant to be free of.
     */
    suspend fun setColours(foreground: String? = null, background: String? = null) = withContext(Dispatchers.IO) {
        val p = JSONObject()
        if (foreground != null) p.put("foreground", foreground)
        if (background != null) p.put("background", background)
        if (p.length() > 0) engine.execute("tools.setColors", p)
        val active = layers.firstOrNull { it.active }
        if (foreground != null && active?.kind == "shape") {
            runCatching {
                engine.execute(
                    "shape.setProps",
                    JSONObject().put("layer", active.id).put("fill", foreground),
                )
            }
            refreshAll()
        }
    }

    /** A finished drag as one stroke, so the History panel shows one step. */
    suspend fun stroke(
        points: List<Pair<Float, Float>>,
        colour: String,
        size: Float,
        opacity: Int = 100,
        tool: Tool = Tool.Brush,
    ) =
        withContext(Dispatchers.IO) {
            if (points.isEmpty()) return@withContext
            val arr = JSONArray()
            points.forEach { (x, y) -> arr.put(JSONArray().put(x.toDouble()).put(y.toDouble())) }
            val p = JSONObject()
                .put("points", arr)
                .put("color", colour)
                .put("size", size.toDouble())
                .put("opacity", opacity.coerceIn(0, 100))
                // One drag is one History row even when the canvas sends it in pieces, and the
                // gesture's name is what the engine coalesces on.
                .put("coalesce", "photocraft.stroke")
            // The rail's tool decides the tip: the pencil is hard and aliased, the eraser takes
            // paint away. Sending every drag as `paint.stroke` is what made the pencil a brush.
            if (tool == Tool.Eraser) p.put("erase", true)
            engine.execute(strokeCommand(tool), p)
            refreshAll()
        }

    /** The command id a painting tool sends: the pencil's hard tip and the eraser's `erase`. */
    private fun strokeCommand(tool: Tool): String = when (tool) {
        Tool.Pencil -> "paint.pencil"
        Tool.Eraser -> "paint.stroke"
        else -> "paint.stroke"
    }

    /** A tap by the selected tool: fill, wand, eyedropper, type, or a single dab. */
    suspend fun tap(tool: Tool, x: Float, y: Float, colour: String, brushSize: Float, brushOpacity: Int): String? =
        withContext(Dispatchers.IO) {
            val at = JSONObject().put("x", x.toDouble()).put("y", y.toDouble())
            when (tool) {
                Tool.Fill -> {
                    engine.execute("paint.bucket", at.put("color", colour))
                    refreshAll()
                    null
                }
                Tool.Wand -> {
                    engine.execute("select.magicWand", at.put("tolerance", 32))
                    refreshAll()
                    null
                }
                Tool.Eyedropper -> {
                    val arr = JSONArray(engine.execute("document.sampleColor", at))
                    refreshAll()
                    val r = (arr.optDouble(0, 0.0) * 255).toInt().coerceIn(0, 255)
                    val g = (arr.optDouble(1, 0.0) * 255).toInt().coerceIn(0, 255)
                    val b = (arr.optDouble(2, 0.0) * 255).toInt().coerceIn(0, 255)
                    String.format("#%02X%02X%02X", r, g, b)
                }
                Tool.Type -> {
                    engine.execute("type.create", at.put("text", "Text").put("size", 64))
                    refreshAll()
                    null
                }
                else -> {
                    stroke(listOf(x to y, x to y), colour, brushSize, brushOpacity, tool)
                    null
                }
            }
        }

    /**
     * A drag by a vector tool: a rectangle, an ellipse, a pen path or a picture frame, kept as
     * geometry rather than painted into the pixels.
     *
     * This is what separates the Vector and Layout workspaces from the Pixel one. The same drag
     * that paints a stroke in Pixel makes a shape here, and the shape can be recoloured, restroked
     * or moved afterwards without its edge being resampled.
     */
    suspend fun shapeDrag(
        kind: String,
        from: Pair<Float, Float>,
        to: Pair<Float, Float>,
        colour: String,
        outline: String,
        strokeWidth: Float,
    ): String = withContext(Dispatchers.IO) {
        val (x0, y0) = from
        val (x1, y1) = to
        val p = JSONObject()
            .put("kind", if (kind == "ellipse") "ellipse" else "rectangle")
            .put("x", minOf(x0, x1).toDouble())
            .put("y", minOf(y0, y1).toDouble())
            .put("width", kotlin.math.abs(x1 - x0).toDouble())
            .put("height", kotlin.math.abs(y1 - y0).toDouble())
            .put("fill", colour)
            .put("strokeWidth", strokeWidth.toDouble())
        if (strokeWidth > 0f) p.put("stroke", outline)
        // A drag too small to be a shape is not one: a tap that slipped should not leave a dot.
        if (kotlin.math.abs(x1 - x0) < 1f || kotlin.math.abs(y1 - y0) < 1f) {
            return@withContext ""
        }
        val out = engine.execute("shape.create", p)
        refreshAll()
        runCatching { JSONObject(out).optLong("layer", -1L) }.getOrDefault(-1L).takeIf { it > 0 }?.toString() ?: ""
    }

    /**
     * The Pen tool's drag: a path of nodes, kept as geometry.
     *
     * The Pen was mapped to the pencil, so a drag in the Vector workspace painted pixels -- in a
     * workspace whose whole point is that nothing is painted. The nodes are the drag's own points,
     * reduced to roughly one per pointer event so the path has as many handles as it has corners.
     */
    suspend fun penPath(
        points: List<Pair<Float, Float>>,
        colour: String,
        outline: String,
        strokeWidth: Float,
    ): String = withContext(Dispatchers.IO) {
        if (points.size < 2) return@withContext ""
        val xs = points.map { it.first }
        val ys = points.map { it.second }
        val left = xs.min().toDouble()
        val top = ys.min().toDouble()
        val right = xs.max().toDouble()
        val bottom = ys.max().toDouble()
        if (right - left < 1.0 || bottom - top < 1.0) return@withContext ""
        val nodes = org.json.JSONArray()
        // The drag's points, thinned: consecutive samples a pixel apart are the same corner.
        var lastX = Float.NaN
        var lastY = Float.NaN
        points.forEach { (x, y) ->
            if (kotlin.math.abs(x - lastX) < 1.5f && kotlin.math.abs(y - lastY) < 1.5f) return@forEach
            lastX = x
            lastY = y
            nodes.put(
                JSONObject()
                    .put("x", x.toDouble())
                    .put("y", y.toDouble()),
            )
        }
        val p = JSONObject()
            .put("kind", "path")
            .put("x", left)
            .put("y", top)
            .put("width", right - left)
            .put("height", bottom - top)
            .put("nodes", nodes)
            .put("closed", false)
            .put("fill", org.json.JSONObject.NULL)
            .put("stroke", if (strokeWidth > 0f) outline else colour)
            .put("strokeWidth", if (strokeWidth > 0f) strokeWidth.toDouble() else 2.0)
        val out = engine.execute("shape.create", p)
        refreshAll()
        runCatching { JSONObject(out).optLong("layer", -1L) }.getOrDefault(-1L).takeIf { it > 0 }?.toString() ?: ""
    }

    /** A picture frame: a shape of its own kind, filled with the picture that is placed in it. */
    suspend fun frameDrag(
        ellipse: Boolean,
        from: Pair<Float, Float>,
        to: Pair<Float, Float>,
    ): String = withContext(Dispatchers.IO) {
        val (x0, y0) = from
        val (x1, y1) = to
        if (kotlin.math.abs(x1 - x0) < 1f || kotlin.math.abs(y1 - y0) < 1f) return@withContext ""
        val p = JSONObject()
            .put("kind", if (ellipse) "ellipse" else "rectangle")
            .put("x", minOf(x0, x1).toDouble())
            .put("y", minOf(y0, y1).toDouble())
            .put("width", kotlin.math.abs(x1 - x0).toDouble())
            .put("height", kotlin.math.abs(y1 - y0).toDouble())
            // A frame is a placeholder until a picture is placed in it, so it is drawn as an
            // outline only: an empty frame that looked filled would be a lie about its contents.
            .put("fill", org.json.JSONObject.NULL)
            .put("stroke", "#8A8F86")
            .put("strokeWidth", 1.0)
        val out = engine.execute("shape.create", p)
        refreshAll()
        runCatching { JSONObject(out).optLong("layer", -1L) }.getOrDefault(-1L).takeIf { it > 0 }?.toString() ?: ""
    }

    /** A marquee or lasso drag: the selection the tools that do not paint make on a drag. */
    suspend fun selectByDrag(tool: Tool, points: List<Pair<Float, Float>>) = withContext(Dispatchers.IO) {
        if (points.isEmpty()) return@withContext
        val p = JSONObject()
        if (tool == Tool.Rect || tool == Tool.Move) {
            // A rectangle from the drag's first point to its last, whatever path it took.
            val xs = points.map { it.first }
            val ys = points.map { it.second }
            p.put("x", xs.min().toDouble()).put("y", ys.min().toDouble())
                .put("width", (xs.max() - xs.min()).toDouble())
                .put("height", (ys.max() - ys.min()).toDouble())
            engine.execute("select.rect", p)
        } else {
            val arr = JSONArray()
            points.forEach { (x, y) -> arr.put(JSONArray().put(x.toDouble()).put(y.toDouble())) }
            engine.execute("select.lasso", p.put("points", arr))
        }
        refreshAll()
    }

    /** The Curves panel's graph, applied as the adjustment layer the panel's heading names. */
    suspend fun applyCurves(points: List<CurvePoint>, channel: String, opacity: Int): String = withContext(Dispatchers.IO) {
        // The graph is 0..100 top-down; the engine's points are [in, out] in 0..255, out up.
        val arr = JSONArray()
        points.sortedBy { it.x }.forEach { p ->
            arr.put(
                JSONArray()
                    .put((p.x * 2.55f).toInt().coerceIn(0, 255))
                    .put(((100f - p.y) * 2.55f).toInt().coerceIn(0, 255)),
            )
        }
        val params = JSONObject()
        when (channel) {
            "Red" -> params.put("red", arr)
            "Green" -> params.put("green", arr)
            "Blue" -> params.put("blue", arr)
            else -> params.put("points", arr)
        }
        val out = engine.execute("layer.newAdjustmentLayer.curves", params)
        activeLayer?.let { id ->
            if (opacity < 100) {
                engine.execute(
                    "layer.setProps",
                    JSONObject().put("layer", id).put("opacity", opacity.coerceIn(0, 100) / 100.0),
                )
            }
        }
        refreshAll()
        out
    }

    /** Export: the engine encodes the document and the shell writes the bytes. */
    suspend fun export(format: String, quality: Int = 90): ByteArray = withContext(Dispatchers.IO) {
        engine.exportBytes(format, quality)
    }

    // ---------------------------------------------------------------- reads

    /**
     * Re-read the document's identity and status, without re-rendering or re-walking the layers.
     *
     * This is what a caller waiting for a particular document needs: the studio can be opened on
     * a document that is about to be replaced, so "is there a document?" is not enough, and asking
     * for a full refresh on every poll would render the whole picture each time.
     */
    suspend fun refreshDocument() = withContext(Dispatchers.IO) {
        status = runCatching { engine.status() }.getOrNull()
        runCatching {
            val doc = JSONObject(engine.execute("document.inspect", "{}"))
            activePage = doc.optInt("activePage", 0)
            serial = doc.optLong("serial", 0L)
        }
    }

    suspend fun refreshAll() = withContext(Dispatchers.IO) {
        // The status is what `hasDocument` reads, so it has to be re-read here: a document can be
        // opened or closed without the layer walk noticing.
        status = runCatching { engine.status() }.getOrNull()
        refreshPreview()
        refreshLayers()
    }

    suspend fun refreshPreview(maxSide: Int = 2048) = withContext(Dispatchers.IO) {
        preview = runCatching { engine.render(maxSide, maxSide) }.getOrNull()
    }

    /** The document's layer tree, top first, plus its history labels. */
    suspend fun refreshLayers() = withContext(Dispatchers.IO) {
        val rows = ArrayList<StudioLayer>()
        runCatching {
            val doc = JSONObject(engine.execute("document.inspect", "{}"))
            val activeId = doc.optLong("activeLayer", -1L)
            fun walk(nodes: JSONArray?, depth: Int) {
                if (nodes == null) return
                for (i in 0 until nodes.length()) {
                    val o = nodes.optJSONObject(i) ?: continue
                    val id = o.optLong("id", -1L)
                    val kind = o.optString("kind", "pixels")
                    rows += StudioLayer(
                        id = id,
                        name = o.optString("name", "Layer"),
                        kind = kind,
                        depth = depth,
                        group = kind == "group",
                        visible = o.optBoolean("visible", true),
                        locked = o.optBoolean("locked", false),
                        opacity = o.optDouble("opacity", 1.0).toFloat(),
                        blend = o.optString("blend", "Normal"),
                        selected = o.optBoolean("selected", false),
                        active = id == activeId,
                    )
                    walk(o.optJSONArray("children"), depth + 1)
                }
            }
            walk(doc.optJSONArray("layers"), 0)
            // The document's real pages, so the Pages panel shows what exists rather than a
            // reference spread. A document always has at least one.
            pages = doc.optJSONArray("pages")?.let { arr ->
                (0 until arr.length()).mapNotNull { i ->
                    arr.optJSONObject(i)?.let { p ->
                        StudioPage(
                            index = p.optInt("index", i),
                            name = p.optString("name", "Page ${i + 1}"),
                            width = p.optInt("width", 0),
                            height = p.optInt("height", 0),
                            layers = p.optInt("layers", 0),
                            active = p.optBoolean("active", i == doc.optInt("activePage", 0)),
                        )
                    }
                }
            } ?: emptyList()
            activePage = doc.optInt("activePage", 0)
            serial = doc.optLong("serial", 0L)
            history = doc.optJSONArray("history")?.let { arr ->
                (arr.length() - 1 downTo 0).mapNotNull { i -> historyEntry(arr.opt(i), arr.length() - 1 - i) }
            } ?: emptyList()
        }
        layers = rows
    }

    /** A History row's name: the engine's label, beside the step number it holds. */
    private fun historyEntry(value: Any?, index: Int): String? = when (value) {
        is String -> value
        is JSONObject -> value.optString("name").ifEmpty { value.optString("label") }.ifEmpty { null }
        else -> null
    }

    suspend fun refreshBrushes() = withContext(Dispatchers.IO) {
        val library = runCatching {
            val o = JSONObject(engine.execute("brush.presets.list", "{}"))
            val arr = o.optJSONArray("presets") ?: JSONArray()
            (0 until arr.length()).mapNotNull { arr.optJSONObject(it)?.let(BrushInfo::from) }
        }.getOrDefault(emptyList())
        brushLibrary = library
        brushPresets = library.map { it.name }
        // The library is a catalogue; the brush in hand is the one the engine stored, with whatever
        // the sliders have changed about it. Only a studio with nothing in hand takes a default.
        if (brush.name.isEmpty()) {
            brush = library.firstOrNull() ?: BrushInfo.fallback
        }
    }

    /**
     * The brush's own settings: tip, spacing, flow, taper and the rest.
     *
     * `setBrush` picks a brush whole; this edits the one in hand, which is what makes the panel's
     * controls change the next stroke instead of resetting when the hand moves.
     */
    suspend fun setBrushParams(params: JSONObject) = withContext(Dispatchers.IO) {
        val reply = runCatching { JSONObject(engine.execute("brush.set", params)) }.getOrNull()
        if (reply != null) brush = BrushInfo.from(reply)
    }

    /**
     * Nudge the active layer: the Move tool, which had no way to move anything.
     *
     * The offset is a property of the layer, so a moved picture keeps its own pixels and a second
     * nudge adds to the first rather than compounding a resample.
     */
    suspend fun moveLayer(dx: Float, dy: Float, coalesce: String = "photocraft.move") =
        withContext(Dispatchers.IO) {
            if (dx == 0f && dy == 0f) return@withContext
            engine.execute(
                "layer.move",
                JSONObject()
                    .put("dx", dx.toDouble())
                    .put("dy", dy.toDouble())
                    .put("coalesce", coalesce),
            )
            refreshAll()
        }

    /**
     * A drag with the Crop tool: the page cut down to the rectangle that was dragged.
     *
     * The rail's crop was mapped to the Rect tool, so the tool made a rectangle layer instead of
     * cutting the picture down -- a tool whose name and whose consequence disagreed.
     */
    suspend fun cropDrag(from: Pair<Float, Float>, to: Pair<Float, Float>) =
        withContext(Dispatchers.IO) {
            val (x0, y0) = from
            val (x1, y1) = to
            // A drag too small to be a crop would leave a one-pixel page; it is refused rather
            // than guessed at.
            if (kotlin.math.abs(x1 - x0) < 2f || kotlin.math.abs(y1 - y0) < 2f) return@withContext
            engine.execute(
                "image.crop",
                JSONObject()
                    .put("x", minOf(x0, x1).toDouble())
                    .put("y", minOf(y0, y1).toDouble())
                    .put("width", kotlin.math.abs(x1 - x0).toDouble())
                    .put("height", kotlin.math.abs(y1 - y0).toDouble()),
            )
            refreshAll()
        }

    /**
     * A drag with the Clone tool: the pixels under the source point, laid along the path.
     *
     * The rail has offered Clone since the restore and the engine has had `paint.cloneStamp` all
     * along; nothing called it, so the tool was a cursor. The source is where the drag started,
     * which is the one point a rail without a separate source-picker can offer honestly.
     */
    suspend fun cloneDrag(points: List<Pair<Float, Float>>, size: Float, opacity: Int) =
        withContext(Dispatchers.IO) {
            if (points.size < 2) return@withContext
            val (sx, sy) = points.first()
            val arr = JSONArray()
            points.forEach { (x, y) -> arr.put(JSONArray().put(x.toDouble()).put(y.toDouble())) }
            engine.execute(
                "paint.cloneStamp",
                JSONObject()
                    .put("points", arr)
                    .put("source", JSONArray().put(sx.toDouble()).put(sy.toDouble()))
                    .put("size", size.toDouble())
                    .put("opacity", opacity.coerceIn(0, 100))
                    .put("coalesce", "photocraft.clone"),
            )
            refreshAll()
        }

    /** A drag with the Heal tool: the blurred composite, mixed in under the path. */
    suspend fun healDrag(points: List<Pair<Float, Float>>, size: Float) =
        withContext(Dispatchers.IO) {
            if (points.size < 2) return@withContext
            val arr = JSONArray()
            points.forEach { (x, y) -> arr.put(JSONArray().put(x.toDouble()).put(y.toDouble())) }
            engine.execute(
                "paint.spotHealing",
                JSONObject()
                    .put("points", arr)
                    .put("size", size.toDouble())
                    .put("coalesce", "photocraft.heal"),
            )
            refreshAll()
        }

    /** A drag with the Gradient tool: front colour to back colour, along the drag. */
    suspend fun gradientDrag(from: Pair<Float, Float>, to: Pair<Float, Float>) =
        withContext(Dispatchers.IO) {
            val (x0, y0) = from
            val (x1, y1) = to
            // A drag too short to give a direction falls back to left-to-right across the whole
            // canvas, rather than failing on a tap that slipped.
            val flat = kotlin.math.abs(x1 - x0) < 1f && kotlin.math.abs(y1 - y0) < 1f
            val p = JSONObject()
                .put("x", x0.toDouble())
                .put("y", y0.toDouble())
                .put("x2", (if (flat) width.toFloat() else x1).toDouble())
                .put("y2", (if (flat) y0 else y1).toDouble())
            engine.execute("paint.gradient", p)
            refreshAll()
        }

    fun release() = engine.release()
}

/** One row of the Pages panel: the engine's own page. */
data class StudioPage(
    val index: Int,
    val name: String,
    val width: Int,
    val height: Int,
    val layers: Int,
    val active: Boolean,
)

/** One row of the Layers panel: the engine's own layer. */
data class StudioLayer(
    val id: Long,
    val name: String,
    val kind: String,
    val depth: Int,
    val group: Boolean,
    val visible: Boolean,
    val locked: Boolean,
    val opacity: Float,
    val blend: String,
    val selected: Boolean,
    val active: Boolean,
)

