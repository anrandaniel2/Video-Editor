package io.github.artcraftandroid.filmcraft.ui.photocraft

import android.content.Context
import android.graphics.Bitmap
import android.graphics.ImageDecoder
import android.net.Uri
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import io.github.artcraftandroid.filmcraft.core.PhotoHistogram
import io.github.artcraftandroid.filmcraft.core.photoAutoSettings
import io.github.artcraftandroid.filmcraft.core.photoFullCrop
import io.github.artcraftandroid.filmcraft.core.photoHistogram
import io.github.artcraftandroid.filmcraft.core.photoRender
import io.github.artcraftandroid.filmcraft.core.photoTransform
import io.github.artcraftandroid.filmcraft.engine.Projects
import java.util.Locale
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt
import kotlin.math.sqrt
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * One photo open in PhotoCraft: the decoded picture, the edit in progress, its undo history and the
 * developed preview. Every pixel operation is the native pipeline (`photo.rs`) on a background
 * dispatcher, so the sliders stay live on a phone-sized picture while the saved photo is developed
 * from the full working copy with exactly the same settings.
 */
@Stable
class PhotoCraftController(
    private val context: Context,
    private val scope: CoroutineScope,
    val uri: Uri,
    private val onMessage: (String) -> Unit,
    private val onSaved: (Uri) -> Unit,
) {
    private companion object {
        /** The preview's longest side: enough for a sharp phone screen without a slow render. */
        const val PREVIEW_SIDE = 1440

        /** The working copy's budget, matching the native pipeline's 8 MP limit. */
        const val WORKING_PIXELS = 8_000_000.0

        /** Filter thumbnails are developed from this small copy. */
        const val THUMB_SIDE = 132
    }

    var name by mutableStateOf("Photo")
        private set
    var loading by mutableStateOf(true)
        private set
    var error by mutableStateOf<String?>(null)
        private set
    var saving by mutableStateOf(false)
        private set
    var saved by mutableStateOf(false)
        private set

    /** The developed preview at the crop frame's or the full picture's size. */
    var preview by mutableStateOf<ImageBitmap?>(null)
        private set
    /** The turned picture's size, before any crop: the crop frame's coordinate space. */
    var baseWidth by mutableIntStateOf(1)
        private set
    var baseHeight by mutableIntStateOf(1)
        private set
    var histogram by mutableStateOf<PhotoHistogram?>(null)
        private set
    /** One small developed picture a filter, for the chips. */
    var thumbs by mutableStateOf<Map<Int, ImageBitmap>>(emptyMap())
        private set

    var edits by mutableStateOf(PhotoEdits())
        private set
    var undoDepth by mutableIntStateOf(0)
        private set
    var redoDepth by mutableIntStateOf(0)
        private set
    /** Hold to see the picture as it was opened. */
    var compare by mutableStateOf(false)
        private set
    /** The crop frame is on screen: the preview shows the whole turned picture, not the frame. */
    var cropMode by mutableStateOf(false)
        private set
    /** The locked crop aspect (width ÷ height), or null for a free frame. */
    var aspect by mutableStateOf<Float?>(null)
        private set

    val canUndo: Boolean get() = undoDepth > 0
    val canRedo: Boolean get() = redoDepth > 0
    val imageAspect: Float get() = imageAspectOf(baseWidth, baseHeight)

    private var full: Bitmap? = null
    private var previewPixels: ByteArray? = null
    private var previewSourceWidth = 1
    private var previewSourceHeight = 1
    private var originalPreview: ImageBitmap? = null
    private var thumbPixels: ByteArray? = null
    private var thumbWidth = 1
    private var thumbHeight = 1

    private var renderJob: Job? = null
    private var thumbJob: Job? = null
    private var gestureStart: PhotoEdits? = null
    private val undoStack = ArrayDeque<PhotoEdits>()
    private val redoStack = ArrayDeque<PhotoEdits>()

    private data class Turned(val width: Int, val height: Int, val pixels: ByteArray)

    private data class Frame(val image: ImageBitmap, val histogram: PhotoHistogram?)

    // ---- opening ----

    /** Decode the picture and draw the first preview. Call once, when the screen opens. */
    fun load() {
        scope.launch {
            try {
                val decoded = withContext(Dispatchers.IO) { decode(uri) }
                name = withContext(Dispatchers.IO) { runCatching { Projects.displayName(context.contentResolver, uri) }.getOrNull() ?: "Photo" }
                full = decoded
                withContext(Dispatchers.Default) {
                    val previewSource = scaled(decoded, PREVIEW_SIDE)
                    previewPixels = previewSource.rgbaBytes()
                    previewSourceWidth = previewSource.width
                    previewSourceHeight = previewSource.height
                    originalPreview = previewSource.asImageBitmap()
                    val thumb = scaled(decoded, THUMB_SIDE)
                    thumbPixels = thumb.rgbaBytes()
                    thumbWidth = thumb.width
                    thumbHeight = thumb.height
                    if (thumb !== decoded) thumb.recycle()
                }
                loading = false
                render(immediate = true)
                buildThumbs()
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                error = e.message ?: "This picture couldn't be opened."
                loading = false
            }
        }
    }

    private fun decode(source: Uri): Bitmap = ImageDecoder.decodeBitmap(ImageDecoder.createSource(context.contentResolver, source)) { decoder, info, _ ->
        decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
        decoder.isMutableRequired = false
        val width = info.size.width
        val height = info.size.height
        val scale = min(1.0, min(8192.0 / max(width, height), sqrt(WORKING_PIXELS / (width.toDouble() * height))))
        if (scale < 1.0) {
            decoder.setTargetSize((width * scale).toInt().coerceAtLeast(1), (height * scale).toInt().coerceAtLeast(1))
        }
    }

    private fun scaled(source: Bitmap, longestSide: Int): Bitmap {
        val longest = max(source.width, source.height)
        if (longest <= longestSide) return source
        val scale = longestSide.toFloat() / longest
        return Bitmap.createScaledBitmap(
            source,
            (source.width * scale).toInt().coerceAtLeast(1),
            (source.height * scale).toInt().coerceAtLeast(1),
            true,
        )
    }

    // ---- editing ----

    /** A slider moving or a crop handle dragged: the canvas follows, the history waits. */
    fun preview(change: (PhotoEdits) -> PhotoEdits) {
        if (gestureStart == null) gestureStart = edits
        val next = change(edits)
        if (next == edits) return
        val onlyCrop = next.copy(crop = edits.crop) == edits
        edits = next
        // While the crop frame is open the frame isn't part of the picture yet, so moving it needs
        // no render at all: the overlay is pure Compose.
        if (!(cropMode && onlyCrop)) render(immediate = false)
    }

    /** The finger lifted: one undo step for everything that changed during the gesture. */
    fun finishGesture() {
        val start = gestureStart ?: return
        gestureStart = null
        if (start == edits) return
        pushUndo(start)
        render(immediate = true)
        if (start.transformKey != edits.transformKey) buildThumbs()
    }

    /** A finished change (a filter tap, a turn, a reset): one undo step. */
    fun commit(change: (PhotoEdits) -> PhotoEdits) {
        val previous = edits
        val next = change(previous)
        if (next == previous) return
        pushUndo(previous)
        edits = next
        afterChange(previous, next)
    }

    fun undo() {
        val previous = undoStack.removeLastOrNull() ?: return
        val current = edits
        redoStack.addLast(current)
        edits = previous
        undoDepth = undoStack.size
        redoDepth = redoStack.size
        afterChange(current, previous)
    }

    fun redo() {
        val next = redoStack.removeLastOrNull() ?: return
        val current = edits
        undoStack.addLast(current)
        edits = next
        undoDepth = undoStack.size
        redoDepth = redoStack.size
        afterChange(current, next)
    }

    private fun pushUndo(previous: PhotoEdits) {
        undoStack.addLast(previous)
        if (undoStack.size > 40) undoStack.removeFirst()
        redoStack.clear()
        undoDepth = undoStack.size
        redoDepth = 0
    }

    private fun afterChange(previous: PhotoEdits, next: PhotoEdits) {
        render(immediate = true)
        if (previous.transformKey != next.transformKey) buildThumbs()
    }

    /** Turn the picture a quarter clockwise (or back); the crop frame is measured on the turned one. */
    fun rotate(clockwise: Boolean) = commit { it.copy(quarters = (it.quarters + if (clockwise) 1 else 3) % 4, crop = null) }.also {
        aspect = null
    }

    fun flip(horizontal: Boolean) = commit { current ->
        if (horizontal) current.copy(flipHorizontal = !current.flipHorizontal) else current.copy(flipVertical = !current.flipVertical)
    }

    // ---- crop ----

    /** The Crop tool opening or closing: closing applies the frame, opening shows the whole picture. */
    fun cropTool(active: Boolean) {
        if (cropMode == active) return
        cropMode = active
        if (active && edits.crop == null) edits = edits.copy(crop = centeredCrop(aspect, imageAspect))
        if (!active && edits.crop?.isFull == true) edits = edits.copy(crop = null)
        render(immediate = true)
    }

    fun lockAspect(value: Float?) {
        aspect = value
        if (value == null) return
        val frame = centeredCrop(value, imageAspect)
        commit { it.copy(crop = frame) }
    }

    fun setCrop(rect: NormRect) = preview { it.copy(crop = rect) }

    fun resetCrop() {
        aspect = null
        commit { it.copy(crop = null) }
    }

    // ---- one-tap work ----

    /** Even out the exposure and colour cast of the picture as it stands. */
    fun autoAdjust() {
        val pixels = previewPixels ?: return
        val snapshot = edits
        scope.launch {
            val settings = withContext(Dispatchers.Default) {
                runCatching {
                    val turned = turn(pixels, previewSourceWidth, previewSourceHeight, snapshot, snapshot.crop)
                    photoAutoSettings(turned.pixels, turned.width.toUInt(), turned.height.toUInt())
                }.getOrNull()
            } ?: return@launch
            commit { it.copy(adjustments = settings.toAdjustments()) }
        }
    }

    fun resetAdjustments() = commit { it.copy(adjustments = PhotoAdjustments()) }

    fun resetAll() = commit { PhotoEdits() }.also { aspect = null }

    fun compareWithOriginal(on: Boolean) {
        if (compare == on) return
        compare = on
        render(immediate = true)
    }

    // ---- saving ----

    /** Develop the full working picture with the current edits and publish it in Photos. */
    fun save() {
        if (saving) return
        val source = full ?: return
        saving = true
        scope.launch {
            try {
                val snapshot = edits
                val photo = withContext(Dispatchers.Default) {
                    val pixels = source.rgbaBytes()
                    val turned = turn(pixels, source.width, source.height, snapshot, snapshot.crop)
                    val rendered = photoRender(turned.pixels, turned.width.toUInt(), turned.height.toUInt(), snapshot.adjustments.toNative())
                    rgbaToBitmap(rendered, turned.width, turned.height)
                }
                val destination = withContext(Dispatchers.IO) { savePhotoToLibrary(context, photo, name) }
                photo.recycle()
                saved = true
                onMessage("Saved to Photos")
                onSaved(destination)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                onMessage(e.message ?: "The photo couldn't be saved.")
            } finally {
                saving = false
            }
        }
    }

    fun close() {
        renderJob?.cancel()
        thumbJob?.cancel()
        full = null
    }

    // ---- rendering ----

    private fun turn(pixels: ByteArray, width: Int, height: Int, snapshot: PhotoEdits, crop: NormRect?): Turned {
        val quarters = snapshot.quarters % 4
        if (quarters == 0 && !snapshot.flipHorizontal && !snapshot.flipVertical && (crop == null || crop.isFull)) {
            return Turned(width, height, pixels)
        }
        val turned = photoTransform(
            pixels,
            width.toUInt(),
            height.toUInt(),
            quarters.toUInt(),
            snapshot.flipHorizontal,
            snapshot.flipVertical,
            crop?.toNative() ?: photoFullCrop(),
        )
        return Turned(turned.width.toInt(), turned.height.toInt(), turned.rgba)
    }

    private fun render(immediate: Boolean) {
        val pixels = previewPixels ?: return
        val snapshot = edits
        val cropping = cropMode
        val showingOriginal = compare
        val original = originalPreview
        // The crop frame's coordinates are the turned picture's, whether or not the frame is shown.
        baseWidth = if (snapshot.quarters % 2 == 0) previewSourceWidth else previewSourceHeight
        baseHeight = if (snapshot.quarters % 2 == 0) previewSourceHeight else previewSourceWidth
        renderJob?.cancel()
        renderJob = scope.launch {
            if (!immediate) delay(20)
            if (showingOriginal) {
                preview = original
                histogram = null
                return@launch
            }
            val frame = withContext(Dispatchers.Default) {
                runCatching {
                    val turned = turn(pixels, previewSourceWidth, previewSourceHeight, snapshot, if (cropping) null else snapshot.crop)
                    val rendered = photoRender(turned.pixels, turned.width.toUInt(), turned.height.toUInt(), snapshot.adjustments.toNative())
                    val histogram = runCatching { photoHistogram(rendered, turned.width.toUInt(), turned.height.toUInt()) }.getOrNull()
                    Frame(rendered.toImageBitmap(turned.width, turned.height), histogram)
                }.getOrNull()
            } ?: return@launch
            preview = frame.image
            histogram = frame.histogram
        }
    }

    /** Develop every filter on a small copy once: the chips are pictures, not colour swatches. */
    private fun buildThumbs() {
        val pixels = thumbPixels ?: return
        val snapshot = edits
        thumbJob?.cancel()
        thumbJob = scope.launch {
            val map = withContext(Dispatchers.Default) {
                runCatching {
                    val turned = turn(pixels, thumbWidth, thumbHeight, snapshot, snapshot.crop)
                    PhotoFilter.values().mapNotNull { filter ->
                        val settings = PhotoAdjustments(filter = filter, filterAmount = 1f).toNative()
                        val developed = photoRender(turned.pixels, turned.width.toUInt(), turned.height.toUInt(), settings)
                        filter.id to developed.toImageBitmap(turned.width, turned.height)
                    }.toMap()
                }.getOrDefault(emptyMap())
            }
            thumbs = map
        }
    }
}

/** A slider's value as a signed percentage: what the labels beside the tracks read. */
internal fun signedPercent(value: Float): String {
    val amount = (value * 100).roundToInt()
    return if (amount > 0) "+$amount" else "$amount"
}

/** Exposure in stops, the unit photographers expect. */
internal fun exposureText(value: Float): String = String.format(Locale.ROOT, "%+.1f EV", value * 1.5f)
