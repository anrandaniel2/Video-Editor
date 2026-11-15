package io.github.artcraftandroid.filmcraft.ui.photocraft

import android.graphics.Bitmap
import androidx.compose.ui.geometry.Offset
import kotlin.math.floor
import kotlin.math.min
import kotlin.math.pow
import kotlin.math.roundToInt
import kotlin.math.sqrt

/**
 * The pixel work behind the retouch brushes. Each of these tools edits the picture itself rather than
 * painting on the layer above it, so the stroke is run over a snapshot of everything developed so
 * far and handed back as one patch of pixels that the layer stack replays over the photo — which is
 * what makes dodge, burn, blur, sharpen, smudge, heal and clone behave like retouching and not like
 * painting.
 */
internal enum class RetouchMode { Heal, Clone, Blur, Sharpen, Smudge, Dodge, Burn, Sponge, RedEye }

/** Which retouch the tool performs, or null for tools that are not retouchers. */
internal fun retouchMode(tool: PhotoTool): RetouchMode? = when (tool) {
    PhotoTool.HealingBrush, PhotoTool.SpotHealing, PhotoTool.Patch, PhotoTool.ContentAwareMove -> RetouchMode.Heal
    PhotoTool.CloneStamp, PhotoTool.PatternStamp -> RetouchMode.Clone
    PhotoTool.BlurTool -> RetouchMode.Blur
    PhotoTool.SharpenTool -> RetouchMode.Sharpen
    PhotoTool.SmudgeTool -> RetouchMode.Smudge
    PhotoTool.Dodge -> RetouchMode.Dodge
    PhotoTool.Burn -> RetouchMode.Burn
    PhotoTool.Sponge -> RetouchMode.Sponge
    PhotoTool.RedEye -> RetouchMode.RedEye
    else -> null
}

internal object PhotoCraftRetouch {

    /**
     * Runs [mode] along [points] (normalised to the picture) on a copy of [base] and returns the patch
     * of pixels that changed, ready to be replayed at the save size. Null when nothing moved.
     */
    fun stroke(
        base: Bitmap,
        points: List<Offset>,
        radius: Float,
        amount: Float,
        mode: RetouchMode,
    ): RegionOp? {
        if (points.isEmpty()) return null
        val width = base.width
        val height = base.height
        if (width < 2 || height < 2) return null
        val reach = radius.coerceIn(1f, min(width, height) / 2.5f)
        val dabs = dabsOf(points, width, height, reach)
        var left = width
        var top = height
        var right = 0
        var bottom = 0
        for (dab in dabs) {
            left = min(left, floor(dab.x - reach).toInt())
            top = min(top, floor(dab.y - reach).toInt())
            right = kotlin.math.max(right, (dab.x + reach).toInt())
            bottom = kotlin.math.max(bottom, (dab.y + reach).toInt())
        }
        left = left.coerceIn(0, width - 1)
        top = top.coerceIn(0, height - 1)
        right = right.coerceIn(left, width - 1)
        bottom = bottom.coerceIn(top, height - 1)
        val regionWidth = right - left + 1
        val regionHeight = bottom - top + 1
        if (regionWidth < 2 || regionHeight < 2) return null

        val source = IntArray(regionWidth * regionHeight)
        base.getPixels(source, 0, regionWidth, left, top, regionWidth, regionHeight)
        val pixels = source.copyOf()
        val soft = blur(source, regionWidth, regionHeight, (reach / 4f).roundToInt().coerceIn(1, 12))
        val strength = amount.coerceIn(0.05f, 1f)

        for (index in dabs.indices) {
            val dab = dabs[index]
            val previous = if (index > 0) dabs[index - 1] else dab
            val shiftX = (dab.x - previous.x).roundToInt()
            val shiftY = (dab.y - previous.y).roundToInt()
            val cloneX = (-2f * reach).roundToInt()
            val fromY = (dab.y - reach).toInt().coerceAtLeast(top)
            val toY = (dab.y + reach).toInt().coerceAtMost(bottom)
            val fromX = (dab.x - reach).toInt().coerceAtLeast(left)
            val toX = (dab.x + reach).toInt().coerceAtMost(right)
            for (y in fromY..toY) {
                for (x in fromX..toX) {
                    val dx = x - dab.x
                    val dy = y - dab.y
                    val distance = sqrt(dx * dx + dy * dy)
                    if (distance > reach) continue
                    val feather = (1f - distance / reach).pow(0.65f)
                    val cover = (feather * strength).coerceIn(0f, 1f)
                    val offset = (y - top) * regionWidth + (x - left)
                    val current = pixels[offset]
                    val softness = soft[offset]
                    pixels[offset] = when (mode) {
                        RetouchMode.Heal -> mix(current, softness, (cover * 1.7f).coerceAtMost(1f))
                        RetouchMode.Blur -> mix(current, softness, (cover * 1.3f).coerceAtMost(1f))
                        RetouchMode.Sharpen -> {
                            val sharp = IntArray(3) { channel ->
                                val baseValue = (current shr (16 - channel * 8)) and 0xFF
                                val blurValue = (softness shr (16 - channel * 8)) and 0xFF
                                (baseValue + (baseValue - blurValue) * (cover * 2.2f)).roundToInt().coerceIn(0, 255)
                            }
                            pack(sharp)
                        }
                        RetouchMode.Smudge -> {
                            val sx = (x - shiftX * 2 - 1).coerceIn(left, right)
                            val sy = (y - shiftY * 2 - 1).coerceIn(top, bottom)
                            mix(current, pixels[(sy - top) * regionWidth + (sx - left)], (cover * 1.4f).coerceAtMost(1f))
                        }
                        RetouchMode.Clone -> {
                            val sx = (x + cloneX).coerceIn(left, right)
                            val sample = source[(y - top) * regionWidth + (sx - left)]
                            mix(current, sample, cover)
                        }
                        RetouchMode.Dodge -> scale(current, 1f + 0.55f * cover, up = true)
                        RetouchMode.Burn -> scale(current, 1f - 0.5f * cover, up = false)
                        RetouchMode.Sponge -> {
                            val luma = luma(current)
                            mix(current, pack(IntArray(3) { luma }), (cover * 0.9f).coerceAtMost(1f))
                        }
                        RetouchMode.RedEye -> {
                            val red = (current shr 16) and 0xFF
                            val green = (current shr 8) and 0xFF
                            val blue = current and 0xFF
                            if (red > green * 1.35f && red > blue * 1.35f) {
                                val target = ((green + blue) / 2f).roundToInt()
                                pack(
                                    intArrayOf(
                                        (red + (target - red) * cover).roundToInt().coerceIn(0, 255),
                                        green, blue,
                                    ),
                                )
                            } else {
                                current
                            }
                        }
                    }
                }
            }
        }

        val pressed = base.copy(Bitmap.Config.ARGB_8888, true) ?: return null
        pressed.setPixels(pixels, 0, regionWidth, left, top, regionWidth, regionHeight)
        val patch = Bitmap.createBitmap(pressed, left, top, regionWidth, regionHeight)
        pressed.recycle()
        return RegionOp(
            patch,
            left.toFloat() / width,
            top.toFloat() / height,
            (left + regionWidth).toFloat() / width,
            (top + regionHeight).toFloat() / height,
        )
    }

