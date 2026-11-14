package io.github.artcraftandroid.filmcraft.ui.photocraft

import io.github.artcraftandroid.filmcraft.core.PhotoCrop
import io.github.artcraftandroid.filmcraft.core.PhotoSettings
import kotlin.math.max
import kotlin.math.min

/**
 * PhotoCraft's filters, in the native pipeline's order (`photo_filter_count`): the id is the index
 * the Rust renderer knows, the label is what the chips say.
 */
enum class PhotoFilter(val id: Int, val label: String) {
    Original(0, "Original"),
    Punch(1, "Punch"),
    Film(2, "Film"),
    Mono(3, "Mono"),
    Noir(4, "Noir"),
    Warm(5, "Warm"),
    Cool(6, "Cool"),
    Fade(7, "Fade"),
    Vivid(8, "Vivid"),
    ;

    companion object {
        fun of(id: Int): PhotoFilter = values().firstOrNull { it.id == id } ?: Original
    }
}

/**
 * The adjustment sliders, mirroring the native `PhotoSettings` record field for field. Every value
 * sits at 0 (no change) except [filterAmount] and [sharpen]/[grain]/[fade], which run 0…1.
 */
data class PhotoAdjustments(
    val exposure: Float = 0f,
    val contrast: Float = 0f,
    val highlights: Float = 0f,
    val shadows: Float = 0f,
    val whites: Float = 0f,
    val blacks: Float = 0f,
    val temperature: Float = 0f,
    val tint: Float = 0f,
    val vibrance: Float = 0f,
    val saturation: Float = 0f,
    val clarity: Float = 0f,
    val sharpen: Float = 0f,
    val vignette: Float = 0f,
    val grain: Float = 0f,
    val fade: Float = 0f,
    val filter: PhotoFilter = PhotoFilter.Original,
    val filterAmount: Float = 1f,
) {
    /** Whether any slider or filter changes the picture. */
    val dirty: Boolean get() = this != PhotoAdjustments()

    /** The native record, positional to match the Rust struct's declaration order. */
    fun toNative(): PhotoSettings = PhotoSettings(
        exposure, contrast, highlights, shadows, whites, blacks, temperature, tint,
        vibrance, saturation, clarity, sharpen, vignette, grain, fade,
        filter.id.toUInt(), filterAmount,
    )
}

/** The native settings after Auto, as slider values. */
internal fun PhotoSettings.toAdjustments(): PhotoAdjustments = PhotoAdjustments(
    exposure = exposure,
    contrast = contrast,
    highlights = highlights,
    shadows = shadows,
    whites = whites,
    blacks = blacks,
    temperature = temperature,
    tint = tint,
    vibrance = vibrance,
    saturation = saturation,
    clarity = clarity,
    sharpen = sharpen,
    vignette = vignette,
    grain = grain,
    fade = fade,
    filter = PhotoFilter.of(filter.toInt()),
    filterAmount = filterAmount,
)

/**
 * One step of a photo edit: the sliders, the quarter turns, the flips and the crop. A crop frame is
 * in the turned picture's coordinates, so turning the picture clears it (the handles would point
 * at a frame that no longer exists).
 */
data class PhotoEdits(
    val adjustments: PhotoAdjustments = PhotoAdjustments(),
    /** Quarter turns clockwise, 0…3. */
    val quarters: Int = 0,
    val flipHorizontal: Boolean = false,
    val flipVertical: Boolean = false,
    val crop: NormRect? = null,
) {
    val hasEdits: Boolean
        get() = adjustments.dirty || quarters != 0 || flipHorizontal || flipVertical || (crop != null && !crop.isFull)

    /** The part of an edit the filter thumbnails have to be rebuilt for. */
    internal val transformKey: Any get() = listOf(quarters % 4, flipHorizontal, flipVertical, crop)
}

/**
 * A rectangle in a picture's own coordinates: 0 at the top or left edge, 1 at the bottom or right.
 * Crop handles work in these units, so a drag means the same thing at any preview size.
 */
data class NormRect(val left: Float, val top: Float, val right: Float, val bottom: Float) {
    val width: Float get() = right - left
    val height: Float get() = bottom - top
    val isFull: Boolean get() = left <= 0.0001f && top <= 0.0001f && right >= 0.9999f && bottom >= 0.9999f

    fun clamped(): NormRect {
        var l = left.coerceIn(0f, 1f)
        var t = top.coerceIn(0f, 1f)
        var r = right.coerceIn(0f, 1f)
        var b = bottom.coerceIn(0f, 1f)
        if (r < l) { val swap = r; r = l; l = swap }
        if (b < t) { val swap = b; b = t; t = swap }
        // Keep at least a sliver visible: an empty frame is never useful.
        if (r - l < MinExtent) { r = (l + MinExtent).coerceAtMost(1f); l = (r - MinExtent).coerceAtLeast(0f) }
        if (b - t < MinExtent) { b = (t + MinExtent).coerceAtMost(1f); t = (b - MinExtent).coerceAtLeast(0f) }
        return NormRect(l, t, r, b)
    }

