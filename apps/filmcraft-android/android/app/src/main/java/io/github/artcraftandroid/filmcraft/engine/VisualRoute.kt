package io.github.artcraftandroid.filmcraft.engine

/**
 * Where opening a Home entry lands. Videos open the editor with the project itself; drawings and
 * pictures open the embedded Draw canvas (drawings keep their layered artwork; pictures open as a
 * photo document). Pure decision, unit-tested.
 */
enum class VisualRoute {
    /** The timeline editor, with [ProjectEntry] itself. */
    Video,

    /** The Draw canvas, holding the layered native document (its artwork reopens). */
    Drawing,

    /** The Draw canvas, holding the picture as a photo document (saves to the photo library). */
    Photo,
}

/**
 * The route for opening [kind] (a [ProjectEntry.kind]: "video", "drawing", "picture", or anything
 * else an older version of the library holds).
 *
 * An unknown kind is treated as a picture: the Draw canvas can open any decodable image, while
 * fabricating a video shell for it could not.
 */
fun visualRoute(kind: String): VisualRoute = when (kind) {
    "video" -> VisualRoute.Video
    "drawing" -> VisualRoute.Drawing
    else -> VisualRoute.Photo
}
