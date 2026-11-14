package io.github.artcraftandroid.filmcraft.ui.editor

import io.github.artcraftandroid.procraft.ui.StudioRangeSlider as Slider
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch
import kotlin.math.roundToInt

@Composable
internal fun DrawingAnimationPanel(view: DrawingView) {
    val context = LocalContext.current; val scope = rememberCoroutineScope()
    var message by remember { mutableStateOf<String?>(null) }; var ioBusy by remember { mutableStateOf(false) }
    var looped by remember { mutableStateOf(true) }; var onion by remember { mutableStateOf(view.onionEnabled) }
    var onionAlpha by remember { mutableFloatStateOf(view.onionOpacity) }
    var pendingOpen by remember { mutableStateOf<android.net.Uri?>(null) }; var resetConfirm by remember { mutableStateOf(false) }
    val info = runCatching { view.animationSession?.info() }.getOrNull()
    val active = info?.active?.toInt() ?: 0; val frame = info?.frames?.getOrNull(active)
    var name by remember(active, frame?.name) { mutableStateOf(frame?.name ?: "Frame") }
    var exposure by remember(active, frame?.exposure) { mutableFloatStateOf(frame?.exposure?.toFloat() ?: 1f) }
    var fps by remember(info?.fps) { mutableFloatStateOf(info?.fps?.toFloat() ?: 12f) }
    val editable = !view.operationBusy && !view.animationPlaying && !ioBusy
    fun action(command: String, index: Int = 0) { scope.launch {
        try { message = null; view.animationAction(command, index, name, exposure.roundToInt().toUInt(), fps.roundToInt().toUInt()) }
        catch (e: CancellationException) { throw e } catch (e: Exception) { message = e.message ?: "Animation operation failed." }
    } }
    val save = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/octet-stream")) { uri -> if (uri != null) scope.launch {
        ioBusy = true
        try { writeDrawingResource(context, uri, view.encodeAnimation()); message = "Flipbook saved." }
        catch (e: CancellationException) { throw e } catch (e: Exception) { message = e.message } finally { ioBusy = false }
    } }
    val exportPng = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("image/apng")) { uri -> if (uri != null) scope.launch {
        ioBusy = true
        try { writeDrawingResource(context, uri, view.exportAnimationPng(looped)); message = "Animated PNG exported with frame timing and transparency." }
        catch (e: CancellationException) { throw e } catch (e: Exception) { message = e.message } finally { ioBusy = false }
    } }
    val open = rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri -> if (uri != null) pendingOpen = uri }
    Column(Modifier.heightIn(max = 410.dp).verticalScroll(rememberScrollState())) {
        Text("Native flipbook frames contain full layered snapshots. Current-frame pixel undo is local; switching frames clears that local stack. Frame Undo/Redo below restores native frame checkpoints and structural edits.", color = Color.White, style = MaterialTheme.typography.bodySmall)
        message?.let { Text(it, color = Color.White) }
        if (view.operationBusy || ioBusy) LinearProgressIndicator(Modifier.fillMaxWidth(), color = Color.White)
        if (info == null) TextButton(enabled = editable, onClick = { action("start") }) { Text("Start flipbook from current canvas") }
        else {
            Text("${info.frames.size} frames · ${info.totalTicks} exposure ticks · ${info.fps} fps", color = Color.White)
            Row(Modifier.horizontalScroll(rememberScrollState())) { info.frames.forEachIndexed { index, item ->
                TextButton(enabled = editable, onClick = { action("select", index) }) { Text("${if (active == index) "● " else ""}${index + 1}: ${item.name} ×${item.exposure}") }
            } }
            Row {
                TextButton(enabled = editable, onClick = { action("blank") }) { Text("Add blank") }
                TextButton(enabled = editable, onClick = { action("duplicate") }) { Text("Duplicate") }
                TextButton(enabled = editable && info.frames.size > 1, onClick = { action("delete") }) { Text("Delete") }
            }
            Row {
                TextButton(enabled = editable && active > 0, onClick = { action("left") }) { Text("Move left") }
                TextButton(enabled = editable && active + 1 < info.frames.size, onClick = { action("right") }) { Text("Move right") }
                TextButton(enabled = editable && info.canUndo, onClick = { action("undo") }) { Text("Frame undo") }
                TextButton(enabled = editable && info.canRedo, onClick = { action("redo") }) { Text("Frame redo") }
            }
            OutlinedTextField(name, { name = it.take(80) }, label = { Text("Frame name") }, singleLine = true, enabled = editable, modifier = Modifier.fillMaxWidth())
            Text("Hold ${exposure.roundToInt()} ticks", color = Color.White); Slider(exposure, { exposure = it }, valueRange = 1f..120f, enabled = editable)
            Text("Frame rate ${fps.roundToInt()} fps", color = Color.White); Slider(fps, { fps = it }, valueRange = 1f..60f, enabled = editable)
            TextButton(enabled = editable, onClick = { action("configure") }) { Text("Apply timing and name") }
            Row(verticalAlignment = Alignment.CenterVertically) { Checkbox(looped, { looped = it }, enabled = editable); Text("Loop playback", color = Color.White) }
            Row {
                TextButton(enabled = editable, onClick = { view.playAnimation(looped) }) { Text("Play preview") }
                TextButton(enabled = view.animationPlaying, onClick = { view.stopAnimation() }) { Text("Stop playback") }
            }
            Row(verticalAlignment = Alignment.CenterVertically) { Checkbox(onion, { value -> onion = value; view.onionEnabled = value; scope.launch {
                try { view.refreshOnionSkins() } catch (e: CancellationException) { throw e } catch (e: Exception) { message = e.message }
            } }, enabled = editable); Text("Onion skins · previous red / next green", color = Color.White) }
            Slider(onionAlpha, { onionAlpha = it; view.onionOpacity = it; view.invalidate() }, enabled = editable)
            Row {
                TextButton(enabled = editable, onClick = { save.launch("Animation.fcanim") }) { Text("Save flipbook…") }
                TextButton(enabled = editable, onClick = { open.launch("*/*") }) { Text("Open flipbook…") }
            }
            TextButton(enabled = editable, onClick = { exportPng.launch("Animation.apng") }) { Text("Export animated PNG…") }
            TextButton(enabled = editable, onClick = { resetConfirm = true }) { Text("Start new flipbook from this canvas…") }
        }
        if (info == null) TextButton(enabled = editable, onClick = { open.launch("*/*") }) { Text("Open flipbook…") }
        if (resetConfirm) {
            Text("Discard the native flipbook timeline and start again with this canvas? Save the flipbook first if needed.", color = Color.White)
            Row { TextButton(onClick = { resetConfirm = false }) { Text("Cancel") }; TextButton(enabled = editable, onClick = { resetConfirm = false; action("reset") }) { Text("Start new") } }
        }
        pendingOpen?.let { uri ->
            Text("Opening replaces this canvas and the flipbook timeline. Unsaved work will be discarded.", color = Color.White)
            Row { TextButton(onClick = { pendingOpen = null }) { Text("Cancel") }; TextButton(enabled = editable, onClick = { pendingOpen = null; scope.launch {
                ioBusy = true
                try { view.openAnimation(readDrawingResource(context, uri, 97 * 1024 * 1024)); message = "Flipbook opened." }
                catch (e: CancellationException) { throw e } catch (e: Exception) { message = e.message } finally { ioBusy = false }
            } }) { Text("Replace") } }
        }
        Text("Limits: 120 frames, 96 MiB unique native frame pixels, bounded structural history. Playback composites frames on demand and may miss timing on heavy documents. This is not a layer-keyframe timeline, KRA animation or animated-video/GIF export. Animated PNG export preserves full frames and exposure holds. Canvas resizing requires starting a new flipbook. Reference frames and onion skins are not stored in frame pixels.", color = Color.White, style = MaterialTheme.typography.bodySmall)
    }
}
