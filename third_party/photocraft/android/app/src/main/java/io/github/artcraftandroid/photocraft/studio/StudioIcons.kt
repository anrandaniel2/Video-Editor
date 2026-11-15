package io.github.artcraftandroid.photocraft.studio

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.graphics.vector.toPath
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/**
 * The studio's icons: the reference UI's single-path 24×24 set (1.4 stroke, round caps and joins),
 * one `ImageVector` per name. The artwork is the reference's own geometry, so the toolbar reads the
 * same as the design it was ported from.
 */
object StudioIcons {
    private val paths: Map<String, String> = mapOf(
        "adjustments" to "M4 5h16M4 12h16M4 19h16M8 2v6m8 1v6m-7 1v6",
        "ai" to "m12 2 2.5 7.5L22 12l-7.5 2.5L12 22l-2.5-7.5L2 12l7.5-2.5L12 2Zm8 0v5m-2.5-2.5h5",
        "align" to "M4 5h16M4 10h11M4 15h16M4 20h11",
        "assets" to "M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z",
        "bold" to "M6 3h7a5 5 0 0 1 0 9H6h8a5 5 0 0 1 0 9H6V3Z",
        "brush" to "m11 13 7-10 3 3-9 9m-1-2c-8-4-3 7-9 7 7 3 14-1 10-6Z",
        "center" to "M4 5h16M7 10h10M4 15h16M7 20h10",
        "check" to "m4 12 5 5L20 6",
        "chevron" to "m9 5 7 7-7 7",
        "circle" to "M20 12a8 8 0 1 1-16 0 8 8 0 0 1 16 0Z",
        "clear" to "M3 3h18v18H3zM7 7l10 10M17 7 7 17",
        "clone" to "M9 3h6v6l3 5H6l3-5V3ZM5 15h14v6H5z",
        "close" to "m6 6 12 12M18 6 6 18",
        "contrast" to "M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0ZM12 3v18",
        "crop" to "M6 2v16h16M2 6h16v16M3 21 21 3",
        "curve" to "M4 19V5h14M7 18c0-8 3-11 11-11m-8 12c0-5 3-9 9-9",
        "down" to "m6 9 6 6 6-6",
        "dropper" to "m15 3 6 6-3 3-2-2L6 20l-4 1 1-4L13 7l-1-1 3-3Zm-2 4 3 3",
        "duplicate" to "M8 3h13v13H8zM3 8v13h13",
        "effects" to "M4 20V4h8M4 11h6m3-3 7 12m0-12-7 12",
        "eraser" to "m4 12 10-10 8 8-10 10H7l-5-5 2-3Zm2-2 8 8m-2 2h10",
        "export" to "M14 3h7v7m0-7L10 14m0-9H4v16h15v-6",
        "eye" to "M2 12s4-6 10-6 10 6 10 6-4 6-10 6S2 12 2 12Zm13 0a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z",
        "fill" to "m7 3 11 11-8 7-9-9L9 4m-5 7h11m5 3s5 6 0 6-1-6 0-6",
        "frame" to "M3 4h18v16H3zM3 4l18 16M21 4 3 20",
        "grid" to "M3 3h7v7H3zM14 3h7v7h-7zM3 14h7v7H3zM14 14h7v7h-7z",
        "hand" to "M7 11V5a1.5 1.5 0 0 1 3 0v5-7a1.5 1.5 0 0 1 3 0v7-5a1.5 1.5 0 0 1 3 0v6-3a1.5 1.5 0 0 1 3 0v7c0 9-10 10-13 3l-3-5c-2-3 1-4 4-1Z",
        "heal" to "m4 12 8-8a5 5 0 0 1 7 7l-8 8a5 5 0 0 1-7-7Zm3-3 8 8M11 9h.01M9 11h.01m4 0h.01M11 13h.01",
        "help" to "M9 8a3 3 0 1 1 5 2c-2 1-2 2-2 4m0 3h.01M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0Z",
        "hidden" to "m3 3 18 18M9 6c8-2 13 6 13 6l-4 4M6 7l-4 5s6 9 14 5",
        "history" to "M4 6a9 9 0 1 1-1 9M4 2v5h5m3 0v6l4 2",
        "image" to "M3 5h18v14H3zM4 16l5-5 4 4 3-3 5 5M16 9h.01",
        "italic" to "M11 3h8M5 21h8M15 3 9 21",
        "layers" to "m12 3 10 6-10 6L2 9l10-6Zm-9 10 9 5 9-5M3 17l9 5 9-5",
        "layout" to "M3 4h5v16H3zM12 4h9v5h-9zM12 12h9v8h-9z",
        "list" to "M8 5h13M8 12h13M8 19h13M3 5h.01M3 12h.01M3 19h.01",
        "lock" to "M5 10h14v11H5zM8 10V6a4 4 0 0 1 8 0v4m-4 4v3",
        "marquee" to "M4 4h2m3 0h2m3 0h2m3 0h1v2m0 3v2m0 3v2m0 3v1h-2m-3 0h-2m-3 0H8m-3 0H4v-2m0-3v-2m0-3V8",
        "mask" to "M3 4h18v16H3zM12 4v16M6 8l3 3-3 3",
        "menu" to "M4 6h16M4 12h16M4 18h16",
        "mesh" to "M3 3h18v18H3zM3 9h18M3 15h18M9 3v18M15 3v18",
        "minus" to "M4 12h16",
        "more" to "M5 12h.01M12 12h.01M19 12h.01",
        "move" to "m5 3 13 9-7 1-3 7-3-17Z",
        "navigator" to "m12 2 3 7 7 3-7 3-3 7-3-7-7-3 7-3 3-7Zm0 4v12M6 12h12",
        "node" to "m6 4 13 8-11 7-2-15ZM6 4l5 9 8-1",
        "pages" to "M7 3h12v15H7zM4 6v15h12M10 7h6m-6 4h6",
        "pen" to "m4 20 3-11 9-6 5 5-6 9-11 3Zm0 0 8-8m1-2a2 2 0 1 0 0 .1M7 9l8 8",
        "pencil" to "m4 16 12-12 4 4L8 20l-5 1 1-5Zm10-10 4 4",
        "pin" to "m8 3 9 0-2 6 4 4H5l4-4-1-6Zm4 10v9",
        "pixel" to "M18 7a7 7 0 1 1-7-3m0-2v4m5-3-1 4m5 0-3 2",
        "plus" to "M12 4v16M4 12h16",
        "rectangle" to "M4 5h16v14H4z",
        "redo" to "m15 5 6 5-6 4m6-4H11c-9 0-9 10-1 10",
        "reset" to "M4 6a9 9 0 1 1-1 9M4 2v5h5",
        "right" to "M4 5h16M9 10h11M4 15h16M9 20h11",
        "search" to "M17 10a7 7 0 1 1-14 0 7 7 0 0 1 14 0Zm-2 5 6 6",
        "select" to "M4 8V4h4m8 0h4v4m0 8v4h-4m-8 0H4v-4M4 11v2m16-2v2M11 4h2m-2 16h2",
        "settings" to "m9 2-1 3-3 1-2 3 2 3-1 3 3 3 3-1 3 2 3-2 3 1 3-3-1-3 2-3-2-3-3-1-1-3H9Zm7 10a4 4 0 1 1-8 0 4 4 0 0 1 8 0Z",
        "sketch" to "m8 3-2 9 5 8 5-8-1-9m-4 1v9m-4-1 4 2 5-2",
        "snap" to "M6 4v7a6 6 0 0 0 12 0V4h-4v7a2 2 0 0 1-4 0V4H6Zm0 4h4m4 0h4",
        "stroke" to "M3 6c5-10 12 10 18 0M3 13c5-10 12 10 18 0M3 20c5-10 12 10 18 0",
        "table" to "M3 4h18v16H3zM3 10h18M9 4v16M3 15h18",
        "text" to "M5 4h14M12 4v16m-4 0h8M3 8V3h18v5",
        "transform" to "M3 3h5v5H3zM16 3h5v5h-5zM3 16h5v5H3zM16 16h5v5h-5zM8 5h8M5 8v8m14-8v8M8 19h8M9 9h6v6H9z",
        "trash" to "M4 6h16M9 6V3h6v3M6 6l1 15h10l1-15M10 10v7m4-7v7",
        "underline" to "M6 3v8a6 6 0 0 0 12 0V3M4 21h16",
        "undo" to "M9 5 3 10l6 4M3 10h10c9 0 9 10 1 10",
        "unlock" to "M5 10h14v11H5zM8 10V6a4 4 0 0 1 8 0m-4 8v3",
        "vector" to "M5 5h3v3H5zM16 5h3v3h-3zM10 17h3v3h-3zM7 8l4 9m6-9-5 9",
        "wand" to "m4 20 12-12 4 4L8 24M14 10l4 4M5 2v5M2 4.5h6M17 1v4m-2-2h4M4 12H1",
        "zoom" to "m15 15 6 6M17 10a7 7 0 1 1-14 0 7 7 0 0 1 14 0Zm-11 0h8m-4-4v8",

    )

