package io.github.artcraftandroid.filmcraft.ui.photocraft

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsBottomHeight
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.chrisbanes.haze.rememberHazeState
import io.github.artcraftandroid.filmcraft.core.PhotoHistogram
import io.github.artcraftandroid.filmcraft.ui.Fc
import io.github.artcraftandroid.filmcraft.ui.FcDialog
import io.github.artcraftandroid.filmcraft.ui.FcIconButton
import io.github.artcraftandroid.filmcraft.ui.FcSlider
import io.github.artcraftandroid.filmcraft.ui.FcType
import io.github.artcraftandroid.filmcraft.ui.Glyph
import io.github.artcraftandroid.filmcraft.ui.GlyphIcon
import io.github.artcraftandroid.filmcraft.ui.LocalEditorBackdrop
import io.github.artcraftandroid.filmcraft.ui.LucideIcons
import io.github.artcraftandroid.filmcraft.ui.PresetTile
import io.github.artcraftandroid.filmcraft.ui.SelectChip
import io.github.artcraftandroid.filmcraft.ui.SliderRow
import io.github.artcraftandroid.filmcraft.ui.editorBackdropSource
import io.github.artcraftandroid.filmcraft.ui.editorCard
import io.github.artcraftandroid.filmcraft.ui.editorGlass
import io.github.artcraftandroid.filmcraft.ui.glyph
import io.github.artcraftandroid.filmcraft.ui.lucide
import kotlin.math.roundToInt

private enum class AdjustTab(val label: String) {
    Light("Light"),
    Color("Color"),
    Detail("Detail"),
}

/** A before/after split, drawn on Lucide's 24-unit grid: no Lucide icon says "compare". */
private val CompareGlyph: Glyph = lucide(
    "PhotoCraftCompare",
    "M12 3v18",
    "M9 6H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h3",
    "M15 6h3a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2h-3",
).glyph()

/**
 * PhotoCraft: the photo editor, laid out like a desktop one. On a tablet the tool rails run down the
 * left, the picture is full-bleed in the middle under a floating bar and zoom control, and the active
 * tool's panel is a card on the right; on a phone the rails and the panel drop into a sheet under the
 * picture. The crop frame and the preview are drawn from the same edited picture the save uses, so
 * nothing is a simulation.
 */
