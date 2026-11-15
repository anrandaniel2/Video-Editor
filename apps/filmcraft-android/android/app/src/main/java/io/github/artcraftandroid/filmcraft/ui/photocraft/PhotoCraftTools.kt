package io.github.artcraftandroid.filmcraft.ui.photocraft

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import io.github.artcraftandroid.filmcraft.ui.Glyph
import io.github.artcraftandroid.filmcraft.ui.LucideIcons
import io.github.artcraftandroid.filmcraft.ui.glyph

/**
 * PhotoCraft's tool palette, the way the desktop editor lays it out: every group of the Photoshop
 * lineage is here, and every tool opens its own options bar. The palette is data — [PhotoTool]
 * carries the label, group, icon, hint and options — so the screen only draws it.
 */
enum class ToolGroup(val label: String) {
    Tone("Tone"),
    Navigate("Navigate"),
    Select("Select"),
    Frame("Frame"),
    Retouch("Retouch"),
    Paint("Paint"),
    Fill("Fill"),
    Vector("Vector"),
    Type("Type"),
    ;

    /** The tools of this group, in palette order. */
    val tools: List<PhotoTool> get() = PhotoTool.entries.filter { it.group == this }

    /** The glyph the group's chip shows: its first tool's. */
    val glyph: Glyph get() = tools.first().glyph
}

/** One control of an options bar. Every tool's options are a list of these. */
sealed interface ToolOption {
    /** A slider from [min] to [max] with a unit suffix, e.g. "40 px" or "100%". */
    data class Slider(
        val key: String,
        val label: String,
        val min: Float,
        val max: Float,
        val default: Float,
        val suffix: String = "",
    ) : ToolOption

    /** A row of chips; the value stored is the selected index. */
    data class Choice(
        val key: String,
        val label: String,
        val choices: List<String>,
        val default: Int = 0,
    ) : ToolOption

    /** A chip that is either on or off. */
    data class Switch(val key: String, val label: String, val default: Boolean = false) : ToolOption
}

/** What a tool's options bar can do to the picture right away: the actions the native pipeline has. */
enum class ToolAction(val label: String, val glyph: Glyph) {
    RotateLeft("Rotate left", LucideIcons.RotateCcw.glyph()),
    RotateRight("Rotate right", LucideIcons.RotateCw.glyph()),
    FlipHorizontal("Flip H", LucideIcons.FlipHorizontal2.glyph()),
    FlipVertical("Flip V", LucideIcons.FlipVertical2.glyph()),
    AutoTone("Auto", LucideIcons.WandSparkles.glyph()),
}

/** Where a tool's options are edited: the tone panels, the crop frame, or its own options bar. */
enum class ToolPanel { Adjust, Filters, Crop, Sample, View, Options }

/**
 * The tool values the options bars set, kept across tool switches so a brush size survives a detour
 * through the crop tool. Keyed by tool and option name, so two tools can both have a "size".
 */
class PhotoToolValues {
    private val numbers = mutableStateMapOf<String, Float>()
    private val choices = mutableStateMapOf<String, Int>()
    private val switches = mutableStateMapOf<String, Boolean>()

    private fun key(tool: PhotoTool, key: String) = "${tool.name}.$key"

    fun number(tool: PhotoTool, option: ToolOption.Slider): Float =
        numbers[key(tool, option.key)] ?: option.default

    fun setNumber(tool: PhotoTool, option: ToolOption.Slider, value: Float) {
        numbers[key(tool, option.key)] = value
    }

    fun choice(tool: PhotoTool, option: ToolOption.Choice): Int =
        choices[key(tool, option.key)] ?: option.default

    fun setChoice(tool: PhotoTool, option: ToolOption.Choice, index: Int) {
        choices[key(tool, option.key)] = index
    }

    fun switch(tool: PhotoTool, option: ToolOption.Switch): Boolean =
        switches[key(tool, option.key)] ?: option.default

    fun setSwitch(tool: PhotoTool, option: ToolOption.Switch, on: Boolean) {
        switches[key(tool, option.key)] = on
    }

    private val strings = mutableStateMapOf<String, String>()

    fun text(tool: PhotoTool, key: String, default: String): String = strings[this.key(tool, key)] ?: default

    fun setText(tool: PhotoTool, key: String, value: String) {
        strings[this.key(tool, key)] = value
    }

