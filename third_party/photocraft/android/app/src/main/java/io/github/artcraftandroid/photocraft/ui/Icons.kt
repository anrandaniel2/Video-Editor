package io.github.artcraftandroid.photocraft.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/** The stroke weight PhotoCraft draws Lucide's 24-unit icons with: lighter than Lucide's 2. */
const val ICON_STROKE = 1.6f

/** A Lucide icon from its SVG path strings (see `LucideIconsTask` in app/build.gradle.kts). */
internal fun lucide(name: String, vararg paths: String): ImageVector {
    val b = ImageVector.Builder(name = name, defaultWidth = 24.dp, defaultHeight = 24.dp, viewportWidth = 24f, viewportHeight = 24f)
    for (d in paths) {
        b.addPath(
            pathData = PathParser().parsePathString(d).toNodes(),
            fill = null,
            stroke = SolidColor(Color.Black),
            strokeLineWidth = ICON_STROKE,
            strokeLineCap = StrokeCap.Round,
            strokeLineJoin = StrokeJoin.Round,
        )
    }
    return b.build()
}

/** An icon: a Lucide vector or one of PhotoCraft's own drawn glyphs. */
sealed interface Glyph {
    data class Vector(val image: ImageVector) : Glyph
    data class Drawn(val draw: DrawScope.(Color) -> Unit) : Glyph
}

fun ImageVector.glyph(): Glyph = Glyph.Vector(this)

@Composable
fun GlyphIcon(glyph: Glyph, tint: Color, modifier: Modifier = Modifier, size: Dp = 24.dp, description: String? = null) {
    when (glyph) {
        is Glyph.Vector -> Icon(glyph.image, contentDescription = description, tint = tint, modifier = modifier.size(size))
        is Glyph.Drawn -> Canvas(modifier.size(size)) { glyph.draw(this, tint) }
    }
}

/** PhotoCraft's own glyphs, drawn on a 24-unit grid: shapes Lucide has no stroke-only form of. */
object Glyphs {
    private fun DrawScope.u(v: Float) = v * size.minDimension / 24f

    /** A filled play triangle. */
    val Play = Glyph.Drawn { c ->
        val p = Path().apply {
            moveTo(u(7f), u(4.5f)); lineTo(u(19.5f), u(12f)); lineTo(u(7f), u(19.5f)); close()
        }
        drawPath(p, c)
    }

    /** Two filled bars. */
    val Pause = Glyph.Drawn { c ->
        drawRoundRect(c, androidx.compose.ui.geometry.Offset(u(6f), u(4.5f)), androidx.compose.ui.geometry.Size(u(4f), u(15f)), androidx.compose.ui.geometry.CornerRadius(u(1f)))
        drawRoundRect(c, androidx.compose.ui.geometry.Offset(u(14f), u(4.5f)), androidx.compose.ui.geometry.Size(u(4f), u(15f)), androidx.compose.ui.geometry.CornerRadius(u(1f)))
    }

    /** A checkerboard square (opacity). */
    val Checker = Glyph.Drawn { c ->
        val s = u(16f)
        val x = (size.width - s) / 2f
        val y = (size.height - s) / 2f
        val cell = s / 4f
        for (i in 0 until 4) for (j in 0 until 4) {
            if ((i + j) % 2 == 0) {
                drawRect(c, androidx.compose.ui.geometry.Offset(x + i * cell, y + j * cell), androidx.compose.ui.geometry.Size(cell, cell))
            }
        }
        drawRect(c, androidx.compose.ui.geometry.Offset(x, y), androidx.compose.ui.geometry.Size(s, s), style = androidx.compose.ui.graphics.drawscope.Stroke(width = u(1.6f)))
    }

