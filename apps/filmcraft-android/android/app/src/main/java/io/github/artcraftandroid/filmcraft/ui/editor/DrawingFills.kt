package io.github.artcraftandroid.filmcraft.ui.editor

import android.content.Context
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import io.github.artcraftandroid.filmcraft.core.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

private fun colorBytes(color: Int) = byteArrayOf(android.graphics.Color.red(color).toByte(), android.graphics.Color.green(color).toByte(), android.graphics.Color.blue(color).toByte(), android.graphics.Color.alpha(color).toByte())
private fun stopColor(stop: DrawingGradientStop) = Color(android.graphics.Color.argb(stop.rgba[3].toInt() and 255, stop.rgba[0].toInt() and 255, stop.rgba[1].toInt() and 255, stop.rgba[2].toInt() and 255))

@Composable
internal fun DrawingMultiGradientPanel(view: DrawingView, foreground: Int) {
    val context = LocalContext.current
    val preferences = remember { context.getSharedPreferences("drawing_resources_v1", Context.MODE_PRIVATE) }
    val scope = rememberCoroutineScope()
    var stops by remember { mutableStateOf(listOf(DrawingGradientStop(0f, colorBytes(foreground)), DrawingGradientStop(1f, colorBytes(android.graphics.Color.WHITE)))) }
    var selected by remember { mutableIntStateOf(0) }
    var kind by remember { mutableIntStateOf(0) }; var repeat by remember { mutableIntStateOf(0) }
    var interpolation by remember { mutableIntStateOf(0) }; var reverse by remember { mutableStateOf(false) }; var linear by remember { mutableStateOf(false) }
    var angle by remember { mutableFloatStateOf(0f) }; var extent by remember { mutableFloatStateOf(.5f) }
    var x by remember { mutableFloatStateOf(.5f) }; var y by remember { mutableFloatStateOf(.5f) }
    var name by remember { mutableStateOf("") }; var message by remember { mutableStateOf<String?>(null) }
    var ioBusy by remember { mutableStateOf(false) }
    var pendingImport by remember { mutableStateOf<List<DrawingGradientPreset>?>(null) }
    var presets by remember { mutableStateOf(runCatching { drawingDecodeGradients(preferences.getString("gradients", null) ?: "{\"format\":\"filmcraft-gradients\",\"version\":1,\"presets\":[]}") }.getOrDefault(emptyList())) }
    fun preset() = DrawingGradientPreset(name.trim().ifEmpty { "Gradient" }, stops, kind.toUInt(), repeat.toUInt(), reverse, interpolation.toUInt(), linear)
    fun store(next: List<DrawingGradientPreset>) { val text = drawingEncodeGradients(next); preferences.edit().putString("gradients", text).apply(); presets = next }
    val save = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/octet-stream")) { uri -> if (uri != null) scope.launch {
        ioBusy = true
        try { writeDrawingResource(context, uri, drawingEncodeGradients(presets).toByteArray()); message = "Gradients exported." }
        catch (e: CancellationException) { throw e } catch (e: Exception) { message = e.message } finally { ioBusy = false }
    } }
    val open = rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri -> if (uri != null) scope.launch {
        ioBusy = true
        try { pendingImport = drawingDecodeGradients(readDrawingResource(context, uri, 262_144).toString(Charsets.UTF_8)) }
        catch (e: CancellationException) { throw e } catch (e: Exception) { message = e.message } finally { ioBusy = false }
    } }
    val editable = !view.operationBusy && !ioBusy
    Column(Modifier.heightIn(max = 410.dp).verticalScroll(rememberScrollState())) {
        Text("Multi-stop gradients apply to the active layer or selection. Geometry and transparent-stop interpolation run in Rust. Bottom opacity controls fill strength.", color = Color.White, style = MaterialTheme.typography.bodySmall)
        Box(Modifier.fillMaxWidth().height(28.dp).background(Brush.horizontalGradient(*stops.map { it.position to stopColor(it) }.toTypedArray())))
        message?.let { Text(it, color = Color.White) }
        if (!editable) LinearProgressIndicator(Modifier.fillMaxWidth(), color = Color.White)
        Row(Modifier.horizontalScroll(rememberScrollState())) { stops.forEachIndexed { index, stop ->
            TextButton(enabled = editable, onClick = { selected = index }) { Text("${if (selected == index) "● " else ""}${(stop.position * 100).toInt()}%", color = Color.White) }
        } }
        val stop = stops[selected]
        Text("Selected stop position ${(stop.position * 100).toInt()}%", color = Color.White)
        Slider(stop.position, { value -> stops = stops.mapIndexed { index, old -> if (index == selected) old.copy(position = value.coerceIn(if (selected > 0) stops[selected - 1].position else 0f, if (selected < stops.lastIndex) stops[selected + 1].position else 1f)) else old } }, enabled = editable)
        Text("Stop alpha ${stop.rgba[3].toInt() and 255}", color = Color.White)
        Slider((stop.rgba[3].toInt() and 255).toFloat(), { value -> stops = stops.mapIndexed { index, old -> if (index == selected) old.copy(rgba = old.rgba.copyOf().also { it[3] = value.toInt().toByte() }) else old } }, valueRange = 0f..255f, enabled = editable)
        Row {
            TextButton(enabled = editable, onClick = { stops = stops.mapIndexed { index, old -> if (index == selected) old.copy(rgba = colorBytes(foreground).also { it[3] = old.rgba[3] }) else old } }) { Text("Use foreground colour") }
            TextButton(enabled = editable && stops.size > 2, onClick = { stops = stops.filterIndexed { i, _ -> i != selected }; selected = selected.coerceAtMost(stops.lastIndex) }) { Text("Remove stop") }
        }
        TextButton(enabled = editable && stops.size < 32, onClick = {
            val gap = stops.zipWithNext().indices.maxByOrNull { stops[it + 1].position - stops[it].position } ?: 0
            val added = DrawingGradientStop((stops[gap].position + stops[gap + 1].position) / 2, colorBytes(foreground))
            stops = stops.toMutableList().also { it.add(gap + 1, added) }; selected = gap + 1
        }) { Text("Add stop in largest gap") }
        Row(Modifier.horizontalScroll(rememberScrollState())) { listOf("Linear", "Radial", "Diamond", "Conical", "Square", "Bilinear", "Spiral").forEachIndexed { index, label -> TextButton(enabled = editable, onClick = { kind = index }) { Text(if (kind == index) "● $label" else label) } } }
        Row { listOf("Clamp", "Repeat", "Reflect").forEachIndexed { index, label -> TextButton(enabled = editable, onClick = { repeat = index }) { Text(if (repeat == index) "● $label" else label) } } }
        Row(Modifier.horizontalScroll(rememberScrollState())) { listOf("Linear blend", "Smoothstep", "Cosine", "Constant").forEachIndexed { index, label -> TextButton(enabled = editable, onClick = { interpolation = index }) { Text(if (interpolation == index) "● $label" else label) } } }
        Row(verticalAlignment = Alignment.CenterVertically) { Checkbox(reverse, { reverse = it }, enabled = editable); Text("Reverse", color = Color.White) }
        Row(verticalAlignment = Alignment.CenterVertically) { Checkbox(linear, { linear = it }, enabled = editable); Text("Interpolate in linear-light sRGB", color = Color.White) }
        Text("Angle ${angle.toInt()}°", color = Color.White); Slider(angle, { angle = it }, valueRange = -180f..180f, enabled = editable)
        Text("Length ${(extent * 100).toInt()}%", color = Color.White); Slider(extent, { extent = it }, valueRange = .01f..2f, enabled = editable)
        Text("Origin X ${(x * 100).toInt()}%", color = Color.White); Slider(x, { x = it }, enabled = editable)
        Text("Origin Y ${(y * 100).toInt()}%", color = Color.White); Slider(y, { y = it }, enabled = editable)
        TextButton(enabled = editable, onClick = { scope.launch {
            try { message = null; view.applyMultiGradient(preset(), angle, extent, x, y); message = "Gradient applied." }
            catch (e: CancellationException) { throw e } catch (e: Exception) { message = e.message }
        } }) { Text("Apply gradient") }
        OutlinedTextField(name, { name = it.take(80) }, label = { Text("Gradient preset name") }, singleLine = true, enabled = editable, modifier = Modifier.fillMaxWidth())
        TextButton(enabled = editable && name.isNotBlank() && presets.size < 64, onClick = { try { store(presets.filterNot { it.name == name.trim() } + preset()); message = "Preset saved." } catch (e: Exception) { message = e.message } }) { Text("Save gradient preset") }
        presets.forEachIndexed { index, item -> Row {
            TextButton(enabled = editable, modifier = Modifier.weight(1f), onClick = { name = item.name; stops = item.stops; selected = 0; kind = item.kind.toInt(); repeat = item.repeat.toInt(); reverse = item.reverse; interpolation = item.interpolation.toInt(); linear = item.linearLight }) { Text(item.name) }
            TextButton(enabled = editable, onClick = { store(presets.filterIndexed { i, _ -> i != index }) }) { Text("Remove") }
        } }
        Row {
            TextButton(enabled = editable, onClick = { open.launch("*/*") }) { Text("Import presets…") }
            TextButton(enabled = editable, onClick = { save.launch("Gradients.fcgradients") }) { Text("Export presets…") }
        }
        pendingImport?.let { imported ->
            Text("Replace saved gradient presets with this collection?", color = Color.White)
            Row { TextButton(onClick = { pendingImport = null }) { Text("Cancel") }; TextButton(onClick = { store(imported); pendingImport = null }) { Text("Replace") } }
        }
        Text("The colour strip is an encoded-colour UI preview; it does not preview every interpolation mode or the final canvas. Presets are FilmCraft resources, not Krita gradient resources.", color = Color.White, style = MaterialTheme.typography.bodySmall)
    }
}

