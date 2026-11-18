package io.github.artcraftandroid.photocraft.studio

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import io.github.artcraftandroid.photocraft.editor.Tool

/** Which geometry a drag draws in the Vector and Layout workspaces. */
enum class VectorTool { Rectangle, Ellipse, Pen, FrameRect, FrameEllipse }

/** The four workspaces the persona bar switches between. */
enum class WorkspaceId(val label: String, val title: String) {
    Sketch("Sketch", "Workout Meals — Logo studies"),
    Vector("Vector", "Otherworld — Botanical forms"),
    Pixel("Pixel", "Rouge — Portrait study"),
    Layout("Layout", "Showcase — Issue 04"),
}

/** One tool in the left rail. */
data class StudioTool(val id: String, val icon: String, val label: String)

/** One studio the right rail can open. */
data class Studio(val name: String, val icon: String, val label: String)

/** A workspace's tools, panel and title, exactly as the reference declares them. */
class WorkspaceSpec(
    val id: WorkspaceId,
    val tools: List<StudioTool>,
    val panel: String,
    val selected: String,
    /** The engine's tool a rail entry stands for. */
    val engineTool: (String) -> Tool,
)

private fun tool(id: String, icon: String, label: String) = StudioTool(id, icon, label)

/** The four workspaces: the reference's tools, in its order, with the engine's tool for each. */
object Workspaces {
    private val shared = listOf(tool("hand", "hand", "View Tool"), tool("zoom", "zoom", "Zoom Tool"))

    val all: Map<WorkspaceId, WorkspaceSpec> = mapOf(
        WorkspaceId.Sketch to WorkspaceSpec(
            WorkspaceId.Sketch,
            listOf(
                tool("move", "move", "Move Tool"), tool("pencil", "pencil", "Pencil Tool"),
                tool("brush", "brush", "Paint Brush Tool"), tool("eraser", "eraser", "Erase Brush Tool"),
                tool("fill", "fill", "Flood Fill Tool"), tool("rectangle", "rectangle", "Rectangle Tool"),
                tool("select", "marquee", "Selection Tool"),
            ) + shared,
            panel = "Brushes",
            selected = "pencil",
            engineTool = {
                when (it) {
                    "move" -> Tool.Move
                    "pencil" -> Tool.Pencil
                    "brush" -> Tool.Brush
                    "eraser" -> Tool.Eraser
                    "fill" -> Tool.Fill
                    "rectangle" -> Tool.Rect
                    "select" -> Tool.Lasso
                    "hand" -> Tool.Hand
                    else -> Tool.Zoom
                }
            },
        ),
        WorkspaceId.Vector to WorkspaceSpec(
            WorkspaceId.Vector,
            listOf(
                tool("move", "move", "Move Tool"), tool("select", "select", "Artboard Tool"),
                tool("node", "node", "Node Tool"), tool("corner", "curve", "Corner Tool"),
                tool("contour", "contrast", "Contour Tool"), tool("pen", "pen", "Pen Tool"),
                tool("pencil", "pencil", "Pencil Tool"), tool("brush", "brush", "Vector Brush Tool"),
                tool("fill", "fill", "Fill Tool"), tool("rectangle", "rectangle", "Rectangle Tool"),
                tool("ellipse", "circle", "Ellipse Tool"), tool("text", "text", "Artistic Text Tool"),
                tool("image", "image", "Place Image Tool"), tool("point", "wand", "Point Transform Tool"),
                tool("measure", "pencil", "Measure Tool"), tool("dropper", "dropper", "Colour Picker Tool"),
            ) + shared,
            panel = "Layers",
            selected = "node",
            engineTool = {
                when (it) {
                    "move", "node", "corner", "contour", "point", "measure" -> Tool.Move
                    "select" -> Tool.Lasso
                    "pen", "pencil" -> Tool.Pencil
                    "brush" -> Tool.Brush
                    "fill" -> Tool.Fill
                    "rectangle" -> Tool.Rect
                    "ellipse" -> Tool.Rect
                    "text" -> Tool.Type
                    "image" -> Tool.Move
                    "dropper" -> Tool.Eyedropper
                    "hand" -> Tool.Hand
                    else -> Tool.Zoom
                }
            },
        ),
        WorkspaceId.Pixel to WorkspaceSpec(
            WorkspaceId.Pixel,
            listOf(
                tool("move", "move", "Move Tool"), tool("crop", "crop", "Crop Tool"),
                tool("select", "select", "Selection Brush Tool"), tool("marquee", "marquee", "Rectangular Marquee Tool"),
                tool("wand", "wand", "Flood Select Tool"), tool("brush", "brush", "Paint Brush Tool"),
                tool("pencil", "pencil", "Pixel Tool"), tool("eraser", "eraser", "Erase Brush Tool"),
                tool("heal", "heal", "Healing Brush Tool"), tool("clone", "clone", "Clone Brush Tool"),
                tool("fill", "fill", "Flood Fill Tool"), tool("pen", "pen", "Pen Tool"),
                tool("rectangle", "rectangle", "Rectangle Tool"), tool("text", "text", "Artistic Text Tool"),
                tool("dropper", "dropper", "Colour Picker Tool"),
            ) + shared,
            panel = "Curves",
            selected = "move",
            engineTool = {
                when (it) {
                    "move" -> Tool.Move
                    "crop" -> Tool.Crop
                    "select" -> Tool.Lasso
                    "marquee" -> Tool.Lasso
                    "wand" -> Tool.Wand
                    "brush" -> Tool.Brush
                    "pencil", "pen" -> Tool.Pencil
                    "eraser" -> Tool.Eraser
                    "heal" -> Tool.Heal
                    "clone" -> Tool.Clone
                    "fill" -> Tool.Fill
                    "rectangle" -> Tool.Rect
                    "text" -> Tool.Type
                    "dropper" -> Tool.Eyedropper
                    "hand" -> Tool.Hand
                    else -> Tool.Zoom
                }
            },
        ),
        WorkspaceId.Layout to WorkspaceSpec(
            WorkspaceId.Layout,
            listOf(
                tool("move", "move", "Move Tool"), tool("node", "node", "Node Tool"),
                tool("text", "text", "Frame Text Tool"), tool("table", "table", "Table Tool"),
                tool("artistic", "text", "Artistic Text Tool"), tool("pen", "pen", "Pen Tool"),
                tool("rectangle", "rectangle", "Rectangle Tool"), tool("frame", "frame", "Picture Frame Rectangle Tool"),
                tool("ellipse", "circle", "Picture Frame Ellipse Tool"), tool("image", "image", "Place Image Tool"),
                tool("point", "wand", "Point Transform Tool"), tool("brush", "brush", "Vector Brush Tool"),
                tool("transparency", "contrast", "Transparency Tool"), tool("dropper", "dropper", "Colour Picker Tool"),
            ) + shared,
            panel = "Pages",
            selected = "text",
            engineTool = {
                when (it) {
                    "move", "node", "point", "transparency" -> Tool.Move
                    "text", "artistic" -> Tool.Type
                    "table", "rectangle", "frame" -> Tool.Rect
                    "pen" -> Tool.Pencil
                    "ellipse" -> Tool.Rect
                    "image" -> Tool.Place
                    "brush" -> Tool.Brush
                    "dropper" -> Tool.Eyedropper
                    "hand" -> Tool.Hand
                    else -> Tool.Zoom
                }
            },
        ),
    )