    private val cache = HashMap<String, ImageVector>()

    /** The vector for [name]; an unknown name draws the reference's placeholder (its rectangle). */
    fun vector(name: String): ImageVector = cache.getOrPut(name) {
        val path = paths[name] ?: paths.getValue("rectangle")
        val parsed = PathParser().parsePathString(path).toNodes().toPath()
        val builder = ImageVector.Builder(
            name = "studio-$name",
            defaultWidth = 24.dp,
            defaultHeight = 24.dp,
            viewportWidth = 24f,
            viewportHeight = 24f,
        )
        builder.addPath(
            pathData = parsed,
            stroke = SolidColor(Color.Black),
            strokeLineWidth = 1.4f,
            strokeLineCap = StrokeCap.Round,
            strokeLineJoin = StrokeJoin.Round,
        )
        builder.build()
    }

    /** The icon, tinted [tint], at [size]. */
    @Composable
    fun Glyph(name: String, tint: Color, size: Dp = 20.dp, modifier: Modifier = Modifier) {
        androidx.compose.material3.Icon(
            imageVector = vector(name),
            contentDescription = null,
            tint = tint,
            modifier = modifier.size(size),
        )
    }

    /** The icon as Compose draws it anywhere else (a Canvas' own drawing, a preview). */
    fun path(name: String): String = paths[name] ?: paths.getValue("rectangle")
}

