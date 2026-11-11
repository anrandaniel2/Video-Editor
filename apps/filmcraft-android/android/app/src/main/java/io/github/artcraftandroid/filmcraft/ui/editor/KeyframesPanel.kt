package io.github.artcraftandroid.filmcraft.ui.editor

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import io.github.artcraftandroid.filmcraft.ui.*
import org.json.JSONArray
import org.json.JSONObject
import kotlin.math.abs
import kotlin.math.roundToLong

private data class AnimationKey(val time: Long, val timeline: Long, val value: Any, val interpolation: String, val incoming: Double, val outgoing: Double)
private data class AnimationLane(val effect: Int, val mask: Int?, val param: String, val group: String, val label: String, val value: Any, val kind: Any?, val animated: Boolean, val keys: List<AnimationKey>) {
    val id get() = "$effect:$mask:$param"
    fun address(clip: ULong) = JSONObject().put("clip", clip.toLong()).put("effect", effect).put("param", param).apply { if (mask != null) put("mask", mask) }
}
private fun JSONObject.lanes(): List<AnimationLane> {
    val a = optJSONArray("lanes") ?: return emptyList()
    return (0 until a.length()).map { index ->
        val j = a.getJSONObject(index)
        val keys = j.getJSONArray("keys")
        AnimationLane(j.getInt("effect"), if (j.isNull("mask")) null else j.getInt("mask"), j.getString("param"), j.getString("group"), j.getString("label"), j.get("value"), j.opt("kind"), j.getBoolean("animated"), (0 until keys.length()).map {
            val k = keys.getJSONObject(it)
            AnimationKey(k.getLong("time"), k.getLong("timeline"), k.get("value"), k.getString("interpolation"), k.optDouble("inInfluence", 1.0/3), k.optDouble("outInfluence", 1.0/3))
        })
    }
}

/** One animation surface for all effect controls, 2D transforms, 3D and per-effect masks. */
@Composable
fun KeyframesPanel(ed: EditorController, close: () -> Unit, modifier: Modifier = Modifier) {
    val clip = ed.selectedClip
    var data by remember(clip?.id) { mutableStateOf<JSONObject?>(null) }
    var error by remember(clip?.id) { mutableStateOf<String?>(null) }
    var laneId by remember(clip?.id) { mutableStateOf<String?>(null) }
    var selected by remember(clip?.id, laneId) { mutableStateOf<Long?>(null) }
    LaunchedEffect(clip?.id, ed.playhead, ed.timeline?.revision) {
        if (clip != null) try { data = ed.animationState(clip.id); error = null } catch (e: Exception) { error = e.message }
    }
    val lanes = data?.lanes().orEmpty()
    val lane = lanes.firstOrNull { it.id == laneId } ?: lanes.firstOrNull()
    val key = lane?.keys?.firstOrNull { it.time == selected }
    ToolPanel("Keyframes", close, modifier, tag = "keyframes_panel") {
        Text("Animate a property, move the playhead, then change its value. Drag diamonds to move keys. Curve presets affect the segment after the selected key.", style = FcType.Caption, color = Fc.Detail, modifier = Modifier.padding(horizontal = FcSize.PanelPadding))
        if (error != null) Text(error.orEmpty(), color = Fc.Detail, modifier = Modifier.padding(12.dp))
        if (clip == null || lane == null) {
            Text("Select a clip with animatable properties.", color = Fc.Detail, modifier = Modifier.padding(12.dp))
            return@ToolPanel
        }
        val groups = lanes.map { it.group }.distinct()
        Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 12.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            groups.forEach { group -> SelectChip(group, lane.group == group, { laneId = lanes.first { it.group == group }.id; selected = null }, tag = "keyframes_group_${groups.indexOf(group)}") }
        }
        Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 12.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            lanes.filter { it.group == lane.group }.forEach { item ->
                SelectChip((if (item.animated) "◆ " else "") + item.label, item.id == lane.id, { laneId = item.id; selected = null }, tag = "keyframes_param_${item.id}")
            }
        }
        Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 12.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            SelectChip(if (lane.animated) "Animation on" else "Animate", lane.animated, { ed.animationCommand("effects.toggleAnimation", lane.address(clip.id)) }, tag = "keyframes_animate")
            SelectChip("◆ Add / remove", false, { ed.animationCommand("effects.addKeyframe", lane.address(clip.id)) }, tag = "keyframes_add")
            val sorted = lane.keys.sortedBy { it.timeline }
            SelectChip("‹ Previous", false, { sorted.lastOrNull { it.timeline < ed.playhead }?.let { selected = it.time; ed.seek(it.timeline, draft = false) } }, tag = "keyframes_prev")
            SelectChip("Next ›", false, { sorted.firstOrNull { it.timeline > ed.playhead }?.let { selected = it.time; ed.seek(it.timeline, draft = false) } }, tag = "keyframes_next")
        }
        KeyframeGraph(lane, selected, clip.start, clip.duration, ed.playhead,
            select = { selected = it.time; ed.seek(it.timeline, draft = false) },
            move = { k, delta ->
                val ref = lane.address(clip.id).put("time", k.time)
                ed.animationCommand("effects.moveKeyframes", JSONObject().put("keyframes", JSONArray().put(ref)).put("delta", delta))
            },
        )
        Text(if (key != null) "Selected key · ${formatSeconds((key.timeline - clip.start).toDouble() / TICKS)} · ${key.interpolation}" else "Value at playhead", style = FcType.Caption, color = Fc.Detail, modifier = Modifier.padding(horizontal = 12.dp))
        ValueEditor(key?.value ?: lane.value, lane.kind, lane.param == "path" && lane.mask != null,
            editPath = { if (key != null) ed.seek(key.timeline, draft = false); ed.panel = Panel.Mask },
            save = { value ->
                val body = lane.address(clip.id).put("value", value)
                if (key != null) ed.animationCommand("effects.setKeyframe", body.put("mediaTime", key.time))
                else ed.animationCommand("effects.setParam", body)
            },
        )
        if (key != null) {
            Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 12.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                listOf("linear" to "Linear", "hold" to "Hold", "easeIn" to "Ease in", "easeOut" to "Ease out", "easeInOut" to "Ease in/out", "strong" to "Strong ease").forEach { (id, label) ->
                    SelectChip(label, false, { ed.animationCommand("animation.curve", lane.address(clip.id).put("mediaTime", key.time).put("preset", id)) }, tag = "keyframes_curve_$id")
                }
            }
            Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 12.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                SelectChip("−1 frame", false, { ed.animationCommand("effects.moveKeyframes", JSONObject().put("keyframes", JSONArray().put(lane.address(clip.id).put("time", key.time))).put("delta", -ed.frameTicks)) }, tag = "keyframes_nudge_back")
                SelectChip("+1 frame", false, { ed.animationCommand("effects.moveKeyframes", JSONObject().put("keyframes", JSONArray().put(lane.address(clip.id).put("time", key.time))).put("delta", ed.frameTicks)) }, tag = "keyframes_nudge_next")
                SelectChip("Delete key", false, { ed.animationCommand("effects.deleteKeyframe", lane.address(clip.id).put("mediaTime", key.time)); selected = null }, tag = "keyframes_delete")
            }
        }
    }
}

