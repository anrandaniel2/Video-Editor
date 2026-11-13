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
            if (maxWidth >= 600.dp && maxWidth > maxHeight) SideBySide(ed) else Stacked(ed)
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

@Composable
private fun Stacked(ed: EditorController) {
    val density = LocalDensity.current
    val panel = ed.panel.takeUnless { it == Panel.OnlineVideos || it == Panel.OnlineSounds }
    val textInput = panel as? Panel.TextInput
    val showPanel = panel != null && textInput == null
    Column(Modifier.fillMaxSize().statusBarsPadding()) {
        AnimatedVisibility(panel == null, enter = expandVertically(tween(300)) + fadeIn(tween(300)), exit = shrinkVertically(tween(300)) + fadeOut(tween(300))) {
            TopBar(ed, sideBySide = false)
        }
        BoxWithConstraints(Modifier.weight(1f).fillMaxWidth()) {
            val room = maxHeight
            val minPlayer = 100.dp
            val minTimeline = if (showPanel) 110.dp else 78.dp
            // the player keeps its height when a panel opens (the top bar makes way; the timeline
            // gives up the rest)
            val default = (room + FcSize.ToolTile) * 0.55f
            val wanted = when {
                ed.playerHeight > 0f -> with(density) { ed.playerHeight.toDp() }
                showPanel && ed.basePlayerPx > 0f -> with(density) { ed.basePlayerPx.toDp() }
                else -> default
            }
            val player = wanted.coerceIn(minPlayer, max(minPlayer.value, (room - FcSize.Transport - minTimeline).value).dp)
            if (!showPanel) SideEffect { ed.basePlayerPx = with(density) { player.toPx() } }
            Column(Modifier.fillMaxSize()) {
                Preview(ed, Modifier.fillMaxWidth().height(player))
                TransportRow(ed, onResize = { dy ->
                    val now = if (ed.playerHeight > 0f) ed.playerHeight else with(density) { player.toPx() }
                    ed.playerHeight = (now + dy).coerceIn(with(density) { minPlayer.toPx() }, with(density) { (room - FcSize.Transport - minTimeline).toPx() })
                })
                Timeline(ed, Modifier.fillMaxWidth().weight(1f))
            }
        }
        when {
            showPanel -> EditorPanel(ed, panel!!)
            textInput != null -> TextInputStrip(ed, textInput)
            else -> {
                EditorToolbar(ed)
                Spacer(Modifier.fillMaxWidth().windowInsetsBottomHeight(WindowInsets.navigationBars))
            }
        }
    }
}

