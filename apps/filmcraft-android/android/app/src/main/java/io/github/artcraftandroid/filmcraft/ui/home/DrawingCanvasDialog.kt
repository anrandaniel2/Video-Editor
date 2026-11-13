package io.github.artcraftandroid.filmcraft.ui.home

import androidx.compose.foundation.layout.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import io.github.artcraftandroid.filmcraft.ui.ImmersiveDialog
import io.github.artcraftandroid.filmcraft.ui.editor.rememberProcraftModel
import io.github.artcraftandroid.procraft.gallery.CanvasPreset
import io.github.artcraftandroid.procraft.ui.*

/** The actual ProCraft preset popup and custom-canvas editor, sharing its saved presets. */
@Composable
internal fun DrawingCanvasDialog(close: () -> Unit, create: (CanvasPreset) -> Unit) {
    val vm = rememberProcraftModel()
    Dialog(onDismissRequest = close, properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false)) {
        ImmersiveDialog()
        ProCraftTheme {
            if (vm.customCanvasOpen) {
                Box(Modifier.fillMaxSize()) {
                    CustomCanvasScreen(vm, onCreate = create) { vm.closeCustomCanvas() }
                }
            } else {
                NewCanvasPanel(
                    modifier = Modifier.widthIn(max = 347.dp), arrowX = 173.dp,
                    presets = vm.canvasPresets,
                    onCustom = { vm.openCustomCanvas() },
                    onEdit = { vm.openCustomCanvas(it) }, onDelete = vm::deletePreset,
                    onPick = create,
                    onScreenSize = { w, h -> create(CanvasPreset("Screen Size", "px", w.toFloat(), h.toFloat(), 300f, profile = CanvasPreset.P3)) },
                )
            }
        }
    }
}
