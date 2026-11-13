package io.github.artcraftandroid.filmcraft.ui.editor

import android.content.Context
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
import io.github.artcraftandroid.filmcraft.core.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

@Composable
private fun NativeBrushSlider(label: String, value: Float, range: ClosedFloatingPointRange<Float>, enabled: Boolean, change: (Float) -> Unit) {
    Text("$label · ${java.lang.String.format(java.util.Locale.ROOT, "%.2f", value)}", color = Color.White)
    Slider(value, change, valueRange = range, enabled = enabled)
}

@Composable
internal fun DrawingNativeBrushControls(view: DrawingView, activate: (Boolean) -> Unit, change: (DrawingNativeBrush) -> Unit) {
    val context = LocalContext.current
    val preferences = remember { context.getSharedPreferences("drawing_resources_v1", Context.MODE_PRIVATE) }
    val scope = rememberCoroutineScope()
    var hardware by remember { mutableStateOf<DrawingHardwareStatus?>(null) }
    var checkingGpu by remember { mutableStateOf(true) }
    LaunchedEffect(Unit) {
        try { hardware = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) { drawingGpuStatus() } }
        catch (e: CancellationException) { throw e }
        catch (e: Exception) { hardware = null }
        finally { checkingGpu = false }
    }
    var settings by remember { mutableStateOf(view.nativeBrushSettings.copy(symmetry = view.symmetryMode)) }
    var enabled by remember { mutableStateOf(view.nativeBrushEnabled) }
    var name by remember { mutableStateOf("") }
    var message by remember { mutableStateOf<String?>(null) }
    var ioBusy by remember { mutableStateOf(false) }
    var presets by remember {
        mutableStateOf(runCatching {
            val root = org.json.JSONObject(preferences.getString("native_brush_presets", "{}") ?: "{}")
            root.keys().asSequence().take(64).mapNotNull { key -> runCatching { key to drawingDecodeNativeBrush(root.getString(key)) }.getOrNull() }.toList()
        }.getOrDefault(emptyList()))
    }
    fun update(next: DrawingNativeBrush) { settings = next; change(next) }
    fun store(next: List<Pair<String, DrawingNativeBrush>>) {
        val root = org.json.JSONObject(); next.forEach { (key, brush) -> root.put(key, drawingEncodeNativeBrush(brush)) }
        preferences.edit().putString("native_brush_presets", root.toString()).apply(); presets = next
    }
    val save = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/octet-stream")) { uri -> if (uri != null) scope.launch {
        ioBusy = true
        try { writeDrawingResource(context, uri, drawingEncodeNativeBrush(settings).toByteArray()); message = "Native brush exported." }
        catch (e: CancellationException) { throw e } catch (e: Exception) { message = e.message } finally { ioBusy = false }
    } }
    val open = rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri -> if (uri != null) scope.launch {
        ioBusy = true
        try { update(drawingDecodeNativeBrush(readDrawingResource(context, uri, 8192).toString(Charsets.UTF_8))); message = "Native brush settings imported." }
        catch (e: CancellationException) { throw e } catch (e: Exception) { message = e.message } finally { ioBusy = false }
    } }
    val editable = !view.operationBusy && !ioBusy
    Column(Modifier.heightIn(max = 410.dp).verticalScroll(rememberScrollState())) {
        Text("Native GPU dab engine: real hardware 4× MSAA rendering and resolve. This mode requires a working GPU; it never silently switches to Android patterned brushes. Committed native strokes retain scene-linear float tiles; Android bitmaps cache their SDR display. Compatibility pixel tools rebase affected layers to 8-bit.", color = Color.White, style = MaterialTheme.typography.bodySmall)
        if (checkingGpu) { Text("Checking GPU render, resolve and readback…", color = Color.White); LinearProgressIndicator(Modifier.fillMaxWidth(), color = Color.White) }
        hardware?.let { status ->
            Text(if (status.available) "${status.adapter} · ${status.backend} · ${status.samples}× MSAA" else "GPU drawing unavailable", color = Color.White)
            Text(status.message, color = Color.White, style = MaterialTheme.typography.bodySmall)
            Text("Canvas display: ${if (view.isHardwareAccelerated) "hardware accelerated" else "not attached to a hardware window"}", color = Color.White, style = MaterialTheme.typography.bodySmall)
        }
        TextButton(enabled = !checkingGpu && !view.operationBusy, onClick = { scope.launch {
            checkingGpu = true
            try { hardware = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) { drawingGpuStatus() } }
            catch (e: CancellationException) { throw e }
            catch (e: Exception) { message = e.message ?: "GPU status check failed." }
            finally { checkingGpu = false }
        } }) { Text("Refresh GPU status") }
        Text("Native precision layers: ${view.nativePaintLayerCount} · .fcdraw preserves native tiles", color = Color.White, style = MaterialTheme.typography.bodySmall)
        message?.let { Text(it, color = Color.White) }
        Row(verticalAlignment = Alignment.CenterVertically) { Checkbox(enabled, { enabled = it; activate(it) }, enabled = editable); Text("Use native GPU brush / eraser", color = Color.White) }
        Row(Modifier.horizontalScroll(rememberScrollState())) { listOf("Round", "Square", "Diamond", "Star", "Ring", "Bristles", "Chalk", "Spray").forEachIndexed { index, label ->
            TextButton(enabled = editable, onClick = { update(settings.copy(tip = index.toUInt())) }) { Text(if (settings.tip == index.toUInt()) "● $label" else label) }
        } }
        Text("Starting recipes", color = Color.White)
        Row(Modifier.horizontalScroll(rememberScrollState())) {
            TextButton(enabled = editable, onClick = { update(settings.copy(tip = 0u, spacing = .15f, hardness = .95f, aspect = 1f, flow = 1f, scatter = 0f, sizeJitter = 0f, rotationJitter = 0f)) }) { Text("Solid ink") }
            TextButton(enabled = editable, onClick = { update(settings.copy(tip = 0u, spacing = .1f, hardness = 0f, aspect = 1f, flow = .08f, scatter = 0f, sizeJitter = 0f, rotationJitter = 0f)) }) { Text("Soft airbrush") }
            TextButton(enabled = editable, onClick = { update(settings.copy(tip = 1u, spacing = .08f, hardness = 1f, aspect = .22f, rotation = 35f, flow = .3f, scatter = 0f)) }) { Text("Flat marker") }
            TextButton(enabled = editable, onClick = { update(settings.copy(tip = 6u, spacing = .15f, hardness = .8f, aspect = 1f, flow = .35f, scatter = .1f, sizeJitter = .15f)) }) { Text("Dry chalk") }
            TextButton(enabled = editable, onClick = { update(settings.copy(tip = 7u, spacing = .45f, hardness = .3f, aspect = 1f, flow = .6f, scatter = .7f, sizeJitter = .4f, rotationJitter = 180f)) }) { Text("Spray") }
        }
        NativeBrushSlider("Spacing / diameter", settings.spacing, .02f..4f, editable) { update(settings.copy(spacing = it)) }
        NativeBrushSlider("Hardness", settings.hardness, 0f..1f, editable) { update(settings.copy(hardness = it)) }
        NativeBrushSlider("Aspect", settings.aspect, .1f..1f, editable) { update(settings.copy(aspect = it)) }
        NativeBrushSlider("Tip rotation", settings.rotation, -180f..180f, editable) { update(settings.copy(rotation = it)) }
        NativeBrushSlider("Dab flow", settings.flow, 0f..1f, editable) { update(settings.copy(flow = it)) }
        NativeBrushSlider("Scatter", settings.scatter, 0f..2f, editable) { update(settings.copy(scatter = it)) }
        NativeBrushSlider("Size jitter", settings.sizeJitter, 0f..1f, editable) { update(settings.copy(sizeJitter = it)) }
        NativeBrushSlider("Rotation jitter", settings.rotationJitter, 0f..180f, editable) { update(settings.copy(rotationJitter = it)) }
        Row(verticalAlignment = Alignment.CenterVertically) { Checkbox(settings.followDirection, { update(settings.copy(followDirection = it)) }, enabled = editable); Text("Rotate tip along stroke direction", color = Color.White) }
        TextButton(enabled = editable, onClick = { update(settings.copy(seed = if (settings.seed == UInt.MAX_VALUE) 1u else settings.seed + 1u)) }) { Text("New jitter / texture seed · ${settings.seed}") }
        Text("Bottom size/opacity, pressure size response, stabilizer, assistants, selection, symmetry, layer clipping, masks and alpha lock also apply. Native flow accumulates per dab; bottom opacity is applied after accumulation.", color = Color.White, style = MaterialTheme.typography.bodySmall)
        OutlinedTextField(name, { name = it.take(80) }, label = { Text("Native brush preset name") }, enabled = editable, singleLine = true, modifier = Modifier.fillMaxWidth())
        TextButton(enabled = editable && name.isNotBlank() && presets.size < 64, onClick = { try { store(presets.filterNot { it.first == name.trim() } + (name.trim() to settings)); message = "Preset saved." } catch (e: Exception) { message = e.message } }) { Text("Save native brush preset") }
        presets.forEachIndexed { index, item -> Row {
            TextButton(enabled = editable, modifier = Modifier.weight(1f), onClick = { update(item.second); name = item.first }) { Text(item.first) }
            TextButton(enabled = editable, onClick = { store(presets.filterIndexed { i, _ -> i != index }) }) { Text("Remove") }
        } }
        Row {
            TextButton(enabled = editable, onClick = { open.launch("*/*") }) { Text("Import settings…") }
            TextButton(enabled = editable, onClick = { save.launch("Brush.fcnativebrush") }) { Text("Export settings…") }
        }
        Text("These are eight procedural tips, not Krita brush-engine or .kpp compatibility. Large strokes can exceed bounded dab/tile limits and are rejected. GPU preview currently performs synchronous readback; performance and shader compatibility are not device-verified.", color = Color.White, style = MaterialTheme.typography.bodySmall)
    }
}
