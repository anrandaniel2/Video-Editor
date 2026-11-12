package io.github.artcraftandroid.filmcraft.ui.editor

import android.graphics.Bitmap
import android.net.Uri
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import io.github.artcraftandroid.filmcraft.core.BackgroundSettings
import io.github.artcraftandroid.filmcraft.core.BackgroundState
import io.github.artcraftandroid.filmcraft.core.CaptionInfo
import io.github.artcraftandroid.filmcraft.core.CaptionLook
import io.github.artcraftandroid.filmcraft.core.CaptionStatus
import io.github.artcraftandroid.filmcraft.core.ClipInfo
import io.github.artcraftandroid.filmcraft.core.ClipKind
import io.github.artcraftandroid.filmcraft.core.LaneKind
import io.github.artcraftandroid.filmcraft.core.MaskInfo
import io.github.artcraftandroid.filmcraft.core.MaskPath
import io.github.artcraftandroid.filmcraft.core.MaskPoint
import io.github.artcraftandroid.filmcraft.core.MaskSettings
import io.github.artcraftandroid.filmcraft.core.MaskShape
import io.github.artcraftandroid.filmcraft.core.MaskState
import io.github.artcraftandroid.filmcraft.core.MobileSession
import io.github.artcraftandroid.filmcraft.core.MotionTrackInfo
import io.github.artcraftandroid.filmcraft.core.NormPoint
import io.github.artcraftandroid.filmcraft.core.Object3DMesh
import io.github.artcraftandroid.filmcraft.core.Object3DSettings
import io.github.artcraftandroid.filmcraft.core.Object3DState
import io.github.artcraftandroid.filmcraft.core.Renderer
import io.github.artcraftandroid.filmcraft.core.SessionStatus
import io.github.artcraftandroid.filmcraft.core.SpeechAudio
import io.github.artcraftandroid.filmcraft.core.SpeechSettings
import io.github.artcraftandroid.filmcraft.core.StereoLayout
import io.github.artcraftandroid.filmcraft.core.TextStyle
import io.github.artcraftandroid.filmcraft.core.TrackOptions
import io.github.artcraftandroid.filmcraft.core.TimelineInfo
import io.github.artcraftandroid.filmcraft.core.TransitionInfo
import io.github.artcraftandroid.filmcraft.engine.Engine
import io.github.artcraftandroid.filmcraft.engine.ProjectEntry
import io.github.artcraftandroid.filmcraft.engine.Projects
import io.github.artcraftandroid.filmcraft.engine.intoBitmap
import io.github.artcraftandroid.filmcraft.engine.userMessage
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.util.UUID

/** A tool's panel. */
sealed interface Panel {
    data object OnlineVideos : Panel
    data object OnlineSounds : Panel
    data object Drawing : Panel
    data object Speed : Panel
    data object Volume : Panel
    data object Fade : Panel
    data object Opacity : Panel
    data object Mask : Panel
    data object Object3D : Panel
    /** The scene's coloured light objects. */
    data object Lights : Panel
    data object Keyframes : Panel
    data object Tracking : Panel
    data object Background : Panel
    data object Filters : Panel
    data object Effects : Panel
    /** The transition between [from] and [to] (either empty at the ends). */
    data class Transitions(val from: ULong?, val to: ULong?) : Panel
    /** Typing text: a new clip ([clip] empty), editing one, or a caption's words ([caption]). */
    data class TextInput(val clip: ULong?, val caption: ULong? = null) : Panel
    data object TextStyle : Panel
    data object AspectRatio : Panel
    data object Voiceover : Panel
    /** Captions: make them, and their look. */
    data object Captions : Panel
    /** Text to speech: new speech ([clip] empty) or saying a speech clip again. */
    data class Speech(val clip: ULong?) : Panel
}

/** The tool bar's submenus. */
enum class Submenu { None, Retime, ClipAudio, AddAudio, Files }

/** What the editor needs from the rest of the app. */
interface EditorHost {
    fun tell(message: String)
    /** Ask the user for media: [kind] picks the picker; [onPicked] gets what they chose. */
    fun pick(kind: PickKind, onPicked: (List<Uri>) -> Unit)
    fun close()
    fun export()
    /** Ask for the microphone; [onGranted] runs once it may be used. */
    fun requestMicrophone(onGranted: () -> Unit)
}

enum class PickKind { PhotosAndVideos, FilesVisual, FilesAudio, ReplaceVisual, ExtractAudioVideo, Model }

/** A scene light object: coloured, placeable in 3D, one of several (fields mirror the engine's `Light3D`). */
data class SceneLight(
    val id: Long = 0,
    val name: String = "Light",
    val enabled: Boolean = true,
    val kind: String = "directional",
    val color: List<Float> = listOf(1f, 1f, 1f),
    val intensity: Double = 1.0,
    val position: List<Double> = listOf(0.0, 0.0, 1500.0),
    val angle: Double = 135.0,
    val elevation: Double = 45.0,
    val range: Double = 0.0,
    val cone: Double = 35.0,
    val softness: Double = 0.5,
) {
    fun toJson(merge: String? = null): String {
        val j = JSONObject()
        j.put("id", id)
        j.put("name", name)
        j.put("enabled", enabled)
        j.put("kind", kind)
        j.put("color", JSONArray(color.map { it.toDouble() }))
        j.put("intensity", intensity)
        j.put("position", JSONArray(position))
        j.put("angle", angle)
        j.put("elevation", elevation)
        j.put("range", range)
        j.put("cone", cone)
        j.put("softness", softness)
        if (merge != null) j.put("merge", merge)
        return j.toString()
    }

    companion object {
        fun from(j: JSONObject) = SceneLight(
            id = j.optLong("id"),
            name = j.optString("name", "Light"),
            enabled = j.optBoolean("enabled", true),
            kind = j.optString("kind", "directional"),
            color = (0..2).map { j.optJSONArray("color")?.optDouble(it, 1.0)?.toFloat() ?: 1f },
            intensity = j.optDouble("intensity", 1.0),
            position = (0..2).map { j.optJSONArray("position")?.optDouble(it, if (it == 2) 1500.0 else 0.0) ?: if (it == 2) 1500.0 else 0.0 },
            angle = j.optDouble("angle", 135.0),
            elevation = j.optDouble("elevation", 45.0),
            range = j.optDouble("range", 0.0),
            cone = j.optDouble("cone", 35.0),
            softness = j.optDouble("softness", 0.5),
        )
    }
}

