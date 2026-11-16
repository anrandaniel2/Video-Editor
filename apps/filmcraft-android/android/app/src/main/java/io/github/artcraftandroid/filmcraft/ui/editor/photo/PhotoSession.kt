package io.github.artcraftandroid.filmcraft.ui.editor.photo

import android.graphics.Bitmap
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import io.github.artcraftandroid.filmcraft.core.DrawingAdjustments
import io.github.artcraftandroid.filmcraft.core.DrawingNativeBrush
import io.github.artcraftandroid.filmcraft.core.DrawingStrokePoint
import io.github.artcraftandroid.filmcraft.core.PhotoDocument
import io.github.artcraftandroid.filmcraft.core.PhotoInfo
import io.github.artcraftandroid.filmcraft.core.photoOpen
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlin.math.roundToInt

/**
 * The photo editor's state.
 *
 * Holds the Rust [PhotoDocument] and the small amount of UI state around it: which tool is open,
 * where the canvas is, what the sliders say. Every pixel it shows and every change it makes goes
 * through the engine — there is no second implementation of an adjustment or a stroke here.
 *
 * A read of the picture is deliberately narrow. [loadRegion] asks for the part being displayed at
 * the canvas' own size, and a slider's preview asks for one thumbnail, so neither scales with the
 * size of the file.
 */
internal class PhotoSession(private val scope: CoroutineScope) {

    var document: PhotoDocument? by mutableStateOf(null)
        private set
    var info: PhotoInfo? by mutableStateOf(null)
        private set
    var status: String? by mutableStateOf("Opening the photo…")
        private set

    /** What the canvas draws: the region Rust just rendered, at the display's own pixel size. */
    var region: Bitmap? by mutableStateOf(null)
        private set

    /**
     * The picture with the adjustments the sliders currently hold, at thumbnail cost.
     *
     * Non-null only while a slider is being dragged. The canvas draws this instead of [region], so
     * the thumb sees the change immediately without the engine having to re-render the document.
     * Lifting the finger commits the stack and clears this.
     */
    var preview: Bitmap? by mutableStateOf(null)
        private set

    private var previewJob: kotlinx.coroutines.Job? = null

    var tool: PhotoTool by mutableStateOf(PhotoTool.Adjust)
        private set

    var brushSize: Float by mutableStateOf(24f)
        private set
    var brushOpacity: Float by mutableStateOf(1f)
        private set
    var brushHardness: Float by mutableStateOf(0.85f)
        private set

    /** Which adjustment the bottom slider and the Adjust panel are changing. */
    var activeAdjustment: Adjustment by mutableStateOf(Adjustment.Exposure)
        private set

    /** The adjustments as the sliders have them. Sent to Rust as one record, so the engine does the maths. */
    var values: Map<Adjustment, Float> by mutableStateOf(Adjustment.entries.associateWith { it.neutral })
        private set

    private var displaySize = Size.Zero
    private val stroke = mutableListOf<DrawingStrokePoint>()

    // ---- opening ----

    /** Open a picture. It is already decoded: [bitmap] is the host's, from any format it supports. */
    fun open(bitmap: Bitmap) {
        scope.launch {
            try {
                val opened = withContext(Dispatchers.Default) {
                    val pixels = IntArray(bitmap.width * bitmap.height)
                    bitmap.getPixels(pixels, 0, bitmap.width, 0, 0, bitmap.width, bitmap.height)
                    // PhotoDocument wants RGBA bytes; the bitmap gives ARGB ints.
                    val rgba = ByteArray(pixels.size * 4)
                    for (i in pixels.indices) {
                        val p = pixels[i]
                        val at = i * 4
                        rgba[at] = (p shr 16).toByte()
                        rgba[at + 1] = (p shr 8).toByte()
                        rgba[at + 2] = p.toByte()
                        rgba[at + 3] = (p ushr 24).toByte()
                    }
                    photoOpen(bitmap.width.toUInt(), bitmap.height.toUInt(), rgba)
                }
                document = opened
                info = opened.info()
                status = null
                loadRegion()
            } catch (e: Exception) {
                status = e.message ?: "The photo couldn't be opened."
            }
        }
    }

    // ---- drawing what is on the glass ----

