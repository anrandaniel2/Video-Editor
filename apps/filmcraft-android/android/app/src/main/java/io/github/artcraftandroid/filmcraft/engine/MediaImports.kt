package io.github.artcraftandroid.filmcraft.engine

import android.content.Context
import android.graphics.Bitmap
import android.graphics.ImageDecoder
import android.net.Uri
import io.github.artcraftandroid.filmcraft.core.decodeImportPicture
import java.io.File
import kotlin.math.sqrt

/** Drawing-only conversion; timeline imports keep native source files and animation intact. */
object MediaImports {
    val nativeImages = setOf("png", "jpg", "jpeg", "gif", "webp", "tif", "tiff", "bmp", "tga", "dpx", "exr", "hdr", "pic", "dng", "jxl", "svg", "svgz", "ico", "pnm", "pbm", "pgm", "ppm", "pam", "qoi", "ff", "farbfeld", "dds")
    val platformImages = setOf("heic", "heif", "avif")

    /** Native drawing import fallback, converted to a temporary lossless PNG for ProCraft. */
    fun preparePicture(context: Context, uri: Uri): File {
        val name = Projects.safeName(Projects.displayName(context.contentResolver, uri))
        val folder = File(context.cacheDir, "picture-import-${java.util.UUID.randomUUID()}").apply { mkdirs() }
        val source = File(folder, name)
        val output = File(context.cacheDir, "picture-ready-${java.util.UUID.randomUUID()}.png")
        try {
            context.contentResolver.openInputStream(uri)?.use { input ->
                source.outputStream().use { out ->
                    val buffer = ByteArray(65536)
                    var total = 0L
                    while (true) {
                        val n = input.read(buffer)
                        if (n < 0) break
                        total += n
                        check(total <= 128L * 1024 * 1024) { "The picture exceeds the 128 MB import limit." }
                        out.write(buffer, 0, n)
                    }
                }
            } ?: error("The image could not be read.")
            val bitmap = runCatching {
                ImageDecoder.decodeBitmap(ImageDecoder.createSource(source)) { decoder, info, _ ->
                    val w = info.size.width; val h = info.size.height
                    val scale = minOf(1.0, 8192.0 / maxOf(w, h), sqrt(16_777_216.0 / (w.toDouble() * h)))
                    if (scale < 1.0) decoder.setTargetSize((w * scale).toInt().coerceAtLeast(1), (h * scale).toInt().coerceAtLeast(1))
                    decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
                }
            }.getOrElse { decodeImportPicture(source.absolutePath).toBitmap() ?: error("The picture could not be decoded.") }
            try { output.outputStream().use { check(bitmap.compress(Bitmap.CompressFormat.PNG, 100, it)) { "Image conversion failed." } } }
            finally { bitmap.recycle() }
            return output
        } catch (e: Exception) { output.delete(); throw e }
        finally { folder.deleteRecursively() }
    }
}