/** The sequence's lighting: its light objects and the ambient fill. */
data class SceneLightingState(
    val lights: List<SceneLight> = emptyList(),
    val ambientColor: List<Float> = listOf(1f, 1f, 1f),
    val ambientLevel: Double = 1.0,
) {
    fun ambientJson(merge: String? = null): String {
        val j = JSONObject()
        j.put("color", JSONArray(ambientColor.map { it.toDouble() }))
        j.put("level", ambientLevel)
        if (merge != null) j.put("merge", merge)
        return j.toString()
    }

    companion object {
        fun from(json: String): SceneLightingState {
            val j = JSONObject(json)
            val arr = j.optJSONArray("lights")
            return SceneLightingState(
                lights = (0 until (arr?.length() ?: 0)).map { SceneLight.from(arr!!.getJSONObject(it)) },
                ambientColor = (0..2).map { j.optJSONArray("ambientColor")?.optDouble(it, 1.0)?.toFloat() ?: 1f },
                ambientLevel = j.optDouble("ambientLevel", 1.0),
            )
        }
    }
}

/**
 * The open project in the editor: the timeline as the engine last reported it, the playhead,
 * the picture at it, playback, the selection and the open tool. Every edit goes through [edit],
 * which runs it on the engine's command thread, re-reads the timeline and saves the project.
 */
