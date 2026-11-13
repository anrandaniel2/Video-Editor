package io.github.artcraftandroid.filmcraft.ui.editor

import android.content.ComponentCallbacks2
import android.content.res.Configuration
import android.hardware.display.DisplayManager
import android.view.KeyEvent
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.key.onKeyEvent
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import io.github.artcraftandroid.procraft.canvas.*

/** MainActivity's non-visual services, retained when the full ProCraft UI is hosted in a dialog. */
@Composable
internal fun ProcraftHostServices(vm: CanvasViewModel) {
    val context = LocalContext.current
    val owner = LocalLifecycleOwner.current
    val projecting = vm.projectCanvas
    DisposableEffect(vm, context, owner, projecting) {
        val displays = context.getSystemService(DisplayManager::class.java)
        var presentation: ProjectionPresentation? = null
        fun showProjection() {
            val display = if (vm.projectCanvas && owner.lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)) {
                displays.getDisplays(DisplayManager.DISPLAY_CATEGORY_PRESENTATION).firstOrNull()
            } else null
            if (presentation?.display?.displayId == display?.displayId) return
            presentation?.dismiss()
            presentation = display?.let { d -> ProjectionPresentation(context, d, vm).takeIf { runCatching { it.show() }.isSuccess } }
        }
        val listener = object : DisplayManager.DisplayListener {
            override fun onDisplayAdded(displayId: Int) = showProjection()
            override fun onDisplayRemoved(displayId: Int) = showProjection()
            override fun onDisplayChanged(displayId: Int) = showProjection()
        }
        val observer = LifecycleEventObserver { _, event ->
            when (event) {
                Lifecycle.Event.ON_START -> { vm.readAppSettings(); vm.resumeTracking(); vm.appInFront(true); showProjection() }
                Lifecycle.Event.ON_STOP -> { vm.appInFront(false); vm.saveNow(); presentation?.dismiss(); presentation = null }
                else -> Unit
            }
        }
        val memory = object : ComponentCallbacks2 {
            override fun onConfigurationChanged(newConfig: Configuration) = Unit
            override fun onLowMemory() { vm.trimBrushPreviews(); vm.session?.trimMemory() }
            override fun onTrimMemory(level: Int) {
                if (level >= ComponentCallbacks2.TRIM_MEMORY_RUNNING_LOW) {
                    vm.trimBrushPreviews()
                    vm.session?.trimMemory()
                }
            }
        }
        owner.lifecycle.addObserver(observer)
        displays.registerDisplayListener(listener, null)
        context.applicationContext.registerComponentCallbacks(memory)
        showProjection()
        onDispose {
            owner.lifecycle.removeObserver(observer)
            displays.unregisterDisplayListener(listener)
            context.applicationContext.unregisterComponentCallbacks(memory)
            presentation?.dismiss()
        }
    }
}

/** Bubble unhandled keys, so text entry gets first refusal, just as in upstream's Activity. */
internal fun Modifier.procraftKeys(vm: CanvasViewModel, doubleTaps: PenDoubleTaps): Modifier = onKeyEvent { press ->
    val event = press.nativeKeyEvent
    val code = event.keyCode
    val alt = code == KeyEvent.KEYCODE_ALT_LEFT || code == KeyEvent.KEYCODE_ALT_RIGHT
    if (event.action == KeyEvent.ACTION_UP) {
        alt && vm.modifyHeld && vm.modifyKey(false)
    } else if (event.action == KeyEvent.ACTION_DOWN) {
        val pen = event.device?.let { penKeyButton(code, it.sources or event.source, it.keyboardType, it.name) }
        when {
            code == PEN_DOUBLE_TAP_KEY -> { if (event.repeatCount == 0 && doubleTaps.isNew(event.eventTime)) vm.penKey(false); true }
            pen != null && (event.repeatCount > 0 || vm.penKey(pen)) -> true
            alt && event.repeatCount == 0 && !event.isCtrlPressed && vm.modifyKey(true) -> true
            event.repeatCount == 0 -> vm.keyShortcut(code, event.isCtrlPressed || event.isMetaPressed, event.isShiftPressed, event.isAltPressed)
            else -> false
        }
    } else false
}
