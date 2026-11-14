package io.github.artcraftandroid.filmcraft.ui.editor

import io.github.artcraftandroid.filmcraft.ui.editorCard
import io.github.artcraftandroid.filmcraft.ui.editorGlass
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
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
    var lanes by remember(clip?.id) { mutableStateOf<List<AnimationLane>>(emptyList()) }
    var error by remember(clip?.id) { mutableStateOf<String?>(null) }
    var laneId by remember(clip?.id) { mutableStateOf<String?>(null) }
    var selected by remember(clip?.id, laneId) { mutableStateOf<Long?>(null) }
    LaunchedEffect(clip?.id, ed.playhead, ed.timeline?.revision) {
        if (clip != null) try { lanes = ed.animationState(clip.id).lanes(); error = null }
        catch (e: kotlinx.coroutines.CancellationException) { throw e }
        catch (e: Exception) { lanes = emptyList(); error = "Could not load keyframes: ${e.message}" }
    }
    val lane = lanes.firstOrNull { it.id == laneId } ?: lanes.firstOrNull()
    val key = lane?.keys?.firstOrNull { it.time == selected }
    ToolPanel("Keyframes", close, modifier, tag = "keyframes_panel", scrollContent = false) {
      LazyColumn(Modifier.fillMaxWidth()) {
       item(key = "animation_header") {
        Text("Add a keyframe snapshot, move the playhead, then change any slider or preview control. Tap a property title to edit its curve.", style = FcType.Caption, color = Fc.Detail, modifier = Modifier.padding(horizontal = FcSize.PanelPadding))
        if (error != null) Text(error.orEmpty(), color = Fc.Detail, modifier = Modifier.padding(12.dp))
        if (clip == null || lane == null) {
            Text("Select a clip with animatable properties.", color = Fc.Detail, modifier = Modifier.padding(12.dp))
            return@item
        }
        val animated = lanes.any { it.animated }
        Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 12.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            SelectChip("◆ Key everything",false,{ed.animationCommand("animation.all",JSONObject().put("clip",clip.id.toLong()).put("action","key"))},tag="keyframes_key_all")
            if (animated) SelectChip("Stop animation",false,{ed.animationCommand("animation.all",JSONObject().put("clip",clip.id.toLong()).put("action","disable"))},tag="keyframes_stop_all")
        }
        Text("Curve: ${lane.group} · ${lane.label}",style=FcType.Caption,color=Fc.Detail,modifier=Modifier.padding(horizontal=12.dp))
        Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal=12.dp),horizontalArrangement=Arrangement.spacedBy(6.dp)) {
            val sorted=lane.keys.sortedBy { it.timeline }
            SelectChip("‹ Previous",false,{sorted.lastOrNull {it.timeline<ed.playhead}?.let {selected=it.time;ed.seek(it.timeline,draft=false)}},tag="keyframes_prev")
            SelectChip("Next ›",false,{sorted.firstOrNull {it.timeline>ed.playhead}?.let {selected=it.time;ed.seek(it.timeline,draft=false)}},tag="keyframes_next")
        }
        KeyframeGraph(lane, selected, clip.start, clip.duration, ed.playhead,
            select = { selected = it.time; ed.seek(it.timeline, draft = false) },
            move = { k, delta ->
                val ref = lane.address(clip.id).put("time", k.time)
                ed.animationCommand("effects.moveKeyframes", JSONObject().put("keyframes", JSONArray().put(ref)).put("delta", delta))
            },
        )
        Text(if (key != null) "Selected key · ${formatSeconds((key.timeline - clip.start).toDouble() / TICKS)} · ${key.interpolation}" else "Value at playhead", style = FcType.Caption, color = Fc.Detail, modifier = Modifier.padding(horizontal = 12.dp))
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
       if (clip != null && lane != null) {
        lanes.groupBy { it.group }.forEach { (group, properties) ->
            if (properties.size != 1 || !properties.single().label.equals(group, ignoreCase = true)) {
                item(key = "group:$group") { Text(group, style=FcType.Header, color=Fc.Title, modifier=Modifier.padding(16.dp)) }
            }
            items(properties, key = { "property:${it.id}" }) { item ->
                Column(Modifier.fillMaxWidth().padding(horizontal=12.dp, vertical=4.dp).editorCard(14.dp).padding(vertical=10.dp)) {
                    val scalar = item.value is Number && (item.kind as? JSONObject)?.optJSONArray("Choice") == null
                    if (!scalar) SelectChip(item.label + if (item.animated) " ◆" else "", item.id==lane.id, {laneId=item.id;selected=null}, tag="keyframes_property_${item.id}")
                    val selectedKey=if (item.id==lane.id) key else null
                    ValueEditor(selectedKey?.value ?: item.value,item.kind,item.param=="path" && item.mask!=null,item.label,item.id,
                        (ed.timeline?.width?.toDouble() ?: 1920.0) to (ed.timeline?.height?.toDouble() ?: 1080.0),
                        selectProperty={laneId=item.id;selected=null}, propertySelected=item.id==lane.id, animated=item.animated,
                        editPath={if(selectedKey!=null) ed.seek(selectedKey.timeline,draft=false);ed.panel=Panel.Mask},
                        save={value,merge ->
                            val body=item.address(clip.id).put("value",value)
                            if(merge!=null) body.put("merge",merge)
                            if(selectedKey!=null) ed.animationCommand("effects.setKeyframe",body.put("mediaTime",selectedKey.time))
                            else ed.animationCommand("effects.setParam",body)
                        },
                    )
                }
            }
        }
       }
      }
    }
}

private const val TICKS = 254016000000.0
private fun formatSeconds(seconds: Double) = "%.2f s".format(seconds)

