package io.github.artcraftandroid.filmcraft.ui.photocraft

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.LinearGradient
import android.graphics.Paint
import android.graphics.Path
import android.graphics.PorterDuff
import android.graphics.PorterDuffXfermode
import android.graphics.RadialGradient
import android.graphics.Rect
import android.graphics.RectF
import android.graphics.Shader
import android.graphics.Typeface
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.toArgb
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.min
import kotlin.math.sin

/**
 * The paint layer over the photo: every brush stroke, shape, line, gradient, fill and piece of type
 * the tools commit, in order, replayed onto one transparent bitmap the size of the preview. The same
 * list is replayed at the saved photo's size when Save runs, so what is on screen is what is written.
 *
 * Operations hold normalised coordinates (0…1 of the picture), which is what lets the layer scale
 * from the 1440 px preview to a 12 MP working copy without drifting.
 */
sealed interface PaintOp {
    fun render(canvas: Canvas, width: Int, height: Int)
}

/** A brush, pencil, pen or eraser stroke: the points the finger passed through. */
data class StrokeOp(
    val points: List<Offset>,
    val color: Color,
    val width: Float,
    val opacity: Float,
    val erase: Boolean,
    val round: Boolean,
) : PaintOp {
    override fun render(canvas: Canvas, frameWidth: Int, frameHeight: Int) {
        if (points.isEmpty()) return
        val thickness = (width * min(frameWidth, frameHeight)).coerceAtLeast(1f)
        val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            style = Paint.Style.STROKE
            strokeWidth = thickness
            strokeCap = if (round) Paint.Cap.ROUND else Paint.Cap.BUTT
            strokeJoin = Paint.Join.ROUND
            alpha = if (erase) 255 else (opacity.coerceIn(0f, 1f) * 255f).toInt().coerceIn(1, 255)
            if (erase) xfermode = PorterDuffXfermode(PorterDuff.Mode.CLEAR) else this.color = this@StrokeOp.color.toArgb()
        }
        val first = points.first()
        if (points.size == 1) {
            val dot = Paint(paint).apply { style = Paint.Style.FILL }
            canvas.drawCircle(first.x * frameWidth, first.y * frameHeight, thickness / 2f, dot)
            return
        }
        val path = Path()
        path.moveTo(first.x * frameWidth, first.y * frameHeight)
        for (index in 1 until points.size) {
            val point = points[index]
            path.lineTo(point.x * frameWidth, point.y * frameHeight)
        }
        canvas.drawPath(path, paint)
    }
}

enum class PaintShape { Rectangle, Ellipse, Triangle, Polygon, Star, Line }

/** A dragged shape or line, from where the finger went down to where it came up. */
data class ShapeOp(
    val shape: PaintShape,
    val from: Offset,
    val to: Offset,
    val color: Color,
    val stroke: Float,
    val filled: Boolean,
    val sides: Int,
    val arrow: Boolean,
) : PaintOp {
    override fun render(canvas: Canvas, width: Int, height: Int) {
        val left = min(from.x, to.x) * width
        val top = min(from.y, to.y) * height
        val right = maxOf(from.x, to.x) * width
        val bottom = maxOf(from.y, to.y) * height
        val bounds = RectF(left, top, right, bottom)
        val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            this.color = this@ShapeOp.color.toArgb()
            style = if (filled) Paint.Style.FILL else Paint.Style.STROKE
            strokeWidth = (stroke * min(width, height)).coerceAtLeast(1f)
            strokeJoin = Paint.Join.ROUND
            strokeCap = Paint.Cap.ROUND
        }
        when (shape) {
            PaintShape.Rectangle -> canvas.drawRect(bounds, paint)
            PaintShape.Ellipse -> canvas.drawOval(bounds, paint)
            PaintShape.Line -> {
                val start = Offset(from.x * width, from.y * height)
                val end = Offset(to.x * width, to.y * height)
                canvas.drawLine(start.x, start.y, end.x, end.y, paint)
                if (arrow) {
                    val head = Paint(paint).apply { style = Paint.Style.FILL }
                    arrowHead(canvas, start, end, head, width, height)
                    arrowHead(canvas, end, start, head, width, height)
                }
            }
            PaintShape.Triangle, PaintShape.Polygon, PaintShape.Star -> {
                val center = Offset(bounds.centerX(), bounds.centerY())
                val radiusX = bounds.width() / 2f
                val radiusY = bounds.height() / 2f
                val corners = when (shape) {
                    PaintShape.Triangle -> 3
                    PaintShape.Star -> (sides.coerceIn(3, 20)) * 2
                    else -> sides.coerceIn(3, 12)
                }
                val path = Path()
                for (index in 0 until corners) {
                    val angle = -PI / 2 + index * 2 * PI / corners
                    val factor = if (shape == PaintShape.Star && index % 2 == 1) .45f else 1f
                    val x = center.x + cos(angle).toFloat() * radiusX * factor
                    val y = center.y + sin(angle).toFloat() * radiusY * factor
                    if (index == 0) path.moveTo(x, y) else path.lineTo(x, y)
                }
                path.close()
                canvas.drawPath(path, paint)
            }
        }
    }

    private fun arrowHead(canvas: Canvas, from: Offset, to: Offset, paint: Paint, width: Int, height: Int) {
        val angle = kotlin.math.atan2((to.y - from.y) * height, (to.x - from.x) * width)
        val size = (stroke * min(width, height)).coerceAtLeast(1f) * 4f
        val a = Offset(to.x * width, to.y * height)
        val left = Offset(a.x - cos(angle - .5f).toFloat() * size, a.y - sin(angle - .5f).toFloat() * size)
        val right = Offset(a.x - cos(angle + .5f).toFloat() * size, a.y - sin(angle + .5f).toFloat() * size)
        val path = Path().apply {
            moveTo(a.x, a.y)
            lineTo(left.x, left.y)
            lineTo(right.x, right.y)
            close()
        }
        canvas.drawPath(path, paint)
    }
}

