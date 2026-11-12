package io.github.artcraftandroid.filmcraft.ui.home

import android.content.Intent
import android.net.Uri
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import io.github.artcraftandroid.filmcraft.R
import io.github.artcraftandroid.filmcraft.core.ModelPackage
import io.github.artcraftandroid.filmcraft.core.engineVersion
import io.github.artcraftandroid.filmcraft.core.speechModels
import io.github.artcraftandroid.filmcraft.core.voicePackage
import io.github.artcraftandroid.filmcraft.engine.ModelDownloads
import io.github.artcraftandroid.filmcraft.engine.megabytes
import io.github.artcraftandroid.filmcraft.ui.DialogButton
import io.github.artcraftandroid.filmcraft.ui.FcDialog
import java.io.File
import io.github.artcraftandroid.filmcraft.ui.AppViewModel
import io.github.artcraftandroid.filmcraft.ui.Fc
import io.github.artcraftandroid.filmcraft.ui.FcIconButton
import io.github.artcraftandroid.filmcraft.ui.FcType
import io.github.artcraftandroid.filmcraft.ui.GlyphIcon
import io.github.artcraftandroid.filmcraft.ui.LucideIcons
import io.github.artcraftandroid.filmcraft.ui.Screen
import io.github.artcraftandroid.filmcraft.ui.glyph
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

private const val SOURCE_URL = "https://github.com/storytold/filmcraft"

@Composable
private fun Page(title: String, onClose: () -> Unit, tag: String, content: @Composable () -> Unit) {
    BackHandler(onBack = onClose)
    Column(Modifier.fillMaxSize().background(Fc.Black).statusBarsPadding().navigationBarsPadding().testTag(tag)) {
        Row(Modifier.fillMaxWidth().height(56.dp).padding(horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            FcIconButton(LucideIcons.X.glyph(), "Close", onClose)
        }
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = 16.dp)) {
            Text(title, style = FcType.PanelTitle.copy(fontSize = FcType.PanelTitle.fontSize * 1.3f), color = Fc.Title)
            Spacer(Modifier.height(24.dp))
            content()
            Spacer(Modifier.height(24.dp))
        }
    }
}

@Composable
private fun Row48(label: String, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 48.dp).clip(RoundedCornerShape(12.dp)).clickable(role = Role.Button, onClick = onClick).padding(horizontal = 12.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, style = FcType.Body, color = Fc.Body, modifier = Modifier.weight(1f))
        GlyphIcon(LucideIcons.ChevronRight.glyph(), Fc.Detail, size = 20.dp)
    }
}

/** What a downloaded package is for, in the user's words. */
private fun packageLabel(id: String) = when (id) {
    "whisper-tiny" -> "Captions: faster speech model"
    "whisper-base" -> "Captions: more accurate speech model"
    else -> "Natural voices"
}

/** Bytes a package's folder holds now (finished and partly downloaded files). */
private fun bytesOnDisk(p: ModelPackage): Long = File(p.dir).listFiles()?.sumOf { if (it.isFile) it.length() else 0L } ?: 0L

@Composable
fun SettingsScreen(vm: AppViewModel) {
    var refresh by remember { mutableIntStateOf(0) }
    var asking by remember { mutableStateOf<ModelPackage?>(null) }
    val downloading = ModelDownloads.state?.id
    val stored by produceState(emptyList<Pair<ModelPackage, Long>>(), refresh, downloading) {
        value = withContext(Dispatchers.IO) {
            runCatching { (speechModels() + voicePackage()).map { it to bytesOnDisk(it) }.filter { it.second > 0 } }.getOrDefault(emptyList())
        }
    }
    Page("Settings", { vm.back() }, "screen_settings") {
        Row48("About FilmCraft") { vm.show(Screen.About) }
        if (stored.isNotEmpty()) {
            Spacer(Modifier.height(24.dp))
            Text("Downloaded for captions and speech", style = FcType.Label, color = Fc.Secondary, modifier = Modifier.padding(horizontal = 12.dp))
            Spacer(Modifier.height(8.dp))
            for ((p, bytes) in stored) {
                Row(Modifier.fillMaxWidth().heightIn(min = 56.dp).padding(horizontal = 12.dp).testTag("settings_model_${p.id}"), verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text(packageLabel(p.id), style = FcType.Body, color = Fc.Body)
                        Text(megabytes(bytes), style = FcType.BodySmall, color = Fc.Detail)
                    }
                    if (downloading != p.id) DialogButton("Delete", { asking = p }, filled = false)
                }
            }
        }
    }
    asking?.let { p ->
        FcDialog(
            "Delete ${packageLabel(p.id).lowercase().replaceFirstChar { it.uppercase() }}?",
            "Delete",
            onConfirm = {
                asking = null
                File(p.dir).deleteRecursively()
                refresh++
            },
            onDismiss = { asking = null },
            destructive = true,
        ) {
            Text("It's downloaded again the next time it's needed.", style = FcType.Body, color = Fc.Body)
        }
    }
}

