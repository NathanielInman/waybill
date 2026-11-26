package io.github.nathanielinman.waybill.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.nathanielinman.waybill.LocalColors
import io.github.nathanielinman.waybill.Pulse
import io.github.nathanielinman.waybill.Screen
import io.github.nathanielinman.waybill.Type
import kotlin.math.roundToInt

@Composable
private fun Stat(label: String, value: String, spark: List<Int>?, modifier: Modifier = Modifier) {
    val c = LocalColors.current
    Tile(modifier) {
        T(label, Type.small, c.dim, lines = 1)
        Row(verticalAlignment = Alignment.Bottom) {
            T(value, Type.big, c.bright)
            Spacer(Modifier.weight(1f))
            if (spark != null) Spark(spark, c.ok, Modifier.width(56.dp).height(20.dp))
        }
    }
}

@Composable
fun Spark(values: List<Int>, color: Color, modifier: Modifier = Modifier) {
    val most = (values.maxOrNull() ?: 0).coerceAtLeast(1)
    Canvas(modifier) {
        if (values.size < 2) return@Canvas
        val step = size.width / (values.size - 1)
        val path = Path()
        values.forEachIndexed { i, v ->
            val p = Offset(i * step, size.height - size.height * v / most)
            if (i == 0) path.moveTo(p.x, p.y) else path.lineTo(p.x, p.y)
        }
        drawPath(path, color, style = Stroke(1.6.dp.toPx(), cap = StrokeCap.Round))
        drawCircle(color, 2.dp.toPx(), Offset(size.width, size.height - size.height * values.last() / most))
    }
}

/** Six weeks of finished (bars) against added (a line). */
@Composable
private fun Flow(p: Pulse) {
    val c = LocalColors.current
    val most = (p.days.maxOfOrNull { maxOf(it.done, it.added) } ?: 0).coerceAtLeast(1)
    Canvas(Modifier.fillMaxWidth().height(110.dp)) {
        val n = p.days.size.coerceAtLeast(1)
        val w = size.width / n
        val base = size.height - 2.dp.toPx()
        for (g in 1..3) {
            val y = base - base * g / 4
            drawLine(c.border.copy(alpha = 0.6f), Offset(0f, y), Offset(size.width, y), 1f)
        }
        p.days.forEachIndexed { i, d ->
            val h = base * d.done / most
            drawRoundRect(
                if (i == n - 1) c.ok else c.ok.copy(alpha = 0.55f),
                Offset(i * w + w * 0.15f, base - h), Size(w * 0.7f, h), CornerRadius(1.5f),
            )
        }
        val line = Path()
        p.days.forEachIndexed { i, d ->
            val pt = Offset(i * w + w / 2, base - base * d.added / most)
            if (i == 0) line.moveTo(pt.x, pt.y) else line.lineTo(pt.x, pt.y)
        }
        drawPath(line, c.accent, style = Stroke(1.5.dp.toPx()))
        drawLine(c.border, Offset(0f, base), Offset(size.width, base), 1f)
    }
}

/** Who the open work waits on: you, agents (ready, started), another task, a date. */
@Composable
private fun Ring(screen: Screen) {
    val c = LocalColors.current
    val l = screen.snapshot.pulse.lanes
    val parts = listOf(
        Triple("you", l.yours, c.warn),
        Triple("agents, ready now", l.ready, c.accent),
        Triple("agents, started", l.doing, c.ok),
        Triple("another task", l.held, c.dim),
        Triple("a later date", l.parked, c.dim.copy(alpha = 0.45f)),
    )
    val total = parts.sumOf { it.second }.coerceAtLeast(1)
    Row(verticalAlignment = Alignment.CenterVertically) {
        Box(Modifier.size(112.dp), contentAlignment = Alignment.Center) {
            Canvas(Modifier.size(112.dp)) {
                val stroke = 14.dp.toPx()
                var start = -90f
                parts.forEach { (_, n, color) ->
                    val sweep = 360f * n / total
                    if (n > 0) {
                        drawArc(
                            color, start + 1f, (sweep - 2f).coerceAtLeast(0.5f), false,
                            topLeft = Offset(stroke / 2, stroke / 2),
                            size = Size(size.width - stroke, size.height - stroke),
                            style = Stroke(stroke),
                        )
                    }
                    start += sweep
                }
            }
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                T(total.toString(), Type.big, c.bright)
                T("open", Type.small, c.dim)
            }
        }
        Column(Modifier.padding(start = 14.dp), verticalArrangement = Arrangement.spacedBy(3.dp)) {
            parts.forEach { (name, n, color) ->
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Dot(color, 6.dp)
                    T(name, Type.small, c.fg, Modifier.padding(start = 6.dp).weight(1f))
                    T(n.toString(), Type.small, c.bright)
                }
            }
        }
    }
}