    /** An eyedropper: a pipette at 45 degrees. */
    val Eyedropper = Glyph.Drawn { c ->
        val w = androidx.compose.ui.graphics.drawscope.Stroke(width = u(1.6f), cap = StrokeCap.Round, join = StrokeJoin.Round)
        val p = Path().apply {
            moveTo(u(14.5f), u(4f)); lineTo(u(20f), u(9.5f)); lineTo(u(16f), u(13.5f)); lineTo(u(10.5f), u(8f)); close()
        }
        drawPath(p, c, style = w)
        drawLine(c, androidx.compose.ui.geometry.Offset(u(10.5f), u(8f)), androidx.compose.ui.geometry.Offset(u(4f), u(14.5f)), u(1.6f), StrokeCap.Round)
        drawLine(c, androidx.compose.ui.geometry.Offset(u(4f), u(14.5f)), androidx.compose.ui.geometry.Offset(u(6.5f), u(17f)), u(1.6f), StrokeCap.Round)
        drawLine(c, androidx.compose.ui.geometry.Offset(u(6.5f), u(17f)), androidx.compose.ui.geometry.Offset(u(9f), u(14.5f)), u(1.6f), StrokeCap.Round)
    }

    /** A brush tip: a rounded rectangle with a pointed end. */
    val BrushTip = Glyph.Drawn { c ->
        val w = androidx.compose.ui.graphics.drawscope.Stroke(width = u(1.6f), cap = StrokeCap.Round, join = StrokeJoin.Round)
        val p = Path().apply {
            moveTo(u(5f), u(19f)); lineTo(u(15f), u(9f)); lineTo(u(19f), u(5f)); lineTo(u(15f), u(5f)); lineTo(u(5f), u(15f)); close()
        }
        drawPath(p, c, style = w)
    }

    /** A hand (pan). */
    val Hand = Glyph.Drawn { c ->
        val w = androidx.compose.ui.graphics.drawscope.Stroke(width = u(1.6f), cap = StrokeCap.Round, join = StrokeJoin.Round)
        val p = Path().apply {
            moveTo(u(7f), u(11f)); lineTo(u(7f), u(6f)); lineTo(u(9.5f), u(6f)); lineTo(u(9.5f), u(10f))
            lineTo(u(11f), u(5f)); lineTo(u(13.5f), u(5f)); lineTo(u(13.5f), u(10f))
            lineTo(u(15f), u(6f)); lineTo(u(17.5f), u(6f)); lineTo(u(17.5f), u(12f))
            lineTo(u(17.5f), u(16f)); lineTo(u(14f), u(20f)); lineTo(u(9f), u(20f)); lineTo(u(7f), u(16f)); close()
        }
        drawPath(p, c, style = w)
    }

    /** A magnifying glass (zoom). */
    val Zoom = Glyph.Drawn { c ->
        drawCircle(c, radius = u(7f), center = androidx.compose.ui.geometry.Offset(u(10.5f), u(10.5f)), style = androidx.compose.ui.graphics.drawscope.Stroke(width = u(1.6f)))
        drawLine(c, androidx.compose.ui.geometry.Offset(u(16f), u(16f)), androidx.compose.ui.geometry.Offset(u(20f), u(20f)), u(1.6f), StrokeCap.Round)
    }

    /** A crop frame: two L corners. */
    val Crop = Glyph.Drawn { c ->
        val w = u(1.6f)
        drawLine(c, androidx.compose.ui.geometry.Offset(u(6f), u(18f)), androidx.compose.ui.geometry.Offset(u(6f), u(6f)), w, StrokeCap.Round)
        drawLine(c, androidx.compose.ui.geometry.Offset(u(6f), u(6f)), androidx.compose.ui.geometry.Offset(u(18f), u(6f)), w, StrokeCap.Round)
        drawLine(c, androidx.compose.ui.geometry.Offset(u(18f), u(6f)), androidx.compose.ui.geometry.Offset(u(18f), u(18f)), w, StrokeCap.Round)
        drawLine(c, androidx.compose.ui.geometry.Offset(u(18f), u(18f)), androidx.compose.ui.geometry.Offset(u(6f), u(18f)), w, StrokeCap.Round)
    }

    /** A rectangle (selection). */
    val Rect = Glyph.Drawn { c ->
        drawRect(c, androidx.compose.ui.geometry.Offset(u(5f), u(5f)), androidx.compose.ui.geometry.Size(u(14f), u(14f)), style = androidx.compose.ui.graphics.drawscope.Stroke(width = u(1.6f)))
    }

