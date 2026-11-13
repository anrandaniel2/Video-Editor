package io.github.artcraftandroid.filmcraft.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Snackbar
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.unit.dp
import io.github.artcraftandroid.filmcraft.ui.editor.EditorScreen
import io.github.artcraftandroid.filmcraft.ui.export.ExportScreen
import io.github.artcraftandroid.filmcraft.ui.home.AboutScreen
import io.github.artcraftandroid.filmcraft.ui.home.HomeScreen
import io.github.artcraftandroid.filmcraft.ui.home.LicencesScreen
import io.github.artcraftandroid.filmcraft.ui.home.SettingsScreen

/** The app: the current screen, and messages along the bottom. */
@OptIn(ExperimentalComposeUiApi::class)
@Composable
fun AppRoot(vm: AppViewModel) {
    val context = androidx.compose.ui.platform.LocalContext.current
    var crashNotice by androidx.compose.runtime.remember { androidx.compose.runtime.mutableStateOf(io.github.artcraftandroid.filmcraft.CrashReports.hasReport(context)) }
    if (crashNotice) androidx.compose.material3.AlertDialog(
        onDismissRequest = { crashNotice = false },
        title = { Text("Crash report available") },
        text = { Text("The last error was saved on this device. Share it to help identify the exact failure. Nothing is sent automatically.") },
        confirmButton = { androidx.compose.material3.TextButton(onClick = { crashNotice = false; io.github.artcraftandroid.filmcraft.CrashReports.share(context) }) { Text("Share report") } },
        dismissButton = { androidx.compose.material3.TextButton(onClick = { crashNotice = false }) { Text("Not now") } },
    )
    val snackbars = remember { SnackbarHostState() }
    LaunchedEffect(Unit) {
        for (m in vm.messages) snackbars.showSnackbar(m)
    }
    // test tags double as resource ids, so on-device checks can find controls by name
    Box(Modifier.fillMaxSize().background(Fc.Black).semantics { testTagsAsResourceId = true }) {
        val ed = vm.editor
        when (vm.screen) {
            Screen.Home -> HomeScreen(vm)
            Screen.Editor -> if (ed != null) EditorScreen(ed) else HomeScreen(vm)
            Screen.Export -> ExportScreen(vm)
            Screen.Settings -> SettingsScreen(vm)
            Screen.About -> AboutScreen(vm)
            Screen.Licences -> LicencesScreen(vm)
        }
        SnackbarHost(snackbars, Modifier.align(Alignment.BottomCenter).navigationBarsPadding().padding(bottom = 96.dp)) { data ->
            Snackbar(shape = RoundedCornerShape(8.dp), containerColor = Fc.Raised, contentColor = Fc.Title) {
                Text(data.visuals.message, style = FcType.BodySmall, color = Fc.Title)
            }
        }
    }
}
