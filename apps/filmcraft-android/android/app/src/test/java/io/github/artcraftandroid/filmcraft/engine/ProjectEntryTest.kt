package io.github.artcraftandroid.filmcraft.engine

import java.io.File
import org.junit.Assert.*
import org.junit.Test

class ProjectEntryTest {
    private fun entry(kind: String = "video") = ProjectEntry("id", "Name", 2, 1, 0, 1080, 1920, File("library/item"), kind)

    @Test fun videoProjectsRetainTheirExistingStorageLayout() {
        val p = entry()
        assertEquals("video", p.kind)
        assertEquals(File("library/item/project.fcproj"), p.projectFile)
        assertEquals(File("library/item/poster.jpg"), p.poster)
        assertFalse(p.temporary)
        assertNull(p.sourceUri)
    }

    @Test fun nativeDrawingsUseNativeThumbnailsWithoutInventingVideoPosters() {
        val p = entry("drawing")
        assertEquals(File("library/item/thumbnail.png"), p.poster)
        assertEquals("drawing", p.kind)
    }

    @Test fun picturesKeepTheirContentUriAndDoNotRequireAProjectFile() {
        val p = entry("picture").copy(sourceUri = "content://media/external/images/media/42")
        assertEquals("content://media/external/images/media/42", p.sourceUri)
        assertEquals("picture", p.kind)
        assertFalse(p.temporary)
    }

    @Test fun temporaryDrawingShellsKeepTheirHiddenMarkerWhenTouchedOrRenamed() {
        val p = entry().copy(temporary = true)
        assertTrue(p.copy(opened = 12, name = "Drawing").temporary)
        assertFalse(entry("drawing").temporary)
    }
}