    /** A lasso: a loop with a tail. */
    val Lasso = Glyph.Drawn { c ->
        val w = androidx.compose.ui.graphics.drawscope.Stroke(width = u(1.6f), cap = StrokeCap.Round, join = StrokeJoin.Round)
        val p = Path().apply {
            moveTo(u(12f), u(4f))
            cubicTo(u(17f), u(4f), u(19f), u(7f), u(19f), u(10f))
            cubicTo(u(19f), u(13f), u(17f), u(15f), u(14f), u(15f))
            cubicTo(u(11f), u(15f), u(9f), u(13f), u(9f), u(10f))
            cubicTo(u(9f), u(7f), u(10f), u(4f), u(12f), u(4f))
            moveTo(u(12f), u(15f)); lineTo(u(7f), u(20f))
        }
        drawPath(p, c, style = w)
    }

    /** A magic wand: a stick with sparkles. */
    val Wand = Glyph.Drawn { c ->
        val w = u(1.6f)
        drawLine(c, androidx.compose.ui.geometry.Offset(u(6f), u(18f)), androidx.compose.ui.geometry.Offset(u(15f), u(9f)), w, StrokeCap.Round)
        drawLine(c, androidx.compose.ui.geometry.Offset(u(18f), u(4f)), androidx.compose.ui.geometry.Offset(u(18f), u(8f)), w, StrokeCap.Round)
        drawLine(c, androidx.compose.ui.geometry.Offset(u(16f), u(6f)), androidx.compose.ui.geometry.Offset(u(20f), u(6f)), w, StrokeCap.Round)
        drawLine(c, androidx.compose.ui.geometry.Offset(u(6f), u(4f)), androidx.compose.ui.geometry.Offset(u(6f), u(7f)), w, StrokeCap.Round)
        drawLine(c, androidx.compose.ui.geometry.Offset(u(4.5f), u(5.5f)), androidx.compose.ui.geometry.Offset(u(7.5f), u(5.5f)), w, StrokeCap.Round)
    }

    /** A move arrow: four directions. */
    val Move = Glyph.Drawn { c ->
        val w = u(1.6f)
        drawLine(c, androidx.compose.ui.geometry.Offset(u(12f), u(4f)), androidx.compose.ui.geometry.Offset(u(12f), u(20f)), w, StrokeCap.Round)
        drawLine(c, androidx.compose.ui.geometry.Offset(u(4f), u(12f)), androidx.compose.ui.geometry.Offset(u(20f), u(12f)), w, StrokeCap.Round)
        // arrowheads
        drawLine(c, androidx.compose.ui.geometry.Offset(u(12f), u(4f)), androidx.compose.ui.geometry.Offset(u(9.5f), u(6.5f)), w, StrokeCap.Round)
        drawLine(c, androidx.compose.ui.geometry.Offset(u(12f), u(4f)), androidx.compose.ui.geometry.Offset(u(14.5f), u(6.5f)), w, StrokeCap.Round)
        drawLine(c, androidx.compose.ui.geometry.Offset(u(12f), u(20f)), androidx.compose.ui.geometry.Offset(u(9.5f), u(17.5f)), w, StrokeCap.Round)
        drawLine(c, androidx.compose.ui.geometry.Offset(u(12f), u(20f)), androidx.compose.ui.geometry.Offset(u(14.5f), u(17.5f)), w, StrokeCap.Round)
        drawLine(c, androidx.compose.ui.geometry.Offset(u(4f), u(12f)), androidx.compose.ui.geometry.Offset(u(6.5f), u(9.5f)), w, StrokeCap.Round)
        drawLine(c, androidx.compose.ui.geometry.Offset(u(4f), u(12f)), androidx.compose.ui.geometry.Offset(u(6.5f), u(14.5f)), w, StrokeCap.Round)
        drawLine(c, androidx.compose.ui.geometry.Offset(u(20f), u(12f)), androidx.compose.ui.geometry.Offset(u(17.5f), u(9.5f)), w, StrokeCap.Round)
        drawLine(c, androidx.compose.ui.geometry.Offset(u(20f), u(12f)), androidx.compose.ui.geometry.Offset(u(17.5f), u(14.5f)), w, StrokeCap.Round)
    }

