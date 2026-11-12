package io.github.artcraftandroid.filmcraft.ui.editor

import android.graphics.*
import kotlin.math.*

/** Keeps point/curve edits vector-based until the user applies them to a raster layer. */
internal class DrawingShapes {
    val names = listOf("Circle", "Rectangle", "Rounded rectangle", "Triangle", "Square", "Rounded square", "Lines", "Curves")
    var kind = "Circle"
        set(value) { clear(); field = value }
    val points = mutableListOf<PointF>()
    private val controls = mutableMapOf<Int, PointF>()
    var connected = false
    var preview: Path? = null
        private set
    private data class Snapshot(val points: List<PointF>, val controls: Map<Int, PointF>, val preview: Path?)
    private var backup: Snapshot? = null
    fun endGesture() { backup = null }
    fun cancelGesture() {
        backup?.let { state -> points.clear(); points.addAll(state.points); controls.clear(); controls.putAll(state.controls); preview = state.preview }
        backup = null
    }
    private var start = PointF()
    private var pointHandle = -1
    private var curveHandle = -1
    private var controlOffset = PointF()
    val multi get() = kind == "Lines" || kind == "Curves"
    val pending get() = points.isNotEmpty() || preview != null
    fun clear() { backup = null; points.clear(); controls.clear(); preview = null; connected = false; pointHandle = -1; curveHandle = -1 }
    private fun control(i: Int): PointF {
        val a = points[i]; val b = points[(i + 1) % points.size]
        return controls[i] ?: PointF((a.x + b.x) / 2, (a.y + b.y) / 2)
    }
    fun path(): Path? {
        if (!multi) return preview
        if (points.size < 2) return null
        return Path().apply {
            moveTo(points[0].x, points[0].y)
            val segments = if (connected) points.size else points.size - 1
            for (i in 0 until segments) { val b = points[(i + 1) % points.size]; if (kind == "Curves") { val c = control(i); quadTo(c.x, c.y, b.x, b.y) } else lineTo(b.x, b.y) }
            if (connected) close()
        }
    }
    fun down(x: Float, y: Float, tolerance: Float) {
        backup = Snapshot(points.map { PointF(it.x, it.y) }, controls.mapValues { PointF(it.value.x, it.value.y) }, preview?.let { Path(it) })
        start = PointF(x, y); pointHandle = -1; curveHandle = -1
        if (!multi) { drag(x, y); return }
        pointHandle = points.indices.minByOrNull { hypot(points[it].x - x, points[it].y - y) }?.takeIf { hypot(points[it].x - x, points[it].y - y) <= tolerance } ?: -1
        if (pointHandle >= 0) return
        if (kind == "Curves" && points.size > 1) {
            var nearest = tolerance
            val segments = if (connected) points.size else points.size - 1
            for (i in 0 until segments) {
                val c = control(i)
                val distance = hypot(c.x - x, c.y - y)
                if (distance < nearest) { nearest = distance; curveHandle = i }
            }
            for (i in 0 until segments) {
                val a = points[i]; val b = points[(i + 1) % points.size]; val c = control(i)
                for (step in 1..19) { val t = step / 20f; val u = 1 - t; val xx = u*u*a.x + 2*u*t*c.x + t*t*b.x; val yy = u*u*a.y + 2*u*t*c.y + t*t*b.y; val d = hypot(xx-x, yy-y); if (d < nearest) { nearest = d; curveHandle = i } }
            }
            if (curveHandle >= 0) { val c = control(curveHandle); controlOffset = PointF(c.x-x, c.y-y); return }
        }
        points.add(PointF(x, y)); pointHandle = points.lastIndex
    }
    fun drag(x: Float, y: Float) {
        if (multi) {
            if (pointHandle >= 0) points[pointHandle].set(x, y)
            if (curveHandle >= 0) controls[curveHandle] = PointF(x + controlOffset.x, y + controlOffset.y)
            return
        }
        val dx = x-start.x; val dy = y-start.y; val radius = hypot(dx, dy)
        val rectangle = RectF(min(start.x,x), min(start.y,y), max(start.x,x), max(start.y,y))
        preview = Path().apply {
            when (kind) {
                "Circle" -> addCircle(start.x, start.y, radius, Path.Direction.CW)
                "Rectangle" -> addRect(rectangle, Path.Direction.CW)
                "Rounded rectangle" -> { val round = min(rectangle.width(), rectangle.height()) * .2f; addRoundRect(rectangle, round, round, Path.Direction.CW) }
                "Triangle" -> { val angle = atan2(dy, dx); for (i in 0..2) { val a = angle + i * 2 * PI.toFloat() / 3; val xx = start.x + cos(a)*radius; val yy = start.y + sin(a)*radius; if (i == 0) moveTo(xx, yy) else lineTo(xx, yy) }; close() }
                else -> { val half = max(abs(dx), abs(dy)); val square = RectF(start.x-half,start.y-half,start.x+half,start.y+half); if (kind == "Rounded square") addRoundRect(square, half*.35f, half*.35f, Path.Direction.CW) else addRect(square, Path.Direction.CW) }
            }
        }
    }
    fun removeLast() { if (points.isNotEmpty()) { controls.remove(points.lastIndex); points.removeAt(points.lastIndex); controls.remove(points.lastIndex) } else preview = null }
    fun guides(c: Canvas, scale: Float) {
        if (!multi) return
        val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.CYAN; strokeWidth = 1.5f/scale; style = Paint.Style.STROKE }
        points.forEach { c.drawCircle(it.x,it.y,7/scale,paint) }
        if (kind == "Curves" && points.size > 1) for (i in 0 until (if (connected) points.size else points.size-1)) {
            val control = control(i); val a = points[i]; val b = points[(i+1)%points.size]
            c.drawLine(a.x,a.y,control.x,control.y,paint); c.drawLine(b.x,b.y,control.x,control.y,paint); c.drawCircle(control.x,control.y,4/scale,paint)
        }
    }
}