    /** The colour the brush, shapes, type and gradients use; the second one is the gradient's end. */
    var color by mutableStateOf(Color(0xFF17171B))
    var endColor by mutableStateOf(Color(0xFFF4F4F6))
}

/** The way a tool puts marks on the picture, which is what the canvas does with a drag or a tap. */
enum class PaintKind { None, Stroke, Pen, Erase, Shape, Text, Gradient, Bucket, Selection, Retouch, Navigate }

/** What this tool paints, if it paints at all. Retouch tools stay with the vendored engine. */
fun PhotoTool.paintKind(): PaintKind = when (this) {
    PhotoTool.Brush, PhotoTool.Pencil, PhotoTool.MixerBrush, PhotoTool.ColorReplacement,
    PhotoTool.CloneStamp, PhotoTool.PatternStamp, PhotoTool.HistoryBrush, PhotoTool.ArtHistoryBrush,
    -> PaintKind.Stroke
    PhotoTool.Eraser, PhotoTool.BackgroundEraser, PhotoTool.MagicEraser -> PaintKind.Erase
    PhotoTool.Pen, PhotoTool.FreeformPen, PhotoTool.CurvaturePen -> PaintKind.Pen
    PhotoTool.Rectangle, PhotoTool.Ellipse, PhotoTool.TriangleShape, PhotoTool.Polygon,
    PhotoTool.StarShape, PhotoTool.Line, PhotoTool.CustomShape, PhotoTool.FrameTool,
    -> PaintKind.Shape
    PhotoTool.HorizontalType, PhotoTool.VerticalType, PhotoTool.TypeMask, PhotoTool.VerticalTypeMask -> PaintKind.Text
    PhotoTool.Gradient -> PaintKind.Gradient
    PhotoTool.PaintBucket -> PaintKind.Bucket
    PhotoTool.Marquee, PhotoTool.EllipseMarquee, PhotoTool.RowMarquee, PhotoTool.ColumnMarquee,
    PhotoTool.Lasso, PhotoTool.PolygonalLasso, PhotoTool.MagneticLasso,
    -> PaintKind.Selection
    PhotoTool.Hand, PhotoTool.Zoom -> PaintKind.Navigate
    PhotoTool.HealingBrush, PhotoTool.SpotHealing, PhotoTool.Patch, PhotoTool.ContentAwareMove,
    PhotoTool.RedEye, PhotoTool.CloneStamp, PhotoTool.PatternStamp, PhotoTool.BlurTool,
    PhotoTool.SharpenTool, PhotoTool.SmudgeTool, PhotoTool.Dodge, PhotoTool.Burn, PhotoTool.Sponge,
    -> PaintKind.Retouch
    else -> PaintKind.None
}

/** Reads one option by name, whatever tool carries it: the canvas needs size, opacity and the rest. */
fun PhotoToolValues.slider(tool: PhotoTool, key: String, fallback: Float): Float =
    tool.options.filterIsInstance<ToolOption.Slider>().firstOrNull { it.key == key }?.let { number(tool, it) } ?: fallback

fun PhotoToolValues.choice(tool: PhotoTool, key: String): String? =
    tool.options.filterIsInstance<ToolOption.Choice>().firstOrNull { it.key == key }
        ?.let { option -> option.choices.getOrNull(choice(tool, option)) }

fun PhotoToolValues.flag(tool: PhotoTool, key: String, fallback: Boolean = false): Boolean =
    tool.options.filterIsInstance<ToolOption.Switch>().firstOrNull { it.key == key }?.let { switch(tool, it) } ?: fallback

/** The view the Navigate tools share with the picture: zoom, pan and the colours sampled so far. */
class PhotoViewState {
    var zoom by mutableFloatStateOf(1f)
    var pan by mutableStateOf(Offset.Zero)
    val samples = mutableStateListOf<Color>()

    fun reset() {
        zoom = 1f
        pan = Offset.Zero
    }

    fun zoomTo(value: Float) {
        zoom = value.coerceIn(1f, 8f)
        if (zoom <= 1.001f) pan = Offset.Zero
    }

    fun addSample(color: Color) {
        samples.add(0, color)
        while (samples.size > 8) samples.removeAt(samples.size - 1)
    }
}

// ---- option sets shared by families of tools ----

