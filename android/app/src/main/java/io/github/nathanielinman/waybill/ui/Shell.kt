package io.github.nathanielinman.waybill.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.nathanielinman.waybill.LocalColors
import io.github.nathanielinman.waybill.Page
import io.github.nathanielinman.waybill.Screen
import io.github.nathanielinman.waybill.Type

/** What the frame says besides the views: a problem, or the last thing that happened. */
data class Status(val problem: String? = null, val line: String? = null, val busy: Boolean = false)

@Composable
fun Header(screen: Screen, tab: String) {
    val c = LocalColors.current
    val l = screen.snapshot.pulse.lanes
    Column(Modifier.fillMaxWidth().padding(start = 16.dp, end = 16.dp, top = 10.dp, bottom = 6.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Dot(c.lamp(screen.lamp.state), 8.dp)
            T("WAYBILL", Type.small.copy(letterSpacing = 3.sp), c.bright, bold = true)
            T("${l.ready} ready · ${l.doing} doing", Type.small, c.dim, lines = 1)
            Spacer(Modifier.weight(1f))
            if (screen.read_only) Chip("demo", "plain")
        }
        screen.views.firstOrNull { it.id == tab }?.let {
            T(it.question, Type.small, c.dim, Modifier.padding(top = 3.dp), lines = 1)
        }
    }
}

@Composable
fun Tabs(screen: Screen, tab: String, onTab: (String) -> Unit) {
    val c = LocalColors.current
    Row(
        Modifier.fillMaxWidth().background(c.well).padding(horizontal = 8.dp, vertical = 8.dp),
        horizontalArrangement = Arrangement.SpaceEvenly,
    ) {
        screen.views.forEach { v ->
            val on = v.id == tab
            val badge = if (v.id == "desk") screen.snapshot.pulse.lanes.yours else 0
            Row(
                Modifier.clip(RoundedCornerShape(14.dp))
                    .background(if (on) c.accent else c.well)
                    .clickable { onTab(v.id) }
                    .padding(horizontal = 10.dp, vertical = 6.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                T(v.name, Type.small, if (on) c.well else c.fg, bold = on)
                if (badge > 0) T(badge.toString(), Type.small.copy(fontSize = 10.sp), if (on) c.well else c.warn, bold = true)
            }
        }
    }
}

/**
 * The `/` line: type to find a card, or with a card open, say what to do
 * ("high", "done shipped it", "wait until friday"). The desk reads the words.
 */
@Composable
fun SlashLine(screen: Screen, openCard: String?, acts: Acts, status: Status) {
    val c = LocalColors.current
    var text by remember { mutableStateOf("") }
    val words = text.trim().lowercase().split(Regex("\\s+")).filter { it.isNotEmpty() }
    val found = if (words.isEmpty() || openCard != null) emptyList() else
        screen.snapshot.cards.filter { card -> card.open && words.all { w -> card.title.lowercase().contains(w) || card.project.lowercase().contains(w) || card.uuid.startsWith(w) } }.take(4)
    Column(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 6.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        found.forEach { card ->
            Row(
                Modifier.fillMaxWidth().clip(RoundedCornerShape(8.dp)).background(c.tile)
                    .clickable { text = ""; acts.open(card.uuid) }.padding(horizontal = 10.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Dot(c.project(card.project), 6.dp)
                T(card.title, Type.body, c.bright, lines = 1)
            }
        }
        (status.problem ?: status.line)?.let {
            T(it, Type.small, if (status.problem != null) c.bad else c.ok, lines = 2)
        }
        Row(
            Modifier.fillMaxWidth().clip(RoundedCornerShape(20.dp)).background(c.tile)
                .border(1.dp, if (text.isEmpty()) c.border else c.accent, RoundedCornerShape(20.dp))
                .padding(horizontal = 14.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            T("/", Type.body, c.accent, Modifier.padding(end = 8.dp), bold = true)
            Box(Modifier.weight(1f)) {
                if (text.isEmpty()) {
                    T(if (openCard != null) "say what to do: done, high, park friday" else "find a card, or say what to do", Type.body, c.dim, lines = 1)
                }
                BasicTextField(
                    text, { text = it },
                    textStyle = Type.body.copy(color = c.bright),
                    cursorBrush = SolidColor(c.accent),
                    singleLine = true,
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Send),
                    keyboardActions = KeyboardActions(onSend = {
                        if (text.isNotBlank()) acts.say(text.trim(), openCard)
                        text = ""
                    }),
                    modifier = Modifier.fillMaxWidth(),
                )
            }
        }
    }
}

/** The whole app around one screen from the desk. */
@Composable
fun Shell(
    screen: Screen,
    tab: String,
    onTab: (String) -> Unit,
    openCard: String?,
    page: Page?,
    acts: Acts,
    onClose: () -> Unit,
    status: Status = Status(),
    since: Long = 0,
    firstLane: Int = 2,
) {
    val c = LocalColors.current
    Box(Modifier.fillMaxSize().background(c.panel)) {
        Column(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing)) {
            Header(screen, tab)
            screen.snapshot.error?.let {
                T("The task list could not be read: $it", Type.small, c.bad, Modifier.padding(horizontal = 16.dp))
            }
            Box(Modifier.weight(1f)) {
                when (tab) {
                    "board" -> BoardView(screen, acts, firstLane)
                    "lines" -> LinesView(screen, acts)
                    "log" -> LogView(screen, acts, since)
                    "pulse" -> PulseView(screen)
                    else -> DeskView(screen, acts)
                }
            }
            Column(Modifier.imePadding()) {
                SlashLine(screen, openCard, acts, status)
                Tabs(screen, tab, onTab)
            }
        }
        val card = openCard?.let { screen.card(it) }
        if (card != null) {
            Box(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing)) {
                CardSheet(screen, card, page, acts, onClose)
            }
        }
    }
}
