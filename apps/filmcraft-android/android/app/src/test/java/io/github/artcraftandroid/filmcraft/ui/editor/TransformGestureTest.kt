package io.github.artcraftandroid.filmcraft.ui.editor

import org.junit.Assert.assertEquals
import org.junit.Test

class TransformGestureTest {
    @Test fun interactiveFramesHaveAThirtyFpsBudgetAndBoundedAdaptiveResolution() {
        assertEquals(33_333_333L,LivePreviewPolicy.FRAME_NANOS)
        org.junit.Assert.assertTrue(LivePreviewPolicy.nextSide(720,720,50_000_000L)<720)
        assertEquals(160,LivePreviewPolicy.nextSide(160,720,50_000_000L))
        assertEquals(720,LivePreviewPolicy.nextSide(720,720,10_000_000L))
    }

    @Test fun pinchTwistNoiseStaysInTheDeadZone() {
        val gate=RotationGate()
        listOf(1f,-0.5f,2f,-1f,3f,-2f).forEach {assertEquals(0f,gate.update(it),0.0001f)}
    }
    @Test fun slowIntentionalTwistsAccumulateWithoutJumpingEightDegrees() {
        val gate=RotationGate()
        repeat(8) {assertEquals(0f,gate.update(1f),0.0001f)}
        assertEquals(1f,gate.update(1f),0.0001f)
        assertEquals(2f,gate.update(2f),0.0001f)
    }
    @Test fun clockwiseAndCounterclockwiseAreSymmetricAndPointerChangesReset() {
        val gate=RotationGate()
        assertEquals(-2f,gate.update(-10f),0.0001f)
        gate.reset()
        assertEquals(0f,gate.update(6f),0.0001f)
        assertEquals(2f,gate.update(4f),0.0001f)
    }
}
