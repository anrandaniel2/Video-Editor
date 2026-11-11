package io.github.artcraftandroid.filmcraft.ui.editor

import io.github.artcraftandroid.filmcraft.core.Object3DMesh
import io.github.artcraftandroid.filmcraft.core.Object3DSettings
import io.github.artcraftandroid.filmcraft.core.Placement
import org.junit.Assert.*
import org.junit.Test
import kotlin.math.abs

class GizmoGeometryTest {
    private fun settings()=Object3DSettings(
        mesh=Object3DMesh.CUBE,text="",font="Inter",fontStyle="Regular",align=1u,tracking=0.0,
        kerning=true,ligatures=true,leading=100.0,baselineShift=0.0,fauxBold=false,fauxItalic=false,
        caps=0u,underline=false,boxWidth=500.0,vertical=false,size=220.0,depth=220.0,bevel=0.0,
        x=540.0,y=960.0,z=1000.0,rotationX=0.0,rotationY=0.0,rotationZ=0.0,scale=100.0,
        color=0xffffffffu,sideColor=0xff808080u,metalness=0.0,roughness=50.0,
        lightAngle=135.0,lightElevation=45.0,opacity=100.0,followTrack=false,trackId=0uL,trackDepth=240.0,
        modelPath="",modelAnim=-1,modelTime=0.0,modelPlay=true,modelLoop=true,modelSpeed=100.0,
    )
    private fun geometry(s:Object3DSettings=settings(),layer:Placement?=null)=GizmoGeometry(s,layer,1080.0,1920.0,540.0,960.0,64.0)

    @Test fun projectionUsesDepthAndTheSameFocalLengthAsObjects() {
        val g=geometry()
        val near=requireNotNull(g.project(GizmoVec(100.0,0.0,1000.0)))
        val far=requireNotNull(g.project(GizmoVec(100.0,0.0,2000.0)))
        assertEquals(366.0,near.screen.x.toDouble(),0.001)
        assertEquals(318.0,far.screen.x.toDouble(),0.001)
        assertNull(g.project(GizmoVec(0.0,0.0,0.5)))
    }
    @Test fun axesAndRingsRotateInThreeDimensionsInsteadOfStayingFlat() {
        val s=settings().copy(rotationY=90.0,rotationX=25.0)
        val g=geometry(s)
        val axis=g.axes[0]
        assertTrue(axis.z< -0.99)
        val ring=g.ring(2)
        assertTrue(ring.maxOf {it.depth}-ring.minOf {it.depth}>30.0)
        assertTrue(abs(g.endpoint(2).x-g.centre.x)>20.0)
    }
    @Test fun layerRotationAndScaleAreAppliedAfterPerspectiveProjection() {
        val g=geometry(layer=Placement(x=100.0,y=200.0,width=1080.0,height=1920.0,scale=2.0,rotation=90.0))
        val p=requireNotNull(g.project(GizmoVec(10.0,0.0,1000.0)))
        assertEquals(50.0,p.screen.x.toDouble(),0.001)
        assertEquals(119.2,p.screen.y.toDouble(),0.001)
    }
    @Test fun localRotationComposesRatherThanAddingAnUnrelatedScreenAngle() {
        val s=settings().copy(rotationX=25.0,rotationY=40.0,rotationZ=12.0)
        val result=rotateGizmoLocal(s,1,10.0)
        for(v in gizmoBasis) {
            val expected=v.rotate(0.0,10.0,0.0).rotate(s.rotationX,s.rotationY,s.rotationZ)
            val actual=v.rotate(result.rotationX,result.rotationY,result.rotationZ)
            assertEquals(expected.x,actual.x,1e-6)
            assertEquals(expected.y,actual.y,1e-6)
            assertEquals(expected.z,actual.z,1e-6)
        }
    }
    @Test fun handlesHaveRealFacesAndPickingUsesTheirProjection() {
        val g=geometry(settings().copy(rotationX=20.0,rotationY=35.0))
        val faces=g.faces(GizmoMode.Move)
        assertEquals(setOf(0,1,2),faces.map {it.axis}.toSet())
        assertTrue(faces.zipWithNext().all {(a,b)->a.depth>=b.depth})
        assertEquals(0,g.hit(g.endpoint(0),GizmoMode.Move,12f))
    }
}