/** Visits by weekday and hour, four weeks. */
@Composable
private fun Punch(p: Pulse) {
    val c = LocalColors.current
    val most = (p.punch.flatten().maxOrNull() ?: 0).coerceAtLeast(1)
    val days = listOf("M", "T", "W", "T", "F", "S", "S")
    Row {
        Column(Modifier.width(14.dp)) {
            days.forEach { T(it, Type.small.copy(fontSize = 9.sp, lineHeight = 13.sp), c.dim, Modifier.height(13.dp)) }
        }
        Canvas(Modifier.weight(1f).height(13.dp * 7)) {
            val cw = size.width / 24
            val ch = size.height / 7
            p.punch.forEachIndexed { r, row ->
                row.forEachIndexed { h, v ->
                    val a = v.toFloat() / most
                    drawRoundRect(
                        if (v == 0) c.tileHover else c.accent.copy(alpha = 0.15f + a * 0.85f),
                        Offset(h * cw + 1f, r * ch + 1f), Size(cw - 2f, ch - 2f), CornerRadius(2f),
                    )
                }
            }
        }
    }
}

/** Pulse: how the work is flowing, stacked for a phone. */
@Composable
fun PulseView(screen: Screen) {
    val c = LocalColors.current
    val p = screen.snapshot.pulse
    val l = p.lanes
    val n = p.days.size
    val last14 = p.days.drop((n - 14).coerceAtLeast(0))
    LazyColumn(
        contentPadding = PaddingValues(horizontal = 14.dp, vertical = 10.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        item {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Stat("finished today", l.done.toString(), last14.map { it.done }, Modifier.weight(1f))
                    Stat("this week", p.done_week.toString(), null, Modifier.weight(1f))
                }
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Stat("visits this week", p.visits_week.toString(), last14.map { it.visits }, Modifier.weight(1f))
                    Stat("open", (l.ready + l.doing + l.yours + l.held + l.parked).toString(), last14.map { it.open_yours + it.open_agents }, Modifier.weight(1f))
                }
            }
        }
        item {
            Tile {
                T("Six weeks: finished and added", Type.small, c.dim)
                Flow(p)
                Row(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
                    Row(verticalAlignment = Alignment.CenterVertically) { Dot(c.ok, 6.dp); T(" finished ${p.done_week} this week", Type.small, c.fg) }
                }
                Row(verticalAlignment = Alignment.CenterVertically) { Dot(c.accent, 6.dp); T(" added ${p.added_week} this week", Type.small, c.fg) }
            }
        }
        item {
            Tile {
                T("Who the open work waits on", Type.small, c.dim)
                Ring(screen)
            }
        }
        item {
            Tile {
                T("When the work happens", Type.small, c.dim)
                Punch(p)
            }
        }
        item {
            Tile {
                T("Projects", Type.small, c.dim)
                p.projects.forEach { pr ->
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Box(Modifier.weight(1f)) { Project(pr.name) }
                        T("${pr.open} open", Type.small, c.fg, Modifier.width(64.dp))
                        T("${pr.done_week}/wk", Type.small, c.dim, Modifier.width(52.dp))
                        Spark(pr.spark, c.project(pr.name), Modifier.width(52.dp).height(16.dp))
                    }
                }
            }
        }
        item {
            Tile {
                T("How long a task lives", Type.small, c.dim)
                val most = (p.cycle.maxOfOrNull { it.count } ?: 0).coerceAtLeast(1)
                p.cycle.forEach { b ->
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        T(b.label, Type.small, c.fg, Modifier.width(88.dp))
                        Bar(b.count.toFloat() / most, c.teal, Modifier.weight(1f))
                        T(b.count.toString(), Type.small, c.dim, Modifier.width(40.dp).padding(start = 8.dp))
                    }
                }
            }
        }
        item {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Stat("visits that name a commit", "${(p.commit_share * 100).roundToInt()}%", null, Modifier.weight(1f))
                Stat("a decision stays open", p.decision_hours?.let { "${it.roundToInt()} h" } ?: "none", null, Modifier.weight(1f))
            }
        }
    }
}
