package io.github.artcraftandroid.filmcraft.ui.editor

import kotlin.math.abs
import kotlin.math.sign

/** Rotation is intentionally locked during pinches until the fingers twist by eight degrees.
 * Accumulation detects slow deliberate twists; subtracting the dead zone prevents a jump.
 * Pointer-count changes must reset this gate, so lifting a finger never creates a rotation.
 */
internal class RotationGate(private val threshold: Float = 8f) {
    private var accumulated = 0f
    private var applied = 0f
    fun update(degrees: Float): Float {
        accumulated += degrees
        val wanted = if (abs(accumulated) > threshold) accumulated - sign(accumulated) * threshold else 0f
        val delta = wanted - applied
        applied = wanted
        return delta
    }
    fun reset() { accumulated = 0f; applied = 0f }
}
