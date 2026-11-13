package io.github.artcraftandroid.filmcraft.ui.editor

import android.content.Context
import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import io.github.artcraftandroid.filmcraft.core.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

internal suspend fun readDrawingResource(context: Context, uri: Uri, limit: Int): ByteArray = withContext(Dispatchers.IO) {
    context.contentResolver.openInputStream(uri)?.use { input ->
        val out = java.io.ByteArrayOutputStream()
        val buffer = ByteArray(16 * 1024)
        while (true) {
            val count = input.read(buffer)
            if (count < 0) break
            require(out.size().toLong() + count <= limit) { "File exceeds the supported size limit." }
            out.write(buffer, 0, count)
        }
        out.toByteArray()
    } ?: error("Could not read the selected file.")
}
internal suspend fun writeDrawingResource(context: Context, uri: Uri, bytes: ByteArray) = withContext(Dispatchers.IO) {
    context.contentResolver.openOutputStream(uri, "wt")?.use { it.write(bytes); it.flush() }
        ?: error("Could not write to the selected destination.")
}

/** Resource controls only. GPL and preset validation/serialization are native Rust. */
@Composable
internal fun DrawingResourcePanel(color: Int, changeColor: (Int) -> Unit, capturePreset: () -> DrawingBrushPreset, applyPreset: (DrawingBrushPreset) -> Unit) {
    val context = LocalContext.current
    val preferences = remember { context.getSharedPreferences("drawing_resources_v1", Context.MODE_PRIVATE) }
    val scope = rememberCoroutineScope()
    var message by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    var palette by remember {
        mutableStateOf(runCatching { drawingImportPalette(preferences.getString("palette", null) ?: "GIMP Palette\nName: Colours\n#\n0 0 0 Black\n255 255 255 White\n255 0 0 Red\n0 128 255 Blue\n") }.getOrDefault(emptyList()))
    }
    var presets by remember { mutableStateOf(runCatching { drawingDecodeBrushPresets(preferences.getString("brushes", null) ?: "{\"format\":\"filmcraft-brushes\",\"version\":1,\"presets\":[]}") }.getOrDefault(emptyList())) }
    var name by remember { mutableStateOf("") }
    var tab by remember { mutableIntStateOf(0) }
    var pendingKind by remember { mutableIntStateOf(0) }
    var importKind by remember { mutableIntStateOf(0) }
    var pendingImport by remember { mutableStateOf<String?>(null) }
    fun storePalette(next: List<DrawingPaletteColor>) {
        val text = drawingExportPalette("Saved colours", next)
        preferences.edit().putString("palette", text).apply(); palette = next
    }
    fun storePresets(next: List<DrawingBrushPreset>) {
        val text = drawingEncodeBrushPresets(next)
        preferences.edit().putString("brushes", text).apply(); presets = next
    }
    val save = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/octet-stream")) { uri ->
        if (uri != null) scope.launch {
            busy = true
            try {
                val text = if (pendingKind == 0) drawingExportPalette("Saved colours", palette) else drawingEncodeBrushPresets(presets)
                writeDrawingResource(context, uri, text.toByteArray(Charsets.UTF_8)); message = "Resource saved."
            } catch (e: CancellationException) { throw e }
            catch (e: Exception) { message = e.message ?: "Save failed." }
            finally { busy = false }
        }
    }
    val open = rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri ->
        if (uri != null) scope.launch {
            busy = true
            try {
                val text = readDrawingResource(context, uri, 262_144).toString(Charsets.UTF_8)
                // Validate before offering the replace confirmation; don't overwrite resources silently.
                if (importKind == 0) drawingImportPalette(text) else drawingDecodeBrushPresets(text)
                pendingImport = text
            } catch (e: CancellationException) { throw e }
            catch (e: Exception) { message = e.message ?: "Import failed." }
            finally { busy = false }
        }
    }
    Column(Modifier.heightIn(max = 380.dp).verticalScroll(rememberScrollState())) {
        Row {
            TextButton(onClick = { tab = 0 }, enabled = !busy) { Text(if (tab == 0) "● Palette" else "Palette") }
            TextButton(onClick = { tab = 1 }, enabled = !busy) { Text(if (tab == 1) "● Brush presets" else "Brush presets") }
        }
        Text(if (tab == 0) "Saved colours persist between sessions. Import/export uses GIMP .gpl palettes." else "Save the current brush, size, opacity, pressure response, stabilizer and symmetry. These are FilmCraft presets, not Krita .kpp brushes.", color = Color.White, style = MaterialTheme.typography.bodySmall)
        OutlinedTextField(name, { name = it.take(80) }, singleLine = true, label = { Text(if (tab == 0) "Colour name" else "Preset name") }, modifier = Modifier.fillMaxWidth(), enabled = !busy)
        message?.let { Text(it, color = Color.White) }
        if (busy) LinearProgressIndicator(Modifier.fillMaxWidth(), color = Color.White)
        if (tab == 0) {
            TextButton(enabled = !busy && palette.size < 256, onClick = {
                try { storePalette(palette + DrawingPaletteColor(name.trim().ifEmpty { "Colour ${palette.size + 1}" }, android.graphics.Color.red(color).toUByte(), android.graphics.Color.green(color).toUByte(), android.graphics.Color.blue(color).toUByte())); message = "Colour saved." }
                catch (e: Exception) { message = e.message }
            }) { Text("Save current colour") }
            palette.forEachIndexed { index, entry ->
                val value = android.graphics.Color.rgb(entry.red.toInt(), entry.green.toInt(), entry.blue.toInt())
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                    Box(Modifier.padding(8.dp).size(28.dp).background(Color(value)).clickable { changeColor(value) })
                    TextButton(onClick = { changeColor(value) }, modifier = Modifier.weight(1f)) { Text(entry.name.ifEmpty { "Colour ${index + 1}" }) }
                    TextButton(enabled = !busy && palette.size > 1, onClick = { storePalette(palette.filterIndexed { i, _ -> i != index }) }) { Text("Remove") }
                }
            }
        } else {
            TextButton(enabled = !busy && name.isNotBlank() && presets.size < 128, onClick = {
                try { storePresets(presets.filterNot { it.name == name.trim() } + capturePreset().copy(name = name.trim())); message = "Brush preset saved." }
                catch (e: Exception) { message = e.message }
            }) { Text("Save current brush preset") }
            presets.forEachIndexed { index, preset ->
                Row(Modifier.fillMaxWidth()) {
                    TextButton(enabled = !busy, onClick = { try { applyPreset(preset); message = "${preset.name} applied." } catch (e: Exception) { message = e.message } }, modifier = Modifier.weight(1f)) { Text("${preset.name} · ${preset.size.toInt()} px") }
                    TextButton(enabled = !busy, onClick = { storePresets(presets.filterIndexed { i, _ -> i != index }) }) { Text("Remove") }
                }
            }
        }
        Row {
            TextButton(enabled = !busy, onClick = { pendingImport = null; importKind = tab; open.launch("*/*") }) { Text("Import…") }
            TextButton(enabled = !busy && (tab != 0 || palette.isNotEmpty()), onClick = { pendingKind = tab; save.launch(if (tab == 0) "Colours.gpl" else "Brushes.fcbrushes") }) { Text("Export…") }
        }
        if (pendingImport != null) {
            Text("Replace the saved ${if (importKind == 0) "palette" else "brush preset collection"} with this file?", color = Color.White)
            Row {
                TextButton(onClick = { pendingImport = null }) { Text("Cancel") }
                TextButton(onClick = {
                    try { val text = pendingImport ?: return@TextButton; if (importKind == 0) storePalette(drawingImportPalette(text)) else storePresets(drawingDecodeBrushPresets(text)); pendingImport = null; message = "Resource imported." }
                    catch (e: Exception) { message = e.message }
                }) { Text("Replace") }
            }
        }
    }
}