    /** A gradient: a rectangle with a left-to-right fade. */
    val Gradient = Glyph.Drawn { c ->
        val s = u(16f)
        val x = (size.width - s) / 2f
        val y = (size.height - s) / 2f
        for (i in 0 until 16) {
            val a = 1f - i / 15f
            drawRect(
                Color(c.red, c.green, c.blue, a),
                androidx.compose.ui.geometry.Offset(x + i * s / 16f, y),
                androidx.compose.ui.geometry.Size(s / 16f + 1f, s),
            )
        }
        drawRect(c, androidx.compose.ui.geometry.Offset(x, y), androidx.compose.ui.geometry.Size(s, s), style = androidx.compose.ui.graphics.drawscope.Stroke(width = u(1.6f)))
    }

    /** A clone stamp: a stamp with a crosshair. */
    val Clone = Glyph.Drawn { c ->
        val w = u(1.6f)
        drawRect(c, androidx.compose.ui.geometry.Offset(u(6f), u(4f)), androidx.compose.ui.geometry.Size(u(12f), u(10f)), style = androidx.compose.ui.graphics.drawscope.Stroke(width = w))
        drawLine(c, androidx.compose.ui.geometry.Offset(u(12f), u(14f)), androidx.compose.ui.geometry.Offset(u(12f), u(20f)), w, StrokeCap.Round)
        drawLine(c, androidx.compose.ui.geometry.Offset(u(8f), u(20f)), androidx.compose.ui.geometry.Offset(u(16f), u(20f)), w, StrokeCap.Round)
    }

    /** A healing brush: a bandage. */
    val Heal = Glyph.Drawn { c ->
        val w = androidx.compose.ui.graphics.drawscope.Stroke(width = u(1.6f), cap = StrokeCap.Round, join = StrokeJoin.Round)
        val p = Path().apply {
            moveTo(u(5f), u(9f)); lineTo(u(9f), u(5f)); lineTo(u(15f), u(5f)); lineTo(u(19f), u(9f)); lineTo(u(19f), u(15f)); lineTo(u(15f), u(19f)); lineTo(u(9f), u(19f)); lineTo(u(5f), u(15f)); close()
        }
        drawPath(p, c, style = w)
        drawLine(c, androidx.compose.ui.geometry.Offset(u(10f), u(12f)), androidx.compose.ui.geometry.Offset(u(14f), u(12f)), w.width, StrokeCap.Round)
    }

    /** An eye (visibility). */
    val Eye = Glyph.Drawn { c ->
        val w = androidx.compose.ui.graphics.drawscope.Stroke(width = u(1.6f), cap = StrokeCap.Round, join = StrokeJoin.Round)
        val p = Path().apply {
            moveTo(u(3f), u(12f))
            cubicTo(u(6f), u(7f), u(9f), u(5f), u(12f), u(5f))
            cubicTo(u(15f), u(5f), u(18f), u(7f), u(21f), u(12f))
            cubicTo(u(18f), u(17f), u(15f), u(19f), u(12f), u(19f))
            cubicTo(u(9f), u(19f), u(6f), u(17f), u(3f), u(12f))
        }
        drawPath(p, c, style = w)
        drawCircle(c, radius = u(3f), center = androidx.compose.ui.geometry.Offset(u(12f), u(12f)), style = w)
    }

    /** An eye with a slash (hidden). */
    val EyeOff = Glyph.Drawn { c ->
        val w = u(1.6f)
        drawLine(c, androidx.compose.ui.geometry.Offset(u(4f), u(4f)), androidx.compose.ui.geometry.Offset(u(20f), u(20f)), w, StrokeCap.Round)
        val p = Path().apply {
            moveTo(u(3f), u(12f))
            cubicTo(u(6f), u(7f), u(9f), u(5f), u(12f), u(5f))
            cubicTo(u(15f), u(5f), u(18f), u(7f), u(21f), u(12f))
            cubicTo(u(18f), u(17f), u(15f), u(19f), u(12f), u(19f))
            cubicTo(u(9f), u(19f), u(6f), u(17f), u(3f), u(12f))
        }
        drawPath(p, c, style = androidx.compose.ui.graphics.drawscope.Stroke(width = w, cap = StrokeCap.Round, join = StrokeJoin.Round))
    }

