package io.github.artcraftandroid.filmcraft.ui.editor

import io.github.artcraftandroid.procraft.ui.StudioRangeSlider as Slider
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
internal fun DrawingAssistantPanel(view: DrawingView, chooseOrigin: () -> Unit) {
    var mode by remember { mutableIntStateOf(view.assistantMode.toInt()) }
    var x by remember { mutableFloatStateOf(view.assistantOriginX) }
    var y by remember { mutableFloatStateOf(view.assistantOriginY) }
    var angle by remember { mutableFloatStateOf(view.assistantAngle) }
    var spacing by remember { mutableFloatStateOf(view.assistantSpacing) }
    var strength by remember { mutableFloatStateOf(view.assistantStrength) }
    var guides by remember { mutableStateOf(view.showAssistantGuides) }
    val editable = !view.operationBusy && view.transformMode == null
    Column(Modifier.heightIn(max = 390.dp).verticalScroll(rememberScrollState())) {
        Text("Rust constrains painting samples in canvas space. Guides never enter exports. Applies to pen/eraser, smudge/blur, mask painting and retouch strokes; not selections, fill, shapes or transforms.", color = Color.White, style = MaterialTheme.typography.bodySmall)
        listOf("Off", "Ruler through origin", "Parallel ruler through stroke start", "Rotated grid points", "Radial line through stroke start", "Concentric circle through stroke start").forEachIndexed { index, name ->
            TextButton(enabled = editable, onClick = { mode = index; view.assistantMode = index.toUInt(); view.invalidate() }) { Text(if (mode == index) "● $name" else name) }
        }
        if (mode != 0) {
            Text("Snap strength ${(strength * 100).toInt()}%", color = Color.White)
            Slider(strength, { strength = it; view.assistantStrength = it }, enabled = editable)
            Text("Origin X ${(x * 100).toInt()}%", color = Color.White)
            Slider(x, { x = it; view.assistantOriginX = it; view.invalidate() }, enabled = editable)
            Text("Origin Y ${(y * 100).toInt()}%", color = Color.White)
            Slider(y, { y = it; view.assistantOriginY = it; view.invalidate() }, enabled = editable)
            TextButton(enabled = editable, onClick = chooseOrigin) { Text("Set origin on canvas") }
            Text("Angle ${angle.toInt()}°", color = Color.White)
            Slider(angle, { angle = it; view.assistantAngle = it; view.invalidate() }, valueRange = -180f..180f, enabled = editable)
            Text("Grid / reference-ring spacing ${spacing.toInt()} px", color = Color.White)
            Slider(spacing, { spacing = it; view.assistantSpacing = it; view.invalidate() }, valueRange = 8f..512f, enabled = editable)
            Row(verticalAlignment = Alignment.CenterVertically) { Checkbox(guides, { guides = it; view.showAssistantGuides = it; view.invalidate() }); Text("Show assistant guides", color = Color.White) }
            Text("Parallel/radial/circle constraints use the first contact as their anchor. Grid snaps to intersections and may produce steps. Concentric reference rings show spacing; the active circle uses the stroke-start radius. Snap strength below 100% allows deviation. This is not Krita's complete assistant/plugin system.", color = Color.White, style = MaterialTheme.typography.bodySmall)
        }
    }
}