private val BrushOptions = listOf(
    ToolOption.Slider("size", "Size", 1f, 300f, 40f, " px"),
    ToolOption.Slider("opacity", "Opacity", 0f, 100f, 100f, "%"),
    ToolOption.Slider("hardness", "Hardness", 0f, 100f, 80f, "%"),
    ToolOption.Slider("flow", "Flow", 0f, 100f, 100f, "%"),
)

private val SelectionOptions = listOf(
    ToolOption.Choice("mode", "Mode", listOf("New", "Add", "Subtract", "Intersect")),
    ToolOption.Slider("feather", "Feather", 0f, 100f, 0f, " px"),
    ToolOption.Switch("antialias", "Anti-alias", true),
    ToolOption.Switch("allLayers", "Sample all layers", true),
)

private val MagicSelectOptions = listOf(
    ToolOption.Slider("tolerance", "Tolerance", 0f, 100f, 32f),
    ToolOption.Switch("contiguous", "Contiguous", true),
    ToolOption.Switch("antialias", "Anti-alias", true),
    ToolOption.Switch("allLayers", "Sample all layers", true),
)

private val RetouchOptions = listOf(
    ToolOption.Slider("size", "Size", 1f, 300f, 40f, " px"),
    ToolOption.Slider("opacity", "Opacity", 0f, 100f, 100f, "%"),
    ToolOption.Slider("hardness", "Hardness", 0f, 100f, 60f, "%"),
    ToolOption.Switch("allLayers", "Sample all layers", true),
)

private val PaintBlebOptions = listOf(
    ToolOption.Slider("size", "Size", 1f, 300f, 40f, " px"),
    ToolOption.Slider("strength", "Strength", 0f, 100f, 50f, "%"),
    ToolOption.Switch("allLayers", "Sample all layers", true),
)

private val ToningOptions = listOf(
    ToolOption.Slider("size", "Size", 1f, 300f, 60f, " px"),
    ToolOption.Slider("exposure", "Exposure", 0f, 100f, 50f, "%"),
    ToolOption.Choice("range", "Range", listOf("Shadows", "Midtones", "Highlights")),
    ToolOption.Switch("protect", "Protect tones", true),
)

private val ShapeOptions = listOf(
    ToolOption.Choice("fill", "Fill", listOf("Solid", "Gradient", "None")),
    ToolOption.Slider("stroke", "Stroke", 0f, 40f, 0f, " px"),
    ToolOption.Slider("radius", "Corner radius", 0f, 200f, 0f, " px"),
    ToolOption.Switch("fromCentre", "From centre", false),
)

/** The eyedropper's own options, shared by the sampler. */
private val SampleOptions = listOf(
    ToolOption.Choice("sample", "Sample", listOf("Point", "3 × 3", "5 × 5")),
    ToolOption.Switch("allLayers", "Sample all layers", true),
)

private val TypeOptions = listOf(
    ToolOption.Slider("size", "Size", 8f, 288f, 48f, " px"),
    ToolOption.Choice("font", "Font", listOf("Inter", "Noto Serif", "JetBrains Mono")),
    ToolOption.Choice("align", "Align", listOf("Left", "Centre", "Right")),
    ToolOption.Switch("antialias", "Anti-alias", true),
)

/**
 * The palette. Labels follow the desktop editor so the screen reads the same; the hint says what
 * the tool does on this picture.
 */