    /**
     * Ask Rust for the picture at the canvas' size and keep the bytes as a bitmap.
     *
     * Called when the canvas is laid out and after every change. The region is the whole picture,
     * but scaled to the display: an 8000-pixel photo asked at 400 pixels is 400 pixels of work.
     */
    fun loadRegion() {
        val photo = document ?: return
        val shown = displaySize
        if (shown.width < 1f || shown.height < 1f) return
        scope.launch {
            try {
                val next = withContext(Dispatchers.Default) {
                    // Read at the size it is drawn. A 48-megapixel file on a 400-pixel canvas is
                    // 400 pixels of work, which is why the canvas keeps up while a slider moves.
                    // Rust does the scaling and says what size it returned.
                    val pixels = photo.fitToBox(
                        shown.width.roundToInt().coerceIn(1, 2048).toUInt(),
                        shown.height.roundToInt().coerceIn(1, 2048).toUInt(),
                        0f,
                    )
                    bitmapOf(pixels.rgba, pixels.width, pixels.height)
                }
                region = next
                info = photo.info()
            } catch (e: Exception) {
                status = e.message ?: "The picture couldn't be drawn."
            }
        }
    }

    private fun bitmapOf(bytes: ByteArray, width: UInt, height: UInt): Bitmap {
        val bitmap = Bitmap.createBitmap(width.toInt(), height.toInt(), Bitmap.Config.ARGB_8888)
        val pixels = IntArray(width.toInt() * height.toInt())
        for (i in pixels.indices) {
            val at = i * 4
            val r = bytes[at].toInt() and 0xFF
            val g = bytes[at + 1].toInt() and 0xFF
            val b = bytes[at + 2].toInt() and 0xFF
            val a = bytes[at + 3].toInt() and 0xFF
            pixels[i] = (a shl 24) or (r shl 16) or (g shl 8) or b
        }
        bitmap.setPixels(pixels, 0, width.toInt(), 0, 0, width.toInt(), height.toInt())
        return bitmap
    }

    /** The canvas telling the session how big it is, so a region is asked at the size it is drawn. */
    fun canvasSize(size: Size) {
        if (size == displaySize) return
        displaySize = size
        if (document != null) loadRegion()
    }

    // ---- tools ----

    fun choose(next: PhotoTool) {
        tool = next
        // Choosing a tool shows what it does; keeping the last one's changes would be a surprise.
        if (next == PhotoTool.Layers) info?.let { info = document?.info() }
    }

    fun setBrushSize(value: Float) { brushSize = value.coerceIn(1f, 200f) }
    fun setBrushOpacity(value: Float) { brushOpacity = value.coerceIn(0f, 1f) }
    fun setBrushHardness(value: Float) { brushHardness = value.coerceIn(0f, 1f) }

    fun selectAdjustment(adjustment: Adjustment) { activeAdjustment = adjustment }

    /**
     * A slider moved.
     *
     * The value is kept and a preview is asked for — a thumbnail with the new stack, not the
     * document. A new drag cancels the previous preview, so a fast thumb does not queue renders.
     */
    fun setAdjustment(value: Float) {
        val adjustment = activeAdjustment
        values = values + (adjustment to adjustment.clamp(value))
        requestPreview()
    }

    private fun requestPreview() {
        val photo = document ?: return
        previewJob?.cancel()
        val held = values.toAdjustments()
        previewJob = scope.launch {
            try {
                val shown = displaySize
                if (shown.width < 1f) return@launch
                val next = withContext(Dispatchers.Default) {
                    // One thumbnail: the same cost for a 2-megapixel photo and a 48-megapixel one.
                    val pixels = photo.preview(
                        held,
                        (shown.width / 2f).roundToInt().coerceIn(16, 640).toUInt(),
                        (shown.height / 2f).roundToInt().coerceIn(16, 640).toUInt(),
                    )
                    bitmapOf(pixels.rgba, pixels.width, pixels.height)
                }
                preview = next
            } catch (e: kotlinx.coroutines.CancellationException) {
                throw e
            } catch (e: Exception) {
                // A preview that cannot render is not worth an error: the committed picture is
                // still on screen and the commit will say if it fails.
                preview = null
            }
        }
    }

    /** The slider released: commit it to the engine, which is the one undoable step. */
    fun commitAdjustments() {
        val photo = document ?: return
        val record = values.toAdjustments()
        scope.launch {
            try {
                withContext(Dispatchers.Default) { photo.adjust(record, true) }
                // Back to neutral: the change is in the pixels now, and the sliders describe the
                // *next* change rather than an absolute state the engine has no notion of.
                values = Adjustment.entries.associateWith { it.neutral }
                preview = null
                loadRegion()
            } catch (e: Exception) {
                status = e.message ?: "The adjustment couldn't be applied."
            }
        }
    }

    // ---- strokes ----

    fun beginStroke(position: Offset, canvas: Size, pressure: Float) {
        stroke.clear()
        strokePoint(position, canvas, pressure)
    }

    fun strokePoint(position: Offset, canvas: Size, pressure: Float) {
        val photo = document ?: return
        val width = photo.info().width.toFloat()
        val height = photo.info().height.toFloat()
        // Canvas coordinates to picture coordinates, the same fit the canvas draws with.
        val scale = minOf(canvas.width / width, canvas.height / height)
        val left = (canvas.width - width * scale) / 2f
        val top = (canvas.height - height * scale) / 2f
        val x = (position.x - left) / scale
        val y = (position.y - top) / scale
        if (x < 0f || y < 0f || x > width || y > height) return
        stroke += DrawingStrokePoint(x, y, (brushSize * pressure.coerceAtLeast(0.05f)) / 2f)
    }