    /** A lock (locked layer). */
    val Lock = Glyph.Drawn { c ->
        val w = androidx.compose.ui.graphics.drawscope.Stroke(width = u(1.6f), cap = StrokeCap.Round, join = StrokeJoin.Round)
        drawRect(c, androidx.compose.ui.geometry.Offset(u(6f), u(11f)), androidx.compose.ui.geometry.Size(u(12f), u(9f)), style = w)
        val p = Path().apply {
            moveTo(u(8f), u(11f)); lineTo(u(8f), u(8f)); cubicTo(u(8f), u(5f), u(10f), u(4f), u(12f), u(4f)); cubicTo(u(14f), u(4f), u(16f), u(5f), u(16f), u(8f)); lineTo(u(16f), u(11f))
        }
        drawPath(p, c, style = w)
    }

    /** A curve (curves adjustment): a diagonal with a bend. */
    val Curve = Glyph.Drawn { c ->
        val w = androidx.compose.ui.graphics.drawscope.Stroke(width = u(1.6f), cap = StrokeCap.Round, join = StrokeJoin.Round)
        val p = Path().apply {
            moveTo(u(4f), u(20f))
            cubicTo(u(8f), u(20f), u(10f), u(16f), u(12f), u(12f))
            cubicTo(u(14f), u(8f), u(16f), u(4f), u(20f), u(4f))
        }
        drawPath(p, c, style = w)
        drawLine(c, androidx.compose.ui.geometry.Offset(u(4f), u(4f)), androidx.compose.ui.geometry.Offset(u(4f), u(20f)), u(1f), StrokeCap.Round)
        drawLine(c, androidx.compose.ui.geometry.Offset(u(4f), u(20f)), androidx.compose.ui.geometry.Offset(u(20f), u(20f)), u(1f), StrokeCap.Round)
    }

    /** A histogram (levels): three bars. */
    val Histogram = Glyph.Drawn { c ->
        drawRect(c, androidx.compose.ui.geometry.Offset(u(5f), u(14f)), androidx.compose.ui.geometry.Size(u(3f), u(6f)))
        drawRect(c, androidx.compose.ui.geometry.Offset(u(10.5f), u(8f)), androidx.compose.ui.geometry.Size(u(3f), u(12f)))
        drawRect(c, androidx.compose.ui.geometry.Offset(u(16f), u(4f)), androidx.compose.ui.geometry.Size(u(3f), u(16f)))
    }

    /** A sun (brightness). */
    val Sun = Glyph.Drawn { c ->
        drawCircle(c, radius = u(4f), center = androidx.compose.ui.geometry.Offset(u(12f), u(12f)), style = androidx.compose.ui.graphics.drawscope.Stroke(width = u(1.6f)))
        for (i in 0 until 8) {
            val a = i * Math.PI / 4.0
            val x1 = 12f + 6.5f * kotlin.math.cos(a).toFloat()
            val y1 = 12f + 6.5f * kotlin.math.sin(a).toFloat()
            val x2 = 12f + 9f * kotlin.math.cos(a).toFloat()
            val y2 = 12f + 9f * kotlin.math.sin(a).toFloat()
            drawLine(c, androidx.compose.ui.geometry.Offset(u(x1), u(y1)), androidx.compose.ui.geometry.Offset(u(x2), u(y2)), u(1.6f), StrokeCap.Round)
        }
    }

    /** A droplet (liquify). */
    val Droplet = Glyph.Drawn { c ->
        val w = androidx.compose.ui.graphics.drawscope.Stroke(width = u(1.6f), cap = StrokeCap.Round, join = StrokeJoin.Round)
        val p = Path().apply {
            moveTo(u(12f), u(4f))
            cubicTo(u(12f), u(4f), u(6f), u(11f), u(6f), u(15f))
            cubicTo(u(6f), u(18f), u(8.5f), u(20f), u(12f), u(20f))
            cubicTo(u(15.5f), u(20f), u(18f), u(18f), u(18f), u(15f))
            cubicTo(u(18f), u(11f), u(12f), u(4f), u(12f), u(4f))
        }
        drawPath(p, c, style = w)
    }

