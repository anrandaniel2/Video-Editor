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
import kotlinx.coroutines.sync.withLock
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
    Ellipse("Ellipse"),
    Line("Line"),
    Frame("Frame"),
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
    /** The classic editor, open over the studio's own interface. */
    Classic,
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

/**
 * One point of a stroke as the engine's wire form needs it: position, pressure, tilt, and the time
 * since the stroke began. The studio stamps every point this way; without the pressure and the
 * clock the engine's pressure curves and its speed-based spacing never engage, which is what made
 * the photo editor's strokes feel unlike the drawing app's.
 */
data class StrokePoint(
    val x: Float,
    val y: Float,
    val pressure: Float = 1f,
    val tiltX: Float = 0f,
    val tiltY: Float = 0f,
    val rotation: Float = 0f,
    val timeMs: Long = 0L,
    val wheel: Float = 0f,
)

/**
 * The drawing app's brush shelf, as the panel lists it: the same 134 brushes the studio ships, in
 * their sets. The photo editor's six generic presets are replaced by this on open, so a brush is
 * the same brush in both editors.
 */
data class BrushSet(val name: String, val brushes: List<String>)

/** One brush preset, as the Brushes panel lists it. */
data class BrushPreset(
    val name: String,
    val builtin: Boolean,
    val size: Float,
    val hardness: Float,
    /** The brush's own stabilisation (null: it has none), so a pick can keep its feel. */
    val smoothing: Float? = null,
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

    /**
     * The studio's interface over this model's session: the reference UI's workspaces, rails and
     * panels driving the same engine, so its layers are this document's and its export comes back
     * through the host's save path.
     */
    val studioEngine: io.github.artcraftandroid.photocraft.studio.StudioEngine by lazy {
        io.github.artcraftandroid.photocraft.studio.StudioEngine(engine)
    }

    /** The active document's name, for the studio's title. */
    fun activeName(): String = status?.active?.name ?: "Untitled"

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

    /** The same brushes grouped by set, as the studio's shelf has them. */
    var brushSets by mutableStateOf<List<BrushSet>>(emptyList())
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

    /** The preset the panel picked last, for the row's highlight; null until one is picked. */
    var selectedBrush by mutableStateOf<String?>(null)
        private set
    // ARGB, as Android's Color: the literals are explicitly Int (0xFF000000 alone is a Long).
    var foreground by mutableStateOf(0xFF000000.toInt())
        private set
    var background by mutableStateOf(0xFFFFFFFF.toInt())
        private set

    private var renderJob: Job? = null
    /** The job that opens or creates the first document: [awaitDocument] joins it. */
    private var openJob: Job? = null

    init {
        refreshCommands()
        // The drawing app's brushes, not the engine's generic six: the panel paints with the same
        // library as the studio, so a brush means the same thing in both editors.
        installStudioBrushes()
    }

    fun chooseMode(m: Mode) {
        mode = m
        // Each mode brings the tool that mode is for. Vector used to select the *pixel* brush,
        // which painted raster strokes, and Layout selected Move, which does nothing on a document
        // with no artboards, so both modes looked broken: nothing a person did produced the
        // Vector or Layout result.
        tool = when (m) {
            Mode.Pixel -> Tool.Brush
            Mode.Sketch -> Tool.Pencil
            // The shape tools: a drag makes a vector shape layer (`shape.create`).
            Mode.Vector -> Tool.Rect
            // A page to lay out on: the Layout mode's first move is an artboard.
            Mode.Layout -> Tool.Frame
            Mode.Canva -> Tool.Wand
        }
        // Layout on a document with no artboard has nothing to lay out: make the page.
        if (m == Mode.Layout) ensureArtboard()
    }

    /** Whether the current tool drags out a vector shape. */
    private fun drawsVector(): Boolean = mode == Mode.Vector && tool in setOf(Tool.Rect, Tool.Ellipse, Tool.Line)

    /**
     * The Layout workspace's page. A document opened as a photo has one flat layer and no board,
     * so the first thing Layout needs is a page; this adds one at A4 and selects it, once.
     */
    private fun ensureArtboard() {
        viewModelScope.launch {
            runCatching {
                val has = JSONObject(engine.execute("document.inspect", "{}"))
                    .optJSONArray("layers")?.length() ?: 0
                // `layer.new.artboard` is cheap and idempotent enough for a mode switch: it only
                // runs when the document has nothing that looks like a page yet.
                if (has <= 1) {
                    engine.execute(
                        "layer.new.artboard",
                        JSONObject().put("preset", "A4").put("name", "Page 1").put("background", "white"),
                    )
                    refresh()
                }
            }
        }
    }

    fun chooseTool(t: Tool) {
        tool = t
        cloneSource = null
        if (t == Tool.Type) panel = Panel.Type
    }

    fun choosePanel(p: Panel) {
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
    // A Compose state, not a plain field: the canvas keys its drag detector on it, and the
    // detector has to be reinstalled the moment the source is picked (a plain `var` in a view
    // model does not recompose, which is a trap this code has been in before).
    private var cloneSource by mutableStateOf<Pair<Float, Float>?>(null)

    /** Where the Clone Stamp copies from, for the canvas to mark once it is picked. */
    val cloneFrom: Pair<Float, Float>? get() = cloneSource

    /** The stroke the finger is drawing, in document pixels, shown over the preview as it goes. */
    var strokePts by mutableStateOf<List<androidx.compose.ui.geometry.Offset>>(emptyList())
        private set

    /**
     * The engine's pixels for the stroke being drawn: one damage rect of the composite of the
     * document with the stroke so far, per frame the stroke changed. The canvas draws them in
     * order over its preview, so the brush's own dabs appear under the finger, not a line
     * standing in for them.
     */
    var strokeParts by mutableStateOf<List<Pair<androidx.compose.ui.geometry.Rect, Bitmap>>>(emptyList())
        private set

    /**
     * How many of the drag's points the running live stroke has been given. The canvas re-sends
     * the whole path each move, so this is what makes only the new samples join the stroke.
     */
    private var liveSent = 0
    private var liveQueue = kotlinx.coroutines.channels.Channel<StrokePoint>(kotlinx.coroutines.channels.Channel.UNLIMITED)
    private var liveJob: Job? = null
    private val liveGate = kotlinx.coroutines.sync.Mutex()

    /** Whether the painting tools draw live (the Brush, Pencil and Eraser do). */
    private fun paintsLive(): Boolean = tool in setOf(Tool.Brush, Tool.Pencil, Tool.Eraser)

    fun previewStroke(points: List<StrokePoint>) {
        // Until the engine's first pixels answer, the path stands in for the brush (see the canvas);
        // once they do, they are the stroke.
        strokePts = points.map { androidx.compose.ui.geometry.Offset(it.x, it.y) }
        if (!paintsLive()) return
        if (points.size > liveSent) {
            points.drop(liveSent).forEach { liveQueue.trySend(it) }
            liveSent = points.size
        }
    }

    /**
     * Start the live stroke under the finger: the engine paints it as it goes, and the commit on
     * release takes those pixels (one History step), so a long stroke is one Undo.
     */
    fun beginStroke(first: StrokePoint) {
        strokeParts = emptyList()
        liveSent = 1
        if (tool == Tool.Clone && cloneSource == null) {
            error = "Tap the area to clone from first."
            return
        }
        if (!paintsLive()) return
        // One stroke at a time: the new stroke waits for the last one's commit, so two previews
        // are never in the engine's slot at once (a quick dab after a quick stroke). The last
        // stroke's queue is closed in case the gesture never ended (a detached window).
        val previous = liveJob
        liveQueue.close()
        val queue = kotlinx.coroutines.channels.Channel<StrokePoint>(kotlinx.coroutines.channels.Channel.UNLIMITED)
        liveQueue = queue
        liveJob = viewModelScope.launch(kotlinx.coroutines.Dispatchers.IO) {
            previous?.join()
            val began = runCatching {
                engine.strokeBegin(strokeCommand(), strokeParams(listOf(first)).toString())
            }.getOrElse {
                error = it.message ?: "The stroke couldn't be painted."
                false
            }
            if (!began) {
                // Nothing to paint on (a hidden or locked layer): the canvas takes the drag's
                // placeholder path, and nothing is committed on release.
                strokePts = emptyList()
                return@launch
            }
            // The stroke's own first dab, drawn now: the brush shows under the finger from the
            // first touch, not from the first move.
            liveGate.withLock { refreshTile(runCatching { engine.strokeBounds() }.getOrNull()) }
            // An airbrush keeps laying dabs while the finger rests; smoothing keeps catching up.
            val ticking = java.util.concurrent.atomic.AtomicBoolean(true)
            val ticker = launch {
                while (ticking.get()) {
                    kotlinx.coroutines.delay(33)
                    if (runCatching { engine.strokeWantsTime() }.getOrDefault(false)) {
                        liveGate.withLock { refreshTile(runCatching { engine.strokeTick() }.getOrNull()) }
                    }
                }
            }
            try {
                for (p in queue) {
                    liveGate.withLock {
                        // A batch of one: the input loop keeps sending, the engine keeps up.
                        refreshTile(runCatching { engine.strokeAdd(pointsJson(listOf(p))) }.getOrNull())
                    }
                }
            } finally {
                ticking.set(false)
                ticker.cancel()
                liveGate.withLock {
                    // The commit: the engine takes the pixels it previewed, so the stroke is one
                    // History step and nothing is re-rendered.
                    val ended = runCatching { engine.strokeEnd() }
                    if (ended.isFailure) {
                        runCatching { engine.strokeCancel() }
                        error = ended.exceptionOrNull()?.message ?: "The stroke couldn't be painted."
                    }
                    // The composite first, then the preview dropped: no frame in between shows
                    // the canvas without the stroke it has just committed.
                    renderInto()
                    strokeParts = emptyList()
                    strokePts = emptyList()
                    status = runCatching { engine.status() }.getOrNull() ?: status
                    if (panel == Panel.Layers) refreshLayers()
                }
            }
        }
    }

    /**
     * End the live stroke: close the queue so the consumer commits it. Whether the finger lifted or
     * the gesture was taken away, the stroke is one History step; a new queue takes the next one
     * (closing a channel is final).
     */
    fun endStroke() {
        // The next stroke gets a new queue: closing a channel is final.
        liveQueue.close()
        // The next stroke gets a new queue, and waits on this one's commit (see beginStroke).
        liveQueue = kotlinx.coroutines.channels.Channel<StrokePoint>(kotlinx.coroutines.channels.Channel.UNLIMITED)
        liveSent = 0
    }

    /** A batch as the wire form: `[[x, y, pressure, tiltX, tiltY, rotation, timeMs, wheel], …]`. */
    private fun pointsJson(points: List<StrokePoint>): String {
        val arr = org.json.JSONArray()
        for (q in points) {
            arr.put(
                org.json.JSONArray().put(q.x.toDouble()).put(q.y.toDouble())
                    .put(q.pressure.toDouble().coerceIn(0.0, 1.0))
                    .put(q.tiltX.toDouble()).put(q.tiltY.toDouble()).put(q.rotation.toDouble())
                    .put(q.timeMs.toDouble()).put(q.wheel.toDouble()),
            )
        }
        return arr.toString()
    }

    /** Fetch the pixels of a damage rect (JSON `{x, y, width, height}`) for the canvas. */
    private suspend fun refreshTile(damage: String?) {
        val d = damage?.let { runCatching { JSONObject(it) }.getOrNull() } ?: return
        val x = d.optInt("x")
        val y = d.optInt("y")
        val w = d.optInt("width")
        val h = d.optInt("height")
        if (w <= 0 || h <= 0) return
        val frame = androidx.compose.ui.geometry.Rect(x.toFloat(), y.toFloat(), (x + w).toFloat(), (y + h).toFloat())
        // A long stroke lays many small rectangles; past a few dozen, fold them into one that
        // covers the stroke's whole reach, so a long stroke costs one bitmap, not one per frame.
        val kept = strokeParts
        val union = kept.fold(frame) { a, p ->
            androidx.compose.ui.geometry.Rect(
                minOf(a.left, p.first.left), minOf(a.top, p.first.top),
                maxOf(a.right, p.first.right), maxOf(a.bottom, p.first.bottom),
            )
        }
        val fetch = if (kept.size >= 24) union else frame
        val fw = fetch.width.toInt().coerceAtLeast(1)
        val fh = fetch.height.toInt().coerceAtLeast(1)
        val bmp = runCatching {
            engine.strokePixels(fetch.left.toInt(), fetch.top.toInt(), fw, fh)
        }.getOrNull() ?: return
        strokeParts = if (kept.size >= 24) listOf(fetch to bmp) else kept + (frame to bmp)
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
                // A non-live path (a tap with the Heal tool, say) has no stamped samples: the
                // positions become points with full pressure and no elapsed time, which is what
                // the engine used for every point before the live pipeline existed.
                val stamped = points.map { (x, y) -> StrokePoint(x, y) }
                engine.execute(strokeCommand(), strokeParams(stamped))
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
    private fun strokeParams(points: List<StrokePoint>): JSONObject {
        val p = JSONObject()
        val pts = org.json.JSONArray()
        for (q in points) {
            // The studio's eight values: the engine reads what it needs and ignores the rest, so a
            // plain drag (no stylus, no tilt) still gets pressure 1 and a real clock.
            pts.put(
                org.json.JSONArray().put(q.x.toDouble()).put(q.y.toDouble())
                    .put(q.pressure.toDouble().coerceIn(0.0, 1.0))
                    .put(q.tiltX.toDouble()).put(q.tiltY.toDouble()).put(q.rotation.toDouble())
                    .put(q.timeMs.toDouble()).put(q.wheel.toDouble()),
            )
        }
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
    fun endDrag(points: List<StrokePoint>, live: Boolean = false) {
        strokePts = emptyList()
        val first = points.firstOrNull() ?: return
        val last = points.last()
        // The other tools think in plain positions; the stroke's pressure and clock are the
        // engine's business.
        val flat = points.map { it.x to it.y }
        when {
            // A live stroke is already in the engine (it committed on release), so there is
            // nothing left to send for the painting tools.
            // A Vector-mode drag makes a shape layer: the mode's whole point.
            drawsVector() -> createShape(tool, first.x to first.y, last.x to last.y)
            live && paintsLive() -> Unit
            paintsByDrag -> paintStroke(flat)
            tool == Tool.Frame -> createArtboard(first.x to first.y, last.x to last.y)
            tool == Tool.Rect -> dragSelection(first.x to first.y, last.x to last.y)
            // The Lasso follows the finger: its path is the selection's outline, thinned to the
            // corners of the shape it drew, so a long gesture doesn't send hundreds of points.
            tool == Tool.Lasso && simplify(flat).size >= 3 -> {
                val array = org.json.JSONArray()
                for ((x, y) in simplify(flat)) array.put(org.json.JSONArray().put(x.toDouble()).put(y.toDouble()))
                command("select.lasso", JSONObject().put("points", array))
            }
            else -> tapAt(first.x, first.y)
        }
    }

    /** What a tap (or a drag that a tap-only tool receives) does, by tool. */
    fun tapAt(x: Float, y: Float) {
        when (tool) {
            Tool.Fill -> command("paint.bucket", JSONObject().put("x", x.toDouble()).put("y", y.toDouble()).put("tolerance", 32).put("color", hex(foreground)))
            Tool.Gradient -> command("paint.gradient", gradientParams(x, y))
            Tool.Wand -> command("select.magicWand", JSONObject().put("x", x.toDouble()).put("y", y.toDouble()).put("tolerance", 32))
            Tool.Eyedropper -> pickColour(x, y)
            Tool.Zoom -> zoomTo((zoom * 1.5f).coerceAtMost(16f))
            // Move has nothing to move yet (no layer transform in this build); the marquee tools
            // need a drag, and a tap gives them a 1x1 box, which selects nothing on purpose.
            Tool.Move, Tool.Lasso, Tool.Rect, Tool.Ellipse, Tool.Line -> Unit
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

    /**
     * A Vector-mode drag: the shape layer the engine draws, with the foreground as its fill and its
     * outline. A drag too small to be a shape still makes a default-sized one, as the studio's
     * shape tool does, so a tap in Vector mode produces something.
     */
    fun createShape(kind: Tool, from: Pair<Float, Float>, to: Pair<Float, Float>) {
        val x = minOf(from.first, to.first)
        val y = minOf(from.second, to.second)
        val w = kotlin.math.abs(to.first - from.first)
        val h = kotlin.math.abs(to.second - from.second)
        val (bw, bh) = if (w < 3f || h < 3f) 160f to 120f else w to h
        val kindName = when (kind) {
            Tool.Ellipse -> "ellipse"
            Tool.Line -> "line"
            else -> "rect"
        }
        val p = JSONObject().put("kind", kindName)
        if (kindName == "line") {
            p.put("from", org.json.JSONArray().put(from.first.toDouble()).put(from.second.toDouble()))
            p.put("to", org.json.JSONArray().put(to.first.toDouble()).put(to.second.toDouble()))
        } else {
            p.put("rect", org.json.JSONArray().put(x.toDouble()).put(y.toDouble()).put(bw.toDouble()).put(bh.toDouble()))
        }
        // `fill` takes the colour itself; `{"color": …}` is refused (that is the paint.* shape).
        p.put("fill", hex(foreground))
        p.put("stroke", JSONObject().put("width", 2).put("color", hex(foreground)))
        command("shape.create", p)
    }

    /**
     * A Layout-mode drag: a new artboard at the dragged rectangle. A tap makes an A4 page, which is
     * what the mode is for.
     */
    fun createArtboard(from: Pair<Float, Float>, to: Pair<Float, Float>) {
        val x = minOf(from.first, to.first)
        val y = minOf(from.second, to.second)
        val w = kotlin.math.abs(to.first - from.first)
        val h = kotlin.math.abs(to.second - from.second)
        // `index` is a UInt (the engine's type), so it is widened before the arithmetic.
        val page = (status?.active?.index?.toInt() ?: 0) + 1
        val p = JSONObject().put("background", "white").put("name", "Page $page")
        if (w >= 8f && h >= 8f) {
            p.put("rect", org.json.JSONArray().put(x.toDouble()).put(y.toDouble()).put(w.toDouble()).put(h.toDouble()))
        } else {
            p.put("preset", "A4")
        }
        command("layer.new.artboard", p)
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
        // path made no gesture worth selecting, which `endDrag` doesn't send the lasso.
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

    fun zoomTo(z: Float) {
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

    fun chooseBrushSize(s: Float) {
        brushSize = s.coerceIn(1f, 512f)
    }

    fun chooseBrushHardness(h: Float) {
        brushHardness = h.coerceIn(0f, 1f)
    }

    fun chooseBrushOpacity(o: Float) {
        brushOpacity = o.coerceIn(0f, 1f)
    }

    /**
     * Pick a preset: the engine resolves it (its own tip, dynamics, stabilisation and size), and
     * the panel's sliders take the values it settled on.
     *
     * Without the read-back the sliders kept the previous brush's numbers, so the next
     * `commitBrush` wrote them straight back over the preset just picked: the brush did not change
     * until something else refreshed the panel, which is why a picked brush only appeared after
     * painting and opening the panel again.
     */
    fun pickPreset(name: String) {
        viewModelScope.launch {
            error = null
            try {
                val p = JSONObject().put("preset", name)
                // Smoothing is a tool option, so `tools.setBrush` keeps the tool's instead of the
                // preset's. The studio's brushes are made with their own stabilisation, and the
                // photo editor is supposed to stroke the way the studio does, so send the
                // preset's own amount back and let the brush be the brush it was drawn as.
                brushes.firstOrNull { it.name == name }?.smoothing?.let { p.put("smoothing", it) }
                engine.execute("tools.setBrush", p)
                selectedBrush = name
                readBrush()
                renderInto()
            } catch (e: Exception) {
                error = e.message ?: "The brush couldn't be picked."
            }
        }
    }

    /** The engine's current brush, as the panel's sliders show it. */
    private suspend fun readBrush() {
        val b = runCatching { engine.brush() }.getOrNull() ?: return
        brushSize = b.optDouble("size", brushSize.toDouble()).toFloat().coerceIn(1f, 512f)
        brushHardness = b.optDouble("hardness", brushHardness.toDouble()).toFloat().coerceIn(0f, 1f)
        brushOpacity = b.optDouble("opacity", brushOpacity.toDouble()).toFloat().coerceIn(0f, 1f)
    }

    /** The panel's brush thumbnails, by preset name and size, drawn once and kept. Concurrent: the
     * rows ask from their own coroutines. */
    private val thumbs = java.util.concurrent.ConcurrentHashMap<String, android.graphics.Bitmap>()

    /**
     * A preset's thumbnail: the engine's own stroke with that brush, fitted to [width] × [height].
     * Null when the engine cannot draw it (an unknown name), so the row keeps its placeholder.
     */
    suspend fun brushThumbnail(name: String, width: Int = 120, height: Int = 24): android.graphics.Bitmap? {
        val key = "$name@$width×$height"
        thumbs[key]?.let { return it }
        val bmp = runCatching {
            engine.brushPreview(width, height, JSONObject().put("preset", name).toString())
        }.getOrNull() ?: return null
        thumbs[key] = bmp
        return bmp
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

    fun pickForeground(c: Int) {
        foreground = c
        // The engine's own foreground: the painting tools and Fill default to it.
        viewModelScope.launch {
            runCatching { engine.execute("tools.setColors", JSONObject().put("foreground", hex(c))) }
        }
    }

    fun pickBackground(c: Int) {
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
        viewModelScope.launch { refreshNow() }
    }

    private suspend fun refreshNow() {
        try {
            status = engine.status()
            renderInto()
            if (panel == Panel.Layers) refreshLayers()
            if (panel == Panel.Brushes) readBrush()
        } catch (e: Exception) {
            error = e.message
        }
    }

    private fun renderViewport() {
        renderJob?.cancel()
        renderJob = viewModelScope.launch { renderInto() }
    }

    /**
     * Render the region the canvas shows into [bitmap]. Suspend so a caller that has just committed
     * a stroke can wait for the composite before dropping the live preview: a frame between the
     * two must not show the canvas without the stroke it has just committed.
     */
    private suspend fun renderInto() {
        try {
            val size = engine.documentSize() ?: return
            // The region the canvas shows: the whole document at 100 %, a window of it as
            // the zoom goes up. Rendering is capped at 2048px on a side, since the engine
            // composites on the CPU and a large window would preview slowly.
            val view = documentView() ?: return
            val rw = view.width.toInt().coerceAtLeast(1)
            val rh = view.height.toInt().coerceAtLeast(1)
            bitmap = engine.renderRegion(view.x.toInt(), view.y.toInt(), rw, rh)
        } catch (_: Exception) {
            // No document yet: the canvas stays blank.
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
                val sets = linkedMapOf<String, MutableList<String>>()
                for (i in 0 until arr.length()) {
                    val o = arr.getJSONObject(i)
                    val name = o.optString("name", "Brush")
                    list += BrushPreset(
                        name = name,
                        builtin = o.optBoolean("builtin", false),
                        size = o.optDouble("size", 32.0).toFloat(),
                        hardness = o.optDouble("hardness", 0.8).toFloat(),
                        smoothing = if (o.isNull("smoothing")) null else o.optDouble("smoothing").toFloat(),
                    )
                    // The set the engine files the brush under (the studio's own grouping).
                    val set = o.optString("group").ifEmpty { "Custom" }
                    sets.getOrPut(set) { mutableListOf() }.add(name)
                }
                brushes = list
                brushSets = sets.map { (k, v) -> BrushSet(k, v) }
            } catch (_: Exception) {
            }
        }
    }

    /**
     * Put the drawing app's brush library in place of the engine's generic six, then list it. The
     * studio does this for its own document; the photo editor's Brushes panel should paint with
     * the same brushes, so the same shelf goes in before the list is read.
     */
    fun installStudioBrushes() {
        viewModelScope.launch {
            runCatching { engine.studioInstallBrushes() }
            readBrush()
            refreshBrushes()
        }
    }
}
