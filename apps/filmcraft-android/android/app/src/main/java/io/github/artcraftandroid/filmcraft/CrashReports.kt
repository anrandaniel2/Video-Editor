package io.github.artcraftandroid.filmcraft

import android.app.ActivityManager
import android.content.Context
import android.content.Intent
import android.os.Build
import java.io.File

/** Local-only diagnostics. Nothing is uploaded; sharing always requires an explicit user action. */
object CrashReports {
    @Volatile var screen: String = "Starting"
    private fun file(context: Context) = File(context.filesDir, "last-crash.txt")

    fun install(context: Context) {
        val previous = Thread.getDefaultUncaughtExceptionHandler()
        Thread.setDefaultUncaughtExceptionHandler { thread, error ->
            runCatching {
                file(context).writeText("${device(context)}\nScreen: $screen\nThread: ${thread.name}\n\n${error.stackTraceToString().take(64000)}")
            }
            previous?.uncaughtException(thread, error)
        }
    }

    fun hasReport(context: Context) = file(context).isFile
    private fun device(context: Context): String {
        val version = runCatching { context.packageManager.getPackageInfo(context.packageName, 0).versionName }.getOrNull()
        return "FilmCraft $version\n${Build.MANUFACTURER} ${Build.MODEL}, Android ${Build.VERSION.RELEASE} (${Build.VERSION.SDK_INT})"
    }

    fun share(context: Context) {
        val report = buildString {
            appendLine(device(context))
            appendLine(runCatching { file(context).readText() }.getOrDefault("No managed exception was recorded."))
            if (Build.VERSION.SDK_INT >= 30) {
                runCatching {
                    context.getSystemService(ActivityManager::class.java).getHistoricalProcessExitReasons(null, 0, 3).forEach {
                        appendLine("Process exit: reason=${it.reason}, status=${it.status}, time=${it.timestamp}, ${it.description}")
                        it.traceInputStream?.use { stream -> val chars = CharArray(32000); val count = stream.bufferedReader().read(chars); if (count > 0) appendLine(String(chars, 0, count)) }
                    }
                }
            }
        }
        context.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).apply {
            type = "text/plain"
            putExtra(Intent.EXTRA_SUBJECT, "FilmCraft crash report")
            putExtra(Intent.EXTRA_TEXT, report.take(96000))
        }, "Share crash report"))
    }
}
