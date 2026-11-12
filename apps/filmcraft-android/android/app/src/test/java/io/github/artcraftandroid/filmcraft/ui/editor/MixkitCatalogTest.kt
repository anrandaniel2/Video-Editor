package io.github.artcraftandroid.filmcraft.ui.editor

import org.jsoup.Jsoup
import org.junit.Assert.*
import org.junit.Test

class MixkitCatalogTest {
    @Test fun numericPaginationAndPosterArePublishedWithoutNextButton() {
        val doc = Jsoup.parse("""
            <div class="item-grid-card"><h3>Ocean</h3>
              <a href="/free-stock-video/ocean-42/">Ocean</a>
              <video poster="https://assets.mixkit.co/ocean.jpg"><source src="https://assets.mixkit.co/ocean.mp4"></video>
            </div>
            <a href="?page=1">1</a><a href="?page=2">2</a><a href="?page=3">3</a>
        """, "https://mixkit.co/free-stock-video/")
        val page = MixkitCatalog.parseListing(doc, true)
        assertTrue(page.next!!.endsWith("page=2"))
        assertEquals("https://assets.mixkit.co/ocean.jpg", page.items.single().thumbnail)
    }

    @Test fun publishedResolutionsAreKeptSeparate() {
        val doc = Jsoup.parse("""
            <div><a data-license="videoFree">Free</a>
            <input name="download-option" data-label="HD" value="/free-stock-video/download/42/?type=720p">
            <input name="download-option" data-label="Full HD" value="/free-stock-video/download/42/?type=1080p">
            </div>
        """, "https://mixkit.co/free-stock-video/example-42/")
        val offer = MixkitCatalog.parseOffer(doc)
        assertEquals(2, offer.choices.size)
        assertTrue(offer.choices[0].label.contains("720p"))
        assertTrue(offer.choices[1].address.endsWith("type=1080p"))
        assertTrue(offer.choices.all { it.modal })
    }
    @Test fun restrictedClipIsExplicitlyLabelled() {
        val doc = Jsoup.parse("""
            <a data-license="videoRestricted">Restricted</a>
            <button data-download--button-modal-url-value="/free-stock-video/download/42/">Download</button>
        """, "https://mixkit.co/free-stock-video/example-42/")
        assertTrue(MixkitCatalog.parseOffer(doc).license.contains("personal use only"))
    }
    @Test(expected = IllegalArgumentException::class) fun noGuessedResolutionWhenOptionsAreMissing() {
        MixkitCatalog.parseOffer(Jsoup.parse("<a data-license=videoFree>Free</a>", "https://mixkit.co/"))
    }
}
