package io.github.artcraftandroid.filmcraft.ui.editor

import io.github.artcraftandroid.filmcraft.ui.editorGlass
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import io.github.artcraftandroid.filmcraft.core.ClipInfo
import io.github.artcraftandroid.filmcraft.core.ClipKind
import io.github.artcraftandroid.filmcraft.ui.Fc
import io.github.artcraftandroid.filmcraft.ui.FcSize
import io.github.artcraftandroid.filmcraft.ui.Glyph
import io.github.artcraftandroid.filmcraft.ui.GlyphIcon
import io.github.artcraftandroid.filmcraft.ui.Glyphs
import io.github.artcraftandroid.filmcraft.ui.LucideIcons
import io.github.artcraftandroid.filmcraft.ui.ToolTile
import io.github.artcraftandroid.filmcraft.ui.glyph

/** One tool on the bar. */
data class Tool(val tag: String, val label: String, val glyph: Glyph, val enabled: Boolean = true, val active: Boolean = false, val onClick: () -> Unit)

/**
 * The tool bar under the timeline: what can be added when nothing is selected, the selected
 * clip's tools otherwise, and the submenus some of them open.
 */
@Composable
fun EditorToolbar(ed: EditorController, modifier: Modifier = Modifier) {
    val clip = ed.selectedClip
    val caption = ed.selectedCaptionInfo
    val (back, tools) = when {
        ed.submenu == Submenu.AddAudio -> Pair({ ed.submenu = Submenu.None }, addAudioTools(ed))
        ed.submenu == Submenu.Files -> Pair({ ed.submenu = Submenu.None }, filesTools(ed))
        clip != null && ed.submenu == Submenu.Retime -> Pair({ ed.submenu = Submenu.None }, retimeTools(ed, clip))
        clip != null && ed.submenu == Submenu.ClipAudio -> Pair({ ed.submenu = Submenu.None }, clipAudioTools(ed, clip))
        clip != null -> Pair({ ed.select(null) }, clipTools(ed, clip))
        caption != null -> Pair({ ed.selectedCaption = null }, captionTools(ed, caption.id))
        else -> Pair(null, addTools(ed))
    }
    // a submenu's back button goes up a level (a double chevron); a clip's clears the selection
    ToolRow(tools, back, deep = clip != null && (ed.submenu == Submenu.Retime || ed.submenu == Submenu.ClipAudio), modifier)
}

@Composable
private fun ToolRow(tools: List<Tool>, back: (() -> Unit)?, deep: Boolean, modifier: Modifier) {
    BoxWithConstraints(modifier.fillMaxWidth().padding(horizontal = 12.dp).height(FcSize.ToolTile + 12.dp), contentAlignment = Alignment.Center) {
        val backWidth = if (back != null) 44.dp else 0.dp
        val wanted = FcSize.ToolTile * tools.size + backWidth + 24.dp
        val fits = wanted <= maxWidth
        Row(
            Modifier.width(wanted.coerceAtMost(maxWidth)).height(FcSize.ToolTile + 12.dp)
                .editorGlass(36.dp)
                .horizontalScroll(rememberScrollState()).padding(horizontal = 12.dp),
            horizontalArrangement = if (fits) Arrangement.Center else Arrangement.Start,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (back != null) {
                io.github.artcraftandroid.filmcraft.ui.FcIconButton(
                    (if (deep) LucideIcons.ChevronsLeft else LucideIcons.ChevronLeft).glyph(),
                    "Back", back, modifier = Modifier.testTag("editor_toolbar_back"), size = 44.dp,
                )
            }
            for (t in tools) ToolTile(t.glyph, t.label, t.onClick, enabled = t.enabled, active = t.active, tag = t.tag)
        }
    }
}

private fun addTools(ed: EditorController) = listOf(
    Tool("editor_tool_overlay", "Overlay", LucideIcons.Layers.glyph()) { ed.addOverlay() },
    Tool("editor_tool_drawing", "Draw", LucideIcons.Paintbrush.glyph()) { ed.panel = Panel.Drawing },
    Tool("editor_tool_audio", "Audio", LucideIcons.Music.glyph()) { ed.submenu = Submenu.AddAudio },
    Tool("editor_tool_text", "Text", LucideIcons.Type.glyph()) { ed.panel = Panel.TextInput(null) },
    Tool("editor_tool_3d", "3D", Glyphs.Object3D) { ed.panel = Panel.Object3D },
    Tool("editor_tool_lights", "Lights", LucideIcons.Sun.glyph()) { ed.panel = Panel.Lights },
    Tool("editor_tool_captions", "Captions", LucideIcons.Captions.glyph(), enabled = ed.duration > 0) { ed.panel = Panel.Captions },
    Tool("editor_tool_files", "Files", LucideIcons.File.glyph()) { ed.submenu = Submenu.Files },
    Tool("editor_tool_blank", "Blank", LucideIcons.RectangleHorizontal.glyph()) { ed.addBlank() },
    Tool("editor_tool_aspect_ratio", "Aspect ratio", LucideIcons.Ratio.glyph()) { ed.panel = Panel.AspectRatio },
)

