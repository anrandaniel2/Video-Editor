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

/** A fused handle hit: which gizmo part and which axis. */
internal data class GizmoHit(val kind: GizmoMode, val axis: Int)

internal data class GizmoPose(val x:Double,val y:Double,val z:Double,val rotationX:Double=0.0,val rotationY:Double=0.0,val rotationZ:Double=0.0)

/**
 * A free camera in the scene's world units, used for the editor's grid view (an orbit around
 * the orbit-target point) and shared with the object/light gizmos so overlay and picture agree
 * on the projection. The render uses the same math natively (`SceneCamera3D`).
 */
internal class ViewProjection(val eye: GizmoVec, val yawDeg: Double, val pitchDeg: Double, val rollDeg: Double, val fov: Double, val frameWidth: Double, val frameHeight: Double, val viewWidth: Double, val viewHeight: Double) {
    constructor(target: GizmoVec, yawDeg: Double, pitchDeg: Double, rollDeg: Double, fov: Double, frameWidth: Double, frameHeight: Double, viewWidth: Double, viewHeight: Double, distance: Double)
        : this(GizmoVec(target.x - forward(pitchDeg, yawDeg, rollDeg).x * distance,
            target.y - forward(pitchDeg, yawDeg, rollDeg).y * distance,
            target.z - forward(pitchDeg, yawDeg, rollDeg).z * distance),
            yawDeg, pitchDeg, rollDeg, fov, frameWidth, frameHeight, viewWidth, viewHeight)

    private val right = GizmoVec(1.0, 0.0, 0.0).rotate(pitchDeg, yawDeg, rollDeg)
    private val down = GizmoVec(0.0, 1.0, 0.0).rotate(pitchDeg, yawDeg, rollDeg)
    private val forwardAxis = forward(pitchDeg, yawDeg, rollDeg)
    private val scaleX = viewWidth / frameWidth
    private val scaleY = viewHeight / frameHeight
    private val focal = (frameHeight / 2.0) / kotlin.math.tan(Math.toRadians(fov) / 2.0)
    /** The eye in world units. */
    fun eyePosition(): GizmoVec = eye
    /** Camera-space depth of a world point (negative when behind the eye). */
    fun depthOf(world: GizmoVec): Double {
        val d = world - eye
        return d.dot(forwardAxis)
    }
    /** Frame-pixel projection (before the screen-space layer affine and view scaling). */
    fun projectFrame(world: GizmoVec): GizmoPoint? {
        val d = world - eye
        if (!d.x.isFinite() || !d.y.isFinite() || !d.z.isFinite()) return null
        val vx = d.dot(right); val vy = d.dot(down); val vz = d.dot(forwardAxis)
        if (vz <= 1.0 || !vx.isFinite() || !vy.isFinite() || !vz.isFinite()) return null
        val sx = frameWidth / 2.0 + focal * vx / vz
        val sy = frameHeight / 2.0 + focal * vy / vz
        return GizmoPoint(Offset(sx.toFloat(), sy.toFloat()), vz)
    }
    fun project(world: GizmoVec): GizmoPoint? {
        val p = projectFrame(world) ?: return null
        return GizmoPoint(Offset(p.screen.x * scaleX.toFloat(), p.screen.y * scaleY.toFloat()), p.depth)
    }

    companion object {
        /** The forward axis (world space) for yaw/pitch/roll through the app's rotation convention. */
        fun forward(pitchDeg: Double, yawDeg: Double, rollDeg: Double): GizmoVec =
            GizmoVec(0.0, 0.0, 1.0).rotate(pitchDeg, yawDeg, rollDeg)
    }
}

