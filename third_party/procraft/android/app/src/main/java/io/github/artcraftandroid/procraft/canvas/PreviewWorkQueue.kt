package io.github.artcraftandroid.procraft.canvas

import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

/** One native preview at a time. Queued work belongs to the requesting cell's coroutine. */
internal class PreviewWorkQueue {
    private val mutex = Mutex()

    suspend fun <T> run(block: suspend () -> T): T = mutex.withLock { block() }
}
