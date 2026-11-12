package io.github.artcraftandroid.filmcraft.ui.editor

import org.jsoup.Jsoup
import org.junit.Assert.*
import org.junit.Test

class MixkitCatalogTest {
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
