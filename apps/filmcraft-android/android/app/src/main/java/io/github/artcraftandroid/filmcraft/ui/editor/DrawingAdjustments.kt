package io.github.artcraftandroid.filmcraft.ui.editor

import io.github.artcraftandroid.procraft.ui.StudioRangeSlider as Slider
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import io.github.artcraftandroid.filmcraft.core.DrawingAdjustments
import io.github.artcraftandroid.filmcraft.core.drawingCurveSamples
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

@Composable
private fun AdjustmentSlider(label: String, value: Float, change: (Float) -> Unit, range: ClosedFloatingPointRange<Float>, enabled: Boolean) {
    Text("$label · ${java.lang.String.format(java.util.Locale.ROOT, "%.2f", value)}", color = Color.White)
    Slider(value, change, valueRange = range, enabled = enabled)
}

@Composable
internal fun DrawingAdjustmentPanel(view: DrawingView) {
    var exposure by remember { mutableFloatStateOf(0f) }
    var brightness by remember { mutableFloatStateOf(0f) }
    var contrast by remember { mutableFloatStateOf(1f) }
    var saturation by remember { mutableFloatStateOf(1f) }
    var hue by remember { mutableFloatStateOf(0f) }
    var gamma by remember { mutableFloatStateOf(1f) }
    var black by remember { mutableFloatStateOf(0f) }
    var white by remember { mutableFloatStateOf(1f) }
    var red by remember { mutableFloatStateOf(1f) }
    var green by remember { mutableFloatStateOf(1f) }
    var blue by remember { mutableFloatStateOf(1f) }
    var curves by remember { mutableStateOf(false) }
    var channel by remember { mutableIntStateOf(0) }
    var anchors by remember { mutableStateOf(listOf(0f, .25f, .5f, .75f, 1f)) }
    var message by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    Column(Modifier.heightIn(max = 390.dp).verticalScroll(rememberScrollState())) {
        Text("Adjust the active layer or selection. Apply commits one undo step. Settings are not a live image preview. Native paint layers use linear-light float adjustments and retain HDR values; compatibility layers use the byte path. Native curves use interpolated linear-light samples, with last-segment extrapolation above 1; byte layers retain the compatibility curve path.", color = Color.White, style = MaterialTheme.typography.bodySmall)
        Row {
            TextButton(onClick = { curves = false }, enabled = !view.operationBusy) { Text(if (!curves) "● Adjustments" else "Adjustments") }
            TextButton(onClick = { curves = true }, enabled = !view.operationBusy) { Text(if (curves) "● Curves" else "Curves") }
        }
        message?.let { Text(it, color = Color.White) }
        if (view.operationBusy) LinearProgressIndicator(Modifier.fillMaxWidth(), color = Color.White)
        if (!curves) {
            AdjustmentSlider("Exposure (stops)", exposure, { exposure = it }, -4f..4f, !view.operationBusy)
            AdjustmentSlider("Brightness", brightness, { brightness = it }, -1f..1f, !view.operationBusy)
            AdjustmentSlider("Contrast", contrast, { contrast = it }, 0f..4f, !view.operationBusy)
            AdjustmentSlider("Saturation", saturation, { saturation = it }, 0f..2f, !view.operationBusy)
            AdjustmentSlider("Hue (degrees)", hue, { hue = it }, -180f..180f, !view.operationBusy)
            AdjustmentSlider("Gamma", gamma, { gamma = it }, .1f..4f, !view.operationBusy)
            AdjustmentSlider("Input black", black, { black = it.coerceAtMost(white - .01f) }, 0f.. .99f, !view.operationBusy)
            AdjustmentSlider("Input white", white, { white = it.coerceAtLeast(black + .01f) }, .01f..1f, !view.operationBusy)
            AdjustmentSlider("Red gain", red, { red = it }, 0f..2f, !view.operationBusy)
            AdjustmentSlider("Green gain", green, { green = it }, 0f..2f, !view.operationBusy)
            AdjustmentSlider("Blue gain", blue, { blue = it }, 0f..2f, !view.operationBusy)
            Row {
                TextButton(enabled = !view.operationBusy, onClick = { exposure = 0f; brightness = 0f; contrast = 1f; saturation = 1f; hue = 0f; gamma = 1f; black = 0f; white = 1f; red = 1f; green = 1f; blue = 1f }) { Text("Reset settings") }
                TextButton(enabled = !view.operationBusy, onClick = { scope.launch {
                    try { message = null; view.applyAdjustments(DrawingAdjustments(exposure, brightness, contrast, saturation, hue, gamma, black, white, red, green, blue)); message = "Adjustments applied." }
                    catch (e: CancellationException) { throw e }
                    catch (e: Exception) { message = e.message ?: "Adjustment failed." }
                } }) { Text("Apply") }
            }
        } else {
            Row(Modifier.horizontalScroll(rememberScrollState())) { listOf("RGB", "Red", "Green", "Blue", "Alpha").forEachIndexed { index, name ->
                TextButton(enabled = !view.operationBusy, onClick = { channel = index }) { Text(if (channel == index) "● $name" else name) }
            } }
            Canvas(Modifier.fillMaxWidth().height(140.dp)) {
                drawRect(Color.White.copy(alpha = .04f))
                drawLine(Color.Gray, Offset(0f, size.height), Offset(size.width, 0f))
                for (i in 0 until anchors.lastIndex) drawLine(Color.White, Offset(size.width * i / anchors.lastIndex, size.height * (1f - anchors[i])), Offset(size.width * (i + 1) / anchors.lastIndex, size.height * (1f - anchors[i + 1])), strokeWidth = 2.dp.toPx())
            }
            listOf("Black", "Shadows", "Midtones", "Highlights", "White").forEachIndexed { index, name ->
                AdjustmentSlider(name, anchors[index], { value -> anchors = anchors.mapIndexed { i, old -> if (i == index) value else old } }, 0f..1f, !view.operationBusy)
            }
            Row(Modifier.horizontalScroll(rememberScrollState())) {
                TextButton(enabled = !view.operationBusy, onClick = { anchors = listOf(0f, .25f, .5f, .75f, 1f) }) { Text("Linear") }
                TextButton(enabled = !view.operationBusy, onClick = { anchors = listOf(0f, .14f, .5f, .86f, 1f) }) { Text("S-curve") }
                TextButton(enabled = !view.operationBusy, onClick = { anchors = listOf(.12f, .3f, .52f, .73f, .92f) }) { Text("Faded") }
                TextButton(enabled = !view.operationBusy, onClick = { anchors = listOf(1f, .75f, .5f, .25f, 0f) }) { Text("Invert") }
            }
            TextButton(enabled = !view.operationBusy, onClick = { scope.launch {
                try { message = null; view.applyCurve(drawingCurveSamples(anchors), channel.toUInt()); message = "Curve applied." }
                catch (e: CancellationException) { throw e }
                catch (e: Exception) { message = e.message ?: "Curve failed." }
            } }) { Text("Apply curve") }
        }
    }
}
