package io.github.artcraftandroid.photocraft.standalone

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.lifecycleScope
import io.github.artcraftandroid.photocraft.editor.PhotoEditorScreen
import io.github.artcraftandroid.photocraft.editor.PhotoEditorViewModel
import io.github.artcraftandroid.photocraft.ui.PhotoCraftTheme
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * The standalone PhotoCraft app: the editor, with the system pickers for opening and sharing.
 * FilmCraft hosts the same editor as a library (see FILMCRAFT-INTEGRATION.md); the library
 * module has no activity of its own, so this one lives in the app module on top of it.
 */
class MainActivity : ComponentActivity() {
    private val vm: PhotoEditorViewModel by viewModels()

    private val openPicker = registerForActivityResult(ActivityResultContracts.OpenDocument()) { uri: Uri? ->
        if (uri != null) open(uri)
    }

    /** Read a picture and hand it to the editor; a failure is the editor's to show. */
    private fun open(uri: Uri) {
        val name = contentResolver.query(uri, null, null, null, null)?.use { c ->
            val i = c.getColumnIndex(android.provider.OpenableColumns.DISPLAY_NAME)
            if (i >= 0 && c.moveToFirst()) c.getString(i) else "photo"
        } ?: "photo"
        val bytes = runCatching { contentResolver.openInputStream(uri)?.use { it.readBytes() } }.getOrNull()
        if (bytes != null) vm.openBytes(name, bytes)
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            PhotoCraftTheme {
                Box(Modifier.fillMaxSize()) {
                    PhotoEditorScreen(
                        vm,
                        onExport = { bytes, format -> shareBytes(bytes, format) },
                    )
                    // With no photo shared in, the editor would open on an empty canvas and
                    // nothing in the UI would load one: the app's own Open action does.
                    if (vm.status?.active == null) {
                        Button(
                            onClick = { openPicker.launch(arrayOf("image/*")) },
                            modifier = Modifier.align(Alignment.BottomCenter).padding(bottom = 96.dp),
                        ) {
                            Text("Open a photo")
                        }
                    }
                }
            }
        }
        // Open a photo if the intent carries one (a share, or a file the user tapped).
        val shared = intent?.let { i ->
            if (android.os.Build.VERSION.SDK_INT >= 33) {
                i.getParcelableExtra(Intent.EXTRA_STREAM, Uri::class.java)
            } else {
                @Suppress("DEPRECATION")
                i.getParcelableExtra(Intent.EXTRA_STREAM)
            }
        }
        (intent?.data ?: shared)?.let(::open)
    }

    /** Share the exported bytes as the file the engine encoded (PNG, JPEG, WebP, TIFF, …). */
    private fun shareBytes(bytes: ByteArray, format: String) {
        val extension = if (format == "jpeg") "jpg" else format
        val mime = when (format) {
            "jpeg", "jpg" -> "image/jpeg"
            "webp" -> "image/webp"
            "tiff" -> "image/tiff"
            "gif" -> "image/gif"
            "bmp" -> "image/bmp"
            "psd" -> "image/vnd.adobe.photoshop"
            else -> "image/png"
        }
        // The bytes are written off the main thread: a large export would stall the UI.
        val file = cacheDir.resolve("photocraft-export.$extension")
        lifecycleScope.launch {
            val written = withContext(Dispatchers.IO) { runCatching { file.writeBytes(bytes) }.isSuccess }
            if (!written) {
                vm.showError("The export couldn't be written.")
                return@launch
            }
            val uri = androidx.core.content.FileProvider.getUriForFile(this@MainActivity, "$packageName.fileprovider", file)
            val share = Intent(Intent.ACTION_SEND).apply {
                type = mime
                putExtra(Intent.EXTRA_STREAM, uri)
                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            }
            startActivity(Intent.createChooser(share, "Share photo"))
        }
    }
}
