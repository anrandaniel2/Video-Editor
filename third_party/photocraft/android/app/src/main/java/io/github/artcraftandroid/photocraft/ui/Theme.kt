package io.github.artcraftandroid.photocraft.ui

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

/**
 * PhotoCraft's colours, after the reference tablet UI: near-black canvas surround, dark grey
 * toolbars and panels, white text, and a coloured pill for the active mode tab.
 */
object Pc {
    // surfaces
    val Black = Color(0xFF1C1C1C)
    val Toolbar = Color(0xFF2A2A2A)
    val Panel = Color(0xFF303030)
    val Raised = Color(0xFF414141)
    val Lane = Color(0xFF191919)
    val Hover = Color(0x1CFFFFFF)
    val Hairline = Color(0x24FFFFFF)
    val Scrim = Color(0x99000000)

    // text and icons
    val Title = Color(0xFFF2F2F2)
    val Body = Color(0xFFFFFFFF)
    val Secondary = Color(0xFFAFAFAF)
    val Detail = Color(0xFF8A8A8A)
    val Disabled = Color(0xFF444444)
    val Icon = Color(0xFFFFFFFF)

    // accent and modes (the top tabs' colours in the reference UI)
    val Accent = Color(0xFF0076FD)
    val AccentLight = Color(0xFF3294FF)
    val Negative = Color(0xFFFC432E)
    val ModeSketch = Color(0xFFF5C518)
    val ModeVector = Color(0xFF00C2D1)
    val ModePixel = Color(0xFF9B5DE5)
    val ModeLayout = Color(0xFFF15BB5)
    val ModeCanva = Color(0xFF00F5D4)

    // controls
    val SliderOn = Color(0xFF0076FD)
    val SliderOff = Color(0xFF484848)
    val ThumbFill = Color(0xFF0E0E0E)
    val ThumbRing = Color(0xFFEBEBEB)
    val PillButton = Color(0xFF484848)
    val Chip = Color(0xFF393939)
    val Divider = Color(0xFF444444)
}

private val DarkScheme = darkColorScheme(
    primary = Pc.Accent,
    onPrimary = Color.White,
    background = Pc.Black,
    surface = Pc.Panel,
    onBackground = Pc.Title,
    onSurface = Pc.Title,
)

@Composable
fun PhotoCraftTheme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = DarkScheme, content = content)
}