    /** A frame (picture frame). */
    val Frame = Glyph.Drawn { c ->
        drawRect(c, androidx.compose.ui.geometry.Offset(u(4f), u(4f)), androidx.compose.ui.geometry.Size(u(16f), u(16f)), style = androidx.compose.ui.graphics.drawscope.Stroke(width = u(1.6f)))
        drawRect(c, androidx.compose.ui.geometry.Offset(u(8f), u(8f)), androidx.compose.ui.geometry.Size(u(8f), u(8f)), style = androidx.compose.ui.graphics.drawscope.Stroke(width = u(1.6f)))
    }

    /** A tree (render tree). */
    val Tree = Glyph.Drawn { c ->
        val w = androidx.compose.ui.graphics.drawscope.Stroke(width = u(1.6f), cap = StrokeCap.Round, join = StrokeJoin.Round)
        val p = Path().apply {
            moveTo(u(12f), u(4f)); lineTo(u(7f), u(11f)); lineTo(u(10f), u(11f)); lineTo(u(6f), u(16f)); lineTo(u(18f), u(16f)); lineTo(u(14f), u(11f)); lineTo(u(17f), u(11f)); close()
        }
        drawPath(p, c, style = w)
        drawLine(c, androidx.compose.ui.geometry.Offset(u(12f), u(16f)), androidx.compose.ui.geometry.Offset(u(12f), u(20f)), u(1.6f), StrokeCap.Round)
    }

    /** A flame (render flame). */
    val Flame = Glyph.Drawn { c ->
        val w = androidx.compose.ui.graphics.drawscope.Stroke(width = u(1.6f), cap = StrokeCap.Round, join = StrokeJoin.Round)
        val p = Path().apply {
            moveTo(u(12f), u(4f))
            cubicTo(u(14f), u(8f), u(17f), u(10f), u(17f), u(14f))
            cubicTo(u(17f), u(17f), u(15f), u(20f), u(12f), u(20f))
            cubicTo(u(9f), u(20f), u(7f), u(17f), u(7f), u(14f))
            cubicTo(u(7f), u(11f), u(9f), u(9f), u(10f), u(7f))
            cubicTo(u(10f), u(9f), u(11f), u(11f), u(12f), u(12f))
            cubicTo(u(13f), u(12f), u(14f), u(10f), u(14f), u(8f))
            cubicTo(u(14f), u(6f), u(13f), u(5f), u(12f), u(4f))
        }
        drawPath(p, c, style = w)
    }

    /** A lens flare: a circle with rays. */
    val LensFlare = Glyph.Drawn { c ->
        drawCircle(c, radius = u(5f), center = androidx.compose.ui.geometry.Offset(u(12f), u(12f)), style = androidx.compose.ui.graphics.drawscope.Stroke(width = u(1.6f)))
        for (i in 0 until 4) {
            val a = i * Math.PI / 2.0 + Math.PI / 4.0
            val x1 = 12f + 7f * kotlin.math.cos(a).toFloat()
            val y1 = 12f + 7f * kotlin.math.sin(a).toFloat()
            val x2 = 12f + 10f * kotlin.math.cos(a).toFloat()
            val y2 = 12f + 10f * kotlin.math.sin(a).toFloat()
            drawLine(c, androidx.compose.ui.geometry.Offset(u(x1), u(y1)), androidx.compose.ui.geometry.Offset(u(x2), u(y2)), u(1.6f), StrokeCap.Round)
        }
    }