    /** The studios, in the rail's order. Colour is drawn as the swatch at the top of the rail. */
    val studios: List<Studio> = listOf(
        Studio("Colour", "circle", "Colour"),
        Studio("Swatches", "grid", "Swatches"),
        Studio("Stroke", "stroke", "Stroke"),
        Studio("Text", "text", "Text"),
        Studio("Paragraph", "list", "Paragraph"),
        Studio("Layers", "layers", "Layers"),
        Studio("Brushes", "brush", "Brushes"),
        Studio("Curves", "contrast", "Adjustments"),
        Studio("Effects", "effects", "Layer Effects"),
        Studio("Transform", "transform", "Transform"),
        Studio("Navigator", "navigator", "Navigator"),
        Studio("History", "history", "History"),
        Studio("Export", "export", "Export"),
    )

    val order: List<WorkspaceId> = listOf(WorkspaceId.Sketch, WorkspaceId.Vector, WorkspaceId.Pixel, WorkspaceId.Layout)

    fun of(id: WorkspaceId): WorkspaceSpec = all.getValue(id)
}

/** One curve point: 0..100 across, 0..100 down, as the reference's graph stores it. */
data class CurvePoint(val x: Float, val y: Float)

/** The reference's starting curve. */
val DefaultPoints = listOf(CurvePoint(0f, 100f), CurvePoint(22f, 80f), CurvePoint(66f, 30f), CurvePoint(100f, 0f))