@Stable
class EditorController(
    val engine: Engine,
    project: ProjectEntry,
    private val projects: Projects,
    private val scope: CoroutineScope,
    private val host: EditorHost,
) {
    private val session: MobileSession get() = engine.session

    var project by mutableStateOf(project)
        private set
    var timeline by mutableStateOf<TimelineInfo?>(null)
        private set
    var status by mutableStateOf<SessionStatus?>(null)
        private set
    var picture by mutableStateOf<ImageBitmap?>(null)
        private set
    var playhead by mutableLongStateOf(0L)
        private set
    var playing by mutableStateOf(false)
        private set
    var panel by mutableStateOf<Panel?>(null)
    var submenu by mutableStateOf(Submenu.None)
    /** Timeline zoom; 0 until the timeline knows its width. */
    var pxPerSecond by mutableFloatStateOf(0f)
    /** The player's height (px) when the user has resized it, else 0 (55 % of the room). */
    var playerHeight by mutableFloatStateOf(0f)
    /** The player's height (px) last laid out without a panel: kept while one is open. */
    var basePlayerPx = 0f
    /** The side-by-side layout's player width fraction. */
    var sideFraction by mutableFloatStateOf(0f)
    /** Busy with something the user waits for (importing media). */
    var busy by mutableStateOf<String?>(null)
        private set
    /** Text being typed for a new text clip (shown on the preview until it is added). */
    var typing by mutableStateOf("")
    /** A clip is being dragged or trimmed: tools wait. */
    var gesture by mutableStateOf(false)

    val media = MediaCache(engine, scope)
    private var renderer: Renderer? = null
    private val player = Player()
    private var playJob: Job? = null
    private var saveJob: Job? = null
    private var lastSaved = -1L

    /** The preview's longest side in pixels (set by the preview from its size). */
    var previewSide = 720
        set(v) {
            field = v.coerceIn(160, 1280)
        }

    private data class FrameRequest(val time: Long, val draft: Boolean, val seq: Long, val snapshot: Renderer?, val interactive: Boolean)
    private var refineJob: Job? = null
    private var interactiveSide = 640
    private val frameRequests = MutableStateFlow<FrameRequest?>(null)
    private var requestSeq = 0L
    private val bitmaps = arrayOfNulls<Bitmap>(2)
    private var nextBitmap = 0

    init {
        scope.launch { renderFrames() }
    }

    // ---- reading ----

    val rateNum: Long get() = timeline?.rateNum ?: 30L
    val rateDen: Long get() = timeline?.rateDen ?: 1L
    val duration: Long get() = timeline?.duration ?: 0L
    val frameTicks: Long get() = TimelineMath.tickOfFrame(1, rateNum, rateDen)

    val selectedClip: ClipInfo?
        get() {
            val id = timeline?.selected ?: return null
            return timeline?.lanes?.firstNotNullOfOrNull { l -> l.clips.firstOrNull { it.id == id } }
        }

    /** The selected caption (picked here, not in the engine: captions aren't clips). */
    var selectedCaption by mutableStateOf<ULong?>(null)

    val selectedCaptionInfo: CaptionInfo?
        get() {
            val id = selectedCaption ?: return null
            return timeline?.captions?.firstOrNull { it.id == id }
        }

    val selectedTransition: TransitionInfo?
        get() {
            val id = timeline?.selectedTransition ?: return null
            return timeline?.transitions?.firstOrNull { it.id == id }
        }

    fun laneOf(clip: ULong): io.github.artcraftandroid.filmcraft.core.LaneInfo? = timeline?.lanes?.firstOrNull { l -> l.clips.any { it.id == clip } }

    fun isMain(clip: ULong): Boolean = laneOf(clip)?.kind == LaneKind.MAIN

    val mainClips: List<ClipInfo> get() = timeline?.lanes?.firstOrNull { it.kind == LaneKind.MAIN }?.clips.orEmpty()

    // ---- loading and saving ----

    /** Open the project: load or create its engine project, then show it. */
    suspend fun load(fresh: Boolean) {
        val t = engine.command {
            if (fresh || !project.projectFile.isFile) newProject(project.name, project.width.toUInt(), project.height.toUInt(), 30u)
            else openProject(project.projectFile.absolutePath)
        }
        timeline = t
        playhead = t.playhead
        refreshStatus()
        newRenderer()
        if (fresh) save()
        lastSaved = t.revision.toLong()
        requestFrame(playhead, draft = false)
    }

    private suspend fun refreshStatus() {
        status = engine.command { poll() }
    }

    private suspend fun newRenderer() {
        val r = runCatching { engine.command { renderer() } }.getOrNull() ?: return
        renderer = r
        media.renderer = r
    }

    /** Re-read the timeline after an edit; a new revision gets a new renderer and a save. */
    suspend fun refresh() {
        val t = runCatching { engine.command { timeline() } }.getOrNull() ?: return
        val changed = t.revision != timeline?.revision
        timeline = t
        // an edit that shortens the video past the playhead brings it back to the end
        playhead = if (t.playhead > t.duration) engine.command { setPlayhead(t.duration) } else t.playhead
        refreshStatus()
        if (changed) {
            newRenderer()
            scheduleSave()
        }
        requestFrame(playhead, draft = false)
    }

    private fun scheduleSave() {
        saveJob?.cancel()
        saveJob = scope.launch {
            delay(400)
            save()
        }
    }

    /** Save the project file and its listing now. */
    suspend fun save() {
        val t = timeline ?: return
        if (t.revision.toLong() == lastSaved) return
        val path = project.projectFile.absolutePath
        runCatching { engine.command { saveProject(path) } }.onFailure { host.tell(it.userMessage) }.onSuccess {
            lastSaved = t.revision.toLong()
            val p = project.copy(duration = t.duration, width = t.width.toInt(), height = t.height.toInt(), opened = System.currentTimeMillis())
            project = p
            withContext(Dispatchers.IO) { projects.write(p) }
            val now = System.currentTimeMillis()
            if (now - lastPoster > 20_000 || !p.poster.isFile) {
                lastPoster = now
                savePoster()
            }
        }
    }

    private var lastPoster = 0L

    /** The first frame, small, for the project's card on Home. */
    private suspend fun savePoster() {
        val r = renderer ?: return
        val bmp = withContext(engine.thumbnails) { runCatching { r.frame(0, 360u, false, null).intoBitmap(null) }.getOrNull() } ?: return
        withContext(Dispatchers.IO) { projects.savePoster(project, bmp) }
    }

    /** Save and make the poster for Home (the first frame). */
    suspend fun close() {
        stop()
        captionJob?.let { job -> runCatching { engine.command { cancelCaptions(job) } } }
        refreshJob?.join()
        refineJob?.cancel()
        saveJob?.cancel()
        save()
        savePoster()
        media.release()
        // the natural voices' model and export buffers aren't needed on Home
        withContext(Dispatchers.IO) { runCatching { io.github.artcraftandroid.filmcraft.core.trimMemory() } }
    }

    // ---- editing ----

    /**
     * Run an edit on the engine's command thread; then show the result. Playback stops first.
     * Errors become a message.
     */
    fun edit(block: MobileSession.() -> Unit) {
        stop()
        scope.launch {
            try {
                engine.command { block() }
            } catch (e: Exception) {
                host.tell(e.userMessage)
            }
            refresh()
        }
    }

    /** An edit that runs in the background without stopping playback or saving (slider drags). */
    fun quickEdit(block: MobileSession.() -> Unit) {
        scope.launch {
            try {
                engine.command { block() }
            } catch (e: Exception) {
                host.tell(e.userMessage)
            }
            refresh()
        }
    }

    fun undo() = edit { undo() }
    fun redo() = edit { redo() }

    fun select(clip: ULong?) {
        resetMaskTool()
        submenu = Submenu.None
        if (panel !is Panel.TextInput) panel = null
        selectedCaption = null
        quickEdit { select(clip) }
    }

    fun selectCaption(id: ULong) {
        submenu = Submenu.None
        if (panel !is Panel.TextInput && panel != Panel.Captions) panel = null
        selectedCaption = id
        if (timeline?.selected != null || timeline?.selectedTransition != null) quickEdit { select(null) }
    }

    fun selectTransition(t: TransitionInfo) {
        submenu = Submenu.None
        quickEdit { selectTransition(t.id) }
    }

    // ---- playhead and playback ----

    /** Move the playhead (the timeline scrolls under it); [draft] while the finger moves. */
    fun seek(t: Long, draft: Boolean = true) {
        if (playing) stop()
        val c = t.coerceIn(0L, duration.coerceAtLeast(0L))
        playhead = c
        requestFrame(c, draft)
        scope.launch { engine.command { setPlayhead(c) } }
    }

    /** The finger left the timeline: draw the frame at full quality. */
    fun settle() {
        requestFrame(playhead, draft = false)
    }

    fun step(frames: Int) {
        val f = TimelineMath.frames(playhead, rateNum, rateDen) + frames
        seek(TimelineMath.tickOfFrame(f.coerceAtLeast(0), rateNum, rateDen), draft = false)
    }

    fun togglePlay() = if (playing) stop() else play()

    fun play() {
        val end = duration
        if (end <= 0) return
        if (playhead >= end - frameTicks) playhead = 0
        playRange(playhead, end, back = null)
    }

    /** Play from [from] to [to], then stop there, or go back to [back] if given. */
    private fun playRange(from: Long, to: Long, back: Long?) {
        val r = renderer ?: return
        stop()
        val end = to.coerceIn(0L, duration)
        val start = from.coerceIn(0L, end)
        if (end - start < frameTicks) return
        playhead = start
        playing = true
        player.start(r, start, end)
        // the UI thread's dispatcher brings the display's frame clock (the view model's scope has none)
        playJob = scope.launch(androidx.compose.ui.platform.AndroidUiDispatcher.Main) {
            while (isActive && playing) {
                withFrameNanos { }
                val t = player.position().coerceAtMost(end)
                playhead = t
                requestFrame(t, draft = true)
                if (t >= end) break
            }
            if (playing) {
                playing = false
                player.stop()
                if (back != null) playhead = back.coerceIn(0L, duration)
                engine.command { setPlayhead(playhead) }
                requestFrame(playhead, draft = false)
            }
        }
    }

    /** Play the transition between [from] and [to], start to end; the playhead stays at its end. */
    private fun previewTransition(from: ULong?, to: ULong?) {
        val tr = timeline?.transitions?.firstOrNull { it.from == from && it.to == to } ?: return
        playRange(tr.start, tr.start + tr.duration, back = null)
    }

    fun stop() {
        if (!playing) return
        playing = false
        playJob?.cancel()
        playJob = null
        player.stop()
        val t = playhead
        scope.launch {
            engine.command { setPlayhead(t) }
            requestFrame(t, draft = false)
        }
    }

    private fun requestFrame(t: Long, draft: Boolean, interactive: Boolean = false) {
        refineJob?.cancel()
        frameRequests.value = FrameRequest(t, draft, ++requestSeq, renderer, interactive)
    }

    private suspend fun renderFrames() {
        frameRequests.filterNotNull().collect { req ->
            val r = req.snapshot ?: return@collect
            val side = when {
                req.interactive -> interactiveSide.coerceIn(160, previewSide)
                req.draft && playing -> (previewSide * 3 / 4).coerceAtLeast(160)
                else -> previewSide
            }
            val started = System.nanoTime()
            val frame = withContext(engine.preview) { runCatching { r.frame(req.time, side.toUInt(), req.draft, null) }.getOrNull() } ?: return@collect
            val bmp = frame.intoBitmap(bitmaps[nextBitmap]) ?: return@collect
            bitmaps[nextBitmap] = bmp
            nextBitmap = 1 - nextBitmap
            picture = bmp.asImageBitmap()
            if (req.interactive) {
                val elapsed = System.nanoTime() - started
                // Adapt working resolution, not input cadence, when a phone cannot render a
                // complex frame inside 33 ms. A settled frame always returns to full quality.
                interactiveSide = LivePreviewPolicy.nextSide(side, previewSide, elapsed)
            }
            if (req.draft && !req.interactive && !playing && frameRequests.value?.seq == req.seq) {
                // Never sleep inside the frame collector: that used to hold newer input up
                // for 120 ms. Refinement is independent and cancelled by the very next edit.
                refineJob = scope.launch {
                    delay(100)
                    if (frameRequests.value?.seq == req.seq && !playing && !liveEditing) requestFrame(req.time, draft = false)
                }
            }
        }
    }

    // ---- adding media ----

    /** Add pictures and videos from the phone's gallery, files, or sounds from files. */
    fun addMedia(kind: PickKind) {
        submenu = Submenu.None
        host.pick(kind) { uris -> if (uris.isNotEmpty()) importAndAdd(uris, audio = kind == PickKind.FilesAudio) }
    }

    fun addOverlay() {
        submenu = Submenu.None
        host.pick(PickKind.FilesVisual) { uris -> if (uris.isNotEmpty()) importAndAdd(uris, audio = false, overlay = true) }
    }

    /** Add picked media; [first] is a new project's starting media, which can't be undone away. */
    fun importAndAdd(uris: List<Uri>, audio: Boolean, first: Boolean = false, overlay: Boolean = false) {
        stop()
        scope.launch {
            busy = "Adding media…"
            try {
                val paths = withContext(Dispatchers.IO) { uris.mapNotNull { u -> runCatching { projects.importUri(project, u).absolutePath }.getOrNull() } }
                if (paths.isEmpty()) {
                    host.tell("That file can't be added.")
                } else {
                    engine.command {
                        if (overlay) addOverlays(paths) else if (audio) addAudio(paths) else addMedia(paths)
                        if (first) forgetHistory()
                    }
                    if (paths.size < uris.size) host.tell("Some files couldn't be added.")
                }
            } catch (e: Exception) {
                host.tell(e.userMessage)
            } finally {
                busy = null
            }
            refresh()
        }
    }

    var onlineImportError by mutableStateOf<String?>(null)

    fun importOnlineVideo(url: String, attribution: String, overlay: Boolean) {
        if (busy != null) return
        stop()
        onlineImportError = null
        scope.launch {
            busy = "Downloading video…"
            try {
                val path = withContext(Dispatchers.IO) {
                    val file = downloadSound(url, project.mediaDir, video = true)
                    try { java.io.File(file.parentFile, file.name + ".credits.txt").writeText(attribution) }
                    catch (e: Exception) { file.delete(); throw e }
                    file.absolutePath
                }
                engine.command { if (overlay) addOverlays(listOf(path)) else addMedia(listOf(path)) }
                panel = null
            } catch (e: kotlinx.coroutines.CancellationException) { throw e }
            catch (e: Exception) { onlineImportError = "Import failed: ${e.userMessage}"; host.tell(e.userMessage) }
            finally { busy = null; refresh() }
        }
    }

    fun importOnlineSound(url: String, attribution: String = "") {
        if (busy != null) return
        stop()
        onlineImportError = null
        scope.launch {
            busy = "Downloading sound…"
            try {
                val path = withContext(Dispatchers.IO) {
                    val file = downloadSound(url, project.mediaDir)
                    if (attribution.isNotBlank()) {
                        try { java.io.File(file.parentFile, file.name + ".credits.txt").writeText(attribution) }
                        catch (e: Exception) { file.delete(); throw e }
                    }
                    file.absolutePath
                }
                engine.command { addAudio(listOf(path)) }
                panel = null
            } catch (e: kotlinx.coroutines.CancellationException) { throw e }
            catch (e: Exception) { onlineImportError = "Import failed: ${e.userMessage}"; host.tell(e.userMessage) }
            finally { busy = null; refresh() }
        }
    }

    var blankDrawing = false

    fun importDrawing(bitmap: android.graphics.Bitmap) {
        scope.launch {
            busy = "Adding drawing…"
            try {
                val path = withContext(Dispatchers.IO) {
                    project.mediaDir.mkdirs()
                    val file = java.io.File(project.mediaDir, "drawing-${java.util.UUID.randomUUID()}.png")
                    file.outputStream().use { check(bitmap.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it)) }
                    file.absolutePath
                }
                engine.command { addDrawing(path) }
                panel = null
            } catch (e: Exception) { host.tell(e.userMessage) }
            finally { bitmap.recycle(); busy = null; refresh() }
        }
    }

    fun replaceSelected() {
        val c = selectedClip ?: return
        host.pick(PickKind.ReplaceVisual) { uris ->
            val u = uris.firstOrNull() ?: return@pick
            scope.launch {
                busy = "Replacing…"
                try {
                    val path = withContext(Dispatchers.IO) { projects.importUri(project, u).absolutePath }
                    engine.command { replace(c.id, path) }
                } catch (e: Exception) {
                    host.tell(e.userMessage)
                } finally {
                    busy = null
                }
                refresh()
            }
        }
    }

    /** Extract the sound of a video the user picks, onto an audio lane at the playhead. */
    fun extractAudioFromFile() {
        submenu = Submenu.None
        host.pick(PickKind.ExtractAudioVideo) { uris -> if (uris.isNotEmpty()) importAndAdd(uris, audio = true) }
    }

    // ---- clip tools ----

    private inline fun withSelected(block: (ClipInfo) -> Unit) {
        selectedClip?.let(block)
    }

    fun split() = withSelected { c -> edit { split(c.id) } }
    fun delete() = withSelected { c -> panel = null; submenu = Submenu.None; edit { deleteClip(c.id) } }
    fun duplicate() = withSelected { c -> edit { duplicate(c.id) } }
    fun freeze() = withSelected { c -> submenu = Submenu.None; edit { freeze(c.id) } }
    fun extractAudio() = withSelected { c -> submenu = Submenu.None; edit { extractAudio(c.id) } }
    fun toggleMirror() = withSelected { c -> edit { setFlip(c.id, true, !c.mirrored) } }
    fun toggleFlip() = withSelected { c -> edit { setFlip(c.id, false, !c.flipped) } }
    fun addBlank() = edit { addBlank() }

    fun setSpeed(speed: Double, done: Boolean) = withSelected { c ->
        if (done) edit { setSpeed(c.id, speed) }
    }

    fun setVolume(v: Double) = withSelected { c -> quickEdit { setVolume(c.id, v, true) } }
    fun setMuted(m: Boolean) = withSelected { c -> edit { setMuted(c.id, m) } }
    fun setFades(fadeIn: Long, fadeOut: Long) = withSelected { c -> edit { setFades(c.id, fadeIn, fadeOut) } }
    fun setOpacity(v: Double, all: Boolean = false) = withSelected { c -> quickEdit { setOpacity(c.id, v, !all, all) } }
    fun setFilter(id: String, amount: Double, merge: Boolean, all: Boolean = false) = withSelected { c -> quickEdit { setFilter(c.id, id, amount, merge, all) } }
    fun setEffect(id: String, amount: Double, merge: Boolean, all: Boolean = false) = withSelected { c -> quickEdit { setEffect(c.id, id, amount, merge, all) } }

    /** Put a transition on a cut (none for a blank [preset]) and play it. */
    fun setTransition(from: ULong?, to: ULong?, preset: String, seconds: Double, all: Boolean = false) {
        stop()
        scope.launch {
            try {
                engine.command { setTransition(from, to, preset, seconds, all) }
            } catch (e: Exception) {
                host.tell(e.userMessage)
            }
            refresh()
            if (preset.isNotEmpty()) previewTransition(from, to)
        }
    }

    fun setTransitionDuration(id: ULong, seconds: Double) {
        stop()
        scope.launch {
            try {
                engine.command { setTransitionDuration(id, seconds) }
            } catch (e: Exception) {
                host.tell(e.userMessage)
            }
            refresh()
            val tr = timeline?.transitions?.firstOrNull { it.id == id } ?: return@launch
            previewTransition(tr.from, tr.to)
        }
    }

    suspend fun maxTransition(from: ULong?, to: ULong?): Double = runCatching { engine.command { maxTransition(from, to) } }.getOrDefault(5.0)

    fun trim(clip: ULong, head: Boolean, delta: Long) = edit { trim(clip, head, delta) }
    fun reorder(clip: ULong, index: Int) = edit { reorder(clip, index.toUInt()) }
    fun move(clip: ULong, lane: Int, start: Long) = edit { moveClip(clip, lane, start) }

    fun setAspect(num: Int, den: Int) = edit { setAspect(num.toUInt(), den.toUInt()) }

    // ---- text ----

    fun addText(text: String) = edit { addText(text) }
    fun setText(clip: ULong, text: String) = edit { setText(clip, text) }

    suspend fun textStyle(clip: ULong): TextStyle? = runCatching { engine.command { textStyle(clip) } }.getOrNull()

    fun setTextStyle(clip: ULong, style: TextStyle, merge: Boolean, all: Boolean = false) = quickEdit { setTextStyle(clip, style, merge, all) }

    // ---- speech ----

    /** Speech of [st] to hear before adding it (null when it can't be made; the user is told). */
    suspend fun previewSpeech(st: SpeechSettings): SpeechAudio? =
        runCatching { engine.command { previewSpeech(st) } }.onFailure { host.tell(it.userMessage) }.getOrNull()

    suspend fun speechSettings(clip: ULong): SpeechSettings? = runCatching { engine.command { speechSettings(clip) } }.getOrNull()

    /** Add speech at the playhead, or say speech clip [clip] again. */
    fun saySpeech(clip: ULong?, st: SpeechSettings) {
        stop()
        scope.launch {
            busy = "Making speech…"
            try {
                val dir = project.mediaDir.apply { mkdirs() }.absolutePath
                engine.command { if (clip == null) addSpeech(st, dir) else editSpeech(clip, st, dir) }
            } catch (e: Exception) {
                host.tell(e.userMessage)
            } finally {
                busy = null
            }
            refresh()
        }
    }

    // ---- captions ----

    /** Captions being made: how far along (null when nothing is running). */
    var captionProgress by mutableStateOf<CaptionStatus?>(null)
        private set
    private var captionJob: ULong? = null

    /** Make captions from what is said, with speech model [model] in [language] ("auto" detects it). */
    fun makeCaptions(model: String, language: String) {
        if (captionProgress != null) return
        stop()
        captionProgress = CaptionStatus(0.0, "Getting ready…", false, null, 0u)
        scope.launch {
            try {
                val job = engine.command { startCaptions(model, language) }
                captionJob = job
                while (true) {
                    val st = engine.command { captionStatus(job) }
                    captionProgress = st
                    if (st.finished) {
                        when {
                            st.error == "cancelled" -> {}
                            st.error != null -> host.tell(st.error ?: "")
                            st.captions == 0u -> host.tell("No speech was heard, so there are no captions.")
                        }
                        break
                    }
                    delay(300)
                }
            } catch (e: Exception) {
                host.tell(e.userMessage)
            } finally {
                captionJob = null
                captionProgress = null
            }
            refresh()
        }
    }

    fun cancelCaptions() {
        val job = captionJob ?: return
        scope.launch { runCatching { engine.command { cancelCaptions(job) } } }
    }

    suspend fun captionLook(): CaptionLook? = runCatching { engine.command { captionLook() } }.getOrNull()

    fun setCaptionLook(look: CaptionLook, merge: Boolean) = quickEdit { setCaptionLook(look, merge) }

    fun setCaptionText(id: ULong, text: String) = edit { setCaptionText(id, text) }

    fun deleteCaption(id: ULong) {
        selectedCaption = null
        edit { deleteCaption(id) }
    }

    fun deleteCaptions() {
        selectedCaption = null
        edit { deleteCaptions() }
    }

    // ---- placement on the frame ----

    internal suspend fun previewObjectSettings(clip: ULong): Object3DSettings? =
        try { engine.command { object3dState(clip) }.takeIf { it.hasObject }?.settings }
        catch (e: kotlinx.coroutines.CancellationException) { throw e }
        catch (_: Exception) { null }

    suspend fun placementOf(clip: ULong): io.github.artcraftandroid.filmcraft.core.Placement? =
        runCatching { engine.command { placement(clip) } }.getOrNull()

    /** Move/scale/turn a clip while a finger does it: one undo step per gesture. */
    fun setPlacement(clip: ULong, p: io.github.artcraftandroid.filmcraft.core.Placement, merge: Boolean) {
        queueLiveEdit("placement:$clip", begin = !merge) { first ->
            setPlacement(clip, p.x, p.y, p.scale, p.rotation, !first)
        }
    }

    private data class LiveEdit(val begin: Boolean, val operation: MobileSession.(Boolean) -> Unit)
    private val liveEdits = linkedMapOf<String, LiveEdit>()
    var liveEditing by mutableStateOf(false)
        private set
    private var refreshJob: Job? = null
    private var refreshPending = false

    /** One latest edit per control, one native batch and one fresh render snapshot per 30 Hz
     * tick. Pointer events never build an unbounded command queue. Timeline polling, saving,
     * thumbnails and full-resolution refinement wait until the gesture settles. */
    private fun queueLiveEdit(key: String, begin: Boolean = false, edit: MobileSession.(Boolean) -> Unit) {
        liveEdits[key] = LiveEdit(begin || liveEdits[key]?.begin == true, edit)
        refreshSoon()
    }

    private fun refreshSoon() {
        refreshPending = true
        if (refreshJob?.isActive == true) return
        liveEditing = true
        refineJob?.cancel()
        refreshJob = scope.launch {
            try {
                var lastEdit = System.nanoTime()
                while (isActive) {
                    val tick = System.nanoTime()
                    if (refreshPending || liveEdits.isNotEmpty()) {
                        refreshPending = false
                        lastEdit = tick
                        val edits = liveEdits.values.toList()
                        liveEdits.clear()
                        val errors = mutableListOf<String>()
                        val snapshot = try {
                            engine.command {
                                edits.forEach { edit ->
                                    try { edit.operation.invoke(this, edit.begin) }
                                    catch (e: Exception) { errors += e.userMessage }
                                }
                                renderer()
                            }
                        } catch (e: CancellationException) { throw e }
                        catch (e: Exception) { errors += e.userMessage; null }
                        errors.firstOrNull()?.let { host.tell(it) }
                        if (snapshot != null) {
                            renderer = snapshot
                            media.renderer = snapshot
                            requestFrame(playhead, draft = true, interactive = true)
                        }
                    } else if (tick - lastEdit >= 100_000_000L) break
                    val remaining = LivePreviewPolicy.FRAME_NANOS - (System.nanoTime() - tick)
                    if (remaining > 0) delay((remaining / 1_000_000L).coerceAtLeast(1))
                }
                liveEditing = false
                refresh()
            } finally {
                liveEditing = false
                refreshJob = null
            }
            if (refreshPending || liveEdits.isNotEmpty()) refreshSoon()
        }
    }

    // ---- clip masks ----

    /** A snapshot for the mask panel; null until the selected picture has been read. */
    var maskState by mutableStateOf<MaskState?>(null)
        private set
    var selectedMaskIndex by mutableStateOf(0u)
    var selectedMaskVertex by mutableStateOf(-1)
    /** A free-draw stroke is being captured on the preview. */
    var maskDrawing by mutableStateOf(false)
    /** The next tap on a path inserts a point on its nearest edge. */
    var maskAddingPoint by mutableStateOf(false)
    /** Suppress snapshot reloads while a finger owns the optimistic path. */
    var maskGesture by mutableStateOf(false)
        private set

    val selectedMask: MaskInfo? get() = maskState?.masks?.firstOrNull { it.index == selectedMaskIndex }

    fun newMaskGesture(): String = UUID.randomUUID().toString()

    fun resetMaskTool() {
        maskState = null
        selectedMaskIndex = 0u
        selectedMaskVertex = -1
        maskDrawing = false
        maskAddingPoint = false
        maskGesture = false
    }

    suspend fun loadMasks(clip: ULong) {
        if (maskGesture) return
        val st = runCatching { engine.command { clipMasks(clip) } }.getOrNull() ?: return
        maskState = st
        if (st.masks.none { it.index == selectedMaskIndex }) selectedMaskIndex = st.masks.lastOrNull()?.index ?: 0u
        if ((selectedMask?.path?.vertices?.size ?: 0) <= selectedMaskVertex) selectedMaskVertex = -1
    }

    fun selectMask(index: UInt) {
        selectedMaskIndex = index
        selectedMaskVertex = -1
        maskAddingPoint = false
    }

    fun beginMaskGesture() {
        stop()
        maskGesture = true
    }

    fun endMaskGesture() {
        maskGesture = false
        refreshSoon()
    }

    /** Mask drags use the placement gesture's coalesced refresh, not a save/render per pointer event. */
    private fun maskCommand(key: String, block: MobileSession.() -> Unit) {
        stop()
        queueLiveEdit(key) { block() }
    }

    private fun optimisticMask(change: (MaskInfo) -> MaskInfo) {
        val st = maskState ?: return
        maskState = st.copy(masks = st.masks.map { if (it.index == selectedMaskIndex) change(it) else it })
    }

    fun addMask(shape: MaskShape, points: List<Pair<Double, Double>> = emptyList()) {
        val c = selectedClip ?: return
        stop()
        maskDrawing = false
        scope.launch {
            try {
                val st = engine.command { addClipMask(c.id, shape, points.map { MaskPoint(it.first, it.second) }) }
                maskState = st
                selectedMaskIndex = st.masks.lastOrNull()?.index ?: 0u
                selectedMaskVertex = -1
            } catch (e: Exception) {
                host.tell(e.userMessage)
            }
            refresh()
        }
    }

    fun setMaskSettings(settings: MaskSettings, merge: String? = null) {
        val c = selectedClip ?: return
        val index = selectedMaskIndex
        optimisticMask { it.copy(settings = settings) }
        maskCommand("mask-settings:${c.id}:$index") { setClipMask(c.id, index, settings, merge) }
    }

    fun setMaskPath(path: MaskPath, merge: String? = null) {
        val c = selectedClip ?: return
        val index = selectedMaskIndex
        optimisticMask { it.copy(path = path) }
        maskCommand("mask-path:${c.id}:$index") { setClipMaskPath(c.id, index, path, merge) }
    }

    fun addMaskVertex(after: Int, at: Pair<Double, Double>) {
        val c = selectedClip ?: return
        val index = selectedMaskIndex
        maskAddingPoint = false
        scope.launch {
            try {
                maskState = engine.command { addClipMaskVertex(c.id, index, after.toUInt(), MaskPoint(at.first, at.second)) }
                selectedMaskVertex = after + 1
            } catch (e: Exception) { host.tell(e.userMessage) }
            refresh()
        }
    }

    fun removeMaskVertex() {
        val c = selectedClip ?: return
        val index = selectedMaskIndex
        val vertex = selectedMaskVertex
        if (vertex < 0 || (selectedMask?.path?.vertices?.size ?: 0) <= 3) return
        selectedMaskVertex = -1
        scope.launch {
            try { maskState = engine.command { removeClipMaskVertex(c.id, index, vertex.toUInt()) } }
            catch (e: Exception) { host.tell(e.userMessage) }
            refresh()
        }
    }

    fun deleteMask() {
        val c = selectedClip ?: return
        val index = selectedMaskIndex
        selectedMaskVertex = -1
        scope.launch {
            try {
                val st = engine.command { deleteClipMask(c.id, index) }
                maskState = st
                selectedMaskIndex = st.masks.lastOrNull()?.index ?: 0u
            } catch (e: Exception) { host.tell(e.userMessage) }
            refresh()
        }
    }

    fun clearMasks() {
        val c = selectedClip ?: return
        resetMaskTool()
        scope.launch {
            try { maskState = engine.command { clearClipMasks(c.id) } }
            catch (e: Exception) { host.tell(e.userMessage) }
            refresh()
        }
    }

    // ---- 3D objects, tracking and background removal ----

    enum class BackgroundPointMode { None, Keep, Remove }

    var object3DState by mutableStateOf<Object3DState?>(null)
        private set
    var backgroundState by mutableStateOf<BackgroundState?>(null)
        private set
    var motionTracks by mutableStateOf<List<MotionTrackInfo>>(emptyList())
        private set
    var trackingTarget by mutableStateOf<Pair<Double, Double>?>(null)
    var trackingLayout by mutableStateOf(StereoLayout.MONO)
    var trackingBusy by mutableStateOf(false)
        private set
    var backgroundPointMode by mutableStateOf(BackgroundPointMode.None)
    var selectedTrackId by mutableStateOf(0uL)

    fun resetAdvancedTools() {
        backgroundPointMode = BackgroundPointMode.None
    }

    suspend fun loadObject3D(clip: ULong) {
        if (liveEditing && object3DState?.clip == clip) return
        object3DState = runCatching { engine.command { object3dState(clip) } }.getOrNull()
    }

    suspend fun loadBackground(clip: ULong) {
        backgroundState = runCatching { engine.command { backgroundState(clip) } }.getOrNull()
    }

    suspend fun loadTracks() {
        motionTracks = runCatching { engine.command { motionTracks() } }.getOrDefault(emptyList())
        if (motionTracks.none { it.id == selectedTrackId }) selectedTrackId = motionTracks.lastOrNull()?.id ?: 0uL
    }

    fun addObject(mesh: Object3DMesh) {
        stop()
        scope.launch {
            try {
                val id = engine.command { addObject3d(mesh) }
                refresh()
                panel = Panel.Object3D
                loadObject3D(id)
            } catch (e: Exception) { host.tell(e.userMessage) }
        }
    }

    /** Pick a glTF/GLB file, copy it into the project, validate it and add it as a 3D overlay. */
    fun addModel() {
        stop()
        host.pick(PickKind.Model) { uris ->
            val uri = uris.firstOrNull() ?: return@pick
            scope.launch {
                busy = "Importing model…"
                try {
                    val path = withContext(Dispatchers.IO) { projects.importUri(project, uri).absolutePath }
                    val id = engine.command { addModel(path) }
                    refresh()
                    panel = Panel.Object3D
                    loadObject3D(id)
                } catch (e: Exception) {
                    host.tell(e.userMessage)
                } finally {
                    busy = null
                }
            }
        }
    }

    fun setObject(settings: Object3DSettings, merge: String? = null) {
        val c = selectedClip ?: return
        object3DState = object3DState?.copy(settings = settings)
        stop()
        queueLiveEdit("object:${c.id}") { setObject3d(c.id, settings, merge) }
    }

    fun toggleObjectAnimation() {
        val c = selectedClip ?: return
        stop()
        scope.launch {
            try {
                engine.command { toggleObject3dAnimation(c.id) }
                loadObject3D(c.id)
            } catch (e: Exception) { host.tell(e.userMessage) }
            refresh()
        }
    }

    fun extrudeText(depth: Double = 48.0, bevel: Double = 4.0) {
        val c = selectedClip ?: return
        stop()
        scope.launch {
            try {
                val id = engine.command { extrudeText(c.id, depth, bevel) }
                refresh()
                panel = Panel.Object3D
                loadObject3D(id)
            } catch (e: Exception) { host.tell(e.userMessage) }
        }
    }

    suspend fun animationState(clip: ULong): JSONObject =
        JSONObject(engine.command { execute("animation.list", """{"clip":$clip}""") })

    fun animationCommand(command: String, body: JSONObject) {
        stop()
        val json = body.toString()
        if (body.has("merge")) {
            val key = listOf(command, body.opt("clip"), body.opt("effect"), body.opt("mask"), body.opt("param"), body.opt("mediaTime")).joinToString(":")
            queueLiveEdit(key) { execute(command, json); Unit }
            return
        }
        scope.launch {
            try {
                engine.command { execute(command, json) }
                refresh()
            } catch (e: Exception) { host.tell(e.userMessage) }
        }
    }

    // ---- scene lights ----

    var sceneLights by mutableStateOf(SceneLightingState())
        private set

    suspend fun loadLights() {
        sceneLights = runCatching {
            SceneLightingState.from(engine.command { execute("lights.list", "{}") })
        }.getOrDefault(SceneLightingState())
    }

    /** Add a light object of `kind` ("directional", "point" or "spot"); `onAdded` gets its id. */
    fun addLight(kind: String, onAdded: (Long) -> Unit = {}) {
        stop()
        scope.launch {
            try {
                val light = SceneLight(name = "Light ${sceneLights.lights.size + 1}", kind = kind)
                val out = engine.command { execute("lights.set", light.toJson()) }
                sceneLights = SceneLightingState.from(out)
                onAdded(JSONObject(out).optLong("id"))
                refreshSoon()
            } catch (e: Exception) { host.tell(e.userMessage) }
        }
    }

    /** Put a light object in the scene (its `id` edits it; 0 creates one). */
    fun setLight(light: SceneLight, merge: String? = null) {
        stop()
        sceneLights = sceneLights.copy(lights = sceneLights.lights.map { if (it.id == light.id) light else it })
        val body = light.toJson(merge)
        queueLiveEdit("light:${light.id}") { execute("lights.set", body); Unit }
    }

    fun removeLight(id: Long) {
        stop()
        scope.launch {
            try {
                val out = engine.command { execute("lights.remove", """{"id":$id}""") }
                sceneLights = SceneLightingState.from(out)
                refreshSoon()
            } catch (e: Exception) { host.tell(e.userMessage) }
        }
    }

    /** The ambient fill: colour × level. */
    fun setAmbient(ambientColor: List<Float>, ambientLevel: Double, merge: String? = null) {
        stop()
        sceneLights = sceneLights.copy(ambientColor = ambientColor, ambientLevel = ambientLevel)
        val body = sceneLights.ambientJson(merge)
        queueLiveEdit("ambient") { execute("lights.ambient", body); Unit }
    }

    fun trackMotion(stereo: StereoLayout, baseline: Double, maxSeconds: Double) {
        val c = selectedClip ?: return
        val target = trackingTarget ?: (0.5 to 0.5)
        stop()
        trackingBusy = true
        scope.launch {
            try {
                val info = engine.command {
                    trackMotion(c.id, TrackOptions(target.first, target.second, 15u, stereo, baseline, maxSeconds))
                }
                selectedTrackId = info.id
                loadTracks()
                val lock = info.quality.toInt()
                host.tell(
                    if (lock < 40) "Tracked ${info.samples} frames but only $lock% stayed locked. Tap a spot with more detail and track again."
                    else "Tracked ${info.samples} frames · $lock% lock" + if (info.lost > 0u) " · lost ${info.lost}" else "",
                )
            } catch (e: Exception) { host.tell(e.userMessage) }
            trackingBusy = false
            refresh()
        }
    }

    fun deleteTrack(id: ULong) {
        scope.launch {
            try { engine.command { deleteMotionTrack(id) } }
            catch (e: Exception) { host.tell(e.userMessage) }
            loadTracks()
            refresh()
        }
    }

    fun followTrack(trackId: ULong, follow: Boolean) {
        val st = object3DState?.settings ?: return
        setObject(st.copy(followTrack = follow, trackId = trackId))
    }

    fun removeBackground() {
        val c = selectedClip ?: return
        stop()
        scope.launch {
            try {
                backgroundState = engine.command { removeBackground(c.id) }
                refresh()
            } catch (e: Exception) { host.tell(e.userMessage) }
        }
    }

    fun setBackground(settings: BackgroundSettings, merge: String? = null) {
        val c = selectedClip ?: return
        val current = backgroundState ?: BackgroundState(c.id, false, BackgroundSettings(38.0, 6.0, 2.0, false, 12.0, 70.0, emptyList(), emptyList()))
        backgroundState = current.copy(enabled = true, settings = settings)
        stop()
        scope.launch {
            try {
                engine.command { setBackground(c.id, settings, merge) }
                loadBackground(c.id)
            } catch (e: Exception) { host.tell(e.userMessage) }
            refreshSoon()
        }
    }

    fun onAdvancedTap(x: Double, y: Double) {
        when (panel) {
            Panel.Tracking -> trackingTarget = x to y
            Panel.Background -> {
                val st = backgroundState?.settings ?: BackgroundSettings(38.0, 6.0, 2.0, false, 12.0, 70.0, emptyList(), emptyList())
                val p = NormPoint(x, y)
                val next = when (backgroundPointMode) {
                    BackgroundPointMode.Keep -> st.copy(keepPoints = st.keepPoints + p)
                    BackgroundPointMode.Remove -> st.copy(removePoints = st.removePoints + p)
                    BackgroundPointMode.None -> return
                }
                setBackground(next)
            }
            else -> {}
        }
    }

    // ---- voice-over ----

    private var voiceStart = 0L

    fun requestMicrophone(onGranted: () -> Unit) = host.requestMicrophone(onGranted)

    fun tellUser(message: String) = host.tell(message)

    /** A recorder for a new take at the playhead, writing "Voiceover N.wav" into the project. */
    fun newVoiceRecorder(): VoiceRecorder? {
        stop()
        voiceStart = playhead
        val dir = project.mediaDir.apply { mkdirs() }
        var n = 1
        while (java.io.File(dir, "Voiceover $n.wav").exists()) n++
        return VoiceRecorder(java.io.File(dir, "Voiceover $n.wav"))
    }

    /** Put a finished take on an audio lane where it started. */
    fun finishVoiceover(r: VoiceRecorder, onAdded: (ULong) -> Unit) {
        val path = r.file.absolutePath
        val at = voiceStart
        scope.launch {
            try {
                if (r.frames < r.rate / 10) {
                    java.io.File(path).delete()
                } else {
                    val ids = engine.command {
                        setPlayhead(at)
                        addAudio(listOf(path))
                    }
                    ids.firstOrNull()?.let(onAdded)
                }
            } catch (e: Exception) {
                host.tell(e.userMessage)
            }
            refresh()
        }
    }

    fun discardTakes(ids: List<ULong>) = edit { ids.forEach { runCatching { deleteClip(it) } } }

    fun closeEditor() = host.close()

    // ---- export ----

    fun export() {
        stop()
        val d = duration
        if (d < frameTicks * 5) {
            host.tell("Add at least a few frames to the timeline to export.")
            return
        }
        host.export()
    }

    fun clipKindHasPicture(c: ClipInfo) = c.kind != ClipKind.AUDIO && c.kind != ClipKind.TEXT
}
