package io.github.artcraftandroid.photocraft.engine

import android.graphics.Bitmap
import io.github.artcraftandroid.filmcraft.core.PhotoSession
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject

/**
 * The engine the Compose editor drives: one [PhotoSession] (PhotoCraft's Rust engine behind
 * UniFFI) and the JSON commands it runs. Every tool, adjustment, filter, layer operation and
 * selection in the editor is one of these command ids, so the UI stays in step with the engine
 * by construction.
 *
 * All calls are suspending and run on the IO dispatcher: rendering and commands can take
 * hundreds of milliseconds on a large document.
 */
class PhotoEngine {
    // Null once released: the UniFFI object holds the Rust session, so dropping the last
    // reference is what frees the documents (and their pixels).
    private var live: PhotoSession? = PhotoSession()

    private fun session(): PhotoSession = live ?: throw IllegalStateException("The photo editor is closed.")

    /** Let the session go: its documents, their pixels and the engine's caches go with it. */
    fun release() {
        live = null
    }

    /** Run one engine command. `params` is JSON (an object, or empty); the result is the command's JSON value. */
    suspend fun execute(id: String, params: String = "{}"): String = withContext(Dispatchers.IO) {
        session().execute(id, params)
    }

    /** Run one engine command with a JSON object built from [params]. */
    suspend fun execute(id: String, params: JSONObject): String = execute(id, params.toString())

    /** Whether the command can run right now. */
    suspend fun isEnabled(id: String): Boolean = withContext(Dispatchers.IO) {
        session().isEnabled(id)
    }

    /** Why the command cannot run, if it cannot. */
    suspend fun disabledReason(id: String): String? = withContext(Dispatchers.IO) {
        session().disabledReason(id)
    }

    /** The session's state: revision, undo/redo, the open documents and the active layer. */
    suspend fun status() = withContext(Dispatchers.IO) {
        session().status()
    }

    /** Open the given bytes as a new document and make it active. Returns the new document's index. */
    suspend fun openBytes(name: String, bytes: ByteArray, asExt: String? = null): Int =
        withContext(Dispatchers.IO) {
            session().openBytes(name, bytes, asExt).toInt()
        }

    /** Place the given bytes as a new layer on the active document. Returns the command's JSON. */
    suspend fun placeBytes(name: String, bytes: ByteArray, params: String = "{}"): String =
        withContext(Dispatchers.IO) {
            session().placeBytes(name, bytes, params)
        }

    /** Export the active document to `format` ("png", "jpeg", "webp", "psd", …) and return the file's bytes. */
    suspend fun exportBytes(format: String, quality: Int = 0): ByteArray = withContext(Dispatchers.IO) {
        // The bridge takes the quality as a u8 percentage (0 keeps the codec defaults) and
        // returns the file's bytes.
        session().exportBytes(format, quality.coerceIn(0, 100).toUByte())
    }

    /** Composite the active document at the given size, fitting the whole canvas into it. */
    suspend fun render(maxWidth: Int, maxHeight: Int): Bitmap = withContext(Dispatchers.IO) {
        val frame = session().render(maxWidth.coerceAtLeast(1).toUInt(), maxHeight.coerceAtLeast(1).toUInt())
        frame.toBitmap()
    }

    /** Composite one region of the active document, in document pixels. */
    suspend fun renderRegion(x: Int, y: Int, width: Int, height: Int): Bitmap = withContext(Dispatchers.IO) {
        val frame = session().renderRegion(
            x,
            y,
            width.coerceAtLeast(1).toUInt(),
            height.coerceAtLeast(1).toUInt(),
        )
        frame.toBitmap()
    }

    /** A thumbnail of the active document, at most `maxSide` pixels on its longer side. */
    suspend fun thumbnail(maxSide: Int): Bitmap = render(maxSide, maxSide)

    /** Make the document at `index` active. */
    suspend fun setActive(index: Int): Boolean = withContext(Dispatchers.IO) {
        session().setActive(index.toUInt())
    }

    /** Close the document at `index`, discarding its changes. */
    suspend fun close(index: Int): Boolean = withContext(Dispatchers.IO) {
        session().close(index.toUInt())
    }

    /** Undo one step. */
    suspend fun undo(): Boolean = withContext(Dispatchers.IO) {
        session().undo()
    }

    /** Redo one step. */
    suspend fun redo(): Boolean = withContext(Dispatchers.IO) {
        session().redo()
    }

    /** The active document's size, in pixels. */
    suspend fun documentSize() = withContext(Dispatchers.IO) {
        session().documentSize()
    }
}

/** The frame's 8-bit RGBA pixels as a Bitmap. */
fun io.github.artcraftandroid.filmcraft.core.PhotoFrame.toBitmap(): Bitmap {
    val w = width.toInt()
    val h = height.toInt()
    val bmp = Bitmap.createBitmap(w, h, Bitmap.Config.ARGB_8888)
    // ARGB_8888 keeps its bytes in memory as R, G, B, A: the engine's order
    bmp.copyPixelsFromBuffer(java.nio.ByteBuffer.wrap(rgba, 0, w * h * 4))
    return bmp
}
