package io.github.artcraftandroid.filmcraft.ui

import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.artcraftandroid.filmcraft.R

/** Inter, FilmCraft's UI face (OFL; copied from FilmCraft's assets at build time). */
val Inter = FontFamily(
    Font(R.font.inter_regular, FontWeight.Normal),
    Font(R.font.inter_medium, FontWeight.Medium),
    Font(R.font.inter_semibold, FontWeight.SemiBold),
    Font(R.font.inter_semibold, FontWeight.Bold),
)

/**
 * FilmCraft's colours. The app is always dark: black screens, dark grey panels, a blue accent,
 * the timeline's clips coloured by kind.
 */
object Fc {
    val Gallery = Color(0xFF212121)
    // surfaces
    val Black = Color(0xFF1C1C1C)
    /** Full-screen pages over the editor (export). */
    val Layer1 = Color(0xFF212121)
    /** Bottom panels, dialogs, menus, the text strip. */
    val Panel = Color(0xFF262626)
    /** Rows and cards on panels. */
    val Raised = Color(0xFF303030)
    /** The band behind the navigation buttons under a panel. */
    val PanelBand = Color(0xFF212121)
    /** Timeline lanes. */
    val Lane = Color(0xFF191919)
    val Hover = Color(0x1CFFFFFF)
    val Hairline = Color(0x24FFFFFF)
    val Scrim = Color(0x99000000)
    /** Behind a bottom sheet: a light dim, the page stays readable. */
    val SheetScrim = Color(0x52000000)

    // text and icons
    val Title = Color(0xFFF2F2F2)
    val Body = Color(0xFFFFFFFF)
    val Secondary = Color(0xFFAFAFAF)
    val Detail = Color(0xFF8A8A8A)
    val Disabled = Color(0xFF444444)
    val Icon = Color(0xFFFFFFFF)

    // accent
    val Accent = Color(0xFF0076FD)
    val AccentLight = Color(0xFF3294FF)
    val Negative = Color(0xFFFC432E)
    val Recording = Color(0xFFF83A2B)

    // controls
    val SliderOn = Color(0xFFD0D0D0)
    val SliderOff = Color(0xFF484848)
    val SliderDisabled = Color(0xFF323232)
    val ThumbFill = Color(0xFF0E0E0E)
    val ThumbRing = Color(0xFFEBEBEB)
    val PillButton = Color(0xFF484848)
    val Handle = Color(0x66CAC4D0)
    val Chip = Color(0xFF393939)
    val Divider = Color(0xFF444444)

    // timeline
    val RulerTick = Color(0xFF6D6D6D)
    val RulerLabel = Color(0xFF8A8A8A)
    val ClipRing = Color(0x2BFFFFFF)
    val Grip = Color(0xFF0E0E0E)
    val ThumbPlaceholder = Color(0xFF323232)
    val TransitionMark = Color(0x59FFFFFF)

    /** A clip kind's fill, ring and label/icon colours. */
    data class ClipColors(val fill: Color, val ring: Color, val content: Color)

    /** A video with sound (measured on the reference: navy, its waveform lane inside). */
    val VideoClip = ClipColors(Color(0xFF003041), Color(0xFF003041), Color(0xFF6BC7FF))
    /** A video without sound, or muted. */
    val SilentVideoClip = ClipColors(Color(0xFF323232), Color(0xFF323232), Color(0xFFEBEBEB))
    val BlankClip = ClipColors(Color(0xFF323232), ClipRing, Color(0xFF6BC7FF))
    val AudioClip = ClipColors(Color(0xFF00402D), Color(0xFF003426), Color(0xFFEBEBEB))
    val MutedClip = ClipColors(Color(0xFF323232), Color(0xFF323232), Color(0xFFEBEBEB))
    val TextClip = ClipColors(Color(0xFF4F005F), Color(0xFF4F005F), Color(0xFFF59FFF))
    val TextClipSelected = ClipColors(Color(0xFF660978), Color(0xFF660978), Color(0xFFF8BFFF))
    val CaptionClip = ClipColors(Color(0xFF4A3A00), Color(0xFF5C4A00), Color(0xFFFFE08A))
    val CaptionClipSelected = ClipColors(Color(0xFF5E4C00), Color(0xFF5E4C00), Color(0xFFFFEBB0))
    val MissingClip = ClipColors(Color(0xFF571107), Color(0xFF571107), Color(0xFFE68619))
    val RecordingClip = ClipColors(Recording, Color(0xFFC5170D), Color(0xFFEBEBEB))