/** A dash of brush along the preset row, as the Brushes panel draws it. */
@Composable
fun BrushStrokePreview(modifier: Modifier = Modifier, colour: Color = Color(0xFFEEEEEE), width: Float = 2f) {
    Canvas(modifier) {
        val h = size.height
        val w = size.width
        val stroke = width.coerceAtLeast(1f)
        val path = androidx.compose.ui.graphics.Path().apply {
            moveTo(3f, h * 0.72f)
            cubicTo(w * 0.26f, h * 0.16f, w * 0.5f, h * 0.47f, w * 0.99f, h * 0.28f)
        }
        drawPath(
            path,
            color = colour,
            style = androidx.compose.ui.graphics.drawscope.Stroke(
                width = stroke,
                cap = StrokeCap.Round,
                join = StrokeJoin.Round,
            ),
        )
        // The reference's faint second line under the top strokes.
        val ghost = androidx.compose.ui.graphics.Path().apply {
            moveTo(6f, h * 0.81f)
            cubicTo(w * 0.26f, h * 0.22f, w * 0.52f, h * 0.59f, w * 0.97f, h * 0.37f)
        }
        drawPath(ghost, color = colour.copy(alpha = 0.35f), style = androidx.compose.ui.graphics.drawscope.Stroke(width = 0.6f))
    }
}
