package io.github.artcraftandroid.filmcraft.ui.editor

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
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
import io.github.artcraftandroid.filmcraft.ui.FcType
import io.github.artcraftandroid.filmcraft.ui.editorGlass
import java.util.Locale
import kotlin.math.roundToInt

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

/**
 * Non-intercepting monitor overlay for 3D Camera Match-Move: projects reconstructed 3D landmarks
 * through the active solved camera pose onto the frame and shows live lock/RMS/lost telemetry,
 * while letting play/pause taps and 3D object gizmos pass straight through.
 */
@Composable
fun MatchMoveOverlay(ed: EditorController, frameWidth: Float, frameHeight: Float, modifier: Modifier = Modifier) {
    val pose = ed.matchmovePose ?: return
    val inMatchMovePanel = ed.panel == Panel.MatchMove
    if (!inMatchMovePanel && !(ed.matchmoveActive && pose.lost)) return
    val fw = frameWidth.coerceAtLeast(1f)
    val fh = frameHeight.coerceAtLeast(1f)
    val scene = ed.matchmoveSceneData
    val tapPicking = inMatchMovePanel && ed.matchmoveTapToPlace && ed.matchmoveActive && !pose.lost
    val showMarkers = inMatchMovePanel && (ed.matchmoveShowMarkers || tapPicking) && ed.matchmoveActive && scene != null && !pose.lost

    val tapMod = if (tapPicking) {
        Modifier.pointerInput(fw, fh) {
            detectTapGestures { tap ->
                val norm = frameNormalizedAtTap(tap.x, tap.y, size.width.toFloat(), size.height.toFloat(), fw, fh)
                    ?: return@detectTapGestures
                ed.tapMatchMoveLandmark(norm.first, norm.second)
            }
        }
    } else {
        Modifier
    }

    Box(modifier.then(tapMod).testTag("matchmove_overlay")) {
        if (showMarkers && scene != null) {
            val cam = SceneCamera.fromNative(pose.camera)
            Canvas(Modifier.fillMaxSize()) {
                val k = minOf(size.width / fw, size.height / fh)
                val iw = fw * k
                val ih = fh * k
                val ox = (size.width - iw) / 2f
                val oy = (size.height - ih) / 2f
                val proj = cameraProjection(cam, fw.toDouble(), fh.toDouble(), iw.toDouble(), ih.toDouble()) ?: return@Canvas
                val arm = 4.5.dp.toPx()
                val ringRadius = 9.dp.toPx()
                val halo = Color.Black.copy(alpha = 0.65f)
                scene.points.forEach { pt ->
                    val p = proj.project(GizmoVec(pt.x, pt.y, pt.z))?.screen ?: return@forEach
                    if (p.x < 0f || p.x > iw || p.y < 0f || p.y > ih) return@forEach
                    val c = Offset(ox + p.x, oy + p.y)
                    val tint = landmarkColor(pt.rmsPx)
                    if (tapPicking) {
                        drawCircle(Fc.Accent.copy(alpha = 0.55f), ringRadius, c, style = Stroke(1.5.dp.toPx()))
                    }
                    drawLine(halo, Offset(c.x - arm - 1f, c.y), Offset(c.x + arm + 1f, c.y), 3.dp.toPx())
                    drawLine(halo, Offset(c.x, c.y - arm - 1f), Offset(c.x, c.y + arm + 1f), 3.dp.toPx())
                    drawLine(tint, Offset(c.x - arm, c.y), Offset(c.x + arm, c.y), 1.6.dp.toPx())
                    drawLine(tint, Offset(c.x, c.y - arm), Offset(c.x, c.y + arm), 1.6.dp.toPx())
                }
            }
        }

        val (dotColor, badgeText) = when {
            tapPicking -> Fc.Accent to "Tap a 3D landmark to place"
            pose.lost -> Color(0xFFFF6575) to "Camera track lost · holding pose"
            pose.rotationOnly -> Color(0xFF4CC9F0) to String.format(
                Locale.US, "Tripod pan · %.2f px · %d%%", pose.rmsPx, pose.confidence.roundToInt(),
            )
            else -> Color(0xFF6FE295) to String.format(
                Locale.US, "3D lock · %.2f px · %d%%", pose.rmsPx, pose.confidence.roundToInt(),
            )
        }
        Row(
            Modifier
                .align(Alignment.TopEnd)
                .padding(10.dp)
                .clip(RoundedCornerShape(14.dp))
                .editorGlass(14.dp, layer = 3f, captureSurface = false)
                .padding(horizontal = 10.dp, vertical = 5.dp)
                .testTag("matchmove_hud"),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(Modifier.size(8.dp).clip(CircleShape).background(dotColor))
            Spacer(Modifier.width(7.dp))
            Text(badgeText, style = FcType.Caption, color = Fc.Title)
        }
    }
}
