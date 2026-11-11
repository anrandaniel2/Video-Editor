package io.github.artcraftandroid.filmcraft.ui.editor

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.calculatePan
import androidx.compose.foundation.gestures.calculateZoom
import androidx.compose.foundation.layout.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.Alignment
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import io.github.artcraftandroid.filmcraft.core.Object3DSettings
import io.github.artcraftandroid.filmcraft.core.Placement
import io.github.artcraftandroid.filmcraft.ui.SelectChip
import java.util.UUID
import kotlin.math.*

private enum class GizmoMode { Move, Rotate, Scale }
private val axisColors = listOf(Color(0xFFFF6575), Color(0xFF6FE295), Color(0xFF669FFF))

private fun distanceToSegment(p: Offset, a: Offset, b: Offset): Float {
    val d = b-a
    val length = d.x*d.x+d.y*d.y
    val u = if (length > 0f) (((p-a).x*d.x+(p-a).y*d.y)/length).coerceIn(0f,1f) else 0f
    return (p-(a+d*u)).getDistance()
}

/** All 3D kinds (primitive, extruded text, model) share the same handles and transform state.
 * A single finger only operates a hit-tested handle. A pinch can scale anywhere, but cannot
 * translate or rotate the object. Nothing here interprets a background drag as a 3D move.
 */
