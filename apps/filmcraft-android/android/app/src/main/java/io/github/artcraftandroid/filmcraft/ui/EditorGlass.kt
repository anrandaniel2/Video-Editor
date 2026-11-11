package io.github.artcraftandroid.filmcraft.ui

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp

/** One small shared blur per preview update, not a screen capture or blur per control. */
val LocalEditorGlass = staticCompositionLocalOf<ImageBitmap?> { null }

@Composable
fun rememberEditorGlass(frame: ImageBitmap?): ImageBitmap? = remember(frame) {
    frame?.let {
        val side = 48
        val bitmap = Bitmap.createBitmap(side, side, Bitmap.Config.ARGB_8888)
        Canvas(bitmap).drawBitmap(it.asAndroidBitmap(), null, android.graphics.Rect(0, 0, side, side), Paint(Paint.FILTER_BITMAP_FLAG))
        var pixels = IntArray(side * side)
        bitmap.getPixels(pixels, 0, side, 0, 0, side, side)
        // Repeated box blur works on older Android too, unlike RenderEffect (API 31+).
        repeat(3) {
            val blurred = IntArray(pixels.size)
            for (y in 0 until side) for (x in 0 until side) {
                var r = 0; var g = 0; var b = 0
                for (oy in -2..2) for (ox in -2..2) {
                    val c = pixels[(y + oy).coerceIn(0, side - 1) * side + (x + ox).coerceIn(0, side - 1)]
                    r += (c ushr 16) and 255; g += (c ushr 8) and 255; b += c and 255
                }
                blurred[y * side + x] = android.graphics.Color.rgb(r / 25, g / 25, b / 25)
            }
            pixels = blurred
        }
        bitmap.setPixels(pixels, 0, side, 0, 0, side, side)
        bitmap.asImageBitmap()
    }
}

/** Paint the glass only: child text/icons and gesture targets are never blurred. */
@Composable
fun Modifier.editorGlass(corner: Dp = 16.dp): Modifier {
    val backdrop = LocalEditorGlass.current
    return clip(RoundedCornerShape(corner)).drawWithCache {
        val sheen = Brush.verticalGradient(listOf(Color.White.copy(alpha = .09f), Color.Transparent, Color.Black.copy(alpha = .12f)))
        onDrawBehind {
            drawRect(Color(0xFF171C28))
            backdrop?.let { drawImage(it, dstOffset = IntOffset.Zero, dstSize = IntSize(size.width.toInt().coerceAtLeast(1), size.height.toInt().coerceAtLeast(1)), filterQuality = FilterQuality.Medium, alpha = .55f) }
            drawRect(Color(0x99202734))
            drawRect(sheen)
            drawLine(Color.White.copy(alpha = .18f), androidx.compose.ui.geometry.Offset.Zero, androidx.compose.ui.geometry.Offset(size.width, 0f), strokeWidth = 1.dp.toPx())
        }
    }
}
