package io.github.artcraftandroid.filmcraft.ui.editor

import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.net.Uri
import androidx.activity.result.contract.ActivityResultContract
import io.github.artcraftandroid.filmcraft.core.encodeDrawingJxl

internal enum class DrawingFormat(val label: String, val extension: String, val mime: String) {
    PNG("PNG", "png", "image/png"), JPEG("JPEG", "jpeg", "image/jpeg"), JPG("JPG", "jpg", "image/jpeg"), JXL("JPEG XL", "jxl", "image/jxl")
}
internal class DrawingSaveContract : ActivityResultContract<Pair<String, String>, Uri?>() {
    override fun createIntent(context: Context, input: Pair<String, String>) = Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
        addCategory(Intent.CATEGORY_OPENABLE); type = input.second; putExtra(Intent.EXTRA_TITLE, input.first)
    }
    override fun parseResult(resultCode: Int, intent: Intent?): Uri? = if (resultCode == android.app.Activity.RESULT_OK) intent?.data else null
}
internal fun writeDrawing(context: Context, uri: Uri, bitmap: Bitmap, format: DrawingFormat) {
    context.contentResolver.openOutputStream(uri, "w")?.use { output ->
        when (format) {
            DrawingFormat.PNG -> check(bitmap.compress(Bitmap.CompressFormat.PNG, 100, output)) { "PNG export failed." }
            DrawingFormat.JPEG, DrawingFormat.JPG -> {
                val opaque = Bitmap.createBitmap(bitmap.width, bitmap.height, Bitmap.Config.ARGB_8888)
                try { val c = Canvas(opaque); c.drawColor(Color.WHITE); c.drawBitmap(bitmap, 0f, 0f, null); check(opaque.compress(Bitmap.CompressFormat.JPEG, 95, output)) { "JPEG export failed." } }
                finally { opaque.recycle() }
            }
            DrawingFormat.JXL -> {
                val pixels = IntArray(bitmap.width * bitmap.height)
                bitmap.getPixels(pixels, 0, bitmap.width, 0, 0, bitmap.width, bitmap.height)
                val rgba = ByteArray(pixels.size * 4)
                pixels.forEachIndexed { i, pixel -> rgba[i * 4] = (pixel ushr 16).toByte(); rgba[i * 4 + 1] = (pixel ushr 8).toByte(); rgba[i * 4 + 2] = pixel.toByte(); rgba[i * 4 + 3] = (pixel ushr 24).toByte() }
                output.write(encodeDrawingJxl(rgba, bitmap.width.toUInt(), bitmap.height.toUInt()))
            }
        }
    } ?: error("Could not open the export destination.")
}

/** Save standalone drawings directly to the photo library; publish only after encoding succeeds. */
internal fun saveDrawingPhoto(context: Context, bitmap: Bitmap, existing: Uri? = null): Uri {
    val resolver = context.contentResolver
    if (existing != null) {
        writeDrawing(context, existing, bitmap, DrawingFormat.PNG)
        return existing
    }
    val values = android.content.ContentValues().apply {
        put(android.provider.MediaStore.Images.Media.DISPLAY_NAME, "Drawing-${System.currentTimeMillis()}.png")
        put(android.provider.MediaStore.Images.Media.MIME_TYPE, "image/png")
        put(android.provider.MediaStore.Images.Media.RELATIVE_PATH, "Pictures/FilmCraft")
        put(android.provider.MediaStore.Images.Media.IS_PENDING, 1)
    }
    val uri = resolver.insert(android.provider.MediaStore.Images.Media.EXTERNAL_CONTENT_URI, values)
        ?: error("Could not create a photo in the library.")
    try {
        writeDrawing(context, uri, bitmap, DrawingFormat.PNG)
        check(resolver.update(uri, android.content.ContentValues().apply { put(android.provider.MediaStore.Images.Media.IS_PENDING, 0) }, null, null) > 0) { "Could not publish the saved photo." }
        return uri
    } catch (e: Exception) { resolver.delete(uri, null, null); throw e }
}
