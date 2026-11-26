package io.github.nathanielinman.waybill.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.unit.dp
import io.github.nathanielinman.waybill.Event
import io.github.nathanielinman.waybill.LocalColors
import io.github.nathanielinman.waybill.Screen
import io.github.nathanielinman.waybill.Snapshot
import io.github.nathanielinman.waybill.Type
import io.github.nathanielinman.waybill.dayLabel
import io.github.nathanielinman.waybill.hm
import io.github.nathanielinman.waybill.localDay

/** Visits per hour over the last seven days, oldest first. */
fun weekByHour(s: Snapshot): List<Int> {
    val hours = IntArray(7 * 24)
    val end = (s.now / 3_600) + 1
    s.log.forEach { e ->
        val back = end - 1 - e.at / 3_600
        if (back in 0 until hours.size) hours[hours.size - 1 - back.toInt()]++
    }
    return hours.toList()
}

@Composable
fun WeekStrip(hours: List<Int>, since: Long, s: Snapshot) {
    val c = LocalColors.current
    val most = (hours.maxOrNull() ?: 0).coerceAtLeast(1)
    val fresh = ((s.now - since) / 3_600).toInt().coerceIn(0, hours.size)
    Canvas(Modifier.fillMaxWidth().height(56.dp)) {
        val w = size.width / hours.size
        val base = size.height - 2.dp.toPx()
        for (d in 1 until 7) {
            val x = d * 24 * w
            drawLine(c.border, Offset(x, 0f), Offset(x, base), 1f)
        }
        hours.forEachIndexed { i, v ->
            if (v > 0) {
                val h = base * v / most
                val new = i >= hours.size - fresh
                drawRect(if (new) c.warn else c.accent.copy(alpha = 0.75f), Offset(i * w, base - h), Size((w - 0.6f).coerceAtLeast(0.8f), h))
            }
        }
        drawLine(c.border, Offset(0f, base + 0.5f), Offset(size.width, base + 0.5f), 1f)
    }
}

@Composable
private fun EventRow(s: Snapshot, e: Event, acts: Acts) {
    val c = LocalColors.current
    val (word, tint) = when (e.kind) {
        "ruling" -> "ruling" to c.violet
        "done" -> "done" to c.ok
        "added" -> "added" to c.dim
        else -> "visit" to c.accent
    }
    Tile(border = if (e.kind == "ruling") c.violet.copy(alpha = 0.5f) else null, onClick = { acts.open(e.uuid) }) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            T("$word · ${hm(s, e.at)}", Type.small, tint)
            Spacer(Modifier.weight(1f))
            e.shas.firstOrNull()?.let { Chip(it, "accent") }
        }
        T(e.title, Type.title, c.bright, lines = 2)
        if (e.text.isNotBlank()) T(e.text, Type.small, if (e.kind == "ruling") c.bright else c.fg, lines = 3)
        Project(e.project)
    }
}

/** Log: every visit, ruling and finished task, newest first, under a week by the hour. */
@Composable
fun LogView(screen: Screen, acts: Acts, since: Long) {
    val c = LocalColors.current
    val s = screen.snapshot
    val hours = remember(s.now, s.log.size) { weekByHour(s) }
    val fresh = s.log.count { it.at > since }
    val days = remember(s.now, s.log.size) {
        s.log.take(400).groupBy { localDay(s, it.at) }.toList()
    }
    LazyColumn(
        contentPadding = PaddingValues(horizontal = 14.dp, vertical = 10.dp),
        verticalArrangement = Arrangement.spacedBy(7.dp),
    ) {
        item {
            Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Row {
                    T("The last seven days, by the hour", Type.small, c.dim)
                    Spacer(Modifier.weight(1f))
                    if (since > 0) T("since you looked: $fresh", Type.small, c.warn)
                }
                WeekStrip(hours, if (since > 0) since else s.now, s)
                T("${s.pulse.visits_week} visits and ${s.pulse.done_week} finished this week", Type.small, c.dim)
            }
        }
        days.forEach { (day, events) ->
            item(key = "day$day") {
                val today = localDay(s, s.now)
                Section(
                    when (today - day) {
                        0L -> "Today"
                        1L -> "Yesterday"
                        else -> dayLabel(day)
                    },
                    Modifier.padding(top = 4.dp),
                )
            }
            itemsIndexed(events, key = { i, _ -> "$day-$i" }) { _, e -> EventRow(s, e, acts) }
        }
        if (s.log.isEmpty()) item { T("Nothing has happened in the last seven days.", Type.body, c.dim) }
    }
}
