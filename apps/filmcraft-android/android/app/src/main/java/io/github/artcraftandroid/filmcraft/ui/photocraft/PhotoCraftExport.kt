package io.github.artcraftandroid.filmcraft.ui.photocraft

import android.content.ContentValues
import android.content.Context
import android.graphics.Bitmap
import android.net.Uri
import android.provider.MediaStore
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import java.nio.ByteBuffer

/**
 * A picture in the form the native photo pipeline takes it: straight sRGB RGBA, a byte a channel,
 * rows top to bottom. Android's `ARGB_8888` memory layout is exactly those four bytes in that
 * order, so handing the buffer back to a bitmap is a straight copy.
 */
internal fun Bitmap.rgbaBytes(): ByteArray {
    // `Bitmap.copy` is nullable in the platform SDK, so a non-RGBA_8888 source is copied once here.
    val source = if (config == Bitmap.Config.ARGB_8888 && !isRecycled) this else copy(Bitmap.Config.ARGB_8888, false)!!
    val count = source.width * source.height
    val pixels = IntArray(count)
    source.getPixels(pixels, 0, source.width, 0, 0, source.width, source.height)
    if (source !== this) source.recycle()
    val out = ByteArray(count * 4)
    for (index in 0 until count) {
        val pixel = pixels[index]
        val offset = index * 4
        out[offset] = ((pixel shr 16) and 0xFF).toByte()
        out[offset + 1] = ((pixel shr 8) and 0xFF).toByte()
        out[offset + 2] = (pixel and 0xFF).toByte()
        out[offset + 3] = ((pixel ushr 24) and 0xFF).toByte()
    }
    return out
}

/** The reverse: a native RGBA buffer as an Android bitmap. */
internal fun rgbaToBitmap(rgba: ByteArray, width: Int, height: Int): Bitmap {
    require(width > 0 && height > 0 && rgba.size >= width * height * 4) { "The rendered pixels don't match their size." }
    val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
    bitmap.copyPixelsFromBuffer(ByteBuffer.wrap(rgba, 0, width * height * 4))
    return bitmap
}

internal fun ByteArray.toImageBitmap(width: Int, height: Int): ImageBitmap =
    rgbaToBitmap(this, width, height).asImageBitmap()

/**
 * Publish a developed photo in the phone's library, where Home lists it as a picture. The file is
 * pending until it is fully written, so a failed save never leaves a half photo behind.
 */
internal fun savePhotoToLibrary(context: Context, bitmap: Bitmap, name: String): Uri {
    val resolver = context.contentResolver
    val clean = name.substringBeforeLast('.').replace(Regex("[\\\\/:*?\"<>|\\p{Cntrl}]"), "_").trim().take(60).ifBlank { "Photo" }
    val values = ContentValues().apply {
        put(MediaStore.Images.Media.DISPLAY_NAME, "PhotoCraft-$clean-${System.currentTimeMillis()}.jpg")
        put(MediaStore.Images.Media.MIME_TYPE, "image/jpeg")
        put(MediaStore.Images.Media.RELATIVE_PATH, "Pictures/FilmCraft")
        put(MediaStore.Images.Media.IS_PENDING, 1)
    }
    val uri = resolver.insert(MediaStore.Images.Media.EXTERNAL_CONTENT_URI, values)
        ?: error("Could not create a photo in the library.")
    try {
        resolver.openOutputStream(uri)?.use { output ->
            check(bitmap.compress(Bitmap.CompressFormat.JPEG, 95, output)) { "JPEG export failed." }
        } ?: error("Could not write the saved photo.")
        check(resolver.update(uri, ContentValues().apply { put(MediaStore.Images.Media.IS_PENDING, 0) }, null, null) > 0) {
            "Could not publish the saved photo."
        }
        return uri
    } catch (e: Exception) {
        resolver.delete(uri, null, null)
        throw e
    }
}
