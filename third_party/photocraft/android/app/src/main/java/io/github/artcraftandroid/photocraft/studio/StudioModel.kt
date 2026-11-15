package io.github.artcraftandroid.photocraft.studio

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import io.github.artcraftandroid.photocraft.editor.Tool

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
                    "crop" -> Tool.Rect
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
                    "image" -> Tool.Move
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
    var collapsed by mutableStateListOf<String>()
    var hidden by mutableStateListOf("sketch")
    var locked by mutableStateListOf("master")
    var page by mutableStateOf(8)
    var brush by mutableStateOf("Natural Pencil 4B")
    var search by mutableStateOf("")
    var category by mutableStateOf("Sketching")

    /** The panel values, by the reference's keys. */
    var values by mutableStateOf(
        mapOf(
            "opacity" to "100", "blend" to "Normal", "size" to "14", "brushSize" to "24",
            "brushOpacity" to "100", "colour" to "#f59ac9", "channel" to "Master", "colourSpace" to "RGB",
            "font" to "Instrument Serif", "fontStyle" to "Regular", "strokeWidth" to "0.7",
            "alignment" to "align", "nodeAction" to "Sharp node", "exportFormat" to "PNG",
            "strokeStyle" to "Solid",
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
        brush = "Natural Pencil 4B"
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

/** The brush names the Brushes panel lists, as the reference declares them. */
val StudioBrushes = listOf(
    "Natural Pencil 6B", "Natural Pencil 2B", "Natural Pencil 4B", "Compressed Charcoal",
    "Dense Graphite", "Round Dry Bristle", "Soft Chalk", "Fine Ink",
)

/** The Colour panel's swatches. */
val StudioSwatches = listOf(
    "#f29aca", "#f94c72", "#fd7e29", "#ffce22", "#d6fb30", "#40bb83",
    "#55e3f9", "#278aff", "#8965ef", "#121313", "#8e9296", "#ffffff",
)

/** The blend modes the Layers panel offers. */
val StudioBlends = listOf("Normal", "Multiply", "Screen", "Overlay", "Soft Light", "Colour")
