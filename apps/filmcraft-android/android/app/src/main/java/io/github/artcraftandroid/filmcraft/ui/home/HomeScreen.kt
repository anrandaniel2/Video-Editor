package io.github.artcraftandroid.filmcraft.ui.home

import io.github.artcraftandroid.filmcraft.ui.editorCard
import android.graphics.BitmapFactory
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.material3.FilterChip
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.layout.displayCutout
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.union
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Text
import androidx.compose.runtime.DisposableEffect
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.artcraftandroid.filmcraft.R
import io.github.artcraftandroid.filmcraft.engine.ProjectEntry
import io.github.artcraftandroid.filmcraft.ui.AppViewModel
import io.github.artcraftandroid.filmcraft.ui.Fc
import io.github.artcraftandroid.filmcraft.ui.FcDialog
import io.github.artcraftandroid.filmcraft.ui.FcIconButton
import io.github.artcraftandroid.filmcraft.ui.FcType
import io.github.artcraftandroid.filmcraft.ui.Glyph
import io.github.artcraftandroid.filmcraft.ui.GlyphIcon
import io.github.artcraftandroid.filmcraft.ui.LucideIcons
import io.github.artcraftandroid.filmcraft.ui.NewProject
import io.github.artcraftandroid.filmcraft.ui.Screen
import io.github.artcraftandroid.filmcraft.ui.glyph
import io.github.artcraftandroid.filmcraft.ui.editor.TPS
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** Home: ways to start a project, the projects, and the + button. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun HomeScreen(vm: AppViewModel) {
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    DisposableEffect(lifecycle) {
        val observer = LifecycleEventObserver { _, event -> if (event == Lifecycle.Event.ON_RESUME) vm.refreshProjects() }
        lifecycle.addObserver(observer)
        onDispose { lifecycle.removeObserver(observer) }
    }
    var drawingSetup by remember { mutableStateOf(false) }
    var filter by remember { mutableStateOf("all") }
    var newMenu by remember { mutableStateOf(false) }
    if (drawingSetup) DrawingCanvasDialog({ drawingSetup = false }) { p -> drawingSetup = false; vm.createDrawing(p.width, p.height, p) }
    Box(Modifier.fillMaxSize().background(Fc.Gallery).windowInsetsPadding(WindowInsets.displayCutout.union(WindowInsets.navigationBars).only(WindowInsetsSides.Horizontal)).testTag("screen_home")) {
        LazyVerticalGrid(
            columns = GridCells.Adaptive(190.dp),
            modifier = Modifier.fillMaxSize().statusBarsPadding().testTag("home_projects_list"),
            contentPadding = PaddingValues(start = 24.dp, end = 24.dp, bottom = 120.dp),
            horizontalArrangement = Arrangement.spacedBy(24.dp),
            verticalArrangement = Arrangement.spacedBy(24.dp),
        ) {
            item(span = { GridItemSpan(maxLineSpan) }) { HomeTopBar(vm) }
            item(span = { GridItemSpan(maxLineSpan) }) {
                Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    QuickTile(LucideIcons.Images.glyph(), "Import picture", "from_your_photo_library") { vm.importPicture() }
                    QuickTile(LucideIcons.FilePlus.glyph(), "From files", "new_from_files") { vm.newProject(NewProject.Files) }
                    QuickTile(LucideIcons.Paintbrush.glyph(), "New drawing", "create_drawing") { drawingSetup = true }
                    QuickTile(LucideIcons.SquarePlus.glyph(), "New video", "new_blank_project") { vm.newProject(NewProject.Blank) }
                    QuickTile(LucideIcons.AudioLines.glyph(), "Extract audio", "extract_audio_button") { vm.newProject(NewProject.ExtractAudio) }
                }
            }
            item(span = { GridItemSpan(maxLineSpan) }) {
                Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    for ((kind, title) in listOf("all" to "All", "video" to "Videos", "drawing" to "Drawings", "picture" to "Pictures")) {
                        io.github.artcraftandroid.filmcraft.ui.SelectChip(title, filter == kind, { filter = kind })
                    }
                }
            }
            val entries = vm.projectList.filter { filter == "all" || it.kind == filter }
            items(entries, key = { it.id }) { p -> ProjectCard(vm, p) }
            if (entries.isEmpty()) item(span = { GridItemSpan(maxLineSpan) }) {
                Column(Modifier.fillMaxWidth().padding(vertical = 48.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                    GlyphIcon(LucideIcons.Images.glyph(), Fc.Secondary, size = 40.dp)
                    Spacer(Modifier.height(16.dp))
                    Text("Your creative library", style = FcType.Header, color = Fc.Title)
                    Text("Videos, layered drawings and saved pictures appear here.", style = FcType.BodySmall, color = Fc.Secondary, textAlign = TextAlign.Center)
                }
            }
            vm.engineProblem?.let { problem -> item(span = { GridItemSpan(maxLineSpan) }) {
                Text("The editor couldn't start: $problem", style = FcType.Body, color = Fc.Negative)
            } }
        }
        // the + button
        Box(
            Modifier
                .align(Alignment.BottomEnd)
                .navigationBarsPadding()
                .padding(16.dp)
                .size(56.dp)
                .clip(RoundedCornerShape(16.dp))
                .background(Fc.Accent)
                .clickable(role = Role.Button) { newMenu = true }
                .testTag("new_project")
                .semantics { contentDescription = "New project" },
            contentAlignment = Alignment.Center,
        ) {
            GlyphIcon(LucideIcons.Plus.glyph(), Color.White, size = 26.dp)
            DropdownMenu(newMenu, { newMenu = false }, containerColor = Fc.Panel) {
                DropdownMenuItem(text = { Text("New drawing") }, onClick = { newMenu = false; drawingSetup = true })
                DropdownMenuItem(text = { Text("New video") }, onClick = { newMenu = false; vm.newProject(NewProject.Blank) })
                DropdownMenuItem(text = { Text("Import picture") }, onClick = { newMenu = false; vm.importPicture() })
                DropdownMenuItem(text = { Text("Import picture from files") }, onClick = { newMenu = false; vm.importPicture(fromFiles = true) })
                DropdownMenuItem(text = { Text("Import photos and videos") }, onClick = { newMenu = false; vm.newProject(NewProject.PhotoLibrary) })
                DropdownMenuItem(text = { Text("Import files") }, onClick = { newMenu = false; vm.newProject(NewProject.Files) })
            }
        }
        if (vm.opening) {
            Box(Modifier.fillMaxSize().background(Fc.Scrim), contentAlignment = Alignment.Center) { CircularProgressIndicator(color = Fc.Accent) }
        }
    }
    vm.renaming?.let { RenameDialog(vm, it) }
    vm.deleting?.let { p ->
        FcDialog("“${p.name}”?", "Delete", { vm.delete(p) }, { vm.deleting = null }, destructive = true) {
            Text(if (p.kind == "picture") "The saved picture will be deleted from Photos and Projects." else if (p.kind == "drawing") "The layered drawing will be deleted. Exported photos are kept." else "The project will be deleted from this phone.", style = FcType.Body, color = Fc.Body)
        }
    }
}