private const val TICKS = 254016000000.0
private fun formatSeconds(seconds: Double) = "%.2f s".format(seconds)

@Composable
private fun ValueEditor(value: Any, kind: Any?, path: Boolean, editPath: () -> Unit, save: (Any) -> Unit) {
    if (path) {
        SelectChip("Edit mask path in preview", false, editPath, tag = "keyframes_edit_path")
        return
    }
    val options = (kind as? JSONObject)?.optJSONArray("Choice")
    if (options != null) {
        Row(Modifier.horizontalScroll(rememberScrollState()).padding(horizontal = 12.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            (0 until options.length()).forEach { i -> SelectChip(options.getString(i), (value as? Number)?.toInt() == i, { save(i) }, tag = "keyframes_choice_$i") }
        }
        return
    }
    if (value is Boolean) {
        Row(Modifier.padding(horizontal = 12.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            SelectChip("Off", !value, { save(false) }, tag = "keyframes_false")
            SelectChip("On", value, { save(true) }, tag = "keyframes_true")
        }
        return
    }
    var text by remember(value.toString()) { mutableStateOf(value.toString()) }
    var invalid by remember(value.toString()) { mutableStateOf(false) }
    val label = when {
        value is Number -> "Value"
        value is JSONArray && value.length() == 2 -> "Position [x, y]"
        value is JSONArray && value.length() == 4 -> "Color [r, g, b, alpha] · 0–1"
        value is String -> "Text"
        else -> "Value (JSON)"
    }
    OutlinedTextField(value = text, onValueChange = { text = it; invalid = false }, label = { Text(label) }, isError = invalid, modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp).testTag("keyframes_value"), singleLine = value !is String)
    SelectChip("Apply value", false, {
        val parsed: Any? = try {
            when (value) {
                is Number -> text.toDoubleOrNull()?.takeIf { it.isFinite() }
                is JSONArray -> JSONArray(text)
                is JSONObject -> JSONObject(text)
                else -> text
            }
        } catch (_: Exception) { null }
        if (parsed != null) save(parsed) else invalid = true
    }, tag = "keyframes_apply_value")
}

@Composable
private fun KeyframeGraph(lane: AnimationLane, selected: Long?, start: Long, duration: Long, playhead: Long, select: (AnimationKey) -> Unit, move: (AnimationKey, Long) -> Unit) {
    var dragKey by remember(lane.id) { mutableStateOf<AnimationKey?>(null) }
    var dragPixels by remember(lane.id) { mutableFloatStateOf(0f) }
    val keys = lane.keys.sortedBy { it.timeline }
    val span = duration.coerceAtLeast(1).toDouble()
    val lineColor = Fc.Detail
    val accent = Fc.Icon
    Canvas(Modifier.fillMaxWidth().height(160.dp).padding(horizontal = 16.dp, vertical = 12.dp).background(Fc.Raised)
        .testTag("keyframes_graph")
        .pointerInput(lane.id, keys.map { it.time to it.timeline }, start, duration) {
            detectTapGestures { point ->
                keys.minByOrNull { abs(((it.timeline - start) / span * size.width) - point.x) }?.let { select(it) }
            }
        }
        .pointerInput(lane.id, keys.map { it.time to it.timeline }, start, duration) {
            detectDragGestures(onDragStart = { point ->
                dragKey = keys.minByOrNull { abs(((it.timeline - start) / span * size.width) - point.x) }?.takeIf { abs(((it.timeline - start) / span * size.width) - point.x) < 28.dp.toPx() }
                dragPixels = 0f
                dragKey?.let(select)
            }, onDragCancel = { dragKey = null }, onDragEnd = {
                dragKey?.let { move(it, (dragPixels / size.width * span).roundToLong()) }
                dragKey = null
            }) { change, amount ->
                if (dragKey != null) { change.consume(); dragPixels += amount.x }
            }
        }) {
        val values = keys.mapNotNull { (it.value as? Number)?.toDouble() }
        val low = values.minOrNull() ?: 0.0
        val high = values.maxOrNull() ?: 1.0
        fun x(k: AnimationKey) = ((k.timeline - start) / span * size.width).toFloat()
        fun y(k: AnimationKey): Float = if (high > low && k.value is Number) (size.height * (0.9 - 0.8 * (k.value.toDouble() - low) / (high - low))).toFloat() else size.height / 2
        for (i in 1..3) drawLine(lineColor.copy(alpha = 0.2f), Offset(0f, size.height * i / 4), Offset(size.width, size.height * i / 4))
        keys.zipWithNext().forEach { (a, b) ->
            val path = Path()
            path.moveTo(x(a), y(a))
            for (i in 1..40) {
                val t = i / 40.0
                val eased = graphEase(a, b, t)
                path.lineTo(x(a) + (x(b) - x(a)) * t.toFloat(), y(a) + (y(b) - y(a)) * eased.toFloat())
            }
            drawPath(path, accent, style = androidx.compose.ui.graphics.drawscope.Stroke(2.dp.toPx()))
        }
        keys.forEach { k ->
            val px = x(k) + if (dragKey?.time == k.time) dragPixels else 0f
            val py = y(k)
            val r = if (k.time == selected) 7.dp.toPx() else 5.dp.toPx()
            val diamond = Path().apply { moveTo(px, py-r); lineTo(px+r,py); lineTo(px,py+r); lineTo(px-r,py); close() }
            drawPath(diamond, if (k.time == selected) Color(0xFFFFCC66) else accent)
        }
        val px = ((playhead - start) / span * size.width).toFloat()
        drawLine(Color(0xFFFFCC66), Offset(px,0f), Offset(px,size.height), 1.dp.toPx())
    }
}

/** Same temporal handles as the engine; only the graph is evaluated on the UI thread. */
private fun graphEase(a: AnimationKey, b: AnimationKey, u: Double): Double {
    if (a.time > b.time) return 1.0 - graphEase(b, a, 1.0-u)
    if (a.interpolation == "Hold") return if (u < 1) 0.0 else 1.0
    val out = a.interpolation in listOf("Bezier", "Auto Bezier", "Continuous Bezier", "Ease Out")
    val incoming = b.interpolation in listOf("Bezier", "Auto Bezier", "Continuous Bezier", "Ease In") || a.interpolation in listOf("Bezier", "Continuous Bezier")
    if (!out && !incoming) return u
    val x1 = if (out) a.outgoing else 1.0/3
    val x2 = if (incoming) 1-b.incoming else 2.0/3
    val y1 = if (out) 0.0 else 1.0/3
    val y2 = if (incoming) 1.0 else 2.0/3
    fun cubic(t: Double, p: Double, q: Double) = 3*(1-t)*(1-t)*t*p + 3*(1-t)*t*t*q + t*t*t
    var lo=0.0; var hi=1.0
    repeat(20) { val m=(lo+hi)/2; if (cubic(m,x1,x2)<u) lo=m else hi=m }
    return cubic((lo+hi)/2,y1,y2)
}