    /** Waveform fill and outline, plain and selected. */
    data class WaveColors(val fill: Color, val line: Color, val fillSelected: Color, val lineSelected: Color)

    val AudioWave = WaveColors(Color(0xFF005F3C), Color(0xFF19D979), Color(0xFF007042), Color(0xFF4FEC9C))
    val VideoWave = WaveColors(Color(0xFF03638C), Color(0xFF6BC7FF), Color(0xFF0873A8), Color(0xFF98DBFF))
    val MutedWave = WaveColors(Color(0xFF6D6D6D), Color(0xFFD0D0D0), Color(0xFF8A8A8A), Color(0xFFEBEBEB))

    // motion
    val EaseOut = CubicBezierEasing(0.2f, 0f, 0f, 1f)
    val EaseInOut = CubicBezierEasing(0.4f, 0f, 0.2f, 1f)
}

/** FilmCraft's text styles: Inter, no tracking, the sizes the screens use. */
object FcType {
    private fun s(size: Int, weight: FontWeight, line: Float = 1.3f) =
        TextStyle(fontFamily = Inter, fontSize = size.sp, lineHeight = (size * line).sp, fontWeight = weight, letterSpacing = (-0.25).sp)

    val PanelTitle = s(20, FontWeight.Bold)
    val DialogTitle = s(20, FontWeight.Bold)
    val Body = s(16, FontWeight.Normal, 1.5f)
    val BodySmall = s(14, FontWeight.Normal, 1.5f)
    val Label = s(16, FontWeight.Normal)
    val Value = s(17, FontWeight.Normal)
    val ToolLabel = s(12, FontWeight.Normal, 1.5f)
    val TileLabel = s(14, FontWeight.Normal)
    val Caption = s(12, FontWeight.Medium)
    val Badge = s(11, FontWeight.Normal)
    val Ruler = s(14, FontWeight.Normal)
    val ClipLabel = s(11, FontWeight.Normal)
    val Header = s(20, FontWeight.Bold)
    val GalleryTitle = s(32, FontWeight.Bold, 1.15f)
    val Button = s(16, FontWeight.Bold)
}

/** Sizes shared by the screens. */
object FcSize {
    val TopBar = 48.dp
    val TopButton = 44.dp
    val Transport = 48.dp
    val FrameBar = 96.dp
    val ToolTile = 80.dp
    val ToolChip = 44.dp
    val PanelCorner = 20.dp
    val PanelInset = 12.dp
    val PanelPadding = 20.dp
    val MainTrack = 64.dp
    val Track = 48.dp
    val TrackGap = 2.dp
    val Ruler = 15.dp
    val RulerGap = 4.dp
}

@Composable
fun FilmcraftTheme(content: @Composable () -> Unit) {
    val colors = darkColorScheme(
        primary = Fc.Accent,
        onPrimary = Color.White,
        background = Fc.Black,
        onBackground = Fc.Body,
        surface = Fc.Panel,
        onSurface = Fc.Body,
        surfaceVariant = Fc.Raised,
        onSurfaceVariant = Fc.Secondary,
        surfaceContainer = Fc.Panel,
        surfaceContainerHigh = Fc.Panel,
        error = Fc.Negative,
    )
    io.github.artcraftandroid.procraft.ui.ProCraftTheme {
        MaterialTheme(
            colorScheme = colors,
            shapes = androidx.compose.material3.Shapes(
                small = androidx.compose.foundation.shape.RoundedCornerShape(10.dp),
                medium = androidx.compose.foundation.shape.RoundedCornerShape(16.dp),
                large = androidx.compose.foundation.shape.RoundedCornerShape(20.dp),
                extraLarge = androidx.compose.foundation.shape.RoundedCornerShape(24.dp),
            ),
            content = content,
        )
    }
}
