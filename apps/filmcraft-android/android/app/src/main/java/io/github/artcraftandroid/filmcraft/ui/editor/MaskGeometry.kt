package io.github.artcraftandroid.filmcraft.ui.editor

import kotlin.math.hypot
import kotlin.math.max
import kotlin.math.min

/**
 * Mask gesture math in Kotlin. The engine owns the path and the source-to-frame matrix; these
 * helpers only simplify a finger stroke and hit-test handles. Keeping them free of Android and
 * Compose types makes the coordinate-sensitive cases ordinary JVM unit tests.
 */
object MaskGeometry {
    /** Apply the bridge's `[a,b,c,d,e,f]` affine to a point. */
    fun apply(m: DoubleArray, x: Double, y: Double): Pair<Double, Double> {
        require(m.size == 6)
        return Pair(m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
    }

    /** Invert a source-to-frame matrix, or null when the picture has been flattened to a line. */
    fun invert(m: DoubleArray): DoubleArray? {
        require(m.size == 6)
        val det = m[0] * m[3] - m[1] * m[2]
        if (!det.isFinite() || kotlin.math.abs(det) < 1e-12) return null
        val a = m[3] / det
        val b = -m[1] / det
        val c = -m[2] / det
        val d = m[0] / det
        return doubleArrayOf(a, b, c, d, -(a * m[4] + c * m[5]), -(b * m[4] + d * m[5]))
    }

    /** `[minX,minY,maxX,maxY]`, or null for an empty path. */
    fun bounds(points: List<Pair<Double, Double>>): DoubleArray? {
        if (points.isEmpty()) return null
        var x0 = Double.POSITIVE_INFINITY; var y0 = Double.POSITIVE_INFINITY
        var x1 = Double.NEGATIVE_INFINITY; var y1 = Double.NEGATIVE_INFINITY
        for ((x, y) in points) {
            if (!x.isFinite() || !y.isFinite()) continue
            x0 = min(x0, x); y0 = min(y0, y)
            x1 = max(x1, x); y1 = max(y1, y)
        }
        return if (x0.isFinite() && y0.isFinite()) doubleArrayOf(x0, y0, x1, y1) else null
    }

    /** Index of the segment nearest a point; the segment joins index i to i+1 (wrapping). */
    fun nearestEdge(points: List<Pair<Double, Double>>, x: Double, y: Double): Int {
        if (points.isEmpty()) return -1
        if (points.size == 1) return 0
        var best = 0
        var distance = Double.POSITIVE_INFINITY
        for (i in points.indices) {
            val a = points[i]
            val b = points[(i + 1) % points.size]
            val dx = b.first - a.first; val dy = b.second - a.second
            val len2 = dx * dx + dy * dy
            val t = if (len2 > 0.0) (((x - a.first) * dx + (y - a.second) * dy) / len2).coerceIn(0.0, 1.0) else 0.0
            val d = hypot(x - (a.first + t * dx), y - (a.second + t * dy))
            if (d < distance) { distance = d; best = i }
        }
        return best
    }

    /**
     * Ramer–Douglas–Peucker for an open finger stroke. The caller closes the resulting polygon.
     * Iterative splitting avoids recursion on long strokes; [maxPoints] bounds the bridge payload.
     */
    fun simplify(points: List<Pair<Double, Double>>, tolerance: Double, maxPoints: Int = 256): List<Pair<Double, Double>> {
        val input = points.filter { it.first.isFinite() && it.second.isFinite() }
        val limit = maxPoints.coerceAtLeast(2)
        if (input.size <= 2) return input
        if (tolerance <= 0.0 || !tolerance.isFinite()) return thin(input, limit)
        val keep = BooleanArray(input.size)
        keep[0] = true; keep[input.lastIndex] = true
        val stack = ArrayDeque<Pair<Int, Int>>()
        stack.addLast(0 to input.lastIndex)
        val tol2 = tolerance * tolerance
        while (stack.isNotEmpty()) {
            val (first, last) = stack.removeLast()
            val a = input[first]; val b = input[last]
            val dx = b.first - a.first; val dy = b.second - a.second
            val len2 = dx * dx + dy * dy
            var far = first + 1
            var best = -1.0
            for (i in first + 1 until last) {
                val p = input[i]
                val t = if (len2 > 0.0) (((p.first - a.first) * dx + (p.second - a.second) * dy) / len2).coerceIn(0.0, 1.0) else 0.0
                val ex = p.first - (a.first + t * dx); val ey = p.second - (a.second + t * dy)
                val d = ex * ex + ey * ey
                if (d > best) { best = d; far = i }
            }
            if (best > tol2 && far in (first + 1) until last) {
                keep[far] = true
                stack.addLast(first to far); stack.addLast(far to last)
            }
        }
        val out = input.filterIndexed { i, _ -> keep[i] }
        return thin(out, limit)
    }

    /** Evenly thin a stroke while retaining both ends. */
    private fun thin(points: List<Pair<Double, Double>>, limit: Int): List<Pair<Double, Double>> {
        if (points.size <= limit) return points
        return List(limit) { points[it * (points.size - 1) / (limit - 1)] }
    }

    /** Nonzero-winding fill hit test, matching the engine's coverage rule for self-crossing paths. */
    fun contains(points: List<Pair<Double, Double>>, x: Double, y: Double): Boolean {
        var winding = 0
        for (i in points.indices) {
            val a = points[i]; val b = points[(i + 1) % points.size]
            if ((a.second <= y) != (b.second <= y)) {
                val cross = a.first + (y - a.second) / (b.second - a.second) * (b.first - a.first)
                if (x < cross) winding += if (b.second > a.second) 1 else -1
            }
        }
        return winding != 0
    }

    /** Absolute shoelace area; zero for a line or a point (not a usable closed mask). */
    fun area(points: List<Pair<Double, Double>>): Double {
        if (points.size < 3) return 0.0
        var sum = 0.0
        for (i in points.indices) {
            val a = points[i]; val b = points[(i + 1) % points.size]
            sum += a.first * b.second - b.first * a.second
        }
        return kotlin.math.abs(sum) * 0.5
    }
}
