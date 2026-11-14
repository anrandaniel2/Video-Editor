package io.github.artcraftandroid.filmcraft.ui.editor

import io.github.artcraftandroid.procraft.ui.StudioRangeSlider as Slider
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch
import kotlin.math.roundToInt

@Composable
internal fun DrawingDocumentTools(view: DrawingView) {
    var width by remember(view.canvasW, view.canvasH) { mutableFloatStateOf(view.canvasW.toFloat().coerceIn(1f, 4096f)) }
    var height by remember(view.canvasW, view.canvasH) { mutableFloatStateOf(view.canvasH.toFloat().coerceIn(1f, 4096f)) }
    var aspectLocked by remember { mutableStateOf(true) }
    var centered by remember { mutableStateOf(true) }
    var message by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    val maximumPixels = minOf(4_194_304L, 96L * 1024 * 1024 / (4 * view.layers))
    val valid = width.roundToInt().toLong() * height.roundToInt() <= maximumPixels
    fun apply(operation: String) {
        scope.launch {
            try { message = null; view.transformDocument(operation, width.roundToInt(), height.roundToInt(), centered); message = "Document geometry updated. Undo restores the previous layered canvas." }
            catch (e: CancellationException) { throw e }
            catch (e: Exception) { message = e.message ?: "Could not transform the document." }
        }
    }
    Column(Modifier.heightIn(max = 390.dp).verticalScroll(rememberScrollState())) {
        Text("These tools modify every paint layer, not the view. Scale image resamples pixels; canvas size adds/removes transparent space. Cropping uses the selection's rectangular bounds.", color = Color.White, style = MaterialTheme.typography.bodySmall)
        Text("Current ${view.canvasW} × ${view.canvasH} · ${view.layers} layers", color = Color.White)
        message?.let { Text(it, color = Color.White) }
        if (view.operationBusy) LinearProgressIndicator(Modifier.fillMaxWidth(), color = Color.White)
        Text("Width ${width.roundToInt()} px", color = Color.White)
        Slider(width, { value ->
            width = value
            if (aspectLocked) height = (width * view.canvasH / view.canvasW).coerceIn(1f, 4096f)
        }, valueRange = 1f..4096f, enabled = !view.operationBusy)
        Text("Height ${height.roundToInt()} px", color = Color.White)
        Slider(height, { value ->
            height = value
            if (aspectLocked) width = (height * view.canvasW / view.canvasH).coerceIn(1f, 4096f)
        }, valueRange = 1f..4096f, enabled = !view.operationBusy)
        Row(verticalAlignment = Alignment.CenterVertically) { Checkbox(aspectLocked, { aspectLocked = it }, enabled = !view.operationBusy); Text("Keep current aspect ratio", color = Color.White) }
        Row(verticalAlignment = Alignment.CenterVertically) { Checkbox(centered, { centered = it }, enabled = !view.operationBusy); Text("Center existing pixels when resizing canvas", color = Color.White) }
        if (!valid) Text("Requested size exceeds this document's raster budget ($maximumPixels pixels per layer). Reduce width or height.", color = Color.White)
        Row {
            TextButton(enabled = !view.operationBusy && valid, onClick = { apply("scale") }) { Text("Scale image") }
            TextButton(enabled = !view.operationBusy && valid, onClick = { apply("canvas") }) { Text("Resize canvas") }
        }
        Row {
            TextButton(enabled = !view.operationBusy, onClick = { apply("rotate-ccw") }) { Text("Rotate 90° left") }
            TextButton(enabled = !view.operationBusy, onClick = { apply("rotate-cw") }) { Text("Rotate 90° right") }
        }
        TextButton(enabled = !view.operationBusy && view.selection != null, onClick = { apply("crop") }) { Text("Crop canvas to selection bounds") }
        TextButton(enabled = !view.operationBusy, onClick = { apply("trim") }) { Text("Trim transparent margins") }
        Text("Trim measures visible paint only, not the reference frame. Hidden layers are cropped too. Selections and clipboard are cleared when document geometry changes.", color = Color.White, style = MaterialTheme.typography.bodySmall)
    }
}

@Composable
internal fun DrawingAdvancedFilters(view: DrawingView) {
    var operation by remember { mutableStateOf("gaussian") }
    var radius by remember { mutableFloatStateOf(6f) }
    var strength by remember { mutableFloatStateOf(1f) }
    var angle by remember { mutableFloatStateOf(0f) }
    var seed by remember { mutableLongStateOf(1L) }
    var message by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    Column(Modifier.heightIn(max = 380.dp).verticalScroll(rememberScrollState())) {
        Text("Native raster filters apply to the active layer/selection as one undo step. Gaussian and motion blur interpolate premultiplied colour to avoid transparent-colour halos.", color = Color.White, style = MaterialTheme.typography.bodySmall)
        val filters = listOf("gaussian" to "Gaussian blur", "unsharp" to "Unsharp mask", "highpass" to "High pass", "motion" to "Motion blur", "median" to "Median 3 × 3", "sobel" to "Sobel edges", "noise-mono" to "Mono noise", "noise-rgb" to "RGB noise", "noise-gaussian" to "Gaussian noise", "spread" to "Pixel spread", "vignette" to "Vignette")
        Row(Modifier.horizontalScroll(rememberScrollState())) { filters.forEach { (id, name) ->
            TextButton(enabled = !view.operationBusy, onClick = { operation = id }) { Text(if (operation == id) "● $name" else name) }
        } }
        message?.let { Text(it, color = Color.White) }
        if (view.operationBusy) LinearProgressIndicator(Modifier.fillMaxWidth(), color = Color.White)
        if (operation in listOf("gaussian", "unsharp", "highpass", "motion", "spread")) {
            Text("Radius ${radius.roundToInt()} px", color = Color.White)
            Slider(radius, { radius = it }, valueRange = 1f..24f, enabled = !view.operationBusy)
        }
        Text("Strength ${(strength * 100).roundToInt()}%", color = Color.White)
        Slider(strength, { strength = it }, enabled = !view.operationBusy)
        if (operation == "motion") {
            Text("Angle ${angle.roundToInt()}°", color = Color.White)
            Slider(angle, { angle = it }, valueRange = -180f..180f, enabled = !view.operationBusy)
        }
        if (operation.startsWith("noise") || operation == "spread") {
            Text("Random seed $seed · identical settings reproduce the same result", color = Color.White, style = MaterialTheme.typography.bodySmall)
            TextButton(enabled = !view.operationBusy, onClick = { seed = if (seed == Long.MAX_VALUE) 1 else seed + 1 }) { Text("New seed") }
        }
        TextButton(enabled = !view.operationBusy, onClick = { scope.launch {
            try { message = null; view.applyAdvancedFilter(operation, radius.roundToInt().toUInt(), strength, angle, seed.toULong()); message = "Filter applied." }
            catch (e: CancellationException) { throw e }
            catch (e: Exception) { message = e.message ?: "Filter failed." }
        } }) { Text("Apply filter") }
    }
}
