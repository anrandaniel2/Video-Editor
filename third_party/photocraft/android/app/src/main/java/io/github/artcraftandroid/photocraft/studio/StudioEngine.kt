package io.github.artcraftandroid.photocraft.studio

import android.graphics.Bitmap
import io.github.artcraftandroid.photocraft.core.PhotoStatus
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
    var status: PhotoStatus? = null
        private set

    /** The composited document, as the canvas paints it. */
    var preview: Bitmap? = null
        private set

    /** The document's layers, top first, flattened from the engine's tree. */
    var layers: List<StudioLayer> = emptyList()
        private set

    /** History labels, newest first. */
    var history: List<String> = emptyList()
        private set

    var brushPresets: List<String> = emptyList()
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
            if (p.length() > 0) engine.execute("tools.setBrush", p)
        }

    suspend fun setColours(foreground: String? = null, background: String? = null) = withContext(Dispatchers.IO) {
        val p = JSONObject()
        if (foreground != null) p.put("foreground", foreground)
        if (background != null) p.put("background", background)
        if (p.length() > 0) engine.execute("tools.setColors", p)
    }

    /** A finished drag as one stroke, so the History panel shows one step. */
    suspend fun stroke(points: List<Pair<Float, Float>>, colour: String, size: Float, opacity: Int = 100) =
        withContext(Dispatchers.IO) {
            if (points.isEmpty()) return@withContext
            val arr = JSONArray()
            points.forEach { (x, y) -> arr.put(JSONArray().put(x.toDouble()).put(y.toDouble())) }
            val p = JSONObject()
                .put("points", arr)
                .put("color", colour)
                .put("size", size.toDouble())
                .put("opacity", opacity.coerceIn(0, 100))
            engine.execute("paint.stroke", p)
            refreshAll()
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
                    stroke(listOf(x to y, x to y), colour, brushSize, brushOpacity)
                    null
                }
            }
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

    suspend fun refreshAll() = withContext(Dispatchers.IO) {
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
                        opacity = o.optDouble("opacity", 1.0).toFloat(),
                        blend = o.optString("blend", "Normal"),
                        selected = o.optBoolean("selected", false),
                        active = id == activeId,
                    )
                    walk(o.optJSONArray("children"), depth + 1)
                }
            }
            walk(doc.optJSONArray("layers"), 0)
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
        brushPresets = runCatching {
            val o = JSONObject(engine.execute("brush.presets.list", "{}"))
            val arr = o.optJSONArray("presets") ?: JSONArray()
            (0 until arr.length()).mapNotNull { arr.optJSONObject(it)?.optString("name")?.takeIf(String::isNotEmpty) }
        }.getOrDefault(emptyList())
    }

    fun release() = engine.release()
}

/** One row of the Layers panel: the engine's own layer. */
data class StudioLayer(
    val id: Long,
    val name: String,
    val kind: String,
    val depth: Int,
    val group: Boolean,
    val visible: Boolean,
    val opacity: Float,
    val blend: String,
    val selected: Boolean,
    val active: Boolean,
)

/**
 * A row of the reference's mock document. The studio shows these only until a document is open (the
 * engine's layers replace them), so the reference's own list is what the interface shows at rest.
 */
data class MockLayer(
    val id: String,
    val name: String,
    val kind: String,
    val depth: Int,
    val group: Boolean = false,
    val parent: String? = null,
)

val MockLayers = listOf(
    MockLayer("master", "Master A", "text", 0),
    MockLayer("text", "Text", "text", 0),
    MockLayer("illustration", "Illustration", "image", 0, group = true),
    MockLayer("flower", "2", "brush", 1, group = true, parent = "illustration"),
    MockLayer("stem", "Stem", "pen", 2, parent = "flower"),
    MockLayer("highlights", "Highlights", "circle", 3, parent = "flower"),
    MockLayer("shadows", "Shadows", "circle", 3, parent = "flower"),
    MockLayer("stripes", "Stripes", "stroke", 2, parent = "flower"),
    MockLayer("gradient", "Gradient", "curve", 2, parent = "flower"),
    MockLayer("mushroom", "Mushroom", "image", 1, group = true, parent = "illustration"),
    MockLayer("sketch", "Sketch", "pencil", 0),
    MockLayer("pixel", "Pixel", "image", 0, group = true),
    MockLayer("brightness", "Brightness / Contrast", "contrast", 1, parent = "pixel"),
    MockLayer("curves", "Curves Adjustment", "curve", 1, parent = "pixel"),
    MockLayer("mask", "Mask", "mask", 1, parent = "pixel"),
)