    /** Clouds (render clouds). */
    val Clouds = Glyph.Drawn { c ->
        val w = androidx.compose.ui.graphics.drawscope.Stroke(width = u(1.6f), cap = StrokeCap.Round, join = StrokeJoin.Round)
        val p = Path().apply {
            moveTo(u(6f), u(16f))
            cubicTo(u(4f), u(16f), u(4f), u(13f), u(6f), u(13f))
            cubicTo(u(6f), u(10f), u(9f), u(8f), u(12f), u(8f))
            cubicTo(u(15f), u(8f), u(18f), u(10f), u(18f), u(13f))
            cubicTo(u(20f), u(13f), u(20f), u(16f), u(18f), u(16f))
            close()
        }
        drawPath(p, c, style = w)
    }

    /** Fibers (render fibers): wavy lines. */
    val Fibers = Glyph.Drawn { c ->
        val w = u(1.6f)
        for (i in 0 until 3) {
            val y = 8f + i * 4f
            val p = Path().apply {
                moveTo(u(4f), u(y))
                cubicTo(u(8f), u(y - 3f), u(10f), u(y + 3f), u(14f), u(y))
                cubicTo(u(18f), u(y - 3f), u(20f), u(y + 3f), u(20f), u(y))
            }
            drawPath(p, c, style = androidx.compose.ui.graphics.drawscope.Stroke(width = w, cap = StrokeCap.Round))
        }
    }

    /** A vanishing point: converging lines. */
    val Vanishing = Glyph.Drawn { c ->
        val w = u(1.6f)
        drawLine(c, androidx.compose.ui.geometry.Offset(u(4f), u(4f)), androidx.compose.ui.geometry.Offset(u(12f), u(12f)), w, StrokeCap.Round)
        drawLine(c, androidx.compose.ui.geometry.Offset(u(20f), u(4f)), androidx.compose.ui.geometry.Offset(u(12f), u(12f)), w, StrokeCap.Round)
        drawLine(c, androidx.compose.ui.geometry.Offset(u(4f), u(20f)), androidx.compose.ui.geometry.Offset(u(12f), u(12f)), w, StrokeCap.Round)
        drawLine(c, androidx.compose.ui.geometry.Offset(u(20f), u(20f)), androidx.compose.ui.geometry.Offset(u(12f), u(12f)), w, StrokeCap.Round)
        drawCircle(c, radius = u(2f), center = androidx.compose.ui.geometry.Offset(u(12f), u(12f)))
    }

    /** A camera (camera raw). */
    val Camera = Glyph.Drawn { c ->
        val w = androidx.compose.ui.graphics.drawscope.Stroke(width = u(1.6f), cap = StrokeCap.Round, join = StrokeJoin.Round)
        drawRect(c, androidx.compose.ui.geometry.Offset(u(4f), u(8f)), androidx.compose.ui.geometry.Size(u(16f), u(10f)), style = w)
        drawCircle(c, radius = u(3.5f), center = androidx.compose.ui.geometry.Offset(u(12f), u(13f)), style = w)
        drawLine(c, androidx.compose.ui.geometry.Offset(u(8f), u(8f)), androidx.compose.ui.geometry.Offset(u(10f), u(5f)), w.width, StrokeCap.Round)
        drawLine(c, androidx.compose.ui.geometry.Offset(u(14f), u(5f)), androidx.compose.ui.geometry.Offset(u(16f), u(8f)), w.width, StrokeCap.Round)
    }

    /** A high-pass: a wavy line through a band. */
    val HighPass = Glyph.Drawn { c ->
        val w = u(1.6f)
        drawLine(c, androidx.compose.ui.geometry.Offset(u(4f), u(8f)), androidx.compose.ui.geometry.Offset(u(20f), u(8f)), w, StrokeCap.Round)
        drawLine(c, androidx.compose.ui.geometry.Offset(u(4f), u(16f)), androidx.compose.ui.geometry.Offset(u(20f), u(16f)), w, StrokeCap.Round)
        val p = Path().apply {
            moveTo(u(4f), u(12f))
            cubicTo(u(8f), u(6f), u(10f), u(18f), u(14f), u(12f))
            cubicTo(u(18f), u(6f), u(20f), u(18f), u(20f), u(12f))
        }
        drawPath(p, c, style = androidx.compose.ui.graphics.drawscope.Stroke(width = w, cap = StrokeCap.Round))
    }
}
