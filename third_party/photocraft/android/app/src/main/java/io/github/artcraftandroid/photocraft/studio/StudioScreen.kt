package io.github.artcraftandroid.photocraft.studio

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.launch

/**
 * The studio: the reference UI's editor, ported to Compose and wired to PhotoCraft.
 *
 * The arrangement is the reference's, measured from its stylesheet: the tool rail at the left with
 * the "a" logo above it, the workspace pills at 64/12, the studios at the right with the colour well
 * on top, the floating panels between them, the node toolbar and the Sketch quick controls at the
 * left, the text bar across the bottom for Layout, and the popover, dialog and toast overlays.
 *
 * Everything that touches the picture goes through [StudioEngine], so the Layers panel lists the
 * document's own tree, the curve becomes an adjustment layer, and Export hands the engine's bytes
 * back to FilmCraft. The actions that were mock-only in the reference (its dialogs, its Canva AI
 * notice) are kept, saying what the studio really does instead.
 */
@Composable
fun StudioScreen(
    engine: StudioEngine,
    initialWidth: Int,
    initialHeight: Int,
    title: String,
    onClose: () -> Unit,
    onExport: (format: String, bytes: ByteArray, name: String) -> Unit,
    onPickPicture: () -> Unit,
    onOpenClassic: () -> Unit = {},
) {
    val states = remember {
        WorkspaceId.entries.associateWith { StudioState(it) }
    }
    var workspace by remember { mutableStateOf(WorkspaceId.Vector) }
    val state = states.getValue(workspace)
    val accent = Sc.accentOf(workspace)
    val scope = rememberCoroutineScope()

    var menu by remember { mutableStateOf<StudioMenu?>(null) }
    var dialog by remember { mutableStateOf<StudioDialog?>(null) }
    var toast by remember { mutableStateOf("") }
    var chromeHidden by remember { mutableStateOf(false) }
    var reducedMotion by remember { mutableStateOf(false) }

    val actions = remember(engine, state, accent) {
        StudioActions(
            engine = engine,
            state = state,
            accent = accent,
            feedback = { toast = it },
            export = {
                scope.launch {
                    val format = state.value("exportFormat").ifEmpty { "PNG" }.lowercase()
                    runCatching { engine.export(format, 90) }
                        .onSuccess { onExport(format, it, title) }
                        .onFailure { toast = it.message ?: "The export failed." }
                }
            },
            openDocument = onPickPicture,
            saveDocument = { onPickPicture() },
            classicEditor = onOpenClassic,
        )
    }

    // The engine's refusals reach the user the way the reference's notices do.
    DisposableEffect(Unit) {
        StudioErrors.sink = { toast = it }
        onDispose { StudioErrors.sink = null }
    }

    // The document, once: its size, its layers, the brush presets.
    LaunchedEffect(engine) {
        engine.refreshAll()
        engine.refreshBrushes()
        states.values.forEach {
            it.docWidth = engine.width.coerceAtLeast(initialWidth)
            it.docHeight = engine.height.coerceAtLeast(initialHeight)
        }
    }

    // A toast clears itself, as the reference's does after 4.2 s.
    LaunchedEffect(toast) {
        if (toast.isNotEmpty()) {
            kotlinx.coroutines.delay(4200)
            toast = ""
        }
    }

    BackHandler {
        // Esc closes the overlays first, then the editor: the reference's own order.
        when {
            menu != null -> menu = null
            dialog != null -> dialog = null
            chromeHidden -> chromeHidden = false
            else -> onClose()
        }
    }

    StudioTheme(accent) {
        // The studio owns the whole window, so it must keep clear of the system bars itself. The
        // app draws edge to edge, and nothing above this composable pads for the status bar: the
        // persona bar was starting at y=0 and the status bar was drawing over its top, which is
        // why the four workspace names kept reading low no matter how their own line box was set.
        Box(
            Modifier.fillMaxSize()
                .background(Sc.artcraftBackdrop(workspace))
                .statusBarsPadding(),
        ) {
            StudioCanvas(
                engine = engine,
                state = state,
                modifier = Modifier.fillMaxSize(),
                onPickedColour = { hex ->
                    state.setValue("colour", hex.lowercase())
                    toast = "Picked ${hex.uppercase()}."
                },
                onFailure = { toast = it },
                onPlacePicture = onPickPicture,
            )

            if (!chromeHidden) {
                ToolRail(
                    state = state,
                    onDocumentMenu = { menu = StudioMenu.Document },
                    onMore = { menu = StudioMenu.MoreTools },
                    onDeselect = {
                        state.selectedLayer = ""
                        scope.launch { runCatching { engine.clearSelection() } }
                        toast = "Selection cleared."
                    },
                    onDelete = {
                        scope.launch {
                            runCatching { engine.deleteLayer() }
                                .onSuccess { toast = "The active layer was deleted." }
                                .onFailure { toast = it.message ?: "Nothing to delete." }
                        }
                    },
                )
                PersonaBar(
                    state = state,
                    onSwitch = { id ->
                        workspace = id
                        menu = null
                        dialog = null
                        toast = ""
                        states.getValue(id).fit()
                    },
                    onCanva = { dialog = StudioDialog.Canva },
                    onWorkspaceMenu = { menu = StudioMenu.Workspace },
                    onDocumentMenu = { menu = StudioMenu.Document },
                    onViewMenu = { menu = StudioMenu.View },
                    onSnapping = { state.toggle("snapping") },
                    onMainMenu = { menu = StudioMenu.Main },
                )
                StudioRail(
                    state = state,
                    onColour = { state.togglePanel("Colour") },
                    onMore = { menu = StudioMenu.Studios },
                    onHelp = { dialog = StudioDialog.About },
                    modifier = Modifier.align(Alignment.TopEnd).padding(top = 10.dp, end = 10.dp),
                )
                // The panels open beside the rail they were tapped on and stay there: every one
                // is anchored to an edge and offset by the rail's own width, so opening a second
                // and third stacks them inward from the same edge instead of landing in the
                // middle of the canvas.
                //
                // The rail is 42 wide at the right edge with a 10 margin, so a panel that hugs it
                // sits at end = 62; the ones on the left sit clear of the tool rail. A couple are
                // deliberately elsewhere -- Pages belongs to the document and opens at the top
                // left, Curves opens under the persona bar -- and those keep their own place.
                val spec = Workspaces.of(workspace)
                val railGap = 62.dp
                state.panels.forEachIndexed { index, name ->
                    val extra = index > 0
                    val own = name == spec.panel
                    val place = when (name) {
                        "Pages" -> Modifier.align(Alignment.TopStart).padding(start = 66.dp, top = 72.dp)
                        "Brushes" -> Modifier.align(Alignment.BottomStart).padding(
                            start = if (workspace == WorkspaceId.Sketch) 109.dp else 68.dp,
                            bottom = 24.dp,
                        )
                        // Stack inward from the rail: the second panel sits one panel-width
                        // further in than the first, so it does not cover it.
                        else -> Modifier.align(Alignment.TopEnd).padding(
                            top = 12.dp,
                            end = railGap + 246.dp * (if (extra) index else 0),
                        )
                    }
                    StudioPanel(
                        name = name,
                        a = actions,
                        onPin = { state.pin(name) },
                        onClose = { state.togglePanel(name) },
                        modifier = place,
                    )
                }
                if (workspace == WorkspaceId.Vector) {
                    NodeToolbar(
                        state = state,
                        engine = engine,
                        onStroke = { state.togglePanel("Stroke") },
                        onFailure = { toast = it },
                        modifier = Modifier.align(Alignment.BottomStart),
                    )
                }
                if (workspace == WorkspaceId.Sketch) {
                    SketchQuickControls(state, modifier = Modifier.align(Alignment.BottomStart))
                }
                if (workspace != WorkspaceId.Sketch) {
                    QuickColourRing(
                        state = state,
                        onClick = { state.togglePanel("Colour") },
                        modifier = Modifier.align(Alignment.BottomStart),
                    )
                }
                if (workspace == WorkspaceId.Layout) {
                    TextContextBar(state, modifier = Modifier.align(Alignment.BottomCenter))
                }
            }

            if (chromeHidden) {
                Row(
                    Modifier.align(Alignment.TopStart).padding(15.dp)
                        .clip(RoundedCornerShape(10.dp)).background(Sc.surfaceBrush)
                        .border(1.dp, Sc.Hairline, RoundedCornerShape(10.dp))
                        .clickable { chromeHidden = false }
                        .padding(12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    StudioIcons.Glyph("menu", Sc.IconInk, 18.dp)
                    Spacer(Modifier.width(10.dp))
                    Text("Show interface", fontSize = 12.sp, color = Sc.Title)
                    Spacer(Modifier.width(10.dp))
                    Text("Esc", fontSize = 10.sp, color = Color(0xFF8F9981))
                }
            }

            menu?.let { which ->
                StudioPopover(
                    which = which,
                    state = state,
                    engine = engine,
                    onDismiss = { menu = null },
                    onDialog = { dialog = it; menu = null },
                    onHideChrome = { chromeHidden = true; menu = null },
                    onSwitch = { id -> workspace = id; menu = null },
                    onCloseAll = { state.panels = emptyList(); state.pinned = emptyList() },
                    onRestorePanel = { state.panels = listOf(Workspaces.of(state.workspace).panel); state.pinned = emptyList() },
                    onResetWorkspace = {
                        state.reset(state.workspace)
                        toast = "${state.workspace.label} workspace reset."
                    },
                    onOpen = onPickPicture,
                )
            }

            dialog?.let { which ->
                StudioModal(
                    which = which,
                    state = state,
                    reducedMotion = reducedMotion,
                    onReducedMotion = { reducedMotion = it },
                    onClose = { dialog = null },
                    onExport = { actions.export() },
                    onNewDocument = {
                        scope.launch {
                            runCatching { engine.newDocument(state.docWidth, state.docHeight) }
                                .onSuccess { toast = "A new document is open." }
                                .onFailure { toast = it.message ?: "The document couldn't be made." }
                        }
                    },
                )
            }

            if (toast.isNotEmpty()) {
                Row(
                    Modifier.align(Alignment.BottomCenter).padding(bottom = 24.dp)
                        .widthIn(max = 520.dp)
                        .clip(RoundedCornerShape(11.dp)).background(Color(0xF5282B25))
                        .border(1.dp, Color(0x20FFFFFF), RoundedCornerShape(11.dp))
                        .padding(horizontal = 14.dp, vertical = 13.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    StudioIcons.Glyph("help", Color(0xFFA1C3E8), 17.dp)
                    Spacer(Modifier.width(10.dp))
                    Text(toast, fontSize = 12.sp, lineHeight = 18.sp, color = Color(0xFFDEE3D6), modifier = Modifier.weight(1f, fill = false))
                    Spacer(Modifier.width(8.dp))
                    Box(Modifier.clickable { toast = "" }) {
                        StudioIcons.Glyph("close", Color(0xFF9EA794), 15.dp)
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------- tool rail

/** The left rail: the "a" logo, the workspace's tools, and the footer's three actions. */
@Composable
private fun ToolRail(
    state: StudioState,
    onDocumentMenu: () -> Unit,
    onMore: () -> Unit,
    onDeselect: () -> Unit,
    onDelete: () -> Unit,
) {
    val spec = Workspaces.of(state.workspace)
    Column(
        Modifier.padding(start = 10.dp, top = 10.dp).width(42.dp)
            .clip(RoundedCornerShape(18.dp)).background(Sc.surfaceBrush)
            .border(1.dp, Sc.Hairline, RoundedCornerShape(18.dp))
            .padding(start = 4.dp, end = 4.dp, bottom = 7.dp)
            .heightIn(max = 800.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        // `.affinity-logo`: the serif "a", which opens the document menu.
        Box(
            Modifier.width(32.dp).height(46.dp).clickable(onClick = onDocumentMenu),
            contentAlignment = Alignment.Center,
        ) {
            Text(
                "a",
                fontSize = 32.sp,
                color = Color(0xFFFAFAF5),
                fontStyle = FontStyle.Italic,
                fontWeight = FontWeight.Bold,
                fontFamily = androidx.compose.ui.text.font.FontFamily.Serif,
            )
        }
        Box(Modifier.width(32.dp).height(1.dp).background(Color(0x18FFFFFF)))
        Spacer(Modifier.height(7.dp))
        Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(1.dp)) {
            spec.tools.forEach { tool ->
                Box(Modifier.size(width = 30.dp, height = 30.dp), contentAlignment = Alignment.Center) {
                    IconButton(
                        icon = tool.icon,
                        label = tool.label,
                        active = state.selectedTool == tool.id,
                        iconSize = 19.dp,
                    ) { state.selectedTool = tool.id }
                }
            }
        }
        // `.tool-rail-footer`
        Spacer(Modifier.height(7.dp))
        Box(Modifier.width(30.dp).height(1.dp).background(Color(0x14FFFFFF)))
        Spacer(Modifier.height(3.dp))
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(1.dp)) {
            IconButton("more", "More tools", onClick = onMore)
            IconButton("clear", "Deselect all", onClick = onDeselect)
            IconButton("trash", "Delete selection", onClick = onDelete)
        }
    }
}

// ---------------------------------------------------------------------------- persona bar

/** `.persona-bar`: the four workspaces, Canva AI, and the five chrome actions. */
@Composable
private fun PersonaBar(
    state: StudioState,
    onSwitch: (WorkspaceId) -> Unit,
    onCanva: () -> Unit,
    onWorkspaceMenu: () -> Unit,
    onDocumentMenu: () -> Unit,
    onViewMenu: () -> Unit,
    onSnapping: () -> Unit,
    onMainMenu: () -> Unit,
) {
    // Height follows the tabs rather than cutting them off: with a fixed 42 and 4 either side
    // the 30-tall rows had 34 to sit in and were pushed down, which is why the workspace names
    // read low against the bar.
    Row(
        Modifier.padding(start = 64.dp, top = 12.dp).heightIn(min = 42.dp)
            .clip(RoundedCornerShape(16.dp)).background(Sc.surfaceBrush)
            .border(1.dp, Sc.Hairline, RoundedCornerShape(16.dp))
            .padding(horizontal = 6.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(3.dp),
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(2.dp), verticalAlignment = Alignment.CenterVertically) {
            Workspaces.order.forEach { id ->
                val on = state.workspace == id
                Row(
                    // A fixed 30 with 6 either side left 18 for a 16dp glyph and a 12sp word,
                    // which pushed the row's contents up and out of its own box. The tab now
                    // takes its height from what it holds, with a minimum for the touch target.
                    Modifier.heightIn(min = 30.dp).clip(RoundedCornerShape(8.dp))
                        // The selected tab wears its workspace's accent, with dark ink.
                        .background(if (on) Sc.accentOf(id) else Color.Transparent)
                        .clickable { onSwitch(id) }
                        .padding(horizontal = 8.dp, vertical = 4.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(5.dp),
                ) {
                    StudioIcons.Glyph(
                        when (id) {
                            WorkspaceId.Sketch -> "sketch"
                            WorkspaceId.Vector -> "vector"
                            WorkspaceId.Pixel -> "pixel"
                            WorkspaceId.Layout -> "layout"
                        },
                        if (on) Color(0xFF151816) else Color(0xFFC7C8C4),
                        16.dp,
                    )
                    // Centre the drawn word on the icon, not on the font's line box. A line box
                    // reserves descender space that a capitalised word never uses, so with the
                    // default line height the label sat visibly below the 16dp glyph beside it --
                    // the "too low" this kept being. Trimming the line height to the font size and
                    // centring what is left removes that slack.
                    Text(
                        text = id.label,
                        color = if (on) Color(0xFF151816) else Color(0xFFC7C8C4),
                        maxLines = 1,
                        style = androidx.compose.ui.text.TextStyle(
                            fontSize = 12.sp,
                            fontWeight = if (on) FontWeight(650) else FontWeight.Normal,
                            lineHeight = 12.sp,
                            lineHeightStyle = androidx.compose.ui.text.style.LineHeightStyle(
                                alignment = androidx.compose.ui.text.style.LineHeightStyle.Alignment.Center,
                                trim = androidx.compose.ui.text.style.LineHeightStyle.Trim.Both,
                            ),
                        ),
                    )
                }
            }
        }
        Row(
            Modifier.height(28.dp).clip(RoundedCornerShape(7.dp)).clickable(onClick = onCanva).padding(horizontal = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(5.dp),
        ) {
            StudioIcons.Glyph("ai", Color(0xFFD5D5D0), 16.dp)
            Text("Canva AI", fontSize = 12.sp, color = Color(0xFFD5D5D0), maxLines = 1)
        }
        IconButton("more", "Workspace options", small = false, iconSize = 16.dp) { onWorkspaceMenu() }
        Box(Modifier.width(1.dp).height(21.dp).background(Color(0x15FFFFFF)))
        IconButton("pages", "Document options", active = true, iconSize = 19.dp) { onDocumentMenu() }
        IconButton("grid", "View options", active = state.isOn("grid"), iconSize = 19.dp) { onViewMenu() }
        IconButton("snap", "Snapping", active = state.isOn("snapping"), iconSize = 19.dp) { onSnapping() }
        IconButton("menu", "Main menu", iconSize = 19.dp) { onMainMenu() }
    }
}

// ---------------------------------------------------------------------------- studio rail

/** `.studio-rail`: the colour well above the twelve studios, then more/help. */
@Composable
private fun StudioRail(
    state: StudioState,
    onColour: () -> Unit,
    onMore: () -> Unit,
    onHelp: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier.width(42.dp).clip(RoundedCornerShape(17.dp)).background(Sc.surfaceBrush)
            .border(1.dp, Sc.Hairline, RoundedCornerShape(17.dp))
            .padding(horizontal = 4.dp, vertical = 8.dp)
            .heightIn(max = 760.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        // `.studio-colour`: the current colour, 25 px square in its 29 px well.
        Box(
            Modifier.size(width = 29.dp, height = 30.dp).clickable(onClick = onColour),
            contentAlignment = Alignment.Center,
        ) {
            Box(
                Modifier.size(25.dp).clip(RoundedCornerShape(6.dp))
                    .background(parseColour(state.value("colour")))
                    .border(1.dp, Color(0x33FFFFFF), RoundedCornerShape(6.dp)),
            )
        }
        if (state.workspace == WorkspaceId.Layout) {
            RailButton("pages", "Pages studio", active = state.panels.contains("Pages")) { state.togglePanel("Pages") }
        }
        Workspaces.studios.filter { it.name != "Colour" }.forEach { studio ->
            RailButton(
                icon = studio.icon,
                label = "${studio.label} studio",
                active = state.panels.contains(studio.name),
            ) { state.togglePanel(studio.name) }
        }
        Box(Modifier.width(21.dp).height(1.dp).background(Color(0x13FFFFFF)))
        RailButton("more", "More studios") { onMore() }
        RailButton("help", "Interface help") { onHelp() }
    }
}

/**
 * A studio button: its icon sits on a pale tile, as `.studio-rail > .icon-button svg` draws it, and
 * the active one loses the tile and turns white.
 */
@Composable
private fun RailButton(icon: String, label: String, active: Boolean = false, onClick: () -> Unit) {
    Box(
        Modifier.size(width = 29.dp, height = 29.dp).clip(RoundedCornerShape(6.dp)).clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Box(
            Modifier.size(22.dp).clip(RoundedCornerShape(4.dp))
                .background(if (active) Color.Transparent else Color(0xFFB8B9B3)),
            contentAlignment = Alignment.Center,
        ) {
            StudioIcons.Glyph(icon, if (active) Color.White else Color(0xFF323431), 18.dp)
        }
    }
}

// ---------------------------------------------------------------------------- overlays

/** The Vector workspace's node toolbar: six node actions, the stroke reading, and more. */
@Composable
private fun NodeToolbar(
    state: StudioState,
    engine: StudioEngine,
    onStroke: () -> Unit,
    onFailure: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val scope = rememberCoroutineScope()
    Column(
        modifier.padding(start = 64.dp, bottom = 56.dp).width(43.dp)
            .clip(RoundedCornerShape(18.dp)).background(Sc.surfaceBrush)
            .border(1.dp, Sc.Hairline, RoundedCornerShape(18.dp))
            .padding(start = 4.dp, end = 4.dp, top = 10.dp, bottom = 7.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        MiniGrip(width = 23.dp)
        // The labels against the engine's own actions: each one edits the selected path's nodes,
        // so pressing it changes the drawing rather than only which button looks pressed.
        listOf(
            "node" to "Sharp node", "curve" to "Smooth node", "circle" to "Smart node",
            "transform" to "Break curve", "vector" to "Join curves", "stroke" to "Close curve",
        ).forEach { (icon, label) ->
            val action = when (label) {
                "Sharp node" -> "sharp"
                "Smooth node" -> "smooth"
                "Smart node" -> "smart"
                "Break curve" -> "break"
                "Join curves" -> "join"
                else -> "close"
            }
            IconButton(icon, label, small = true, active = state.value("nodeAction") == label, iconSize = 18.dp) {
                state.setValue("nodeAction", label)
                scope.launch {
                    runCatching {
                        engine.run("shape.editNodes", org.json.JSONObject().put("action", action))
                    }.onFailure { onFailure(it.message ?: "There is no path selected to edit.") }
                }
            }
        }
        Column(Modifier.clickable(onClick = onStroke), horizontalAlignment = Alignment.CenterHorizontally) {
            StudioIcons.Glyph("eye", Color(0xFFBFC3B6), 22.dp)
            Text("${state.value("strokeWidth")} pt", fontSize = 9.sp, color = Color(0xFFBFC3B6))
        }
        IconButton("more", "More node options", small = true) { state.toggle("nodeOptions") }
    }
}

/** The Sketch workspace's quick controls: a tall narrow rail of the brush's settings. */
@Composable
private fun SketchQuickControls(state: StudioState, modifier: Modifier = Modifier) {
    Column(
        modifier.padding(start = 65.dp, bottom = 20.dp).width(35.dp).height(430.dp)
            .clip(RoundedCornerShape(15.dp)).background(Sc.surfaceBrush)
            .border(1.dp, Sc.Hairline, RoundedCornerShape(15.dp))
            .padding(horizontal = 4.dp, vertical = 10.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.SpaceBetween,
    ) {
        MiniGrip(width = 18.dp, height = 4.dp)
        // The reference's range inputs run vertically down the rail.
        VerticalSlider(
            fraction = ((state.value("brushSize").toFloatOrNull() ?: 24f) - 1f) / 499f,
            height = 150.dp,
        ) { f -> state.setValue("brushSize", (1f + f * 499f).toInt().toString()) }
        Box(
            Modifier.size(24.dp).clip(RoundedCornerShape(7.dp))
                .background(parseColour(state.value("colour")))
                .border(1.dp, Color(0x55FFFFFF), RoundedCornerShape(7.dp))
                .clickable { state.togglePanel("Colour") },
        )
        VerticalSlider(
            fraction = (state.value("brushOpacity").toFloatOrNull() ?: 100f) / 100f,
            height = 150.dp,
        ) { f -> state.setValue("brushOpacity", (f * 100f).toInt().toString()) }
        IconButton("more", "Brush settings", small = true) { state.togglePanel("Brushes") }
    }
}

/** A range input rotated to run vertically, as the reference's Sketch rail has it. */
@Composable
private fun VerticalSlider(fraction: Float, height: androidx.compose.ui.unit.Dp, onFraction: (Float) -> Unit) {
    Box(
        Modifier.width(15.dp).height(height)
            .pointerInput(Unit) {
                detectDragGestures(
                    onDragStart = { pos -> onFraction(1f - (pos.y / size.height).coerceIn(0f, 1f)) },
                ) { change, _ ->
                    onFraction(1f - (change.position.y / size.height).coerceIn(0f, 1f))
                    change.consume()
                }
            },
        contentAlignment = Alignment.Center,
    ) {
        androidx.compose.foundation.Canvas(Modifier.width(15.dp).height(height)) {
            val cx = size.width / 2f
            val track = 3.dp.toPx()
            drawRoundRect(
                Sc.SliderTrack,
                androidx.compose.ui.geometry.Offset(cx - track / 2f, 0f),
                androidx.compose.ui.geometry.Size(track, size.height),
                androidx.compose.ui.geometry.CornerRadius(track / 2f),
            )
            val y = (1f - fraction.coerceIn(0f, 1f)) * size.height
            drawRoundRect(
                Sc.Blue,
                androidx.compose.ui.geometry.Offset(cx - track / 2f, y),
                androidx.compose.ui.geometry.Size(track, size.height - y),
                androidx.compose.ui.geometry.CornerRadius(track / 2f),
            )
            drawCircle(Color(0xFFE9EDEA), 5.dp.toPx(), androidx.compose.ui.geometry.Offset(cx, y))
        }
    }
}

/** `.quick-colour-ring`: the round colour well the non-Sketch workspaces show. */
@Composable
private fun QuickColourRing(state: StudioState, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val (start, bottom) = when (state.workspace) {
        WorkspaceId.Pixel -> 118.dp to 82.dp
        WorkspaceId.Layout -> 171.dp to 97.dp
        else -> 123.dp to 103.dp
    }
    Box(
        modifier.padding(start = start, bottom = bottom)
            .size(45.dp).clip(RoundedCornerShape(23.dp))
            .background(Color(0xDCFFFFFF))
            .border(2.dp, Color(0xFF999C96), RoundedCornerShape(23.dp))
            .clickable(onClick = onClick)
            .padding(3.dp),
    ) {
        Box(Modifier.fillMaxSize().clip(RoundedCornerShape(20.dp)).background(parseColour(state.value("colour"))))
    }
}

/** `.text-context-bar`: the Layout workspace's text controls across the bottom. */
@Composable
private fun TextContextBar(state: StudioState, modifier: Modifier = Modifier) {
    Row(
        modifier.fillMaxWidth().heightIn(min = 57.dp)
            .background(Color(0xFF2D2E2C)).padding(start = 65.dp, end = 65.dp, top = 11.dp, bottom = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        TextControlsRow(state)
    }
}

/** The text bar's controls: the reference's `TextControls`, compact. */
@Composable
private fun TextControlsRow(state: StudioState) {
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
        SelectField(
            "Font family", state.value("font"),
            listOf("Instrument Serif", "Arial", "Georgia", "Helvetica", "Times New Roman"),
            modifier = Modifier.width(127.dp),
        ) { state.setValue("font", it) }
        SelectField(
            "Font style", state.value("fontStyle"),
            listOf("Regular", "Medium", "Bold", "Italic"),
            modifier = Modifier.width(85.dp),
        ) { state.setValue("fontStyle", it) }
        NumberField(state.value("size"), "Font size", suffix = "pt", minWidth = 61.dp) { state.setValue("size", it) }
        listOf("bold", "italic", "underline").forEach { icon ->
            IconButton(icon, icon, small = false, active = state.isOn(icon), iconSize = 16.dp) { state.toggle(icon) }
        }
        Box(
            Modifier.size(26.dp).clip(RoundedCornerShape(13.dp)).background(parseColour(state.value("colour")))
                .border(1.dp, Color(0x33FFFFFF), RoundedCornerShape(13.dp))
                .clickable { state.togglePanel("Colour") },
        )
        listOf("align", "center", "right", "list").forEach { icon ->
            IconButton(icon, icon, small = false, active = state.value("alignment") == icon, iconSize = 16.dp) {
                state.setValue("alignment", icon)
            }
        }
    }
}
