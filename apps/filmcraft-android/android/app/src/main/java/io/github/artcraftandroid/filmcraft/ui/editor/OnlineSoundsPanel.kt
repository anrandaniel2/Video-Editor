package io.github.artcraftandroid.filmcraft.ui.editor

import android.media.MediaPlayer
import android.text.Html
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URI
import java.net.URLEncoder

internal data class OnlineSound(val id: Long, val title: String, val url: String, val page: String, val artist: String, val license: String, val licenseUrl: String, val bytes: Long) {
    val credits: String get() = "$title\nCreator: $artist\nLicense: $license\n$licenseUrl\nSource: $page\nAudio: $url\n"
}
internal data class SoundPage(val sounds: List<OnlineSound>, val next: Int?)

/** Public catalog: no embedded secret, scraping, or proxy service required. */
internal object CommonsSounds {
    private fun plain(value: String) = Html.fromHtml(value, Html.FROM_HTML_MODE_LEGACY).toString().trim()
    fun parse(json: JSONObject): SoundPage {
        json.optJSONObject("error")?.let { error(it.optString("info", "The sound service returned an error.")) }
        val pages = json.optJSONObject("query")?.optJSONArray("pages")
        val sounds = buildList {
            if (pages != null) for (i in 0 until pages.length()) {
                val page = pages.getJSONObject(i)
                val info = page.optJSONArray("imageinfo")?.optJSONObject(0) ?: continue
                val url = info.optString("url")
                val mime = info.optString("mime")
                val ext = runCatching { URI(url).path.substringAfterLast('.').lowercase() }.getOrDefault("")
                if (!url.startsWith("https://") || !mime.startsWith("audio/") || ext !in listOf("wav", "mp3", "ogg", "flac", "m4a", "aac", "opus")) continue
                val meta = info.optJSONObject("extmetadata") ?: continue
                fun field(key: String) = plain(meta.optJSONObject(key)?.optString("value").orEmpty())
                val license = field("LicenseShortName")
                // Do not offer media without visible rights information or files above the importer limit.
                val bytes = info.optLong("size")
                if (license.isBlank() || bytes <= 0 || bytes > 100L * 1024 * 1024) continue
                add(OnlineSound(page.getLong("pageid"), page.optString("title").removePrefix("File:").substringBeforeLast('.'), url,
                    info.optString("descriptionurl"), field("Artist").ifBlank { "See source page" }, license, field("LicenseUrl"), bytes))
            }
        }
        return SoundPage(sounds, json.optJSONObject("continue")?.let { if (it.has("gsroffset")) it.getInt("gsroffset") else null })
    }
    suspend fun search(query: String, offset: Int = 0): SoundPage = withContext(Dispatchers.IO) {
        val term = query.trim().ifBlank { "sound effect" }
        val encoded = URLEncoder.encode("filetype:audio $term", "UTF-8")
        val url = "https://commons.wikimedia.org/w/api.php?action=query&format=json&formatversion=2&generator=search&gsrnamespace=6&gsrlimit=20&gsroffset=$offset&gsrsearch=$encoded&prop=imageinfo&iiprop=url%7Cmime%7Csize%7Cextmetadata&iiextmetadatafilter=Artist%7CLicenseShortName%7CLicenseUrl&maxlag=5"
        val connection = URI(url).toURL().openConnection() as HttpURLConnection
        connection.connectTimeout = 15000; connection.readTimeout = 20000
        connection.setRequestProperty("User-Agent", "FilmCraft-Android/0.0.16 (https://github.com/anrandaniel2/Video-Editor)")
        try {
            require(connection.responseCode == 200) { "Sound service unavailable (HTTP ${connection.responseCode}). Try again." }
            val body = connection.inputStream.use { input ->
                val output = java.io.ByteArrayOutputStream()
                val buffer = ByteArray(8192)
                while (true) {
                    val count = input.read(buffer)
                    if (count < 0) break
                    require(output.size() + count <= 2 * 1024 * 1024) { "Sound service response was too large." }
                    output.write(buffer, 0, count)
                }
                output.toByteArray()
            }
            require(body.size <= 2 * 1024 * 1024) { "Sound service response was too large." }
            parse(JSONObject(String(body, Charsets.UTF_8)))
        } finally { connection.disconnect() }
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
    Dialog(onDismissRequest = close, properties = DialogProperties(usePlatformDefaultWidth = false)) {
        Surface(Modifier.fillMaxSize().systemBarsPadding()) {
            Column(Modifier.fillMaxSize().padding(16.dp)) {
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                    Text("Online sounds", style = MaterialTheme.typography.titleLarge)
                    TextButton(onClick = close) { Text("Close") }
                }
                OutlinedTextField(query, { query = it }, label = { Text("Search sounds") }, placeholder = { Text("Rain, footsteps, birds…") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceEvenly) {
                    listOf("Rain", "Nature", "Footsteps", "Whoosh").forEach { term -> TextButton(onClick = { query = term }) { Text(term) } }
                }
                Text("Wikimedia Commons • Check the license for your intended use. Credits are saved with imports.", style = MaterialTheme.typography.bodySmall)
                ed.busy?.let { Text(it); LinearProgressIndicator(Modifier.fillMaxWidth()) }
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
                                                player.setOnErrorListener { _, _, _ -> previewError = "Preview unavailable. You can still try importing."; stopPreview(); true }
                                                player.setDataSource(sound.url); player.prepareAsync()
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
