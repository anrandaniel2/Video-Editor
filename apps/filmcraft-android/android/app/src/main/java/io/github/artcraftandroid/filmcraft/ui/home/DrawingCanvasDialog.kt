package io.github.artcraftandroid.filmcraft.ui.home

import android.graphics.Bitmap
import android.graphics.Canvas
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
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
            Column(Modifier.fillMaxWidth().editorGlass(28.dp).padding(24.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text("New canvas", style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.SemiBold)
                        Text("A little space for your next idea", style = MaterialTheme.typography.bodySmall, color = Color.White.copy(alpha = .65f))
                    }
                    IconButton(onClick = close) { GlyphIcon(LucideIcons.X.glyph(), Color.White, description = "Close") }
                }
                Box(Modifier.fillMaxWidth().height(112.dp).clip(RoundedCornerShape(18.dp)).background(Color.White.copy(alpha = .04f)), contentAlignment = Alignment.Center) {
                    val ratio = if (w > 0 && h > 0) w.toFloat() / h else 1f
                    Box(Modifier.height(if (ratio >= 1) (80 / ratio.coerceAtMost(4f)).dp else 80.dp).aspectRatio(ratio.coerceIn(.25f, 4f)).background(Color.White, RoundedCornerShape(4.dp)))
                }
                Text("CHOOSE A PRESET", style = MaterialTheme.typography.labelSmall, color = Color.White.copy(alpha = .6f))
                val presets = listOf("Sketch" to (512 to 512), "Square" to (1080 to 1080), "Portrait" to (1080 to 1920), "Landscape" to (1920 to 1080), "2K square" to (2048 to 2048))
                presets.chunked(2).forEach { row ->
                    Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        row.forEach { (name, size) ->
                            val selected = w == size.first && h == size.second
                            Column(Modifier.weight(1f).clip(RoundedCornerShape(14.dp)).background(Color.White.copy(alpha = if (selected) .16f else .04f))
                                .border(1.dp, Color.White.copy(alpha = if (selected) .75f else .12f), RoundedCornerShape(14.dp))
                                .clickable { width = size.first.toString(); height = size.second.toString() }.padding(12.dp)) {
                                Text(name, style = MaterialTheme.typography.labelLarge)
                                Text("${size.first} × ${size.second}", style = MaterialTheme.typography.bodySmall, color = Color.White.copy(alpha = .65f))
                            }
                        }
                        if (row.size == 1) Spacer(Modifier.weight(1f))
                    }
                }
                Text("CUSTOM SIZE", style = MaterialTheme.typography.labelSmall, color = Color.White.copy(alpha = .6f))
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    OutlinedTextField(width, { width = it.filter(Char::isDigit).take(4) }, label = { Text("Width · px") }, singleLine = true, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number), modifier = Modifier.weight(1f), shape = RoundedCornerShape(12.dp))
                    IconButton(onClick = { val old = width; width = height; height = old }) { GlyphIcon(LucideIcons.RotateCcw.glyph(), Color.White, description = "Swap dimensions") }
                    OutlinedTextField(height, { height = it.filter(Char::isDigit).take(4) }, label = { Text("Height · px") }, singleLine = true, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number), modifier = Modifier.weight(1f), shape = RoundedCornerShape(12.dp))
                }
                Text(if (valid) "${"%.1f".format(w.toLong() * h / 1_000_000.0)} MP · transparent canvas" else "Use 64–4096 px per side, up to 4 megapixels.", style = MaterialTheme.typography.bodySmall, color = if (valid) Color.White.copy(alpha = .65f) else MaterialTheme.colorScheme.error)
                Button(onClick = { create(w, h) }, enabled = valid, modifier = Modifier.fillMaxWidth().height(48.dp), shape = RoundedCornerShape(14.dp)) { Text("Create canvas") }
            }
        }
    }
}
