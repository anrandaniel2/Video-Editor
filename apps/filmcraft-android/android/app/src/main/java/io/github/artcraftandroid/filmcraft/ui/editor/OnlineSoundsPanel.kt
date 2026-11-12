package io.github.artcraftandroid.filmcraft.ui.editor

import io.github.artcraftandroid.filmcraft.ui.*
import android.media.MediaPlayer
import android.text.Html
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URI
import java.net.URLEncoder

internal const val ONLINE_MEDIA_AGENT = "FilmCraft/0.0.28 (https://github.com/anrandaniel2/Video-Editor; Android media editor)"
internal val onlineMediaHeaders: Map<String, String> get() = mapOf("User-Agent" to ONLINE_MEDIA_AGENT)

internal data class OnlineSound(val id: Long, val title: String, val url: String, val page: String, val artist: String, val license: String, val licenseUrl: String, val bytes: Long) {
    val credits: String get() = "$title\nCreator: $artist\nLicense: $license\n$licenseUrl\nSource: $page\nMedia: $url\n"
}
internal data class SoundPage(val sounds: List<OnlineSound>, val next: Int?)

/** Public catalog: no embedded secret, scraping, or proxy service required. */
internal object CommonsSounds {
    private fun plain(value: String) = Html.fromHtml(value, Html.FROM_HTML_MODE_LEGACY).toString().trim()
    fun parse(json: JSONObject, video: Boolean = false): SoundPage {
        json.optJSONObject("error")?.let { error(it.optString("info", "The sound service returned an error.")) }
        val pages = json.optJSONObject("query")?.optJSONArray("pages")
        val sounds = buildList {
            if (pages != null) for (i in 0 until pages.length()) {
                val page = pages.getJSONObject(i)
                val info = page.optJSONArray("imageinfo")?.optJSONObject(0) ?: continue
                val url = info.optString("url")
                val mime = info.optString("mime")
                val ext = runCatching { URI(url).path.substringAfterLast('.').lowercase() }.getOrDefault("")
                val extensions = if (video) listOf("webm", "mp4", "mov", "m4v") else listOf("wav", "mp3", "ogg", "flac", "m4a", "aac", "opus")
                if (!url.startsWith("https://") || !mime.startsWith(if (video) "video/" else "audio/") || ext !in extensions) continue
                val meta = info.optJSONObject("extmetadata") ?: continue
                fun field(key: String) = plain(meta.optJSONObject(key)?.optString("value").orEmpty())
                val license = field("LicenseShortName")
                // Do not offer media without visible rights information or files above the importer limit.
                val bytes = info.optLong("size")
                if (license.isBlank() || bytes <= 0 || bytes > (if (video) 250L else 100L) * 1024 * 1024) continue
                add(OnlineSound(page.getLong("pageid"), page.optString("title").removePrefix("File:").substringBeforeLast('.'), url,
                    info.optString("descriptionurl"), field("Artist").ifBlank { "See source page" }, license, field("LicenseUrl"), bytes))
            }
        }
        return SoundPage(sounds, json.optJSONObject("continue")?.let { if (it.has("gsroffset")) it.getInt("gsroffset") else null })
    }
    private fun request(address: String): JSONObject {
        val connection = URI(address).toURL().openConnection() as HttpURLConnection
        connection.connectTimeout = 15000; connection.readTimeout = 20000
        connection.setRequestProperty("User-Agent", ONLINE_MEDIA_AGENT)
        connection.setRequestProperty("Accept", "application/json")
        try {
            require(connection.responseCode == 200) { "Internet Archive unavailable (HTTP ${connection.responseCode}). Try again shortly." }
            val output = java.io.ByteArrayOutputStream()
            connection.inputStream.use { input ->
                val buffer = ByteArray(8192)
                while (true) {
                    val n = input.read(buffer)
                    if (n < 0) break
                    require(output.size() + n <= 4 * 1024 * 1024) { "Catalog response was too large." }
                    output.write(buffer, 0, n)
                }
            }
            return JSONObject(output.toString("UTF-8"))
        } finally { connection.disconnect() }
    }
    suspend fun search(query: String, offset: Int = 0, video: Boolean = false): SoundPage = withContext(Dispatchers.IO) {
        val term = query.trim().ifBlank { if (video) "nature" else "sound" }
        // Treat user input as text, not as Archive query operators.
        val words = term.replace(Regex("[^\\p{L}\\p{N} ]"), " ").trim().ifBlank { "nature" }
        val q = URLEncoder.encode("mediatype:${if (video) "movies" else "audio"} AND ($words) AND licenseurl:*", "UTF-8")
        val pageNumber = offset / 8 + 1
        val response = request("https://archive.org/advancedsearch.php?q=$q&fl[]=identifier&fl[]=title&rows=8&page=$pageNumber&output=json").getJSONObject("response")
        val docs = response.getJSONArray("docs")
        val results = mutableListOf<OnlineSound>()
        for (i in 0 until docs.length()) {
            kotlinx.coroutines.currentCoroutineContext().ensureActive()
            val doc = docs.getJSONObject(i)
            val id = doc.getString("identifier")
            val encodedId = URLEncoder.encode(id, "UTF-8").replace("+", "%20")
            val item = request("https://archive.org/metadata/$encodedId")
            if (item.optBoolean("is_dark") || item.optJSONObject("metadata")?.optString("access-restricted-item") == "true") continue
            val meta = item.optJSONObject("metadata") ?: continue
            val licenseUrl = meta.opt("licenseurl").let { if (it is org.json.JSONArray) it.optString(0) else it?.toString().orEmpty() }
            if (!licenseUrl.startsWith("https://") && !licenseUrl.startsWith("http://")) continue
            val files = item.optJSONArray("files") ?: continue
            val candidates = (0 until files.length()).map { files.getJSONObject(it) }.filter {
                val name = it.optString("name").lowercase()
                val bytes = it.optString("size").toLongOrNull() ?: 0L
                bytes in 1..(if (video) 250L else 100L) * 1024 * 1024 &&
                    (if (video) name.endsWith(".mp4") else name.endsWith(".mp3")) && it.optString("private") != "true"
            }
            val file = candidates.minByOrNull { it.optString("size").toLong() } ?: continue
            val name = file.getString("name").split('/').joinToString("/") { URLEncoder.encode(it, "UTF-8").replace("+", "%20") }
            val url = "https://archive.org/download/$encodedId/$name"
            results.add(OnlineSound(id.fold(0L) { acc, c -> acc * 31 + c.code }, doc.optString("title", id), url,
                "https://archive.org/details/$encodedId", plain(meta.optString("creator", "See source page")), "See source license", licenseUrl, file.getString("size").toLong()))
        }
        SoundPage(results, if (offset + docs.length() < response.optInt("numFound")) offset + 8 else null)
    }
}

