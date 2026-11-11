package io.github.artcraftandroid.filmcraft.ui.editor

/** Interactive frames have a fixed 30 Hz budget. Quality adapts only while editing; the
 * settled frame is rendered at the monitor's full resolution. */
internal object LivePreviewPolicy {
    const val FRAME_NANOS = 1_000_000_000L / 30
    fun nextSide(side:Int,maximum:Int,elapsedNanos:Long):Int = when {
        elapsedNanos>FRAME_NANOS -> (side*0.85).toInt().coerceAtLeast(160)
        elapsedNanos<20_000_000L -> (side*1.08).toInt().coerceAtMost(maximum)
        else -> side
    }.coerceIn(160,maximum.coerceAtLeast(160))
}
