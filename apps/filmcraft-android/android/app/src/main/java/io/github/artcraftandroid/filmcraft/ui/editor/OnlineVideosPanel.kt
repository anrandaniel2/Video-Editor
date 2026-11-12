package io.github.artcraftandroid.filmcraft.ui.editor

import android.net.Uri
import android.widget.VideoView
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import io.github.artcraftandroid.filmcraft.ui.ImmersiveDialog
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay

@Composable
internal fun OnlineVideosPanel(ed: EditorController, close: () -> Unit) {
    var query by remember { mutableStateOf("") }
    var offset by remember { mutableIntStateOf(0) }
    var retry by remember { mutableIntStateOf(0) }
    var results by remember { mutableStateOf<List<OnlineSound>>(emptyList()) }
    var next by remember { mutableStateOf<Int?>(null) }
    var loading by remember { mutableStateOf(true) }
    var error by remember { mutableStateOf<String?>(null) }
    var preview by remember { mutableStateOf<Long?>(null) }
    var previewError by remember { mutableStateOf<String?>(null) }
    var overlay by remember { mutableStateOf(false) }
    val links = LocalUriHandler.current
    LaunchedEffect(Unit) { ed.stop() }
    LaunchedEffect(query, offset, retry) {
        loading = true; error = null; preview = null; previewError = null
        if (offset == 0) { results = emptyList(); next = null; delay(350) }
        try {
            val page = CommonsSounds.search(query, offset, video = true)
            results = (if (offset == 0) page.sounds else results + page.sounds).distinctBy { it.id }
            next = page.next
        } catch (e: CancellationException) { throw e }
        catch (e: Exception) { error = e.message ?: "Could not load videos. Check your connection." }
        finally { loading = false }
    }
    Dialog(onDismissRequest = close, properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false)) {
        ImmersiveDialog()
        Surface(Modifier.fillMaxSize()) {
            Column(Modifier.padding(16.dp)) {
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                    Text("Online videos", style = MaterialTheme.typography.titleLarge)
                    TextButton(onClick = close) { Text("Close") }
                }
                OutlinedTextField(query, { query = it; offset = 0 }, label = { Text("Search videos") }, placeholder = { Text("Nature, ocean, city…") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                Row { listOf("Nature", "Ocean", "City", "Space").forEach { term -> TextButton(onClick = { query = term; offset = 0 }) { Text(term) } } }
                Row { Checkbox(overlay, { overlay = it }); Text("Import as overlay", modifier = Modifier.padding(top = 12.dp)) }
                Text("Wikimedia Commons · up to 250 MB per video. Check the source license; credits are saved with imports.", style = MaterialTheme.typography.bodySmall)
                ed.busy?.let { Text(it); LinearProgressIndicator(Modifier.fillMaxWidth()) }
                previewError?.let { Text(it, color = MaterialTheme.colorScheme.error) }
                LazyColumn(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    items(results, key = { it.id }) { video ->
                        OutlinedCard(Modifier.fillMaxWidth()) {
                            Column(Modifier.padding(12.dp)) {
                                Text(video.title, style = MaterialTheme.typography.titleSmall)
                                Text(video.artist.take(160), maxLines = 2, style = MaterialTheme.typography.bodySmall)
                                Text("${video.license} · ${video.bytes / (1024 * 1024)} MB", style = MaterialTheme.typography.bodySmall)
                                if (preview == video.id) AndroidView(factory = { context -> VideoView(context).apply {
                                    setOnPreparedListener { it.isLooping = true; start() }
                                    setOnErrorListener { _, _, _ -> previewError = "This device cannot stream this preview. You can still try importing it."; preview = null; true }
                                    setVideoURI(Uri.parse(video.url))
                                } }, modifier = Modifier.fillMaxWidth().height(180.dp), onReset = null, onRelease = { it.stopPlayback() })
                                Row {
                                    TextButton(onClick = { previewError = null; preview = if (preview == video.id) null else video.id }) { Text(if (preview == video.id) "Stop" else "Preview") }
                                    TextButton(onClick = { preview = null; ed.importOnlineVideo(video.url, video.credits, overlay) }, enabled = ed.busy == null) { Text("Import") }
                                    TextButton(onClick = { runCatching { links.openUri(video.page) } }) { Text("Source / license") }
                                }
                            }
                        }
                    }
                    if (loading) item { LinearProgressIndicator(Modifier.fillMaxWidth()) }
                    if (error != null && !loading) item { Text(error!!, color = MaterialTheme.colorScheme.error); TextButton(onClick = { retry++ }) { Text("Retry") } }
                    if (!loading && error == null && results.isEmpty()) item { Text("No supported videos found. Try another search or load more results.") }
                    if (!loading && error == null && next != null) item { TextButton(onClick = { next?.let { offset = it } }) { Text("Load more videos") } }
                }
            }
        }
    }
}