@Composable
internal fun OnlineSoundsPanel(ed: EditorController, close: () -> Unit) {
    var query by remember { mutableStateOf("") }
    var results by remember { mutableStateOf<List<OnlineSound>>(emptyList()) }
    var next by remember { mutableStateOf<Int?>(null) }
    var loading by remember { mutableStateOf(true) }
    var error by remember { mutableStateOf<String?>(null) }
    var retry by remember { mutableIntStateOf(0) }
    var playing by remember { mutableStateOf<Long?>(null) }
    var preparing by remember { mutableStateOf(false) }
    var previewError by remember { mutableStateOf<String?>(null) }
    val player = remember { MediaPlayer() }
    val scope = rememberCoroutineScope()
    var pageJob by remember { mutableStateOf<kotlinx.coroutines.Job?>(null) }
    val uriHandler = LocalUriHandler.current
    val context = LocalContext.current
    fun stopPreview() { player.reset(); playing = null; preparing = false }
    DisposableEffect(player) { onDispose { player.release() } }
    LaunchedEffect(query, retry) {
        pageJob?.cancel(); stopPreview(); results = emptyList(); next = null; loading = true; error = null
        delay(350)
        try { val page = CommonsSounds.search(query); results = page.sounds; next = page.next }
        catch (e: CancellationException) { throw e }
        catch (e: Exception) { error = e.message ?: "Could not load sounds. Check your connection." }
        finally { loading = false }
    }
    Dialog(onDismissRequest = close, properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false)) {
        ImmersiveDialog()
        Surface(Modifier.fillMaxSize().systemBarsPadding()) {
            Column(Modifier.fillMaxSize().padding(16.dp)) {
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                    Text("Online sounds", style = MaterialTheme.typography.titleLarge)
                    FcIconButton(LucideIcons.X.glyph(), "Close", close)
                }
                OutlinedTextField(query, { query = it }, label = { Text("Search sounds") }, placeholder = { Text("Rain, footsteps, birds…") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceEvenly) {
                    listOf("Rain", "Nature", "Footsteps", "Whoosh").forEach { term -> TextButton(onClick = { query = term }) { Text(term) } }
                }
                Text("Internet Archive • Check the license for your intended use. Credits are saved with imports.", style = MaterialTheme.typography.bodySmall)
                ed.busy?.let { Text(it); LinearProgressIndicator(Modifier.fillMaxWidth()) }
                ed.onlineImportError?.let { Text(it, color = MaterialTheme.colorScheme.error) }
                previewError?.let { Text(it, color = MaterialTheme.colorScheme.error) }
                LazyColumn(Modifier.weight(1f).fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    items(results, key = { it.id }) { sound ->
                        OutlinedCard(Modifier.fillMaxWidth()) {
                            Column(Modifier.padding(12.dp)) {
                                Text(sound.title, style = MaterialTheme.typography.titleSmall)
                                Text(sound.artist.take(160), style = MaterialTheme.typography.bodySmall, maxLines = 2)
                                Text("${sound.license} • ${sound.bytes / 1024} KB", style = MaterialTheme.typography.bodySmall)
                                Row {
                                    TextButton(onClick = {
                                        previewError = null
                                        if (playing == sound.id) stopPreview() else {
                                            stopPreview(); playing = sound.id; preparing = true
                                            try {
                                                player.setOnPreparedListener { preparing = false; it.start() }
                                                player.setOnCompletionListener { playing = null; preparing = false }
                                                player.setOnErrorListener { _, what, extra -> previewError = "Audio preview failed (player $what/$extra). This may be a network or file-format error, not a device streaming limitation."; stopPreview(); true }
                                                player.setDataSource(context, android.net.Uri.parse(java.net.URI(sound.url).toASCIIString()), onlineMediaHeaders); player.prepareAsync()
                                            } catch (_: Exception) { previewError = "Could not play this preview."; stopPreview() }
                                        }
                                    }) { Text(if (playing == sound.id) { if (preparing) "Cancel preview" else "Stop" } else "Preview") }
                                    TextButton(onClick = { stopPreview(); ed.importOnlineSound(sound.url, sound.credits) }, enabled = ed.busy == null) { Text("Import") }
                                    TextButton(onClick = { runCatching { uriHandler.openUri(sound.page) } }) { Text("Source / license") }
                                }
                            }
                        }
                    }
                    if (loading) item { LinearProgressIndicator(Modifier.fillMaxWidth().padding(vertical = 12.dp)) }
                    if (!loading && error != null) item {
                        Text(error!!, color = MaterialTheme.colorScheme.error)
                        TextButton(onClick = { retry++ }) { Text("Retry") }
                    }
                    if (!loading && error == null && results.isEmpty()) item { Text("No sounds found. Try another search or browse more results.") }
                    if (!loading && error == null && next != null) item {
                        TextButton(onClick = {
                            val offset = next ?: return@TextButton
                            val requestedQuery = query
                            loading = true
                            pageJob = scope.launch {
                                try {
                                    val page = CommonsSounds.search(requestedQuery, offset)
                                    if (query == requestedQuery) { results = (results + page.sounds).distinctBy { it.id }; next = page.next }
                                } catch (e: CancellationException) { throw e }
                                catch (e: Exception) { if (query == requestedQuery) error = e.message ?: "Could not load more sounds." }
                                finally { if (query == requestedQuery) loading = false }
                            }
                        }) { Text("Load more sounds") }
                    }
                }
            }
        }
    }
}
