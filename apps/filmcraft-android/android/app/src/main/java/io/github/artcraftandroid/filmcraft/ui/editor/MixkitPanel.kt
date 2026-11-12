package io.github.artcraftandroid.filmcraft.ui.editor

import android.media.MediaPlayer
import android.net.Uri
import android.widget.VideoView
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import io.github.artcraftandroid.filmcraft.ui.ImmersiveDialog
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/** Native catalog UI. Web pages are parsed as data, never rendered in a WebView. */
@Composable
internal fun MixkitPanel(ed: EditorController, video: Boolean, close: () -> Unit) {
    var query by remember { mutableStateOf("") }
    var overlay by remember { mutableStateOf(false) }
    var results by remember { mutableStateOf<List<MixkitAsset>>(emptyList()) }
    var next by remember { mutableStateOf<String?>(null) }
    var loading by remember { mutableStateOf(true) }
    var error by remember { mutableStateOf<String?>(null) }
    var retry by remember { mutableIntStateOf(0) }
    var preview by remember { mutableStateOf<String?>(null) }
    var selected by remember { mutableStateOf<MixkitAsset?>(null) }
    var offer by remember { mutableStateOf<MixkitOffer?>(null) }
    var choice by remember { mutableStateOf<MixkitResolution?>(null) }
    var resolving by remember { mutableStateOf(false) }
    var pageJob by remember { mutableStateOf<kotlinx.coroutines.Job?>(null) }
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    val player = remember { MediaPlayer() }
    fun stopPreview() { player.reset(); preview = null }
    DisposableEffect(player) { onDispose { player.release() } }
    LaunchedEffect(query, retry) {
        pageJob?.cancel(); stopPreview(); results = emptyList(); next = null; loading = true; error = null
        delay(350)
        try {
            val page = MixkitCatalog.search(MixkitCatalog.listingUrl(video, query), video)
            results = page.items; next = page.next
        } catch (e: CancellationException) { throw e }
        catch (e: Exception) { error = e.message ?: "Could not load Mixkit." }
        finally { loading = false }
    }
    LaunchedEffect(selected) {
        offer = null; choice = null
        val asset = selected ?: return@LaunchedEffect
        resolving = true; error = null
        try { offer = MixkitCatalog.offer(asset, video) }
        catch (e: CancellationException) { throw e }
        catch (e: Exception) { error = e.message; selected = null }
        finally { resolving = false }
    }
    Dialog(onDismissRequest = close, properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false)) {
        ImmersiveDialog()
        Surface(Modifier.fillMaxSize()) {
            Column(Modifier.fillMaxSize().padding(16.dp)) {
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                    Text(if (video) "Stock videos" else "Sound effects", style = MaterialTheme.typography.titleLarge)
                    TextButton(onClick = close) { Text("Close") }
                }
                OutlinedTextField(query, { query = it }, singleLine = true, label = { Text("Browse Mixkit category") }, placeholder = { Text(if (video) "Nature, city, aerial…" else "Whoosh, impact, cinematic…") }, modifier = Modifier.fillMaxWidth())
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceEvenly) {
                    (if (video) listOf("All", "Nature", "City", "Aerial") else listOf("All", "Whoosh", "Impact", "Cinematic")).forEach { category ->
                        TextButton(onClick = { query = if (category == "All") "" else category }) { Text(category) }
                    }
                }
                Text("Mixkit · Check the displayed license. Video resolution is chosen before downloading.", style = MaterialTheme.typography.bodySmall)
                if (video) Row { Checkbox(overlay, { overlay = it }); Text("Import as overlay", Modifier.padding(top = 12.dp)) }
                ed.busy?.let { Text(it); LinearProgressIndicator(Modifier.fillMaxWidth()) }
                ed.onlineImportError?.let { Text(it, color = MaterialTheme.colorScheme.error) }
                error?.let { Text(it, color = MaterialTheme.colorScheme.error); TextButton(onClick = { retry++ }) { Text("Retry catalog") } }
                if (resolving) { Text("Loading published download options…"); LinearProgressIndicator(Modifier.fillMaxWidth()) }
                LazyColumn(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    items(results, key = { it.id }) { asset ->
                        OutlinedCard(Modifier.fillMaxWidth()) {
                            Column(Modifier.padding(12.dp)) {
                                Text(asset.title, style = MaterialTheme.typography.titleSmall)
                                if (video && preview == asset.id) AndroidView(factory = { ctx ->
                                    VideoView(ctx).apply {
                                        setOnPreparedListener { it.isLooping = true; start() }
                                        setOnErrorListener { _, what, extra -> error = "Preview failed ($what/$extra). Try another clip."; preview = null; true }
                                        setVideoURI(Uri.parse(asset.preview), onlineMediaHeaders + ("Referer" to "https://mixkit.co/"))
                                    }
                                }, modifier = Modifier.fillMaxWidth().height(180.dp), onReset = null, onRelease = { it.stopPlayback() })
                                Row {
                                    TextButton(onClick = {
                                        if (preview == asset.id) stopPreview() else {
                                            stopPreview(); error = null; preview = asset.id
                                            if (!video) try {
                                                player.setOnPreparedListener { it.start() }
                                                player.setOnCompletionListener { preview = null }
                                                player.setOnErrorListener { _, what, extra -> error = "Preview failed ($what/$extra)."; stopPreview(); true }
                                                player.setDataSource(context, Uri.parse(asset.preview), onlineMediaHeaders + ("Referer" to "https://mixkit.co/")); player.prepareAsync()
                                            } catch (e: Exception) { error = e.message; stopPreview() }
                                        }
                                    }, enabled = asset.preview.isNotBlank()) { Text(if (preview == asset.id) "Stop" else "Preview") }
                                    TextButton(onClick = { stopPreview(); selected = asset }, enabled = ed.busy == null && !resolving) { Text(if (video) "Choose resolution" else "Import") }
                                }
                            }
                        }
                    }
                    if (loading) item { LinearProgressIndicator(Modifier.fillMaxWidth()) }
                    if (!loading && results.isEmpty() && error == null) item { Text("No assets found. Try another category.") }
                    if (!loading && next != null) item {
                        TextButton(onClick = {
                            val url = next ?: return@TextButton
                            loading = true; error = null
                            pageJob = scope.launch {
                                try { val page = MixkitCatalog.search(url, video); results = (results + page.items).distinctBy { it.id }; next = page.next }
                                catch (e: CancellationException) { throw e }
                                catch (e: Exception) { error = e.message }
                                finally { loading = false }
                            }
                        }) { Text("Load more") }
                    }
                }
            }
        }
    }
    val asset = selected
    val options = offer
    if (asset != null && options != null) AlertDialog(
        onDismissRequest = { if (!resolving) selected = null },
        title = { Text(if (video) "Choose video resolution" else "Import sound effect") },
        text = {
            Column {
                Text(asset.title)
                Text(options.license, style = MaterialTheme.typography.bodySmall)
                options.choices.forEach { option ->
                    Row { RadioButton(choice == option, { choice = option }, enabled = !resolving); TextButton(onClick = { choice = option }, enabled = !resolving) { Text(option.label) } }
                }
                if (resolving) LinearProgressIndicator(Modifier.fillMaxWidth())
            }
        },
        confirmButton = { TextButton(enabled = choice != null && !resolving && ed.busy == null, onClick = {
            val chosen = choice ?: return@TextButton
            resolving = true; error = null
            scope.launch {
                try {
                    val url = MixkitCatalog.resolve(chosen, video)
                    val credits = "${asset.title}\nSource: ${asset.page}\nLicense: ${options.license}\nhttps://mixkit.co/license/\nSelected format: ${chosen.label}\n"
                    selected = null
                    if (video) ed.importOnlineVideo(url, credits, overlay) else ed.importOnlineSound(url, credits)
                } catch (e: CancellationException) { throw e }
                catch (e: Exception) { error = e.message; selected = null }
                finally { resolving = false }
            }
        }) { Text("Download & import") } },
        dismissButton = { TextButton(onClick = { selected = null }, enabled = !resolving) { Text("Cancel") } },
    )
}