@Composable
private fun SideBySide(ed: EditorController) {
    val density = LocalDensity.current
    val panel = ed.panel.takeUnless { it == Panel.OnlineVideos || it == Panel.OnlineSounds }
    val textInput = panel as? Panel.TextInput
    val showPanel = panel != null && textInput == null
    BoxWithConstraints(Modifier.fillMaxSize().statusBarsPadding()) {
        val total = maxWidth
        val room = maxHeight
        val t = ed.timeline
        val aspect = if (t != null && t.height.toInt() > 0) t.width.toFloat() / t.height.toFloat() else 9f / 16f
        val available = total - 60.dp
        val default = min((maxHeight * aspect).value, (available / 2).value).dp
        val minPlayer = 160.dp
        val minTimeline = 340.dp
        val playerW = (if (ed.sideFraction > 0f) available * ed.sideFraction else default).coerceIn(minPlayer, max(minPlayer.value, (available - minTimeline).value).dp)
        // The floating frame bar owns the bottom of the screen: the full-height preview lifts
        // above it, and shrinks to the minimum when there is no room to lift.
        val barSpace = if (frameBarVisible(ed)) FcSize.FrameBar else 0.dp
        val previewH = (room - barSpace).coerceAtLeast(minPlayer)
        Row(Modifier.fillMaxSize()) {
            // transport rail
            Column(Modifier.width(48.dp).fillMaxHeight().editorGlass(), verticalArrangement = Arrangement.Center, horizontalAlignment = Alignment.CenterHorizontally) {
                FcIconButton(Glyphs.StepBack, "Previous frame", { ed.step(-1) }, size = 40.dp, iconSize = 22.dp, modifier = Modifier.testTag("editor_previous_frame_button"))
                FcIconButton(if (ed.playing) Glyphs.Pause else Glyphs.Play, if (ed.playing) "Pause" else "Play", { ed.togglePlay() }, size = 40.dp, iconSize = 22.dp, modifier = Modifier.testTag("editor_play_button"))
                FcIconButton(Glyphs.StepForward, "Next frame", { ed.step(1) }, size = 40.dp, iconSize = 22.dp, modifier = Modifier.testTag("editor_next_frame_button"))
            }
            Preview(ed, Modifier.width(playerW).height(previewH))
            // the divider between the monitor and the timeline
            Box(
                Modifier
                    .width(12.dp)
                    .fillMaxHeight()
                    .pointerInput(available) {
                        detectHorizontalDragGestures { change, dx ->
                            change.consume()
                            val now = ed.sideFraction.takeIf { it > 0f } ?: (playerW / available)
                            ed.sideFraction = (now + dx / with(density) { available.toPx() }).coerceIn(minPlayer / available, ((available - minTimeline) / available).coerceAtLeast(minPlayer / available))
                        }
                    }
                    .testTag("editor_side_by_side_divider")
                    .semantics { contentDescription = "Resize monitor" },
                contentAlignment = Alignment.Center,
            ) {
                Box(Modifier.size(width = 4.dp, height = 32.dp).clip(CircleShape).background(Fc.Detail))
            }
            Column(Modifier.weight(1f).fillMaxHeight()) {
                AnimatedVisibility(!showPanel) { TopBar(ed, sideBySide = true) }
                Timeline(ed, Modifier.fillMaxWidth().weight(1f))
                when {
                    showPanel -> Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.BottomCenter) {
                        // never more than most of the height: the timeline stays in sight
                        Box(Modifier.width(min((total - playerW - 60.dp).value, 640f).dp).heightIn(max = room * 0.72f)) { EditorPanel(ed, panel!!) }
                    }
                    textInput != null -> TextInputStrip(ed, textInput)
                    else -> {
                        EditorToolbar(ed)
                        Spacer(Modifier.fillMaxWidth().windowInsetsBottomHeight(WindowInsets.navigationBars))
                    }
                }
            }
        }
    }
}

@Composable
private fun TopBar(ed: EditorController, sideBySide: Boolean) {
    Row(Modifier.fillMaxWidth().height(FcSize.TopBar).editorGlass(0.dp).padding(horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically) {
        FcIconButton(LucideIcons.House.glyph(), "Home", { ed.closeEditor() }, modifier = Modifier.testTag("editor_home_button"))
        Spacer(Modifier.weight(1f))
        if (sideBySide) {
            val st = ed.status
            FcIconButton(LucideIcons.Undo2.glyph(), "Undo", { ed.undo() }, enabled = st?.canUndo == true, modifier = Modifier.testTag("editor_top_bar_undo_button"))
            FcIconButton(LucideIcons.Redo2.glyph(), "Redo", { ed.redo() }, enabled = st?.canRedo == true, modifier = Modifier.testTag("editor_top_bar_redo_button"))
            Spacer(Modifier.width(8.dp))
        }
        EditorExportButton({ ed.export() }, enabled = (ed.timeline?.duration ?: 0L) > 0L, modifier = Modifier.testTag("editor_export_button"))
    }
}

@Composable
private fun TransportRow(ed: EditorController, onResize: (Float) -> Unit) {
    val st = ed.status
    Row(Modifier.fillMaxWidth().height(FcSize.Transport).editorGlass(0.dp).padding(horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically) {
        Box(
            Modifier
                .size(48.dp)
                .pointerInput(Unit) { detectVerticalDragGestures { change, dy -> change.consume(); onResize(dy) } }
                .testTag("editor_resize_player_button")
                .semantics { contentDescription = "Resize player" },
            contentAlignment = Alignment.Center,
        ) {
            io.github.artcraftandroid.filmcraft.ui.GlyphIcon(LucideIcons.ChevronsUpDown.glyph(), Fc.Icon, size = 22.dp)
        }
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
