package io.github.artcraftandroid.filmcraft.ui.home

import android.graphics.Bitmap
import android.graphics.Canvas
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import io.github.artcraftandroid.filmcraft.ui.*

@Composable
internal fun DrawingCanvasDialog(close: () -> Unit, create: (Int, Int) -> Unit) {
    val host = LocalView.current
    val snapshot = remember {
        val w = host.width.coerceAtLeast(1); val h = host.height.coerceAtLeast(1)
        Bitmap.createBitmap(96, (96f * h / w).toInt().coerceAtLeast(1), Bitmap.Config.ARGB_8888).also { bitmap ->
            val canvas = Canvas(bitmap); canvas.scale(bitmap.width.toFloat() / w, bitmap.height.toFloat() / h); host.draw(canvas)
        }
    }
    val image = remember(snapshot) { snapshot.asImageBitmap() }
    val glass = rememberEditorGlass(image)
    DisposableEffect(Unit) { onDispose { snapshot.recycle() } }
    var width by remember { mutableStateOf("1080") }
    var height by remember { mutableStateOf("1080") }
    val w = width.toIntOrNull() ?: 0; val h = height.toIntOrNull() ?: 0
    val valid = w in 64..4096 && h in 64..4096 && w.toLong() * h <= 4_194_304
    Dialog(onDismissRequest = close, properties = DialogProperties(decorFitsSystemWindows = false)) {
        ImmersiveDialog()
        CompositionLocalProvider(LocalEditorGlass provides glass) {
            Column(Modifier.fillMaxWidth().editorGlass(24.dp).padding(20.dp).verticalScroll(rememberScrollState())) {
                Text("Create drawing", style = MaterialTheme.typography.titleLarge)
                Text("Canvas resolution", style = MaterialTheme.typography.bodyMedium)
                listOf("Small square" to (512 to 512), "Square" to (1080 to 1080), "Portrait" to (1080 to 1920), "Landscape" to (1920 to 1080), "2K square" to (2048 to 2048)).forEach { (name, size) ->
                    TextButton(onClick = { width = size.first.toString(); height = size.second.toString() }) { Text("$name · ${size.first} × ${size.second}") }
                }
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    OutlinedTextField(width, { width = it.filter(Char::isDigit).take(4) }, label = { Text("Width") }, singleLine = true, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number), modifier = Modifier.weight(1f))
                    OutlinedTextField(height, { height = it.filter(Char::isDigit).take(4) }, label = { Text("Height") }, singleLine = true, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number), modifier = Modifier.weight(1f))
                }
                Text("64–4096 px per side, up to 4 megapixels. Your chosen resolution is preserved.", style = MaterialTheme.typography.bodySmall)
                Row(Modifier.align(Alignment.End)) {
                    TextButton(onClick = close) { Text("Cancel") }
                    TextButton(onClick = { create(w, h) }, enabled = valid) { Text("Create") }
                }
            }
        }
    }
}
