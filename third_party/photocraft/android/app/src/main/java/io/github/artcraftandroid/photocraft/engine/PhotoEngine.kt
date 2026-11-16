package io.github.artcraftandroid.photocraft.engine

import android.graphics.Bitmap
import io.github.artcraftandroid.photocraft.core.PhotoSession
import io.github.artcraftandroid.photocraft.core.StudioBrushSet
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

    // -------------------------------------------------------------------- live strokes

    /**
     * Start a stroke that is drawn as it goes. [params] is the `paint.stroke` (or `paint.pencil`)
     * command's JSON with the first point in `points`; the document is not touched until
     * [strokeEnd]. Returns false when nothing can be painted on (no document, or a locked layer),
     * so the caller can ignore the gesture rather than handle an exception.
     */
    suspend fun strokeBegin(cmd: String, params: String): Boolean = withContext(Dispatchers.IO) {
        session().strokeBegin(cmd, params)
    }

    /**
     * Render more points into the live stroke; `points` is JSON `[[x, y, pressure, …], …]`. The
     * result is the damage rect as `{"x","y","width","height"}`, or null when nothing changed.
     */
    suspend fun strokeAdd(points: String): String? = withContext(Dispatchers.IO) {
        session().strokeAdd(points)
    }

    /** The rect the live stroke has touched so far, as [strokeAdd] returns a damage rect. */
    suspend fun strokeBounds(): String? = withContext(Dispatchers.IO) {
        session().strokeBounds()
    }

    /** The live stroke's pixels over a document rect: what to draw over the canvas. */
    suspend fun strokePixels(x: Int, y: Int, width: Int, height: Int): Bitmap? = withContext(Dispatchers.IO) {
        session().strokePixels(x, y, width.coerceAtLeast(1).toUInt(), height.coerceAtLeast(1).toUInt())?.toBitmap()
    }

    /** Whether the stroke changes while the pointer is still (airbrush build-up, smoothing). */
    suspend fun strokeWantsTime(): Boolean = withContext(Dispatchers.IO) {
        session().strokeWantsTime()
    }

    /** Let the stroke catch up where the pointer is; returns its damage rect as [strokeAdd]. */
    suspend fun strokeTick(): String? = withContext(Dispatchers.IO) {
        session().strokeTick()
    }

    /** Finish the stroke: one History step, committed by the engine from the previewed pixels. */
    suspend fun strokeEnd(): String = withContext(Dispatchers.IO) {
        session().strokeEnd()
    }

    /** Throw the stroke away; the document never saw it. */
    suspend fun strokeCancel() = withContext(Dispatchers.IO) {
        session().strokeCancel()
    }

    // -------------------------------------------------------------------- brushes

    /** The brush the engine will paint the next stroke with, as JSON (`tools.setBrush`'s shape). */
    suspend fun brush(): JSONObject = withContext(Dispatchers.IO) {
        JSONObject(session().brush())
    }

    /** One key of the current brush, as JSON (`"size"`, `"opacity"`, …); "null" when it has none. */
    suspend fun brushValue(key: String): String = withContext(Dispatchers.IO) {
        session().brushValue(key)
    }

    /**
     * A preview of the brush's next stroke, [width] × [height] pixels. [params] is a
     * `tools.setBrush` patch to preview instead of the current brush (empty for the brush as it
     * is): the studio's preset grid shows every preset without picking it.
     */
    suspend fun brushPreview(width: Int, height: Int, params: String = ""): Bitmap = withContext(Dispatchers.IO) {
        val frame = session().brushPreview(
            width.coerceIn(8, 512).toUInt(),
            height.coerceIn(8, 512).toUInt(),
            params,
        )
        frame.toBitmap()
    }

    /**
     * The drawing app's brush shelf: its sets and the brushes in each, read from the engine's live
     * preset list.
     */
    suspend fun studioBrushSets(): List<StudioBrushSet> = withContext(Dispatchers.IO) {
        session().studioBrushSets()
    }

    /**
     * Put the drawing app's brush library in place of the engine's generic preset set; returns how
     * many brushes the shelf holds afterwards.
     */
    suspend fun studioInstallBrushes(): Int = withContext(Dispatchers.IO) {
        session().studioInstallBrushes().toInt()
    }

    /**
     * The drawing app's brush shelf as the library ships it, with no session and no document: the
     * same sets and brushes the studio lists, for a host that wants the names (FilmCraft's drawing
     * canvas offers this shelf, so a brush means the same brush in both).
     */
    suspend fun studioShelf(): List<StudioBrushSet> = withContext(Dispatchers.IO) {
        io.github.artcraftandroid.photocraft.core.studioShelf()
    }
}

/** The frame's 8-bit RGBA pixels as a Bitmap. */
fun io.github.artcraftandroid.photocraft.core.PhotoFrame.toBitmap(): Bitmap {
    val w = width.toInt()
    val h = height.toInt()
    val bmp = Bitmap.createBitmap(w, h, Bitmap.Config.ARGB_8888)
    // ARGB_8888 keeps its bytes in memory as R, G, B, A: the engine's order
    bmp.copyPixelsFromBuffer(java.nio.ByteBuffer.wrap(rgba, 0, w * h * 4))
    return bmp
}
