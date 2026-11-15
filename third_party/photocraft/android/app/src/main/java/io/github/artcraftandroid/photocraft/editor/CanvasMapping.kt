package io.github.artcraftandroid.photocraft.editor

/**
 * The part of the document the canvas shows, in document pixels: the whole document at 100 %
 * zoom, a smaller window of it as the zoom goes up, moved by panning.
 */
data class DocView(val x: Float, val y: Float, val width: Float, val height: Float)

/**
 * Where a pointer on the canvas lands in the document. The view is drawn fitted (contained and
 * centred) in the canvas box, so its pixels are offset from the box's and scaled by whichever
 * side is the tighter fit. Returns null when the pointer is outside the picture (the letterbox),
 * which paints nothing.
 */
fun documentPoint(canvasWidth: Int, canvasHeight: Int, view: DocView, x: Float, y: Float): Pair<Float, Float>? {
    if (canvasWidth <= 0 || canvasHeight <= 0 || view.width <= 0f || view.height <= 0f) return null
    val scale = minOf(canvasWidth / view.width, canvasHeight / view.height)
    val drawnWidth = view.width * scale
    val drawnHeight = view.height * scale
    val left = (canvasWidth - drawnWidth) / 2f
    val top = (canvasHeight - drawnHeight) / 2f
    val dx = view.x + (x - left) / scale
    val dy = view.y + (y - top) / scale
    return if (dx < view.x || dy < view.y || dx >= view.x + view.width || dy >= view.y + view.height) null else dx to dy
}
