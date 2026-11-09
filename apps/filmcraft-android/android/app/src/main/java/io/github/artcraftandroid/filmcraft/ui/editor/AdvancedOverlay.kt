package io.github.artcraftandroid.filmcraft.ui.editor

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import io.github.artcraftandroid.filmcraft.core.StereoLayout
import io.github.artcraftandroid.filmcraft.ui.Fc

/** Tap layer for tracking targets and background keep/remove seeds. */
@Composable
fun AdvancedOverlay(ed: EditorController, frameWidth: Float, frameHeight: Float, modifier: Modifier = Modifier) {
    val fw = frameWidth.coerceAtLeast(1f)
    val fh = frameHeight.coerceAtLeast(1f)
    Canvas(
        modifier
            .testTag("advanced_overlay")
            .semantics { contentDescription = "Tracking and background points" }
            .pointerInput(ed.panel, ed.backgroundPointMode, ed.trackingLayout, fw, fh) {
                detectTapGestures { tap ->
                    val k = minOf(size.width / fw, size.height / fh)
                    val iw = fw * k
                    val ih = fh * k
                    val ox = (size.width - iw) / 2f
                    val oy = (size.height - ih) / 2f
                    if (tap.x < ox || tap.x > ox + iw || tap.y < oy || tap.y > oy + ih) return@detectTapGestures
                    val (vw, vh) = when (ed.trackingLayout) {
                        StereoLayout.SIDE_BY_SIDE -> 0.5f to 1f
                        StereoLayout.TOP_BOTTOM -> 1f to 0.5f
                        else -> 1f to 1f
                    }
                    val tw = if (ed.panel == Panel.Tracking) iw * vw else iw
                    val th = if (ed.panel == Panel.Tracking) ih * vh else ih
                    val x = ((tap.x - ox) / tw).toDouble().coerceIn(0.0, 1.0)
                    val y = ((tap.y - oy) / th).toDouble().coerceIn(0.0, 1.0)
                    ed.onAdvancedTap(x, y)
                }
            },
    ) {
        val k = minOf(size.width / fw, size.height / fh)
        val iw = fw * k
        val ih = fh * k
        val ox = (size.width - iw) / 2f
        val oy = (size.height - ih) / 2f
        fun pt(x: Double, y: Double) = Offset(ox + x.toFloat() * iw, oy + y.toFloat() * ih)
        if (ed.panel == Panel.Tracking) {
            val (vw, vh) = when (ed.trackingLayout) {
                StereoLayout.SIDE_BY_SIDE -> 0.5f to 1f
                StereoLayout.TOP_BOTTOM -> 1f to 0.5f
                else -> 1f to 1f
            }
            if (vw < 1f) drawLine(Fc.Detail, Offset(ox + iw * vw, oy), Offset(ox + iw * vw, oy + ih), 1.dp.toPx())
            if (vh < 1f) drawLine(Fc.Detail, Offset(ox, oy + ih * vh), Offset(ox + iw, oy + ih * vh), 1.dp.toPx())
            val target = ed.trackingTarget ?: (0.5 to 0.5)
            val c = pt(target.first * vw, target.second * vh)
            val r = 14.dp.toPx()
            drawCircle(Fc.Accent, r, c, style = Stroke(2.dp.toPx()))
            drawLine(Fc.Accent, Offset(c.x - r * 1.6f, c.y), Offset(c.x + r * 1.6f, c.y), 2.dp.toPx())
            drawLine(Fc.Accent, Offset(c.x, c.y - r * 1.6f), Offset(c.x, c.y + r * 1.6f), 2.dp.toPx())
        }
        if (ed.panel == Panel.Background) {
            val st = ed.backgroundState?.settings
            st?.keepPoints?.forEach { p ->
                val c = pt(p.x, p.y)
                drawCircle(Color(0xFF80ED99), 7.dp.toPx(), c)
                drawCircle(Color.Black, 7.dp.toPx(), c, style = Stroke(1.5.dp.toPx()))
            }
            st?.removePoints?.forEach { p ->
                val c = pt(p.x, p.y)
                drawCircle(Color(0xFFF72585), 7.dp.toPx(), c)
                drawLine(Color.White, Offset(c.x - 4.dp.toPx(), c.y - 4.dp.toPx()), Offset(c.x + 4.dp.toPx(), c.y + 4.dp.toPx()), 2.dp.toPx())
                drawLine(Color.White, Offset(c.x - 4.dp.toPx(), c.y + 4.dp.toPx()), Offset(c.x + 4.dp.toPx(), c.y - 4.dp.toPx()), 2.dp.toPx())
            }
        }
    }
}
