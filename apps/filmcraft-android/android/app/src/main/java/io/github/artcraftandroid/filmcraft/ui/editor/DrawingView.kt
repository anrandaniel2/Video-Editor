package io.github.artcraftandroid.filmcraft.ui.editor

import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.runtime.mutableStateOf
import android.content.Context
import android.graphics.*
import android.view.MotionEvent
import android.view.View
import kotlin.math.*

/** Editable raster layers, selection masks and transactional layer/selection transforms. */
internal class DrawingView(context: Context, val backgroundFrame: Bitmap?, val canvasW: Int, val canvasH: Int) : View(context) {
    var onEdit: () -> Unit = {}
    val backdropObservers = mutableSetOf<View>()
    private val images = mutableListOf(blank())
    private data class Edit(val layer: Int, val before: Bitmap?)
    private val history = ArrayDeque<Edit>()
    private val future = ArrayDeque<Edit>()
    val canUndo get() = (history.isNotEmpty() || shapes.pending) && transformMode == null
    val canRedo get() = future.isNotEmpty() && transformMode == null && !shapes.pending
    var inkColor = Color.BLACK
    var brushSize = 12f
    var opacity = 1f
    var brush = "Pen"
    val shapes = DrawingShapes()
    var tool = "Brush"
        set(value) { if (field != value) shapes.clear(); field = value }
    var layer = 0
        set(value) { shapes.clear(); if (transformMode != null) finishTransform(false); field = value.coerceIn(0, images.lastIndex); clearSelection() }
    val layers get() = images.size
    val canAddLayer get() = images.size < 16 && transformMode == null && (images.size + 1L) * canvasW * canvasH * 4 <= 96L * 1024 * 1024
    var currentLayerOnly = false
    var selectionMode = "Add"
    var wandTolerance = 16
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
    private var normalScale = 1f
    private var moveX = 0f
    private var moveY = 0f
    private var handle = -1
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
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG or Paint.FILTER_BITMAP_FLAG)
    init { setLayerType(LAYER_TYPE_SOFTWARE, null) }
    private fun blank() = Bitmap.createBitmap(canvasW, canvasH, Bitmap.Config.ARGB_8888)
    private fun copy(b: Bitmap) = b.copy(Bitmap.Config.ARGB_8888, true)!!
    private fun fit() = min(width.toFloat() / canvasW, height.toFloat() / canvasH)
    private fun left() = (width - canvasW * fit()) / 2
    private fun top() = (height - canvasH * fit()) / 2
    private fun camera() = Matrix().apply { postScale(fit() * zoom, fit() * zoom); postRotate(angle); postTranslate(left() + dx, top() + dy) }
    private fun point(x: Float, y: Float): FloatArray = floatArrayOf(x, y).also { val inverse = Matrix(); camera().invert(inverse); inverse.mapPoints(it) }
    private fun changed() { invalidate(); onEdit() }
    private fun disposeEdits(edits: ArrayDeque<Edit>) { edits.forEach { it.before?.recycle() }; edits.clear() }
    private fun record(before: Bitmap?, index: Int = layer) {
        disposeEdits(future); history.addLast(Edit(index, before))
        // Bound undo storage to 64 MiB (clipboard/layers are separately bounded).
        while (history.size > 1 && history.sumOf { it.before?.allocationByteCount?.toLong() ?: 0L } > 64L * 1024 * 1024) history.removeFirst().before?.recycle()
    }
    fun undo() { if (shapes.pending) { shapes.removeLast(); changed() } else exchange(history, future) }
    fun changeShape(name: String) { shapes.kind = name; changed() }
    fun connectShape() { shapes.connected = !shapes.connected; changed() }
    fun cancelShapeDraft() { shapes.clear(); changed() }
    private fun shapeStroke(c: Canvas, path: Path) {
        c.save(); selection?.let { c.clipPath(it) }
        val p = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = inkColor; alpha = (opacity * 255).toInt(); style = Paint.Style.STROKE; strokeWidth = brushSize; strokeCap = Paint.Cap.ROUND; strokeJoin = Paint.Join.ROUND }
        c.drawPath(path, p); c.restore()
    }
    fun commitShapeDraft() {
        val path = shapes.path() ?: return
        val before = copy(images[layer]); shapeStroke(Canvas(images[layer]), path); record(before); shapes.clear(); changed()
    }
    fun redo() = exchange(future, history)
    private fun exchange(from: ArrayDeque<Edit>, to: ArrayDeque<Edit>) {
        if (from.isEmpty() || transformMode != null) return
        clearSelection()
        val edit = from.removeLast()
        if (edit.before == null) { to.addLast(Edit(edit.layer, images.removeAt(edit.layer))) }
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
    fun clearSelection() { selection = null; changed() }
    private fun mergeSelection(path: Path, mode: String) {
        val clip = Region(0, 0, canvasW, canvasH)
        val next = Region().apply { setPath(path, clip) }
        val previous = Region().apply { selection?.let { setPath(it, clip) } }
        when (mode) { "Add" -> next.op(previous, Region.Op.UNION); "Subtract" -> { previous.op(next, Region.Op.DIFFERENCE); next.set(previous) } }
        selection = if (next.isEmpty) null else next.boundaryPath
        changed()
    }
    private fun visibleSample(): Bitmap = blank().also { b ->
        val c = Canvas(b); c.drawColor(Color.WHITE)
        backgroundFrame?.let { c.drawBitmap(it, null, Rect(0, 0, canvasW, canvasH), null) }
        images.forEach { c.drawBitmap(it, 0f, 0f, null) }
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
            images.forEach { c.drawBitmap(it, 0f, 0f, null) }
        } else c.drawBitmap(images[layer], 0f, 0f, null)
    }
    fun beginTransform(mode: String, selectedOnly: Boolean) {
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
        transformMode = mode; normalScale = 1f; moveX = 0f; moveY = 0f
        corners = floatArrayOf(bounds.left, bounds.top, bounds.right, bounds.top, bounds.right, bounds.bottom, bounds.left, bounds.bottom)
        resetMesh(meshDivisions); if (mode == "Mesh") prepareMesh(); changed()
    }
    fun setScale(scale: Float) { normalScale = scale; changed() }
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
            val m = Matrix().apply { postScale(normalScale, normalScale, bounds.centerX(), bounds.centerY()); postTranslate(moveX, moveY) }
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
        if (apply && valid) { transformed(Canvas(images[layer])); record(before) }
        else { images[layer].recycle(); images[layer] = before }
        original = null; source?.recycle(); source = null; meshSource?.recycle(); meshSource = null; transformMode = null; selection = null; changed()
    }
    fun bitmap(): Bitmap = blank().also { b -> val c = Canvas(b); images.forEachIndexed { i, image -> c.drawBitmap(image, 0f, 0f, null); if (i == layer && transformMode != null) transformed(c) } }
    fun drawScene(c: Canvas) {
        c.drawColor(Color.rgb(15, 17, 24)); c.save(); c.concat(camera())
        c.clipRect(0, 0, canvasW, canvasH)
        c.drawColor(if (backgroundFrame == null) Color.WHITE else Color.BLACK)
        paint.reset(); paint.isFilterBitmap = true
        backgroundFrame?.let { c.drawBitmap(it, null, Rect(0, 0, canvasW, canvasH), paint) }
        images.forEachIndexed { i, image ->
            val saved = c.saveLayer(0f, 0f, canvasW.toFloat(), canvasH.toFloat(), null)
            c.drawBitmap(image, 0f, 0f, null)
            if (i == layer) {
                if (transformMode != null) transformed(c)
                stroke?.let { path -> if (tool == "Brush" || tool == "Eraser") drawStroke(c, path) }
                if (tool == "Shapes") shapes.path()?.let { shapeStroke(c, it) }
            }
            c.restoreToCount(saved)
        }
        paint.reset(); paint.isAntiAlias = true; paint.color = Color.CYAN; paint.style = Paint.Style.STROKE; paint.strokeWidth = 1.5f / (fit() * zoom)
        selection?.let { c.drawPath(it, paint) }
        if (tool == "Lasso") stroke?.let { c.drawPath(it, paint) }
        if (tool == "Shapes") shapes.guides(c, fit() * zoom)
        if (transformMode != null) {
            val points = if (transformMode == "Mesh") vertices else if (transformMode == "Perspective") corners else floatArrayOf(bounds.left * normalScale + bounds.centerX() * (1 - normalScale) + moveX, bounds.top * normalScale + bounds.centerY() * (1 - normalScale) + moveY, bounds.right * normalScale + bounds.centerX() * (1 - normalScale) + moveX, bounds.bottom * normalScale + bounds.centerY() * (1 - normalScale) + moveY)
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
    override fun onDraw(canvas: Canvas) { drawScene(canvas); backdropObservers.forEach { it.postInvalidateOnAnimation() } }
    private fun drawStroke(c: Canvas, path: Path) {
        c.save(); selection?.let { c.clipPath(it) }
        if (tool == "Eraser") {
            paint.reset(); paint.isAntiAlias = true; paint.style = Paint.Style.STROKE; paint.strokeCap = Paint.Cap.ROUND; paint.strokeJoin = Paint.Join.ROUND
            paint.strokeWidth = brushSize; paint.alpha = (opacity * 255).toInt(); paint.xfermode = PorterDuffXfermode(PorterDuff.Mode.CLEAR)
            c.drawPath(path, paint)
        } else DrawingBrushes.draw(c, path, brush, inkColor, brushSize, opacity)
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
    private fun wand(x: Int, y: Int) {
        val merged = if (currentLayerOnly) images[layer] else visibleSample()
        val pixels = IntArray(canvasW * canvasH); merged.getPixels(pixels, 0, canvasW, 0, 0, canvasW, canvasH)
        if (!currentLayerOnly) merged.recycle()
        val seed = y * canvasW + x; val color = pixels[seed]; val seen = BooleanArray(pixels.size); val queue = IntArray(pixels.size)
        fun similar(c: Int) = abs(Color.alpha(c) - Color.alpha(color)) <= wandTolerance && (Color.alpha(c) < 16 && Color.alpha(color) < 16 || abs(Color.red(c) - Color.red(color)) <= wandTolerance && abs(Color.green(c) - Color.green(color)) <= wandTolerance && abs(Color.blue(c) - Color.blue(color)) <= wandTolerance)
        var head = 0; var tail = 0; queue[tail++] = seed; seen[seed] = true
        while (head < tail) {
            val index = queue[head++]; val xx = index % canvasW; val yy = index / canvasW
            fun visit(n: Int) { if (!seen[n]) { seen[n] = true; if (similar(pixels[n])) queue[tail++] = n } }
            if (xx > 0) visit(index - 1); if (xx + 1 < canvasW) visit(index + 1); if (yy > 0) visit(index - canvasW); if (yy + 1 < canvasH) visit(index + canvasW)
        }
        val chosen = BooleanArray(pixels.size); for (i in 0 until tail) chosen[queue[i]] = true
        val added = Path().apply { for (yy in 0 until canvasH) { var xx = 0; while (xx < canvasW) { if (!chosen[yy * canvasW + xx]) { xx++; continue }; val start = xx; while (xx < canvasW && chosen[yy * canvasW + xx]) xx++; addRect(start.toFloat(), yy.toFloat(), xx.toFloat(), yy + 1f, Path.Direction.CW) } } }
        mergeSelection(added, "Add")
    }
    override fun onTouchEvent(e: MotionEvent): Boolean {
        parent?.requestDisallowInterceptTouchEvent(true)
        if (e.pointerCount > 1) {
            shapeTouchActive = false
            if (tool == "Shapes" && !navigating) shapes.cancelGesture()
            if (!navigating) { stroke = null; strokeBefore?.let { images[layer].recycle(); images[layer] = it }; strokeBefore = null; navigating = true; oldSpan = 0f }
            val x = (e.getX(0) + e.getX(1)) / 2; val y = (e.getY(0) + e.getY(1)) / 2
            val span = hypot(e.getX(1) - e.getX(0), e.getY(1) - e.getY(0)); val a = Math.toDegrees(atan2(e.getY(1) - e.getY(0), e.getX(1) - e.getX(0)).toDouble()).toFloat()
            if (oldSpan > 0 && e.actionMasked == MotionEvent.ACTION_MOVE) {
                val anchor = point(oldX, oldY); zoom = (zoom * span / oldSpan).coerceIn(.25f, 8f); angle += ((a - oldAngle + 540) % 360) - 180
                Matrix().apply { postScale(fit() * zoom, fit() * zoom); postRotate(angle) }.mapPoints(anchor)
                dx = x - left() - anchor[0]; dy = y - top() - anchor[1]
            }
            oldSpan = span; oldAngle = a; oldX = x; oldY = y; invalidate(); return true
        }
        if (navigating) { if (e.actionMasked == MotionEvent.ACTION_UP || e.actionMasked == MotionEvent.ACTION_CANCEL) navigating = false; return true }
        val p = point(e.x, e.y); val x = p[0]; val y = p[1]
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
                strokeBefore = if (tool == "Lasso") null else copy(images[layer])
                stroke = Path().apply { moveTo(x, y); lineTo(x + .01f, y) }
            }
            MotionEvent.ACTION_MOVE -> if (stroke != null) {
                for (i in 0..e.historySize) {
                    val q = if (i == e.historySize) p else point(e.getHistoricalX(i), e.getHistoricalY(i))
                    if (tool == "Smudge" || tool == "Blur") {
                        val distance = hypot(q[0] - lastX, q[1] - lastY)
                        val count = ceil(distance / (brushSize / 4).coerceAtLeast(1f)).toInt().coerceIn(1, 256)
                        val startX = lastX; val startY = lastY
                        for (step in 1..count) {
                            val xx = startX + (q[0] - startX) * step / count; val yy = startY + (q[1] - startY) * step / count
                            filterStamp(xx, yy); lastX = xx; lastY = yy
                        }
                    }
                    stroke!!.quadTo(lastX, lastY, (lastX + q[0]) / 2, (lastY + q[1]) / 2); lastX = q[0]; lastY = q[1]
                }
                // Keep the existing selection while previewing an additive/subtractive lasso.
            }
            MotionEvent.ACTION_UP -> {
                stroke?.let { path ->
                    path.lineTo(x, y)
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
        images.forEach { it.recycle() }; disposeEdits(history); disposeEdits(future); clipboard?.recycle(); source?.recycle(); original?.recycle(); meshSource?.recycle(); strokeBefore?.recycle()
    }
}
