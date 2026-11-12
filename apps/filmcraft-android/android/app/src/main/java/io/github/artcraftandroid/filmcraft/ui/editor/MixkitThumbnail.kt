package io.github.artcraftandroid.filmcraft.ui.editor

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.media.MediaMetadataRetriever
import android.util.LruCache
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.coroutines.sync.Semaphore
import kotlinx.coroutines.sync.withPermit
import java.net.HttpURLConnection
import java.net.URI

private object MixkitThumbnails {
    val slots = Semaphore(2)
    val images = object : LruCache<String, Bitmap>(8 * 1024 * 1024) {
        override fun sizeOf(key: String, value: Bitmap) = value.allocationByteCount
    }
    suspend fun load(asset: MixkitAsset): Bitmap? = withContext(Dispatchers.IO) {
        images.get(asset.id)?.let { return@withContext it }
        slots.withPermit {
            images.get(asset.id)?.let { return@withPermit it }
            var bitmap: Bitmap? = null
            if (asset.thumbnail.isNotBlank()) try {
                val c = URI(asset.thumbnail).toURL().openConnection() as HttpURLConnection
                c.connectTimeout = 10000; c.readTimeout = 10000; c.instanceFollowRedirects = false
                c.setRequestProperty("User-Agent", ONLINE_MEDIA_AGENT)
                try {
                    if (c.responseCode == 200) {
                        val bytes = c.inputStream.use { input ->
                            val out = java.io.ByteArrayOutputStream(); val buffer = ByteArray(8192)
                            while (true) { val n = input.read(buffer); if (n < 0) break; require(out.size() + n <= 2 * 1024 * 1024); out.write(buffer, 0, n) }
                            out.toByteArray()
                        }
                        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
                        BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
                        var sample = 1
                        while (bounds.outWidth / sample > 640 || bounds.outHeight / sample > 640) sample *= 2
                        bitmap = BitmapFactory.decodeByteArray(bytes, 0, bytes.size, BitmapFactory.Options().apply { inSampleSize = sample })
                    }
                } finally { c.disconnect() }
            } catch (e: CancellationException) { throw e } catch (_: Exception) { }
            if (bitmap == null && asset.preview.isNotBlank()) {
                val retriever = MediaMetadataRetriever()
                try {
                    retriever.setDataSource(asset.preview, onlineMediaHeaders + ("Referer" to "https://mixkit.co/"))
                    bitmap = retriever.getScaledFrameAtTime(0, MediaMetadataRetriever.OPTION_CLOSEST_SYNC, 480, 270)
                } catch (e: CancellationException) { throw e } catch (_: Exception) { }
                finally { retriever.release() }
            }
            bitmap?.let { images.put(asset.id, it) }
            bitmap
        }
    }
}

@Composable
internal fun MixkitThumbnail(asset: MixkitAsset) {
    var bitmap by remember(asset.id) { mutableStateOf<Bitmap?>(null) }
    var loading by remember(asset.id) { mutableStateOf(true) }
    LaunchedEffect(asset.id) { try { bitmap = MixkitThumbnails.load(asset) } finally { loading = false } }
    Box(Modifier.fillMaxWidth().aspectRatio(16f / 9f).background(Color.Black.copy(alpha = .15f)), contentAlignment = Alignment.Center) {
        val image = bitmap
        if (image != null) Image(image.asImageBitmap(), asset.title, Modifier.fillMaxSize(), contentScale = ContentScale.Crop)
        else if (loading) CircularProgressIndicator(Modifier.size(24.dp), strokeWidth = 2.dp)
        else Text("Preview image unavailable", style = MaterialTheme.typography.bodySmall)
    }
}
