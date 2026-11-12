package io.github.artcraftandroid.filmcraft.ui.editor

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.jsoup.Jsoup
import org.jsoup.nodes.Document
import org.jsoup.nodes.Element
import org.json.JSONArray
import org.json.JSONObject
import java.net.URI
import java.net.HttpURLConnection

internal data class MixkitAsset(val id: String, val title: String, val page: String, val preview: String, val download: String = "", val thumbnail: String = "", val provider: String = "Mixkit")
internal data class MixkitPage(val items: List<MixkitAsset>, val next: String?)
internal data class MixkitResolution(val label: String, val address: String, val modal: Boolean)
internal data class MixkitOffer(val license: String, val choices: List<MixkitResolution>, val licenseUrl: String = "https://mixkit.co/license/")

/** Read only published links. No guessed CDN filenames, automatic crawling, or bulk downloads. */
internal object MixkitCatalog {
    private fun allowed(url: String): Boolean = runCatching {
        val uri = URI(url)
        uri.scheme == "https" && (uri.host == "mixkit.co" || uri.host.endsWith(".mixkit.co"))
    }.getOrDefault(false)
    private fun absolute(base: String, value: String): String = runCatching { URI(base).resolve(value).toASCIIString() }.getOrDefault("")
    private fun media(url: String, video: Boolean): Boolean = allowed(url) && runCatching {
        URI(url).path.lowercase().let { path -> if (video) path.endsWith(".mp4") else listOf(".mp3", ".wav", ".ogg").any { path.endsWith(it) } }
    }.getOrDefault(false)
    private fun fetch(url: String): Document {
        require(allowed(url)) { "Mixkit returned an invalid link." }
        var address = url
        repeat(5) {
            val c = URI(address).toURL().openConnection() as HttpURLConnection
            c.connectTimeout = 15000; c.readTimeout = 20000; c.instanceFollowRedirects = false
            c.setRequestProperty("User-Agent", ONLINE_MEDIA_AGENT)
            c.setRequestProperty("Referer", "https://mixkit.co/")
            try {
                if (c.responseCode in 300..399) {
                    address = absolute(address, c.getHeaderField("Location") ?: error("Missing redirect"))
                    require(allowed(address)) { "Unexpected provider redirect." }
                } else {
                    require(c.responseCode == 200) { "Mixkit request failed (HTTP ${c.responseCode}). Try again shortly." }
                    val output = java.io.ByteArrayOutputStream()
                    c.inputStream.use { input ->
                        val buffer = ByteArray(8192)
                        while (true) {
                            val n = input.read(buffer); if (n < 0) break
                            require(output.size() + n <= 4 * 1024 * 1024) { "Mixkit response was too large." }
                            output.write(buffer, 0, n)
                        }
                    }
                    return Jsoup.parse(output.toString("UTF-8"), address)
                }
            } finally { c.disconnect() }
        }
        error("Too many provider redirects.")
    }
    fun listingUrl(video: Boolean, query: String): String {
        val slug = query.trim().lowercase().replace(Regex("[^a-z0-9]+"), "-").trim('-')
        return "https://mixkit.co/${if (video) "free-stock-video" else "free-sound-effects"}/" + if (slug.isBlank()) "" else "$slug/"
    }
    suspend fun search(url: String, video: Boolean): MixkitPage = withContext(Dispatchers.IO) { parseListing(fetch(url), video) }
    internal fun parseListing(doc: Document, video: Boolean): MixkitPage {
        val items = linkedMapOf<String, MixkitAsset>()
        val cards = doc.select(".item-grid-card, .item-grid__item, .item-grid-item, .audio-player, [data-controller*=audio-player]").toMutableList()
        if (!video) doc.select("audio, [data-download--button-modal-url-value]").forEach { element ->
            var parent: Element? = element
            repeat(6) {
                val node = parent
                if (node != null && node.selectFirst("h2, h3") != null && node.tagName() !in listOf("body", "html", "main")) {
                    if (node !in cards) cards.add(node)
                    return@forEach
                }
                parent = node?.parent()
            }
        }
        for (card in cards) {
            val page = card.select("a[href]").map { it.absUrl("href") }.firstOrNull {
                allowed(it) && it.contains("/free-stock-video/") && Regex("-\\d+/?$").containsMatchIn(it)
            }.orEmpty()
            fun urls(e: Element): List<String> = e.getAllElements().flatMap { node -> node.attributes().map { absolute(doc.baseUri(), it.value) } }
            val links = urls(card).filter { media(it, video) }.distinct()
            val preview = links.firstOrNull { it.contains("preview") } ?: links.firstOrNull().orEmpty()
            val modal = card.getAllElements().firstNotNullOfOrNull { e -> e.attr("data-download--button-modal-url-value").takeIf { it.isNotBlank() } }
            val download = modal?.let { absolute(doc.baseUri(), it) }.orEmpty().ifBlank { links.firstOrNull { it.contains("download") }.orEmpty() }
            val title = card.selectFirst("h2, h3, .item-grid-card__title, [class*=title]")?.text().orEmpty()
            if (title.isNotBlank() && (page.isNotBlank() || preview.isNotBlank())) {
                val id = page.ifBlank { preview }
                val poster = card.selectFirst("video[poster]")?.absUrl("poster").orEmpty()
                val image = card.selectFirst("img[src], img[data-src]")
                val thumbnail = poster.ifBlank { image?.absUrl("data-src").orEmpty().ifBlank { image?.absUrl("src").orEmpty() } }.takeIf { allowed(it) }.orEmpty()
                items[id] = MixkitAsset(id, title, page.ifBlank { doc.baseUri() }, preview, download, thumbnail)
            }
        }
        // Schema.org is also published by Mixkit and survives many visual markup changes.
        fun walk(value: Any?) {
            when (value) {
                is JSONArray -> for (i in 0 until value.length()) walk(value.opt(i))
                is JSONObject -> {
                    val type = value.optString("@type")
                    if (type == if (video) "VideoObject" else "AudioObject") {
                        val content = absolute(doc.baseUri(), value.optString("contentUrl"))
                        val name = value.optString("name")
                        val page = absolute(doc.baseUri(), value.optString("url", doc.baseUri()))
                        if (name.isNotBlank() && media(content, video) && items.values.none { it.preview == content || it.page == page }) {
                            val thumb = value.opt("thumbnailUrl").let { if (it is JSONArray) it.optString(0) else it as? String }.orEmpty()
                            items[content] = MixkitAsset(content, name, page, content, if (video) "" else content, absolute(doc.baseUri(), thumb).takeIf { thumb.isNotBlank() && allowed(it) }.orEmpty())
                        }
                    }
                    value.keys().forEach { walk(value.opt(it)) }
                }
            }
        }
        doc.select("script[type=application/ld+json]").forEach { script ->
            runCatching { org.json.JSONTokener(script.data()).nextValue() }.getOrNull()?.let { walk(it) }
        }
        fun pageNumber(url: String) = Regex("[?&]page=(\\d+)").find(url)?.groupValues?.get(1)?.toIntOrNull() ?: 1
        val currentPage = pageNumber(doc.baseUri())
        val numberedNext = doc.select("a[href]").map { it.absUrl("href") }.filter {
            allowed(it) && it.substringBefore('?').trimEnd('/') == doc.baseUri().substringBefore('?').trimEnd('/') && pageNumber(it) > currentPage
        }.minByOrNull { pageNumber(it) }
        val next = doc.selectFirst("a[rel=next], a.pagination__link--next")?.absUrl("href")?.takeIf { allowed(it) }
            ?: doc.select("a.pagination__link").firstOrNull { it.text().lowercase() in listOf("next", "next page", "›", "→") }?.absUrl("href")?.takeIf { allowed(it) } ?: numberedNext
        require(items.isNotEmpty() || doc.text().contains("no results", true)) { "Mixkit did not publish readable results for this category. Try a category such as Nature or Whoosh." }
        return MixkitPage(items.values.toList(), next)
    }
    suspend fun offer(asset: MixkitAsset, video: Boolean): MixkitOffer = withContext(Dispatchers.IO) {
        if (!video) {
            require(asset.download.isNotBlank()) { "No published download link was found for this sound." }
            return@withContext MixkitOffer("Mixkit Sound Effects Free License", listOf(MixkitResolution("Published audio download", asset.download, !media(asset.download, false))))
        }
        parseOffer(fetch(asset.page))
    }
    internal fun parseOffer(doc: Document): MixkitOffer {
        val license = when {
            doc.select("[data-license=videoRestricted], a[href*=videoRestricted]").isNotEmpty() -> "Mixkit Restricted License — personal use only"
            doc.select("[data-license=videoFree], a[href*=videoFree]").isNotEmpty() -> "Mixkit Stock Video Free License"
            else -> error("The clip did not publish a recognized license. Download blocked; choose another clip.")
        }
        val options = doc.select("input[name=download-option]").mapNotNull { input ->
            val url = absolute(doc.baseUri(), input.attr("value"))
            if (!allowed(url)) return@mapNotNull null
            val resolution = Regex("[?&]type=([^&]+)").find(url)?.groupValues?.get(1)
            val label = listOfNotNull(resolution, input.attr("data-label").takeIf { it.isNotBlank() }).joinToString(" · ").ifBlank { "Published original" }
            MixkitResolution(label, url, true)
        }.ifEmpty {
            doc.select("[data-download--button-modal-url-value]").mapNotNull { button ->
                val url = absolute(doc.baseUri(), button.attr("data-download--button-modal-url-value"))
                if (!allowed(url)) null else MixkitResolution(doc.selectFirst("[class*=download-title]")?.text()?.takeIf { it.isNotBlank() } ?: "Published download", url, true)
            }
        }.distinctBy { it.address }
        require(options.isNotEmpty()) { "No published resolution options found. No video has been downloaded." }
        return MixkitOffer(license, options)
    }
    suspend fun resolve(choice: MixkitResolution, video: Boolean): String = withContext(Dispatchers.IO) {
        if (!choice.modal) { require(media(choice.address, video)); return@withContext choice.address }
        val doc = fetch(choice.address)
        val urls = doc.select("[data-download--modal-url-value]").map { absolute(doc.baseUri(), it.attr("data-download--modal-url-value")) }
        urls.firstOrNull { media(it, video) } ?: error("Mixkit did not return a media file for the selected option. Nothing was downloaded.")
    }
}
