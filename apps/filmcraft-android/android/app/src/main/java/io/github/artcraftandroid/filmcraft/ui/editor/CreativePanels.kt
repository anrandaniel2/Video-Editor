package io.github.artcraftandroid.filmcraft.ui.editor

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.coroutines.Dispatchers
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Path
import android.view.MotionEvent
import android.view.View
import androidx.compose.runtime.*
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.horizontalScroll
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.compose.foundation.Canvas as ComposeCanvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.Alignment
import androidx.compose.ui.draw.blur
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.animation.core.FastOutSlowInEasing
import io.github.artcraftandroid.filmcraft.ui.*
import java.io.File
import java.net.HttpURLConnection
import java.net.URI

/** Downloads only on explicit request, to project-local storage, with a bounded streaming copy. */
internal fun downloadSound(address: String, dir: File, video: Boolean = false): File {
    var url = URI(URI(address.trim()).toASCIIString()).toURL()
    dir.mkdirs()
    val temporary = File(dir, "${if (video) "video" else "sound"}-${java.util.UUID.randomUUID()}.part")
    try {
        repeat(6) {
            require(url.protocol == "https") { "Use a direct HTTPS audio link." }
            val connection = url.openConnection() as HttpURLConnection
            connection.connectTimeout = 15000
            connection.readTimeout = 30000
            connection.instanceFollowRedirects = false
            connection.setRequestProperty("User-Agent", ONLINE_MEDIA_AGENT)
            connection.setRequestProperty("Accept", "*/*")
            if (url.host == "mixkit.co" || url.host.endsWith(".mixkit.co")) connection.setRequestProperty("Referer", "https://mixkit.co/")
            try {
                val code = connection.responseCode
                if (code in 300..399) {
                    url = java.net.URL(url, connection.getHeaderField("Location") ?: error("Missing redirect location"))
                } else {
                    require(code in 200..299) { "Download failed (HTTP $code). ${if (code == 429) "The server is rate limiting requests; try again shortly." else "The media server refused the request. Try another result."}" }
                    val type = connection.contentType.orEmpty().substringBefore(';').lowercase()
                    val ext = if (video) when {
                        type.contains("webm") -> "webm"
                        type.contains("mp4") -> "mp4"
                        type.contains("quicktime") -> "mov"
                        else -> url.path.substringAfterLast('.', "").lowercase()
                    } else when {
                        type.contains("wav") -> "wav"
                        type.contains("ogg") -> "ogg"
                        type.contains("flac") -> "flac"
                        type.contains("mp4") || type.contains("aac") -> "m4a"
                        type.contains("mpeg") -> "mp3"
                        else -> url.path.substringAfterLast('.', "").lowercase()
                    }
                    require(ext in (if (video) listOf("webm", "mp4", "mov", "m4v") else listOf("wav", "mp3", "ogg", "flac", "m4a", "aac", "opus")) && !type.contains("text/html")) { "The service did not return a supported ${if (video) "video" else "audio"} file." }
                    require(connection.contentLengthLong <= (if (video) 250L else 100L) * 1024 * 1024) { "File exceeds the download size limit." }
                    connection.inputStream.use { input -> temporary.outputStream().use { output ->
                        val buffer = ByteArray(32768)
                        var total = 0L
                        while (true) {
                            val count = input.read(buffer)
                            if (count < 0) break
                            total += count
                            require(total <= (if (video) 250L else 100L) * 1024 * 1024) { "File exceeds the ${if (video) 250 else 100} MB limit." }
                            output.write(buffer, 0, count)
                        }
                        require(total > 0) { "The sound file was empty." }
                    } }
                    val out = File(dir, temporary.nameWithoutExtension + ".$ext")
                    check(temporary.renameTo(out)) { "Could not save sound." }
                    return out
                }
            } finally { connection.disconnect() }
        }
        error("Too many download redirects.")
    } finally { temporary.delete() }
}

@Composable
internal fun ColorPicker(label: String, color: Int, change: (Int) -> Unit) {
    var expanded by remember { mutableStateOf(false) }
    Row(Modifier.fillMaxWidth().padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(label, modifier = Modifier.weight(1f))
        Box(Modifier.size(36.dp).clip(RoundedCornerShape(8.dp)).background(Color(color))
            .border(1.dp, Color.White.copy(alpha = .7f), RoundedCornerShape(8.dp))
            .semantics { contentDescription = label }
            .clickable { expanded = true })
    }
    if (expanded) Dialog(onDismissRequest = { expanded = false }, properties = DialogProperties(decorFitsSystemWindows = false)) {
        ImmersiveDialog()
        Column(Modifier.widthIn(max = 340.dp).editorGlass(24.dp, layer = 4f).padding(20.dp), horizontalAlignment = Alignment.CenterHorizontally) {
            Text(label, color = Color.White)
            Spacer(Modifier.height(12.dp))
            ProcraftColorPicker(color, change)
            FcIconButton(LucideIcons.X.glyph(), "Close color picker", { expanded = false })
        }
    }
}

@Composable
internal fun DrawingPanel(ed: EditorController, close: () -> Unit) {
    MaterialTheme(colorScheme = MaterialTheme.colorScheme.copy(
        primary = Color.White, onPrimary = Color.Black, secondary = Color.White,
        onSurface = Color.White, onSurfaceVariant = Color.White.copy(alpha = .75f),
    )) { ProcraftDrawingPanel(ed, close) }
}
