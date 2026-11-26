package io.github.nathanielinman.waybill

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import io.github.nathanielinman.waybill.ui.Status
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** Everything the app shows, in one value. */
data class UiState(
    val pairing: Pairing? = null,
    val screen: Screen? = null,
    val tab: String = "desk",
    val openCard: String? = null,
    val page: Page? = null,
    val status: Status = Status(),
    /** When the Log was last looked at, in the desk's seconds. */
    val since: Long = 0,
)

/**
 * Follows the desk while the app is open and hands it what you say. It
 * keeps no rules of its own: the screen it holds is the one the desk sent.
 */
class Model(app: Application) : AndroidViewModel(app) {
    private val ctx = app.applicationContext
    private val _state = MutableStateFlow(UiState(pairing = Store.pairing(ctx), since = Store.lastLook(ctx)))
    val state: StateFlow<UiState> = _state

    private fun api(): Api? = _state.value.pairing?.let(::Api)

    /**
     * Keeps the screen current: one read, then a long wait on the desk for
     * anything newer, again and again. Runs while the app is on screen.
     */
    suspend fun follow() {
        while (true) {
            val api = api() ?: return
            val after = _state.value.screen?.version
            try {
                val screen = withContext(Dispatchers.IO) { api.screen(after) }
                if (screen.wire > WIRE) {
                    problem("The desk speaks a newer Waybill. Update this app.")
                } else if (screen.version != after) {
                    _state.update { it.copy(screen = screen, status = it.status.copy(problem = null)) }
                    refreshPage()
                } else {
                    _state.update { it.copy(status = it.status.copy(problem = null)) }
                }
            } catch (e: ApiError) {
                problem(e.message ?: "The desk said no.")
                if (e.status == 401) return
                delay(10_000)
            } catch (e: Exception) {
                problem(e.message ?: "Can't reach the desk.")
                delay(5_000)
            }
        }
    }

    private fun problem(text: String) = _state.update { it.copy(status = it.status.copy(problem = text)) }

    fun tab(id: String) {
        _state.update { it.copy(tab = id) }
        if (id == "log") markLooked()
    }

    /** "Since you looked" counts from the next time the Log is opened. */
    private fun markLooked() {
        val now = _state.value.screen?.snapshot?.now ?: return
        val before = Store.lastLook(ctx)
        Store.setLastLook(ctx, now)
        _state.update { it.copy(since = if (before == 0L) now else before) }
    }

    fun open(uuid: String?) {
        _state.update { it.copy(openCard = uuid, page = if (uuid == it.page?.uuid) it.page else null) }
        if (uuid != null) refreshPage()
    }

    private fun refreshPage() {
        val uuid = _state.value.openCard ?: return
        val api = api() ?: return
        viewModelScope.launch {
            val page = runCatching { withContext(Dispatchers.IO) { api.page(uuid) } }.getOrNull()
            if (page != null && _state.value.openCard == uuid) _state.update { it.copy(page = page) }
        }
    }

    /** Hands words to the desk, about [card] when one is open. */
    fun say(text: String, card: String?) {
        val api = api() ?: return
        _state.update { it.copy(status = it.status.copy(busy = true, line = null)) }
        viewModelScope.launch {
            try {
                val said = withContext(Dispatchers.IO) { api.say(text, card) }
                when {
                    said.view != null -> _state.update { it.copy(tab = said.view, openCard = null) }
                    said.new != null -> add(said.new)
                }
                _state.update {
                    it.copy(status = Status(line = said.result ?: said.said))
                }
                // A finished card leaves the sheet; anything else stays open.
                if (said.result?.startsWith("Finished") == true || said.result?.startsWith("Ruled") == true) open(null)
                Watch.now(ctx)
            } catch (e: Exception) {
                _state.update { it.copy(status = Status(problem = e.message ?: "That did not work.")) }
            }
        }
    }

    private suspend fun add(title: String) {
        val api = api() ?: return
        val said = withContext(Dispatchers.IO) { api.add(title, "", "", mine = false, note = "") }
        _state.update { it.copy(status = Status(line = said.result ?: said.said)) }
    }

    /** Pairs with the desk from a `waybill://pair` link, checking it answers first. */
    fun pair(link: String, done: (String?) -> Unit) {
        val pairing = Pairing.parse(link) ?: return done("That is not a waybill pair link.")
        viewModelScope.launch {
            val hello = runCatching { withContext(Dispatchers.IO) { Api(pairing).hello() } }
            hello.onSuccess {
                Store.pair(ctx, pairing)
                _state.update { s -> s.copy(pairing = pairing, screen = null, status = Status()) }
                Watch.schedule(ctx)
                Watch.now(ctx)
                done(null)
            }.onFailure { done(it.message ?: "The desk did not answer.") }
        }
    }

    fun forget() {
        Store.forget(ctx)
        _state.value = UiState()
    }
}