    /** The stamps along the stroke, spaced closely enough that the circles read as one stroke. */
    private fun dabsOf(points: List<Offset>, width: Int, height: Int, reach: Float): List<Offset> {
        val spacing = (reach * 0.4f).coerceAtLeast(1f)
        val dabs = mutableListOf<Offset>()
        val first = points.first()
        var last = Offset(first.x * width, first.y * height)
        dabs.add(last)
        for (index in 1 until points.size) {
            val target = Offset(points[index].x * width, points[index].y * height)
            val dx = target.x - last.x
            val dy = target.y - last.y
            val length = sqrt(dx * dx + dy * dy)
            if (length < spacing) continue
            val steps = (length / spacing).toInt()
            for (step in 1..steps) {
                dabs.add(Offset(last.x + dx * step / steps, last.y + dy * step / steps))
            }
            last = target
        }
        return dabs
    }

    /** A separable box blur, run twice so it reads as a soft Gaussian rather than a grid. */
    private fun blur(source: IntArray, width: Int, height: Int, radius: Int): IntArray {
        if (radius <= 0) return source.copyOf()
        var current = source
        repeat(2) {
            current = boxPass(current, width, height, radius, horizontal = true)
            current = boxPass(current, width, height, radius, horizontal = false)
        }
        return current
    }

    private fun boxPass(source: IntArray, width: Int, height: Int, radius: Int, horizontal: Boolean): IntArray {
        val out = IntArray(source.size)
        val outer = if (horizontal) height else width
        val inner = if (horizontal) width else height
        for (line in 0 until outer) {
            for (index in 0 until inner) {
                var red = 0
                var green = 0
                var blue = 0
                var count = 0
                var offset = -radius
                while (offset <= radius) {
                    val position = index + offset
                    if (position in 0 until inner) {
                        val pixel = if (horizontal) source[line * width + position] else source[position * width + line]
                        red += (pixel shr 16) and 0xFF
                        green += (pixel shr 8) and 0xFF
                        blue += pixel and 0xFF
                        count++
                    }
                    offset++
                }
                val packed = 0xFF shl 24 or ((red / count) shl 16) or ((green / count) shl 8) or (blue / count)
                if (horizontal) out[line * width + index] = packed else out[index * width + line] = packed
            }
        }
        return out
    }

    private fun channel(pixel: Int, index: Int) = (pixel shr (16 - index * 8)) and 0xFF

    private fun pack(channels: IntArray) =
        (0xFF shl 24) or (channels[0].coerceIn(0, 255) shl 16) or
            (channels[1].coerceIn(0, 255) shl 8) or channels[2].coerceIn(0, 255)

    private fun mix(from: Int, to: Int, amount: Float): Int {
        if (amount <= 0f) return from
        if (amount >= 1f) return to
        val value = IntArray(3) { index ->
            val a = channel(from, index)
            val b = channel(to, index)
            (a + (b - a) * amount).roundToInt().coerceIn(0, 255)
        }
        return pack(value)
    }

    private fun scale(pixel: Int, factor: Float, up: Boolean): Int {
        val value = IntArray(3) { index ->
            val current = channel(pixel, index)
            if (up) (current + (255 - current) * (factor - 1f) * 1.6f).roundToInt().coerceIn(0, 255)
            else (current * factor).roundToInt().coerceIn(0, 255)
        }
        return pack(value)
    }

    private fun luma(pixel: Int): Int =
        (channel(pixel, 0) * 0.299f + channel(pixel, 1) * 0.587f + channel(pixel, 2) * 0.114f).roundToInt()
}
