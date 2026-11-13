package io.github.artcraftandroid.filmcraft.ui.editor

import androidx.compose.runtime.CompositionLocalProvider
import io.github.artcraftandroid.filmcraft.ui.LocalEditorBackdrop
import dev.chrisbanes.haze.rememberHazeState
import io.github.artcraftandroid.filmcraft.ui.editorBackdropSource
import io.github.artcraftandroid.filmcraft.ui.editorGlass
import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.animation.expandVertically
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.calculateCentroid
import androidx.compose.foundation.gestures.calculatePan
import androidx.compose.foundation.gestures.calculateRotation
import androidx.compose.foundation.gestures.calculateZoom
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.gestures.detectVerticalDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsBottomHeight
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.displayCutout
import androidx.compose.foundation.layout.union
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChanged
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.github.artcraftandroid.filmcraft.core.ClipKind
import io.github.artcraftandroid.filmcraft.core.Placement
import io.github.artcraftandroid.filmcraft.ui.Fc
import io.github.artcraftandroid.filmcraft.ui.EditorExportButton
import io.github.artcraftandroid.filmcraft.ui.FcIconButton
import io.github.artcraftandroid.filmcraft.ui.FcSize
import io.github.artcraftandroid.filmcraft.ui.FcType
import io.github.artcraftandroid.filmcraft.ui.Glyphs
import io.github.artcraftandroid.filmcraft.ui.LucideIcons
import io.github.artcraftandroid.filmcraft.ui.glyph
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt
import kotlin.math.sin
import kotlinx.coroutines.launch

/** The editor: preview, transport, timeline and tools, stacked or side by side. */
@Composable
fun EditorScreen(ed: EditorController) {
    SideEffect { io.github.artcraftandroid.filmcraft.CrashReports.screen = "Editor panel=${ed.panel}, clip=${ed.selectedClip?.kind}, 3D=${ed.selectedClip?.has3d}" }
    BackHandler {
        when {
            ed.panel != null -> ed.panel = null
            ed.submenu != Submenu.None -> ed.submenu = Submenu.None
            ed.selectedClip != null || ed.selectedTransition != null || ed.selectedCaption != null -> ed.select(null)
            else -> ed.closeEditor()
        }
    }
    CompositionLocalProvider(LocalEditorBackdrop provides rememberHazeState()) {
        BoxWithConstraints(
            Modifier
                .fillMaxSize()
                .background(Fc.Black)
                .windowInsetsPadding(WindowInsets.displayCutout.union(WindowInsets.navigationBars).only(WindowInsetsSides.Horizontal))
                .testTag("screen_editor"),
        ) {
            EditorWorkspace(ed)
            // The animation frame browser floats across the bottom. The layouts above reserve its
            // height (the preview window goes up; when there is no room it shrinks) so the two never
            // sit on top of each other.
            if (frameBarVisible(ed)) {
                AnimationFrameBar(
                    ed,
                    Modifier
                        .align(Alignment.BottomCenter)
                        .padding(horizontal = 12.dp)
                        .navigationBarsPadding()
                        .padding(bottom = 8.dp),
                )
            }
            if (ed.panel == Panel.OnlineVideos || ed.panel == Panel.OnlineSounds) {
                MixkitPanel(ed, video = ed.panel == Panel.OnlineVideos, close = { ed.panel = null })
            }
            // A single composition slot survives switching between portrait and landscape layouts.
            // Putting Draw inside each layout destroyed its VM and created a new artwork on rotation.
            if (ed.panel == Panel.Drawing) DrawingPanel(ed) { ed.panel = null }
            ed.busy?.let { BusyOverlay(it) }
        }
    }
}

@Composable
private fun BusyOverlay(text: String) {
    Box(Modifier.fillMaxSize().background(Fc.Scrim).clickable(enabled = true, onClick = {}), contentAlignment = Alignment.Center) {
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            CircularProgressIndicator(color = Fc.Accent)
            Spacer(Modifier.height(16.dp))
            Text(text, style = FcType.Body, color = Fc.Title)
        }
    }
}