@Composable
internal fun DrawingPatternPanel(view: DrawingView, foreground: Int) {
    var kind by remember { mutableIntStateOf(0) }; var scale by remember { mutableFloatStateOf(32f) }
    var angle by remember { mutableFloatStateOf(0f) }; var density by remember { mutableFloatStateOf(.5f) }
    var background by remember { mutableIntStateOf(android.graphics.Color.WHITE) }; var transparent by remember { mutableStateOf(false) }
    var seed by remember { mutableIntStateOf(1) }; var message by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope(); val editable = !view.operationBusy
    Column(Modifier.heightIn(max = 370.dp).verticalScroll(rememberScrollState())) {
        Text("Procedural raster fills use foreground and background colours, clipped to the active selection. Bottom opacity controls strength.", color = Color.White, style = MaterialTheme.typography.bodySmall)
        Row(Modifier.horizontalScroll(rememberScrollState())) { listOf("Checker", "Stripes", "Dots", "Grid", "Brick", "Crosshatch", "Triangles", "Random tiles").forEachIndexed { index, label -> TextButton(enabled = editable, onClick = { kind = index }) { Text(if (kind == index) "● $label" else label) } } }
        Text("Cell size ${scale.toInt()} px", color = Color.White); Slider(scale, { scale = it }, valueRange = 2f..512f, enabled = editable)
        Text("Angle ${angle.toInt()}°", color = Color.White); Slider(angle, { angle = it }, valueRange = -180f..180f, enabled = editable)
        Text("Density ${(density * 100).toInt()}%", color = Color.White); Slider(density, { density = it }, valueRange = .02f.. .98f, enabled = editable)
        Row(verticalAlignment = Alignment.CenterVertically) { Checkbox(transparent, { transparent = it }, enabled = editable); Text("Transparent background", color = Color.White) }
        Row {
            TextButton(enabled = editable, onClick = { background = android.graphics.Color.WHITE }) { Text("White background") }
            TextButton(enabled = editable, onClick = { background = foreground }) { Text("Capture foreground as background") }
        }
        if (kind == 7) TextButton(enabled = editable, onClick = { seed = if (seed == Int.MAX_VALUE) 1 else seed + 1 }) { Text("New random seed · $seed") }
        message?.let { Text(it, color = Color.White) }
        TextButton(enabled = editable, onClick = { scope.launch {
            try { view.applyPattern(kind.toUInt(), scale, angle, density, foreground, if (transparent) android.graphics.Color.TRANSPARENT else background, seed.toUInt()); message = "Pattern applied." }
            catch (e: CancellationException) { throw e } catch (e: Exception) { message = e.message }
        } }) { Text("Apply pattern") }
    }
}