/** Type placed on the picture: horizontal or stacked, in one of the bundled faces. */
data class TextOp(
    val text: String,
    val at: Offset,
    val size: Float,
    val color: Color,
    val font: String,
    val align: Int,
    val vertical: Boolean,
) : PaintOp {
    override fun render(canvas: Canvas, width: Int, height: Int) {
        if (text.isEmpty()) return
        val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            color = this@TextOp.color.toArgb()
            textSize = (size * height).coerceAtLeast(4f)
            typeface = Typeface.create(font, Typeface.NORMAL)
        }
        val x = at.x * width
        val y = at.y * height
        if (vertical) {
            var cursor = y
            for (character in text) {
                val advance = paint.measureText(character.toString())
                val shift = when (align) {
                    1 -> -advance / 2f
                    2 -> -advance
                    else -> 0f
                }
                canvas.drawText(character.toString(), x + shift, cursor, paint)
                cursor += paint.textSize * 1.02f
            }
        } else {
            val measured = paint.measureText(text)
            val drawX = when (align) {
                1 -> x - measured / 2f
                2 -> x - measured
                else -> x
            }
            canvas.drawText(text, drawX, y, paint)
        }
    }
}

/** A gradient dragged over the picture: linear or radial, in the two colours of the swatches. */
data class GradientOp(
    val from: Offset,
    val to: Offset,
    val start: Color,
    val end: Color,
    val radial: Boolean,
    val opacity: Float,
) : PaintOp {
    override fun render(canvas: Canvas, width: Int, height: Int) {
        val x0 = from.x * width
        val y0 = from.y * height
        val x1 = to.x * width
        val y1 = to.y * height
        val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            alpha = (opacity.coerceIn(0f, 1f) * 255f).toInt().coerceIn(1, 255)
            val colors = intArrayOf(start.toArgb(), end.toArgb())
            shader = if (radial) {
                val radius = kotlin.math.hypot((x1 - x0).toDouble(), (y1 - y0).toDouble()).toFloat().coerceAtLeast(1f)
                RadialGradient(x0, y0, radius, colors, null, Shader.TileMode.CLAMP)
            } else {
                LinearGradient(x0, y0, x1, y1, colors, null, Shader.TileMode.CLAMP)
            }
        }
        canvas.drawRect(0f, 0f, width.toFloat(), height.toFloat(), paint)
    }
}

/** A finished fill: the flood-filled patch, kept as pixels because the region is the picture's. */
class PatchOp(private val patch: Bitmap) : PaintOp {
    override fun render(canvas: Canvas, width: Int, height: Int) {
        if (patch.isRecycled) return
        canvas.drawBitmap(patch, null, Rect(0, 0, width, height), Paint(Paint.FILTER_BITMAP_FLAG))
    }
}

/** An operation clipped to the selection that was live when it was made. */
class ClippedOp(private val inner: PaintOp, private val clip: List<Offset>?) : PaintOp {
    override fun render(canvas: Canvas, width: Int, height: Int) {
        val points = clip
        if (points == null || points.size < 3) {
            inner.render(canvas, width, height)
            return
        }
        val save = canvas.save()
        canvas.clipPath(selectionPath(points, width, height))
        inner.render(canvas, width, height)
        canvas.restoreToCount(save)
    }
}

/** A selection polygon in pixels, for clipping paint and for drawing the marching ants. */
internal fun selectionPath(points: List<Offset>, width: Int, height: Int): Path {
    val path = Path()
    points.forEachIndexed { index, point ->
        val x = point.x * width
        val y = point.y * height
        if (index == 0) path.moveTo(x, y) else path.lineTo(x, y)
    }
    path.close()
    return path
}

/** The layer itself: the operations, the bitmap they replay into, and the undo history. */
class PhotoPaintState {
    private var bitmap: Bitmap? = null
    private var canvas: Canvas? = null
    private val ops = ArrayList<PaintOp>()
    private var active: PaintOp? = null
    private var layerWidth = 0
    private var layerHeight = 0

