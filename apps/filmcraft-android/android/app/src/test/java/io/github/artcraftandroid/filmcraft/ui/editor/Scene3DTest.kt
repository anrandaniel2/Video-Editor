package io.github.artcraftandroid.filmcraft.ui.editor

import androidx.compose.ui.graphics.Color
import io.github.artcraftandroid.filmcraft.core.MatchMoveSettings
import io.github.artcraftandroid.filmcraft.core.Object3DMesh
import io.github.artcraftandroid.filmcraft.core.Object3DSettings
import io.github.artcraftandroid.filmcraft.core.SceneCamera3d
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

class Scene3DTest {
    private fun cubeAt(x: Double = 540.0, y: Double = 960.0, z: Double = 900.0) = Object3DSettings(
        mesh = Object3DMesh.CUBE, text = "", font = "Inter", fontStyle = "Regular", align = 1u, tracking = 0.0,
        kerning = true, ligatures = true, leading = 100.0, baselineShift = 0.0, fauxBold = false, fauxItalic = false,
        caps = 0u, underline = false, boxWidth = 500.0, vertical = false, size = 200.0, depth = 200.0, bevel = 0.0,
        x = x, y = y, z = z, rotationX = 0.0, rotationY = 0.0, rotationZ = 0.0, scale = 100.0,
        color = 0xffffffffu, sideColor = 0xff808080u, metalness = 0.0, roughness = 50.0, envReflection = 100.0, emissive = 0.0,
        lightAngle = 135.0, lightElevation = 45.0, opacity = 100.0, followTrack = false, trackId = 0uL, trackDepth = 240.0,
        modelPath = "", modelAnim = -1, modelTime = 0.0, modelPlay = true, modelLoop = true, modelSpeed = 100.0,
    )

    @Test
    fun fromNativeMapsAllCameraFields() {
        val cam = SceneCamera.fromNative(SceneCamera3d(listOf(120.0, -45.0, 310.0), 12.5, -6.0, 2.0, 48.0))
        assertEquals(listOf(120.0, -45.0, 310.0), cam.position)
        assertEquals(12.5, cam.yaw, 1e-6)
        assertEquals(-6.0, cam.pitch, 1e-6)
        assertEquals(2.0, cam.roll, 1e-6)
        assertEquals(48.0, cam.fov, 1e-6)
    }

    @Test
    fun effectiveRenderCameraPrefersMatchMoveOverManualCamera() {
        val solved = SceneCamera(listOf(80.0, 0.0, 150.0), 4.0, -2.0, 0.0, 50.0)
        val manual = SceneCamera.DEFAULT
        assertSame(solved, effectiveRenderCamera(solved, manual))
        assertSame(manual, effectiveRenderCamera(null, manual))
        assertNull(effectiveRenderCamera(null, null))
    }

    @Test
    fun matchMoveCameraParallaxShiftsWorldObjectInGizmoProjection() {
        val fw = 1080.0
        val fh = 1920.0
        val camCenter = SceneCamera(listOf(0.0, 0.0, 0.0), 0.0, 0.0, 0.0, CLASSIC_FOV)
        val camTruckRight = SceneCamera(listOf(180.0, 0.0, 0.0), 0.0, 0.0, 0.0, CLASSIC_FOV)

        val nearObj = cubeAt(z = 800.0)
        val farObj = cubeAt(z = 1600.0)

        val near0 = GizmoGeometry(nearObj, null, fw, fh, fw, fh, 64.0, cameraProjection(camCenter, fw, fh, fw, fh)).centre
        val near1 = GizmoGeometry(nearObj, null, fw, fh, fw, fh, 64.0, cameraProjection(camTruckRight, fw, fh, fw, fh)).centre
        val far0 = GizmoGeometry(farObj, null, fw, fh, fw, fh, 64.0, cameraProjection(camCenter, fw, fh, fw, fh)).centre
        val far1 = GizmoGeometry(farObj, null, fw, fh, fw, fh, 64.0, cameraProjection(camTruckRight, fw, fh, fw, fh)).centre

        val nearShift = (near0.x - near1.x).toDouble()
        val farShift = (far0.x - far1.x).toDouble()
        assertTrue("near object shifts left when camera trucks right", nearShift > 200.0)
        assertEquals("parallax scales inversely with depth", nearShift * 0.5, farShift, 0.5)
    }

