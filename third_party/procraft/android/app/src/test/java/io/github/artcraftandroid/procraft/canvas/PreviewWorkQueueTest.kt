package io.github.artcraftandroid.procraft.canvas

import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

class PreviewWorkQueueTest {
    @Test
    fun concurrentCellsShareOneRenderer() = runBlocking {
        val queue = PreviewWorkQueue()
        val active = AtomicInteger()
        val peak = AtomicInteger()
        coroutineScope {
            repeat(30) {
                launch(Dispatchers.Default) {
                    queue.run {
                        val count = active.incrementAndGet()
                        peak.updateAndGet { maxOf(it, count) }
                        delay(2)
                        active.decrementAndGet()
                    }
                }
            }
        }
        assertEquals(1, peak.get())
        assertEquals(0, active.get())
    }

    @Test
    fun scrollingAwayCancelsQueuedCellBeforeNativeWork() = runBlocking {
        val queue = PreviewWorkQueue()
        val entered = CompletableDeferred<Unit>()
        val release = CompletableDeferred<Unit>()
        val first = launch { queue.run { entered.complete(Unit); release.await() } }
        entered.await()
        var rendered = false
        val waiting = launch { queue.run { rendered = true } }
        kotlinx.coroutines.yield() // Let the second cell actually queue behind the first.
        waiting.cancelAndJoin()
        release.complete(Unit)
        first.join()
        assertFalse(rendered)
        assertEquals("available", queue.run { "available" })
    }

    @Test
    fun failedPreviewDoesNotBlockFollowingBrushes() = runBlocking {
        val queue = PreviewWorkQueue()
        try { queue.run { throw IllegalStateException("invalid preview") } }
        catch (_: IllegalStateException) { }
        assertEquals("next brush", queue.run { "next brush" })
    }
}