    /** The layer as a Compose image; a new wrapper after every change so the canvas redraws. */
    var image by mutableStateOf<ImageBitmap?>(null)
        private set
    var version by mutableIntStateOf(0)
        private set
    var marked by mutableStateOf(false)
        private set

    /** The live bitmap, handed to the controller so Save develops the same strokes. */
    val pixels: Bitmap? get() = bitmap

    /** The live selection, as a normalised polygon: paint made now is clipped to it. */
    var selection by mutableStateOf<List<Offset>?>(null)

    fun select(points: List<Offset>?) {
        selection = points
    }

    val isEmpty: Boolean get() = ops.isEmpty() && active == null
    val canUndo: Boolean get() = ops.isNotEmpty()

    /** Match the layer to the preview's pixel size; strokes survive because they are normalised. */
    fun resize(width: Int, height: Int) {
        if (width <= 0 || height <= 0) return
        if (width == layerWidth && height == layerHeight && bitmap != null) return
        layerWidth = width
        layerHeight = height
        val next = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        bitmap?.recycle()
        bitmap = next
        canvas = Canvas(next)
        redraw()
    }

    fun begin(op: PaintOp) {
        active = ClippedOp(op, selection)
        redraw()
    }

    fun commit() {
        val finished = active ?: return
        active = null
        ops.add(finished)
        marked = true
        redraw()
    }

    fun cancel() {
        active = null
        redraw()
    }

    fun undoPainted(): Boolean {
        if (ops.isEmpty()) return false
        ops.removeAt(ops.size - 1)
        redraw()
        return true
    }

    fun clear() {
        ops.clear()
        active = null
        marked = false
        redraw()
    }

    fun add(op: PaintOp) {
        ops.add(ClippedOp(op, selection))
        marked = true
        redraw()
    }

    private fun redraw() {
        val canvas = canvas ?: return
        canvas.drawColor(0, PorterDuff.Mode.CLEAR)
        for (op in ops) op.render(canvas, layerWidth, layerHeight)
        active?.render(canvas, layerWidth, layerHeight)
        image = bitmap?.asImageBitmap()
        version++
    }

    /** The picture with the paint on top, at the save's size. Used before handing over to Photos. */
    fun flatten(source: Bitmap): Bitmap {
        if (ops.isEmpty()) return source
        val out = source.copy(Bitmap.Config.ARGB_8888, true) ?: return source
        val target = Canvas(out)
        for (op in ops) op.render(target, out.width, out.height)
        return out
    }

    /** Flood fill from a point, on a snapshot of the whole picture: the PatchOp it returns. */
    fun fill(source: Bitmap, at: Offset, color: Color, tolerance: Float): PaintOp {
        val width = source.width
        val height = source.height
        val pixels = IntArray(width * height)
        source.getPixels(pixels, 0, width, 0, 0, width, height)
        val start = (at.y.coerceIn(0f, .999f) * height).toInt() * width +
            (at.x.coerceIn(0f, .999f) * width).toInt()
        val from = pixels[start]
        val replacement = color.toArgb() or (0xFF shl 24)
        val limit = (tolerance.coerceIn(0f, 1f) * 255f * 3f)
        fun near(a: Int, b: Int): Boolean {
            val dr = kotlin.math.abs(((a shr 16) and 0xFF) - ((b shr 16) and 0xFF))
            val dg = kotlin.math.abs(((a shr 8) and 0xFF) - ((b shr 8) and 0xFF))
            val db = kotlin.math.abs((a and 0xFF) - (b and 0xFF))
            return dr + dg + db <= limit
        }
        if (from == replacement) return PatchOp(source.copy(Bitmap.Config.ARGB_8888, false) ?: source)
        val stack = ArrayDeque<Int>()
        stack.addLast(start)
        val seen = BooleanArray(width * height)
        seen[start] = true
        while (stack.isNotEmpty()) {
            val index = stack.removeLast()
            pixels[index] = replacement
            val x = index % width
            val y = index / width
            if (x > 0 && !seen[index - 1] && near(pixels[index - 1], from)) { seen[index - 1] = true; stack.addLast(index - 1) }
            if (x < width - 1 && !seen[index + 1] && near(pixels[index + 1], from)) { seen[index + 1] = true; stack.addLast(index + 1) }
            if (y > 0 && !seen[index - width] && near(pixels[index - width], from)) { seen[index - width] = true; stack.addLast(index - width) }
            if (y < height - 1 && !seen[index + width] && near(pixels[index + width], from)) { seen[index + width] = true; stack.addLast(index + width) }
        }
        val patch = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        // Keep only the filled pixels: the patch sits above the developed picture, not in place of it.
        val mask = IntArray(width * height)
        for (index in pixels.indices) mask[index] = if (pixels[index] == replacement) replacement else 0
        patch.setPixels(mask, 0, width, 0, 0, width, height)
        return PatchOp(patch)
    }
}