    @Test
    fun landmarkColorClassifiesReprojectionRms() {
        assertNotNull(landmarkColor(0.4))
        assertEquals(Color(0xFF6FE295), landmarkColor(0.6))
        assertEquals(Color(0xFFFFC947), landmarkColor(1.8))
        assertEquals(Color(0xFFFF6575), landmarkColor(3.4))
    }

    @Test
    fun frameNormalizedAtTapHandlesLetterboxPillarboxAndBars() {
        // 16:9 frame (1920x1080) inside square 1000x1000 container -> fitted 1000x562.5 with oy = 218.75.
        val center = frameNormalizedAtTap(500f, 500f, 1000f, 1000f, 1920f, 1080f)
        assertNotNull(center)
        assertEquals(0.5, center!!.first, 1e-4)
        assertEquals(0.5, center.second, 1e-4)

        val quarter = frameNormalizedAtTap(250f, 218.75f + 562.5f * 0.25f, 1000f, 1000f, 1920f, 1080f)
        assertNotNull(quarter)
        assertEquals(0.25, quarter!!.first, 1e-4)
        assertEquals(0.25, quarter.second, 1e-4)

        // Tap on top/bottom letterbox black bars returns null.
        assertNull(frameNormalizedAtTap(500f, 100f, 1000f, 1000f, 1920f, 1080f))
        assertNull(frameNormalizedAtTap(500f, 900f, 1000f, 1000f, 1920f, 1080f))
        assertNull(frameNormalizedAtTap(10f, 10f, 0f, 1000f, 1920f, 1080f))
    }

    @Test
    fun sceneOrbitDragAndPinchPanMoveViewPredictably() {
        val base = SceneOrbit.DEFAULT
        val orbited = base.orbitBy(dx = 100f, dy = -40f)
        assertEquals(base.yaw + 35.0, orbited.yaw, 1e-6)
        assertEquals(base.pitch + 14.0, orbited.pitch, 1e-6)

        val clamped = base.orbitBy(dx = 0f, dy = 1000f)
        assertEquals(-85.0, clamped.pitch, 1e-6)

        // Straight-on orbit (yaw = 0, pitch = 0) at distance 700, CLASSIC_FOV, viewHeight = 700:
        // worldPerPx = 2 * 700 * 0.5 / 700 = 1.0, so panning (+50, -30) shifts target by (-50, +30, 0).
        val aligned = SceneOrbit(target = listOf(0.0, 0.0, 700.0), yaw = 0.0, pitch = 0.0, distance = 700.0)
        val panned = aligned.dollyAndPanBy(zoom = 1f, panX = 50f, panY = -30f, fov = CLASSIC_FOV, viewHeightPx = 700f)
        assertEquals(700.0, panned.distance, 1e-6)
        assertEquals(-50.0, panned.target[0], 1e-3)
        assertEquals(30.0, panned.target[1], 1e-3)
        assertEquals(700.0, panned.target[2], 1e-3)

        val zoomedIn = aligned.dollyAndPanBy(zoom = 2f, panX = 0f, panY = 0f, fov = CLASSIC_FOV, viewHeightPx = 700f)
        assertEquals(350.0, zoomedIn.distance, 1e-6)
    }