@Composable
private fun HomeTopBar(vm: AppViewModel) {
    var menu by remember { mutableStateOf(false) }
    Row(Modifier.fillMaxWidth().height(88.dp).padding(horizontal = 4.dp).testTag("home_screen_top_bar"), verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.padding(vertical = 8.dp)) {
            Text("Projects", style = FcType.GalleryTitle, color = Fc.Title, modifier = Modifier.testTag("home_your_projects_header"))
            Text("YOUR CREATIVE LIBRARY", style = FcType.Caption, color = Fc.Secondary)
        }
        Spacer(Modifier.weight(1f))
        Box {
            FcIconButton(LucideIcons.Ellipsis.glyph(), "More options", { menu = true }, size = 48.dp, modifier = Modifier.testTag("more_options"))
            DropdownMenu(menu, { menu = false }, containerColor = Fc.Panel, shape = RoundedCornerShape(12.dp)) {
                DropdownMenuItem(
                    text = { Text("Settings", style = FcType.Label, color = Fc.Body) },
                    leadingIcon = { GlyphIcon(LucideIcons.Settings.glyph(), Fc.Icon, size = 20.dp) },
                    onClick = { menu = false; vm.show(Screen.Settings) },
                )
            }
        }
    }
}

@Composable
private fun QuickTile(glyph: Glyph, label: String, tag: String, onClick: () -> Unit) {
    Row(Modifier.clip(RoundedCornerShape(12.dp)).editorCard(18.dp).clickable(role = Role.Button, onClick = onClick)
        .padding(horizontal = 14.dp, vertical = 12.dp).testTag(tag), verticalAlignment = Alignment.CenterVertically) {
        GlyphIcon(glyph, Fc.Icon, size = 20.dp)
        Spacer(Modifier.width(8.dp))
        Text(label, style = FcType.Label, color = Fc.Body, maxLines = 1)
    }
}

/** A project's length on its card: whole seconds rounded up (":07", "1:05", "1:02:03"). */
fun cardDuration(ticks: Long): String {
    val s = (ticks + TPS - 1) / TPS
    val h = s / 3600
    val m = (s / 60) % 60
    val sec = s % 60
    return when {
        h > 0 -> "%d:%02d:%02d".format(h, m, sec)
        m > 0 -> "%d:%02d".format(m, sec)
        else -> ":%02d".format(sec)
    }
}