@Composable
fun PhotoCraftScreen(ed: PhotoCraftController, onClose: () -> Unit) {
    var tool by remember { mutableStateOf(PhotoTool.Adjust) }
    var confirmDiscard by remember { mutableStateOf(false) }
    val values = remember { PhotoToolValues() }
    val view = remember { PhotoViewState() }
    val requestClose: () -> Unit = {
        when {
            ed.saving -> Unit
            ed.edits.hasEdits && !ed.saved -> confirmDiscard = true
            else -> onClose()
        }
    }
    BackHandler { requestClose() }
    // The crop frame is only meaningful while its tool is open; leaving applies it to the picture.
    LaunchedEffect(tool) { ed.cropTool(tool == PhotoTool.Crop) }
    LaunchedEffect(tool) { if (tool == PhotoTool.Crop) ed.compareWithOriginal(false) }

    CompositionLocalProvider(LocalEditorBackdrop provides rememberHazeState()) {
        BoxWithConstraints(Modifier.fillMaxSize().background(Fc.Black).testTag("screen_photocraft")) {
            // The tablet layout, the way the desktop editor and the reference do it: the rails on the
            // left, the picture full-bleed in the middle with its own bar and zoom control, the panel on
            // the right; on a phone the panel drops to a sheet under the picture.
            val wide = maxWidth >= 760.dp && maxWidth > maxHeight
            // Read here: inside the sheet's Column the implicit receiver is ColumnScope, not this box.
            val sheetRoom = maxHeight * .60f
            var openGroup by remember { mutableStateOf<ToolGroup?>(tool.group) }
            val choose: (PhotoTool) -> Unit = { tool = it }
            val toggleGroup: (ToolGroup) -> Unit = { openGroup = if (openGroup == it) null else it }
            if (wide) {
                Row(Modifier.fillMaxSize()) {
                    GroupRail(tool.group, openGroup, toggleGroup, true, Modifier.width(64.dp).fillMaxHeight())
                    openGroup?.let { group ->
                        ToolStrip(group, tool, choose, true, Modifier.width(54.dp).fillMaxHeight())
                    }
                    Box(Modifier.weight(1f).fillMaxHeight()) {
                        PhotoPreview(ed, tool, values, view, Modifier.fillMaxSize().padding(top = 64.dp, bottom = 16.dp))
                        PhotoCraftTopBar(
                            ed, requestClose,
                            Modifier.align(Alignment.TopCenter).statusBarsPadding().padding(top = 8.dp, start = 8.dp, end = 8.dp),
                        )
                        ZoomPill(view, Modifier.align(Alignment.BottomStart).padding(start = 16.dp, bottom = 16.dp))
                    }
                    Column(
                        Modifier
                            .width(338.dp)
                            .fillMaxHeight()
                            .padding(start = 4.dp, end = 10.dp, top = 10.dp, bottom = 12.dp)
                            .clip(RoundedCornerShape(22.dp))
                            .editorGlass(22.dp, material = Fc.Panel.copy(alpha = .95f)),
                    ) {
                        PanelHeader(tool, ed.baseWidth, ed.baseHeight)
                        ToolPanelBody(ed, tool, values, view, Modifier.fillMaxWidth().weight(1f))
                        Spacer(Modifier.fillMaxWidth().windowInsetsBottomHeight(WindowInsets.navigationBars))
                    }
                }
            } else {
                Column(Modifier.fillMaxSize()) {
                    Box(Modifier.weight(1f).fillMaxWidth()) {
                        PhotoPreview(ed, tool, values, view, Modifier.fillMaxSize().padding(top = 58.dp))
                        PhotoCraftTopBar(
                            ed, requestClose,
                            Modifier.align(Alignment.TopCenter).statusBarsPadding().padding(top = 6.dp, start = 6.dp, end = 6.dp),
                        )
                        ZoomPill(view, Modifier.align(Alignment.BottomStart).padding(start = 12.dp, bottom = 12.dp))
                    }
                    Column(
                        Modifier
                            .fillMaxWidth()
                            .heightIn(max = sheetRoom)
                            .clip(RoundedCornerShape(topStart = 24.dp, topEnd = 24.dp))
                            .editorGlass(24.dp, material = Fc.Panel.copy(alpha = .96f)),
                    ) {
                        GroupRail(tool.group, openGroup, toggleGroup, false, Modifier.fillMaxWidth())
                        openGroup?.let { group ->
                            ToolStrip(group, tool, choose, false, Modifier.fillMaxWidth())
                        }
                        PanelHeader(tool, ed.baseWidth, ed.baseHeight)
                        ToolPanelBody(ed, tool, values, view, Modifier.fillMaxWidth().weight(1f))
                        Spacer(Modifier.fillMaxWidth().windowInsetsBottomHeight(WindowInsets.navigationBars))
                    }
                }
            }
            if (ed.loading) {
                Box(Modifier.fillMaxSize().background(Fc.Black.copy(alpha = .55f)), contentAlignment = Alignment.Center) {
                    CircularProgressIndicator(color = Fc.Accent)
                }
            }
            if (ed.saving) {
                Box(Modifier.fillMaxSize().background(Fc.Scrim), contentAlignment = Alignment.Center) {
                    Column(
                        Modifier.clip(RoundedCornerShape(20.dp)).editorGlass(20.dp).padding(horizontal = 28.dp, vertical = 22.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                    ) {
                        CircularProgressIndicator(color = Fc.Accent, modifier = Modifier.size(28.dp))
                        Spacer(Modifier.height(12.dp))
                        Text("Saving photo…", style = FcType.Body, color = Fc.Title)
                    }
                }
            }
            ed.error?.let { message ->
                Box(Modifier.fillMaxSize().background(Fc.Scrim), contentAlignment = Alignment.Center) {
                    Column(Modifier.padding(36.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                        Text(message, style = FcType.Body, color = Fc.Title, textAlign = TextAlign.Center)
                        Spacer(Modifier.height(18.dp))
                        SelectChip("Close", false, onClose)
                    }
                }
            }
        }
    }
    if (confirmDiscard) {
        FcDialog(
            "Discard this edit?",
            "Discard",
            { confirmDiscard = false; onClose() },
            { confirmDiscard = false },
            destructive = true,
        ) {
            Text(
                "Your adjustments to this photo will be lost. Nothing is saved until you tap Save.",
                style = FcType.Body,
                color = Fc.Body,
            )
        }
    }
}

// ---- top bar ----

@Composable
private fun PhotoCraftTopBar(ed: PhotoCraftController, close: () -> Unit, modifier: Modifier = Modifier) {
    Row(
        modifier
            .height(50.dp)
            .clip(CircleShape)
            .editorGlass(25.dp, material = Fc.Panel.copy(alpha = .92f))
            .padding(horizontal = 5.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        FcIconButton(LucideIcons.X.glyph(), "Close PhotoCraft", close, size = 38.dp, iconSize = 20.dp)
        Spacer(Modifier.width(4.dp))
        Column(Modifier.padding(end = 12.dp)) {
            Text("PHOTOCRAFT", style = FcType.Badge, color = Fc.AccentLight)
            Text(ed.name, style = FcType.Label, color = Fc.Title, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        FcIconButton(LucideIcons.Undo2.glyph(), "Undo", { ed.undo() }, enabled = ed.canUndo, size = 40.dp, iconSize = 20.dp)
        FcIconButton(LucideIcons.Redo2.glyph(), "Redo", { ed.redo() }, enabled = ed.canRedo, size = 40.dp, iconSize = 20.dp)
        CompareButton(ed)
        Spacer(Modifier.width(8.dp))
        SavePill(ed.saving) { ed.save() }
    }
}

/** Hold to see the picture exactly as it was opened. */
@Composable
private fun CompareButton(ed: PhotoCraftController) {
    Box(
        Modifier
            .size(40.dp)
            .clip(CircleShape)
            .background(if (ed.compare) Fc.Raised else Color.Transparent)
            .pointerInput(Unit) {
                detectTapGestures(onPress = {
                    ed.compareWithOriginal(true)
                    tryAwaitRelease()
                    ed.compareWithOriginal(false)
                })
            }
            .semantics { contentDescription = "Hold to compare with the original" },
        contentAlignment = Alignment.Center,
    ) {
        GlyphIcon(CompareGlyph, if (ed.compare) Fc.AccentLight else Fc.Icon, size = 20.dp)
    }
}

@Composable
private fun SavePill(saving: Boolean, onClick: () -> Unit) {
    Row(
        Modifier
            .clip(RoundedCornerShape(14.dp))
            .background(if (saving) Fc.Accent.copy(alpha = .5f) else Fc.Accent)
            .clickable(enabled = !saving, role = Role.Button, onClick = onClick)
            .testTag("photocraft_save")
            .semantics { contentDescription = "Save photo" }
            .padding(horizontal = 14.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        GlyphIcon(LucideIcons.Download.glyph(), Color.White, size = 17.dp)
        Spacer(Modifier.width(6.dp))
        Text(if (saving) "Saving…" else "Save", style = FcType.Button.copy(fontSize = 14.sp), color = Color.White)
    }
}

// ---- preview ----

@Composable
private fun PhotoPreview(
    ed: PhotoCraftController,
    tool: PhotoTool,
    values: PhotoToolValues,
    view: PhotoViewState,
    modifier: Modifier,
) {
    LaunchedEffect(tool) {
        if (tool == PhotoTool.Crop) view.reset()
    }
    BoxWithConstraints(modifier.editorBackdropSource(), contentAlignment = Alignment.Center) {
        val image = ed.preview
        if (image == null) {
            if (!ed.loading) GlyphIcon(LucideIcons.Image.glyph(), Fc.Disabled, size = 48.dp)
            return@BoxWithConstraints
        }
        val availableWidth = (maxWidth.value - 24f).coerceAtLeast(1f)
        val availableHeight = (maxHeight.value - 24f).coerceAtLeast(1f)
        val (imageWidth, imageHeight) = fitAspect(image.width.toFloat() / image.height.coerceAtLeast(1), availableWidth, availableHeight)
        val sampling = tool.panel == ToolPanel.Sample
        val gestures = if (tool == PhotoTool.Crop || sampling) Modifier else Modifier
            .pointerInput(Unit) {
                detectTransformGestures { _, movement, change, _ ->
                    view.zoom = (view.zoom * change).coerceIn(1f, 6f)
                    view.pan = if (view.zoom <= 1.001f) Offset.Zero else view.pan + movement
                }
            }
            .pointerInput(Unit) {
                detectTapGestures(onDoubleTap = { view.reset() })
            }
        Box(Modifier.size(imageWidth.dp, imageHeight.dp)) {
            Box(
                Modifier
                    .fillMaxSize()
                    .clip(RoundedCornerShape(6.dp))
                    .graphicsLayer {
                        scaleX = view.zoom
                        scaleY = view.zoom
                        translationX = view.pan.x
                        translationY = view.pan.y
                    }
                    .then(gestures),
            ) {
                Image(
                    bitmap = image,
                    contentDescription = "Photo being edited",
                    modifier = Modifier.fillMaxSize(),
                    contentScale = ContentScale.Fit,
                )
            }
            // The frame sits outside the clipped, zoomed layer so its corner handles stay whole.
            if (tool == PhotoTool.Crop) CropOverlay(ed, Modifier.fillMaxSize())
            if (sampling) {
                val grid = sampleGrid(tool, values)
                Box(
                    Modifier
                        .fillMaxSize()
                        .pointerInput(tool, grid) {
                            detectTapGestures { offset ->
                                val picked = sampleColor(image, offset, size, grid)
                                if (picked != null) view.addSample(picked)
                            }
                        }
                        .semantics { contentDescription = "Tap the picture to sample its colour" },
                )
            }
        }
    }
}

/** How many pixels the eyedropper averages: the sampler's own option, 1 when it has none. */
private fun sampleGrid(tool: PhotoTool, values: PhotoToolValues): Int {
    val option = tool.options.filterIsInstance<ToolOption.Choice>().firstOrNull { it.key == "sample" } ?: return 1
    return when (option.choices.getOrNull(values.choice(tool, option))) {
        "3 × 3" -> 3
        "5 × 5" -> 5
        else -> 1
    }
}

/** Reads the colour of the picture as it is drawn, averaged over a [grid]-wide square of pixels. */
private fun sampleColor(image: ImageBitmap, offset: Offset, box: IntSize, grid: Int): Color? = runCatching {
    val bitmap = image.asAndroidBitmap()
    val x = (offset.x / box.width.coerceAtLeast(1) * bitmap.width).roundToInt().coerceIn(0, bitmap.width - 1)
    val y = (offset.y / box.height.coerceAtLeast(1) * bitmap.height).roundToInt().coerceIn(0, bitmap.height - 1)
    val half = grid / 2
    var red = 0L
    var green = 0L
    var blue = 0L
    var count = 0
    for (dy in -half..half) {
        for (dx in -half..half) {
            val px = (x + dx).coerceIn(0, bitmap.width - 1)
            val py = (y + dy).coerceIn(0, bitmap.height - 1)
            val pixel = bitmap.getPixel(px, py)
            red += (pixel shr 16) and 0xFF
            green += (pixel shr 8) and 0xFF
            blue += pixel and 0xFF
            count++
        }
    }
    if (count == 0) null else Color(
        (red.toFloat() / count) / 255f,
        (green.toFloat() / count) / 255f,
        (blue.toFloat() / count) / 255f,
    )
}.getOrNull()

/** The crop frame: the dimmed outside, the thirds, four handles and a draggable inside. */
@Composable
private fun CropOverlay(ed: PhotoCraftController, modifier: Modifier = Modifier) {
    val frame = ed.edits.crop ?: NormRect.Full
    val current by rememberUpdatedState(frame)
    val density = LocalDensity.current
    var box by remember { mutableStateOf(Size.Zero) }
    Box(modifier.onSizeChanged { box = Size(it.width.toFloat(), it.height.toFloat()) }) {
        Canvas(Modifier.fillMaxSize()) {
            if (box.width <= 0f || box.height <= 0f) return@Canvas
            val left = current.left * size.width
            val top = current.top * size.height
            val right = current.right * size.width
            val bottom = current.bottom * size.height
            val dim = Color.Black.copy(alpha = .55f)
            drawRect(dim, Offset.Zero, Size(size.width, top))
            drawRect(dim, Offset(0f, bottom), Size(size.width, size.height - bottom))
            drawRect(dim, Offset(0f, top), Size(left, bottom - top))
            drawRect(dim, Offset(right, top), Size(size.width - right, bottom - top))
            drawRect(Color.White, Offset(left, top), Size(right - left, bottom - top), style = Stroke(1.5.dp.toPx()))
            val hairline = Color.White.copy(alpha = .32f)
            for (third in 1..2) {
                val x = left + (right - left) * third / 3f
                val y = top + (bottom - top) * third / 3f
                drawLine(hairline, Offset(x, top), Offset(x, bottom), 1.dp.toPx())
                drawLine(hairline, Offset(left, y), Offset(right, y), 1.dp.toPx())
            }
        }
        val frameLeft = with(density) { (current.left * box.width).toDp() }
        val frameTop = with(density) { (current.top * box.height).toDp() }
        val frameWidth = with(density) { (current.width * box.width).toDp() }
        val frameHeight = with(density) { (current.height * box.height).toDp() }
        // the inside drags the whole frame
        Box(
            Modifier
                .align(Alignment.TopStart)
                .offset(frameLeft, frameTop)
                .size(frameWidth.coerceAtLeast(2.dp), frameHeight.coerceAtLeast(2.dp))
                .pointerInput(box) {
                    detectDragGestures(
                        onDragEnd = { ed.finishGesture() },
                        onDragCancel = { ed.finishGesture() },
                    ) { change, drag ->
                        change.consume()
                        if (box.width > 0f && box.height > 0f) ed.setCrop(moveCrop(current, drag.x / box.width, drag.y / box.height))
                    }
                },
        )
        CropHandle(ed, CropCorner.TopLeft, current, box, Modifier.align(Alignment.TopStart))
        CropHandle(ed, CropCorner.TopRight, current, box, Modifier.align(Alignment.TopStart))
        CropHandle(ed, CropCorner.BottomLeft, current, box, Modifier.align(Alignment.TopStart))
        CropHandle(ed, CropCorner.BottomRight, current, box, Modifier.align(Alignment.TopStart))
    }
}

@Composable
private fun CropHandle(ed: PhotoCraftController, corner: CropCorner, frame: NormRect, box: Size, modifier: Modifier = Modifier) {
    val current by rememberUpdatedState(frame)
    val density = LocalDensity.current
    val x = when (corner) {
        CropCorner.TopLeft, CropCorner.BottomLeft -> frame.left
        CropCorner.TopRight, CropCorner.BottomRight -> frame.right
    }
    val y = when (corner) {
        CropCorner.TopLeft, CropCorner.TopRight -> frame.top
        CropCorner.BottomLeft, CropCorner.BottomRight -> frame.bottom
    }
    val xOffset = with(density) { (x * box.width).toDp() } - 22.dp
    val yOffset = with(density) { (y * box.height).toDp() } - 22.dp
    Box(
        modifier
            .offset(xOffset, yOffset)
            .size(44.dp)
            .pointerInput(box, ed.aspect) {
                detectDragGestures(
                    onDragEnd = { ed.finishGesture() },
                    onDragCancel = { ed.finishGesture() },
                ) { change, drag ->
                    change.consume()
                    if (box.width > 0f && box.height > 0f) {
                        ed.setCrop(resizeCrop(current, corner, drag.x / box.width, drag.y / box.height, ed.imageAspect, ed.aspect))
                    }
                }
            },
        contentAlignment = Alignment.Center,
    ) {
        Box(Modifier.size(20.dp).clip(CircleShape).background(Color.White).border(2.dp, Color.Black.copy(alpha = .35f), CircleShape))
    }
}

// ---- tools ----

/** Which panel the card shows: the tool's own, or the tone, filter and crop panels. */
@Composable
private fun ToolPanelBody(
    ed: PhotoCraftController,
    tool: PhotoTool,
    values: PhotoToolValues,
    view: PhotoViewState,
    modifier: Modifier,
) {
    Box(modifier) {
        when (tool.panel) {
            ToolPanel.Adjust -> AdjustPanel(ed)
            ToolPanel.Filters -> FiltersPanel(ed)
            ToolPanel.Crop -> CropPanel(ed)
            ToolPanel.Sample -> SamplePanel(tool, values, view)
            ToolPanel.View -> ViewPanel(view)
            ToolPanel.Options -> ToolOptionsPanel(tool, values) { action -> runToolAction(ed, action) }
        }
    }
}

@Composable
private fun AdjustPanel(ed: PhotoCraftController) {
    var tab by remember { mutableStateOf(AdjustTab.Light) }
    Column(Modifier.fillMaxSize()) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                for (option in AdjustTab.values()) SelectChip(option.label, tab == option, { tab = option })
            }
            AutoChip({ ed.autoAdjust() })
        }
        HistogramStrip(ed.histogram)
        Column(Modifier.fillMaxWidth().weight(1f).verticalScroll(rememberScrollState())) {
            val adjustments = ed.edits.adjustments
            when (tab) {
                AdjustTab.Light -> {
                    SliderLine(ed, "Exposure", adjustments.exposure, -1f..1f, exposureText(adjustments.exposure)) { a, v -> a.copy(exposure = v) }
                    SliderLine(ed, "Contrast", adjustments.contrast, -1f..1f, signedPercent(adjustments.contrast)) { a, v -> a.copy(contrast = v) }
                    SliderLine(ed, "Highlights", adjustments.highlights, -1f..1f, signedPercent(adjustments.highlights)) { a, v -> a.copy(highlights = v) }
                    SliderLine(ed, "Shadows", adjustments.shadows, -1f..1f, signedPercent(adjustments.shadows)) { a, v -> a.copy(shadows = v) }
                    SliderLine(ed, "Whites", adjustments.whites, -1f..1f, signedPercent(adjustments.whites)) { a, v -> a.copy(whites = v) }
                    SliderLine(ed, "Blacks", adjustments.blacks, -1f..1f, signedPercent(adjustments.blacks)) { a, v -> a.copy(blacks = v) }
                }
                AdjustTab.Color -> {
                    SliderLine(ed, "Temperature", adjustments.temperature, -1f..1f, signedPercent(adjustments.temperature)) { a, v -> a.copy(temperature = v) }
                    SliderLine(ed, "Tint", adjustments.tint, -1f..1f, signedPercent(adjustments.tint)) { a, v -> a.copy(tint = v) }
                    SliderLine(ed, "Vibrance", adjustments.vibrance, -1f..1f, signedPercent(adjustments.vibrance)) { a, v -> a.copy(vibrance = v) }
                    SliderLine(ed, "Saturation", adjustments.saturation, -1f..1f, signedPercent(adjustments.saturation)) { a, v -> a.copy(saturation = v) }
                }
                AdjustTab.Detail -> {
                    SliderLine(ed, "Clarity", adjustments.clarity, -1f..1f, signedPercent(adjustments.clarity)) { a, v -> a.copy(clarity = v) }
                    SliderLine(ed, "Sharpen", adjustments.sharpen, 0f..1f, signedPercent(adjustments.sharpen)) { a, v -> a.copy(sharpen = v) }
                    SliderLine(ed, "Vignette", adjustments.vignette, -1f..1f, signedPercent(adjustments.vignette)) { a, v -> a.copy(vignette = v) }
                    SliderLine(ed, "Grain", adjustments.grain, 0f..1f, signedPercent(adjustments.grain)) { a, v -> a.copy(grain = v) }
                    SliderLine(ed, "Fade", adjustments.fade, 0f..1f, signedPercent(adjustments.fade)) { a, v -> a.copy(fade = v) }
                }
            }
            if (adjustments.dirty) {
                Row(Modifier.fillMaxWidth().padding(horizontal = 14.dp, vertical = 8.dp), horizontalArrangement = Arrangement.End) {
                    SelectChip("Reset adjustments", false, { ed.resetAdjustments() })
                }
            }
        }
    }
}

@Composable
private fun SliderLine(
    ed: PhotoCraftController,
    label: String,
    value: Float,
    range: ClosedFloatingPointRange<Float>,
    shown: String,
    change: (PhotoAdjustments, Float) -> PhotoAdjustments,
) {
    val span = range.endInclusive - range.start
    val fraction = if (span > 0f) ((value - range.start) / span).coerceIn(0f, 1f) else 0f
    SliderRow(label, shown) {
        FcSlider(
            fraction = fraction,
            onChange = { position -> ed.preview { it.copy(adjustments = change(it.adjustments, range.start + position * span)) } },
            onDone = { ed.finishGesture() },
            description = label,
            numericRange = range,
        )
    }
}

@Composable
private fun AutoChip(onClick: () -> Unit) {
    Row(
        Modifier
            .height(36.dp)
            .clip(CircleShape)
            .background(Fc.Raised)
            .clickable(role = Role.Button, onClick = onClick)
            .semantics { contentDescription = "Auto adjust" }
            .padding(horizontal = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        GlyphIcon(LucideIcons.WandSparkles.glyph(), Fc.AccentLight, size = 16.dp)
        Spacer(Modifier.width(6.dp))
        Text("Auto", style = FcType.Caption, color = Fc.Body)
    }
}

@Composable
private fun HistogramStrip(histogram: PhotoHistogram?) {
    Box(Modifier.fillMaxWidth().height(34.dp).padding(horizontal = 16.dp, vertical = 3.dp)) {
        if (histogram == null) return@Box
        Canvas(Modifier.fillMaxSize()) {
            drawChannel(histogram.red, Color(0xFFFF5A5A))
            drawChannel(histogram.green, Color(0xFF54E38A))
            drawChannel(histogram.blue, Color(0xFF5AA8FF))
        }
    }
}

private fun DrawScope.drawChannel(buckets: List<UInt>, color: Color) {
    if (buckets.size < 2) return
    var peak = 1f
    for (bucket in buckets) peak = maxOf(peak, bucket.toFloat())
    val path = Path()
    buckets.forEachIndexed { index, bucket ->
        val x = index.toFloat() / (buckets.size - 1) * size.width
        val y = size.height * (1f - bucket.toFloat() / peak)
        if (index == 0) path.moveTo(x, y) else path.lineTo(x, y)
    }
    path.lineTo(size.width, size.height)
    path.lineTo(0f, size.height)
    path.close()
    drawPath(path, color.copy(alpha = .35f))
}

@Composable
private fun FiltersPanel(ed: PhotoCraftController) {
    Column(Modifier.fillMaxSize()) {
        LazyRow(
            Modifier.fillMaxWidth().height(122.dp),
            contentPadding = PaddingValues(horizontal = 12.dp),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            items(PhotoFilter.values().toList()) { filter ->
                PresetTile(filter.label, ed.edits.adjustments.filter == filter, {
                    ed.commit { it.copy(adjustments = it.adjustments.copy(filter = filter)) }
                }) {
                    val thumb = ed.thumbs[filter.id]
                    if (thumb != null) {
                        Image(thumb, contentDescription = null, modifier = Modifier.fillMaxSize(), contentScale = ContentScale.Crop)
                    } else {
                        Box(Modifier.fillMaxSize().background(Fc.Raised))
                    }
                }
            }
        }
        if (ed.edits.adjustments.filter != PhotoFilter.Original) {
            SliderLine(ed, "Strength", ed.edits.adjustments.filterAmount, 0f..1f, "${(ed.edits.adjustments.filterAmount * 100).roundToInt()}%") { a, v -> a.copy(filterAmount = v) }
        }
    }
}

@Composable
private fun CropPanel(ed: PhotoCraftController) {
    Column(Modifier.fillMaxSize().padding(horizontal = 14.dp)) {
        Row(Modifier.fillMaxWidth().padding(top = 4.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CropAction(LucideIcons.RotateCcw.glyph(), "Left", "Rotate left", { ed.rotate(false) })
            CropAction(LucideIcons.RotateCcw.glyph(), "Right", "Rotate right", { ed.rotate(true) }, rotation = 180f)
            CropAction(LucideIcons.FlipHorizontal2.glyph(), "Flip H", "Flip horizontal", { ed.flip(true) })
            CropAction(LucideIcons.FlipVertical2.glyph(), "Flip V", "Flip vertical", { ed.flip(false) })
        }
        Spacer(Modifier.height(10.dp))
        Text("FRAME", style = FcType.Caption, color = Fc.Detail)
        Row(
            Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(vertical = 6.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            SelectChip("Free", ed.aspect == null, { ed.lockAspect(null) })
            for ((ratio, label) in CropRatios) SelectChip(label, ed.aspect == ratio, { ed.lockAspect(ratio) })
        }
        Spacer(Modifier.weight(1f))
        Row(Modifier.fillMaxWidth().padding(bottom = 12.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(
                "Drag the corners to frame the picture. Pick another tool to apply it.",
                style = FcType.Caption,
                color = Fc.Detail,
                modifier = Modifier.weight(1f),
            )
            SelectChip("Reset", false, { ed.resetCrop() })
        }
    }
}

private val CropRatios = listOf(
    1f to "1:1",
    4f / 5f to "4:5",
    5f / 4f to "5:4",
    3f / 4f to "3:4",
    4f / 3f to "4:3",
    2f / 3f to "2:3",
    3f / 2f to "3:2",
    9f / 16f to "9:16",
    16f / 9f to "16:9",
)

@Composable
private fun RowScope.CropAction(glyph: Glyph, label: String, description: String, onClick: () -> Unit, rotation: Float = 0f) {
    Column(
        Modifier
            .weight(1f)
            .height(64.dp)
            .clip(RoundedCornerShape(14.dp))
            .editorCard(14.dp)
            .clickable(role = Role.Button, onClick = onClick)
            .semantics { contentDescription = description },
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        Box(Modifier.graphicsLayer { rotationZ = rotation }) { GlyphIcon(glyph, Fc.Icon, size = 20.dp) }
        Spacer(Modifier.height(4.dp))
        Text(label, style = FcType.ToolLabel, color = Fc.Body, maxLines = 1)
    }
}
