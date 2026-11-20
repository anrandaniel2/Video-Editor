package io.github.artcraftandroid.procraft.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.artcraftandroid.procraft.canvas.CanvasViewModel
import io.github.artcraftandroid.procraft.core.QuickShapeKind

/** The drawable kinds, in rows: what a held stroke can snap to (a plugin's curve is not one of them). */
private val DrawableKinds = listOf(
    QuickShapeKind.LINE, QuickShapeKind.POLYLINE, QuickShapeKind.ARC, QuickShapeKind.ELLIPSE, QuickShapeKind.CIRCLE,
    QuickShapeKind.TRIANGLE, QuickShapeKind.EQUILATERAL_TRIANGLE, QuickShapeKind.QUADRILATERAL, QuickShapeKind.RECTANGLE,
    QuickShapeKind.SQUARE,
)

/**
 * The host's shapes library (FilmCraft only, gone when procraft ships on its own): the shapes the
 * QuickShape recognition can snap a stroke to, on the canvas itself.
 *
 * It does not draw anything and it never forks the shape code: the stroke is painted by the normal
 * brush path, the snapping and the outline are QuickShape's, and the handles shown while a shape is
 * selected are the ones QuickShapeHandles draws. The panel only offers the same choices the snapped
 * shape's bar does, and drives the same entry points ([CanvasViewModel.chooseQuickShape],
 * [CanvasViewModel.quickShapeEditing], [CanvasViewModel.endQuickShape]) — one more place that asks,
 * not a second engine.
 *
 * [arrowX] is where the arrow points from the panel's left edge; the caller hangs it from the
 * shapes button like the other popovers.
 */
@Composable
fun ShapesLibraryPanel(vm: CanvasViewModel, arrowX: Dp, modifier: Modifier = Modifier) {
    val info = vm.quickShape
    Popover(330.dp, arrowX, modifier) {
        // a 52 header, the title as the other panels' own, with a way out at the trailing end
        Box(Modifier.fillMaxWidth().height(52.dp).padding(start = 24.dp, top = 17.dp, end = 8.dp), contentAlignment = Alignment.CenterStart) {
            Text("Shapes", color = Pc.PanelTitle, fontSize = 24.sp, fontWeight = FontWeight.Bold, maxLines = 1)
            Box(
                Modifier.align(Alignment.CenterEnd).size(44.dp, 52.dp).tap { vm.hostShapes = false; vm.endQuickShape() },
                contentAlignment = Alignment.Center,
            ) {
                Icon(PcIcons.Cross, contentDescription = "Close the shapes", tint = Pc.PanelTitle, modifier = Modifier.size(20.dp))
            }
        }
        Column(Modifier.fillMaxWidth().padding(start = 10.dp, top = 8.dp, end = 10.dp, bottom = 10.dp)) {
            // five across, two rows; the pair sits on the panels' own card
            for (row in 0 until 2) {
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(2.dp)) {
                    for (col in 0 until 5) {
                        val kind = DrawableKinds[row * 5 + col]
                        ShapeCell(vm, kind, Modifier.weight(1f))
                    }
                }
                if (row == 0) Spacer(Modifier.height(2.dp))
            }
            Spacer(Modifier.height(10.dp))
            Box(Modifier.fillMaxWidth().height(1.dp).background(Pc.Divider))
            Text(
                when {
                    info == null -> "Draw a rough shape and lift; the stroke snaps to the closest of these, painted with the current brush."
                    info.kind == QuickShapeKind.CURVE -> "A plugin's curve, with its own handles. Tap a cell to show it as that shape instead."
                    vm.quickShapeEditing -> "Editing: drag the handles on the canvas. The shape keeps its kind."
                    else -> "It snapped. Tap the selected cell to show its handles, or another to show it as that instead."
                },
                Modifier.fillMaxWidth().padding(top = 10.dp, start = 4.dp, end = 4.dp),
                color = Pc.ActionsRow, fontSize = 12.sp, lineHeight = 17.sp, maxLines = 3,
            )
        }
    }
}

/** One cell: the shape's own kind glyph and name. Dimmed while a snapped shape can't be shown as it. */
@Composable
private fun ShapeCell(vm: CanvasViewModel, kind: QuickShapeKind, modifier: Modifier = Modifier) {
    val info = vm.quickShape
    val chosen = info?.kind == kind
    val applicable = info == null || chosen || kind in info.choices
    Column(
        modifier
            .clip(RoundedCornerShape(13.dp))
            .background(if (chosen) Pc.Accent else Pc.ActionsGroup)
            .tap { pick(vm, kind) }
            .alpha(if (applicable) 1f else 0.38f)
            .padding(vertical = 9.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(5.dp),
    ) {
        val ink = if (chosen) Color.White else Pc.PanelGlyph
        Icon(kind.quickshapeIcon(), contentDescription = kind.quickshapeLabel(), tint = ink, modifier = Modifier.size(26.dp))
        Text(kind.quickshapeLabel(), color = ink, fontSize = 9.5.sp, maxLines = 1, fontWeight = if (chosen) FontWeight.Bold else FontWeight.Normal)
    }
}

/** What tapping a [kind] does: shows a snapped shape as it, or asks for a stroke when there is none. */
private fun pick(vm: CanvasViewModel, kind: QuickShapeKind) {
    val info = vm.quickShape
    if (info == null) {
        vm.showNotice("Draw a rough shape and lift; it snaps to the closest of these.")
        return
    }
    if (kind == info.kind) {
        vm.quickShapeEditing = true
        return
    }
    if (kind in info.choices) vm.chooseQuickShape(kind) else vm.showNotice("This shape can't be shown as that.", warning = true)
}
