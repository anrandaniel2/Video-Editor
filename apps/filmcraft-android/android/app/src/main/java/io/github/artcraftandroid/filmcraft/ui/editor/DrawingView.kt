package io.github.artcraftandroid.filmcraft.ui.editor

import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.runtime.mutableStateOf
import io.github.artcraftandroid.filmcraft.core.DrawingStrokePoint
import io.github.artcraftandroid.filmcraft.core.drawingStrokeCoverage
import io.github.artcraftandroid.filmcraft.core.drawingGeometricSelection
import io.github.artcraftandroid.filmcraft.core.drawingAverageColor
import io.github.artcraftandroid.filmcraft.core.drawingFillRegion
import io.github.artcraftandroid.filmcraft.core.DrawingAnimation
import io.github.artcraftandroid.filmcraft.core.DrawingTiledDocument
import io.github.artcraftandroid.filmcraft.core.drawingImportTiled
import io.github.artcraftandroid.filmcraft.core.drawingEncodePrecision
import io.github.artcraftandroid.filmcraft.core.drawingDecodePrecision
import io.github.artcraftandroid.filmcraft.core.DrawingAnimationInfo
import io.github.artcraftandroid.filmcraft.core.drawingDecodeAnimation
import io.github.artcraftandroid.filmcraft.core.drawingCompositeDocument
import io.github.artcraftandroid.filmcraft.core.DrawingNativeBrush
import io.github.artcraftandroid.filmcraft.core.drawingNativeBrushCoverage
import io.github.artcraftandroid.filmcraft.core.drawingNativeBrushPaint
import io.github.artcraftandroid.filmcraft.core.DrawingGradientPreset
import io.github.artcraftandroid.filmcraft.core.DrawingAffineSettings
import io.github.artcraftandroid.filmcraft.core.drawingAffineMatrix
import io.github.artcraftandroid.filmcraft.core.drawingPolygonSelection
import io.github.artcraftandroid.filmcraft.core.drawingMultistopGradient
import io.github.artcraftandroid.filmcraft.core.drawingPatternFill
import io.github.artcraftandroid.filmcraft.core.DrawingAssistantSettings
import io.github.artcraftandroid.filmcraft.core.drawingAssistantSample
import io.github.artcraftandroid.filmcraft.core.drawingRetouchStroke
import io.github.artcraftandroid.filmcraft.core.drawingPaintLayerMask
import kotlinx.coroutines.launch
import kotlinx.coroutines.cancel
import io.github.artcraftandroid.filmcraft.core.drawingMaskPixels
import io.github.artcraftandroid.filmcraft.core.drawingProcessMask
import io.github.artcraftandroid.filmcraft.core.drawingFlattenDocument
import io.github.artcraftandroid.filmcraft.core.drawingPreserveAlpha
import io.github.artcraftandroid.filmcraft.core.drawingAdvancedFilter
import io.github.artcraftandroid.filmcraft.core.drawingTransformDocument
import io.github.artcraftandroid.filmcraft.core.drawingDocumentBounds
import io.github.artcraftandroid.filmcraft.core.DrawingAdjustments
import io.github.artcraftandroid.filmcraft.core.DrawingDocumentSnapshot
import io.github.artcraftandroid.filmcraft.core.DrawingRasterLayer
import io.github.artcraftandroid.filmcraft.core.drawingEncodeDocument
import io.github.artcraftandroid.filmcraft.core.drawingDecodeDocument
import io.github.artcraftandroid.filmcraft.core.drawingAdjustRgba
import io.github.artcraftandroid.filmcraft.core.drawingApplyCurve
import io.github.artcraftandroid.filmcraft.core.drawingMergeLayers
import io.github.artcraftandroid.filmcraft.core.drawingColorSelection
import io.github.artcraftandroid.filmcraft.core.drawingSelectionMask
import io.github.artcraftandroid.filmcraft.core.drawingGradientRgba
import io.github.artcraftandroid.filmcraft.core.drawingFilterRgba
import io.github.artcraftandroid.filmcraft.core.drawingFloodFill
import io.github.artcraftandroid.filmcraft.core.drawingBlendColor
import io.github.artcraftandroid.filmcraft.core.drawingSymmetryMatrices
import io.github.artcraftandroid.filmcraft.core.DrawingInput
import io.github.artcraftandroid.filmcraft.core.drawingStrokeSamples
import android.content.Context
import android.graphics.*
import android.view.MotionEvent
import android.view.View
import kotlin.math.*

internal val drawingLayerBlends = listOf(0 to "Normal", 2 to "Darken", 3 to "Multiply", 4 to "Colour burn", 7 to "Lighten", 8 to "Screen", 9 to "Colour dodge", 12 to "Overlay", 13 to "Soft light", 14 to "Hard light", 19 to "Difference", 20 to "Exclusion", 23 to "Hue", 24 to "Saturation", 25 to "Colour", 26 to "Luminosity")

