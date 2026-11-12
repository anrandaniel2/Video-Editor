package io.github.artcraftandroid.filmcraft.ui.editor

import kotlinx.coroutines.*
import kotlinx.coroutines.sync.Semaphore
import kotlinx.coroutines.sync.withPermit
import org.json.JSONArray
import org.jsoup.Jsoup
import org.jsoup.nodes.Document
import java.net.HttpURLConnection
import java.net.URI
import java.net.URLEncoder

/** Multiple providers behind the same cards, pagination and import UI; no embedded API keys. */
internal object OnlineMediaCatalog {
    private val slots = Semaphore(3)
    private val knownHosts = setOf("vidsplay.com", "soundbible.com", "vimeo.com", "vimeocdn.com")
    private fun trusted(address: String): Boolean = runCatching {
        val u = URI(address)
        u.scheme == "https" && knownHosts.any { u.host == it || u.host.endsWith(".$it") }
    }.getOrDefault(false)
    private fun link(base: String, address: String): String = runCatching {
        val u = URI(base).resolve(address)
        // SoundBible publishes some download links using HTTP; use the same link over TLS.
        val https = if (u.scheme == "http") "https:" + u.toString().removePrefix("http:") else u.toString()
        URI(https).toASCIIString().takeIf { trusted(it) }.orEmpty()
    }.getOrDefault("")
    private fun document(url: String): Document {
        var address = url
        repeat(5) {
            require(trusted(address)) { "Unrecognized media provider link" }
            val c = URI(address).toURL().openConnection() as HttpURLConnection
            c.connectTimeout = 15000; c.readTimeout = 20000; c.instanceFollowRedirects = false
            c.setRequestProperty("User-Agent", ONLINE_MEDIA_AGENT)
            try {
                if (c.responseCode in 300..399) address = link(address, c.getHeaderField("Location") ?: error("Missing redirect"))
                else {
                    require(c.responseCode == 200) { "${URI(address).host} returned HTTP ${c.responseCode}" }
                    val out = java.io.ByteArrayOutputStream()
                    c.inputStream.use { input ->
                        val bytes = ByteArray(8192)
                        while (true) { val n = input.read(bytes); if (n < 0) break; require(out.size() + n <= 4 * 1024 * 1024); out.write(bytes, 0, n) }
                    }
                    return Jsoup.parse(out.toString("UTF-8"), address)
                }
            } finally { c.disconnect() }
        }
        error("Too many provider redirects")
    }
    fun listingUrl(video: Boolean, query: String): String {
        val q = URLEncoder.encode(query.trim(), "UTF-8")
        val other = if (video) "https://www.vidsplay.com/" + if (q.isBlank()) "" else "?s=$q"
            else "https://soundbible.com/" + if (q.isBlank()) "" else "tags-${query.trim().lowercase().replace(Regex("[^a-z0-9]+"), "-").trim('-')}.html"
        return JSONArray().put(MixkitCatalog.listingUrl(video, query)).put(other).toString()
    }
    suspend fun search(cursor: String, video: Boolean): MixkitPage = coroutineScope {
        val urls = JSONArray(cursor).let { a -> (0 until a.length()).map { a.getString(it) } }
        val pages = urls.map { url -> async(Dispatchers.IO) {
            try { Result.success(if (URI(url).host.endsWith("mixkit.co")) MixkitCatalog.search(url, video) else otherPage(url, video)) }
            catch (e: CancellationException) { throw e }
            catch (e: Exception) { Result.failure<MixkitPage>(e) }
        } }.awaitAll()
        val good = pages.mapNotNull { it.getOrNull() }
        if (good.isEmpty()) throw (pages.firstNotNullOfOrNull { it.exceptionOrNull() } ?: IllegalStateException("No providers available"))
        val next = good.mapNotNull { it.next }.distinct()
        // Interleave so the additional provider isn't hidden behind the first provider's page.
        val items = (0 until (good.maxOfOrNull { it.items.size } ?: 0)).flatMap { i -> good.mapNotNull { it.items.getOrNull(i) } }.distinctBy { it.id }
        MixkitPage(items, if (next.isEmpty()) null else JSONArray(next).toString())
    }
    private suspend fun otherPage(url: String, video: Boolean): MixkitPage = coroutineScope {
        val doc = document(url)
        val candidates = if (video) doc.select("a[title][href]").mapNotNull { a ->
            val page = link(url, a.attr("href")); val title = a.attr("title")
            if (page.isBlank() || !URI(page).host.endsWith("vidsplay.com") || title.isBlank() || a.select("img").isEmpty()) null
            else {
                var parent = a.parent(); var poster = ""
                repeat(3) { if (poster.isBlank()) poster = parent?.select("img[src*=/wp-content/uploads/]")?.firstOrNull()?.let { link(url, it.attr("src")) }.orEmpty(); parent = parent?.parent() }
                MixkitAsset(page, title, page, "", thumbnail = poster, provider = "Vidsplay")
            }
        }.distinctBy { it.id }.take(16) else doc.select("h3 a[href], h2 a[href]").mapNotNull { a ->
            val page = link(url, a.attr("href"))
            if (page.isBlank() || !Regex("/\\d+-[^/]+\\.html$").containsMatchIn(page)) null
            else MixkitAsset(page, a.text(), page, "", provider = "SoundBible")
        }.distinctBy { it.id }.take(12)
        // Sound waveforms need published preview URLs. Limit simultaneous metadata requests.
        val items = if (video) candidates else candidates.map { asset -> async {
            slots.withPermit {
                try { asset.copy(preview = soundFile(document(asset.page), true)) }
                catch (e: CancellationException) { throw e }
                catch (_: Exception) { asset }
            }
        } }.awaitAll()
        val current = Regex("/page/(\\d+)/|[?&]page=(\\d+)").find(url)?.groupValues?.drop(1)?.firstOrNull { it.isNotBlank() }?.toIntOrNull() ?: 1
        val next = doc.select("a[rel=next], a.next, a.nextpostslink").map { link(url, it.attr("href")) }.firstOrNull { it.isNotBlank() }
            ?: doc.select("a[href]").firstNotNullOfOrNull { a ->
                val number = a.text().trim().toIntOrNull()
                if (number == current + 1) link(url, a.attr("href")).takeIf { it.isNotBlank() } else null
            }
        MixkitPage(items, next)
    }
    private fun soundFile(doc: Document, preview: Boolean): String {
        val media = doc.select("audio[src], audio source[src]").map { link(doc.baseUri(), it.attr("src")) }.firstOrNull { it.isNotBlank() }
        if (preview && media != null) return media
        return doc.select("a[href]").map { link(doc.baseUri(), it.attr("href")) }.firstOrNull {
            it.contains("grab.php") && it.contains(if (preview) "type=mp3" else "type=wav")
        } ?: media ?: error("No published audio download found")
    }
    private fun videoFiles(doc: Document): List<String> = (doc.select("a[href]").map { link(doc.baseUri(), it.attr("href")) } +
        doc.select("video[src], video source[src]").map { link(doc.baseUri(), it.attr("src")) }).filter {
        it.isNotBlank() && URI(it).path.endsWith(".mp4", true)
    }.distinct()
    suspend fun preview(asset: MixkitAsset): String = withContext(Dispatchers.IO) {
        if (asset.preview.isNotBlank()) asset.preview
        else if (asset.provider == "Vidsplay") videoFiles(document(asset.page)).firstOrNull() ?: error("No published preview available")
        else if (asset.provider == "SoundBible") soundFile(document(asset.page), true)
        else error("No preview available")
    }
    suspend fun offer(asset: MixkitAsset, video: Boolean): MixkitOffer = withContext(Dispatchers.IO) {
        when (asset.provider) {
            "Mixkit" -> MixkitCatalog.offer(asset, video)
            "Vidsplay" -> {
                val doc = document(asset.page)
                val resolution = Regex("Resolution\\s*[—–:-]?\\s*(\\d+\\s*[x×]\\s*\\d+)", RegexOption.IGNORE_CASE).find(doc.text())?.groupValues?.get(1)
                val files = videoFiles(doc)
                require(files.isNotEmpty()) { "No published video download found" }
                MixkitOffer("Vidsplay license — attribution to Vidsplay.com required", files.mapIndexed { i, url -> MixkitResolution(resolution ?: "Published original ${i + 1}", url, false) }, "https://www.vidsplay.com/terms/")
            }
            "SoundBible" -> {
                val doc = document(asset.page)
                val license = Regex("License\\s+(Attribution\\s+3\\.0|Public Domain|Attribution\\s+4\\.0)", RegexOption.IGNORE_CASE).find(doc.text())?.groupValues?.get(1)
                    ?: error("No recognized reusable license found. Choose another sound.")
                val author = doc.select("img[alt]").map { it.attr("alt") }.firstOrNull { it.isNotBlank() && !it.contains("sound", true) }.orEmpty()
                MixkitOffer("SoundBible · $license${if (author.isBlank()) " — credit the creator listed on the source page" else " — credit: $author"}", listOf(MixkitResolution("WAV", soundFile(doc, false), false)), asset.page)
            }
            else -> error("Unknown media provider")
        }
    }
    suspend fun resolve(asset: MixkitAsset, choice: MixkitResolution, video: Boolean): String =
        if (asset.provider == "Mixkit") MixkitCatalog.resolve(choice, video) else choice.address.also { require(trusted(it)) }
}