/**
 * One workspace's interface state: the reference's `WorkspaceState`, field for field, plus what the
 * host adds (the document's size, the engine's layer list is not held here — it is read live).
 */
class StudioState(workspace: WorkspaceId) {
    var workspace by mutableStateOf(workspace)
    var selectedTool by mutableStateOf(Workspaces.of(workspace).selected)
    var panels by mutableStateOf(listOf(Workspaces.of(workspace).panel))
    var pinned by mutableStateOf(emptyList<String>())
    var selectedLayer by mutableStateOf("stem")
    val collapsed = mutableStateListOf<String>()
    val hidden = mutableStateListOf("sketch")
    val locked = mutableStateListOf("master")
    var page by mutableStateOf(8)
    var brush by mutableStateOf("HB Pencil")
    var search by mutableStateOf("")
    var category by mutableStateOf("Sketching")

    /** The panel values, by the reference's keys. */
    var values by mutableStateOf(
        mapOf(
            "opacity" to "100", "blend" to "Normal", "size" to "14", "brushSize" to "24",
            "brushOpacity" to "100", "colour" to "#f59ac9", "channel" to "Master", "colourSpace" to "RGB",
            "font" to "Instrument Serif", "fontStyle" to "Regular", "strokeWidth" to "0.7",
            "alignment" to "align", "nodeAction" to "Sharp node", "exportFormat" to "PNG",
            "strokeStyle" to "Solid", "outlineColour" to "#000000", "shapeStrokeWidth" to "0",
        ),
    )
    var toggles by mutableStateOf(listOf("snapping"))
    var points by mutableStateOf(DefaultPoints)
    var zoom by mutableStateOf(1f)
    var panX by mutableStateOf(0f)
    var panY by mutableStateOf(0f)

    /** The open document's size, for the canvas and the panels that show it. */
    var docWidth by mutableStateOf(1)
    var docHeight by mutableStateOf(1)

    fun value(key: String): String = values[key] ?: ""

    fun setValue(key: String, value: String) {
        values = values + (key to value)
    }

    fun toggle(key: String) {
        toggles = if (key in toggles) toggles - key else toggles + key
    }

    fun isOn(key: String): Boolean = key in toggles

    fun isPinned(name: String): Boolean = name in pinned

    /** The panel toggle: opening one keeps the pinned ones; closing one unpins it. */
    fun togglePanel(name: String) {
        panels = if (name in panels) {
            pinned = pinned - name
            panels - name
        } else {
            panels.filter { it in pinned } + name
        }
    }

    fun pin(name: String) {
        pinned = if (name in pinned) pinned - name else pinned + name
    }

    /** Back to the workspace's defaults, as the reference's Reset workspace does. */
    fun reset(id: WorkspaceId) {
        val spec = Workspaces.of(id)
        workspace = id
        selectedTool = spec.selected
        panels = listOf(spec.panel)
        pinned = emptyList()
        selectedLayer = "stem"
        collapsed.clear()
        hidden.clear(); hidden.add("sketch")
        locked.clear(); locked.add("master")
        page = 8
        brush = "HB Pencil"
        search = ""
        category = "Sketching"
        values = values + mapOf(
            "opacity" to "100", "blend" to "Normal", "size" to "14", "brushSize" to "24",
            "brushOpacity" to "100", "colour" to "#f59ac9", "channel" to "Master",
            "colourSpace" to "RGB", "font" to "Instrument Serif", "fontStyle" to "Regular",
            "strokeWidth" to "0.7", "alignment" to "align", "nodeAction" to "Sharp node",
            "strokeStyle" to "Solid", "exportFormat" to "PNG",
        )
        toggles = listOf("snapping")
        points = DefaultPoints
        zoom = 1f
        panX = 0f
        panY = 0f
    }

    /** The reference's `fit()`. */
    fun fit() {
        zoom = 1f
        panX = 0f
        panY = 0f
    }
}

/**
 * The brushes the panel lists: Procraft's own, in its own sets and order.
 *
 * These are not names this file invented. Each one is a brush from Procraft's
 * `crates/engine/src/library.rs`, and the engine's `LIBRARY` carries its parameters under the same
 * name -- so what the panel offers is what the engine draws. The list used to hold eight names of
 * its own, none of which matched the nine the engine had: picking most brushes sent the engine a
 * name it did not know, which is how every brush came to draw the same line.
 */
val StudioBrushes = listOf(
    // Sketching
    "Graphite", "Soft Pencil", "Technical Pencil", "HB Pencil", "6B Pencil", "Charcoal Pencil",
    // Inking
    "Fine Liner", "Brush Pen", "Round Pen", "Flat Brush",
    // Painting
    "Soft Airbrush",
)