@Composable
fun AboutScreen(vm: AppViewModel) {
    val ctx = LocalContext.current
    Page("About FilmCraft", { vm.back() }, "screen_about") {
        Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(Fc.Layer1).padding(16.dp)) {
            Image(painterResource(R.drawable.ic_launcher_tile), contentDescription = "FilmCraft", modifier = Modifier.size(56.dp).clip(RoundedCornerShape(12.dp)))
            Spacer(Modifier.height(12.dp))
            Text("FilmCraft for Android", style = FcType.Header, color = Fc.Title)
            Text("Version ${vm.appVersion} (engine ${engineVersion()})", style = FcType.BodySmall, color = Fc.Detail)
            Spacer(Modifier.height(12.dp))
            Text(
                "Built on FilmCraft's Rust engine. MIT OR Apache-2.0. FilmCraft is not affiliated with Adobe; Premiere is a trademark of Adobe Inc.",
                style = FcType.BodySmall,
                color = Fc.Body,
            )
            Spacer(Modifier.height(12.dp))
            HorizontalDivider(color = Fc.Hairline, thickness = 1.dp)
            Row48("Source code") { runCatching { ctx.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(SOURCE_URL))) } }
            Row48("Open-source licences") { vm.show(Screen.Licences) }
        }
    }
}

@Composable
fun LicencesScreen(vm: AppViewModel) {
    val ctx = LocalContext.current
    val texts by produceState(emptyList<Pair<String, String>>()) {
        value = withContext(Dispatchers.IO) {
            val names = runCatching { ctx.assets.list("licences")?.toList() }.getOrNull().orEmpty()
            names.mapNotNull { n -> runCatching { n to ctx.assets.open("licences/$n").bufferedReader().readText() }.getOrNull() }
        }
    }
    Page("Open-source licences", { vm.back() }, "screen_licences") {
        val parts = listOf(
            "FilmCraft engine" to "MIT OR Apache-2.0, the FilmCraft contributors",
            "Inter (UI font)" to "SIL Open Font License 1.1, the Inter Project Authors",
            "Lucide (icons)" to "ISC, Lucide Contributors",
            "UniFFI" to "MPL-2.0, Mozilla",
            "JNA" to "Apache-2.0",
            "AndroidX, Jetpack Compose, Kotlin coroutines" to "Apache-2.0",
            "Haze (live backdrop blur)" to "Apache-2.0, Christopher Banes and Haze contributors",
            "candle (speech and voices on the phone)" to "MIT OR Apache-2.0, Hugging Face",
            "Whisper speech models (downloaded when asked)" to "MIT, OpenAI",
            "Kokoro-82M natural voices (downloaded when asked)" to "Apache-2.0, hexgrad; CMUdict: BSD-style, Carnegie Mellon University",
        )
        for ((name, licence) in parts) {
            Text(name, style = FcType.Label, color = Fc.Title)
            Text(licence, style = FcType.BodySmall, color = Fc.Detail)
            Spacer(Modifier.height(12.dp))
        }
        for ((name, text) in texts) {
            HorizontalDivider(color = Fc.Hairline, thickness = 1.dp)
            Spacer(Modifier.height(12.dp))
            Text(name, style = FcType.Label, color = Fc.Title)
            Spacer(Modifier.height(8.dp))
            Text(text, style = FcType.BodySmall, color = Fc.Detail)
            Spacer(Modifier.height(12.dp))
        }
    }
}
