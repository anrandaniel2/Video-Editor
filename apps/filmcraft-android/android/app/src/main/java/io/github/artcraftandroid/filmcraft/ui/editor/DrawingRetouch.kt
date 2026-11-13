package io.github.artcraftandroid.filmcraft.ui.editor

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

@Composable
internal fun DrawingRetouchPanel(view: DrawingView, choose: (String) -> Unit) {
    var aligned by remember { mutableStateOf(view.cloneAligned) }
    var merged by remember { mutableStateOf(view.cloneSampleMerged) }
    var range by remember { mutableIntStateOf(view.retouchTonalRange.toInt()) }
    val editable = !view.operationBusy && !view.layerLocked(view.layer) && view.transformMode == null
    Column(Modifier.heightIn(max = 370.dp).verticalScroll(rememberScrollState())) {
        Text("Native retouch strokes commit on pen lift. The guide is display-only. Size, opacity, pressure response, stabilization, selection and alpha lock apply; patterned brushes and symmetry do not yet apply.", color = Color.White, style = MaterialTheme.typography.bodySmall)
        Text("Clone stamp", color = Color.White, style = MaterialTheme.typography.titleSmall)
        Text(view.cloneSource?.let { "Source ${it.x.toInt()}, ${it.y.toInt()}" } ?: "No clone source selected", color = Color.White)
        Row {
            TextButton(enabled = !view.operationBusy, onClick = { choose("Clone source") }) { Text("Set source on canvas") }
            TextButton(enabled = !view.operationBusy && view.cloneSource != null, onClick = { view.clearCloneSource() }) { Text("Clear source") }
        }
        Row(verticalAlignment = Alignment.CenterVertically) { Checkbox(aligned, { aligned = it; view.cloneAligned = it }, enabled = !view.operationBusy); Text("Aligned source across strokes", color = Color.White) }
        Row(verticalAlignment = Alignment.CenterVertically) { Checkbox(merged, { merged = it; view.cloneSampleMerged = it }, enabled = !view.operationBusy); Text("Sample visible merged image", color = Color.White) }
        Text("Merged sampling includes the current reference frame. Otherwise the active layer's raw pre-stroke pixels are sampled. Source pixels outside the canvas are skipped.", color = Color.White, style = MaterialTheme.typography.bodySmall)
        TextButton(enabled = editable && view.cloneSource != null, onClick = { choose("Clone stamp") }) { Text("Paint clone stamp") }
        HorizontalDivider(color = Color.White.copy(alpha = .15f))
        Text("Tonal retouch", color = Color.White, style = MaterialTheme.typography.titleSmall)
        Row(Modifier.horizontalScroll(rememberScrollState())) { listOf("All tones", "Shadows", "Midtones", "Highlights").forEachIndexed { index, name ->
            TextButton(enabled = !view.operationBusy, onClick = { range = index; view.retouchTonalRange = index.toUInt() }) { Text(if (range == index) "● $name" else name) }
        } }
        listOf("Dodge" to "Lighten existing colours", "Burn" to "Darken existing colours", "Desaturate" to "Reduce colour saturation", "Colorize" to "Apply foreground hue/saturation, retaining luminance").forEach { (tool, description) ->
            TextButton(enabled = editable, onClick = { choose(tool) }) { Text("$tool · $description") }
        }
        Text("Tonal tools preserve pixel alpha and use display-encoded RGB; they are not colour-managed HDR exposure brushes or healing/content-aware tools.", color = Color.White, style = MaterialTheme.typography.bodySmall)
    }
}