@Composable
private fun AnimationSlider(label:String,value:Double,min:Double,max:Double,tag:String,onTitleClick:(() -> Unit)? = null, selected:Boolean = false, save:(Double,String)->Unit) {
    var v by remember(tag) { mutableDoubleStateOf(value) }
    var dragging by remember(tag) { mutableStateOf(false) }
    var token by remember(tag) { mutableStateOf(java.util.UUID.randomUUID().toString()) }
    LaunchedEffect(value) { if(!dragging) v=value }
    // Optional/unbounded engine ranges are JSON null; optDouble returns NaN for them.
    // Never feed NaN/infinite fractions into Compose's slider/layout calculations.
    val safeValue = value.takeIf { it.isFinite() } ?: 0.0
    val lo=minOf(min.takeIf { it.isFinite() } ?: -1000.0,safeValue)
    val hi=maxOf(max.takeIf { it.isFinite() } ?: 1000.0,safeValue,lo+0.01)
    SliderRow(label,"${kotlin.math.round(v*100)/100}", onLabelClick = onTitleClick, selected = selected) {
        FcSlider(numericRange=lo.toFloat()..hi.toFloat(),fraction=(((v.takeIf { it.isFinite() } ?: safeValue)-lo)/(hi-lo)).toFloat().coerceIn(0f,1f),onChange={f ->
            dragging=true;v=lo+f*(hi-lo);save(v,token)
        },onDone={dragging=false;token=java.util.UUID.randomUUID().toString()},description=label,tag="keyframes_slider_$tag")
    }
}

@Composable
private fun ValueEditor(value: Any, kind: Any?, path: Boolean, label:String, id:String, frame:Pair<Double,Double>, selectProperty: () -> Unit, propertySelected: Boolean, animated: Boolean, editPath: () -> Unit, save: (Any,String?) -> Unit) {
    if(path) {SelectChip("Edit mask path in preview",false,editPath,tag="keyframes_edit_path_$id");return}
    val options=(kind as? JSONObject)?.optJSONArray("Choice")
    if(options!=null) {
        Row(Modifier.horizontalScroll(rememberScrollState()).padding(horizontal=12.dp),horizontalArrangement=Arrangement.spacedBy(6.dp)) {
            (0 until options.length()).forEach {i -> SelectChip(options.getString(i),(value as? Number)?.toInt()==i,{save(i,null)},tag="keyframes_choice_${id}_$i")}
        }
    } else if(value is Boolean) {
        Row(Modifier.padding(horizontal=12.dp),horizontalArrangement=Arrangement.spacedBy(6.dp)) {
            SelectChip("Off",!value,{save(false,null)},tag="keyframes_false_$id")
            SelectChip("On",value,{save(true,null)},tag="keyframes_true_$id")
        }
    } else if(value is Number) {
        val range=(kind as? JSONObject)?.optJSONObject("Float")
        val min=range?.optDouble("soft_min",0.0) ?: if(kind=="Angle") -360.0 else -1000.0
        val max=range?.optDouble("soft_max",100.0) ?: if(kind=="Angle") 360.0 else 1000.0
        AnimationSlider(label + if (animated) " ◆" else "",value.toDouble(),min,max,id,onTitleClick=selectProperty,selected=propertySelected) {v,token->save(v,token)}
    } else if(value is JSONArray && value.length() in listOf(2,4) && value.opt(0) is Number) {
        val color=value.length()==4
        val wheel=kind=="Wheel"
        (0 until value.length()).forEach {i ->
            val component=if(color) listOf("Red","Green","Blue","Alpha")[i] else listOf("X","Y")[i]
            val dimension=if(i==0) frame.first else frame.second
            AnimationSlider(component,value.optDouble(i),if(color) 0.0 else if(wheel) -1.0 else -dimension,
                if(color||wheel) 1.0 else dimension*2,"${id}_$i") {v,token ->
                val next=JSONArray(value.toString()).put(i,v);save(next,token)
            }
        }
    } else if(value is String) {
        // Text content isn't a numeric value; numeric and color controls never use input boxes.
        var text by remember(id,value) {mutableStateOf(value)}
        OutlinedTextField(value=text,onValueChange={text=it},modifier=Modifier.fillMaxWidth().padding(horizontal=12.dp))
        SelectChip("Apply text",false,{save(text,null)},tag="keyframes_apply_text_$id")
    } else {
        Text("Edit this shape in its preview tool.",style=FcType.Caption,color=Fc.Detail,modifier=Modifier.padding(12.dp))
    }
}

@Composable
private fun KeyframeGraph(lane: AnimationLane, selected: Long?, start: Long, duration: Long, playhead: Long, select: (AnimationKey) -> Unit, move: (AnimationKey, Long) -> Unit) {
    var dragKey by remember(lane.id) { mutableStateOf<AnimationKey?>(null) }
    var dragPixels by remember(lane.id) { mutableFloatStateOf(0f) }
    val keys = lane.keys.sortedBy { it.timeline }
    val span = duration.coerceAtLeast(1).toDouble()
    val lineColor = Fc.Detail
    val accent = Fc.Icon
    Canvas(Modifier.fillMaxWidth().height(160.dp).padding(horizontal = 16.dp, vertical = 12.dp).editorGlass(12.dp, layer = 3f)
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
                if (size.width > 0) dragKey?.let { move(it, (dragPixels / size.width * span).roundToLong()) }
                dragKey = null
            }) { change, amount ->
                if (dragKey != null) { change.consume(); dragPixels += amount.x }
            }
        }) {
        val values = keys.mapNotNull { (it.value as? Number)?.toDouble()?.takeIf { v -> v.isFinite() } }
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
