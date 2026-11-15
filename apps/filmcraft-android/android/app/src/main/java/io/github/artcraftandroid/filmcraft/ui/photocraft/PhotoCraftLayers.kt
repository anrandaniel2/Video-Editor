package io.github.artcraftandroid.filmcraft.ui.photocraft

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.ColorMatrix
import android.graphics.ColorMatrixColorFilter
import android.graphics.Paint
import android.graphics.PorterDuff
import android.graphics.PorterDuffXfermode
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.ImageBitmap
import kotlin.math.roundToInt

/** How a layer sits on top of what is below it. */
enum class LayerBlend(val label: String) {
    Normal("Normal"),
    Multiply("Multiply"),
    Screen("Screen"),
    Overlay("Overlay"),
    Darken("Darken"),
    Lighten("Lighten"),
    ;

    /** The Porter-Duff mode behind it, or null for plain source-over. */
    val mode: PorterDuff.Mode? get() = when (this) {
        Normal -> null
        Multiply -> PorterDuff.Mode.MULTIPLY
        Screen -> PorterDuff.Mode.SCREEN
        Overlay -> PorterDuff.Mode.OVERLAY
        Darken -> PorterDuff.Mode.DARKEN
        Lighten -> PorterDuff.Mode.LIGHTEN
    }
}

/** The adjustment layers the stack can carry: each one works on everything painted below it. */
enum class AdjustmentKind(val label: String, val first: String, val second: String) {
    BrightnessContrast("Brightness / Contrast", "Brightness", "Contrast"),
    Curves("Curves Adjustment", "Shadows", "Highlights"),
    Saturation("Saturation", "Saturation", "Vibrance"),
}

/**
 * One row of the layer stack: the photo itself, a paint layer with its own operations and its own
 * mask, or an adjustment layer that works on everything under it. Paint layers and masks hold their
 * operations at the preview's size but are replayed at the working copy's size on save, so a stroke
 * lands in the file where it was drawn.
 */
class PhotoLayer(
    val id: Int,
    var name: String,
    val adjustment: AdjustmentKind? = null,
    val base: Boolean = false,
) {
    var visible by mutableStateOf(true)
    var opacity by mutableFloatStateOf(1f)
    var blend by mutableStateOf(LayerBlend.Normal)

    /** The adjustment's two amounts, 0…1, each meaning whatever the adjustment's sliders say. */
    var amount by mutableFloatStateOf(.5f)
    var second by mutableFloatStateOf(.5f)

    /** Painting on the layer draws the stroke; painting on the mask hides it. */
    var maskActive by mutableStateOf(false)
    var hasMask by mutableStateOf(false)

    val paint = PhotoPaintState()
    val mask = PhotoPaintState()

    /** The thumbnail the panel shows. */
    var thumbnail by mutableStateOf<ImageBitmap?>(null)

    val state: PhotoPaintState get() = if (maskActive) mask else paint

    /** Whether this layer has changed the picture, for the "unsaved edits" question. */
    val touched: Boolean get() = paint.marked || mask.marked || adjustment != null

    /** Paints the layer's own operations into a full-size tile; null when it draws nothing. */
    fun tile(width: Int, height: Int): Bitmap? {
        if (paint.isEmpty) return null
        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bitmap)
        paint.renderInto(canvas, width, height)
        if (hasMask) {
            // The mask is a clear sheet with the strokes cut out of it: paint on the mask hides the
            // layer where the brush went, which is what a layer mask does.
            val strokes = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
            mask.renderInto(Canvas(strokes), width, height)
            val stencil = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
            val sheet = Canvas(stencil)
            sheet.drawColor(android.graphics.Color.WHITE)
            val cut = Paint(Paint.ANTI_ALIAS_FLAG)
            cut.xfermode = PorterDuffXfermode(PorterDuff.Mode.DST_OUT)
            sheet.drawBitmap(strokes, 0f, 0f, cut)
            strokes.recycle()
            val keep = Paint(Paint.ANTI_ALIAS_FLAG)
            keep.xfermode = PorterDuffXfermode(PorterDuff.Mode.DST_IN)
            canvas.drawBitmap(stencil, 0f, 0f, keep)
            stencil.recycle()
        }
        return bitmap
    }

    /** Draws the layer over [target] at its opacity, blend mode and mask. */
    fun drawOnto(target: Bitmap, width: Int, height: Int) {
        if (!visible) return
        val bitmap = tile(width, height) ?: return
        val canvas = Canvas(target)
        val paint = Paint(Paint.FILTER_BITMAP_FLAG or Paint.ANTI_ALIAS_FLAG)
        paint.alpha = (opacity.coerceIn(0f, 1f) * 255).roundToInt()
        blend.mode?.let { paint.xfermode = PorterDuffXfermode(it) }
        canvas.drawBitmap(bitmap, 0f, 0f, paint)
        bitmap.recycle()
    }

    /** Applies the adjustment to [target] in place. */
    fun adjust(target: Bitmap) {
        if (!visible || adjustment == null) return
        when (adjustment) {
            AdjustmentKind.BrightnessContrast -> {
                val brightness = (amount - .5f) * 200f
                val contrast = (second - .5f) * 2f + 1f
                val matrix = ColorMatrix()
                matrix.set(
                    floatArrayOf(
                        contrast, 0f, 0f, 0f, brightness,
                        0f, contrast, 0f, 0f, brightness,
                        0f, 0f, contrast, 0f, brightness,
                        0f, 0f, 0f, 1f, 0f,
                    ),
                )
                applyMatrix(target, matrix)
            }
            AdjustmentKind.Saturation -> {
                val matrix = ColorMatrix()
                matrix.setSaturation((amount * 2f).coerceIn(0f, 2f))
                applyMatrix(target, matrix)
            }
            AdjustmentKind.Curves -> applyCurve(target, amount, second)
        }
    }

    private fun applyMatrix(target: Bitmap, matrix: ColorMatrix) {
        val source = target.copy(Bitmap.Config.ARGB_8888, true) ?: return
        val canvas = Canvas(target)
        canvas.drawColor(android.graphics.Color.TRANSPARENT, PorterDuff.Mode.CLEAR)
        val paint = Paint(Paint.FILTER_BITMAP_FLAG)
        paint.colorFilter = ColorMatrixColorFilter(matrix)
        canvas.drawBitmap(source, 0f, 0f, paint)
        source.recycle()
    }

    /** A real curve: a lifted or crushed mid-tone, applied through a 256-entry lookup table. */
    private fun applyCurve(target: Bitmap, shadows: Float, highlights: Float) {
        val lift = (shadows - .5f) * 90f
        val crush = (highlights - .5f) * 90f
        val table = IntArray(256) { value ->
            val lifted = value + lift * (1f - value / 255f)
            val curved = lifted + crush * (lifted / 255f)
            curved.roundToInt().coerceIn(0, 255)
        }
        val width = target.width
        val height = target.height
        val pixels = IntArray(width * height)
        target.getPixels(pixels, 0, width, 0, 0, width, height)
        for (index in pixels.indices) {
            val pixel = pixels[index]
            pixels[index] = (pixel and 0xFF000000.toInt()) or
                (table[(pixel shr 16) and 0xFF] shl 16) or
                (table[(pixel shr 8) and 0xFF] shl 8) or
                table[pixel and 0xFF]
        }
        target.setPixels(pixels, 0, width, 0, 0, width, height)
    }
}