    @Test
    fun gizmoRadiusRemainsConstantOnScreenAcrossSolvedMatchMoveFovs() {
        val fw = 1080.0
        val fh = 1920.0
        val obj = cubeAt(z = 900.0)
        for (fov in listOf(24.0, CLASSIC_FOV, 88.0)) {
            val cam = SceneCamera(listOf(0.0, 0.0, 0.0), 0.0, 0.0, 0.0, fov)
            val g = GizmoGeometry(obj, null, fw, fh, fw, fh, 64.0, cameraProjection(cam, fw, fh, fw, fh))
            val screenLen = (g.endpoint(0) - g.centre).getDistance().toDouble()
            assertEquals("screen handle length stays ~64 px at fov=$fov", 64.0, screenLen, 0.5)
        }
    }

    @Test
    fun rotateMatchMoveCameraPivotsAroundCurrentEyeWithoutDriftingPosition() {
        val mm = MatchMoveSettings(1uL, true, 700.0, 8.0, -4.0, 2.0, 20.0, -15.0, 40.0)
        val cam = SceneCamera(listOf(140.0, -60.0, 220.0), 18.0, -7.0, 3.0, 50.0)
        val (nextMm, nextCam) = rotateMatchMoveCamera(cam, mm, axis = 1, degrees = 12.0)
        assertEquals(cam.position[0], nextCam.position[0], 1e-6)
        assertEquals(cam.position[1], nextCam.position[1], 1e-6)
        assertEquals(cam.position[2], nextCam.position[2], 1e-6)
        assertTrue(kotlin.math.abs(nextMm.yawTrim - mm.yawTrim) > 5.0)
    }

    @Test
    fun blenderLightConeSpecReflectsEmitDirectionConeSoftnessAndRange() {
        // Default light (135° / 45°) shines forward (+Z) into the 3D scene, never backwards (-Z).
        val emit = lightEmitDirection(135.0, 45.0)
        assertTrue("emission points forward into +Z", emit.z > 0.5)
        val (roundAngle, roundElev) = lightAnglesFromEmit(emit)
        assertEquals(135.0, roundAngle, 1e-4)
        assertEquals(45.0, roundElev, 1e-4)

        val narrowHard = SceneLight(kind = "spot", cone = 18.0, softness = 0.0, range = 600.0)
        val wideSoft = narrowHard.copy(cone = 52.0, softness = 0.65, range = 2200.0)
        val specA = buildLightConeSpec(narrowHard, 80.0)
        val specB = buildLightConeSpec(wideSoft, 80.0)

        assertTrue(specA.hasAimTarget && specB.hasAimTarget)
        assertNull(specA.innerRadius)
        assertNotNull(specB.innerRadius)
        assertTrue("wider cone + longer range produces wider base", specB.outerRadius > specA.outerRadius * 2.0)
        assertTrue("longer falloff range extends beam length", specB.beamLength > specA.beamLength)
        assertTrue("softness penumbra ring lies strictly inside outer cone", specB.innerRadius!! < specB.outerRadius)

        val pointSpec = buildLightConeSpec(SceneLight(kind = "point", range = 1000.0), 80.0)
        assertFalse(pointSpec.hasAimTarget)
    }

    @Test
    fun sceneLightJsonRoundTripsKeyframeMetadata() {
        val raw = JSONObject()
            .put("id", 7L)
            .put("name", "Key Spot")
            .put("enabled", true)
            .put("kind", "spot")
            .put("intensity", 2.4)
            .put("cone", 42.0)
            .put("softness", 0.4)
            .put("animated", true)
            .put("hasKeyframe", true)
            .put("interpolation", "smooth")
            .put("keyframes", org.json.JSONArray(listOf(0L, TPS, TPS * 2)))
        val parsed = SceneLight.from(raw)
        assertEquals(7L, parsed.id)
        assertTrue(parsed.animated)
        assertTrue(parsed.hasKeyframe)
        assertEquals("smooth", parsed.interpolation)
        assertEquals(listOf(0L, TPS, TPS * 2), parsed.keyframes)
    }
}
