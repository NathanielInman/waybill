package io.github.nathanielinman.waybill.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.nathanielinman.waybill.Card
import io.github.nathanielinman.waybill.LocalColors
import io.github.nathanielinman.waybill.Snapshot
import io.github.nathanielinman.waybill.Type
import io.github.nathanielinman.waybill.ago
import io.github.nathanielinman.waybill.holdLine
import io.github.nathanielinman.waybill.whenText

/** What a tap or a press does: every view and the card sheet send these. */
interface Acts {
    /** Open a card's sheet. */
    fun open(uuid: String)

    /** Hand words to the desk's `/` line, about [card] when one is open. */
    fun say(text: String, card: String? = null)

    /** Go to a view by its id. */
    fun view(id: String)

    companion object {
        val none = object : Acts {
            override fun open(uuid: String) {}
            override fun say(text: String, card: String?) {}
            override fun view(id: String) {}
        }
    }
}

@Composable
fun T(
    text: String,
    style: TextStyle = Type.body,
    color: Color = LocalColors.current.fg,
    modifier: Modifier = Modifier,
    lines: Int = Int.MAX_VALUE,
    bold: Boolean = false,
) {
    Text(
        text,
        style = if (bold) style.copy(fontWeight = FontWeight.Bold) else style,
        color = color,
        modifier = modifier,
        maxLines = lines,
        overflow = TextOverflow.Ellipsis,
    )
}

@Composable
fun Dot(color: Color, size: Dp = 7.dp) {
    Box(Modifier.size(size).clip(CircleShape).background(color))
}

/** A small rounded label: a priority, a tag, "frees 3". */
@Composable
fun Chip(text: String, fg: Color, bg: Color) {
    Box(
        Modifier.clip(RoundedCornerShape(6.dp)).background(bg).padding(horizontal = 6.dp, vertical = 1.dp),
    ) {
        Text(text, style = Type.small.copy(fontSize = 10.sp, lineHeight = 14.sp), color = fg, maxLines = 1)
    }
}

/** A chip in one of the palette's meanings: bad, warn, accent, violet, ok or plain. */
@Composable
fun Chip(text: String, kind: String) {
    val c = LocalColors.current
    val fg = when (kind) {
        "bad" -> c.bad
        "warn" -> c.warn
        "accent" -> c.accent
        "violet" -> c.violet
        "ok" -> c.ok
        else -> c.dim
    }
    Chip(text, if (kind == "plain") c.fg else fg, if (kind == "plain") c.border else fg.copy(alpha = 0.2f))
}

@Composable
fun Priority(p: String) {
    when (p) {
        "H" -> Chip("H", "bad")
        "M", "L" -> Chip(p, "plain")
    }
}

fun tagKind(tag: String) = when (tag) {
    "bug" -> "bad"
    "research" -> "accent"
    "verify" -> "warn"
    else -> "plain"
}

@Composable
fun Project(name: String) {
    val c = LocalColors.current
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(5.dp)) {
        Dot(c.project(name), 6.dp)
        T(name.ifEmpty { "no project" }, Type.small, c.dim, lines = 1)
    }
}

/** A section's label, small and spaced like the board's. */
@Composable
fun Section(text: String, modifier: Modifier = Modifier) {
    T(text, Type.tiny, LocalColors.current.dim, modifier.padding(top = 10.dp, bottom = 2.dp))
}

/** The raised surface a card or a row sits on. */
@Composable
fun Tile(
    modifier: Modifier = Modifier,
    border: Color? = null,
    onClick: (() -> Unit)? = null,
    content: @Composable ColumnScope.() -> Unit,
) {
    val c = LocalColors.current
    var m = modifier.fillMaxWidth().clip(RoundedCornerShape(10.dp)).background(c.tile)
    if (border != null) m = m.border(1.dp, border, RoundedCornerShape(10.dp))
    if (onClick != null) m = m.clickable(onClick = onClick)
    Column(m.padding(horizontal = 12.dp, vertical = 10.dp), verticalArrangement = Arrangement.spacedBy(5.dp), content = content)
}

/** Visits per day over two weeks, as the board's small bars. */
@Composable
fun Activity(days: List<Int>, modifier: Modifier = Modifier) {
    val c = LocalColors.current
    val most = (days.maxOrNull() ?: 0).coerceAtLeast(1)
    Canvas(modifier.width(48.dp).height(12.dp)) {
        val n = days.size.coerceAtLeast(1)
        val w = size.width / n
        drawLine(
            c.dim, Offset(0f, size.height), Offset(size.width, size.height), 1f,
            pathEffect = PathEffect.dashPathEffect(floatArrayOf(2f, 3f)),
        )
        days.forEachIndexed { i, v ->
            if (v > 0) {
                val h = size.height * v / most
                drawRect(c.accent, Offset(i * w + w * 0.2f, size.height - h), androidx.compose.ui.geometry.Size(w * 0.6f, h))
            }
        }
    }
}

@Composable
fun Meta(content: @Composable RowScope.() -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp), content = content)
}

/** One card as the board draws it: title, project and chips, who holds it, its visits. */
@Composable
fun CardTile(s: Snapshot, card: Card, acts: Acts, modifier: Modifier = Modifier) {
    val c = LocalColors.current
    val doing = card.lane == "doing"
    val border = when {
        card.alarm -> c.bad
        doing -> c.ok.copy(alpha = 0.45f)
        card.yours -> c.warn.copy(alpha = 0.45f)
        else -> null
    }
    Tile(modifier, border = border, onClick = { acts.open(card.uuid) }) {
        T(card.title, Type.title, c.bright, lines = 3)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Project(card.project)
            Priority(card.priority)
            if (card.alarm) Chip("alarm", "bad")
            if (card.decision) Chip("decision", "violet")
            card.tags.take(2).forEach { Chip(it, tagKind(it)) }
            when (card.lane) {
                "doing" -> if (card.yours) Chip("you", "warn")
                "done", "past" -> card.ended?.let { T(whenText(s, it), Type.small, c.dim) }
                else -> when {
                    card.frees.isNotEmpty() ->
                        if (card.yours) Chip("frees ${card.frees.size}", "warn") else Chip("opens ${card.frees.size}", "accent")
                    card.blocked -> Chip("waits on ${card.waits_on.size}", "plain")
                }
            }
        }
        if (doing) {
            val (line, stale) = holdLine(s, card)
            Meta {
                Dot(if (stale) c.bad else c.ok, 6.dp)
                T(line, Type.small, if (stale) c.bad else c.ok, lines = 1)
            }
        }
        if (card.visits.isNotEmpty() && card.lane != "done" && card.lane != "past") {
            val n = card.visits.size
            val last = card.visits.maxOf { it.at }
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                T("$n ${if (n == 1) "visit" else "visits"} · last ${ago(s.now - last)}", Type.small, c.dim)
                Spacer(Modifier.weight(1f))
                if (card.activity.any { it > 0 }) Activity(card.activity)
            }
        }
    }
}

/** A thin bar, filled to [share]. */
@Composable
fun Bar(share: Float, color: Color, modifier: Modifier = Modifier) {
    val c = LocalColors.current
    Box(modifier.height(4.dp).clip(RoundedCornerShape(2.dp)).background(c.border)) {
        Box(Modifier.fillMaxWidth(share.coerceIn(0f, 1f)).height(4.dp).background(color))
    }
}
