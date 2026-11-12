package io.github.artcraftandroid.filmcraft.ui.editor

import android.graphics.*
import kotlin.math.*

internal data class DrawingBrush(val name: String, val family: String, val variant: Int)
internal object DrawingBrushes {
    val families = listOf("Hair", "Writing", "Ink", "Pencil", "Marker", "Airbrush", "Dots", "Stars", "Chains", "Leaves", "Flowers", "Hearts", "Sparkles", "Hatching", "Texture")
    private val styles = listOf("Fine", "Soft", "Bold", "Wide", "Dense", "Loose", "Ribbon", "Delicate", "Expressive", "Heavy")
    val all = families.flatMap { family -> styles.mapIndexed { i, style -> DrawingBrush("$family · $style", family, i) } }
    private val byName = all.associateBy { it.name }
    fun footprint(name: String, size: Float): Float {
        val b = byName[name] ?: DrawingBrush(name, if (name == "Pen") "Ink" else name, 0)
        val width = size.coerceAtLeast(1f) * (.5f + b.variant * .12f)
        return if (b.family == "Ink" || b.family == "Pen") width * .45f else width
    }
    fun draw(canvas: Canvas, path: Path, name: String, color: Int, size: Float, opacity: Float) {
        val b = byName[name] ?: DrawingBrush(name, when (name) { "Pen" -> "Ink"; else -> name }, 0)
        val v = b.variant
        val width = size.coerceAtLeast(1f) * (.5f + v * .12f)
        val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply { this.color = color; alpha = (opacity * 255).toInt().coerceIn(0, 255); style = Paint.Style.STROKE; strokeCap = Paint.Cap.ROUND; strokeJoin = Paint.Join.ROUND; strokeWidth = width }
        when (b.family) {
            "Ink", "Pen" -> { paint.strokeWidth = width * .45f; if (v % 3 == 1) paint.strokeCap = Paint.Cap.SQUARE; canvas.drawPath(path, paint) }
            "Marker" -> { paint.alpha = (paint.alpha * (.2f + v * .035f)).toInt(); paint.strokeWidth = width; paint.strokeCap = if (v % 2 == 0) Paint.Cap.SQUARE else Paint.Cap.ROUND; canvas.drawPath(path, paint) }
            "Airbrush" -> { paint.strokeWidth = width; paint.maskFilter = BlurMaskFilter(width * (.25f + v * .04f), BlurMaskFilter.Blur.NORMAL); canvas.drawPath(path, paint) }
            "Hair" -> {
                val strands = 3 + v * 2
                paint.strokeWidth = (size / (strands * 2)).coerceAtLeast(.6f)
                for (i in 0 until strands) { val shift = width * (i.toFloat() / (strands - 1) - .5f); val saved = canvas.save(); canvas.translate(shift, sin(i * 2f) * width * .12f); paint.alpha = (opacity * 255 * (.3f + (i % 4) * .17f)).toInt(); canvas.drawPath(path, paint); canvas.restoreToCount(saved) }
            }
            "Writing" -> {
                // Repeated offset thin strokes approximate a directional broad-nib pen.
                val nib = 3 + v; paint.strokeWidth = (size * .08f).coerceAtLeast(.6f)
                val angle = (20 + v * 7) * PI.toFloat() / 180
                for (i in 0..nib) { val t = (i.toFloat() / nib - .5f) * width; canvas.save(); canvas.translate(cos(angle) * t, sin(angle) * t); canvas.drawPath(path, paint); canvas.restore() }
            }
            else -> stamps(canvas, path, b, paint, width)
        }
    }
    private fun stamps(c: Canvas, path: Path, b: DrawingBrush, p: Paint, width: Float) {
        val measure = PathMeasure(path, false)
        val spacing = (width * when (b.family) { "Pencil", "Texture" -> .12f; "Chains" -> .7f; else -> 1.1f + b.variant * .09f }).coerceAtLeast(1f)
        val position = FloatArray(2); val tangent = FloatArray(2)
        var count = 0
        do {
            var distance = 0f
            while (distance <= measure.length && count < 6000) {
                measure.getPosTan(distance, position, tangent)
                c.save(); c.translate(position[0], position[1]); c.rotate(atan2(tangent[1], tangent[0]) * 180 / PI.toFloat())
                p.style = if (b.variant % 3 == 1) Paint.Style.STROKE else Paint.Style.FILL; p.strokeWidth = (width * .1f).coerceAtLeast(.7f)
                val r = width / 2
                when (b.family) {
                    "Dots" -> c.drawCircle(0f, 0f, r * (.4f + b.variant * .04f), p)
                    "Stars", "Sparkles" -> {
                        val tips = if (b.family == "Sparkles") 4 else 5 + b.variant % 3
                        c.drawPath(Path().apply { for (i in 0 until tips * 2) { val a = i * PI / tips - PI / 2; val radius = if (i % 2 == 0) r else r * (.25f + b.variant * .025f); val x = cos(a).toFloat() * radius; val y = sin(a).toFloat() * radius; if (i == 0) moveTo(x, y) else lineTo(x, y) }; close() }, p)
                    }
                    "Chains" -> { p.style = Paint.Style.STROKE; c.rotate(if (count % 2 == 0) 25f else -25f); c.drawOval(-r, -r * (.35f + b.variant * .025f), r, r * (.35f + b.variant * .025f), p) }
                    "Leaves" -> c.drawPath(Path().apply { moveTo(-r, 0f); quadTo(0f, -width, r, 0f); quadTo(0f, width * (.4f + b.variant * .04f), -r, 0f) }, p)
                    "Flowers" -> { val petals = 4 + b.variant; for (i in 0 until petals) { c.save(); c.rotate(i * 360f / petals); c.drawOval(-r * .25f, -r, r * .25f, 0f, p); c.restore() }; c.drawCircle(0f, 0f, r * .2f, p) }
                    "Hearts" -> c.drawPath(Path().apply { moveTo(0f, r); cubicTo(-width, 0f, -r, -width, 0f, -r * .3f); cubicTo(r, -width, width, 0f, 0f, r); close() }, p)
                    "Hatching" -> { p.style = Paint.Style.STROKE; c.drawLine(-r * .5f, -r, r * .5f, r, p); if (b.variant > 4) c.drawLine(-r, r * .5f, r, -r * .5f, p) }
                    "Pencil", "Texture" -> {
                        val alpha = p.alpha; p.style = Paint.Style.FILL
                        for (i in 0 until 4 + b.variant * 2) { val hash = ((count * 1103515245 + i * 12345) ushr 1); val x = ((hash % 101) / 100f - .5f) * width; val y = (((hash / 101) % 101) / 100f - .5f) * width; p.alpha = (alpha * (.12f + (i % 5) * .08f)).toInt(); c.drawCircle(x, y, (width * .035f).coerceAtLeast(.4f), p) }; p.alpha = alpha
                    }
                    else -> c.drawCircle(0f, 0f, r, p)
                }
                c.restore(); distance += spacing; count++
            }
        } while (count < 6000 && measure.nextContour())
    }
}
