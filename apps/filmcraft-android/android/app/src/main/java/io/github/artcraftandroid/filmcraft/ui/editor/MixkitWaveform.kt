package io.github.artcraftandroid.filmcraft.ui.editor

import android.media.*
import android.os.SystemClock
import android.util.LruCache
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.*
import kotlinx.coroutines.sync.Semaphore
import kotlinx.coroutines.sync.withPermit
import java.nio.ByteOrder
import kotlin.math.*

/** Bounded PCM decoding of the published preview, rather than decorative random bars. */
private object PreviewPeaks {
    val slots = Semaphore(2)
    val cache = LruCache<String, FloatArray>(64)
    suspend fun load(url: String): FloatArray = withContext(Dispatchers.IO) {
        cache.get(url)?.let { return@withContext it }
        slots.withPermit {
            cache.get(url)?.let { return@withPermit it }
            val extractor = MediaExtractor()
            var codec: MediaCodec? = null
            try {
                extractor.setDataSource(url, onlineMediaHeaders + ("Referer" to "https://mixkit.co/"))
                val track = (0 until extractor.trackCount).firstOrNull { extractor.getTrackFormat(it).getString(MediaFormat.KEY_MIME)?.startsWith("audio/") == true }
                    ?: error("No audio stream")
                extractor.selectTrack(track)
                val format = extractor.getTrackFormat(track)
                val duration = format.getLong(MediaFormat.KEY_DURATION)
                require(duration in 1..300_000_000L) { "Preview is too long to analyse" }
                var channels = format.getInteger(MediaFormat.KEY_CHANNEL_COUNT).coerceAtLeast(1)
                var rate = format.getInteger(MediaFormat.KEY_SAMPLE_RATE).coerceAtLeast(1)
                format.setInteger(MediaFormat.KEY_PCM_ENCODING, AudioFormat.ENCODING_PCM_16BIT)
                val decoder = MediaCodec.createDecoderByType(format.getString(MediaFormat.KEY_MIME)!!)
                codec = decoder; decoder.configure(format, null, null, 0); decoder.start()
                val peaks = FloatArray(96)
                val info = MediaCodec.BufferInfo()
                var inputEnded = false; var outputEnded = false
                val started = SystemClock.elapsedRealtime()
                while (!outputEnded) {
                    currentCoroutineContext().ensureActive()
                    require(SystemClock.elapsedRealtime() - started < 45000) { "Preview analysis timed out" }
                    if (!inputEnded) {
                        val index = decoder.dequeueInputBuffer(10000)
                        if (index >= 0) {
                            val input = decoder.getInputBuffer(index)!!; input.clear()
                            val n = extractor.readSampleData(input, 0)
                            if (n < 0) { decoder.queueInputBuffer(index, 0, 0, 0, MediaCodec.BUFFER_FLAG_END_OF_STREAM); inputEnded = true }
                            else { decoder.queueInputBuffer(index, 0, n, extractor.sampleTime, 0); extractor.advance() }
                        }
                    }
                    val index = decoder.dequeueOutputBuffer(info, 10000)
                    if (index == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED) {
                        val output = decoder.outputFormat
                        channels = output.getInteger(MediaFormat.KEY_CHANNEL_COUNT).coerceAtLeast(1)
                        rate = output.getInteger(MediaFormat.KEY_SAMPLE_RATE).coerceAtLeast(1)
                        require(!output.containsKey(MediaFormat.KEY_PCM_ENCODING) || output.getInteger(MediaFormat.KEY_PCM_ENCODING) == AudioFormat.ENCODING_PCM_16BIT)
                    } else if (index >= 0) {
                        try {
                            val output = decoder.getOutputBuffer(index)!!.order(ByteOrder.LITTLE_ENDIAN)
                            output.position(info.offset); output.limit(info.offset + info.size)
                            var sample = 0L
                            while (output.remaining() >= 2) {
                                val value = abs(output.short.toInt()) / 32768f
                                val time = info.presentationTimeUs + sample / channels * 1_000_000L / rate
                                val bin = (time.toDouble() / duration * peaks.size).toInt().coerceIn(0, peaks.lastIndex)
                                peaks[bin] = max(peaks[bin], value); sample++
                            }
                            outputEnded = info.flags and MediaCodec.BUFFER_FLAG_END_OF_STREAM != 0
                        } finally { decoder.releaseOutputBuffer(index, false) }
                    }
                }
                val max = peaks.maxOrNull()?.coerceAtLeast(.001f) ?: 1f
                FloatArray(peaks.size) { sqrt(peaks[it] / max) }.also { cache.put(url, it) }
            } finally {
                runCatching { codec?.stop() }; runCatching { codec?.release() }; runCatching { extractor.release() }
            }
        }
    }
}

@Composable
internal fun MixkitWaveform(asset: MixkitAsset, progress: Float, modifier: Modifier = Modifier) {
    var peaks by remember(asset.preview) { mutableStateOf<FloatArray?>(null) }
    var loading by remember(asset.preview) { mutableStateOf(true) }
    LaunchedEffect(asset.preview) {
        try { if (asset.preview.isNotBlank()) peaks = PreviewPeaks.load(asset.preview) }
        catch (e: CancellationException) { throw e }
        catch (_: Exception) { peaks = null }
        finally { loading = false }
    }
    Box(modifier.height(56.dp).semantics { contentDescription = "Audio waveform, ${(progress * 100).toInt()} percent played" }) {
        val data = peaks
        if (data != null) Canvas(Modifier.fillMaxSize()) {
            val step = size.width / data.size
            data.forEachIndexed { i, peak ->
                val x = (i + .5f) * step; val half = (peak * size.height * .45f).coerceAtLeast(1f)
                drawLine(Color.White.copy(alpha = if (x <= progress * size.width) 1f else .5f), Offset(x, size.height / 2 - half), Offset(x, size.height / 2 + half), (step * .55f).coerceAtLeast(1f))
            }
            if (progress > 0f) drawLine(Color.White, Offset(progress * size.width, 0f), Offset(progress * size.width, size.height), 2.dp.toPx())
        } else Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.Center) {
            if (loading) LinearProgressIndicator(Modifier.fillMaxWidth(), color = Color.White, trackColor = Color.White.copy(alpha = .15f))
            Text(if (loading) "Reading waveform…" else "Waveform unavailable", color = Color.White.copy(alpha = .65f), style = MaterialTheme.typography.labelSmall)
        }
    }
}
