package io.github.artcraftandroid.filmcraft.ui.editor

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.calculateZoom
import androidx.compose.foundation.layout.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.Alignment
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import io.github.artcraftandroid.filmcraft.core.Object3DSettings
import io.github.artcraftandroid.filmcraft.core.Placement
import io.github.artcraftandroid.filmcraft.ui.SelectChip
import java.util.UUID
import kotlin.math.*

private val axisColors=listOf(Color(0xFFFF6575),Color(0xFF6FE295),Color(0xFF669FFF))

/** Depth-sorted 3D cone/cylinder and torus meshes, projected through the object's actual
 * camera and layer placement. Picking uses these same meshes. A free one-finger drag and a
 * two-finger twist never move/rotate a 3D object; a pinch still uniformly scales it.
 */
@Composable
fun ObjectGizmoOverlay(ed:EditorController,fw:Float,fh:Float,placement:Placement?,modifier:Modifier=Modifier) {
    val clip=ed.selectedClip ?: return
    val state=ed.object3DState?.takeIf {it.clip==clip.id && it.hasObject} ?: return
    var mode by remember(clip.id) {mutableStateOf(GizmoMode.Move)}
    var active by remember(clip.id) {mutableStateOf(-1)}
    val settings by rememberUpdatedState(state.settings)
    val transform by rememberUpdatedState(placement)
    Box(modifier.testTag("object3d_gizmo")) {
        Canvas(Modifier.fillMaxSize().pointerInput(clip.id,mode,fw,fh) {
            fun geometry(s:Object3DSettings)=GizmoGeometry(s,transform,fw.toDouble(),fh.toDouble(),size.width.toDouble(),size.height.toDouble(),minOf(62.dp.toPx(),minOf(size.width,size.height).toFloat()*0.28f).toDouble())
            awaitEachGesture {
                val down=awaitFirstDown(requireUnconsumed=false)
                var current=settings
                val initial=geometry(current)
                active=initial.hit(down.position,mode,16.dp.toPx())
                var previous=down.position
                var total=androidx.compose.ui.geometry.Offset.Zero
                var count=1
                var multi=false
                var changed=false
                var pinch=1f
                val token="gizmo:${UUID.randomUUID()}"
                while(true) {
                    val event=awaitPointerEvent()
                    val fingers=event.changes.count {it.pressed}
                    if(fingers==0)break
                    if(fingers!=count) {
                        count=fingers;multi=multi || fingers>=2;active=-1;pinch=1f
                        event.changes.firstOrNull {it.pressed}?.let {previous=it.position}
                        continue
                    }
                    if(fingers>=2) {
                        pinch*=event.calculateZoom()
                        if(abs(pinch-1f)>0.015f || changed) {
                            current=current.copy(scale=(current.scale*pinch).coerceIn(0.1,400.0))
                            pinch=1f;ed.setObject(current,token);changed=true
                        }
                        event.changes.forEach {it.consume()}
                        continue
                    }
                    if(multi || active<0)continue
                    val point=event.changes.firstOrNull {it.pressed}?.position ?: continue
                    val delta=point-previous;total+=delta
                    if(!changed && total.getDistance()<viewConfiguration.touchSlop) {previous=point;continue}
                    val drag=if(!changed)total else delta
                    current=when(mode) {
                        GizmoMode.Move -> initial.move(current,active,drag)
                        GizmoMode.Rotate -> rotateGizmoLocal(current,active,geometry(current).rotationDelta(active,point-drag,drag))
                    }
                    previous=point
                    ed.setObject(current,token);changed=true
                    event.changes.forEach {it.consume()}
                }
                active=-1
            }
        }) {
            if (size.width <= 0f || size.height <= 0f || fw <= 0f || fh <= 0f) return@Canvas
            val geometry=GizmoGeometry(settings,placement,fw.toDouble(),fh.toDouble(),size.width.toDouble(),size.height.toDouble(),minOf(62.dp.toPx(),minOf(size.width,size.height).toFloat()*0.28f).toDouble())
            for(face in geometry.faces(mode)) {
                val base=axisColors[face.axis]
                val brightness=if(face.axis==active)1f else face.brightness.takeIf { it.isFinite() }?.coerceIn(0f,1f) ?: 0.4f
                val color=Color(base.red*brightness,base.green*brightness,base.blue*brightness,if(mode==GizmoMode.Rotate && face.behind)0.4f else 1f)
                val path=Path().apply {
                    moveTo(face.points[0].screen.x,face.points[0].screen.y)
                    face.points.drop(1).forEach {lineTo(it.screen.x,it.screen.y)}
                    close()
                }
                drawPath(path,color)
            }
        }
        Row(Modifier.align(Alignment.TopStart).padding(6.dp),horizontalArrangement=Arrangement.spacedBy(4.dp)) {
            GizmoMode.entries.forEach {m->SelectChip(m.name,mode==m,{mode=m},tag="gizmo_${m.name.lowercase()}")}
        }
        Row(Modifier.align(Alignment.BottomStart).padding(6.dp),horizontalArrangement=Arrangement.spacedBy(5.dp)) {
            listOf("X","Y","Z").forEachIndexed {i,label->androidx.compose.material3.Text(label,color=axisColors[i],modifier=Modifier.padding(3.dp))}
        }
    }
}
