package io.github.artcraftandroid.filmcraft.ui.editor

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
internal fun DrawingAffineControls(view: DrawingView) {
    var sx by remember { mutableFloatStateOf(1f) }; var sy by remember { mutableFloatStateOf(1f) }
    var angle by remember { mutableFloatStateOf(0f) }; var skewX by remember { mutableFloatStateOf(0f) }; var skewY by remember { mutableFloatStateOf(0f) }
    var flipX by remember { mutableStateOf(false) }; var flipY by remember { mutableStateOf(false) }
    var px by remember { mutableFloatStateOf(.5f) }; var py by remember { mutableFloatStateOf(.5f) }
    var linked by remember { mutableStateOf(true) }
    fun update() = view.setAffine(sx, sy, angle, skewX, skewY, flipX, flipY, px, py)
    Column(Modifier.heightIn(max = 280.dp).verticalScroll(rememberScrollState())) {
        Text("Drag canvas contents to translate. These settings transform the current layer/selection, not the camera.", color = Color.White, style = MaterialTheme.typography.bodySmall)
        view.affineError?.let { Text(it, color = Color.White) }
        Row(verticalAlignment = Alignment.CenterVertically) { Checkbox(linked, { linked = it }); Text("Link X/Y scale", color = Color.White) }
        Text("Scale X ${(sx * 100).toInt()}%", color = Color.White)
        Slider(sx, { sx = it; if (linked) sy = it; update() }, valueRange = .1f..4f)
        if (!linked) { Text("Scale Y ${(sy * 100).toInt()}%", color = Color.White); Slider(sy, { sy = it; update() }, valueRange = .1f..4f) }
        Text("Rotation ${angle.toInt()}°", color = Color.White); Slider(angle, { angle = it; update() }, valueRange = -180f..180f)
        Text("Skew X ${skewX.toInt()}°", color = Color.White); Slider(skewX, { skewX = it; update() }, valueRange = -60f..60f)
        Text("Skew Y ${skewY.toInt()}°", color = Color.White); Slider(skewY, { skewY = it; update() }, valueRange = -60f..60f)
        Row(verticalAlignment = Alignment.CenterVertically) { Checkbox(flipX, { flipX = it; update() }); Text("Flip X", color = Color.White); Checkbox(flipY, { flipY = it; update() }); Text("Flip Y", color = Color.White) }
        Text("Pivot X ${(px * 100).toInt()}%", color = Color.White); Slider(px, { px = it; update() })
        Text("Pivot Y ${(py * 100).toInt()}%", color = Color.White); Slider(py, { py = it; update() })
        TextButton(onClick = { sx = 1f; sy = 1f; angle = 0f; skewX = 0f; skewY = 0f; flipX = false; flipY = false; px = .5f; py = .5f; update() }) { Text("Reset transform settings") }
    }
}