private fun addAudioTools(ed: EditorController) = listOf(
    Tool("editor_tool_online_sounds", "Online sounds", LucideIcons.Music.glyph()) { ed.submenu = Submenu.None; ed.panel = Panel.OnlineSounds },
    Tool("editor_tool_voiceover", "Voiceover", LucideIcons.Mic.glyph()) { ed.submenu = Submenu.None; ed.panel = Panel.Voiceover },
    Tool("editor_tool_text_to_speech", "Text to speech", LucideIcons.Speech.glyph()) { ed.submenu = Submenu.None; ed.panel = Panel.Speech(null) },
    Tool("editor_tool_extract_audio_file", "Extract audio", LucideIcons.AudioLines.glyph()) { ed.extractAudioFromFile() },
)

private fun filesTools(ed: EditorController) = listOf(
    Tool("editor_tool_files_online_sounds", "Online sounds", LucideIcons.Music.glyph()) { ed.submenu = Submenu.None; ed.panel = Panel.OnlineSounds },
    Tool("editor_tool_files_online_videos", "Online videos", LucideIcons.Film.glyph()) { ed.submenu = Submenu.None; ed.panel = Panel.OnlineVideos },
    Tool("editor_tool_files_overlay", "Overlay", LucideIcons.Layers.glyph()) { ed.addOverlay() },
    Tool("editor_tool_files_visual", "Images and videos", LucideIcons.Images.glyph()) { ed.addMedia(PickKind.FilesVisual) },
    Tool("editor_tool_files_audio", "Audio", LucideIcons.Music.glyph()) { ed.addMedia(PickKind.FilesAudio) },
)

/** Whether the playhead is far enough inside [c] to split it (two frames from each end). */
private fun canSplit(ed: EditorController, c: ClipInfo): Boolean {
    val margin = ed.frameTicks * 2
    return ed.playhead >= c.start + margin && ed.playhead <= c.start + c.duration - margin
}