/** One stable workspace. Inspectors FLOAT over the monitor instead of rebuilding its layout. */
@Composable
private fun EditorWorkspace(ed: EditorController) {
    val panel = ed.panel.takeUnless { it == Panel.Drawing || it == Panel.OnlineVideos || it == Panel.OnlineSounds || it is Panel.TextInput }
    BoxWithConstraints(Modifier.fillMaxSize().statusBarsPadding().navigationBarsPadding()) {
        val wide = maxWidth >= 700.dp && maxWidth > maxHeight
        val inspectorWidth = if (wide) 380.dp else maxWidth - 12.dp
        val inspectorHeight = maxHeight * if (wide) 0.80f else 0.62f
        Column(Modifier.fillMaxSize()) {
            WorkspaceBar(ed)
            if (wide) {
                Row(Modifier.weight(1f).fillMaxWidth()) {
                    WorkspaceMonitor(ed, Modifier.weight(1f).fillMaxHeight())
                    Column(Modifier.weight(1f).fillMaxHeight()) {
                        TransportRow(ed)
                        Timeline(ed, Modifier.fillMaxWidth().weight(1f))
                    }
                }
            } else {
                WorkspaceMonitor(ed, Modifier.weight(1f).fillMaxWidth())
                TransportRow(ed)
                Timeline(ed, Modifier.fillMaxWidth().height(174.dp))
            }
            val input = ed.panel as? Panel.TextInput
            if (input != null) TextInputStrip(ed, input) else EditorToolbar(ed)
        }
        if (panel != null) {
            Box(Modifier.align(Alignment.BottomEnd).padding(end = if (wide) 12.dp else 0.dp, bottom = FcSize.ToolTile + 8.dp)
                .width(inspectorWidth).heightIn(max = inspectorHeight)) {
                EditorPanel(ed, panel, Modifier.fillMaxWidth())
            }
        }
    }
}

@Composable
private fun WorkspaceMonitor(ed: EditorController, modifier: Modifier) {
    Box(modifier.padding(horizontal = 12.dp, vertical = 8.dp).clip(RoundedCornerShape(16.dp)).background(Fc.Lane)) {
        Preview(ed, Modifier.fillMaxSize())
        Column(Modifier.align(Alignment.CenterStart).padding(6.dp).clip(RoundedCornerShape(14.dp)).background(Fc.Black)) {
            NativeAction(io.github.artcraftandroid.procraft.ui.PcIcons.Undo, "Undo", false) { ed.undo() }
            NativeAction(io.github.artcraftandroid.procraft.ui.PcIcons.Redo, "Redo", false) { ed.redo() }
        }
    }
}

@Composable
private fun WorkspaceBar(ed: EditorController) {
    Row(Modifier.fillMaxWidth().height(52.dp).padding(horizontal = 8.dp), verticalAlignment = Alignment.CenterVertically) {
        NativeAction(io.github.artcraftandroid.procraft.ui.PcIcons.Cross, "Save and return to Projects", false) { ed.closeEditor() }
        NativeAction(io.github.artcraftandroid.procraft.ui.PcIcons.Actions, "Keyframes", ed.panel == Panel.Keyframes) { if (ed.selectedClip != null) ed.panel = Panel.Keyframes }
        NativeAction(io.github.artcraftandroid.procraft.ui.PcIcons.Adjustments, "Effects", ed.panel == Panel.Effects) { if (ed.selectedClip != null) ed.panel = Panel.Effects }
        NativeAction(io.github.artcraftandroid.procraft.ui.PcIcons.Transform, "3D", ed.panel == Panel.Object3D) { ed.panel = Panel.Object3D }
        Spacer(Modifier.weight(1f))
        NativeAction(io.github.artcraftandroid.procraft.ui.PcIcons.Brush, "Draw", ed.panel == Panel.Drawing) { ed.panel = Panel.Drawing }
        NativeAction(io.github.artcraftandroid.procraft.ui.PcIcons.MenuShare, "Export", false) { if ((ed.timeline?.duration ?: 0L) > 0L) ed.export() }
    }
}

@Composable
private fun NativeAction(icon: androidx.compose.ui.graphics.vector.ImageVector, label: String, selected: Boolean, click: () -> Unit) {
    Box(Modifier.size(44.dp).clip(RoundedCornerShape(12.dp)).background(if (selected) Fc.Accent else Color.Transparent)
        .clickable(onClick = click).semantics { contentDescription = label }, contentAlignment = Alignment.Center) {
        androidx.compose.material3.Icon(icon, label, tint = Color.White, modifier = Modifier.size(42.dp))
    }
}

