package io.github.artcraftandroid.photocraft.studio

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.launch

/** The menus the reference opens from its chrome. */
enum class StudioMenu(val title: String) {
    Document("Document"),
    View("View"),
    Workspace("Workspace"),
    Main("Main menu"),
    MoreTools("More tools"),
    Studios("Studios"),
}

/** A menu entry: its icon, label, optional shortcut and whether it is ticked. */
private data class Entry(
    val label: String,
    val icon: String,
    val shortcut: String? = null,
    val checked: Boolean = false,
    val run: () -> Unit,
)

/** `.editor-popover`: the reference's menu, with its title, icons and shortcuts. */
@Composable
fun StudioPopover(
    which: StudioMenu,
    state: StudioState,
    engine: StudioEngine,
    onDismiss: () -> Unit,
    onDialog: (StudioDialog) -> Unit,
    onHideChrome: () -> Unit,
    onSwitch: (WorkspaceId) -> Unit,
    onCloseAll: () -> Unit,
    onRestorePanel: () -> Unit,
    onResetWorkspace: () -> Unit,
    onOpen: () -> Unit,
) {
    val scope = rememberCoroutineScope()
    val spec = Workspaces.of(state.workspace)

    val view: List<Entry> = listOf(
        Entry("Fit document", "zoom", "⌘0") { state.fit() },
        Entry("Zoom in", "plus") { state.zoom = (state.zoom + 0.15f).coerceAtMost(2.5f) },
        Entry("Zoom out", "minus") { state.zoom = (state.zoom - 0.15f).coerceAtLeast(0.75f) },
        Entry("Show grid", "grid", checked = state.isOn("grid")) { state.toggle("grid") },
        Entry("Hide interface", "eye") { onHideChrome() },
    )
    val document: List<Entry> = listOf(
        Entry("New document…", "plus") { onDialog(StudioDialog.NewDocument) },
        Entry("Open picture…", "pages") { onOpen() },
        Entry("Save document…", "layers") { onOpen() },
        Entry("Export…", "export") { onDialog(StudioDialog.Export) },
        Entry("Preferences…", "settings") { onDialog(StudioDialog.Preferences) },
        Entry("About the studio", "help") { onDialog(StudioDialog.About) },
    )
    val entries: List<Entry> = when (which) {
        StudioMenu.Document -> document
        StudioMenu.View -> view
        StudioMenu.Workspace -> listOf(
            Entry("Reset workspace", "reset") { onResetWorkspace() },
            Entry("Close all panels", "close") { onCloseAll() },
            Entry("Restore default panel", "layers") { onRestorePanel() },
        )
        StudioMenu.Main -> listOf(
            view.first(),
            Entry("Undo", "undo", "⌘Z") {
                scope.launch { runCatching { engine.undo() }.onFailure { StudioErrors.report(it.message ?: "Nothing to undo.") } }
            },
            Entry("Redo", "redo", "⇧⌘Z") {
                scope.launch { runCatching { engine.redo() }.onFailure { StudioErrors.report(it.message ?: "Nothing to redo.") } }
            },
        ) + document.drop(3)
        StudioMenu.MoreTools -> listOf(
            Entry("Deselect", "clear") {
                state.selectedLayer = ""
                scope.launch { runCatching { engine.clearSelection() } }
            },
            Entry("Reset tool", "reset") { state.selectedTool = spec.selected },
            Entry("Keyboard & interface help", "help") { onDialog(StudioDialog.About) },
        )
        StudioMenu.Studios -> listOf(
            Entry("Assets", "assets") { state.togglePanel("Swatches") },
            Entry("Close all panels", "close") { onCloseAll() },
            Entry("Reset studios", "reset") { onRestorePanel() },
        )
    }

    Box(Modifier.fillMaxSize().clickable(onClick = onDismiss)) {
        Column(
            Modifier.padding(start = 66.dp, top = 58.dp).width(242.dp)
                .clip(RoundedCornerShape(12.dp)).background(Sc.surfaceBrush)
                .border(1.dp, Sc.Hairline, RoundedCornerShape(12.dp))
                .padding(6.dp)
                // The popover swallows taps so the scrim below does not dismiss it.
                .clickable(enabled = false) {},
        ) {
            Text(
                which.title,
                fontSize = 10.sp,
                color = Color(0xFF8E9487),
                modifier = Modifier.fillMaxWidth().padding(start = 9.dp, end = 9.dp, top = 5.dp, bottom = 9.dp),
            )
            entries.forEach { entry ->
                Row(
                    Modifier.fillMaxWidth().height(38.dp).clip(RoundedCornerShape(6.dp))
                        .clickable {
                            onDismiss()
                            entry.run()
                        }
                        .padding(horizontal = 9.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(11.dp),
                ) {
                    StudioIcons.Glyph(if (entry.checked) "check" else entry.icon, Color(0xFFD9DFD0), 16.dp)
                    Text(entry.label, fontSize = 12.sp, color = Color(0xFFD9DFD0), modifier = Modifier.weight(1f), maxLines = 1)
                    entry.shortcut?.let { Text(it, fontSize = 10.sp, color = Color(0xFF8E9585)) }
                }
            }
        }
    }
}

/** The dialogs the reference opens: its modal, with the studio's real behaviour inside. */
sealed class StudioDialog {
    data object NewDocument : StudioDialog()
    data object Export : StudioDialog()
    data object Preferences : StudioDialog()
    data object About : StudioDialog()
    data object Canva : StudioDialog()
}

/** `.editor-dialog`: the 440 px modal with the serif logo, the title and the actions. */
@Composable
fun StudioModal(
    which: StudioDialog,
    state: StudioState,
    reducedMotion: Boolean,
    onReducedMotion: (Boolean) -> Unit,
    onClose: () -> Unit,
    onExport: () -> Unit,
    onNewDocument: () -> Unit,
) {
    Box(Modifier.fillMaxSize().background(Color(0x80171A18)).clickable(onClick = onClose), contentAlignment = Alignment.Center) {
        Column(
            // The sheet is as wide as the reference's, and no wider than a phone.
            Modifier.widthIn(max = 440.dp).fillMaxWidth(0.92f).clip(RoundedCornerShape(20.dp)).background(Color(0xFF2A2D27))
                .border(1.dp, Color(0x24FFFFFF), RoundedCornerShape(20.dp))
                .padding(start = 26.dp, end = 26.dp, top = 20.dp, bottom = 26.dp)
                .heightIn(max = 620.dp).verticalScroll(rememberScrollState())
                .clickable(enabled = false) {},
        ) {
            Row(Modifier.fillMaxWidth().padding(bottom = 18.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(
                    "a",
                    fontSize = 37.sp,
                    color = Color(0xFFE0E5D8),
                    fontStyle = FontStyle.Italic,
                    fontWeight = FontWeight.Bold,
                    fontFamily = androidx.compose.ui.text.font.FontFamily.Serif,
                )
                Spacer(Modifier.weight(1f))
                IconButton("close", "Close dialog", onClick = onClose)
            }
            Text(
                when (which) {
                    StudioDialog.NewDocument -> "New document"
                    StudioDialog.Export -> "Export document"
                    StudioDialog.Preferences -> "Preferences"
                    StudioDialog.About -> "About the studio"
                    StudioDialog.Canva -> "Canva AI"
                },
                fontSize = 25.sp,
                fontWeight = FontWeight(550),
                color = Color(0xFFE0E5D8),
                modifier = Modifier.padding(bottom = 12.dp),
            )
            when (which) {
                StudioDialog.NewDocument -> {
                    Text(
                        "A blank canvas at the timeline's size, so what you paint drops straight into the edit.",
                        fontSize = 13.sp,
                        lineHeight = 22.sp,
                        color = Color(0xFFE0E5D8),
                        modifier = Modifier.padding(bottom = 14.dp),
                    )
                    InfoBlock("layers", "PhotoCraft opens the document on its engine: layers, brushes, curves and export are the engine's own.")
                    StandardButton("New document", primary = true) {
                        onNewDocument()
                        onClose()
                    }
                }
                StudioDialog.Export -> {
                    Text(
                        "The document is encoded by PhotoCraft and handed back to FilmCraft as a picture.",
                        fontSize = 13.sp,
                        lineHeight = 22.sp,
                        color = Color(0xFFE0E5D8),
                        modifier = Modifier.padding(bottom = 14.dp),
                    )
                    InfoBlock("export", "PNG, JPEG, WebP and PSD come from the document's own pixels, at its own size.")
                    StandardButton("Export ${state.value("exportFormat").ifEmpty { "PNG" }}", primary = true) {
                        onExport()
                        onClose()
                    }
                }
                StudioDialog.Preferences -> {
                    Text("Interface preferences", fontSize = 13.sp, lineHeight = 22.sp, color = Color(0xFFE0E5D8))
                    Row(
                        Modifier.fillMaxWidth().padding(vertical = 14.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.SpaceBetween,
                    ) {
                        Text("Reduce interface motion", fontSize = 12.sp, color = Color(0xFFE0E5D8))
                        StudioSwitch(reducedMotion) { onReducedMotion(!reducedMotion) }
                    }
                    Text(
                        "Workspace settings are kept while you paint; the document keeps its own history.",
                        fontSize = 12.sp,
                        lineHeight = 20.sp,
                        color = Color(0xFFA5AE99),
                    )
                    Spacer(Modifier.height(14.dp))
                    StandardButton("Back to document", primary = true, onClick = onClose)
                }
                StudioDialog.About -> {
                    Text("A familiar canvas. Four creative workspaces.", fontSize = 13.sp, lineHeight = 22.sp, color = Color(0xFFE0E5D8))
                    Text(
                        "Sketch, Vector, Pixel and Layout, each with its own tools, studios and canvas.",
                        fontSize = 12.sp,
                        lineHeight = 20.sp,
                        color = Color(0xFFA5AE99),
                        modifier = Modifier.padding(top = 8.dp),
                    )
                    InfoBlock("layers", "This is the working editor: its panels drive PhotoCraft's engine, and Export puts the result on the timeline.")
                    Text("Esc closes overlays", fontSize = 10.sp, color = Color(0xFF98A28C))
                    Spacer(Modifier.height(10.dp))
                    StandardButton("Back to document", primary = true, onClick = onClose)
                }
                StudioDialog.Canva -> {
                    Text("A little space for big ideas.", fontSize = 13.sp, lineHeight = 22.sp, color = Color(0xFFE0E5D8))
                    InfoBlock("ai", "AI generation needs a service this app doesn't ship. The studio's tools work on-device instead.")
                    StandardButton("Back to document", primary = true, onClick = onClose)
                }
            }
        }
    }
}

/** `.dialog-info`: the tinted block with its icon. */
@Composable
private fun InfoBlock(icon: String, text: String) {
    Row(
        Modifier.fillMaxWidth().padding(vertical = 10.dp).clip(RoundedCornerShape(10.dp))
            .background(Color(0x2E171B16)).border(1.dp, Color(0x0EFFFFFF), RoundedCornerShape(10.dp))
            .padding(15.dp),
        horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        StudioIcons.Glyph(icon, Color(0xFF8ABCED), 20.dp)
        Text(text, fontSize = 12.sp, lineHeight = 20.sp, color = Color(0xFFC2CBB7))
    }
}

/** A round switch: the reference's preference checkbox, drawn for the dark sheet. */
@Composable
fun StudioSwitch(on: Boolean, onToggle: () -> Unit) {
    Box(
        Modifier.size(width = 26.dp, height = 15.dp).clip(RoundedCornerShape(8.dp))
            .background(if (on) Sc.Blue else Sc.Field)
            .border(1.dp, Color(0x14FFFFFF), RoundedCornerShape(8.dp))
            .clickable(onClick = onToggle),
        contentAlignment = if (on) Alignment.CenterEnd else Alignment.CenterStart,
    ) {
        Box(
            Modifier.padding(horizontal = 2.dp).size(11.dp).clip(RoundedCornerShape(6.dp))
                .background(if (on) Color(0xFFEDF3F1) else Sc.Muted),
        )
    }
}