/**
 * The stack itself: the photo, then whatever paint, adjustment and masked layers were added over it.
 * The preview and the saved file are both built by walking this list, so what the panel shows is
 * what the file gets.
 */
class PhotoDocument {
    val layers = mutableStateListOf<PhotoLayer>()
    var activeId by mutableIntStateOf(-1)
        private set

    /** Bumped whenever the stack changes in a way the picture should show. */
    var version by mutableIntStateOf(0)
        private set

    /** Called after a stroke is committed: the composite has to be rebuilt. */
    fun refresh() {
        version++
    }

    private var nextId = 1

    val active: PhotoLayer? get() = layers.firstOrNull { it.id == activeId }
    val edits: Boolean get() = layers.any { it.touched }

    /** Starts the stack with the locked photo layer on the bottom. */
    fun reset() {
        layers.clear()
        layers.add(PhotoLayer(id = 0, name = "Photo", base = true))
        activeId = 0
        nextId = 1
        version++
    }

    fun select(layer: PhotoLayer) {
        activeId = layer.id
        version++
    }

    fun addPaint(name: String = "Layer ${layers.count { it.adjustment == null }}"): PhotoLayer {
        val layer = PhotoLayer(id = nextId++, name = name)
        val at = layers.indexOfFirst { it.base }.coerceAtLeast(0) + 1
        layers.add(at, layer)
        activeId = layer.id
        version++
        return layer
    }

    fun addAdjustment(kind: AdjustmentKind): PhotoLayer {
        val layer = PhotoLayer(id = nextId++, name = kind.label, adjustment = kind)
        layers.add(layer)
        activeId = layer.id
        version++
        return layer
    }

    fun duplicate(layer: PhotoLayer): PhotoLayer {
        val copy = PhotoLayer(id = nextId++, name = "${layer.name} copy", adjustment = layer.adjustment)
        copy.opacity = layer.opacity
        copy.blend = layer.blend
        copy.amount = layer.amount
        copy.second = layer.second
        copy.hasMask = layer.hasMask
        val at = layers.indexOf(layer)
        layers.add((at + 1).coerceIn(0, layers.size), copy)
        activeId = copy.id
        version++
        return copy
    }

    fun remove(layer: PhotoLayer) {
        if (layer.base || layers.size < 2) return
        val at = layers.indexOf(layer)
        layers.remove(layer)
        activeId = layers[(at - 1).coerceAtLeast(0)].id
        version++
    }

    fun move(layer: PhotoLayer, up: Boolean) {
        val at = layers.indexOf(layer)
        val to = if (up) at + 1 else at - 1
        if (at < 0 || to < 0 || to >= layers.size) return
        if (layers[to].base && !up) return
        layers.removeAt(at)
        layers.add(to, layer)
        version++
    }

    /** Everything the stack draws, over a copy of the developed photo. */
    fun composite(base: Bitmap): Bitmap {
        val target = base.copy(Bitmap.Config.ARGB_8888, true) ?: base
        if (target === base) return base
        for (layer in layers) {
            // The photo's own row carries strokes too: they sit directly on the picture, under
            // everything stacked above it.
            if (!layer.visible) continue
            if (layer.adjustment != null) layer.adjust(target) else layer.drawOnto(target, target.width, target.height)
        }
        return target
    }

    /** The same walk at the working copy's size, for the file. */
    fun bake(developed: Bitmap): Bitmap {
        for (layer in layers) {
            if (!layer.visible) continue
            if (layer.adjustment != null) layer.adjust(developed) else layer.drawOnto(developed, developed.width, developed.height)
        }
        return developed
    }

    /** True while any layer hides part of itself with a mask. */
    val masked: Boolean get() = layers.any { it.hasMask }
}