@Composable
private fun TransportRow(ed: EditorController) {
    val st = ed.status
    Row(Modifier.fillMaxWidth().height(FcSize.Transport).editorGlass(0.dp).padding(horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(TimelineMath.clock(ed.playhead), style = FcType.Value, color = Fc.Body, modifier = Modifier.padding(start = 16.dp))
        Spacer(Modifier.weight(1f))
        val has = (ed.timeline?.duration ?: 0L) > 0L
        FcIconButton(Glyphs.StepBack, "Previous frame", { ed.step(-1) }, enabled = has, size = 36.dp, iconSize = 22.dp, modifier = Modifier.testTag("editor_previous_frame_button"))
        FcIconButton(if (ed.playing) Glyphs.Pause else Glyphs.Play, if (ed.playing) "Pause" else "Play", { ed.togglePlay() }, enabled = has, size = 36.dp, iconSize = 22.dp, modifier = Modifier.testTag("editor_play_button"))
        FcIconButton(Glyphs.StepForward, "Next frame", { ed.step(1) }, enabled = has, size = 36.dp, iconSize = 22.dp, modifier = Modifier.testTag("editor_next_frame_button"))
        Spacer(Modifier.weight(1f))
        FcIconButton(LucideIcons.Undo2.glyph(), "Undo", { ed.undo() }, enabled = st?.canUndo == true, size = 36.dp, iconSize = 22.dp, modifier = Modifier.testTag("editor_undo_button"))
        FcIconButton(LucideIcons.Redo2.glyph(), "Redo", { ed.redo() }, enabled = st?.canRedo == true, size = 36.dp, iconSize = 22.dp, modifier = Modifier.testTag("editor_redo_button"))
        Spacer(Modifier.width(4.dp))
    }
}

/** The program monitor: the picture at the playhead, and the selected item's frame to move it by. */
@Composable
private fun Preview(ed: EditorController, modifier: Modifier) {
    val t = ed.timeline
    val density = LocalDensity.current
    val haptic = LocalHapticFeedback.current
    val scope = rememberCoroutineScope()
    val sel = ed.selectedClip
    val masking = ed.panel == Panel.Mask && sel != null && (sel.kind == ClipKind.VIDEO || sel.kind == ClipKind.IMAGE)
    val advanced = (ed.panel == Panel.Background || ed.panel == Panel.Tracking) && sel != null && (sel.kind == ClipKind.VIDEO || sel.kind == ClipKind.IMAGE)
    val object3d = sel?.has3d == true
    val movable = !object3d && sel != null && sel.kind != ClipKind.AUDIO && sel.kind != ClipKind.MISSING && ed.panel !is Panel.TextInput && !masking && !advanced && ed.panel != Panel.Object3D
    var placement by remember { mutableStateOf<Placement?>(null) }
    LaunchedEffect(sel?.id, t?.revision, ed.playhead, movable) {
        placement = if ((movable || object3d) && sel != null) ed.placementOf(sel.id) else null
    }
    LaunchedEffect(sel?.id, t?.revision, ed.playhead, ed.panel) {
        if (object3d && sel != null) ed.loadObject3D(sel.id)
        if (masking && sel != null) ed.loadMasks(sel.id)
    }
    BoxWithConstraints(modifier.editorBackdropSource().background(Fc.Black).testTag("editor_player_area"), contentAlignment = Alignment.Center) {
        val aspect = if (t != null && t.height.toInt() > 0) t.width.toFloat() / t.height.toFloat() else 9f / 16f
        // an empty project shows the whole monitor; otherwise the frame, fitted
        val empty = (t?.duration ?: 0L) == 0L
        val boxW = if (empty) maxWidth else min(maxWidth.value, maxHeight.value * aspect).dp
        val boxH = if (empty) maxHeight else boxW / aspect
        val px = with(density) { max(boxW.toPx(), boxH.toPx()) }
        LaunchedEffect(px) { ed.previewSide = px.roundToInt() }
        Box(
            Modifier
                .size(boxW, boxH)
                .border(1.dp, Fc.Raised)
                .pointerInput(sel?.id, movable, object3d, ed.panel, t?.width, t?.height) {
                    if (masking || advanced || ed.panel == Panel.Lights || ed.panel is Panel.TextInput) return@pointerInput
                    val frameW = t?.width?.toFloat() ?: return@pointerInput
                    val k = size.width / frameW
                    awaitEachGesture {
                        val down = awaitFirstDown(requireUnconsumed = false)
                        var handledByChild = down.isConsumed
                        var p = placement
                        var moved = false
                        var edited = false
                        var multi = false
                        var count = 1
                        var pendingPan = Offset.Zero
                        var pendingZoom = 1f
                        val rotation = RotationGate()
                        while (true) {
                            val ev = awaitPointerEvent()
                            val fingers = ev.changes.count { it.pressed }
                            if (ev.changes.any { it.isConsumed }) handledByChild = true
                            if (fingers == 0) break
                            if (ev.changes.any { it.isConsumed }) continue
                            if (fingers != count) {
                                count = fingers
                                multi = multi || fingers >= 2
                                pendingPan = Offset.Zero; pendingZoom = 1f; rotation.reset()
                                continue // Rebase on every pointer change: no centroid jumps.
                            }
                            val pan = if (fingers == 1 && !multi) ev.calculatePan() else Offset.Zero
                            val zoom = if (fingers >= 2) ev.calculateZoom() else 1f
                            val rot = if (fingers >= 2) rotation.update(ev.calculateRotation()) else 0f
                            pendingPan += pan
                            pendingZoom *= zoom
                            if (!moved) moved = pendingPan.getDistance() > viewConfiguration.touchSlop || abs(pendingZoom - 1f) > 0.015f || rot != 0f
                            if (moved && movable && p != null && sel != null) {
                                ed.stop()
                                val delta = if (!edited) pendingPan else pan
                                val scale = if (!edited) pendingZoom else zoom
                                val next = Placement(
                                    x = p.x + delta.x / k, y = p.y + delta.y / k,
                                    width = p.width, height = p.height,
                                    scale = (p.scale * scale).coerceIn(0.001, 100.0), rotation = p.rotation + rot,
                                )
                                placement = next; p = next
                                ed.setPlacement(sel.id, next, merge = edited)
                                edited = true
                                ev.changes.forEach { it.consume() }
                            } else if (moved && !object3d) ev.changes.forEach { it.consume() }
                        }
                        if (!moved && !multi && !handledByChild) {
                            val point = down.position / k
                            val tolerance = 8.dp.toPx() / k
                            scope.launch {
                                val hit = ed.pickPreview(point, tolerance.toDouble())
                                if (hit != null) { ed.stop(); if (ed.selectedClip?.id != hit) ed.select(hit) }
                                else ed.togglePlay()
                            }
                        }
                    }
                }
                .semantics { contentDescription = "Preview" },
        ) {
            ed.picture?.let { Image(it, contentDescription = null, modifier = Modifier.fillMaxSize(), contentScale = ContentScale.Fit) }
            // the text being typed, where the new clip will put it
            if (ed.typing.isNotBlank() && t != null) {
                val k = with(density) { boxW.toPx() } / t.width.toFloat()
                val px = minOf(t.width, t.height).toFloat() * 0.1f * k
                Text(
                    ed.typing,
                    style = androidx.compose.ui.text.TextStyle(
                        fontFamily = io.github.artcraftandroid.filmcraft.ui.Inter,
                        fontWeight = androidx.compose.ui.text.font.FontWeight.SemiBold,
                        fontSize = with(density) { px.toSp() },
                        color = Color.White,
                        textAlign = androidx.compose.ui.text.style.TextAlign.Center,
                    ),
                    modifier = Modifier.align(Alignment.Center).fillMaxWidth(),
                )
            }
            val pl = placement
            if (movable && pl != null && t != null) {
                Canvas(Modifier.fillMaxSize()) {
                    val k = size.width / t.width.toFloat()
                    val w = (pl.width * pl.scale * k).toFloat()
                    val h = (pl.height * pl.scale * k).toFloat()
                    val cx = (pl.x * k).toFloat()
                    val cy = (pl.y * k).toFloat()
                    val a = Math.toRadians(pl.rotation).toFloat()
                    val corners = listOf(-w / 2 to -h / 2, w / 2 to -h / 2, w / 2 to h / 2, -w / 2 to h / 2).map { (x, y) ->
                        Offset(cx + x * cos(a) - y * sin(a), cy + x * sin(a) + y * cos(a))
                    }
                    val path = Path().apply {
                        moveTo(corners[0].x, corners[0].y)
                        for (c in corners.drop(1)) lineTo(c.x, c.y)
                        close()
                    }
                    drawPath(path, Color.White, style = Stroke(2.dp.toPx()))
                }
            }
            if (object3d && !masking && !advanced && ed.panel != Panel.Lights && ed.panel !is Panel.TextInput && t != null) {
                ObjectGizmoOverlay(ed, t.width.toFloat(), t.height.toFloat(), placement, Modifier.fillMaxSize())
            }
            if (ed.panel == Panel.Lights && t != null) {
                LightGizmoOverlay(ed, t.width.toFloat(), t.height.toFloat())
            }
            val maskState = ed.maskState
            if (masking && maskState != null && t != null) {
                MaskOverlay(ed, maskState, t.width.toFloat(), Modifier.fillMaxSize())
            }
            if (advanced && t != null) {
                AdvancedOverlay(ed, t.width.toFloat(), t.height.toFloat(), Modifier.fillMaxSize())
            }
        }
    }
}

@Suppress("unused")
private fun Dp.half() = this / 2
