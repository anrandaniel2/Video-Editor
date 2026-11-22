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
    var active by remember(clip.id) {mutableStateOf<GizmoHit?>(null)}
    val latestState by rememberUpdatedState(state)
    val settings by rememberUpdatedState(state.settings)
    val transform by rememberUpdatedState(placement)
    fun effectiveOf(s:Object3DSettings,st:io.github.artcraftandroid.filmcraft.core.Object3DState):Object3DSettings=
        if(s.followTrack && s.trackId!=0uL) {
            val relScale=(st.trackedScale/st.settings.scale.coerceAtLeast(0.01)).coerceAtLeast(0.01)
            val dthetaDeg=st.trackedRotationZ-st.settings.rotationZ
            val rad=Math.toRadians(dthetaDeg)
            val dx=s.x-st.settings.x
            val dy=s.y-st.settings.y
            val dz=s.z-st.settings.z
            s.copy(
                x=st.trackedX+relScale*(cos(rad)*dx-sin(rad)*dy),
                y=st.trackedY+relScale*(sin(rad)*dx+cos(rad)*dy),
                z=st.trackedZ+dz,
                scale=(s.scale*relScale).coerceIn(0.1,10000.0),
                rotationZ=s.rotationZ+dthetaDeg,
            )
        } else s
    Box(modifier.testTag("object3d_gizmo")) {
        Canvas(Modifier.fillMaxSize().pointerInput(clip.id,fw,fh) {
            fun geometry(s:Object3DSettings)=GizmoGeometry(s,transform,fw.toDouble(),fh.toDouble(),size.width.toDouble(),size.height.toDouble(),minOf(62.dp.toPx(),minOf(size.width,size.height).toFloat()*0.28f).toDouble(),viewFor(ed,fw.toDouble(),fh.toDouble(),size.width.toDouble(),size.height.toDouble()))
            awaitEachGesture {
                val down=awaitFirstDown(requireUnconsumed=false)
                if(down.isConsumed)return@awaitEachGesture
                val stSnap=latestState
                var current=settings
                val initial=geometry(effectiveOf(current,stSnap))
                active=initial.hitGizmo(down.position,16.dp.toPx())
                if(active!=null) {ed.stop();down.consume()}
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
                    if(fingers==0) {
                        if(active!=null) event.changes.forEach {it.consume()}
                        break
                    }
                    if(fingers!=count) {
                        count=fingers;multi=multi || fingers>=2;active=null;pinch=1f
                        event.changes.firstOrNull {it.pressed}?.let {previous=it.position}
                        continue
                    }
                    if(fingers>=2) {
                        if (ed.panel == Panel.Scene) {
                            // the scene overlay owns two fingers (orbit dolly)
                            multi = true; active = null
                            continue
                        }
                        pinch*=event.calculateZoom()
                        if(abs(pinch-1f)>0.015f || changed) {
                            current=current.copy(scale=(current.scale*pinch).coerceIn(0.1,400.0))
                            pinch=1f;ed.setObject(current,token);changed=true
                        }
                        event.changes.forEach {it.consume()}
                        continue
                    }
                    if(multi || active==null)continue
                    val hit=active!!
                    val point=event.changes.firstOrNull {it.pressed}?.position ?: continue
                    val delta=point-previous;total+=delta
                    if(!changed && total.getDistance()<viewConfiguration.touchSlop) {previous=point;continue}
                    val drag=if(!changed)total else delta
                    val eff=effectiveOf(current,stSnap)
                    current=when(hit.kind) {
                        GizmoMode.Move -> {
                            val movedEff=geometry(eff).move(eff,hit.axis,drag)
                            val dxScreen=movedEff.x-eff.x
                            val dyScreen=movedEff.y-eff.y
                            val dzScreen=movedEff.z-eff.z
                            if(current.followTrack && current.trackId!=0uL) {
                                val relScale=(stSnap.trackedScale/stSnap.settings.scale.coerceAtLeast(0.01)).coerceAtLeast(0.01)
                                val rad=Math.toRadians(stSnap.trackedRotationZ-stSnap.settings.rotationZ)
                                val dxAnc=(cos(rad)*dxScreen+sin(rad)*dyScreen)/relScale
                                val dyAnc=(-sin(rad)*dxScreen+cos(rad)*dyScreen)/relScale
                                current.copy(x=current.x+dxAnc,y=current.y+dyAnc,z=(current.z+dzScreen).coerceIn(20.0,20000.0))
                            } else {
                                current.copy(x=movedEff.x,y=movedEff.y,z=movedEff.z)
                            }
                        }
                        GizmoMode.Rotate -> rotateGizmoLocal(current,hit.axis,geometry(eff).rotationDelta(hit.axis,point-drag,drag))
                    }
                    previous=point
                    ed.setObject(current,token);changed=true
                    event.changes.forEach {it.consume()}
                }
                active=null
            }
        }) {
            if (size.width <= 0f || size.height <= 0f || fw <= 0f || fh <= 0f) return@Canvas
            val effDraw=effectiveOf(settings,latestState)
            val geometry=GizmoGeometry(effDraw,placement,fw.toDouble(),fh.toDouble(),size.width.toDouble(),size.height.toDouble(),minOf(62.dp.toPx(),minOf(size.width,size.height).toFloat()*0.28f).toDouble(),viewFor(ed,fw.toDouble(),fh.toDouble(),size.width.toDouble(),size.height.toDouble()))
            for((kind,face) in geometry.fusedFaces()) {
                val base=axisColors[face.axis]
                val brightness=if(active?.kind==kind && active?.axis==face.axis)1f else face.brightness.takeIf { it.isFinite() }?.coerceIn(0f,1f) ?: 0.4f
                val color=Color(base.red*brightness,base.green*brightness,base.blue*brightness,if(kind==GizmoMode.Rotate && face.behind)0.4f else 1f)
                val path=Path().apply {
                    moveTo(face.points[0].screen.x,face.points[0].screen.y)
                    face.points.drop(1).forEach {lineTo(it.screen.x,it.screen.y)}
                    close()
                }
                drawPath(path,color)
            }
        }
        Row(Modifier.align(Alignment.BottomStart).padding(6.dp),horizontalArrangement=Arrangement.spacedBy(5.dp)) {
            listOf("X","Y","Z").forEachIndexed {i,label->androidx.compose.material3.Text(label,color=axisColors[i],modifier=Modifier.padding(3.dp))}
        }
    }
}