@Composable
private fun ProjectCard(vm: AppViewModel, p: ProjectEntry) {
    val context = androidx.compose.ui.platform.LocalContext.current
    var menu by remember { mutableStateOf(false) }
    val poster by produceState<ImageBitmap?>(null, p.id, p.opened) {
        value = withContext(Dispatchers.IO) { runCatching {
            if (p.sourceUri != null) {
                android.graphics.ImageDecoder.decodeBitmap(android.graphics.ImageDecoder.createSource(context.contentResolver, android.net.Uri.parse(p.sourceUri))) { decoder, info, _ ->
                    decoder.allocator = android.graphics.ImageDecoder.ALLOCATOR_SOFTWARE
                    val scale = minOf(1f, 400f / maxOf(info.size.width, info.size.height))
                    decoder.setTargetSize((info.size.width * scale).toInt().coerceAtLeast(1), (info.size.height * scale).toInt().coerceAtLeast(1))
                }.asImageBitmap()
            } else BitmapFactory.decodeFile(p.poster.absolutePath)?.asImageBitmap()
        }.getOrNull() }
    }
    Column(Modifier.fillMaxWidth()) {
        Box(
            Modifier
                .fillMaxWidth().aspectRatio(1f)
                .clip(RoundedCornerShape(12.dp))
                .editorCard(18.dp)
                .clickable(role = Role.Button) { vm.openProject(p) }
                .testTag("home_project_${p.id}"),
        ) {
            poster?.let { Image(it, null, Modifier.fillMaxSize(), contentScale = ContentScale.Crop) }
            Box(
                Modifier.align(Alignment.TopEnd).padding(6.dp).clip(CircleShape).background(Fc.Layer1.copy(alpha = 0.7f)).padding(horizontal = 6.dp, vertical = 1.dp),
            ) {
                Text(if (p.kind == "video") cardDuration(p.duration) else if (p.kind == "drawing") "Drawing" else "Picture", style = FcType.Badge, color = Fc.Title)
            }
            Box(Modifier.align(Alignment.BottomEnd).padding(4.dp)) {
                Box(
                    Modifier
                        .size(30.dp)
                        .clip(CircleShape)
                        .background(Fc.Layer1.copy(alpha = 0.7f))
                        .clickable(role = Role.Button) { menu = true }
                        .semantics { contentDescription = "More options" },
                    contentAlignment = Alignment.Center,
                ) {
                    GlyphIcon(LucideIcons.Ellipsis.glyph(), Fc.Title, size = 18.dp)
                }
                DropdownMenu(menu, { menu = false }, containerColor = Fc.Panel, shape = RoundedCornerShape(12.dp)) {
                    DropdownMenuItem(text = { Text("Rename", style = FcType.Label, color = Fc.Body) }, leadingIcon = { GlyphIcon(LucideIcons.Pencil.glyph(), Fc.Icon, size = 20.dp) }, onClick = { menu = false; vm.renaming = p })
                    DropdownMenuItem(text = { Text("Duplicate", style = FcType.Label, color = Fc.Body) }, leadingIcon = { GlyphIcon(LucideIcons.CopyPlus.glyph(), Fc.Icon, size = 20.dp) }, onClick = { menu = false; vm.duplicate(p) })
                    DropdownMenuItem(text = { Text("Delete", style = FcType.Label, color = Fc.Negative) }, leadingIcon = { GlyphIcon(LucideIcons.Trash2.glyph(), Fc.Negative, size = 20.dp) }, onClick = { menu = false; vm.deleting = p })
                }
            }
        }
        Spacer(Modifier.height(5.dp))
        Text(p.name, style = FcType.Label, color = Fc.Title, maxLines = 1, overflow = TextOverflow.Ellipsis)
        Text("${p.width} × ${p.height}", style = FcType.Caption, color = Fc.Detail)
    }
}

@Composable
private fun RenameDialog(vm: AppViewModel, p: ProjectEntry) {
    var name by remember(p.id) { mutableStateOf(p.name) }
    val ok = name.trim().isNotEmpty() && name.trim() != p.name
    FcDialog("Rename ${if (p.kind == "video") "project" else if (p.kind == "drawing") "drawing" else "picture"}", "Save", { if (ok) vm.rename(p, name) }, { vm.renaming = null }, confirmEnabled = ok) {
        Text("Name", style = FcType.Caption, color = Fc.Detail)
        Spacer(Modifier.height(8.dp))
        androidx.compose.foundation.text.BasicTextField(
            value = name,
            onValueChange = { name = it },
            singleLine = true,
            textStyle = FcType.Body.copy(color = Fc.Title),
            cursorBrush = androidx.compose.ui.graphics.SolidColor(Fc.Title),
            modifier = Modifier.fillMaxWidth().border(1.dp, Fc.Divider, RoundedCornerShape(12.dp)).padding(horizontal = 14.dp, vertical = 12.dp),
        )
    }
}
