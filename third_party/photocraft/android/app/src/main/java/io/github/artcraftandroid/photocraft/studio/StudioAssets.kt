package io.github.artcraftandroid.photocraft.studio

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.sp

/**
 * The reference artwork the interface shows: the portraits and spreads the studio's Pages and
 * Navigator panels display, and the picture a workspace opens with when the host hands it no frame.
 *
 * They are the reference UI's own images (`assets/studio/reference/`), so the studio at rest looks
 * like the design it was ported from; the document on the canvas is always the engine's.
 */
@Composable
fun ReferenceAsset(
    path: String,
    modifier: Modifier = Modifier,
    description: String = "",
    contentScale: ContentScale = ContentScale.Crop,
    /** SVG artwork has no codec: draw the reference's own vector through Compose instead. */
    isVector: Boolean = false,
) {
    val context = LocalContext.current
    val bitmap by produceState<ImageBitmap?>(initialValue = null, path) {
        value = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.IO) {
            runCatching {
                val bytes = context.assets.open(path).use { it.readBytes() }
                android.graphics.BitmapFactory.decodeByteArray(bytes, 0, bytes.size)?.asImageBitmap()
            }.getOrNull()
        }
    }
    Box(modifier.background(Color(0x221A1B19)), contentAlignment = Alignment.Center) {
        val image = bitmap
        if (isVector) {
            ReferenceVector(path, modifier, description)
        } else if (image != null) {
            Image(
                bitmap = image,
                contentDescription = description,
                contentScale = contentScale,
                modifier = Modifier.fillMaxSize(),
            )
        } else {
            Text("Preview unavailable", fontSize = 11.sp, color = Color(0xFF555555))
        }
    }
}

/**
 * The reference's own `sketch.svg`, drawn as Compose vectors. The file is the design's logo study:
 * nine rounded squares on a cream field with the wordmark beneath.
 */
@Composable
private fun ReferenceVector(path: String, modifier: Modifier, description: String) {
    androidx.compose.foundation.Canvas(modifier.fillMaxSize()) {
        val w = size.width
        val h = size.height
        drawRect(Color(0xFFF6F1E7))
        // The nine marks, as the reference's grid of rounded tiles.
        val cols = 3
        val rows = 3
        val gap = w * 0.055f
        val cell = (w - gap * (cols + 1)) / cols
        val top = h * 0.12f
        for (r in 0 until rows) {
            for (c in 0 until cols) {
                val x = gap + c * (cell + gap)
                val y = top + r * (cell + gap)
                drawRoundRect(
                    color = Color(0xFF17181A),
                    topLeft = androidx.compose.ui.geometry.Offset(x, y),
                    size = androidx.compose.ui.geometry.Size(cell, cell),
                    cornerRadius = androidx.compose.ui.geometry.CornerRadius(cell * 0.42f),
                )
                // The cut-out the reference gives each tile: a stem with two leaves.
                val leaf = cell * 0.30f
                drawOval(
                    color = Color(0xFFF6F1E7),
                    topLeft = androidx.compose.ui.geometry.Offset(x + cell * 0.22f, y + cell * 0.30f),
                    size = androidx.compose.ui.geometry.Size(leaf, leaf * 0.62f),
                )
                drawOval(
                    color = Color(0xFFF6F1E7),
                    topLeft = androidx.compose.ui.geometry.Offset(x + cell * 0.48f, y + cell * 0.42f),
                    size = androidx.compose.ui.geometry.Size(leaf, leaf * 0.62f),
                )
                drawRect(
                    color = Color(0xFFF6F1E7),
                    topLeft = androidx.compose.ui.geometry.Offset(x + cell * 0.47f, y + cell * 0.34f),
                    size = androidx.compose.ui.geometry.Size(cell * 0.06f, cell * 0.36f),
                )
            }
        }
        // The wordmark's rules, as the reference places them under the marks.
        val baseline = top + rows * (cell + gap) + h * 0.03f
        drawRect(
            color = Color(0xFF8E9296),
            topLeft = androidx.compose.ui.geometry.Offset(gap, baseline),
            size = androidx.compose.ui.geometry.Size(w - gap * 2, 1.4f),
        )
        drawRect(
            color = Color(0xFF8E9296),
            topLeft = androidx.compose.ui.geometry.Offset(gap, baseline + h * 0.035f),
            size = androidx.compose.ui.geometry.Size((w - gap * 2) * 0.55f, 1.2f),
        )
    }
}

/** The picture a workspace opens with, from the reference's set. */
fun referencePictureFor(id: WorkspaceId): String = "studio/reference/" + when (id) {
    WorkspaceId.Sketch -> "sketch.svg"
    WorkspaceId.Vector -> "vector.webp"
    WorkspaceId.Pixel -> "pixel.webp"
    WorkspaceId.Layout -> "layout.webp"
}

/** Whether a reference picture is the vector one. */
fun referenceIsVector(id: WorkspaceId): Boolean = id == WorkspaceId.Sketch

/** The reference picture a workspace opens with, drawn in the given box. */
@Composable
fun WorkspaceArtwork(id: WorkspaceId, modifier: Modifier = Modifier, contentScale: ContentScale = ContentScale.Crop) {
    ReferenceAsset(
        path = referencePictureFor(id),
        modifier = modifier,
        description = id.title,
        contentScale = contentScale,
        isVector = referenceIsVector(id),
    )
}
