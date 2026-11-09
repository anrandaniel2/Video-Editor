package io.github.artcraftandroid.filmcraft.ui.editor

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.cos
import kotlin.math.sin

class MaskGeometryTest {
    @Test
    fun sourceToFrameAndBackIncludesRotationScaleAndTranslation() {
        val angle = Math.toRadians(30.0)
        val c = cos(angle); val s = sin(angle)
        val m = doubleArrayOf(2.0 * c, 2.0 * s, -2.0 * s, 2.0 * c, 300.0, 700.0)
        val p = MaskGeometry.apply(m, 120.0, 80.0)
        val inv = requireNotNull(MaskGeometry.invert(m))
        val back = MaskGeometry.apply(inv, p.first, p.second)
        assertEquals(120.0, back.first, 1e-9)
        assertEquals(80.0, back.second, 1e-9)
    }

    @Test
    fun mirrorIsItsOwnInverseAndDegenerateTransformsAreRejected() {
        val mirror = doubleArrayOf(-1.0, 0.0, 0.0, 1.0, 1920.0, 0.0)
        val p = MaskGeometry.apply(mirror, 10.0, 20.0)
        assertEquals(1910.0, p.first, 1e-9)
        val inv = requireNotNull(MaskGeometry.invert(mirror))
        assertArrayEquals(mirror, inv, 1e-9)
        assertNull(MaskGeometry.invert(doubleArrayOf(0.0, 0.0, 0.0, 0.0, 10.0, 10.0)))
        assertNull(MaskGeometry.invert(doubleArrayOf(Double.NaN, 0.0, 0.0, 1.0, 0.0, 0.0)))
    }

    @Test
    fun freehandSimplificationKeepsCornersAndBoundsThePayload() {
        val line = List(100) { it.toDouble() to 0.0 }
        assertEquals(listOf(0.0 to 0.0, 99.0 to 0.0), MaskGeometry.simplify(line, 1.0))
        val corner = listOf(0.0 to 0.0, 10.0 to 0.0, 10.0 to 10.0)
        assertEquals(corner, MaskGeometry.simplify(corner, 0.5))
        val noisy = List(1000) { it.toDouble() to kotlin.math.sin(it * 0.7) }
        val simple = MaskGeometry.simplify(noisy, 0.2, maxPoints = 64)
        assertTrue(simple.size <= 64)
        assertEquals(noisy.first(), simple.first())
        assertEquals(noisy.last(), simple.last())
        assertEquals(emptyList<Pair<Double, Double>>(), MaskGeometry.simplify(emptyList(), 1.0))
        val dense = List(100) { it.toDouble() to it.toDouble() }
        val exact = MaskGeometry.simplify(dense, 0.0, maxPoints = 10)
        assertEquals(10, exact.size)
        assertEquals(dense.first(), exact.first())
        assertEquals(dense.last(), exact.last())
    }

    @Test
    fun pointInsertionFindsTheNearestClosingEdge() {
        val square = listOf(0.0 to 0.0, 10.0 to 0.0, 10.0 to 10.0, 0.0 to 10.0)
        assertEquals(0, MaskGeometry.nearestEdge(square, 5.0, 0.1))
        assertEquals(2, MaskGeometry.nearestEdge(square, 5.0, 9.9))
        assertEquals(3, MaskGeometry.nearestEdge(square, 0.1, 5.0))
        assertEquals(-1, MaskGeometry.nearestEdge(emptyList(), 0.0, 0.0))
    }

    @Test
    fun hitTestingUsesTheClosedFillAndIgnoresInvalidBounds() {
        val square = listOf(0.0 to 0.0, 10.0 to 0.0, 10.0 to 10.0, 0.0 to 10.0)
        assertTrue(MaskGeometry.contains(square, 5.0, 5.0))
        assertFalse(MaskGeometry.contains(square, 11.0, 5.0))
        assertArrayEquals(doubleArrayOf(0.0, 0.0, 10.0, 10.0), requireNotNull(MaskGeometry.bounds(square)), 1e-9)
        assertNull(MaskGeometry.bounds(emptyList()))
        assertNull(MaskGeometry.bounds(listOf(Double.NaN to 0.0)))
        // Two overlapping squares wound the same way: the overlap is inside the engine's
        // nonzero fill even though an even-odd rule would cancel it.
        val overlapping = listOf(0.0 to 0.0, 2.0 to 0.0, 2.0 to 2.0, 0.0 to 2.0, 1.0 to 0.0, 3.0 to 0.0, 3.0 to 2.0, 1.0 to 2.0)
        assertTrue(MaskGeometry.contains(overlapping, 1.5, 1.0))
        assertEquals(100.0, MaskGeometry.area(square), 1e-9)
        assertEquals(0.0, MaskGeometry.area(listOf(0.0 to 0.0, 1.0 to 1.0, 2.0 to 2.0)), 1e-9)
    }
}
