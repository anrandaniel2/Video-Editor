package io.github.artcraftandroid.filmcraft.ui.editor

import androidx.compose.ui.geometry.Offset
import io.github.artcraftandroid.filmcraft.core.Object3DSettings
import io.github.artcraftandroid.filmcraft.core.Placement
import kotlin.math.*

internal data class GizmoVec(val x:Double,val y:Double,val z:Double) {
    operator fun plus(v:GizmoVec)=GizmoVec(x+v.x,y+v.y,z+v.z)
    operator fun minus(v:GizmoVec)=GizmoVec(x-v.x,y-v.y,z-v.z)
    operator fun times(n:Double)=GizmoVec(x*n,y*n,z*n)
    fun dot(v:GizmoVec)=x*v.x+y*v.y+z*v.z
    fun cross(v:GizmoVec)=GizmoVec(y*v.z-z*v.y,z*v.x-x*v.z,x*v.y-y*v.x)
    fun unit():GizmoVec {val n=sqrt(dot(this)).coerceAtLeast(1e-9);return this*(1/n)}
    fun rotate(x:Double,y:Double,z:Double):GizmoVec {
        val rx=Math.toRadians(x);val ry=Math.toRadians(y);val rz=Math.toRadians(z)
        val yy=this.y*cos(rx)-this.z*sin(rx);val zz=this.y*sin(rx)+this.z*cos(rx)
        val xx=this.x*cos(ry)+zz*sin(ry);val zzz=-this.x*sin(ry)+zz*cos(ry)
        return GizmoVec(xx*cos(rz)-yy*sin(rz),xx*sin(rz)+yy*cos(rz),zzz)
    }
}
internal val gizmoBasis=listOf(GizmoVec(1.0,0.0,0.0),GizmoVec(0.0,1.0,0.0),GizmoVec(0.0,0.0,1.0))
internal data class GizmoPoint(val screen:Offset,val depth:Double)
internal data class GizmoFace(val axis:Int,val points:List<GizmoPoint>,val brightness:Float,val behind:Boolean) {
    val depth get()=points.sumOf {it.depth}/points.size
}
internal enum class GizmoMode { Move,Rotate }

internal data class GizmoPose(val x:Double,val y:Double,val z:Double,val rotationX:Double=0.0,val rotationY:Double=0.0,val rotationZ:Double=0.0)

