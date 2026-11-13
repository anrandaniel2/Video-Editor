package io.github.artcraftandroid.filmcraft.ui.editor

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

/** Android only supplies file permissions and controls. ZIP, XML, PNG and layer mapping are native. */
@Composable
internal fun DrawingOpenRasterPanel(view: DrawingView) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var busy by remember { mutableStateOf(false) }
    var message by remember { mutableStateOf<String?>(null) }
    var name by remember { mutableStateOf("Drawing") }
    var pending by remember { mutableStateOf<android.net.Uri?>(null) }
    val save = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("image/openraster")) { uri ->
        if (uri != null) scope.launch {
            busy = true
            try {
                val data = view.encodeOpenRaster()
                writeDrawingResource(context, uri, data)
                message = "OpenRaster file saved."
            } catch (e: CancellationException) { throw e }
            catch (e: Exception) { message = e.message ?: "OpenRaster export failed." }
            finally { busy = false }
        }
    }
    val open = rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri -> if (uri != null) pending = uri }
    val enabled = !busy && !view.operationBusy && !view.animationPlaying && view.transformMode == null
    Column(Modifier.heightIn(max = 370.dp).verticalScroll(rememberScrollState())) {
        Text("OpenRaster (.ora) exchanges layered raster paintings with applications that support the format, including Krita. It is not a Krita .kra document.", color = Color.White)
        Text("Preserves flat paint-layer pixels, order, names, visibility, opacity, supported SVG blend modes and active-layer selection. Layers with offsets are positioned into the imported canvas.", color = Color.White, style = MaterialTheme.typography.bodySmall)
        OutlinedTextField(name, { name = it.take(80) }, enabled = enabled, singleLine = true, label = { Text("File name") }, modifier = Modifier.fillMaxWidth())
        message?.let { Text(it, color = Color.White) }
        if (busy || view.operationBusy) LinearProgressIndicator(Modifier.fillMaxWidth(), color = Color.White)
        Row {
            TextButton(enabled = enabled && name.isNotBlank(), onClick = {
                view.commitShapeDraft()
                val safe = name.trim().map { if (it in "\\/:*?\"<>|") '_' else it }.joinToString("").removeSuffix(".ora")
                save.launch("$safe.ora")
            }) { Text("Export layered .ora…") }
            TextButton(enabled = enabled, onClick = { open.launch("*/*") }) { Text("Import .ora…") }
        }
        pending?.let { uri ->
            Text("Import replaces this canvas, clears pixel undo and detaches its flipbook. Save current work first. Invalid or unsupported files leave the canvas unchanged.", color = Color.White)
            Row {
                TextButton(onClick = { pending = null }) { Text("Cancel") }
                TextButton(enabled = enabled, onClick = { pending = null; scope.launch {
                    busy = true
                    try {
                        val data = readDrawingResource(context, uri, 97 * 1024 * 1024)
                        view.openOpenRaster(data)
                        message = "OpenRaster layers imported."
                    } catch (e: CancellationException) { throw e }
                    catch (e: Exception) { message = e.message ?: "OpenRaster import failed." }
                    finally { busy = false }
                } }) { Text("Replace canvas") }
            }
        }
        Text("Limits: 16 layers, 4,194,304 canvas pixels and 96 MiB decoded layer storage. Nested groups, masks, clipping, lock metadata and unsupported blends are rejected rather than flattened silently. Import is 8-bit; 16-bit layers are rejected. Embedded colour profiles are not applied: do not use this interchange for colour-critical profile conversion. No text/vector/keyframe metadata or HDR preservation.", color = Color.White, style = MaterialTheme.typography.bodySmall)
    }
}
