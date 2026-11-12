package io.github.artcraftandroid.filmcraft.ui.editor

import android.media.MediaPlayer
import android.net.Uri
import android.widget.VideoView
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.clickable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.ui.Alignment
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import io.github.artcraftandroid.filmcraft.ui.editorGlass
import androidx.activity.compose.BackHandler
import kotlinx.coroutines.flow.distinctUntilChanged
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
import kotlinx.coroutines.cancelAndJoin
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
    val listState = rememberLazyListState()
    val visited = remember { mutableSetOf<String>() }
    val player = remember { MediaPlayer() }
    fun stopPreview() { player.reset(); preview = null }
    DisposableEffect(player) { onDispose { player.release() } }
    LaunchedEffect(query, retry) {
        pageJob?.cancelAndJoin(); visited.clear(); stopPreview(); results = emptyList(); next = null; loading = true; error = null
        listState.scrollToItem(0)
        delay(350)
        try {
            val page = MixkitCatalog.search(MixkitCatalog.listingUrl(video, query), video)
            visited.add(MixkitCatalog.listingUrl(video, query))
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
    fun loadNext() {
        val url = next ?: return
        if (loading || !visited.add(url)) return
        loading = true; error = null
        pageJob = scope.launch {
            try {
                val page = MixkitCatalog.search(url, video)
                results = (results + page.items).distinctBy { it.id }
                next = page.next?.takeUnless { it in visited }
            } catch (e: CancellationException) { throw e }
            catch (e: Exception) { visited.remove(url); error = e.message }
            finally { loading = false }
        }
    }
    LaunchedEffect(listState, next, loading, results.size, error) {
        if (loading || next == null || error != null || results.isEmpty()) return@LaunchedEffect
        snapshotFlow { (listState.layoutInfo.visibleItemsInfo.lastOrNull()?.index ?: -1) >= results.lastIndex - 2 }
            .distinctUntilChanged().collect { nearBottom -> if (nearBottom && !loading) loadNext() }
    }
    BackHandler { if (selected != null) selected = null else close() }
    BoxWithConstraints(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        Box(Modifier.fillMaxSize().editorGlass(0.dp, layer = 4f, blurRadius = 8.dp)
            .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null, onClick = close))
        Column(Modifier.padding(16.dp).widthIn(max = 720.dp).fillMaxWidth().height(maxHeight * .88f)
            .editorGlass(28.dp, layer = 5f, blurRadius = 18.dp)
            .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null, onClick = {})
            .padding(18.dp)) {
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                    Column(Modifier.weight(1f)) {
                        Text("MIXKIT LIBRARY", style = MaterialTheme.typography.labelSmall, color = Color.White.copy(alpha = .65f))
                        Text(if (video) "Find your next shot" else "Make it sound right", style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.SemiBold)
                    }
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
                LazyColumn(Modifier.weight(1f), state = listState, verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    items(results, key = { it.id }) { asset ->
                        Column(Modifier.fillMaxWidth().editorGlass(20.dp, layer = 6f, blurRadius = 38.dp).padding(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                                if (video && preview != asset.id) MixkitThumbnail(asset)
                                Text(asset.title, style = MaterialTheme.typography.titleSmall, fontWeight = FontWeight.SemiBold)
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
                    if (loading) item { LinearProgressIndicator(Modifier.fillMaxWidth()) }
                    if (!loading && results.isEmpty() && error == null) item { Text("No assets found. Try another category.") }
                    if (!loading && error != null && results.isNotEmpty() && next != null) item {
                        TextButton(onClick = { loadNext() }) { Text("Retry loading more") }
                    }
                }
        }
    }

    val asset = selected
    val options = offer
    if (asset != null && options != null) AlertDialog(
        modifier = Modifier.editorGlass(24.dp, layer = 8f, blurRadius = 38.dp),
        containerColor = Color.Transparent,
        onDismissRequest = { if (!resolving) selected = null },
        title = { Text(if (video) "Choose video resolution" else "Import sound effect") },
        text = {
            ImmersiveDialog()
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