/** Mesh display and hit-testing share the renderer's fixed, frame-centred camera. */
internal class GizmoGeometry(val pose:GizmoPose,private val layer:Placement?,private val fw:Double,private val fh:Double,private val width:Double,private val height:Double,pixels:Double) {
    constructor(settings:Object3DSettings,layer:Placement?,fw:Double,fh:Double,width:Double,height:Double,pixels:Double):this(GizmoPose(settings.x,settings.y,settings.z,settings.rotationX,settings.rotationY,settings.rotationZ),layer,fw,fh,width,height,pixels)
    val origin=GizmoVec((pose.x-fw/2)*pose.z.coerceAtLeast(1.0)/fh.coerceAtLeast(1.0),(pose.y-fh/2)*pose.z.coerceAtLeast(1.0)/fh.coerceAtLeast(1.0),pose.z.coerceAtLeast(1.0))
    val radius=pixels*origin.z/fh.coerceAtLeast(1.0)/(width/fw.coerceAtLeast(1.0))/(layer?.scale?.absoluteValue ?: 1.0).coerceAtLeast(0.001)
    val axes=gizmoBasis.map {rotate(it)}
    fun rotate(v:GizmoVec)=v.rotate(pose.rotationX,pose.rotationY,pose.rotationZ)
    fun project(world:GizmoVec):GizmoPoint? {
        if(world.z<1.0)return null
        val lx=fw/2+fh*world.x/world.z
        val ly=fh/2+fh*world.y/world.z
        val a=Math.toRadians(layer?.rotation ?: 0.0);val scale=layer?.scale ?: 1.0
        val x=(lx-fw/2)*scale;val y=(ly-fh/2)*scale
        val px=(layer?.x ?: fw/2)+x*cos(a)-y*sin(a)
        val py=(layer?.y ?: fh/2)+x*sin(a)+y*cos(a)
        return GizmoPoint(Offset((px*width/fw).toFloat(),(py*height/fh).toFloat()),world.z)
    }
    fun local(v:GizmoVec)=project(origin+rotate(v))
    val centre get()=project(origin)?.screen ?: Offset.Zero
    fun endpoint(axis:Int)=local(gizmoBasis[axis]*radius)?.screen ?: centre
    fun ring(axis:Int):List<GizmoPoint> {
        val u=gizmoBasis[(axis+1)%3];val v=gizmoBasis[(axis+2)%3]
        return (0..96).mapNotNull {i -> val a=i*2*PI/96;local((u*cos(a)+v*sin(a))*(radius*0.9))}
    }
    fun faces(mode:GizmoMode):List<GizmoFace> {
        val faces=mutableListOf<GizmoFace>()
        fun triangle(axis:Int,a:GizmoVec,b:GizmoVec,c:GizmoVec) {
            val world=listOf(a,b,c).map {origin+rotate(it)}
            val projected=world.mapNotNull {project(it)}
            if(projected.size!=3)return
            val normal=(world[1]-world[0]).cross(world[2]-world[0]).unit()
            val light=GizmoVec(-0.4,-0.6,-1.0).unit()
            val brightness=(0.4+0.6*abs(normal.dot(light))).toFloat()
            faces+=GizmoFace(axis,projected,brightness,world.sumOf {it.z}/3>origin.z)
        }
        fun quad(axis:Int,a:GizmoVec,b:GizmoVec,c:GizmoVec,d:GizmoVec) {triangle(axis,a,b,c);triangle(axis,a,c,d)}
        for(axis in 0..2) {
            val direction=gizmoBasis[axis];val u=gizmoBasis[(axis+1)%3];val v=gizmoBasis[(axis+2)%3]
            when(mode) {
                GizmoMode.Move -> {
                    val n=12
                    for(i in 0 until n) {
                        val a=i*2*PI/n;val b=(i+1)*2*PI/n
                        val ra=u*cos(a)+v*sin(a);val rb=u*cos(b)+v*sin(b)
                        quad(axis,direction*(radius*0.14)+ra*(radius*0.024),direction*(radius*0.78)+ra*(radius*0.024),direction*(radius*0.78)+rb*(radius*0.024),direction*(radius*0.14)+rb*(radius*0.024))
                        triangle(axis,direction*radius,direction*(radius*0.74)+ra*(radius*0.085),direction*(radius*0.74)+rb*(radius*0.085))
                        triangle(axis,direction*(radius*0.74),direction*(radius*0.74)+rb*(radius*0.085),direction*(radius*0.74)+ra*(radius*0.085))
                    }
                }
                GizmoMode.Rotate -> {
                    // Torus meshes lie in the object's three local rotation planes.
                    for(i in 0 until 64) {
                        val a=i*2*PI/64;val b=(i+1)*2*PI/64
                        val ra=u*cos(a)+v*sin(a);val rb=u*cos(b)+v*sin(b)
                        for(j in 0 until 4) {
                            val t=j*PI/2;val q=(j+1)*PI/2
                            val pa=ra*(radius*(0.9+0.018*cos(t)))+direction*(radius*0.018*sin(t))
                            val pb=rb*(radius*(0.9+0.018*cos(t)))+direction*(radius*0.018*sin(t))
                            val pc=rb*(radius*(0.9+0.018*cos(q)))+direction*(radius*0.018*sin(q))
                            val pd=ra*(radius*(0.9+0.018*cos(q)))+direction*(radius*0.018*sin(q))
                            quad(axis,pa,pb,pc,pd)
                        }
                    }
                }
            }
        }
        return faces.sortedByDescending {it.depth}
    }
    fun hit(point:Offset,mode:GizmoMode,tolerance:Float):Int {
        // Test visible, nearest mesh faces first, using the same triangles we display.
        val mesh=faces(mode)
        for(face in mesh.asReversed()) {
            val p=face.points.map {it.screen}
            if(inside(point,p[0],p[1],p[2]))return face.axis
        }
        return (0..2).map {axis ->
            val points=if(mode==GizmoMode.Rotate) ring(axis).map {it.screen} else listOf(centre+(endpoint(axis)-centre)*0.25f,endpoint(axis))
            axis to points.zipWithNext().minOfOrNull {(a,b)->segmentDistance(point,a,b)}
        }.filter {it.second!=null && it.second!!<tolerance}.minByOrNull {it.second!!}?.first ?: -1
    }
    /** Screen tangent per world unit along a local axis. */
    fun tangent(axis:Int):Offset {
        val epsilon=radius*0.01
        return ((project(origin+axes[axis]*epsilon)?.screen ?: centre)-centre)/(epsilon.toFloat().coerceAtLeast(1e-6f))
    }
    fun move(current:Object3DSettings,axis:Int,drag:Offset):Object3DSettings {
        val t=tangent(axis);val length=t.x*t.x+t.y*t.y
        // Looking directly down an axis collapses its projection. Its cone is still pickable;
        // drag vertically to push/pull it in depth instead of dividing by a near-zero tangent.
        val amount:Double=if(length>0.0001f)((drag.x*t.x+drag.y*t.y)/length).toDouble() else -drag.y*radius/64.0
        val d=axes[axis]*amount.toDouble()
        val old=GizmoVec((current.x-fw/2)*current.z/fh,(current.y-fh/2)*current.z/fh,current.z)
        val next=old+d;val z=next.z.coerceIn(1.0,10000.0)
        return current.copy(x=fw/2+fh*next.x/z,y=fh/2+fh*next.y/z,z=z)
    }
    fun rotationDelta(axis:Int,point:Offset,drag:Offset):Double {
        val samples=ring(axis)
        if(samples.size<97)return 0.0
        val index=(0 until 96).minByOrNull {segmentDistance(point,samples[it].screen,samples[it+1].screen)} ?: return 0.0
        val tangent=(samples[index+1].screen-samples[index].screen)/(2*PI/96).toFloat()
        val length=tangent.x*tangent.x+tangent.y*tangent.y
        return if(length>1e-5) Math.toDegrees(((drag.x*tangent.x+drag.y*tangent.y)/length).toDouble()).coerceIn(-20.0,20.0) else 0.0
    }
}

