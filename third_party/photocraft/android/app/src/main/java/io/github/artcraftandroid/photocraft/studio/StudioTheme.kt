package io.github.artcraftandroid.photocraft.studio

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * The studio's colours and metrics: the reference UI's own values, taken from its stylesheet
 * (`--chrome: #2c2d2b`, `--blue: #278cfa`, the four workspace accents, the panel gradient), so the
 * port is the same design rather than an interpretation of it.
 */
object Sc {
    // Chrome and surfaces
    val Backdrop = Color(0xFFBFC6CA)
    val Chrome = Color(0xFF2C2D2B)
    /** The chrome gradient's two ends (135°), as the floating panels use it. */
    val SurfaceTop = Color(0xFB30312F)
    val SurfaceBottom = Color(0xFB262725)
    val Hairline = Color(0x1A111111)
    val Divider = Color(0x15FFFFFF)
    val Sunken = Color(0xFF20211F)
    val SunkenDark = Color(0xFF1D1E1B)
    val Lane = Color(0xFF222321)

    // Ink
    val Detail = Color(0xFFC6C7C3)
    val Title = Color(0xFFD2D3CE)
    val Muted = Color(0xFF92978C)
    val Faint = Color(0xFF858D7A)
    val Disabled = Color(0xFF7F827A)

    // Accents
    val Blue = Color(0xFF278CFA)
    val BlueLight = Color(0xFF79BAFF)
    val AccentLight = Color(0xFF8ABCED)
    val Negative = Color(0xFFC73536)

    // The workspace accents, as the persona tabs paint them.
    val Sketch = Color(0xFFE8F633)
    val Vector = Color(0xFF55E3F9)
    val Pixel = Color(0xFFDD9CFB)
    val Layout = Color(0xFFFFAB89)

    // Controls: the reference's own input colours.
    val IconInk = Color(0xFFC6C7C3)
    val ThumbInk = Color(0xFF7F827A)
    val Faded = Color(0xFF858D7A)
    val Field = Color(0xFF20211F)
    /// A slider's value pill: Procraft's StudioPill in its dark theme.
    val PillFill = Color(0xFF292929)
    val FieldInk = Color(0xFFAAACA5)
    val FieldIcon = Color(0xFFA0A29B)
    val Grip = Color(0xFFA0A19B)
    val SliderTrack = Color(0xFF3A3D38)
    val Note = Color(0xFF92978C)
    val Footer = Color(0xFF888B82)
    val ButtonPrimary = Color(0xFF328FFF)
    val ButtonPrimaryEdge = Color(0xFF62AAFF)

    // Metrics
    val RailWidth = 42.dp
    val ToolHeight = 30.dp
    val PanelWidth = 238.dp
    val PanelRadius = 17.dp
    val RailRadius = 18.dp
    val Pill = RoundedCornerShape(13.dp)
    val Chip = RoundedCornerShape(7.dp)
    val Sheet = RoundedCornerShape(16.dp)

    /** The chrome surface's gradient, top-left to bottom-right. */
    val surfaceBrush: androidx.compose.ui.graphics.Brush
        get() = androidx.compose.ui.graphics.Brush.linearGradient(
            colors = listOf(SurfaceTop, SurfaceBottom),
            start = androidx.compose.ui.geometry.Offset.Zero,
            end = androidx.compose.ui.geometry.Offset.Infinite,
        )

    /**
     * The persona accent, per workspace: what focus rings and the selected tab use. Kept as a
     * plain value the theme passes down (the reference's `--persona-accent`).
     */
    fun accentOf(id: WorkspaceId): Color = when (id) {
        WorkspaceId.Sketch -> Sketch
        WorkspaceId.Vector -> Vector
        WorkspaceId.Pixel -> Pixel
        WorkspaceId.Layout -> Layout
    }

    /** The viewport behind the document: the reference's `.artwork-*` backgrounds. */
    fun artcraftBackdrop(id: WorkspaceId): Color = artworkBackdrop(id)

    /** The document's backdrop, per workspace (the reference's `.artwork-*` backgrounds). */
    fun artworkBackdrop(id: WorkspaceId): Color = when (id) {
        WorkspaceId.Sketch -> Color.White
        WorkspaceId.Vector -> Color(0xFFBFC6CA)
        WorkspaceId.Pixel -> Color(0xFFD5B9B1)
        WorkspaceId.Layout -> Color(0xFF8CB0F5)
    }
}

private val typography = Typography(
    // The reference's chrome text is small and close: 12 sp labels, 11 sp fields, 10 sp rows.
    bodyLarge = TextStyle(fontFamily = FontFamily.SansSerif, fontSize = 13.sp, color = Sc.Title),
    bodyMedium = TextStyle(fontFamily = FontFamily.SansSerif, fontSize = 12.sp, color = Sc.Title),
    bodySmall = TextStyle(fontFamily = FontFamily.SansSerif, fontSize = 11.sp, color = Sc.Detail),
    labelSmall = TextStyle(fontFamily = FontFamily.SansSerif, fontSize = 10.sp, color = Sc.Muted),
)

/** The theme the studio runs in. */
@Composable
fun StudioTheme(accent: Color, content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = darkColorScheme(
            primary = Sc.Blue,
            onPrimary = Color.White,
            background = Sc.Chrome,
            onBackground = Sc.Title,
            surface = Sc.Chrome,
            onSurface = Sc.Title,
            error = Sc.Negative,
        ),
        typography = typography,
        content = content,
    )
}

/** Text weights the reference uses by name. */
object Weights {
    val Section = FontWeight.Medium
    val Strong = FontWeight.SemiBold
    val Tab = FontWeight(650)
}
