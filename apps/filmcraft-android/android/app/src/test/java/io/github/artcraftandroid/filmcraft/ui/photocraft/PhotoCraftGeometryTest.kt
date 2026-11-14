package io.github.artcraftandroid.filmcraft.ui.photocraft

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PhotoCraftGeometryTest {
    private fun assertFrame(expected: NormRect, actual: NormRect, delta: Float = 0.0001f) {
        assertEquals("left", expected.left, actual.left, delta)
        assertEquals("top", expected.top, actual.top, delta)
        assertEquals("right", expected.right, actual.right, delta)
        assertEquals("bottom", expected.bottom, actual.bottom, delta)
    }

    @Test
    fun aSquareFrameOnAWidePictureUsesTheHeightAndStaysCentred() {
        // A 2:1 picture: a 1:1 frame is half the width and the full height.
        assertFrame(NormRect(0.25f, 0f, 0.75f, 1f), centeredCrop(1f, 2f))
        // A 1:1 picture is its own square frame.
        assertFrame(NormRect.Full, centeredCrop(1f, 1f))
        // 9:16 on a wide picture: full height, narrow width.
        val portrait = centeredCrop(9f / 16f, 2f)
        assertEquals(1f, portrait.height, 0.0001f)
        assertEquals(0.28125f, portrait.width, 0.0001f)
        // The locked frame is always inside the picture.
        for (aspect in listOf(0.3f, 1f, 1.5f, 4f)) {
            val frame = centeredCrop(aspect, 3f)
            assertTrue(frame.left >= 0f && frame.top >= 0f && frame.right <= 1f && frame.bottom <= 1f)
            assertEquals(aspect, frame.width * 3f / frame.height, 0.0001f)
        }
    }

    @Test
    fun aFreeFrameIsTheWholePictureAndNonsenseRatiosFallBackToIt() {
        assertFrame(NormRect.Full, centeredCrop(null, 2f))
        assertFrame(NormRect.Full, centeredCrop(0f, 2f))
        assertFrame(NormRect.Full, centeredCrop(1f, 0f))
    }

    @Test
    fun movingAFrameKeepsItInsideThePicture() {
        val frame = NormRect(0.2f, 0.2f, 0.6f, 0.6f)
        assertFrame(NormRect(0f, 0.2f, 0.4f, 0.6f), moveCrop(frame, -0.5f, 0f))
        assertFrame(NormRect(0.6f, 0.6f, 1f, 1f), moveCrop(frame, 0.9f, 0.9f))
        // A drag inside the room moves exactly as far as the finger.
        assertFrame(NormRect(0.25f, 0.25f, 0.65f, 0.65f), moveCrop(frame, 0.05f, 0.05f))
    }

    @Test
    fun draggingACornerMovesOnlyItsEdges() {
        assertFrame(NormRect(0.1f, 0.2f, 1f, 1f), resizeCrop(NormRect.Full, CropCorner.TopLeft, 0.1f, 0.2f, 1f, null))
        assertFrame(NormRect(0f, 0f, 0.8f, 0.7f), resizeCrop(NormRect.Full, CropCorner.BottomRight, -0.2f, -0.3f, 1f, null))
        // A corner can't be dragged past its opposite edge.
        val squeezed = resizeCrop(NormRect(0f, 0f, 0.2f, 0.2f), CropCorner.TopRight, -1f, 0f, 1f, null)
        assertTrue(squeezed.width >= NormRect.MinExtent - 0.0001f)
        assertEquals(0f, squeezed.left, 0.0001f)
        assertEquals(0.2f, squeezed.bottom, 0.0001f)
    }

    @Test
    fun aLockedCornerKeepsThePixelRatioAndShrinksInsteadOfLeavingThePicture() {
        // Square frame dragged on a 2:1 picture: 0.5 width is exactly 1.0 height.
        assertFrame(NormRect(0f, 0f, 0.5f, 1f), resizeCrop(NormRect.Full, CropCorner.BottomRight, -0.5f, 0f, 2f, 1f))
        // Same square on a 1:1 picture.
        assertFrame(NormRect(0f, 0f, 0.5f, 0.5f), resizeCrop(NormRect.Full, CropCorner.BottomRight, -0.5f, 0f, 1f, 1f))
        // Asking for more width than the room allows leaves a frame that still fits.
        val wide = resizeCrop(NormRect(0.5f, 0f, 1f, 1f), CropCorner.BottomLeft, -1f, 0f, 2f, 1f)
        assertTrue(wide.left >= 0f && wide.right <= 1f && wide.top >= 0f && wide.bottom <= 1f)
        assertEquals(1f, wide.width * 2f / wide.height, 0.001f)
    }

    @Test
    fun aCornerDraggedPastTheEdgeNeitherThrowsNorEscapes() {
        // The frame's own minimum bound would fall outside the picture: a drag must not crash.
        val dragged = resizeCrop(NormRect(0.97f, 0f, 1f, 1f), CropCorner.TopRight, 0.5f, 0f, 1f, null)
        assertTrue(dragged.left >= 0f && dragged.right <= 1f)
        assertTrue(dragged.bottom <= 1f && dragged.top >= 0f)
        // A locked corner whose room is narrower than the minimum frame is still attempted safely.
        val pinned = resizeCrop(NormRect(0f, 0f, 0.03f, 1f), CropCorner.TopLeft, 0.01f, 0f, 1f, 1f)
        assertTrue(pinned.left >= 0f && pinned.right <= 1f)
        assertTrue(pinned.top >= 0f && pinned.bottom <= 1f)
    }

    @Test
    fun anInvertedOrEmptyFrameIsRepairedRatherThanDropped() {
        assertFrame(NormRect(0.3f, 0.2f, 0.7f, 0.5f), NormRect(0.7f, 0.2f, 0.3f, 0.5f).clamped())
        val tiny = NormRect(0.5f, 0.5f, 0.5f, 0.5f).clamped()
        assertEquals(NormRect.MinExtent, tiny.width, 0.0001f)
        assertEquals(NormRect.MinExtent, tiny.height, 0.0001f)
        assertTrue(tiny.right <= 1f && tiny.bottom <= 1f)
    }

    @Test
    fun thePreviewBoxFitsTheWidestSideOfTheRoom() {
        assertEquals(100f to 50f, fitAspect(2f, 100f, 100f))
        assertEquals(50f to 50f, fitAspect(1f, 50f, 100f))
        assertEquals(20f to 40f, fitAspect(0.5f, 100f, 40f))
        assertEquals(100f to 100f, fitAspect(Float.NaN, 100f, 100f))
        assertEquals(1f, imageAspectOf(0, 0), 0.0001f)
        assertEquals(2f, imageAspectOf(2000, 1000), 0.0001f)
    }

    @Test
    fun editsKnowWhenTheyChangeThePicture() {
        assertFalse(PhotoEdits().hasEdits)
        assertFalse(PhotoAdjustments().dirty)
        assertTrue(PhotoEdits(quarters = 1).hasEdits)
        assertTrue(PhotoEdits(flipHorizontal = true).hasEdits)
        assertTrue(PhotoEdits(crop = NormRect(0.1f, 0.1f, 0.9f, 0.9f)).hasEdits)
        assertTrue(PhotoEdits(adjustments = PhotoAdjustments(exposure = 0.2f)).hasEdits)
        assertTrue(PhotoEdits(adjustments = PhotoAdjustments(filter = PhotoFilter.Film)).hasEdits)
        // A frame that covers everything is not an edit.
        assertFalse(PhotoEdits(crop = NormRect.Full).hasEdits)
        assertEquals(PhotoFilter.Film, PhotoFilter.of(2))
        assertEquals(PhotoFilter.Original, PhotoFilter.of(99))
    }
}