/**
 * One brush, as the engine describes it.
 *
 * The panel draws each brush's own stroke from these numbers, so the list cannot show eight
 * identical squiggles for eight different brushes: the silhouette, the grain and the taper that
 * the canvas will use are the same numbers the sample is drawn from.
 */
data class BrushInfo(
    val name: String = "",
    val tip: String = "round",
    val hardness: Float = 0.85f,
    val grain: Float = 0f,
    val grainScale: Float = 0.35f,
    val grainMode: String = "canvas",
    val spacing: Float = 0.08f,
    val flow: Float = 1f,
    val roundness: Float = 1f,
    val rotation: Float = 0f,
    val follow: Float = 0f,
    val scatter: Float = 0f,
    val jitter: Float = 0f,
    val count: Int = 1,
    val taper: Float = 0f,
    val wet: Boolean = false,
    /** How its stamps build up within a stroke: "buildup", "glaze" or "gradual". */
    val rendering: String = "buildup",
    /** How far its path is pulled towards a smooth curve, 0..1. */
    val streamline: Float = 0f,
) {
    companion object {
        /** What the panel draws when the engine has not answered yet: a plain round pen. */
        val fallback = BrushInfo(name = "Round Pen", hardness = 0.92f, spacing = 0.08f)

        /** Read one brush from the engine's JSON. Absent settings keep the fallback's values. */
        fun from(o: org.json.JSONObject): BrushInfo {
            val f = fallback
            fun f32(key: String, d: Float) = o.optDouble(key, d.toDouble()).toFloat()
            return BrushInfo(
                name = o.optString("name").ifEmpty { o.optString("preset") }.ifEmpty { f.name },
                tip = o.optString("tip").ifEmpty { f.tip },
                hardness = f32("hardness", f.hardness),
                grain = f32("grain", f.grain),
                grainScale = f32("grainScale", f.grainScale),
                grainMode = o.optString("grainMode").ifEmpty { f.grainMode },
                rendering = o.optString("rendering").ifEmpty { f.rendering },
                streamline = f32("streamline", f.streamline),
                spacing = f32("spacing", f.spacing),
                flow = f32("flow", f.flow),
                roundness = f32("roundness", f.roundness),
                rotation = f32("rotation", f.rotation),
                follow = f32("follow", f.follow),
                scatter = f32("scatter", f.scatter),
                jitter = f32("jitter", f.jitter),
                count = o.optInt("count", f.count).coerceAtLeast(1),
                taper = f32("taper", f.taper),
                wet = o.optBoolean("wet", f.wet),
            )
        }
    }
}

/**
 * The brush sets the Brushes panel steps through with its arrows.
 *
 * Each set is a rule over the brush's name rather than a second list, so a brush added to the
 * engine shows up in the set it belongs to without being registered twice.
 */
object StudioBrushSets {
    /**
     * Procraft's own sets, in its own order.
     *
     * These used to be invented groupings guessed from the brush's name ("everything with
     * `pencil` in it"), which meant the sets did not match the brushes they held. Each brush now
     * knows the set it came from, and the sets are that list.
     */
    val names = listOf("Sketching", "Inking", "Painting", "All Brushes")

    /** The set each brush came from, by the name the engine knows it by. */
    private val byBrush: Map<String, String> = mapOf(
        "Graphite" to "Sketching", "Soft Pencil" to "Sketching",
        "Technical Pencil" to "Sketching", "HB Pencil" to "Sketching",
        "6B Pencil" to "Sketching", "Charcoal Pencil" to "Sketching",
        "Fine Liner" to "Inking", "Brush Pen" to "Inking", "Round Pen" to "Inking",
        "Flat Brush" to "Painting", "Soft Airbrush" to "Painting",
    )

    /** Whether a brush belongs to a set. */
    fun filters(set: String, brush: String): Boolean =
        set == "All Brushes" || byBrush[brush] == set
}

/** The Colour panel's swatches. */
val StudioSwatches = listOf(
    "#f29aca", "#f94c72", "#fd7e29", "#ffce22", "#d6fb30", "#40bb83",
    "#55e3f9", "#278aff", "#8965ef", "#121313", "#8e9296", "#ffffff",
)

/** The blend modes the Layers panel offers. */
val StudioBlends = listOf("Normal", "Multiply", "Screen", "Overlay", "Soft Light", "Colour")