@Composable
fun ObjectGizmoOverlay(ed: EditorController, fw: Float, fh: Float, placement: Placement?, modifier: Modifier = Modifier) {
    val clip = ed.selectedClip ?: return
    val state = ed.object3DState?.takeIf { it.clip == clip.id && it.hasObject } ?: return
    var mode by remember(clip.id) { mutableStateOf(GizmoMode.Move) }
    var active by remember(clip.id) { mutableStateOf(-1) }
    val settings by rememberUpdatedState(state.settings)
    val transform by rememberUpdatedState(placement)
    Box(modifier.testTag("object3d_gizmo")) {
        Canvas(Modifier.fillMaxSize().pointerInput(clip.id, mode, fw, fh) {
            val density = density
            fun centre(s: Object3DSettings): Offset = gizmoCentre(s, transform, fw, fh, size.width.toFloat(), size.height.toFloat())
            val radius = 62.dp.toPx()
            val hit = 18.dp.toPx()
            awaitEachGesture {
                val down = awaitFirstDown(requireUnconsumed = false)
                val initial = settings
                val center = centre(initial)
                val axes = gizmoAxes(radius)
                val rings = gizmoRings(center, radius)
                active = when (mode) {
                    GizmoMode.Move -> axes.indices.minByOrNull { distanceToSegment(down.position, center+axes[it]*0.25f, center+axes[it]) }
                        ?.takeIf { distanceToSegment(down.position,center+axes[it]*0.25f,center+axes[it]) < hit } ?: -1
                    GizmoMode.Scale -> if ((down.position-(center+Offset(radius*0.7f,-radius*0.7f))).getDistance()<hit) 3 else -1
                    GizmoMode.Rotate -> rings.indices.minByOrNull { i -> rings[i].zipWithNext().minOf { (a,b) -> distanceToSegment(down.position,a,b) } }
                        ?.takeIf { i -> rings[i].zipWithNext().minOf { (a,b) -> distanceToSegment(down.position,a,b) } < hit } ?: -1
                }
                var current = initial
                var previous = down.position
                var total = Offset.Zero
                var count = 1
                var multi = false
                var changed = false
                var pinch = 1f
                val token = "gizmo:${UUID.randomUUID()}"
                while (true) {
                    val event = awaitPointerEvent()
                    val fingers = event.changes.count { it.pressed }
                    if (fingers == 0) break
                    if (fingers != count) {
                        count = fingers
                        multi = multi || fingers >= 2
                        active = -1 // Joining a second finger releases the handle, without jumping.
                        pinch = 1f
                        event.changes.firstOrNull { it.pressed }?.let { previous = it.position }
                        continue
                    }
                    if (fingers >= 2) {
                        pinch *= event.calculateZoom()
                        if (abs(pinch-1f)>0.015f || changed) {
                            current = current.copy(scale=(current.scale*pinch).coerceIn(0.1,400.0))
                            pinch=1f
                            ed.setObject(current,token); changed=true
                        }
                        // Explicitly ignore two-finger pan and twist for 3D.
                        event.changes.forEach { it.consume() }
                        continue
                    }
                    if (multi || active < 0) continue
                    val point = event.changes.firstOrNull { it.pressed }?.position ?: continue
                    val delta = point-previous; previous=point; total+=delta
                    if (!changed && total.getDistance()<viewConfiguration.touchSlop) continue
                    val drag=if (!changed) total else delta
                    val pixelsPerFramePixel = size.width / fw.coerceAtLeast(1f)
                    current = when (mode) {
                        GizmoMode.Move -> {
                            val axis=axes[active]; val unit=axis/axis.getDistance()
                            val amount=(drag.x*unit.x+drag.y*unit.y)/pixelsPerFramePixel
                            // Undo the layer's 2D placement for X/Y, so handles remain in frame axes.
                            val layer=transform
                            val angle=Math.toRadians(-(layer?.rotation ?: 0.0))
                            val scale=(layer?.scale ?: 1.0).coerceAtLeast(0.001)
                            when (active) {
                                0 -> current.copy(x=current.x+amount*cos(angle)/scale,y=current.y+amount*sin(angle)/scale)
                                1 -> current.copy(x=current.x-amount*sin(angle)/scale,y=current.y+amount*cos(angle)/scale)
                                else -> current.copy(z=(current.z+amount*2.0).coerceIn(1.0,10000.0))
                            }
                        }
                        GizmoMode.Rotate -> when (active) {
                            0 -> current.copy(rotationX=current.rotationX+drag.y/density*0.7)
                            1 -> current.copy(rotationY=current.rotationY+drag.x/density*0.7)
                            else -> {
                                val a=previous-drag-center; val b=point-center
                                var degrees=(atan2(b.y,b.x)-atan2(a.y,a.x))*180f/PI.toFloat()
                                if (degrees>180f) degrees-=360f
                                if (degrees< -180f) degrees+=360f
                                current.copy(rotationZ=current.rotationZ+degrees)
                            }
                        }
                        GizmoMode.Scale -> current.copy(scale=(current.scale*exp((drag.x-drag.y)/density/160.0)).coerceIn(0.1,400.0))
                    }
                    ed.setObject(current,token); changed=true
                    event.changes.forEach { it.consume() }
                }
                active=-1
            }
        }) {
            val center=gizmoCentre(settings,placement,fw,fh,size.width,size.height)
            val radius=62.dp.toPx()
            val axes=gizmoAxes(radius)
            when (mode) {
                GizmoMode.Move -> axes.forEachIndexed { i,axis ->
                    val end=center+axis
                    val color=axisColors[i]
                    drawLine(Color.Black.copy(alpha=0.75f),center,end,6.dp.toPx())
                    drawLine(color,center,end,if (active==i) 4.dp.toPx() else 2.5.dp.toPx())
                    val unit=axis/axis.getDistance();val side=Offset(-unit.y,unit.x)
                    val arrow=Path().apply {moveTo(end.x,end.y); val a=end-unit*13.dp.toPx()+side*6.dp.toPx();lineTo(a.x,a.y);val b=end-unit*13.dp.toPx()-side*6.dp.toPx();lineTo(b.x,b.y);close()}
                    drawPath(arrow,color)
                }
                GizmoMode.Rotate -> gizmoRings(center,radius).forEachIndexed { i,points ->
                    val path=Path().apply {moveTo(points[0].x,points[0].y);points.drop(1).forEach {lineTo(it.x,it.y)}}
                    drawPath(path,Color.Black.copy(alpha=0.7f),style=Stroke(5.dp.toPx()))
                    drawPath(path,axisColors[i],style=Stroke(if (active==i) 4.dp.toPx() else 2.dp.toPx()))
                }
                GizmoMode.Scale -> {
                    val end=center+Offset(radius*0.7f,-radius*0.7f)
                    drawLine(Color.White,center,end,2.dp.toPx())
                    drawRect(Color.White,end-Offset(7.dp.toPx(),7.dp.toPx()),androidx.compose.ui.geometry.Size(14.dp.toPx(),14.dp.toPx()))
                }
            }
            drawCircle(Color.White,4.dp.toPx(),center)
        }
        Row(Modifier.align(Alignment.TopStart).padding(6.dp),horizontalArrangement=Arrangement.spacedBy(4.dp)) {
            GizmoMode.entries.forEach { m -> SelectChip(m.name,mode==m,{mode=m},tag="gizmo_${m.name.lowercase()}") }
        }
        Row(Modifier.align(Alignment.BottomStart).padding(6.dp),horizontalArrangement=Arrangement.spacedBy(5.dp)) {
            // Axis names remain legible even where projected handles intersect.
            listOf("X","Y","Z").forEachIndexed { i,label -> androidx.compose.material3.Text(label,color=axisColors[i],modifier=Modifier.padding(3.dp)) }
        }
    }
}

private fun gizmoCentre(s:Object3DSettings,p:Placement?,fw:Float,fh:Float,w:Float,h:Float):Offset {
    val scale=p?.scale ?: 1.0
    val a=Math.toRadians(p?.rotation ?: 0.0)
    val x=(s.x-fw/2)*scale; val y=(s.y-fh/2)*scale
    val px=(p?.x ?: (fw/2).toDouble())+x*cos(a)-y*sin(a)
    val py=(p?.y ?: (fh/2).toDouble())+x*sin(a)+y*cos(a)
    return Offset((px*w/fw.coerceAtLeast(1f)).toFloat(),(py*h/fh.coerceAtLeast(1f)).toFloat())
}
private fun gizmoAxes(radius:Float)=listOf(Offset(radius,0f),Offset(0f,radius),Offset(-radius*0.65f,radius*0.65f))
private fun gizmoRings(center:Offset,radius:Float):List<List<Offset>> = (0..2).map { axis ->
    (0..72).map { i ->
        val a=i*2*PI/72
        val x=cos(a).toFloat()*radius;val y=sin(a).toFloat()*radius
        center+when(axis) {0->Offset(x*0.28f,y);1->Offset(x,y*0.28f);else->Offset(x*0.85f,y*0.85f)}
    }
}