/** Editable raster layers, selection masks and transactional layer/selection transforms. */
internal class DrawingView(context: Context, val backgroundFrame: Bitmap?, canvasW: Int, canvasH: Int) : View(context) {
    var canvasW = canvasW
        private set
    var canvasH = canvasH
        private set
    var animationSession: DrawingAnimation? by mutableStateOf(null)
        private set
    var animationPlaying by mutableStateOf(false)
        private set
    var onionEnabled = false
    var onionOpacity = .25f
    private var previousOnion: Bitmap? = null
    private var nextOnion: Bitmap? = null
    private var animationPreview: Bitmap? = null
    private var animationJob: kotlinx.coroutines.Job? = null
    private fun bitmapFromRgba(bytes: ByteArray, w: Int = canvasW, h: Int = canvasH): Bitmap {
        require(bytes.size == w * h * 4)
        val pixels = IntArray(w * h) { i -> Color.argb(bytes[i*4+3].toInt() and 255, bytes[i*4].toInt() and 255, bytes[i*4+1].toInt() and 255, bytes[i*4+2].toInt() and 255) }
        return Bitmap.createBitmap(pixels, w, h, Bitmap.Config.ARGB_8888)
    }
    fun stopAnimation() { animationJob?.cancel(); animationJob = null; animationPlaying = false; animationPreview = null; changed() }
    private suspend fun synchronizeAnimation(): DrawingAnimation {
        val document = documentSnapshot()
        val current = animationSession
        return kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) {
            if (current == null) DrawingAnimation(document) else { current.updateCurrent(document); current }
        }.also { animationSession = it }
    }
    suspend fun animationAction(action: String, index: Int = 0, name: String = "Frame", exposure: UInt = 1u, fps: UInt = 12u) {
        if (operationBusy || stroke != null || maskPoints.isNotEmpty() || transformMode != null) return
        stopAnimation(); operationBusy = true
        try {
            val session = if (action == "reset") {
                val document = documentSnapshot()
                kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) { DrawingAnimation(document) }.also { animationSession = it }
            } else synchronizeAnimation()
            val document = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) {
                when (action) {
                    "select" -> session.select(index.toUInt()); "blank" -> session.insert(false); "duplicate" -> session.insert(true)
                    "delete" -> session.deleteCurrent(); "left" -> session.moveCurrent(-1); "right" -> session.moveCurrent(1)
                    "configure" -> session.configure(name, exposure, fps); "undo" -> session.undo(); "redo" -> session.redo()
                }
                session.current()
            }
            if (action !in listOf("start", "configure", "sync", "reset")) installDocument(document, false)
            refreshOnionSkins()
        } finally { operationBusy = false; changed() }
    }
    suspend fun encodeAnimation(): ByteArray {
        check(!operationBusy && transformMode == null && maskPoints.isEmpty() && stroke == null) { "Finish the current operation first." }
        stopAnimation(); operationBusy = true
        try { val session = synchronizeAnimation(); return kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) { session.encode() } }
        finally { operationBusy = false; changed() }
    }
    suspend fun exportAnimationPng(looped: Boolean): ByteArray {
        check(!operationBusy && transformMode == null && maskPoints.isEmpty() && stroke == null) { "Finish the current operation first." }
        stopAnimation(); operationBusy = true
        try {
            val session = synchronizeAnimation()
            return kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) { session.encodeApng(looped) }
        } finally { operationBusy = false; changed() }
    }
    suspend fun openAnimation(bytes: ByteArray) {
        check(!operationBusy && transformMode == null && maskPoints.isEmpty() && stroke == null) { "Finish the current operation first." }
        stopAnimation(); operationBusy = true
        try {
            val session = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) { drawingDecodeAnimation(bytes) }
            val document = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) { session.current() }
            installDocument(document, false); animationSession = session; refreshOnionSkins()
        } finally { operationBusy = false; changed() }
    }
    suspend fun refreshOnionSkins() {
        previousOnion = null; nextOnion = null
        val session = animationSession ?: return
        if (!onionEnabled) { changed(); return }
        val info = session.info(); val index = info.active.toInt()
        val images = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) {
            val previous = if (index > 0) drawingCompositeDocument(session.frame((index - 1).toUInt())) else null
            val next = if (index + 1 < info.frames.size) drawingCompositeDocument(session.frame((index + 1).toUInt())) else null
            previous to next
        }
        previousOnion = images.first?.let { bitmapFromRgba(it) }; nextOnion = images.second?.let { bitmapFromRgba(it) }; changed()
    }
    fun playAnimation(looped: Boolean) {
        if (animationPlaying || operationBusy || transformMode != null || stroke != null || maskPoints.isNotEmpty()) return
        animationJob = maskScope.launch {
            operationBusy = true
            try {
                val session = synchronizeAnimation(); val info = session.info(); animationPlaying = true
                val start = android.os.SystemClock.uptimeMillis(); var previous = -1
                while (animationPlaying) {
                    val tick = ((android.os.SystemClock.uptimeMillis() - start) * info.fps.toLong() / 1000).coerceAtMost(UInt.MAX_VALUE.toLong()).toUInt()
                    if (!looped && tick >= info.totalTicks) break
                    val index = session.frameAtTick(tick, looped).toInt()
                    if (index != previous) {
                        val pixels = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) { drawingCompositeDocument(session.frame(index.toUInt())) }
                        animationPreview = bitmapFromRgba(pixels); previous = index; changed()
                    }
                    kotlinx.coroutines.delay(8)
                }
            } catch (e: kotlinx.coroutines.CancellationException) { throw e }
            catch (e: Exception) { android.widget.Toast.makeText(context, e.message ?: "Animation playback failed", android.widget.Toast.LENGTH_LONG).show() }
            finally { animationPlaying = false; animationPreview = null; operationBusy = false; changed() }
        }
    }
    var onEdit: () -> Unit = {}
    var onColorPicked: (Int) -> Unit = {}
    var pickerRadius = 0
    var cloneAligned = true
    var cloneSampleMerged = false
    var retouchTonalRange = 0u
    var cloneSource: PointF? = null
        private set
    private var cloneOffset: PointF? = null
    fun clearCloneSource() { cloneSource = null; cloneOffset = null; changed() }
    private var pickerImage: Bitmap? = null
    private var selectionStart: PointF? = null
    private var selectionDraft: RectF? = null
    private val polygonVertices = mutableListOf<PointF>()
    private var polygonContactAdded = false
    val polygonVertexCount get() = polygonVertices.size
    fun removePolygonVertex() { if (polygonVertices.isNotEmpty()) polygonVertices.removeAt(polygonVertices.lastIndex); changed() }
    fun cancelPolygonSelection() { polygonVertices.clear(); changed() }
    fun commitPolygonSelection() {
        if (operationBusy || transformMode != null || polygonVertices.size < 3) return
        try {
            val mask = drawingPolygonSelection(canvasW.toUInt(), canvasH.toUInt(), polygonVertices.flatMap { listOf(it.x, it.y) })
            mergeSelection(maskPath(mask), selectionMode); polygonVertices.clear(); changed()
        } catch (e: Exception) { android.widget.Toast.makeText(context, e.message ?: "Polygon failed", android.widget.Toast.LENGTH_LONG).show() }
    }
    private val maskScope = kotlinx.coroutines.CoroutineScope(kotlinx.coroutines.SupervisorJob() + kotlinx.coroutines.Dispatchers.Main.immediate)
    private val maskPoints = mutableListOf<DrawingStrokePoint>()
    private var maskGuide: Path? = null
    private var maskStrokeOverflow = false
    val backdropObservers = mutableSetOf<View>()
    private val images = mutableListOf(blank())
    private data class LayerProperties(var name: String = "Layer", var visible: Boolean = true, var opacity: Float = 1f, var blendMode: Int = 0, var clipped: Boolean = false, var locked: Boolean = false, var alphaLocked: Boolean = false, var mask: ByteArray = ByteArray(0), var maskEnabled: Boolean = true)
    private val layerProperties = java.util.WeakHashMap<Bitmap, LayerProperties>()
    private data class NativeBacking(val document: DrawingTiledDocument, val generation: Int)
    private val nativeBackings = java.util.WeakHashMap<Bitmap, NativeBacking>()
    private fun nativeBacking(bitmap: Bitmap): DrawingTiledDocument? {
        val backing = nativeBackings[bitmap] ?: return null
        if (bitmap.isRecycled || backing.generation != bitmap.generationId) { nativeBackings.remove(bitmap); return null }
        return backing.document
    }
    private fun attachNative(bitmap: Bitmap, document: DrawingTiledDocument) {
        nativeBackings[bitmap] = NativeBacking(document, bitmap.generationId)
    }
    val nativePaintLayerCount get() = images.count { nativeBacking(it) != null }
    private fun historyBytes(bitmap: Bitmap): Long = bitmap.allocationByteCount.toLong() + properties(bitmap).mask.size + (nativeBacking(bitmap)?.info()?.liveBytes?.toLong() ?: 0L)

    private fun properties(image: Bitmap) = layerProperties.getOrPut(image) { LayerProperties("Layer ${images.indexOf(image) + 1}") }
    private data class Edit(val layer: Int, val before: Bitmap?, val stack: List<Bitmap>? = null, val width: Int = 0, val height: Int = 0, val selectionChange: Boolean = false, val selectionBefore: Path? = null)
    private val history = ArrayDeque<Edit>()
    private val future = ArrayDeque<Edit>()
    val canUndo get() = !operationBusy && (history.isNotEmpty() || shapes.pending) && transformMode == null
    val canRedo get() = !operationBusy && future.isNotEmpty() && transformMode == null && !shapes.pending
    var inkColor = Color.BLACK
    var brushSize = 12f
    var nativeBrushEnabled = true
    var nativeBrushSettings = DrawingNativeBrush(0u, .2f, .9f, 1f, 0f, 0f, 0f, 0f, .5f, 1u, false, 0u)
    private var nativeStrokeSettings: DrawingNativeBrush? = null
    private var nativeStrokeColor = Color.BLACK
    private var nativeStrokeOpacity = 1f
    private var nativeStrokeErase = false
    private var nativePreview: CoverageTile? = null
    private var nativePreviewFailed = false
    private var nativePreviewJob: kotlinx.coroutines.Job? = null
    private var nativePreviewPointCount = -1
    private fun clearNativePreview() {
        nativePreview?.bitmap?.recycle(); nativePreview = null; nativePreviewPointCount = -1
    }
    var usePressure = true
    var assistantMode = 0u
    var assistantOriginX = .5f
    var assistantOriginY = .5f
    var assistantAngle = 0f
    var assistantSpacing = 64f
    var assistantStrength = 1f
    var showAssistantGuides = true
    private var assistantAnchor: DrawingInput? = null
    private fun assisted(sample: DrawingInput): DrawingInput = drawingAssistantSample(sample, assistantAnchor ?: sample,
        DrawingAssistantSettings(assistantMode, canvasW * assistantOriginX, canvasH * assistantOriginY, assistantAngle, assistantSpacing, assistantStrength))
    var stabilization = 0f
    var pressureCurve = 1f
    var symmetryMode = 0u
    var operationBusy by mutableStateOf(false)
        private set
    private fun rgba(image: Bitmap): ByteArray {
        val pixels = IntArray(canvasW * canvasH); image.getPixels(pixels, 0, canvasW, 0, 0, canvasW, canvasH)
        return ByteArray(pixels.size * 4).also { bytes -> pixels.forEachIndexed { i, c ->
            bytes[i*4] = Color.red(c).toByte(); bytes[i*4+1] = Color.green(c).toByte(); bytes[i*4+2] = Color.blue(c).toByte(); bytes[i*4+3] = Color.alpha(c).toByte()
        } }
    }
    private fun replacePixels(bytes: ByteArray, clipToSelection: Boolean = true) {
        check(!layerLocked(layer)) { "Unlock the active layer before editing pixels." }
        require(bytes.size == canvasW * canvasH * 4)
        val pixels = IntArray(canvasW * canvasH) { i -> Color.argb(bytes[i*4+3].toInt() and 255, bytes[i*4].toInt() and 255, bytes[i*4+1].toInt() and 255, bytes[i*4+2].toInt() and 255) }
        val before = copy(images[layer]); val result = blank()
        try {
            result.setPixels(pixels, 0, canvasW, 0, 0, canvasW, canvasH)
            val c = Canvas(images[layer]); c.save(); if (clipToSelection) selection?.let { c.clipPath(it) }
            c.drawBitmap(result, 0f, 0f, Paint().apply { xfermode = PorterDuffXfermode(PorterDuff.Mode.SRC) }); c.restore()
            record(before)
        } catch (e: Exception) { before.recycle(); throw e }
        finally { result.recycle() }
        changed()
    }
    suspend fun encodeDocument(): ByteArray {
        check(!operationBusy && stroke == null && maskPoints.isEmpty() && transformMode == null) { "Finish the active operation before saving." }
        operationBusy = true
        try {
            val document = DrawingDocumentSnapshot(canvasW.toUInt(), canvasH.toUInt(), layer.toUInt(), images.map { image ->
                val p = properties(image); DrawingRasterLayer(p.name, p.visible, p.opacity, rgba(image), p.blendMode.toUInt(), p.clipped, p.locked, p.alphaLocked, p.mask, p.maskEnabled)
            })
            val native = images.map { nativeBacking(it) }
            return kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) { drawingEncodePrecision(document, native) }
        } finally { operationBusy = false }
    }
    suspend fun encodeOpenRaster(): ByteArray {
        check(!operationBusy && stroke == null && maskPoints.isEmpty() && transformMode == null) { "Finish the active operation before exporting." }
        operationBusy = true
        try {
            val document = documentSnapshot()
            return kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) {
                io.github.artcraftandroid.filmcraft.core.drawingEncodeOpenraster(document)
            }
        } finally { operationBusy = false; changed() }
    }
    suspend fun openOpenRaster(bytes: ByteArray) {
        check(!operationBusy && stroke == null && maskPoints.isEmpty() && transformMode == null) { "Finish the active operation before importing." }
        operationBusy = true
        try {
            val document = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) {
                io.github.artcraftandroid.filmcraft.core.drawingDecodeOpenraster(bytes)
            }
            installDocument(document, false)
            animationSession = null; previousOnion = null; nextOnion = null
        } finally { operationBusy = false; changed() }
    }
    private fun documentSnapshot() = DrawingDocumentSnapshot(canvasW.toUInt(), canvasH.toUInt(), layer.toUInt(), images.map { image ->
        val p = properties(image); DrawingRasterLayer(p.name, p.visible, p.opacity, rgba(image), p.blendMode.toUInt(), p.clipped, p.locked, p.alphaLocked, p.mask, p.maskEnabled)
    })
    private fun clearDimensionCaches() {
        selection = null; shapes.clear(); previousOnion = null; nextOnion = null; clearNativePreview(); nativeStrokeSettings = null; polygonVertices.clear(); assistantAnchor = null; cloneSource = null; cloneOffset = null; maskTiles.evictAll(); liveLayer?.recycle(); liveLayer = null; coverageMasks.evictAll()
        clipboard?.recycle(); clipboard = null
        pickerImage?.recycle(); pickerImage = null; selectionStart = null; selectionDraft = null; maskPoints.clear(); maskGuide = null
    }
    private fun installDocument(document: DrawingDocumentSnapshot, keepHistory: Boolean) {
        val restored = mutableListOf<Bitmap>()
        try {
            val w = document.width.toInt(); val h = document.height.toInt()
            for (item in document.layers) {
                val bitmap = Bitmap.createBitmap(w, h, Bitmap.Config.ARGB_8888); restored.add(bitmap)
                val data = item.rgba
                val pixels = IntArray(w * h) { i -> Color.argb(data[i*4+3].toInt() and 255, data[i*4].toInt() and 255, data[i*4+1].toInt() and 255, data[i*4+2].toInt() and 255) }
                bitmap.setPixels(pixels, 0, w, 0, 0, w, h)
                require(drawingLayerBlends.any { it.first == item.blendMode.toInt() }) { "This document uses a blend mode not yet supported by the hardware canvas." }
                layerProperties[bitmap] = LayerProperties(item.name, item.visible, item.opacity, item.blendMode.toInt(), item.clipped, item.locked, item.alphaLocked, item.mask, item.maskEnabled)
            }
            if (keepHistory) recordStack() else { disposeEdits(history); disposeEdits(future) }
            images.forEach { it.recycle() }; images.clear(); images.addAll(restored); restored.clear()
            canvasW = w; canvasH = h; clearDimensionCaches()
            operationBusy = false; layer = document.activeLayer.toInt(); resetCanvasView(); changed()
        } finally { restored.forEach { it.recycle() } }
    }
    suspend fun openDocument(bytes: ByteArray) {
        check(!operationBusy && stroke == null && maskPoints.isEmpty() && transformMode == null) { "Finish the active operation before opening a drawing." }
        operationBusy = true
        try {
            val decoded = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) { drawingDecodePrecision(bytes) }
            installDocument(decoded.document, false)
            decoded.nativeLayers.forEachIndexed { index, document -> document?.let { attachNative(images[index], it) } }
            animationSession = null; previousOnion = null; nextOnion = null
        } finally { operationBusy = false }
    }
    suspend fun transformDocument(operation: String, width: Int = canvasW, height: Int = canvasH, centered: Boolean = true) {
        if (operationBusy || stroke != null || maskPoints.isNotEmpty() || transformMode != null) return
        val bounds = RectF(); selection?.computeBounds(bounds, true)
        if (operation == "crop" && (selection == null || bounds.isEmpty)) error("Select a non-empty area to crop.")
        operationBusy = true
        try {
            val snapshot = documentSnapshot(); val native = images.map { nativeBacking(it) }
            val result = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) {
                var command = operation; var w = width; var h = height
                var ox = if (centered) (width - snapshot.width.toInt()) / 2 else 0
                var oy = if (centered) (height - snapshot.height.toInt()) / 2 else 0
                when (operation) {
                    "trim" -> { val edges = drawingDocumentBounds(snapshot); command = "crop"; w = edges[2].toInt(); h = edges[3].toInt(); ox = edges[0].toInt(); oy = edges[1].toInt() }
                    "crop" -> {
                        ox = floor(bounds.left).toInt().coerceIn(0, snapshot.width.toInt() - 1)
                        oy = floor(bounds.top).toInt().coerceIn(0, snapshot.height.toInt() - 1)
                        w = ceil(bounds.right).toInt().coerceIn(ox + 1, snapshot.width.toInt()) - ox
                        h = ceil(bounds.bottom).toInt().coerceIn(oy + 1, snapshot.height.toInt()) - oy
                    }
                }
                val output = drawingTransformDocument(snapshot, command, w.toUInt(), h.toUInt(), ox, oy)
                val transformed = native.map { source -> source?.fork()?.also { it.transformFloat(command, w.toUInt(), h.toUInt(), ox, oy); it.discardHistory() } }
                check(transformed.sumOf { it?.info()?.liveBytes?.toLong() ?: 0L } <= 96L * 1024 * 1024) { "Transformed native layers exceed the 96 MiB live-tile budget." }
                output.copy(layers = output.layers.mapIndexed { index, item -> transformed[index]?.let { item.copy(rgba = it.layerSrgb(1u)) } ?: item }) to transformed
            }
            installDocument(result.first, true)
            result.second.forEachIndexed { index, native -> native?.let { attachNative(images[index], it) } }
        } finally { operationBusy = false; changed() }
    }
    private suspend fun nativeRasterOperation(operation: suspend (ByteArray, UInt, UInt) -> ByteArray) {
        if (operationBusy || transformMode != null || stroke != null) return
        check(!layerLocked(layer)) { "Unlock the active layer before editing pixels." }
        operationBusy = true
        try {
            val index = layer; val input = rgba(images[index]); val w = canvasW.toUInt(); val h = canvasH.toUInt()
            val result = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) { operation(input, w, h) }
            require(index == layer && transformMode == null) { "The selected layer changed." }
            replacePixels(result)
        } finally { operationBusy = false }
    }
    suspend fun applyAdvancedFilter(operation: String, radius: UInt, strength: Float, angle: Float, seed: ULong) = nativeRasterOperation { input, w, h -> drawingAdvancedFilter(input, w, h, operation, radius, strength, angle, seed) }
    suspend fun applyMultiGradient(preset: DrawingGradientPreset, angle: Float, extent: Float, x: Float, y: Float) {
        val strength = opacity
        nativeRasterOperation { input, w, h ->
            val radians = Math.toRadians(angle.toDouble()); val length = min(w.toFloat(), h.toFloat()) * extent
            val start = listOf(w.toFloat() * x, h.toFloat() * y)
            drawingMultistopGradient(input, w, h, start, listOf(start[0] + cos(radians).toFloat() * length, start[1] + sin(radians).toFloat() * length), preset, strength)
        }
    }
    suspend fun applyPattern(kind: UInt, scale: Float, angle: Float, density: Float, foreground: Int, background: Int, seed: UInt) {
        fun bytes(c: Int) = byteArrayOf(Color.red(c).toByte(), Color.green(c).toByte(), Color.blue(c).toByte(), Color.alpha(c).toByte())
        val first = bytes(foreground); val last = bytes(background); val strength = opacity
        nativeRasterOperation { input, w, h -> drawingPatternFill(input, w, h, kind, scale, angle, density, first, last, strength, seed) }
    }
    private fun installNativePixels(bitmap: Bitmap, document: DrawingTiledDocument, output: ByteArray) {
        val before = copy(bitmap)
        try {
            val pixels = IntArray(canvasW * canvasH) { i -> Color.argb(output[i*4+3].toInt() and 255, output[i*4].toInt() and 255, output[i*4+1].toInt() and 255, output[i*4+2].toInt() and 255) }
            bitmap.setPixels(pixels, 0, canvasW, 0, 0, canvasW, canvasH)
            attachNative(bitmap, document)
            record(before, constrainAlpha = false)
        } catch (e: Exception) {
            Canvas(bitmap).drawBitmap(before, 0f, 0f, Paint().apply { xfermode = PorterDuffXfermode(PorterDuff.Mode.SRC) })
            nativeBacking(before)?.let { attachNative(bitmap, it.fork()) } ?: nativeBackings.remove(bitmap)
            before.recycle(); throw e
        }
    }
    suspend fun applyAdjustments(settings: DrawingAdjustments) {
        if (operationBusy || transformMode != null || stroke != null || maskPoints.isNotEmpty()) return
        val bitmap = images[layer]; val native = nativeBacking(bitmap)
        if (native == null) { nativeRasterOperation { input, w, h -> drawingAdjustRgba(input, w, h, settings) }; return }
        check(!layerLocked(layer)) { "Unlock the active layer before editing pixels." }
        val selected = selectionBytes(); operationBusy = true
        try {
            val result = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) {
                val target = native.fork(); target.adjustLinear(1u, settings, selected); target.discardHistory()
                target to target.layerSrgb(1u)
            }
            check(images[layer] === bitmap) { "The selected layer changed." }
            installNativePixels(bitmap, result.first, result.second)
        } finally { operationBusy = false; changed() }
    }
    private suspend fun nativeFloatOperation(operation: (DrawingTiledDocument, ByteArray, Boolean) -> Unit): Boolean {
        if (operationBusy || transformMode != null || stroke != null || maskPoints.isNotEmpty()) return true
        val bitmap = images[layer]; val native = nativeBacking(bitmap) ?: return false
        check(!layerLocked(layer)) { "Unlock the active layer before editing pixels." }
        val selected = selectionBytes(); val alphaLocked = layerAlphaLocked(layer)
        val otherBytes = images.filter { it !== bitmap }.sumOf { nativeBacking(it)?.info()?.liveBytes?.toLong() ?: 0L }
        operationBusy = true
        try {
            val result = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) {
                val target = native.fork(); operation(target, selected, alphaLocked); target.discardHistory()
                check(otherBytes + target.info().liveBytes.toLong() <= 96L * 1024 * 1024) { "Native layers exceed the 96 MiB live-tile budget." }
                target to target.layerSrgb(1u)
            }
            check(images[layer] === bitmap) { "The selected layer changed." }
            installNativePixels(bitmap, result.first, result.second)
        } finally { operationBusy = false; changed() }
        return true
    }
    suspend fun applyCurve(values: ByteArray, channel: UInt) {
        if (nativeFloatOperation { target, selected, locked -> target.applyCurveFloat(1u, values, channel, selected, locked) }) return
        nativeRasterOperation { input, w, h -> drawingApplyCurve(input, w, h, values, channel) }
    }
    suspend fun applyFilter(operation: String, amount: Float) {
        if (operation in setOf("flip-x", "flip-y", "rotate-180", "offset-wrap") && nativeFloatOperation { target, selected, locked -> target.remapFloat(1u, operation, selected, locked) }) return
        val floatFilters = setOf("invert", "grayscale", "sepia", "threshold", "brightness", "contrast", "saturation", "posterize", "gamma", "temperature", "solarize", "red-channel", "green-channel", "blue-channel", "alpha-channel", "alpha-threshold", "alpha-luma", "opaque")
        if (operation in floatFilters && nativeFloatOperation { target, selected, locked -> target.filterFloat(1u, operation, amount, selected, locked) }) return
        if (operationBusy || transformMode != null || stroke != null) return
        check(!layerLocked(layer)) { "Unlock the active layer before editing pixels." }
        operationBusy = true
        try {
            val index = layer; val input = rgba(images[index])
            val result = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) { drawingFilterRgba(input, canvasW.toUInt(), canvasH.toUInt(), operation, amount) }
            require(index == layer && transformMode == null) { "The selected layer changed. Apply the filter again." }
            replacePixels(result)
        } finally { operationBusy = false }
    }
    suspend fun blendColor(mode: UInt) {
        if (operationBusy || transformMode != null || stroke != null) return
        check(!layerLocked(layer)) { "Unlock the active layer before editing pixels." }
        operationBusy = true
        try {
            val index = layer; val input = rgba(images[index]); val strength = opacity
            val color = byteArrayOf(Color.red(inkColor).toByte(), Color.green(inkColor).toByte(), Color.blue(inkColor).toByte(), Color.alpha(inkColor).toByte())
            val result = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) { drawingBlendColor(input, canvasW.toUInt(), canvasH.toUInt(), color, strength, mode) }
            require(index == layer && transformMode == null) { "The selected layer changed. Apply the blend again." }
            replacePixels(result)
        } finally { operationBusy = false }
    }
    suspend fun applyGradient(kind: UInt, repeat: UInt, reverse: Boolean, transparentEnd: Boolean, endColor: Int, angleDegrees: Float, extent: Float, centerX: Float, centerY: Float) {
        if (operationBusy || transformMode != null || stroke != null) return
        check(!layerLocked(layer)) { "Unlock the active layer before editing pixels." }
        operationBusy = true
        try {
            val index = layer; val input = rgba(images[index]); val strength = opacity
            fun colorBytes(c: Int, transparent: Boolean = false) = byteArrayOf(Color.red(c).toByte(), Color.green(c).toByte(), Color.blue(c).toByte(), if (transparent) 0 else Color.alpha(c).toByte())
            val first = colorBytes(inkColor); val last = colorBytes(endColor, transparentEnd)
            val angle = Math.toRadians(angleDegrees.toDouble())
            val length = min(canvasW, canvasH) * extent
            val start = listOf(canvasW * centerX, canvasH * centerY)
            val end = listOf(start[0] + cos(angle).toFloat() * length, start[1] + sin(angle).toFloat() * length)
            val result = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) {
                drawingGradientRgba(input, canvasW.toUInt(), canvasH.toUInt(), start, end, first, last, kind, repeat, reverse, strength)
            }
            require(index == layer && transformMode == null) { "The selected layer changed. Apply the gradient again." }
            replacePixels(result)
        } finally { operationBusy = false }
    }
    private fun selectionBytes(): ByteArray {
        val path = selection ?: return ByteArray(0)
        val mask = Bitmap.createBitmap(canvasW, canvasH, Bitmap.Config.ALPHA_8)
        try {
            Canvas(mask).drawPath(path, Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.WHITE; style = Paint.Style.FILL })
            val buffer = java.nio.ByteBuffer.allocate(mask.rowBytes * mask.height)
            mask.copyPixelsToBuffer(buffer)
            val bytes = buffer.array()
            return ByteArray(canvasW * canvasH).also { out -> for (row in 0 until canvasH) bytes.copyInto(out, row * canvasW, row * mask.rowBytes, row * mask.rowBytes + canvasW) }
        } finally { mask.recycle() }
    }
    private fun fill(x: Int, y: Int) {
        if (operationBusy || layerLocked(layer)) return
        val color = byteArrayOf(Color.red(inkColor).toByte(), Color.green(inkColor).toByte(), Color.blue(inkColor).toByte(), (Color.alpha(inkColor) * opacity).toInt().toByte())
        val input = rgba(images[layer]); val reference = if (fillSampleMerged) visibleSample().let { image -> try { rgba(image) } finally { image.recycle() } } else input
        val selected = selectionBytes(); val w = canvasW.toUInt(); val h = canvasH.toUInt(); val tolerance = wandTolerance.coerceIn(0, 255).toUByte(); val contiguous = fillContiguous; val grow = fillGrow
        operationBusy = true
        maskScope.launch {
            try {
                val output = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) { drawingFillRegion(input, reference, w, h, x.toUInt(), y.toUInt(), color, tolerance, contiguous, grow, selected) }
                replacePixels(output, clipToSelection = false)
            } catch (e: kotlinx.coroutines.CancellationException) { throw e }
            catch (e: Exception) { android.widget.Toast.makeText(context, e.message ?: "Fill failed", android.widget.Toast.LENGTH_LONG).show() }
            finally { operationBusy = false; changed() }
        }
    }
    private data class PressureSegment(val path: Path, val pressure: Float)
    private val pressureSegments = mutableListOf<PressureSegment>()
    private var pressureStroke = false
    private var eraserStroke = false
    private var lastPressure = 1f
    private var midX = 0f
    private var midY = 0f
    private fun penPressure(event: MotionEvent, history: Int? = null): Float {
        val raw = if (history == null) event.pressure else event.getHistoricalPressure(history)
        val range = event.device?.getMotionRange(MotionEvent.AXIS_PRESSURE, event.source)
        val normalized = if (range != null && range.range > 0) (raw - range.min) / range.range else raw
        return if (normalized.isFinite()) normalized.coerceIn(0f, 1f) else 1f
    }
    var opacity = 1f
    var brush = "Pen"
    val shapes = DrawingShapes()
    var tool = "Brush"
        set(value) {
            if (operationBusy) return
            if (field != value) { shapes.clear(); polygonVertices.clear(); maskPoints.clear(); maskGuide = null; selectionStart = null; selectionDraft = null; pickerImage?.recycle(); pickerImage = null }
            field = value
        }
    var layer = 0
        set(value) { if (operationBusy) return; shapes.clear(); polygonVertices.clear(); polygonContactAdded = false; maskPoints.clear(); maskGuide = null; if (transformMode != null) finishTransform(false); field = value.coerceIn(0, images.lastIndex); clearSelection(recordUndo = false) }
    val layers get() = images.size
    val canAddLayer get() = !operationBusy && images.size < 16 && transformMode == null && (images.size + 1L) * canvasW * canvasH * 4 + images.sumOf { properties(it).mask.size.toLong() } <= 96L * 1024 * 1024
    var currentLayerOnly = false
    var selectionMode = "Add"
    var fillContiguous = true
    var fillSampleMerged = false
    var fillGrow = 0u
    var wandTolerance = 16
    var wandContiguous = true
    var selection: Path? by mutableStateOf(null)
        private set
    private var clipboard: Bitmap? by mutableStateOf(null)
    val canPaste get() = clipboard != null
    var transformMode: String? by mutableStateOf(null)
        private set
    var meshDivisions = 3
        private set
    private var source: Bitmap? = null
    private var original: Bitmap? = null
    private var corners = FloatArray(8)
    private var vertices = FloatArray(0)
    private var bounds = RectF()
    var affineError: String? by mutableStateOf(null)
        private set
    private var normalScale = 1f
    private var normalScaleY = 1f
    private var normalAngle = 0f
    private var normalSkewX = 0f
    private var normalSkewY = 0f
    private var normalFlipX = false
    private var normalFlipY = false
    private var pivotX = .5f
    private var pivotY = .5f
    private fun normalMatrix() = Matrix().apply { setValues(drawingAffineMatrix(DrawingAffineSettings(
        normalScale * if (normalFlipX) -1f else 1f, normalScaleY * if (normalFlipY) -1f else 1f,
        normalAngle, normalSkewX, normalSkewY, moveX, moveY,
        bounds.left + bounds.width() * pivotX, bounds.top + bounds.height() * pivotY)).toFloatArray()) }
    fun setAffine(scaleX: Float, scaleY: Float, angle: Float, skewX: Float, skewY: Float, flipX: Boolean, flipY: Boolean, pivotHorizontal: Float, pivotVertical: Float) {
        if (transformMode != "Normal" || operationBusy) return
        // Validate first, leaving the previous transform untouched if the candidate is singular.
        try {
            drawingAffineMatrix(DrawingAffineSettings(scaleX * if (flipX) -1f else 1f, scaleY * if (flipY) -1f else 1f, angle, skewX, skewY, moveX, moveY, bounds.left + bounds.width() * pivotHorizontal, bounds.top + bounds.height() * pivotVertical))
            affineError = null
        } catch (e: Exception) { affineError = "Settings rejected; previous preview remains. ${e.message}"; changed(); return }
        normalScale = scaleX; normalScaleY = scaleY; normalAngle = angle; normalSkewX = skewX; normalSkewY = skewY
        normalFlipX = flipX; normalFlipY = flipY; pivotX = pivotHorizontal; pivotY = pivotVertical; changed()
    }
    private var moveX = 0f
    private var moveY = 0f
    private var handle = -1
    var mirrorCanvas = false
    var showTransparency = false
    var showGrid = false
    var gridSpacing = 64f
    private var zoom = 1f
    private var angle = 0f
    private var dx = 0f
    private var dy = 0f
    private var oldSpan = 0f
    private var oldAngle = 0f
    private var oldX = 0f
    private var oldY = 0f
    private var navigating = false
    private var shapeTouchActive = false
    private var stroke: Path? = null
    private var strokeBefore: Bitmap? = null
    private var lastX = 0f
    private var lastY = 0f
    private var penCursor: PointF? = null
    private var penEraser = false
    var showPenCursor = true
    private fun updatePenCursor(e: MotionEvent) {
        val pen = e.pointerCount == 1 && (e.getToolType(0) == MotionEvent.TOOL_TYPE_STYLUS || e.getToolType(0) == MotionEvent.TOOL_TYPE_ERASER)
        penEraser = pen && e.getToolType(0) == MotionEvent.TOOL_TYPE_ERASER
        penCursor = if (pen && showPenCursor && tool in listOf("Brush", "Eraser", "Smudge", "Blur", "Reveal mask", "Hide mask", "Clone stamp", "Dodge", "Burn", "Desaturate", "Colorize")) PointF(e.x, e.y) else null
    }
    override fun onHoverEvent(event: MotionEvent): Boolean {
        when (event.actionMasked) {
            MotionEvent.ACTION_HOVER_ENTER, MotionEvent.ACTION_HOVER_MOVE -> updatePenCursor(event)
            MotionEvent.ACTION_HOVER_EXIT -> penCursor = null
        }
        invalidate()
        return if (event.getToolType(0) == MotionEvent.TOOL_TYPE_STYLUS || event.getToolType(0) == MotionEvent.TOOL_TYPE_ERASER) true else super.onHoverEvent(event)
    }
    private fun drawPenCursor(c: Canvas) {
        val cursor = penCursor ?: return
        if (!showPenCursor || transformMode != null || tool !in listOf("Brush", "Eraser", "Smudge", "Blur")) return
        val position = point(cursor.x, cursor.y)
        if (assistantMode != 0u && tool in listOf("Brush", "Eraser", "Smudge", "Blur", "Reveal mask", "Hide mask", "Clone stamp", "Dodge", "Burn", "Desaturate", "Colorize")) {
            val snap = assisted(DrawingInput(position[0], position[1], 1f)); position[0] = snap.x; position[1] = snap.y
        }
        if (position[0] !in 0f..canvasW.toFloat() || position[1] !in 0f..canvasH.toFloat()) return
        camera().mapPoints(position)
        val native = nativeBrushEnabled && tool == "Brush"
        val width = if (tool != "Brush" || penEraser || native) brushSize else DrawingBrushes.footprint(brush, brushSize)
        val radius = (width * fit() * zoom / 2).coerceAtLeast(3f)
        val p = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE }
        c.save(); c.translate(position[0], position[1])
        if (native) c.rotate(angle + if (mirrorCanvas) -nativeBrushSettings.rotation else nativeBrushSettings.rotation)
        fun outline() {
            if (!native) c.drawCircle(0f, 0f, radius, p)
            else {
                val ry = radius * nativeBrushSettings.aspect
                when (nativeBrushSettings.tip) {
                    1u -> c.drawRect(-radius, -ry, radius, ry, p)
                    2u -> c.drawPath(Path().apply { moveTo(0f, -ry); lineTo(radius, 0f); lineTo(0f, ry); lineTo(-radius, 0f); close() }, p)
                    else -> c.drawOval(-radius, -ry, radius, ry, p)
                }
            }
        }
        p.color = Color.BLACK; p.strokeWidth = 3f * resources.displayMetrics.density; outline()
        p.color = Color.WHITE; p.strokeWidth = resources.displayMetrics.density; outline()
        p.style = Paint.Style.FILL; c.drawCircle(0f, 0f, 1.5f * resources.displayMetrics.density, p)
        c.restore()
    }
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG or Paint.FILTER_BITMAP_FLAG)
    private var liveLayer: Bitmap? = null
    private data class CoverageKey(val path: Path, val width: Float)
    private data class CoverageTile(val bitmap: Bitmap, val x: Float, val y: Float)
    private val coverageMasks = object : android.util.LruCache<CoverageKey, CoverageTile>(8 * 1024 * 1024) {
        override fun sizeOf(key: CoverageKey, value: CoverageTile) = value.bitmap.allocationByteCount
    }
    init { setLayerType(LAYER_TYPE_HARDWARE, null) }
    private fun blank() = Bitmap.createBitmap(canvasW, canvasH, Bitmap.Config.ARGB_8888)
    private fun copy(b: Bitmap) = b.copy(Bitmap.Config.ARGB_8888, true)!!.also { result ->
        layerProperties[result] = properties(b).copy()
        nativeBacking(b)?.let { attachNative(result, it.fork()) }
    }
    private fun fit() = min(width.toFloat() / canvasW, height.toFloat() / canvasH)
    private fun left() = (width - canvasW * fit()) / 2
    private fun top() = (height - canvasH * fit()) / 2
    private fun contentMatrix() = Matrix().apply {
        if (mirrorCanvas) postScale(-1f, 1f, canvasW / 2f, canvasH / 2f)
        postScale(fit() * zoom, fit() * zoom); postRotate(angle)
    }
    private fun camera() = contentMatrix().apply { postTranslate(left() + dx, top() + dy) }
    private fun point(x: Float, y: Float): FloatArray = floatArrayOf(x, y).also { val inverse = Matrix(); camera().invert(inverse); inverse.mapPoints(it) }
    private fun changed() {
        nativeBackings.entries.removeAll { it.key.isRecycled || it.key.generationId != it.value.generation }
        invalidate(); onEdit()
    }
    private fun disposeEdits(edits: ArrayDeque<Edit>) { edits.forEach { edit -> edit.before?.recycle(); edit.stack?.forEach { it.recycle() } }; edits.clear() }
    private fun record(before: Bitmap?, index: Int = layer, constrainAlpha: Boolean = true) {
        if (constrainAlpha && before != null && properties(images[index]).alphaLocked && transformMode == null && nativeBacking(images[index]) == null) {
            val constrained = drawingPreserveAlpha(rgba(images[index]), rgba(before), canvasW.toUInt(), canvasH.toUInt())
            val pixels = IntArray(canvasW * canvasH) { i -> Color.argb(constrained[i*4+3].toInt() and 255, constrained[i*4].toInt() and 255, constrained[i*4+1].toInt() and 255, constrained[i*4+2].toInt() and 255) }
            images[index].setPixels(pixels, 0, canvasW, 0, 0, canvasW, canvasH)
        }
        disposeEdits(future); history.addLast(Edit(index, before)); trimHistory()
    }
    private fun trimHistory() {
        while (history.size > 1 && (history.size > 128 || history.sumOf { edit -> (edit.before?.let { historyBytes(it) } ?: 0L) + (edit.stack?.sumOf { historyBytes(it) } ?: 0L) } > 64L * 1024 * 1024)) {
            val removed = history.removeFirst(); removed.before?.recycle(); removed.stack?.forEach { it.recycle() }
        }
    }
    private fun recordSelection() {
        disposeEdits(future)
        history.addLast(Edit(layer, null, selectionChange = true, selectionBefore = selection?.let { Path(it) }))
        trimHistory()
    }
    private fun recordStack() {
        disposeEdits(future); history.addLast(Edit(layer, null, images.map { copy(it) }, canvasW, canvasH)); trimHistory()
    }
    fun layerName(index: Int) = properties(images[index]).name
    fun layerVisible(index: Int) = properties(images[index]).visible
    fun layerOpacity(index: Int) = properties(images[index]).opacity
    fun hasLayerMask(index: Int) = properties(images[index]).mask.isNotEmpty()
    fun layerMaskEnabled(index: Int) = properties(images[index]).maskEnabled
    private fun setMask(bytes: ByteArray) {
        require(bytes.isEmpty() || bytes.size == canvasW * canvasH)
        val others = images.indices.filter { it != layer }.sumOf { properties(images[it]).mask.size.toLong() }
        require(images.size.toLong() * canvasW * canvasH * 4 + others + bytes.size <= 96L * 1024 * 1024) { "Mask exceeds the document raster budget." }
        record(copy(images[layer]), constrainAlpha = false)
        properties(images[layer]).mask = bytes; properties(images[layer]).maskEnabled = true; changed()
    }
    fun createLayerMask(fromSelection: Boolean) {
        if (operationBusy || transformMode != null || layerLocked(layer)) return
        require(!fromSelection || selection != null) { "Create a selection first." }
        setMask(if (fromSelection) selectionBytes() else ByteArray(canvasW * canvasH) { 255.toByte() })
    }
    fun toggleLayerMask(enabled: Boolean) {
        if (operationBusy || transformMode != null || !hasLayerMask(layer)) return
        record(copy(images[layer]), constrainAlpha = false); properties(images[layer]).maskEnabled = enabled; changed()
    }
    fun removeLayerMask() {
        if (operationBusy || transformMode != null || layerLocked(layer) || !hasLayerMask(layer)) return
        setMask(ByteArray(0))
    }
    suspend fun editLayerMask(operation: String, radius: UInt) {
        if (operationBusy || transformMode != null || layerLocked(layer) || !hasLayerMask(layer)) return
        operationBusy = true
        try {
            val input = properties(images[layer]).mask; val w = canvasW.toUInt(); val h = canvasH.toUInt()
            val output = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) { drawingProcessMask(input, w, h, operation, radius) }
            setMask(output)
        } finally { operationBusy = false }
    }
    suspend fun bakeLayerMask() {
        if (operationBusy || transformMode != null || layerLocked(layer) || !hasLayerMask(layer) || !layerMaskEnabled(layer)) return
        operationBusy = true
        try {
            val input = rgba(images[layer]); val mask = properties(images[layer]).mask; val w = canvasW.toUInt(); val h = canvasH.toUInt()
            val output = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) { drawingMaskPixels(input, mask, w, h) }
            val pixels = IntArray(canvasW * canvasH) { i -> Color.argb(output[i*4+3].toInt() and 255, output[i*4].toInt() and 255, output[i*4+1].toInt() and 255, output[i*4+2].toInt() and 255) }
            record(copy(images[layer]), constrainAlpha = false)
            images[layer].setPixels(pixels, 0, canvasW, 0, 0, canvasW, canvasH)
            properties(images[layer]).mask = ByteArray(0); changed()
        } finally { operationBusy = false }
    }
    fun layerLocked(index: Int) = properties(images[index]).locked
    fun layerAlphaLocked(index: Int) = properties(images[index]).alphaLocked
    fun layerClipped(index: Int) = properties(images[index]).clipped
    fun layerBlendMode(index: Int) = properties(images[index]).blendMode
    fun setLayerFlag(flag: String, enabled: Boolean) {
        if (operationBusy || transformMode != null) return
        record(copy(images[layer])); val p = properties(images[layer])
        when (flag) { "locked" -> p.locked = enabled; "alpha" -> p.alphaLocked = enabled; "clip" -> p.clipped = enabled }
        changed()
    }
    fun setLayerBlendMode(mode: Int) {
        if (operationBusy || transformMode != null || drawingLayerBlends.none { it.first == mode }) return
        record(copy(images[layer])); properties(images[layer]).blendMode = mode; changed()
    }
    fun renameLayer(name: String) {
        if (operationBusy || transformMode != null) return
        record(copy(images[layer])); properties(images[layer]).name = name.trim().take(80).ifEmpty { "Layer" }; changed()
    }
    fun toggleLayerVisibility(index: Int) {
        if (operationBusy || transformMode != null) return
        record(copy(images[index]), index); properties(images[index]).visible = !properties(images[index]).visible; changed()
    }
    fun setLayerOpacity(value: Float) {
        if (operationBusy || transformMode != null || !value.isFinite()) return
        val clamped = value.coerceIn(0f, 1f)
        if (clamped == properties(images[layer]).opacity) return
        record(copy(images[layer])); properties(images[layer]).opacity = clamped; changed()
    }
    fun duplicateLayer() {
        if (!canAddLayer) return
        if ((images.size + 1L) * canvasW * canvasH * 4 + images.sumOf { properties(it).mask.size.toLong() } + properties(images[layer]).mask.size > 96L * 1024 * 1024) return
        recordStack(); val duplicate = copy(images[layer]); properties(duplicate).name += " copy"
        images.add(layer + 1, duplicate); layer += 1; changed()
    }
    fun deleteLayer() {
        if (operationBusy || transformMode != null || images.size <= 1 || layerLocked(layer)) return
        recordStack(); images.removeAt(layer).recycle(); layer = min(layer, images.lastIndex); changed()
    }
    fun moveLayer(offset: Int) {
        val target = layer + offset
        if (operationBusy || transformMode != null || target !in images.indices || layerLocked(layer)) return
        recordStack(); val image = images.removeAt(layer); images.add(target, image); layer = target; changed()
    }
    val canMergeDown get() = layer > 0 && !layerLocked(layer) && !layerLocked(layer - 1) && layerVisible(layer) && layerVisible(layer - 1) && !layerClipped(layer) && !layerClipped(layer - 1) && layerBlendMode(layer) == 0 && layerBlendMode(layer - 1) == 0 && !hasLayerMask(layer) && !hasLayerMask(layer - 1)
    suspend fun flattenDocument() {
        if (operationBusy || transformMode != null || stroke != null) return
        check(!layerLocked(layer)) { "Unlock the active layer before editing pixels." }
        operationBusy = true
        try {
            val snapshot = documentSnapshot()
            val flattened = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) { drawingFlattenDocument(snapshot) }
            installDocument(flattened, true)
        } finally { operationBusy = false }
    }
    suspend fun mergeLayerDown(mode: UInt = 0u) {
        if (!canMergeDown || operationBusy || transformMode != null) return
        operationBusy = true
        try {
            val index = layer; val lower = rgba(images[index - 1]); val upper = rgba(images[index])
            val lowerAlpha = layerOpacity(index - 1); val upperAlpha = layerOpacity(index)
            val output = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) {
                drawingMergeLayers(lower, upper, canvasW.toUInt(), canvasH.toUInt(), lowerAlpha, upperAlpha, mode)
            }
            require(index == layer && transformMode == null) { "The selected layer changed." }
            val merged = blank()
            try {
                val pixels = IntArray(canvasW * canvasH) { i -> Color.argb(output[i*4+3].toInt() and 255, output[i*4].toInt() and 255, output[i*4+1].toInt() and 255, output[i*4+2].toInt() and 255) }
                merged.setPixels(pixels, 0, canvasW, 0, 0, canvasW, canvasH)
                layerProperties[merged] = LayerProperties(layerName(index - 1), true, 1f)
                recordStack()
            } catch (e: Exception) { merged.recycle(); throw e }
            images.removeAt(index).recycle(); images[index - 1].recycle(); images[index - 1] = merged
            // The public setter intentionally ignores changes while a native operation is busy.
            operationBusy = false; layer = index - 1; changed()
        } finally { operationBusy = false }
    }
    private fun blendPaint(image: Bitmap) = Paint(Paint.FILTER_BITMAP_FLAG).apply {
        val p = properties(image); alpha = (p.opacity * 255).roundToInt()
        blendMode = when (p.blendMode) {
            2 -> BlendMode.DARKEN; 3 -> BlendMode.MULTIPLY; 4 -> BlendMode.COLOR_BURN
            7 -> BlendMode.LIGHTEN; 8 -> BlendMode.SCREEN; 9 -> BlendMode.COLOR_DODGE
            12 -> BlendMode.OVERLAY; 13 -> BlendMode.SOFT_LIGHT; 14 -> BlendMode.HARD_LIGHT
            19 -> BlendMode.DIFFERENCE; 20 -> BlendMode.EXCLUSION; 23 -> BlendMode.HUE
            24 -> BlendMode.SATURATION; 25 -> BlendMode.COLOR; 26 -> BlendMode.LUMINOSITY
            else -> BlendMode.SRC_OVER
        }
    }
    private fun beginLayer(c: Canvas, image: Bitmap) = c.saveLayer(0f, 0f, canvasW.toFloat(), canvasH.toFloat(), blendPaint(image))
    private val maskTiles = object : android.util.LruCache<ByteArray, Bitmap>(8 * 1024 * 1024) {
        override fun sizeOf(key: ByteArray, value: Bitmap) = value.allocationByteCount
    }
    private fun maskBitmap(bytes: ByteArray): Bitmap = maskTiles.get(bytes) ?: Bitmap.createBitmap(canvasW, canvasH, Bitmap.Config.ALPHA_8).also { bitmap ->
        val data = java.nio.ByteBuffer.allocate(bitmap.rowBytes * canvasH)
        for (row in 0 until canvasH) { data.position(row * bitmap.rowBytes); data.put(bytes, row * canvasW, canvasW) }
        data.rewind(); bitmap.copyPixelsFromBuffer(data); maskTiles.put(bytes, bitmap)
    }
    private fun endLayer(c: Canvas, saved: Int, index: Int) {
        val p = properties(images[index])
        if (p.maskEnabled && p.mask.isNotEmpty()) c.drawBitmap(maskBitmap(p.mask), 0f, 0f, Paint().apply { xfermode = PorterDuffXfermode(PorterDuff.Mode.DST_IN) })
        if (properties(images[index]).clipped) {
            if (index == 0 || !layerVisible(index - 1)) c.drawColor(Color.TRANSPARENT, PorterDuff.Mode.CLEAR)
            else {
                c.drawBitmap(images[index - 1], 0f, 0f, Paint(Paint.FILTER_BITMAP_FLAG).apply { xfermode = PorterDuffXfermode(PorterDuff.Mode.DST_IN); alpha = (layerOpacity(index - 1) * 255).roundToInt() })
                val lower = properties(images[index - 1])
                if (lower.maskEnabled && lower.mask.isNotEmpty()) c.drawBitmap(maskBitmap(lower.mask), 0f, 0f, Paint().apply { xfermode = PorterDuffXfermode(PorterDuff.Mode.DST_IN) })
            }
        }
        c.restoreToCount(saved)
    }
    private fun drawLayer(c: Canvas, image: Bitmap) {
        if (!properties(image).visible) return
        val index = images.indexOf(image); val saved = beginLayer(c, image)
        c.drawBitmap(image, 0f, 0f, null); endLayer(c, saved, index)
    }
    fun cutSelection() {
        if (operationBusy || selection == null || transformMode != null || layerLocked(layer)) return
        copySelection(); eraseSelection()
    }
    fun resetCanvasView() { zoom = 1f; angle = 0f; dx = 0f; dy = 0f; changed() }
    fun rotateCanvasView(degrees: Float) { if (degrees.isFinite()) { angle = degrees; changed() } }
    fun setCanvasZoom(value: Float) { if (value.isFinite()) { zoom = value.coerceIn(.25f, 8f); changed() } }
    val canvasZoom get() = zoom
    val canvasAngle get() = angle
    fun undo() { if (operationBusy) return; if (shapes.pending) { shapes.removeLast(); changed() } else exchange(history, future) }
    fun changeShape(name: String) { shapes.kind = name; changed() }
    fun connectShape() { shapes.connected = !shapes.connected; changed() }
    fun cancelShapeDraft() { shapes.clear(); changed() }
    private fun shapeStroke(c: Canvas, path: Path) {
        c.save(); selection?.let { c.clipPath(it) }
        val p = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = inkColor; alpha = (opacity * 255).toInt(); style = Paint.Style.STROKE; strokeWidth = brushSize; strokeCap = Paint.Cap.ROUND; strokeJoin = Paint.Join.ROUND }
        c.drawPath(path, p); c.restore()
    }
    fun commitShapeDraft() {
        if (operationBusy || layerLocked(layer)) return
        val path = shapes.path() ?: return
        val before = copy(images[layer]); shapeStroke(Canvas(images[layer]), path); record(before); shapes.clear(); changed()
    }
    fun redo() = exchange(future, history)
    private fun exchange(from: ArrayDeque<Edit>, to: ArrayDeque<Edit>) {
        if (operationBusy || from.isEmpty() || transformMode != null) return
        val edit = from.removeLast()
        if (edit.selectionChange) {
            to.addLast(Edit(layer, null, selectionChange = true, selectionBefore = selection?.let { Path(it) }))
            selection = edit.selectionBefore?.let { Path(it) }; changed(); return
        }
        clearSelection(recordUndo = false)
        if (edit.stack != null) {
            to.addLast(Edit(layer, null, images.toList(), canvasW, canvasH)); images.clear(); images.addAll(edit.stack)
            val resized = canvasW != edit.width || canvasH != edit.height
            canvasW = edit.width; canvasH = edit.height
            if (resized) { clearDimensionCaches(); resetCanvasView() }
        }
        else if (edit.before == null) { to.addLast(Edit(edit.layer, images.removeAt(edit.layer))) }
        else if (edit.layer >= images.size) { images.add(edit.before); to.addLast(Edit(edit.layer, null)) }
        else { to.addLast(Edit(edit.layer, images[edit.layer])); images[edit.layer] = edit.before }
        layer = min(edit.layer, images.lastIndex); changed()
    }
    fun addLayer() {
        if (!canAddLayer) return
        images.add(blank()); record(null, images.lastIndex); layer = images.lastIndex; changed()
    }
    fun importImage(bitmap: Bitmap) {
        if (!canAddLayer) { bitmap.recycle(); return }
        addLayer()
        val scale = min(canvasW.toFloat() / bitmap.width, canvasH.toFloat() / bitmap.height)
        val w = bitmap.width * scale; val h = bitmap.height * scale
        Canvas(images[layer]).drawBitmap(bitmap, null, RectF((canvasW - w) / 2, (canvasH - h) / 2, (canvasW + w) / 2, (canvasH + h) / 2), paint)
        bitmap.recycle(); changed()
    }
    private fun maskPath(bytes: ByteArray): Path {
        require(bytes.size == canvasW * canvasH)
        return Path().apply {
            for (y in 0 until canvasH) {
                var x = 0
                while (x < canvasW) {
                    if ((bytes[y * canvasW + x].toInt() and 255) < 128) { x++; continue }
                    val start = x
                    while (x < canvasW && (bytes[y * canvasW + x].toInt() and 255) >= 128) x++
                    addRect(start.toFloat(), y.toFloat(), x.toFloat(), y + 1f, Path.Direction.CW)
                }
            }
        }
    }
    fun selectAll() {
        if (operationBusy || transformMode != null) return
        recordSelection(); selection = Path().apply { addRect(0f, 0f, canvasW.toFloat(), canvasH.toFloat(), Path.Direction.CW) }; changed()
    }
    fun selectLayerOpacity() {
        if (operationBusy || transformMode != null) return
        val pixels = rgba(images[layer]); recordSelection(); selection = maskPath(ByteArray(canvasW * canvasH) { i -> if ((pixels[i*4+3].toInt() and 255) > 0) 255.toByte() else 0 }); changed()
    }
    suspend fun modifySelection(operation: String, radius: UInt) {
        if (operationBusy || transformMode != null) return
        operationBusy = true
        try {
            val bytes = if (selection == null) ByteArray(canvasW * canvasH) else selectionBytes()
            val result = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) { drawingSelectionMask(bytes, canvasW.toUInt(), canvasH.toUInt(), operation, radius) }
            recordSelection(); selection = maskPath(result); changed()
        } finally { operationBusy = false }
    }
    fun eraseSelection() {
        if (operationBusy || transformMode != null || selection == null || layerLocked(layer)) return
        val before = copy(images[layer]); val c = Canvas(images[layer]); c.save(); c.clipPath(selection!!)
        c.drawColor(Color.TRANSPARENT, PorterDuff.Mode.CLEAR); c.restore(); record(before); changed()
    }
    fun clearSelection(recordUndo: Boolean = true) {
        if (operationBusy || selection == null) return
        if (recordUndo) recordSelection()
        selection = null; changed()
    }
    private fun mergeSelection(path: Path, mode: String) {
        val clip = Region(0, 0, canvasW, canvasH)
        val next = Region().apply { setPath(path, clip) }
        val previous = Region().apply { selection?.let { setPath(it, clip) } }
        when (mode) { "Add" -> next.op(previous, Region.Op.UNION); "Intersect" -> next.op(previous, Region.Op.INTERSECT); "Subtract" -> { previous.op(next, Region.Op.DIFFERENCE); next.set(previous) } }
        recordSelection(); selection = if (next.isEmpty) Path() else next.boundaryPath
        changed()
    }
    private fun visibleSample(): Bitmap = blank().also { b ->
        val c = Canvas(b); c.drawColor(Color.WHITE)
        backgroundFrame?.let { c.drawBitmap(it, null, Rect(0, 0, canvasW, canvasH), null) }
        images.forEach { drawLayer(c, it) }
    }
    fun copySelection() {
        if (selection == null || transformMode != null) return
        clipboard?.recycle(); clipboard = selectedPixels(merged = !currentLayerOnly); changed()
    }
    fun paste() {
        val copied = clipboard ?: return
        if (!canAddLayer) return
        addLayer(); Canvas(images[layer]).drawBitmap(copied, 0f, 0f, null)
        selection = Path().apply { addRect(0f, 0f, canvasW.toFloat(), canvasH.toFloat(), Path.Direction.CW) }; changed()
    }
    private fun selectedPixels(merged: Boolean = false): Bitmap = blank().also { result ->
        val c = Canvas(result); selection?.let { c.clipPath(it) }
        if (merged) {
            backgroundFrame?.let { c.drawBitmap(it, null, Rect(0, 0, canvasW, canvasH), null) }
            images.forEach { drawLayer(c, it) }
        } else c.drawBitmap(images[layer], 0f, 0f, null)
    }
    fun beginTransform(mode: String, selectedOnly: Boolean) {
        if (operationBusy || layerLocked(layer)) return
        commitShapeDraft()
        if (transformMode != null) finishTransform(true)
        if (selectedOnly && selection == null) return
        original = copy(images[layer]); source = if (selectedOnly) selectedPixels() else copy(images[layer])
        bounds = RectF(0f, 0f, canvasW.toFloat(), canvasH.toFloat())
        if (selectedOnly) { selection!!.computeBounds(bounds, true); if (!bounds.intersect(0f, 0f, canvasW.toFloat(), canvasH.toFloat())) bounds.setEmpty() }
        if (bounds.width() < 1f || bounds.height() < 1f) { original?.recycle(); source?.recycle(); original = null; source = null; return }
        val c = Canvas(images[layer]); c.save()
        if (selectedOnly) c.clipPath(selection!!)
        c.drawColor(Color.TRANSPARENT, PorterDuff.Mode.CLEAR); c.restore()
        transformMode = mode; affineError = null; normalScale = 1f; normalScaleY = 1f; normalAngle = 0f; normalSkewX = 0f; normalSkewY = 0f; normalFlipX = false; normalFlipY = false; pivotX = .5f; pivotY = .5f; moveX = 0f; moveY = 0f
        corners = floatArrayOf(bounds.left, bounds.top, bounds.right, bounds.top, bounds.right, bounds.bottom, bounds.left, bounds.bottom)
        resetMesh(meshDivisions); if (mode == "Mesh") prepareMesh(); changed()
    }
    fun setScale(scale: Float) { normalScale = scale; normalScaleY = scale; changed() }
    fun resetMesh(divisions: Int) {
        meshDivisions = divisions.coerceIn(2, 12)
        vertices = FloatArray((meshDivisions + 1) * (meshDivisions + 1) * 2)
        var i = 0
        for (y in 0..meshDivisions) for (x in 0..meshDivisions) {
            vertices[i++] = bounds.left + bounds.width() * x / meshDivisions
            vertices[i++] = bounds.top + bounds.height() * y / meshDivisions
        }
        changed()
    }
    private fun transformed(c: Canvas) {
        val src = source ?: return
        paint.reset(); paint.isAntiAlias = true; paint.isFilterBitmap = true
        val crop = Rect(bounds.left.toInt().coerceAtLeast(0), bounds.top.toInt().coerceAtLeast(0), ceil(bounds.right).toInt().coerceAtMost(canvasW), ceil(bounds.bottom).toInt().coerceAtMost(canvasH))
        if (transformMode == "Normal") {
            val m = normalMatrix()
            c.save(); c.concat(m); c.drawBitmap(src, 0f, 0f, paint); c.restore()
        } else if (transformMode == "Perspective") {
            val from = floatArrayOf(bounds.left, bounds.top, bounds.right, bounds.top, bounds.right, bounds.bottom, bounds.left, bounds.bottom)
            val m = Matrix()
            if (m.setPolyToPoly(from, 0, corners, 0, 4)) c.drawBitmap(src, m, paint)
        } else if (!crop.isEmpty) {
            // Source is cropped once at transform start; mesh UVs span exactly the selected bounds.
            val cropped = meshSource ?: return
            c.drawBitmapMesh(cropped, meshDivisions, meshDivisions, vertices, 0, null, 0, paint)
        }
    }
    private var meshSource: Bitmap? = null
    fun prepareMesh() {
        meshSource?.recycle(); meshSource = null
        val src = source ?: return
        val x = bounds.left.toInt().coerceIn(0, canvasW - 1); val y = bounds.top.toInt().coerceIn(0, canvasH - 1)
        val crop = Bitmap.createBitmap(src, x, y, ceil(bounds.width()).toInt().coerceIn(1, canvasW - x), ceil(bounds.height()).toInt().coerceIn(1, canvasH - y))
        meshSource = copy(crop); if (crop !== src) crop.recycle()
    }
    fun finishTransform(apply: Boolean) {
        val before = original ?: return
        val valid = transformMode != "Perspective" || Matrix().setPolyToPoly(floatArrayOf(bounds.left, bounds.top, bounds.right, bounds.top, bounds.right, bounds.bottom, bounds.left, bounds.bottom), 0, corners, 0, 4)
        if (apply && affineError != null) return
        if (apply && valid) { transformed(Canvas(images[layer])); record(before) }
        else { images[layer].recycle(); images[layer] = before }
        original = null; source?.recycle(); source = null; meshSource?.recycle(); meshSource = null; transformMode = null; selection = null; changed()
    }
    fun bitmap(): Bitmap = blank().also { b ->
        val c = Canvas(b)
        images.forEachIndexed { i, image ->
            val props = properties(image)
            if (props.visible) {
                val save = beginLayer(c, image)
                c.drawBitmap(image, 0f, 0f, null); if (i == layer && transformMode != null) transformed(c)
                endLayer(c, save, i)
            }
        }
    }
    fun drawScene(c: Canvas) {
        c.drawColor(Color.rgb(15, 17, 24)); c.save(); c.concat(camera())
        c.clipRect(0, 0, canvasW, canvasH)
        c.drawColor(if (backgroundFrame == null) Color.WHITE else Color.BLACK)
        if (showTransparency && backgroundFrame == null) {
            val checker = Paint().apply { color = Color.rgb(196, 196, 196) }
            val cell = 24f
            for (row in 0..ceil(canvasH / cell).toInt()) for (column in 0..ceil(canvasW / cell).toInt()) {
                if ((row + column) % 2 == 0) c.drawRect(column * cell, row * cell, (column + 1) * cell, (row + 1) * cell, checker)
            }
        }
        paint.reset(); paint.isFilterBitmap = true
        backgroundFrame?.let { c.drawBitmap(it, null, Rect(0, 0, canvasW, canvasH), paint) }
        if (onionEnabled && !animationPlaying) {
            previousOnion?.let { c.drawBitmap(it, 0f, 0f, Paint(Paint.FILTER_BITMAP_FLAG).apply { alpha = (onionOpacity * 255).toInt(); colorFilter = PorterDuffColorFilter(Color.RED, PorterDuff.Mode.SRC_IN) }) }
            nextOnion?.let { c.drawBitmap(it, 0f, 0f, Paint(Paint.FILTER_BITMAP_FLAG).apply { alpha = (onionOpacity * 255).toInt(); colorFilter = PorterDuffColorFilter(Color.GREEN, PorterDuff.Mode.SRC_IN) }) }
        }
        animationPreview?.let { c.drawBitmap(it, 0f, 0f, null) }
        if (animationPreview == null) images.forEachIndexed { i, image ->
            val props = properties(image)
            if (!props.visible) return@forEachIndexed
            val saved = beginLayer(c, image)
            if (i == layer && c.isHardwareAccelerated && stroke != null) {
                // Legacy patterned brushes require a software layer for blur/mask filters.
                // The screen still composites via HWUI; standard pen/eraser coverage uses wgpu MSAA.
                val tile = liveLayer ?: blank().also { liveLayer = it }
                tile.eraseColor(Color.TRANSPARENT)
                val raster = Canvas(tile); raster.drawBitmap(image, 0f, 0f, null)
                stroke?.let { path -> if (tool == "Brush" || tool == "Eraser") drawStroke(raster, path) }
                c.drawBitmap(tile, 0f, 0f, paint.apply { reset(); isFilterBitmap = true })
            } else {
                c.drawBitmap(image, 0f, 0f, null)
                if (i == layer) {
                    if (transformMode != null) transformed(c)
                    stroke?.let { path -> if (tool == "Brush" || tool == "Eraser") drawStroke(c, path) }
                }
            }
            if (i == layer && usesNativeStroke()) drawNativeBrushPreview(c)
            if (i == layer && tool == "Shapes") shapes.path()?.let { shapeStroke(c, it) }
            endLayer(c, saved, i)
        }
        if (showAssistantGuides && assistantMode != 0u) drawAssistantGuides(c)
        if (showGrid) {
            val grid = Paint().apply { color = Color.argb(90, 128, 128, 128); strokeWidth = 1f / (fit() * zoom) }
            val spacing = gridSpacing.coerceIn(8f, 512f)
            var gx = spacing; while (gx < canvasW) { c.drawLine(gx, 0f, gx, canvasH.toFloat(), grid); gx += spacing }
            var gy = spacing; while (gy < canvasH) { c.drawLine(0f, gy, canvasW.toFloat(), gy, grid); gy += spacing }
        }
        paint.reset(); paint.isAntiAlias = true; paint.color = Color.CYAN; paint.style = Paint.Style.STROKE; paint.strokeWidth = 1.5f / (fit() * zoom)
        selection?.let { c.drawPath(it, paint) }
        if (!usesNativeStroke()) maskGuide?.let { c.drawPath(it, paint) }
        if (tool == "Polygon selection" && polygonVertices.isNotEmpty()) {
            val outline = Path().apply { polygonVertices.forEachIndexed { i, p -> if (i == 0) moveTo(p.x, p.y) else lineTo(p.x, p.y) }; if (polygonVertices.size >= 3) close() }
            c.drawPath(outline, paint)
            polygonVertices.forEach { c.drawCircle(it.x, it.y, 4f / (fit() * zoom), paint) }
        }
        if (tool == "Clone source" || tool == "Clone stamp") cloneSource?.let { source ->
            val arm = 10f / (fit() * zoom)
            c.drawLine(source.x - arm, source.y, source.x + arm, source.y, paint)
            c.drawLine(source.x, source.y - arm, source.x, source.y + arm, paint)
        }
        selectionDraft?.let { if (tool == "Ellipse selection") c.drawOval(it, paint) else c.drawRect(it, paint) }
        if (tool == "Lasso") stroke?.let { c.drawPath(it, paint) }
        if (tool == "Shapes") shapes.guides(c, fit() * zoom)
        if (transformMode != null) {
            val points = if (transformMode == "Mesh") vertices else if (transformMode == "Perspective") corners else floatArrayOf(bounds.left, bounds.top, bounds.right, bounds.top, bounds.right, bounds.bottom, bounds.left, bounds.bottom).also { normalMatrix().mapPoints(it) }
            for (i in points.indices step 2) c.drawCircle(points[i], points[i + 1], 7f / (fit() * zoom), paint)
            if (transformMode == "Mesh") {
                val n = meshDivisions + 1
                for (y in 0 until n) for (x in 0 until n) {
                    val i = (y * n + x) * 2
                    if (x < n - 1) c.drawLine(vertices[i], vertices[i + 1], vertices[i + 2], vertices[i + 3], paint)
                    if (y < n - 1) c.drawLine(vertices[i], vertices[i + 1], vertices[i + n * 2], vertices[i + n * 2 + 1], paint)
                }
            }
        }
        c.restore()
    }
    private fun drawAssistantGuides(c: Canvas) {
        val guide = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.argb(150, 255, 200, 80); style = Paint.Style.STROKE; strokeWidth = 1f / (fit() * zoom) }
        val ox = canvasW * assistantOriginX; val oy = canvasH * assistantOriginY
        val diagonal = hypot(canvasW.toFloat(), canvasH.toFloat())
        val angle = Math.toRadians(assistantAngle.toDouble()); val ux = cos(angle).toFloat(); val uy = sin(angle).toFloat()
        fun line(x: Float, y: Float, vx: Float = ux, vy: Float = uy) { c.drawLine(x - vx * diagonal, y - vy * diagonal, x + vx * diagonal, y + vy * diagonal, guide) }
        when (assistantMode) {
            1u -> line(ox, oy)
            2u -> { line(ox, oy); assistantAnchor?.let { line(it.x, it.y) } }
            3u -> {
                val step = assistantSpacing.coerceAtLeast(8f); val count = ceil(diagonal / step).toInt().coerceAtMost(256)
                for (i in -count..count) { val offset = i * step; line(ox - uy * offset, oy + ux * offset); line(ox + ux * offset, oy + uy * offset, -uy, ux) }
            }
            4u -> {
                for (i in 0 until 12) { val a = i * Math.PI / 12; line(ox, oy, cos(a).toFloat(), sin(a).toFloat()) }
                assistantAnchor?.let { val dx = it.x - ox; val dy = it.y - oy; val length = hypot(dx, dy); if (length > .001f) line(ox, oy, dx / length, dy / length) }
            }
            5u -> {
                val step = assistantSpacing.coerceAtLeast(8f); var radius = step; var count = 0
                while (radius <= diagonal && count++ < 256) { c.drawCircle(ox, oy, radius, guide); radius += step }
                assistantAnchor?.let { c.drawCircle(ox, oy, hypot(it.x - ox, it.y - oy).coerceAtLeast(1f), guide) }
            }
        }
        val arm = 8f / (fit() * zoom); c.drawLine(ox - arm, oy, ox + arm, oy, guide); c.drawLine(ox, oy - arm, ox, oy + arm, guide)
    }
    override fun onDraw(canvas: Canvas) { drawScene(canvas); drawPenCursor(canvas); backdropObservers.forEach { it.postInvalidateOnAnimation() } }
    /** Android flattens its UI path; coverage generation and multisample resolve are Rust. */
    private fun drawNativeCoverage(c: Canvas, path: Path, width: Float, strength: Float): Boolean {
        val key = CoverageKey(path, width)
        coverageMasks.get(key)?.let { mask ->
            val p = Paint().apply { color = inkColor; alpha = (strength * Color.alpha(inkColor)).toInt().coerceIn(0,255)
                if (eraserStroke) { alpha = (strength * 255).toInt().coerceIn(0,255); xfermode = PorterDuffXfermode(PorterDuff.Mode.DST_OUT) } }
            c.drawBitmap(mask.bitmap, mask.x, mask.y, p)
            return true
        }
        val measure = PathMeasure(path, false)
        val points = ArrayList<DrawingStrokePoint>()
        val position = FloatArray(2)
        val step = (width / 3f).coerceAtLeast(.5f)
        do {
            if (measure.length / step > 4096) return false
            var distance = 0f
            while (distance < measure.length) {
                measure.getPosTan(distance, position, null)
                points.add(DrawingStrokePoint(position[0], position[1], width / 2f)); distance += step
            }
            measure.getPosTan(measure.length, position, null)
            points.add(DrawingStrokePoint(position[0], position[1], width / 2f))
        } while (measure.nextContour())
        val tile = drawingStrokeCoverage(points, canvasW.toUInt(), canvasH.toUInt()) ?: return false
        val mask = Bitmap.createBitmap(tile.width.toInt(), tile.height.toInt(), Bitmap.Config.ALPHA_8)
        try {
            val data = java.nio.ByteBuffer.allocate(mask.rowBytes * mask.height)
            for (row in 0 until mask.height) { data.position(row * mask.rowBytes); data.put(tile.alpha, row * mask.width, mask.width) }
            data.rewind(); mask.copyPixelsFromBuffer(data)
            val p = Paint(Paint.ANTI_ALIAS_FLAG).apply {
                color = inkColor; alpha = (strength * Color.alpha(inkColor)).toInt().coerceIn(0, 255)
                if (eraserStroke) { alpha = (strength * 255).toInt().coerceIn(0, 255); xfermode = PorterDuffXfermode(PorterDuff.Mode.DST_OUT) }
            }
            c.drawBitmap(mask, tile.left.toFloat(), tile.top.toFloat(), p)
        } catch (e: Exception) { mask.recycle(); throw e }
        coverageMasks.put(key, CoverageTile(mask, tile.left.toFloat(), tile.top.toFloat()))
        return true
    }

    private fun drawStroke(c: Canvas, path: Path) {
        val alphaSave = if (layerAlphaLocked(layer)) c.saveLayer(0f, 0f, canvasW.toFloat(), canvasH.toFloat(), Paint().apply { xfermode = PorterDuffXfermode(PorterDuff.Mode.SRC_ATOP) }) else -1
        drawStrokeOnce(c, path)
        for (values in drawingSymmetryMatrices(canvasW.toFloat(), canvasH.toFloat(), symmetryMode)) {
            c.save(); selection?.let { c.clipPath(it) }
            c.concat(Matrix().apply { setValues(values.toFloatArray()) })
            drawStrokeOnce(c, path, clipSelection = false); c.restore()
        }
        if (alphaSave >= 0) c.restoreToCount(alphaSave)
    }
    private fun drawStrokeOnce(c: Canvas, path: Path, clipSelection: Boolean = true) {
        c.save(); if (clipSelection) selection?.let { c.clipPath(it) }
        fun draw(path: Path, pressure: Float) {
            val width = brushSize * if (pressureStroke) .1f + .9f * sqrt(pressure) else 1f
            val strength = opacity * if (pressureStroke) .45f + .55f * pressure else 1f
            if ((eraserStroke || brush == "Pen") && drawNativeCoverage(c, path, if (eraserStroke) width else width * .225f, strength)) return
            if (eraserStroke) {
                paint.reset(); paint.isAntiAlias = true; paint.style = Paint.Style.STROKE; paint.strokeCap = Paint.Cap.ROUND; paint.strokeJoin = Paint.Join.ROUND
                paint.strokeWidth = width; paint.alpha = (strength * 255).toInt(); paint.xfermode = PorterDuffXfermode(PorterDuff.Mode.DST_OUT)
                c.drawPath(path, paint)
            } else DrawingBrushes.draw(c, path, brush, inkColor, width, strength)
        }
        if (pressureStroke) pressureSegments.forEach { draw(it.path, it.pressure) } else draw(path, 1f)
        c.restore()
    }

    private fun filterStamp(x: Float, y: Float) {
        val b = images[layer]; val radius = (brushSize / 2).toInt().coerceAtLeast(2)
        val left = (x.toInt() - radius).coerceIn(0, canvasW - 1); val top = (y.toInt() - radius).coerceIn(0, canvasH - 1)
        val w = (radius * 2).coerceAtMost(canvasW - left); val h = (radius * 2).coerceAtMost(canvasH - top)
        val patch = Bitmap.createBitmap(b, left, top, w, h)
        val c = Canvas(b); c.save(); selection?.let { c.clipPath(it) }; c.clipPath(Path().apply { addCircle(x, y, radius.toFloat(), Path.Direction.CW) })
        paint.reset(); paint.isFilterBitmap = true; paint.alpha = (opacity * 180).toInt()
        if (tool == "Blur") {
            val small = Bitmap.createScaledBitmap(patch, (w / 6).coerceAtLeast(1), (h / 6).coerceAtLeast(1), true)
            c.drawBitmap(small, null, Rect(left, top, left + w, top + h), paint); if (small !== patch) small.recycle()
        } else c.drawBitmap(patch, left + (x - lastX), top + (y - lastY), paint)
        c.restore(); if (patch !== b) patch.recycle()
    }
    private fun addMaskPoint(e: MotionEvent, x: Float, y: Float, historicalPressure: Float? = null) {
        if (maskPoints.size >= 8192) { maskStrokeOverflow = true; return }
        val pen = e.getToolType(0) == MotionEvent.TOOL_TYPE_STYLUS || e.getToolType(0) == MotionEvent.TOOL_TYPE_ERASER
        val previous = maskPoints.lastOrNull()
        val smooth = drawingStrokeSamples(previous?.x ?: x, previous?.y ?: y, listOf(DrawingInput(x, y, if (usePressure && pen) historicalPressure ?: penPressure(e) else 1f)), stabilization, pressureCurve).firstOrNull() ?: return
        val sample = assisted(smooth)
        val radius = brushSize / 2 * if (usePressure && pen) .1f + .9f * sqrt(sample.pressure) else 1f
        maskPoints.add(DrawingStrokePoint(sample.x, sample.y, radius))
        if (maskGuide == null) maskGuide = Path().apply { moveTo(sample.x, sample.y); lineTo(sample.x + .01f, sample.y) } else maskGuide?.lineTo(sample.x, sample.y)
    }
    private fun commitMaskStroke(reveal: Boolean) {
        val points = maskPoints.toList(); maskPoints.clear(); maskGuide = null
        if (maskStrokeOverflow) { android.widget.Toast.makeText(context, "Mask stroke is too long; use shorter strokes.", android.widget.Toast.LENGTH_LONG).show(); changed(); return }
        if (points.isEmpty() || !hasLayerMask(layer)) return
        val input = properties(images[layer]).mask; val selected = selectionBytes(); val w = canvasW.toUInt(); val h = canvasH.toUInt(); val strength = opacity
        operationBusy = true
        maskScope.launch {
            try {
                val output = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) { drawingPaintLayerMask(input, w, h, points, if (reveal) 255.toUByte() else 0.toUByte(), strength, selected) }
                setMask(output)
            } catch (e: kotlinx.coroutines.CancellationException) { throw e }
            catch (e: Exception) { android.widget.Toast.makeText(context, e.message ?: "Mask stroke failed", android.widget.Toast.LENGTH_LONG).show() }
            finally { operationBusy = false; changed() }
        }
    }
    private fun usesNativeStroke() = nativeBrushEnabled && tool in listOf("Brush", "Eraser")
    private fun drawNativeBrushPreview(c: Canvas) {
        if (!usesNativeStroke() || nativePreviewFailed || (maskPoints.isEmpty() && !operationBusy)) return
        val settings = nativeStrokeSettings ?: return
        try {
            if (maskPoints.isNotEmpty() && nativePreviewPointCount != maskPoints.size && nativePreviewJob == null) {
                val points = maskPoints.toList(); val w = canvasW.toUInt(); val h = canvasH.toUInt()
                nativePreviewJob = maskScope.launch {
                    try {
                        val coverage = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) { drawingNativeBrushCoverage(points, w, h, settings) }
                        if (nativeStrokeSettings === settings && (maskPoints.isNotEmpty() || operationBusy)) {
                            val mask = if (coverage.width == 0u || coverage.height == 0u) null else Bitmap.createBitmap(coverage.width.toInt(), coverage.height.toInt(), Bitmap.Config.ALPHA_8).also { bitmap ->
                                val bytes = java.nio.ByteBuffer.allocate(bitmap.rowBytes * bitmap.height)
                                for (row in 0 until bitmap.height) { bytes.position(row * bitmap.rowBytes); bytes.put(coverage.alpha, row * bitmap.width, bitmap.width) }
                                bytes.rewind(); bitmap.copyPixelsFromBuffer(bytes)
                            }
                            clearNativePreview()
                            nativePreview = mask?.let { CoverageTile(it, coverage.left.toFloat(), coverage.top.toFloat()) }
                            nativePreviewPointCount = points.size
                        }
                    } catch (e: kotlinx.coroutines.CancellationException) { throw e }
                    catch (e: Exception) {
                        if (nativeStrokeSettings === settings) {
                            nativePreviewFailed = true
                            android.widget.Toast.makeText(context, e.message ?: "Native GPU preview unavailable", android.widget.Toast.LENGTH_LONG).show()
                        }
                    } finally { nativePreviewJob = null; invalidate() }
                }
            }
            val tile = nativePreview ?: return
            if (nativeStrokeErase && layerAlphaLocked(layer)) return
            val saved = c.save(); selection?.let { c.clipPath(it) }
            val alphaSave = if (layerAlphaLocked(layer)) c.saveLayer(0f, 0f, canvasW.toFloat(), canvasH.toFloat(), Paint().apply { xfermode = PorterDuffXfermode(PorterDuff.Mode.SRC_ATOP) }) else -1
            c.drawBitmap(tile.bitmap, tile.x, tile.y, Paint().apply { color = nativeStrokeColor; alpha = (nativeStrokeOpacity * if (nativeStrokeErase) 255 else Color.alpha(nativeStrokeColor)).toInt().coerceIn(0, 255); if (nativeStrokeErase) xfermode = PorterDuffXfermode(PorterDuff.Mode.DST_OUT) })
            if (alphaSave >= 0) c.restoreToCount(alphaSave)
            c.restoreToCount(saved)
        } catch (e: Exception) {
            nativePreviewFailed = true
            android.widget.Toast.makeText(context, e.message ?: "Native GPU preview unavailable", android.widget.Toast.LENGTH_LONG).show()
        }
    }
    private fun commitNativeBrushStroke() {
        val points = maskPoints.toList(); maskPoints.clear(); maskGuide = null
        val settings = nativeStrokeSettings ?: return
        if (points.isEmpty() || maskStrokeOverflow) {
            nativeStrokeSettings = null; clearNativePreview()
            if (maskStrokeOverflow) android.widget.Toast.makeText(context, "Native brush stroke exceeds the point limit; use shorter strokes.", android.widget.Toast.LENGTH_LONG).show()
            changed(); return
        }
        val bitmap = images[layer]; val index = layer
        val existing = nativeBacking(bitmap)
        val input = if (existing == null) rgba(bitmap) else null
        val selected = selectionBytes(); val w = canvasW.toUInt(); val h = canvasH.toUInt()
        val strength = nativeStrokeOpacity; val erase = nativeStrokeErase; val alphaLocked = layerAlphaLocked(layer)
        val ink = nativeStrokeColor
        val otherNativeBytes = images.filter { it !== bitmap }.sumOf { nativeBacking(it)?.info()?.liveBytes?.toLong() ?: 0L }
        operationBusy = true
        maskScope.launch {
            try {
                val result = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) {
                    val native = if (existing != null) existing.fork() else drawingImportTiled(DrawingDocumentSnapshot(w, h, 0u,
                        listOf(DrawingRasterLayer("Paint", true, 1f, input!!, 0u, false, false, false, ByteArray(0), true))))
                    val coverage = drawingNativeBrushCoverage(points, w, h, settings)
                    val clipped = if (selected.isEmpty()) ByteArray(0) else ByteArray(coverage.width.toInt() * coverage.height.toInt()) { i ->
                        val x = i % coverage.width.toInt() + coverage.left.toInt()
                        val y = i / coverage.width.toInt() + coverage.top.toInt()
                        selected[y * w.toInt() + x]
                    }
                    fun linear(value: Int): Float {
                        val v = value / 255f
                        return if (v <= .04045f) v / 12.92f else ((v + .055f) / 1.055f).pow(2.4f)
                    }
                    native.paintCoverage(1u, coverage, listOf(linear(Color.red(ink)), linear(Color.green(ink)), linear(Color.blue(ink)), if (erase) 1f else Color.alpha(ink) / 255f), strength, clipped, erase, alphaLocked)
                    native.discardHistory()
                    check(otherNativeBytes + native.info().liveBytes.toLong() <= 96L * 1024 * 1024) { "Native paint layers exceed the 96 MiB live-tile budget." }
                    native to native.layerSrgb(1u)
                }
                check(index == layer && images[index] === bitmap) { "The drawing layer changed during the stroke." }
                installNativePixels(bitmap, result.first, result.second)
            } catch (e: kotlinx.coroutines.CancellationException) { throw e }
            catch (e: Exception) { android.widget.Toast.makeText(context, e.message ?: "Native GPU brush failed; no pixels committed", android.widget.Toast.LENGTH_LONG).show() }
            finally { clearNativePreview(); nativeStrokeSettings = null; operationBusy = false; changed() }
        }
    }
    private fun commitRetouchStroke(operation: String) {
        val points = maskPoints.toList(); maskPoints.clear(); maskGuide = null
        if (points.isEmpty()) return
        if (maskStrokeOverflow) { android.widget.Toast.makeText(context, "Retouch stroke is too long; use shorter strokes.", android.widget.Toast.LENGTH_LONG).show(); changed(); return }
        val input = rgba(images[layer]); val selection = selectionBytes(); val w = canvasW.toUInt(); val h = canvasH.toUInt()
        val source = if (operation == "Clone stamp") {
            if (cloneSampleMerged) visibleSample().let { bitmap -> try { rgba(bitmap) } finally { bitmap.recycle() } } else input
        } else ByteArray(0)
        val delta = cloneOffset
        val offset = if (delta == null) emptyList() else listOf(delta.x, delta.y)
        val color = byteArrayOf(Color.red(inkColor).toByte(), Color.green(inkColor).toByte(), Color.blue(inkColor).toByte())
        val strength = opacity; val range = retouchTonalRange
        operationBusy = true
        maskScope.launch {
            try {
                val output = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) { drawingRetouchStroke(input, source, w, h, points, operation, offset, color, strength, selection, range) }
                replacePixels(output, clipToSelection = false)
            } catch (e: kotlinx.coroutines.CancellationException) { throw e }
            catch (e: Exception) { android.widget.Toast.makeText(context, e.message ?: "Retouch stroke failed", android.widget.Toast.LENGTH_LONG).show() }
            finally { operationBusy = false; changed() }
        }
    }
    private fun pickColor(x: Int, y: Int) {
        val image = pickerImage ?: return
        val radius = pickerRadius.coerceIn(0, 32)
        val left = (x - radius).coerceIn(0, canvasW - 1); val right = (x + radius + 1).coerceIn(left + 1, canvasW)
        val top = (y - radius).coerceIn(0, canvasH - 1); val bottom = (y + radius + 1).coerceIn(top + 1, canvasH)
        val w = right - left; val h = bottom - top
        val colors = IntArray(w * h); image.getPixels(colors, 0, w, left, top, w, h)
        val bytes = ByteArray(colors.size * 4)
        colors.forEachIndexed { i, c -> bytes[i*4] = Color.red(c).toByte(); bytes[i*4+1] = Color.green(c).toByte(); bytes[i*4+2] = Color.blue(c).toByte(); bytes[i*4+3] = Color.alpha(c).toByte() }
        val color = drawingAverageColor(bytes, w.toUInt(), h.toUInt())
        inkColor = Color.argb(color[3].toInt() and 255, color[0].toInt() and 255, color[1].toInt() and 255, color[2].toInt() and 255)
        onColorPicked(inkColor); changed()
    }
    private fun wand(x: Int, y: Int) {
        val merged = if (currentLayerOnly) images[layer] else visibleSample()
        try {
            val mask = drawingColorSelection(rgba(merged), canvasW.toUInt(), canvasH.toUInt(), x.toUInt(), y.toUInt(), wandTolerance.coerceIn(0, 255).toUByte(), wandContiguous)
            mergeSelection(maskPath(mask), selectionMode)
        } catch (e: Exception) {
            android.widget.Toast.makeText(context, e.message ?: "Selection failed", android.widget.Toast.LENGTH_LONG).show()
        } finally { if (!currentLayerOnly) merged.recycle() }
    }
    override fun onTouchEvent(e: MotionEvent): Boolean {
        if (!isEnabled || operationBusy || animationPlaying) return true
        // The current path is mutable; immutable pressure segments can keep cached coverage.
        stroke?.let { path -> coverageMasks.snapshot().keys.filter { it.path === path }.forEach { coverageMasks.remove(it) } }
        updatePenCursor(e)
        if (e.actionMasked == MotionEvent.ACTION_CANCEL) penCursor = null
        parent?.requestDisallowInterceptTouchEvent(true)
        if (e.pointerCount > 1) {
            if (!navigating && tool == "Polygon selection" && polygonContactAdded && polygonVertices.isNotEmpty()) polygonVertices.removeAt(polygonVertices.lastIndex)
            polygonContactAdded = false
            maskPoints.clear(); maskGuide = null; clearNativePreview(); nativeStrokeSettings = null
            selectionStart = null; selectionDraft = null; pickerImage?.recycle(); pickerImage = null
            shapeTouchActive = false
            if (tool == "Shapes" && !navigating) shapes.cancelGesture()
            if (!navigating) { stroke = null; strokeBefore?.let { images[layer].recycle(); images[layer] = it }; strokeBefore = null; navigating = true; oldSpan = 0f }
            val x = (e.getX(0) + e.getX(1)) / 2; val y = (e.getY(0) + e.getY(1)) / 2
            val span = hypot(e.getX(1) - e.getX(0), e.getY(1) - e.getY(0)); val a = Math.toDegrees(atan2(e.getY(1) - e.getY(0), e.getX(1) - e.getX(0)).toDouble()).toFloat()
            if (oldSpan > 0 && e.actionMasked == MotionEvent.ACTION_MOVE) {
                val anchor = point(oldX, oldY); zoom = (zoom * span / oldSpan).coerceIn(.25f, 8f); angle += ((a - oldAngle + 540) % 360) - 180
                contentMatrix().mapPoints(anchor)
                dx = x - left() - anchor[0]; dy = y - top() - anchor[1]
            }
            oldSpan = span; oldAngle = a; oldX = x; oldY = y; invalidate(); return true
        }
        if (navigating) { if (e.actionMasked == MotionEvent.ACTION_UP || e.actionMasked == MotionEvent.ACTION_CANCEL) navigating = false; return true }
        val p = point(e.x, e.y)
        val constrained = tool in listOf("Brush", "Eraser", "Smudge", "Blur", "Reveal mask", "Hide mask", "Clone stamp", "Dodge", "Burn", "Desaturate", "Colorize")
        if (e.actionMasked == MotionEvent.ACTION_DOWN) assistantAnchor = DrawingInput(p[0], p[1], 1f)
        if (tool in listOf("Brush", "Eraser", "Smudge", "Blur") && !usesNativeStroke() && assistantMode != 0u && e.actionMasked == MotionEvent.ACTION_DOWN) {
            val snapped = assisted(DrawingInput(p[0], p[1], 1f)); p[0] = snapped.x; p[1] = snapped.y
        }
        val x = p[0]; val y = p[1]
        if (layerLocked(layer) && tool in listOf("Brush", "Eraser", "Smudge", "Blur", "Fill", "Shapes")) return true
        if (transformMode != null) {
            if (e.actionMasked == MotionEvent.ACTION_DOWN) {
                lastX = x; lastY = y
                val points = if (transformMode == "Mesh") vertices else corners
                handle = points.indices.step(2).minByOrNull { hypot(points[it] - x, points[it + 1] - y) } ?: -1
                if (handle >= 0 && hypot(points[handle] - x, points[handle + 1] - y) > 40 / (fit() * zoom)) handle = -1
            } else if (e.actionMasked == MotionEvent.ACTION_MOVE) {
                if (transformMode == "Normal") { moveX += x - lastX; moveY += y - lastY }
                else if (handle >= 0) { val points = if (transformMode == "Mesh") vertices else corners; points[handle] = x; points[handle + 1] = y }
                lastX = x; lastY = y; invalidate()
            }
            return true
        }
        if (usesNativeStroke()) {
            if (layerLocked(layer)) return true
            when (e.actionMasked) {
                MotionEvent.ACTION_DOWN -> {
                    maskPoints.clear(); maskGuide = null; maskStrokeOverflow = false; clearNativePreview(); nativePreviewFailed = false
                    nativeStrokeSettings = if (tool == "Eraser") nativeBrushSettings.copy(tip = 0u, spacing = .15f, hardness = 1f, aspect = 1f, scatter = 0f, sizeJitter = 0f, rotationJitter = 0f, flow = 1f, followDirection = false, symmetry = symmetryMode) else nativeBrushSettings.copy(symmetry = symmetryMode)
                    nativeStrokeColor = inkColor; nativeStrokeOpacity = opacity; nativeStrokeErase = tool == "Eraser" || e.getToolType(0) == MotionEvent.TOOL_TYPE_ERASER
                    addMaskPoint(e, x, y)
                }
                MotionEvent.ACTION_MOVE -> if (maskPoints.isNotEmpty()) {
                    for (i in 0 until e.historySize) {
                        val historical = point(e.getHistoricalX(i), e.getHistoricalY(i))
                        addMaskPoint(e, historical[0], historical[1], penPressure(e, i))
                    }
                    addMaskPoint(e, x, y)
                }
                MotionEvent.ACTION_UP -> if (maskPoints.isNotEmpty()) { addMaskPoint(e, x, y); commitNativeBrushStroke() }
                MotionEvent.ACTION_CANCEL -> { maskPoints.clear(); maskGuide = null; clearNativePreview(); nativeStrokeSettings = null }
            }
            changed(); return true
        }
        if (tool == "Polygon selection") {
            if (e.actionMasked == MotionEvent.ACTION_DOWN && polygonVertices.size < 128) { polygonVertices.add(PointF(x, y)); polygonContactAdded = true; changed() }
            if (e.actionMasked == MotionEvent.ACTION_UP) polygonContactAdded = false
            if (e.actionMasked == MotionEvent.ACTION_CANCEL && polygonContactAdded) { if (polygonVertices.isNotEmpty()) polygonVertices.removeAt(polygonVertices.lastIndex); polygonContactAdded = false; changed() }
            return true
        }
        if (tool == "Assistant origin") {
            if (e.actionMasked == MotionEvent.ACTION_DOWN) {
                assistantOriginX = (x / canvasW).coerceIn(0f, 1f); assistantOriginY = (y / canvasH).coerceIn(0f, 1f); assistantAnchor = null; changed()
            }
            return true
        }
        if (tool == "Clone source") {
            if (e.actionMasked == MotionEvent.ACTION_DOWN) { cloneSource = PointF(x.coerceIn(0f, canvasW - 1f), y.coerceIn(0f, canvasH - 1f)); cloneOffset = null; changed() }
            return true
        }
        if (tool in listOf("Clone stamp", "Dodge", "Burn", "Desaturate", "Colorize")) {
            if (layerLocked(layer)) return true
            when (e.actionMasked) {
                MotionEvent.ACTION_DOWN -> {
                    maskPoints.clear(); maskGuide = null; maskStrokeOverflow = false
                    if (tool == "Clone stamp") {
                        val source = cloneSource
                        if (source == null) { android.widget.Toast.makeText(context, "Choose a clone source in Retouch first.", android.widget.Toast.LENGTH_LONG).show(); return true }
                        if (!cloneAligned || cloneOffset == null) {
                            val target = assisted(DrawingInput(x, y, 1f)); cloneOffset = PointF(source.x - target.x, source.y - target.y)
                        }
                    }
                    addMaskPoint(e, x, y)
                }
                MotionEvent.ACTION_MOVE -> if (maskPoints.isNotEmpty()) addMaskPoint(e, x, y)
                MotionEvent.ACTION_UP -> if (maskPoints.isNotEmpty()) { addMaskPoint(e, x, y); commitRetouchStroke(tool) }
                MotionEvent.ACTION_CANCEL -> { maskPoints.clear(); maskGuide = null }
            }
            changed(); return true
        }
        if (tool == "Reveal mask" || tool == "Hide mask") {
            if (layerLocked(layer) || !hasLayerMask(layer)) return true
            when (e.actionMasked) {
                MotionEvent.ACTION_DOWN -> { maskPoints.clear(); maskGuide = null; maskStrokeOverflow = false; addMaskPoint(e, x, y) }
                MotionEvent.ACTION_MOVE -> addMaskPoint(e, x, y)
                MotionEvent.ACTION_UP -> { addMaskPoint(e, x, y); commitMaskStroke(tool == "Reveal mask") }
                MotionEvent.ACTION_CANCEL -> { maskPoints.clear(); maskGuide = null }
            }
            changed(); return true
        }
        if (tool == "Color picker") {
            when (e.actionMasked) {
                MotionEvent.ACTION_DOWN -> {
                    pickerImage?.recycle(); pickerImage = if (currentLayerOnly) copy(images[layer]) else visibleSample()
                    pickColor(x.toInt().coerceIn(0, canvasW - 1), y.toInt().coerceIn(0, canvasH - 1))
                }
                MotionEvent.ACTION_MOVE -> pickColor(x.toInt().coerceIn(0, canvasW - 1), y.toInt().coerceIn(0, canvasH - 1))
                MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> { pickerImage?.recycle(); pickerImage = null }
            }
            return true
        }
        if (tool == "Rectangle selection" || tool == "Ellipse selection") {
            when (e.actionMasked) {
                MotionEvent.ACTION_DOWN -> { selectionStart = PointF(x, y); selectionDraft = RectF(x, y, x, y) }
                MotionEvent.ACTION_MOVE -> selectionStart?.let { start -> selectionDraft = RectF(min(start.x, x), min(start.y, y), max(start.x, x), max(start.y, y)) }
                MotionEvent.ACTION_UP -> {
                    selectionStart?.let { start ->
                        try { mergeSelection(maskPath(drawingGeometricSelection(canvasW.toUInt(), canvasH.toUInt(), listOf(start.x, start.y), listOf(x, y), tool == "Ellipse selection")), selectionMode) }
                        catch (e: Exception) { android.widget.Toast.makeText(context, e.message ?: "Selection failed", android.widget.Toast.LENGTH_LONG).show() }
                    }
                    selectionStart = null; selectionDraft = null
                }
                MotionEvent.ACTION_CANCEL -> { selectionStart = null; selectionDraft = null }
            }
            changed(); return true
        }
        if (tool == "Shapes") {
            when (e.actionMasked) {
                MotionEvent.ACTION_DOWN -> { shapeTouchActive = x >= 0 && x < canvasW && y >= 0 && y < canvasH; if (shapeTouchActive) shapes.down(x, y, 24f / (fit() * zoom)) }
                MotionEvent.ACTION_MOVE -> if (shapeTouchActive) shapes.drag(x, y)
                MotionEvent.ACTION_UP -> { if (shapeTouchActive) { shapes.drag(x, y); if (!shapes.multi) commitShapeDraft(); shapes.endGesture() }; shapeTouchActive = false }
                MotionEvent.ACTION_CANCEL -> { shapeTouchActive = false; shapes.cancelGesture() }
            }
            changed(); return true
        }
        when (e.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                if (x < 0 || x >= canvasW || y < 0 || y >= canvasH) return true
                lastX = x; lastY = y
                if (tool == "Magic wand") { wand(x.toInt(), y.toInt()); return true }
                if (tool == "Fill") {
                    try { fill(x.toInt(), y.toInt()) }
                    catch (e: Exception) { android.widget.Toast.makeText(context, e.message ?: "Fill failed", android.widget.Toast.LENGTH_LONG).show() }
                    return true
                }
                strokeBefore = if (tool == "Lasso") null else copy(images[layer])
                val pen = e.getToolType(0) == MotionEvent.TOOL_TYPE_STYLUS || e.getToolType(0) == MotionEvent.TOOL_TYPE_ERASER
                pressureStroke = usePressure && pen && (tool == "Brush" || tool == "Eraser")
                eraserStroke = tool == "Eraser" || e.getToolType(0) == MotionEvent.TOOL_TYPE_ERASER
                pressureSegments.clear()
                lastPressure = if (pressureStroke) drawingStrokeSamples(x, y, listOf(DrawingInput(x, y, penPressure(e))), 0f, pressureCurve).firstOrNull()?.pressure ?: 1f else 1f
                midX = x; midY = y
                if (pressureStroke) pressureSegments.add(PressureSegment(Path().apply { moveTo(x, y); lineTo(x + .01f, y) }, lastPressure))
                stroke = Path().apply { moveTo(x, y); lineTo(x + .01f, y) }
            }
            MotionEvent.ACTION_MOVE -> if (stroke != null) {
                for (i in 0..e.historySize) {
                    val raw = if (i == e.historySize) p else point(e.getHistoricalX(i), e.getHistoricalY(i))
                    val rawPressure = if (pressureStroke) penPressure(e, if (i == e.historySize) null else i) else 1f
                    val sample = if (tool == "Brush" || tool == "Eraser") drawingStrokeSamples(
                        lastX, lastY, listOf(DrawingInput(raw[0], raw[1], rawPressure)), stabilization, pressureCurve,
                    ).firstOrNull() else null
                    val input = sample ?: DrawingInput(raw[0], raw[1], rawPressure)
                    val snapped = if (constrained && assistantMode != 0u) assisted(input) else input
                    val q = floatArrayOf(snapped.x, snapped.y)
                    if (tool == "Smudge" || tool == "Blur") {
                        val distance = hypot(q[0] - lastX, q[1] - lastY)
                        val count = ceil(distance / (brushSize / 4).coerceAtLeast(1f)).toInt().coerceIn(1, 256)
                        val startX = lastX; val startY = lastY
                        for (step in 1..count) {
                            val xx = startX + (q[0] - startX) * step / count; val yy = startY + (q[1] - startY) * step / count
                            filterStamp(xx, yy); lastX = xx; lastY = yy
                        }
                    }
                    if (pressureStroke) {
                        val pressure = sample?.pressure ?: rawPressure
                        val mx = (lastX + q[0]) / 2; val my = (lastY + q[1]) / 2
                        pressureSegments.add(PressureSegment(Path().apply { moveTo(midX, midY); quadTo(lastX, lastY, mx, my) }, (lastPressure + pressure) / 2))
                        midX = mx; midY = my; lastPressure = pressure
                    }
                    stroke!!.quadTo(lastX, lastY, (lastX + q[0]) / 2, (lastY + q[1]) / 2); lastX = q[0]; lastY = q[1]
                }
                // Keep the existing selection while previewing an additive/subtractive lasso.
            }
            MotionEvent.ACTION_UP -> {
                stroke?.let { path ->
                    val endpoint = if (constrained && assistantMode != 0u) assisted(DrawingInput(x, y, lastPressure)) else DrawingInput(x, y, lastPressure)
                    path.lineTo(endpoint.x, endpoint.y)
                    if (pressureStroke) pressureSegments.add(PressureSegment(Path().apply { moveTo(midX, midY); quadTo(lastX, lastY, endpoint.x, endpoint.y) }, lastPressure))
                    if (tool == "Lasso") mergeSelection(Path(path).apply { close() }, selectionMode)
                    else { if (tool == "Brush" || tool == "Eraser") drawStroke(Canvas(images[layer]), path); strokeBefore?.let { record(it) } }
                }
                stroke = null; strokeBefore = null; onEdit()
            }
            MotionEvent.ACTION_CANCEL -> { stroke = null; strokeBefore?.let { images[layer].recycle(); images[layer] = it }; strokeBefore = null }
        }
        invalidate(); return true
    }
    fun dispose() {
        stopAnimation(); previousOnion = null; nextOnion = null
        maskScope.cancel(); maskPoints.clear(); maskGuide = null; clearNativePreview(); nativeStrokeSettings = null
        penCursor = null
        pickerImage?.recycle(); pickerImage = null
        liveLayer?.recycle(); liveLayer = null; coverageMasks.evictAll(); maskTiles.evictAll()
        images.forEach { it.recycle() }; disposeEdits(history); disposeEdits(future); clipboard?.recycle(); source?.recycle(); original?.recycle(); meshSource?.recycle(); strokeBefore?.recycle()
        nativeBackings.clear()
    }
}