internal fun rotateGizmoLocal(s:Object3DSettings,axis:Int,degrees:Double):Object3DSettings {
    val delta=DoubleArray(3);delta[axis]=degrees
    val columns=gizmoBasis.map {it.rotate(delta[0],delta[1],delta[2]).rotate(s.rotationX,s.rotationY,s.rotationZ)}
    val y=asin((-columns[0].z).coerceIn(-1.0,1.0))
    val x:Double;val z:Double
    if(abs(cos(y))>1e-6) {x=atan2(columns[1].z,columns[2].z);z=atan2(columns[0].y,columns[0].x)}
    else {x=0.0;z=atan2(-columns[1].x,columns[1].y)}
    return s.copy(rotationX=Math.toDegrees(x),rotationY=Math.toDegrees(y),rotationZ=Math.toDegrees(z))
}
internal fun segmentDistance(p:Offset,a:Offset,b:Offset):Float {
    val d=b-a;val length=d.x*d.x+d.y*d.y
    val u=if(length>0f)(((p-a).x*d.x+(p-a).y*d.y)/length).coerceIn(0f,1f) else 0f
    return (p-(a+d*u)).getDistance()
}
private fun inside(p:Offset,a:Offset,b:Offset,c:Offset):Boolean {
    fun cross(u:Offset,v:Offset)=u.x*v.y-u.y*v.x
    val area=cross(b-a,c-a)
    if(abs(area)<0.01f)return false
    val u=cross(b-p,c-p)/area;val v=cross(c-p,a-p)/area
    return u>=0f && v>=0f && u+v<=1f
}