    fun toNative(): PhotoCrop = PhotoCrop(left, top, right, bottom)

    companion object {
        const val MinExtent = 0.04f
        val Full = NormRect(0f, 0f, 1f, 1f)
    }
}

/** Which corner of a crop frame a finger is holding. */
enum class CropCorner { TopLeft, TopRight, BottomLeft, BottomRight }

/**
 * The largest centred frame with [aspect] (width ÷ height in pixels) in a picture whose own pixel
 * aspect is [imageAspect]; the whole picture when no aspect is locked.
 */
fun centeredCrop(aspect: Float?, imageAspect: Float): NormRect {
    if (aspect == null || aspect <= 0f || imageAspect <= 0f) return NormRect.Full
    var width = 1f
    var height = width * imageAspect / aspect
    if (height > 1f) {
        height = 1f
        width = height * aspect / imageAspect
    }
    val left = (1f - width) / 2f
    val top = (1f - height) / 2f
    return NormRect(left, top, left + width, top + height).clamped()
}

/** Slide a crop frame, keeping it inside the picture. */
fun moveCrop(rect: NormRect, dx: Float, dy: Float): NormRect {
    val left = (rect.left + dx).coerceIn(0f, (1f - rect.width).coerceAtLeast(0f))
    val top = (rect.top + dy).coerceIn(0f, (1f - rect.height).coerceAtLeast(0f))
    return NormRect(left, top, left + rect.width, top + rect.height)
}

/**
 * Drag one corner of a crop frame by ([dx], [dy]) in normalized units. With [locked] unset the
 * corner follows the finger; with an aspect ratio (width ÷ height in pixels) the opposite corner
 * stays put and the frame keeps that ratio, shrinking instead of leaving the picture.
 */
fun resizeCrop(rect: NormRect, corner: CropCorner, dx: Float, dy: Float, imageAspect: Float, locked: Float?): NormRect {
    val min = NormRect.MinExtent
    val movingRight = corner == CropCorner.TopRight || corner == CropCorner.BottomRight
    val movingBottom = corner == CropCorner.BottomLeft || corner == CropCorner.BottomRight
    var left = rect.left
    var top = rect.top
    var right = rect.right
    var bottom = rect.bottom

    // coerceIn throws when its bounds cross (a frame already at an edge), so each range is
    // widened to something valid before a drag is applied.
    if (movingRight) right = (right + dx).coerceIn((left + min).coerceAtMost(1f), 1f)
    else left = (left + dx).coerceIn(0f, (right - min).coerceAtLeast(0f))
    if (movingBottom) bottom = (bottom + dy).coerceIn((top + min).coerceAtMost(1f), 1f)
    else top = (top + dy).coerceIn(0f, (bottom - min).coerceAtLeast(0f))

    if (locked != null && locked > 0f && imageAspect > 0f) {
        // Pixel width ÷ pixel height = locked  ⇒  normalized height = width × imageAspect ÷ locked.
        val anchorX = if (movingRight) left else right
        val anchorY = if (movingBottom) top else bottom
        var width = if (movingRight) right - anchorX else anchorX - left
        var height = width * imageAspect / locked
        val roomX = if (movingRight) 1f - anchorX else anchorX
        val roomY = if (movingBottom) 1f - anchorY else anchorY
        if (height > roomY) {
            height = roomY
            width = height * locked / imageAspect
        }
        width = width.coerceIn(min, roomX.coerceAtLeast(min))
        height = (width * imageAspect / locked).coerceIn(min, roomY.coerceAtLeast(min))
        if (movingRight) right = anchorX + width else left = anchorX - width
        if (movingBottom) bottom = anchorY + height else top = anchorY - height
    }
    return NormRect(left, top, right, bottom).clamped()
}

/** The pixel aspect of a turned picture: its width ÷ height, never zero. */
internal fun imageAspectOf(width: Int, height: Int): Float = max(1, width).toFloat() / max(1, height).toFloat()

/** The largest frame with the given aspect that fits in [maxWidth] × [maxHeight]. */
internal fun fitAspect(aspect: Float, maxWidth: Float, maxHeight: Float): Pair<Float, Float> {
    val safe = if (aspect.isFinite() && aspect > 0f) aspect else 1f
    val width = min(maxWidth, maxHeight * safe)
    return width to width / safe
}