    fun endStroke() {
        val photo = document ?: return
        if (stroke.isEmpty()) return
        val points = stroke.toList()
        stroke.clear()
        val brush = DrawingNativeBrush(
            tip = 0u,
            spacing = 0.12f,
            hardness = brushHardness,
            aspect = 1f,
            rotation = 0f,
            scatter = 0f,
            sizeJitter = 0f,
            rotationJitter = 0f,
            flow = brushOpacity,
            seed = 7u,
            followDirection = false,
            symmetry = 0u,
        )
        val erase = tool == PhotoTool.Erase
        val colour = listOf(0f, 0f, 0f, 0f)
        scope.launch {
            try {
                withContext(Dispatchers.Default) { photo.paint(points, brush, colour, brushOpacity, erase) }
                loadRegion()
            } catch (e: Exception) {
                status = e.message ?: "The stroke couldn't be painted."
            }
        }
    }

    // ---- history and layers ----

    fun undo() {
        val photo = document ?: return
        scope.launch {
            try {
                withContext(Dispatchers.Default) { photo.undo() }
                info = photo.info()
                loadRegion()
            } catch (e: Exception) { status = e.message ?: "Nothing to undo." }
        }
    }

    fun redo() {
        val photo = document ?: return
        score(photo) { it.redo() }
    }

    private fun score(photo: PhotoDocument, work: (PhotoDocument) -> Unit) {
        scope.launch {
            try {
                withContext(Dispatchers.Default) { work(photo) }
                info = photo.info()
                loadRegion()
            } catch (e: Exception) { status = e.message ?: "That didn't work." }
        }
    }

    fun addLayer() = document?.let { photo -> score(photo) { it.addLayer("Paint") } }
    fun selectLayer(id: UInt) = document?.let { photo -> score(photo) { it.selectLayer(id) } }

    fun setLayer(id: UInt, name: String, visible: Boolean, opacity: Float) =
        document?.let { photo -> score(photo) { it.setLayer(id, name, visible, opacity) } }

    /** Save: the whole picture, flattened by Rust, as RGBA. */
    fun flatten(onReady: (ByteArray, Int, Int) -> Unit) {
        val photo = document ?: return
        scope.launch {
            try {
                val (bytes, w, h) = withContext(Dispatchers.Default) {
                    val info = photo.info()
                    Triple(photo.flatten(), info.width.toInt(), info.height.toInt())
                }
                onReady(bytes, w, h)
            } catch (e: Exception) { status = e.message ?: "The photo couldn't be saved." }
        }
    }
}

/** One adjustment a photo editor offers. [neutral] is its no-op value. */
internal enum class Adjustment(val label: String, val neutral: Float, val from: Float, val to: Float) {
    Exposure("Exposure", 0f, -2f, 2f),
    Brightness("Brightness", 0f, -0.5f, 0.5f),
    Contrast("Contrast", 1f, 0.25f, 2f),
    Saturation("Saturation", 1f, 0f, 2f),
    Hue("Hue", 0f, -180f, 180f),
    Gamma("Gamma", 1f, 0.4f, 2.5f),
    Black("Black point", 0f, 0f, 0.4f),
    White("White point", 1f, 0.6f, 1f),
    Temperature("Red gain", 1f, 0.6f, 1.4f),
    Tint("Blue gain", 1f, 0.6f, 1.4f),
    ;

    val value: Float get() = this.neutral
    fun clamp(v: Float) = v.coerceIn(from, to)

    /** The slider position for this adjustment, 0..1. */
    fun fraction(v: Float) = ((v - from) / (to - from)).coerceIn(0f, 1f)
    fun fromFraction(f: Float) = from + (to - from) * f
}

/** The sliders' values as the engine's one adjustment record. */
internal fun Map<Adjustment, Float>.toAdjustments(): DrawingAdjustments {
    fun of(a: Adjustment) = this[a] ?: a.neutral
    return DrawingAdjustments(
        exposure = of(Adjustment.Exposure),
        brightness = of(Adjustment.Brightness),
        contrast = of(Adjustment.Contrast),
        saturation = of(Adjustment.Saturation),
        hueDegrees = of(Adjustment.Hue),
        gamma = of(Adjustment.Gamma),
        blackPoint = of(Adjustment.Black),
        whitePoint = of(Adjustment.White),
        redGain = of(Adjustment.Temperature),
        greenGain = 1f,
        blueGain = of(Adjustment.Tint),
    )
}