enum class PhotoTool(
    val label: String,
    val group: ToolGroup,
    val glyph: Glyph,
    val hint: String,
    val panel: ToolPanel = ToolPanel.Options,
    val options: List<ToolOption> = emptyList(),
    val actions: List<ToolAction> = emptyList(),
) {
    // ---- tone: the panels this screen really runs ----
    Adjust("Adjust", ToolGroup.Tone, LucideIcons.WandSparkles.glyph(), "Exposure, colour and detail with a histogram and Auto.", ToolPanel.Adjust, actions = listOf(ToolAction.AutoTone)),
    Filters("Filters", ToolGroup.Tone, LucideIcons.SwatchBook.glyph(), "Nine one-tap looks from the native filter set.", ToolPanel.Filters),

    // ---- navigate ----
    Move("Move", ToolGroup.Navigate, LucideIcons.Move.glyph(), "Move the layer on the canvas.", options = listOf(ToolOption.Switch("transform", "Show transform controls", true), ToolOption.Switch("autoSelect", "Auto-select layer", true))),
    Hand("Hand", ToolGroup.Navigate, LucideIcons.Hand.glyph(), "Drag to pan the picture.", ToolPanel.View),
    RotateView("Rotate View", ToolGroup.Navigate, LucideIcons.RotateCw.glyph(), "Turn the picture a quarter at a time.", actions = listOf(ToolAction.RotateRight, ToolAction.RotateLeft)),
    Zoom("Zoom", ToolGroup.Navigate, LucideIcons.ZoomIn.glyph(), "Pinch to zoom, double-tap to fit.", ToolPanel.View),
    Eyedropper("Eyedropper", ToolGroup.Navigate, LucideIcons.Pipette.glyph(), "Tap the picture to sample its colour.", ToolPanel.Sample, options = SampleOptions),
    ColorSampler("Colour Sampler", ToolGroup.Navigate, LucideIcons.Crosshair.glyph(), "Tap to keep samples and compare them.", ToolPanel.Sample, options = SampleOptions),
    Ruler("Ruler", ToolGroup.Navigate, LucideIcons.Ruler.glyph(), "Measure distance and angle on the picture.", options = listOf(ToolOption.Switch("info", "Show info panel", true))),

    // ---- select ----
    Marquee("Rectangular Marquee", ToolGroup.Select, LucideIcons.SquareDashed.glyph(), "Drag a rectangular selection.", options = SelectionOptions),
    EllipseMarquee("Elliptical Marquee", ToolGroup.Select, LucideIcons.CircleDashed.glyph(), "Drag an elliptical selection.", options = SelectionOptions),
    RowMarquee("Single Row", ToolGroup.Select, LucideIcons.Rows2.glyph(), "Select one row of pixels.", options = SelectionOptions),
    ColumnMarquee("Single Column", ToolGroup.Select, LucideIcons.Columns2.glyph(), "Select one column of pixels.", options = SelectionOptions),
    Lasso("Lasso", ToolGroup.Select, LucideIcons.Lasso.glyph(), "Draw a freehand selection.", options = SelectionOptions),
    PolygonalLasso("Polygonal Lasso", ToolGroup.Select, LucideIcons.LassoSelect.glyph(), "Tap to lay down straight selection edges.", options = SelectionOptions),
    MagneticLasso("Magnetic Lasso", ToolGroup.Select, LucideIcons.Magnet.glyph(), "Lasso that snaps to edges.", options = SelectionOptions),
    ObjectSelection("Object Selection", ToolGroup.Select, LucideIcons.Scan.glyph(), "Select the subject or the background.", options = listOf(ToolOption.Choice("target", "Select", listOf("Subject", "Background", "Sky")), ToolOption.Switch("allLayers", "Sample all layers", true))),
    QuickSelection("Quick Selection", ToolGroup.Select, LucideIcons.Wand.glyph(), "Paint over an area to select it.", options = listOf(ToolOption.Slider("size", "Brush size", 1f, 300f, 40f, " px"), ToolOption.Choice("mode", "Mode", listOf("New", "Add", "Subtract")), ToolOption.Switch("allLayers", "Sample all layers", true))),
    MagicWand("Magic Wand", ToolGroup.Select, LucideIcons.WandSparkles.glyph(), "Select similar colours with one tap.", options = MagicSelectOptions),

    // ---- frame ----
    Crop("Crop", ToolGroup.Frame, LucideIcons.Crop.glyph(), "Frame the picture; pick a ratio, drag the corners.", ToolPanel.Crop),
    PerspectiveCrop("Perspective Crop", ToolGroup.Frame, LucideIcons.Frame.glyph(), "Keystone frame that straightens converging lines.", options = listOf(ToolOption.Slider("grid", "Grid density", 4f, 20f, 8f), ToolOption.Switch("showGrid", "Show grid", true))),
    Slice("Slice", ToolGroup.Frame, LucideIcons.Slice.glyph(), "Cut the picture into export slices.", options = listOf(ToolOption.Choice("type", "Slice type", listOf("Divide", "Horizontal", "Vertical")))),
    SliceSelect("Slice Select", ToolGroup.Frame, LucideIcons.MousePointer2.glyph(), "Pick a slice to edit."),
    Artboard("Artboard", ToolGroup.Frame, LucideIcons.PanelsTopLeft.glyph(), "Lay the picture out on a device artboard.", options = listOf(ToolOption.Choice("preset", "Preset", listOf("Phone", "Tablet", "Web", "Print")))),
    FrameTool("Frame Tool", ToolGroup.Frame, LucideIcons.Square.glyph(), "Clip the picture to a frame.", options = ShapeOptions),
    Transform("Transform", ToolGroup.Frame, LucideIcons.Scaling.glyph(), "Rotate and flip the picture.", actions = listOf(ToolAction.RotateLeft, ToolAction.RotateRight, ToolAction.FlipHorizontal, ToolAction.FlipVertical), options = listOf(ToolOption.Choice("interpolation", "Interpolation", listOf("Bicubic", "Bilinear", "Nearest")), ToolOption.Switch("constrain", "Constrain proportions", true))),

    // ---- retouch ----
    SpotHealing("Spot Healing", ToolGroup.Retouch, LucideIcons.Sparkles.glyph(), "One-tap blemish removal.", options = listOf(ToolOption.Slider("size", "Size", 1f, 300f, 30f, " px"), ToolOption.Choice("type", "Type", listOf("Content-aware", "Proximity match", "Create texture")), ToolOption.Switch("allLayers", "Sample all layers", true))),
    HealingBrush("Healing Brush", ToolGroup.Retouch, LucideIcons.Bandage.glyph(), "Paint over a blemish to blend it away.", options = RetouchOptions),
    Patch("Patch", ToolGroup.Retouch, LucideIcons.Puzzle.glyph(), "Drag a patch over a clean area.", options = listOf(ToolOption.Choice("patch", "Patch", listOf("Source", "Destination")), ToolOption.Switch("transparent", "Transparent", false))),
    ContentAwareMove("Content-Aware Move", ToolGroup.Retouch, LucideIcons.Move.glyph(), "Move a selection and fill behind it.", options = listOf(ToolOption.Choice("mode", "Mode", listOf("Move", "Extend")), ToolOption.Slider("structure", "Structure", 1f, 7f, 4f), ToolOption.Slider("colour", "Colour", 0f, 7f, 4f))),
    RedEye("Red Eye", ToolGroup.Retouch, LucideIcons.Eye.glyph(), "Tap the pupil to take the red out.", options = listOf(ToolOption.Slider("pupil", "Pupil size", 1f, 100f, 50f, "%"), ToolOption.Slider("darken", "Darken amount", 1f, 100f, 50f, "%"))),
    BlurTool("Blur", ToolGroup.Retouch, LucideIcons.DrawBlur.glyph(), "Paint to blur.", options = PaintBlebOptions),
    SharpenTool("Sharpen", ToolGroup.Retouch, LucideIcons.Focus.glyph(), "Paint to sharpen.", options = PaintBlebOptions),
    SmudgeTool("Smudge", ToolGroup.Retouch, LucideIcons.DrawSmudge.glyph(), "Paint to smudge.", options = PaintBlebOptions),
    Dodge("Dodge", ToolGroup.Retouch, LucideIcons.Sun.glyph(), "Paint to lighten.", options = ToningOptions),
    Burn("Burn", ToolGroup.Retouch, LucideIcons.Flame.glyph(), "Paint to darken.", options = ToningOptions),
    Sponge("Sponge", ToolGroup.Retouch, LucideIcons.Droplets.glyph(), "Paint to saturate or desaturate.", options = listOf(ToolOption.Slider("size", "Size", 1f, 300f, 60f, " px"), ToolOption.Slider("flow", "Flow", 0f, 100f, 50f, "%"), ToolOption.Choice("mode", "Mode", listOf("Saturate", "Desaturate")), ToolOption.Switch("allLayers", "Sample all layers", true))),

    // ---- paint ----
    Brush("Brush", ToolGroup.Paint, LucideIcons.Paintbrush.glyph(), "Paint with the brush.", options = BrushOptions),
    Pencil("Pencil", ToolGroup.Paint, LucideIcons.Pencil.glyph(), "Hard-edged pencil strokes.", options = listOf(ToolOption.Slider("size", "Size", 1f, 300f, 3f, " px"), ToolOption.Slider("opacity", "Opacity", 0f, 100f, 100f, "%"), ToolOption.Switch("autoErase", "Auto-erase", false))),
    MixerBrush("Mixer Brush", ToolGroup.Paint, LucideIcons.Paintbrush.glyph(), "Paint that mixes with what is under it.", options = listOf(ToolOption.Slider("size", "Size", 1f, 300f, 40f, " px"), ToolOption.Slider("mix", "Mix", 0f, 100f, 50f, "%"), ToolOption.Slider("wet", "Wet", 0f, 100f, 50f, "%"), ToolOption.Slider("load", "Load", 0f, 100f, 50f, "%"))),
    ColorReplacement("Colour Replacement", ToolGroup.Paint, LucideIcons.Replace.glyph(), "Paint to swap one colour for another.", options = listOf(ToolOption.Slider("size", "Size", 1f, 300f, 40f, " px"), ToolOption.Slider("tolerance", "Tolerance", 0f, 100f, 30f), ToolOption.Choice("mode", "Mode", listOf("Colour", "Hue", "Saturation", "Luminosity")))),
    CloneStamp("Clone Stamp", ToolGroup.Paint, LucideIcons.Stamp.glyph(), "Sample with one tap, then paint the copy.", options = listOf(ToolOption.Slider("size", "Size", 1f, 300f, 40f, " px"), ToolOption.Slider("opacity", "Opacity", 0f, 100f, 100f, "%"), ToolOption.Slider("hardness", "Hardness", 0f, 100f, 80f, "%"), ToolOption.Switch("aligned", "Aligned", true))),
    PatternStamp("Pattern Stamp", ToolGroup.Paint, LucideIcons.Grid2x2.glyph(), "Paint with a repeating pattern.", options = listOf(ToolOption.Slider("size", "Size", 1f, 300f, 40f, " px"), ToolOption.Slider("opacity", "Opacity", 0f, 100f, 100f, "%"), ToolOption.Switch("impressionist", "Impressionist", false))),
    HistoryBrush("History Brush", ToolGroup.Paint, LucideIcons.ClockArrowDown.glyph(), "Paint the picture back to an earlier state.", options = listOf(ToolOption.Slider("size", "Size", 1f, 300f, 40f, " px"), ToolOption.Slider("opacity", "Opacity", 0f, 100f, 100f, "%"), ToolOption.Slider("history", "History source", 0f, 20f, 0f))),
    ArtHistoryBrush("Art History Brush", ToolGroup.Paint, LucideIcons.Timer.glyph(), "Paint with stylised earlier states.", options = listOf(ToolOption.Slider("size", "Size", 1f, 300f, 40f, " px"), ToolOption.Slider("style", "Style", 0f, 100f, 50f), ToolOption.Choice("mode", "Brush style", listOf("Tight short", "Tight medium", "Loose long")))),
    Eraser("Eraser", ToolGroup.Paint, LucideIcons.Eraser.glyph(), "Erase to transparency.", options = listOf(ToolOption.Slider("size", "Size", 1f, 300f, 40f, " px"), ToolOption.Slider("opacity", "Opacity", 0f, 100f, 100f, "%"), ToolOption.Slider("hardness", "Hardness", 0f, 100f, 80f, "%"), ToolOption.Choice("mode", "Mode", listOf("Brush", "Pencil", "Block")))),
    BackgroundEraser("Background Eraser", ToolGroup.Paint, LucideIcons.DrawEraser.glyph(), "Erase to transparency, sampling the edge.", options = listOf(ToolOption.Slider("size", "Size", 1f, 300f, 40f, " px"), ToolOption.Slider("tolerance", "Tolerance", 0f, 100f, 20f), ToolOption.Switch("protect", "Protect foreground", true))),
    MagicEraser("Magic Eraser", ToolGroup.Paint, LucideIcons.Eraser.glyph(), "One tap erases similar colour.", options = MagicSelectOptions),

    // ---- fill ----
    Gradient("Gradient", ToolGroup.Fill, LucideIcons.Blend.glyph(), "Drag to fill with a gradient.", options = listOf(ToolOption.Choice("style", "Style", listOf("Linear", "Radial", "Angle", "Reflected", "Diamond")), ToolOption.Slider("opacity", "Opacity", 0f, 100f, 100f, "%"), ToolOption.Switch("dither", "Dither", true), ToolOption.Switch("transparency", "Transparency", true))),
    PaintBucket("Paint Bucket", ToolGroup.Fill, LucideIcons.PaintBucket.glyph(), "Tap to flood-fill similar colour.", options = listOf(ToolOption.Slider("tolerance", "Tolerance", 0f, 100f, 32f), ToolOption.Slider("opacity", "Opacity", 0f, 100f, 100f, "%"), ToolOption.Switch("contiguous", "Contiguous", true), ToolOption.Switch("allLayers", "All layers", true))),

    // ---- vector ----
    Pen("Pen", ToolGroup.Vector, LucideIcons.PenTool.glyph(), "Tap to place anchor points.", options = listOf(ToolOption.Slider("width", "Stroke width", 1f, 40f, 2f, " px"), ToolOption.Switch("rubberBand", "Rubber band", true), ToolOption.Switch("autoAddDelete", "Auto add/delete", true))),
    FreeformPen("Freeform Pen", ToolGroup.Vector, LucideIcons.PenLine.glyph(), "Draw a path freehand.", options = listOf(ToolOption.Slider("width", "Stroke width", 1f, 40f, 2f, " px"), ToolOption.Switch("magnetic", "Magnetic", false))),
    CurvaturePen("Curvature Pen", ToolGroup.Vector, LucideIcons.Spline.glyph(), "Tap points; the path curves itself.", options = listOf(ToolOption.Slider("width", "Stroke width", 1f, 40f, 2f, " px"))),
    AddAnchor("Add Anchor Point", ToolGroup.Vector, LucideIcons.CirclePlus.glyph(), "Tap a path to add an anchor point."),
    DeleteAnchor("Delete Anchor Point", ToolGroup.Vector, LucideIcons.CircleMinus.glyph(), "Tap an anchor point to remove it."),
    ConvertPoint("Convert Point", ToolGroup.Vector, LucideIcons.Dot.glyph(), "Tap an anchor to switch corner and curve."),
    PathSelection("Path Selection", ToolGroup.Vector, LucideIcons.MousePointer2.glyph(), "Select whole paths.", options = listOf(ToolOption.Switch("boundingBox", "Show bounding box", true))),
    DirectSelection("Direct Selection", ToolGroup.Vector, LucideIcons.MousePointer.glyph(), "Select anchor points and handles."),
    Rectangle("Rectangle", ToolGroup.Vector, LucideIcons.Square.glyph(), "Draw a rectangle.", options = ShapeOptions),
    Ellipse("Ellipse", ToolGroup.Vector, LucideIcons.Circle.glyph(), "Draw an ellipse.", options = ShapeOptions),
    TriangleShape("Triangle", ToolGroup.Vector, LucideIcons.Triangle.glyph(), "Draw a triangle.", options = ShapeOptions + ToolOption.Slider("sides", "Sides", 3f, 12f, 3f)),
    Polygon("Polygon", ToolGroup.Vector, LucideIcons.Hexagon.glyph(), "Draw a polygon.", options = ShapeOptions + ToolOption.Slider("sides", "Sides", 3f, 12f, 6f)),
    StarShape("Star", ToolGroup.Vector, LucideIcons.Star.glyph(), "Draw a star.", options = ShapeOptions + ToolOption.Slider("points", "Points", 3f, 20f, 5f) + ToolOption.Slider("inner", "Inner radius", 10f, 90f, 50f, "%")),
    Line("Line", ToolGroup.Vector, LucideIcons.Minus.glyph(), "Draw a straight line.", options = listOf(ToolOption.Slider("weight", "Weight", 1f, 40f, 2f, " px"), ToolOption.Choice("arrow", "Arrowheads", listOf("None", "Start", "End", "Both")))),
    CustomShape("Custom Shape", ToolGroup.Vector, LucideIcons.Shapes.glyph(), "Draw a preset shape.", options = listOf(ToolOption.Choice("shape", "Shape", listOf("Heart", "Arrow", "Check", "Burst")), ToolOption.Slider("stroke", "Stroke", 0f, 40f, 0f, " px"))),

    // ---- type ----
    HorizontalType("Horizontal Type", ToolGroup.Type, LucideIcons.Type.glyph(), "Tap to type across the picture.", options = TypeOptions),
    VerticalType("Vertical Type", ToolGroup.Type, LucideIcons.Type.glyph(), "Tap to type down the picture.", options = TypeOptions),
    TypeMask("Type Mask", ToolGroup.Type, LucideIcons.TypeOutline.glyph(), "Type that becomes a selection.", options = TypeOptions),
    VerticalTypeMask("Vertical Type Mask", ToolGroup.Type, LucideIcons.TypeOutline.glyph(), "Vertical type that becomes a selection.", options = TypeOptions),
}
