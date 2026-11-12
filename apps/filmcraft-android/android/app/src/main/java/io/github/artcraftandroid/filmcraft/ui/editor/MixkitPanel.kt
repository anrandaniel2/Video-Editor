package io.github.artcraftandroid.filmcraft.ui.editor

import android.webkit.WebView
import android.webkit.WebViewClient
import android.webkit.WebResourceRequest
import android.webkit.WebResourceError
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import io.github.artcraftandroid.filmcraft.ui.ImmersiveDialog

/** User-driven browsing of the provider's own pages, not a scraped/rehosted catalog. */
@Composable
internal fun MixkitPanel(ed: EditorController, video: Boolean, close: () -> Unit) {
    var overlay by remember { mutableStateOf(false) }
    var pending by remember { mutableStateOf<String?>(null) }
    var pageError by remember { mutableStateOf<String?>(null) }
    var browser by remember { mutableStateOf<WebView?>(null) }
    Dialog(onDismissRequest = close, properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false)) {
        ImmersiveDialog()
        Surface(Modifier.fillMaxSize()) {
            Column(Modifier.fillMaxSize().padding(12.dp)) {
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                    Text(if (video) "Mixkit videos" else "Mixkit sound effects", style = MaterialTheme.typography.titleLarge)
                    TextButton(onClick = close) { Text("Close") }
                }
                Text("Browse and preview on Mixkit. Tap a download, then confirm Import. Check each asset’s license.", style = MaterialTheme.typography.bodySmall)
                if (video) Row { Checkbox(overlay, { overlay = it }); Text("Import as overlay", Modifier.padding(top = 12.dp)) }
                ed.busy?.let { Text(it); LinearProgressIndicator(Modifier.fillMaxWidth()) }
                ed.onlineImportError?.let { Text(it, color = MaterialTheme.colorScheme.error) }
                pageError?.let { Text(it, color = MaterialTheme.colorScheme.error); TextButton(onClick = { pageError = null; browser?.reload() }) { Text("Retry page") } }
                pending?.let { address ->
                    Row {
                        Button(onClick = {
                            val credits = "Source: ${browser?.url.orEmpty()}\nProvider: Mixkit\nLicense: https://mixkit.co/license/\n"
                            pending = null
                            if (video) ed.importOnlineVideo(address, credits, overlay) else ed.importOnlineSound(address, credits)
                        }, enabled = ed.busy == null) { Text("Import download") }
                        TextButton(onClick = { pending = null }) { Text("Cancel") }
                    }
                }
                AndroidView(factory = { context ->
                    WebView(context).apply {
                        browser = this
                        settings.javaScriptEnabled = true
                        settings.domStorageEnabled = true
                        settings.allowFileAccess = false
                        settings.allowContentAccess = false
                        webViewClient = object : WebViewClient() {
                            override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
                                val uri = request.url
                                // Keep browsing on the provider, without permitting arbitrary schemes.
                                return uri.scheme != "https" || !(uri.host == "mixkit.co" || uri.host?.endsWith(".mixkit.co") == true)
                            }
                            override fun onReceivedError(view: WebView, request: WebResourceRequest, error: WebResourceError) {
                                if (request.isForMainFrame) pageError = "Could not open Mixkit: ${error.description}"
                            }
                        }
                        setDownloadListener { url, _, _, mime, _ ->
                            if (url.startsWith("https://") && !mime.orEmpty().contains("text/html")) { pending = url; pageError = null }
                            else pageError = "This download did not provide a direct HTTPS media file."
                        }
                        loadUrl(if (video) "https://mixkit.co/free-stock-video/" else "https://mixkit.co/free-sound-effects/")
                    }
                }, modifier = Modifier.weight(1f).fillMaxWidth(), onReset = null, onRelease = { browser = null; it.stopLoading(); it.destroy() })
            }
        }
    }
}