private fun clipTools(ed: EditorController, c: ClipInfo): List<Tool> {
    val split = Tool("editor_tool_split", "Split", LucideIcons.Scissors.glyph(), enabled = canSplit(ed, c)) { ed.split() }
    val delete = Tool("editor_tool_delete", "Delete", LucideIcons.Trash2.glyph()) { ed.delete() }
    val duplicate = Tool("editor_tool_duplicate", "Duplicate", LucideIcons.CopyPlus.glyph()) { ed.duplicate() }
    val replace = Tool("editor_tool_replace", "Replace", LucideIcons.Replace.glyph()) { ed.replaceSelected() }
    val onMain = ed.isMain(c.id)
    val picture = listOfNotNull(
        Tool("editor_tool_filters", "Filters", LucideIcons.SwatchBook.glyph()) { ed.panel = Panel.Filters },
        if (onMain) Tool("editor_tool_effects", "Effects", LucideIcons.WandSparkles.glyph()) { ed.panel = Panel.Effects } else null,
        Tool("editor_tool_opacity", "Opacity", Glyphs.Checker) { ed.panel = Panel.Opacity },
        Tool("editor_tool_mask", "Mask", Glyphs.Mask, active = ed.panel == Panel.Mask) { ed.panel = Panel.Mask },
        Tool("editor_tool_background", "Remove BG", LucideIcons.WandSparkles.glyph(), active = ed.panel == Panel.Background) { ed.panel = Panel.Background },
        // Lucide names these by their axis line: "flip-vertical-2" has left/right arrows (a mirror)
        // one-tap toggles with no on/off look: the preview shows which way the picture faces
        Tool("editor_tool_flip_h", "Mirror", LucideIcons.FlipVertical2.glyph()) { ed.toggleMirror() },
        Tool("editor_tool_flip_v", "Flip", LucideIcons.FlipHorizontal2.glyph()) { ed.toggleFlip() },
    )
    val animation = Tool("editor_tool_keyframes", "Keyframes", LucideIcons.ChartSpline.glyph(), active = ed.panel == Panel.Keyframes) { ed.panel = Panel.Keyframes }
    return (if (c.kind != ClipKind.MISSING) listOf(animation) else emptyList()) + when (c.kind) {
        ClipKind.MISSING -> listOf(delete, replace)
        ClipKind.TEXT -> if (c.has3d) listOf(
            Tool("editor_tool_3d", "3D", Glyphs.Object3D, active = ed.panel == Panel.Object3D) { ed.panel = Panel.Object3D },
            split, delete, duplicate,
        ) else listOf(
            Tool("editor_tool_edit_text", "Edit text", LucideIcons.Keyboard.glyph()) { ed.panel = Panel.TextInput(c.id) },
            Tool("editor_tool_style", "Style", LucideIcons.Paintbrush.glyph()) { ed.panel = Panel.TextStyle },
            Tool("editor_tool_extrude", "Extrude", LucideIcons.Type.glyph()) { ed.extrudeText() },
            split, delete, duplicate,
        )
        ClipKind.AUDIO -> listOfNotNull(
            if (c.speech) Tool("editor_tool_edit_speech", "Edit speech", LucideIcons.Speech.glyph()) { ed.panel = Panel.Speech(c.id) } else null,
            split, delete,
            Tool("editor_tool_volume", "Volume", (if (c.muted) LucideIcons.VolumeX else LucideIcons.Volume2).glyph()) { ed.panel = Panel.Volume },
            Tool("editor_tool_fade", "Fade", LucideIcons.ChartSpline.glyph()) { ed.panel = Panel.Fade },
            duplicate, replace,
        )
        ClipKind.BLANK -> listOf(split, delete, duplicate, replace)
        ClipKind.IMAGE -> listOf(split, delete) + picture + listOf(duplicate, replace)
        ClipKind.VIDEO -> listOfNotNull(
            split, delete,
            Tool("editor_tool_retime_submenu", "Retime", LucideIcons.Timer.glyph()) { ed.submenu = Submenu.Retime },
            if (c.hasAudio) Tool("editor_tool_audio_submenu", "Audio", (if (c.muted) LucideIcons.VolumeX else LucideIcons.Volume2).glyph()) { ed.submenu = Submenu.ClipAudio } else null,
        ) + picture + listOf(
            Tool("editor_tool_track", "Track", LucideIcons.Timer.glyph(), active = ed.panel == Panel.Tracking) { ed.panel = Panel.Tracking },
            duplicate, replace,
        )
    }
}

private fun captionTools(ed: EditorController, id: ULong) = listOf(
    Tool("editor_tool_edit_caption", "Edit text", LucideIcons.Keyboard.glyph()) { ed.panel = Panel.TextInput(null, caption = id) },
    Tool("editor_tool_caption_style", "Style", LucideIcons.Paintbrush.glyph()) { ed.panel = Panel.Captions },
    Tool("editor_tool_delete_caption", "Delete", LucideIcons.Trash2.glyph()) { ed.deleteCaption(id) },
)

private fun retimeTools(ed: EditorController, c: ClipInfo) = listOf(
    Tool("editor_tool_speed", "Speed", LucideIcons.Gauge.glyph(), active = ed.panel == Panel.Speed) { ed.panel = Panel.Speed },
    Tool("editor_tool_freeze", "Freeze", LucideIcons.Snowflake.glyph(), enabled = ed.playhead >= c.start && ed.playhead < c.start + c.duration - ed.frameTicks) { ed.freeze() },
)

private fun clipAudioTools(ed: EditorController, c: ClipInfo) = listOf(
    Tool("editor_tool_volume", "Volume", (if (c.muted) LucideIcons.VolumeX else LucideIcons.Volume2).glyph(), active = ed.panel == Panel.Volume) { ed.panel = Panel.Volume },
    Tool("editor_tool_fade", "Fade", LucideIcons.ChartSpline.glyph(), active = ed.panel == Panel.Fade) { ed.panel = Panel.Fade },
    Tool("editor_tool_extract_audio", "Extract audio", LucideIcons.AudioLines.glyph(), enabled = c.speed < 5.0) { ed.extractAudio() },
)

@Suppress("unused")
private val minTile = Modifier.widthIn(min = FcSize.ToolTile)
