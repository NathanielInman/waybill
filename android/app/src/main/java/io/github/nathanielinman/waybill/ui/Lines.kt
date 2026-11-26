package io.github.nathanielinman.waybill.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.dp
import io.github.nathanielinman.waybill.Line
import io.github.nathanielinman.waybill.LocalColors
import io.github.nathanielinman.waybill.Screen
import io.github.nathanielinman.waybill.Type

private val ROW = 46.dp
private val TRACK = 26.dp

/**
 * One line, run top to bottom: each stop on its own row in the order the
 * desk laid them out (by column, then track), each branch on its own track.
 */
@Composable
fun LineRail(line: Line, acts: Acts) {
    val c = LocalColors.current
    val measure = rememberTextMeasurer()
    val tint = c.project(line.project)
    val height = ROW * line.stops.size
    Canvas(
        Modifier.fillMaxWidth().height(height).pointerInput(line) {
            detectTapGestures { at ->
                val i = (at.y / ROW.toPx()).toInt()
                line.stops.getOrNull(i)?.let { acts.open(it.uuid) }
            }
        },
    ) {
        val row = ROW.toPx()
        val track = TRACK.toPx()
        fun at(i: Int) = Offset(track * 0.7f + line.stops[i].track * track, row * i + row / 2)
        val labelX = track * 0.7f + line.tracks * track + 6.dp.toPx()
        // Track first, under the stops. Travelled track is the line's own color.
        line.links.forEach { (from, to) ->
            val a = at(from)
            val b = at(to)
            val done = line.stops[from].state == "done" && line.stops[to].state != "waiting"
            val path = Path().apply {
                moveTo(a.x, a.y)
                if (a.x == b.x) lineTo(b.x, b.y) else cubicTo(a.x, a.y + row * 0.6f, b.x, b.y - row * 0.6f, b.x, b.y)
            }
            drawPath(path, if (done) tint else tint.copy(alpha = 0.32f), style = Stroke(5.dp.toPx(), cap = StrokeCap.Round))
        }
        line.stops.forEachIndexed { i, stop ->
            val p = at(i)
            val r = 7.dp.toPx()
            val (fill, ring) = when (stop.state) {
                "done" -> tint to c.panel
                "doing" -> c.ok to c.panel
                "next" -> c.panel to c.bright
                else -> c.panel to c.dim
            }
            if (stop.state == "doing" || (stop.yours && stop.state == "next")) {
                drawCircle((if (stop.yours) c.warn else c.ok).copy(alpha = 0.2f), r * 1.8f, p)
            }
            if (stop.yours) {
                val sq = Size(r * 1.8f, r * 1.8f)
                val tl = Offset(p.x - sq.width / 2, p.y - sq.height / 2)
                val filled = stop.state == "next" || stop.state == "doing"
                drawRoundRect(if (filled) c.warn else c.panel, tl, sq, CornerRadius(3.dp.toPx()))
                drawRoundRect(c.warn, tl, sq, CornerRadius(3.dp.toPx()), style = Stroke(2.dp.toPx()))
            } else {
                drawCircle(fill, r, p)
                drawCircle(ring, r, p, style = Stroke(2.5.dp.toPx()))
            }
            if (stop.state == "done") {
                drawLine(c.panel, Offset(p.x - r * 0.45f, p.y), Offset(p.x - r * 0.05f, p.y + r * 0.4f), 2.dp.toPx())
                drawLine(c.panel, Offset(p.x - r * 0.05f, p.y + r * 0.4f), Offset(p.x + r * 0.5f, p.y - r * 0.4f), 2.dp.toPx())
            }
            val style = TextStyle(
                fontFamily = Type.body.fontFamily,
                fontSize = Type.body.fontSize,
                fontWeight = if (stop.state == "next" || stop.state == "doing") FontWeight.Bold else FontWeight.Normal,
                color = when (stop.state) {
                    "done" -> c.dim
                    "next", "doing" -> c.bright
                    else -> c.fg
                },
            )
            val text = measure.measure(
                stop.title, style, overflow = TextOverflow.Ellipsis, maxLines = 2,
                constraints = Constraints(maxWidth = (size.width - labelX).toInt().coerceAtLeast(1)),
            )
            drawText(text, topLeft = Offset(labelX, p.y - text.size.height / 2f))
        }
    }
}

/** Lines: every dependency chain as a rail line, read top to bottom. */
@Composable
fun LinesView(screen: Screen, acts: Acts) {
    val c = LocalColors.current
    val lines = screen.snapshot.lines
    LazyColumn(
        contentPadding = PaddingValues(horizontal = 14.dp, vertical = 10.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        if (lines.isEmpty()) {
            item { T("Tasks tied together by what they wait on show up here as lines.", Type.body, c.dim) }
        }
        items(lines, key = { it.name + it.project }) { line ->
            Tile {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    T(line.name, Type.title, c.bright, Modifier.weight(1f, fill = false), lines = 2, bold = true)
                    Spacer(Modifier.padding(start = 8.dp))
                    Project(line.project)
                }
                val need = line.stops.count { it.yours && it.state != "done" }
                T(
                    "${line.open} of ${line.stops.size} stops to go" + if (need > 0) " · $need ${if (need == 1) "needs" else "need"} you" else "",
                    Type.small, c.dim,
                )
                Bar(1f - line.open.toFloat() / line.stops.size.coerceAtLeast(1), c.project(line.project), Modifier.fillMaxWidth())
                LineRail(line, acts)
            }
        }
        item {
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
                Legend("finished", c.accent)
                Legend("started", c.ok)
                Legend("next up", c.bright)
                Legend("needs you", c.warn)
            }
        }
    }
}

@Composable
private fun Legend(text: String, color: androidx.compose.ui.graphics.Color) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        Dot(color, 6.dp)
        T(text, Type.small, LocalColors.current.dim)
    }
}
