package io.github.artcraftandroid.filmcraft.engine

import android.content.ContentUris
import android.content.ContentValues
import android.content.Context
import android.net.Uri
import android.os.Build
import android.provider.MediaStore
import io.github.artcraftandroid.procraft.gallery.Artwork
import io.github.artcraftandroid.procraft.gallery.ArtworkStore
import io.github.artcraftandroid.procraft.gallery.Stack
import java.io.File

/** Existing layered artworks and FilmCraft-owned photo exports, not copies or thumbnail-only projects. */
internal class SavedVisuals(private val context: Context) {
    private val store by lazy { ArtworkStore(File(context.filesDir, "artworks")) }

    fun list(): List<ProjectEntry> = artworks() + runCatching { pictures() }.getOrElse {
        android.util.Log.w("FilmCraftLibrary", "Saved pictures could not be listed", it)
        emptyList()
    }

    private fun artworks(): List<ProjectEntry> = store.list().flatMap { entry ->
        when (entry) {
            is Artwork -> listOf(entry)
            is Stack -> entry.artworks
        }
    }.map { a ->
        ProjectEntry("drawing:${a.dir.relativeTo(store.root).path}", a.name, a.modified, a.modified,
            0, a.width, a.height, a.dir, kind = "drawing")
    }

    fun artwork(p: ProjectEntry) = Artwork(p.dir, p.name, p.width, p.height, p.opened)

    private fun pictures(): List<ProjectEntry> {
        if (Build.VERSION.SDK_INT < 29) return emptyList()
        val columns = arrayOf(MediaStore.Images.Media._ID, MediaStore.Images.Media.DISPLAY_NAME, MediaStore.Images.Media.DATE_MODIFIED, MediaStore.Images.Media.DATE_ADDED, MediaStore.Images.Media.WIDTH, MediaStore.Images.Media.HEIGHT)
        return context.contentResolver.query(MediaStore.Images.Media.EXTERNAL_CONTENT_URI, columns,
            "${MediaStore.Images.Media.RELATIVE_PATH} IN (?, ?) AND ${MediaStore.Images.Media.IS_PENDING} = 0", arrayOf("Pictures/FilmCraft/", "Pictures/FilmCraft"), "${MediaStore.Images.Media.DATE_MODIFIED} DESC")?.use { cursor ->
            buildList {
                while (cursor.moveToNext()) {
                    val id = cursor.getLong(0)
                    val uri = ContentUris.withAppendedId(MediaStore.Images.Media.EXTERNAL_CONTENT_URI, id)
                    add(ProjectEntry("picture:$id", cursor.getString(1) ?: "Picture", cursor.getLong(2) * 1000,
                        cursor.getLong(3) * 1000, 0, cursor.getInt(4).coerceAtLeast(1), cursor.getInt(5).coerceAtLeast(1),
                        context.filesDir, kind = "picture", sourceUri = uri.toString()))
                }
            }
        } ?: emptyList()
    }

    fun rename(p: ProjectEntry, name: String) {
        if (p.kind == "drawing") store.rename(artwork(p), name)
        else check(context.contentResolver.update(Uri.parse(p.sourceUri), ContentValues().apply {
            put(MediaStore.Images.Media.DISPLAY_NAME, name)
        }, null, null) > 0) { "Could not rename picture." }
    }

    fun delete(p: ProjectEntry) {
        if (p.kind == "drawing") store.delete(artwork(p))
        else check(context.contentResolver.delete(Uri.parse(p.sourceUri), null, null) > 0) { "Could not delete picture." }
    }

    fun duplicate(p: ProjectEntry) {
        if (p.kind == "drawing") { store.duplicate(artwork(p)); return }
        check(Build.VERSION.SDK_INT >= 29)
        val resolver = context.contentResolver
        val source = Uri.parse(p.sourceUri)
        val destination = resolver.insert(MediaStore.Images.Media.EXTERNAL_CONTENT_URI, ContentValues().apply {
            put(MediaStore.Images.Media.DISPLAY_NAME, "Copy-${System.currentTimeMillis()}-${p.name}")
            put(MediaStore.Images.Media.MIME_TYPE, resolver.getType(source) ?: "image/png")
            put(MediaStore.Images.Media.RELATIVE_PATH, "Pictures/FilmCraft/")
            put(MediaStore.Images.Media.IS_PENDING, 1)
        }) ?: error("Could not copy picture.")
        try {
            resolver.openInputStream(source)?.use { input ->
                resolver.openOutputStream(destination)?.use { output -> input.copyTo(output) } ?: error("Could not write picture.")
            } ?: error("Could not read picture.")
            check(resolver.update(destination, ContentValues().apply { put(MediaStore.Images.Media.IS_PENDING, 0) }, null, null) > 0)
        } catch (e: Exception) { resolver.delete(destination, null, null); throw e }
    }
}