/** Mesh display and hit-testing share the renderer's fixed, frame-centred camera. */
internal class GizmoGeometry(val pose:GizmoPose,private val layer:Placement?,private val fw:Double,private val fh:Double,private val width:Double,private val height:Double,pixels:Double,private val orbit:ViewProjection?=null,private val poseIsWorld:Boolean=false) {
    constructor(settings:Object3DSettings,layer:Placement?,fw:Double,fh:Double,width:Double,height:Double,pixels:Double):this(GizmoPose(settings.x,settings.y,settings.z,settings.rotationX,settings.rotationY,settings.rotationZ),layer,fw,fh,width,height,pixels)
    val origin=if(poseIsWorld)GizmoVec(pose.x,pose.y,pose.z)else GizmoVec((pose.x-fw/2)*pose.z.coerceAtLeast(1.0)/fh.coerceAtLeast(1.0),(pose.y-fh/2)*pose.z.coerceAtLeast(1.0)/fh.coerceAtLeast(1.0),pose.z.coerceAtLeast(1.0))
    val radius=(if(orbit!=null)pixels*orbit.depthOf(origin).coerceAtLeast(20.0)/fh.coerceAtLeast(1.0)/(width/fw.coerceAtLeast(1.0)) else pixels*origin.z/fh.coerceAtLeast(1.0)/(width/fw.coerceAtLeast(1.0)))/(layer?.scale?.absoluteValue ?: 1.0).coerceAtLeast(0.001)
    val axes=gizmoBasis.map {rotate(it)}
    fun rotate(v:GizmoVec)=v.rotate(pose.rotationX,pose.rotationY,pose.rotationZ)
    fun project(world:GizmoVec):GizmoPoint? {
        if(width <= 0.0 || height <= 0.0 || fw <= 0.0 || fh <= 0.0) return null
        // frame pixels before the layer affine: the orbit camera, or the classic fixed camera
        val base = orbit?.projectFrame(world)
            ?: if(world.z<1.0 || !world.x.isFinite() || !world.y.isFinite() || !world.z.isFinite()) null
            else GizmoPoint(Offset((fw/2+fh*world.x/world.z).toFloat(), (fh/2+fh*world.y/world.z).toFloat()), world.z)
            ?: return null
        // screen-space layer placement applies on top of either camera, about the frame centre
        val a=Math.toRadians(layer?.rotation ?: 0.0);val scale=(layer?.scale ?: 1.0)
        val x=(base.screen.x-fw/2)*scale;val y=(base.screen.y-fh/2)*scale
        val nx=(layer?.x ?: fw/2)+x*cos(a)-y*sin(a)
        val ny=(layer?.y ?: fh/2)+x*sin(a)+y*cos(a)
        return GizmoPoint(Offset((nx*width/fw).toFloat(),(ny*height/fh).toFloat()),base.depth)
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
    /** Both halves of the fused gizmo, nearest last (draw order) — move shafts + rotate rings. */
    fun fusedFaces():List<Pair<GizmoMode,GizmoFace>> =
        (faces(GizmoMode.Move).map { GizmoMode.Move to it } + faces(GizmoMode.Rotate).map { GizmoMode.Rotate to it })
            .sortedByDescending { it.second.depth }

    /** Hit test the fused gizmo: nearest face wins; rings and shafts pick up the slack by distance. */
    fun hitGizmo(point:Offset, tolerance:Float, kinds:Set<GizmoMode> = setOf(GizmoMode.Move, GizmoMode.Rotate)):GizmoHit? {
        val mesh = fusedFaces().filter { (kind, _) -> kind in kinds }
        for ((kind, face) in mesh.asReversed()) {
            val p = face.points.map { it.screen }
            if (p.size >= 3 && inside(point, p[0], p[1], p[2])) return GizmoHit(kind, face.axis)
        }
        val candidates = mutableListOf<Pair<GizmoHit, Float>>()
        for (axis in 0..2) {
            if (GizmoMode.Rotate in kinds) {
                ring(axis).map { it.screen }.zipWithNext()
                    .minOfOrNull { (a, b) -> segmentDistance(point, a, b) }
                    ?.takeIf { it < tolerance }
                    ?.let { candidates += GizmoHit(GizmoMode.Rotate, axis) to it }
            }
            if (GizmoMode.Move in kinds) {
                val pts = listOf(centre + (endpoint(axis) - centre) * 0.25f, endpoint(axis))
                segmentDistance(point, pts[0], pts[1]).takeIf { it < tolerance }
                    ?.let { candidates += GizmoHit(GizmoMode.Move, axis) to it }
            }
        }
        return candidates.minByOrNull { it.second }?.first
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

/** Rotate by [degrees] about [axis] (0=X, 1=Y, 2=Z) in local space; returns degrees for x/y/z. */
internal fun rotateEulerLocal(rotationX:Double,rotationY:Double,rotationZ:Double,axis:Int,degrees:Double):Triple<Double,Double,Double> {
    val delta=DoubleArray(3);delta[axis]=degrees
    val columns=gizmoBasis.map {it.rotate(delta[0],delta[1],delta[2]).rotate(rotationX,rotationY,rotationZ)}
    val y=asin((-columns[0].z).coerceIn(-1.0,1.0))
    val x:Double;val z:Double
    if(abs(cos(y))>1e-6) {x=atan2(columns[1].z,columns[2].z);z=atan2(columns[0].y,columns[0].x)}
    else {x=0.0;z=atan2(-columns[1].x,columns[1].y)}
    return Triple(Math.toDegrees(x),Math.toDegrees(y),Math.toDegrees(z))
}
internal fun rotateGizmoLocal(s:Object3DSettings,axis:Int,degrees:Double):Object3DSettings {
    val (x,y,z)=rotateEulerLocal(s.rotationX,s.rotationY,s.rotationZ,axis,degrees)
    return s.copy(rotationX=x,rotationY=y,rotationZ=z)
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
