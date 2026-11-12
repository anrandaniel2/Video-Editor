package io.github.artcraftandroid.filmcraft.ui

import dev.chrisbanes.haze.HazeState
import dev.chrisbanes.haze.HazeStyle
import dev.chrisbanes.haze.HazeTint
import dev.chrisbanes.haze.hazeSource
import dev.chrisbanes.haze.hazeEffect
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
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp

/** Live editor sources are actual screen-positioned UI layers, never a stretched monitor image. */
val LocalEditorBackdrop = staticCompositionLocalOf<HazeState?> { null }

@Composable
fun Modifier.editorBackdropSource(): Modifier {
    val state = LocalEditorBackdrop.current
    return if (state != null) hazeSource(state, zIndex = 0f) else this
}

/** Static screenshot fallback for the Home canvas dialog, outside the editor's live source tree. */
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
fun Modifier.editorGlass(corner: Dp = 16.dp, layer: Float = 1f, blurRadius: Dp = 20.dp, captureSurface: Boolean = true): Modifier {
    val state = LocalEditorBackdrop.current
    if (state != null) {
        val style = remember(blurRadius) {
            HazeStyle(backgroundColor = Color.Transparent, tints = emptyList(), blurRadius = blurRadius,
                noiseFactor = 0f, fallbackTint = HazeTint(Color.Transparent))
        }
        // Floating catalogs use effect-only panes: all panes sample the editor, not each
        // other's render layers. Capturing nested full-screen effects can form a draw cycle.
        // Normal editor panels retain their explicit source ordering.
        return clip(RoundedCornerShape(corner))
            .then(if (captureSurface) Modifier.hazeSource(state, zIndex = layer) else Modifier)
            .hazeEffect(state, style = style)
    }
    val backdrop = LocalEditorGlass.current
    var origin by remember { mutableStateOf(Offset.Zero) }
    var rootSize by remember { mutableStateOf(IntSize.Zero) }
    return onGloballyPositioned { coordinates ->
        origin = coordinates.positionInRoot()
        rootSize = coordinates.findRootCoordinates().size
    }.clip(RoundedCornerShape(corner)).drawWithCache {
        onDrawBehind {
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
        }
    }
}
