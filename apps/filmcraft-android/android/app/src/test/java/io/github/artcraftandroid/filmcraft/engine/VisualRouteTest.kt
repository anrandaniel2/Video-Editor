package io.github.artcraftandroid.filmcraft.engine

import org.junit.Assert.*
import org.junit.Test

/**
 * Where opening a Home entry lands. The routes matter: a drawing must reopen its layered native
 * document (not a flattened picture of it), a saved picture must reopen as a photo document, and a
 * video must open the timeline editor untouched.
 */
class VisualRouteTest {
    @Test fun videosOpenTheTimelineEditor() {
        assertEquals(VisualRoute.Video, visualRoute("video"))
    }

    @Test fun drawingsOpenTheNativeLayeredDocument() {
        assertEquals(VisualRoute.Drawing, visualRoute("drawing"))
    }

    @Test fun picturesOpenAsPhotoDocumentsInTheDrawCanvas() {
        assertEquals(VisualRoute.Photo, visualRoute("picture"))
    }

    @Test fun unknownKindsAreTreatedAsPicturesRatherThanCrashing() {
        // the Draw canvas can open any decodable image; guessing "video" could not
        assertEquals(VisualRoute.Photo, visualRoute(""))
        assertEquals(VisualRoute.Photo, visualRoute("SOMETHING_FUTURE"))
    }

    @Test fun routesAreStableAcrossTheSavedLibraryShapes() {
        // the kinds written by SavedVisuals and Projects (both slash spellings of the MediaStore
        // folder land here too, because SavedVisuals only reports kind "picture")
        assertEquals(VisualRoute.Drawing, visualRoute("drawing"))
        assertEquals(VisualRoute.Photo, visualRoute("picture"))
    }
}
