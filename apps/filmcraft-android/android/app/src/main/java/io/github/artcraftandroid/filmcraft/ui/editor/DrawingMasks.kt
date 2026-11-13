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
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

@Composable
internal fun DrawingMaskPanel(view: DrawingView, paint: (Boolean) -> Unit) {
    var feather by remember { mutableFloatStateOf(8f) }
    var message by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    val hasMask = view.hasLayerMask(view.layer)
    val editable = !view.operationBusy && !view.layerLocked(view.layer) && view.transformMode == null
    fun edit(operation: String) { scope.launch {
        try { message = null; view.editLayerMask(operation, feather.toUInt()); message = "Mask updated. Undo restores the previous mask." }
        catch (e: CancellationException) { throw e }
        catch (e: Exception) { message = e.message ?: "Mask operation failed." }
    } }
    Column(Modifier.heightIn(max = 380.dp).verticalScroll(rememberScrollState())) {
        Text("One grayscale transparency mask per paint layer. White reveals, black hides, gray partially reveals. Layer pixels remain unchanged until you bake the mask.", color = Color.White, style = MaterialTheme.typography.bodySmall)
        message?.let { Text(it, color = Color.White) }
        if (view.operationBusy) LinearProgressIndicator(Modifier.fillMaxWidth(), color = Color.White)
        if (!hasMask) {
            TextButton(enabled = editable, onClick = { try { view.createLayerMask(false) } catch (e: Exception) { message = e.message } }) { Text("Add reveal-all mask") }
            TextButton(enabled = editable && view.selection != null, onClick = { try { view.createLayerMask(true) } catch (e: Exception) { message = e.message } }) { Text("Add mask from selection") }
        } else {
            Row(verticalAlignment = Alignment.CenterVertically) { Checkbox(view.layerMaskEnabled(view.layer), { view.toggleLayerMask(it) }, enabled = !view.operationBusy); Text("Enable mask", color = Color.White) }
            Row {
                TextButton(enabled = editable, onClick = { paint(true) }) { Text("Paint reveal") }
                TextButton(enabled = editable, onClick = { paint(false) }) { Text("Paint hide") }
            }
            Text("Mask strokes commit on pen lift; the stroke guide is display-only. Brush size, opacity and stylus pressure control the mask stroke.", color = Color.White, style = MaterialTheme.typography.bodySmall)
            Row {
                TextButton(enabled = editable, onClick = { edit("invert") }) { Text("Invert") }
                TextButton(enabled = editable, onClick = { edit("threshold") }) { Text("Threshold") }
            }
            Text("Feather radius ${feather.toInt()} px", color = Color.White)
            Slider(feather, { feather = it }, valueRange = 1f..128f, enabled = editable)
            TextButton(enabled = editable, onClick = { edit("feather") }) { Text("Feather mask") }
            Row {
                TextButton(enabled = editable, onClick = { edit("white") }) { Text("Reveal all") }
                TextButton(enabled = editable, onClick = { edit("black") }) { Text("Hide all") }
            }
            TextButton(enabled = editable && view.selection != null, onClick = { try { view.createLayerMask(true) } catch (e: Exception) { message = e.message } }) { Text("Replace mask from selection") }
            TextButton(enabled = editable && view.layerMaskEnabled(view.layer), onClick = { scope.launch {
                try { message = null; view.bakeLayerMask(); message = "Mask baked into layer alpha. Undo restores pixels and mask." }
                catch (e: CancellationException) { throw e }
                catch (e: Exception) { message = e.message ?: "Could not bake mask." }
            } }) { Text("Bake mask into layer pixels") }
            TextButton(enabled = editable, onClick = { view.removeLayerMask() }) { Text("Remove mask without changing pixels") }
        }
    }
}
