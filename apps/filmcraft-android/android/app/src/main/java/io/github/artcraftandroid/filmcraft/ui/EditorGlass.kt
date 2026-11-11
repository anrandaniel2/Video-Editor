package io.github.artcraftandroid.filmcraft.ui

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInRoot
import androidx.compose.ui.layout.findRootCoordinates
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.drawscope.Stroke
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
        // Keep the preview's aspect, rather than squeezing it into a square on every bar.
        val ratio = 96f / maxOf(it.width, it.height).coerceAtLeast(1)
        val width = (it.width * ratio).toInt().coerceAtLeast(1)
        val height = (it.height * ratio).toInt().coerceAtLeast(1)
        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        Canvas(bitmap).drawBitmap(it.asAndroidBitmap(), null, android.graphics.Rect(0, 0, width, height), Paint(Paint.FILTER_BITMAP_FLAG))
        var pixels = IntArray(width * height)
        bitmap.getPixels(pixels, 0, width, 0, 0, width, height)
        // Repeated box blur works on older Android too, unlike RenderEffect (API 31+).
        repeat(3) {
            val blurred = IntArray(pixels.size)
            for (y in 0 until height) for (x in 0 until width) {
                var r = 0; var g = 0; var b = 0
                for (oy in -2..2) for (ox in -2..2) {
                    val c = pixels[(y + oy).coerceIn(0, height - 1) * width + (x + ox).coerceIn(0, width - 1)]
                    r += (c ushr 16) and 255; g += (c ushr 8) and 255; b += c and 255
                }
                blurred[y * width + x] = android.graphics.Color.rgb(r / 25, g / 25, b / 25)
            }
            pixels = blurred
        }
        bitmap.setPixels(pixels, 0, width, 0, 0, width, height)
        bitmap.asImageBitmap()
    }
}

/** Paint the glass only: child text/icons and gesture targets are never blurred. */
@Composable
fun Modifier.editorGlass(corner: Dp = 16.dp): Modifier {
    val backdrop = LocalEditorGlass.current
    var origin by remember { mutableStateOf(Offset.Zero) }
    var rootSize by remember { mutableStateOf(IntSize.Zero) }
    return onGloballyPositioned { coordinates ->
        origin = coordinates.positionInRoot()
        rootSize = coordinates.findRootCoordinates().size
    }.clip(RoundedCornerShape(corner)).drawWithCache {
        val sheen = Brush.verticalGradient(listOf(Color.White.copy(alpha = .12f), Color.Transparent, Color.Black.copy(alpha = .08f)))
        onDrawBehind {
            drawRect(Color(0xFF171C28))
            backdrop?.let {
                // Every pane samples the same screen-aligned backdrop, never a repeated thumbnail.
                val rw = rootSize.width.coerceAtLeast(size.width.toInt())
                val rh = rootSize.height.coerceAtLeast(size.height.toInt())
                val scale = maxOf(rw.toFloat() / it.width, rh.toFloat() / it.height)
                val w = (it.width * scale).toInt().coerceAtLeast(1)
                val h = (it.height * scale).toInt().coerceAtLeast(1)
                drawImage(it, dstOffset = IntOffset(((rw - w) / 2f - origin.x).toInt(), ((rh - h) / 2f - origin.y).toInt()),
                    dstSize = IntSize(w, h), filterQuality = FilterQuality.Medium)
            }
            drawRect(Color(0x99232835))
            drawRect(sheen)
            drawRoundRect(Color.White.copy(alpha = .18f), cornerRadius = androidx.compose.ui.geometry.CornerRadius(corner.toPx()), style = Stroke(1.dp.toPx()))
        }
    }
}
