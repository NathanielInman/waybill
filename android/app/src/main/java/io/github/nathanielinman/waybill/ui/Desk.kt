package io.github.nathanielinman.waybill.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.material3.Text
import io.github.nathanielinman.waybill.Card
import io.github.nathanielinman.waybill.Headline
import io.github.nathanielinman.waybill.LocalColors
import io.github.nathanielinman.waybill.Screen
import io.github.nathanielinman.waybill.Type
import io.github.nathanielinman.waybill.span

/** "12 tasks wait on you. 4 of them hold up 8 others." with the second half in amber. */
@Composable
fun HeadlineText(h: Headline, modifier: Modifier = Modifier) {
    val c = LocalColors.current
    Text(
        buildAnnotatedString {
            h.broke?.let {
                withStyle(SpanStyle(color = c.bad, fontWeight = FontWeight.Bold)) { append(it) }
                append(" ")
            }
            withStyle(SpanStyle(color = c.bright)) { append(h.count) }
            h.holding?.let {
                append(" ")
                withStyle(SpanStyle(color = c.warn)) { append(it) }
            }
        },
        style = Type.headline,
        modifier = modifier,
    )
}

/** A decision's options, one tap each. */
@Composable
fun Options(uuid: String, options: List<String>, acts: Acts, limit: Int = 4) {
    val c = LocalColors.current
    FlowRow(
        Modifier.fillMaxWidth().padding(start = 4.dp),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        options.take(limit).forEach { option ->
            Box(
                Modifier
                    .clip(RoundedCornerShape(8.dp))
                    .border(1.dp, c.violet, RoundedCornerShape(8.dp))
                    .background(c.violet.copy(alpha = 0.08f))
                    .clickable { acts.say("rule $option", uuid) }
                    .padding(horizontal = 10.dp, vertical = 7.dp),
            ) {
                T(option, Type.small.copy(fontSize = 12.sp), c.bright, lines = 2)
            }
        }
    }
}

/** One of your tasks: how many it frees, its title, and what comes after it. */
@Composable
fun DeskRow(screen: Screen, card: Card, most: Int, acts: Acts) {
    val c = LocalColors.current
    val s = screen.snapshot
    Tile(onClick = { acts.open(card.uuid) }, border = if (card.alarm) c.bad else null) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.width(44.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                when {
                    card.frees.isNotEmpty() -> {
                        T(card.frees.size.toString(), Type.big, c.warn)
                        T("frees", Type.small, c.warn)
                    }
                    card.decision -> T("◆", Type.big.copy(fontSize = 18.sp), c.violet)
                    else -> T("■", Type.big.copy(fontSize = 14.sp), c.warn)
                }
            }
            Column(Modifier.weight(1f).padding(start = 8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                T(card.title, Type.title, c.bright, lines = 2)
                FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Project(card.project)
                    Priority(card.priority)
                    if (card.decision) Chip("decision", "violet")
                    if (card.blocked) Chip("waits on ${card.waits_on.size}", "plain")
                    T("open ${span(s.now - card.entry)}", Type.small, c.dim)
                }
                if (card.frees.isNotEmpty()) {
                    val then = screen.cards(card.frees).joinToString(" · ") { it.title }
                    T("then: $then", Type.small, c.dim, lines = 2)
                    Bar(card.frees.size.toFloat() / most, c.warn, Modifier.fillMaxWidth().padding(top = 2.dp))
                }
            }
        }
    }
}

/** Desk: what waits on you, and what each of those frees. */
@Composable
fun DeskView(screen: Screen, acts: Acts) {
    val c = LocalColors.current
    val d = screen.desk
    LazyColumn(
        contentPadding = PaddingValues(horizontal = 14.dp, vertical = 10.dp),
        verticalArrangement = Arrangement.spacedBy(7.dp),
    ) {
        item { HeadlineText(screen.headline, Modifier.padding(bottom = 4.dp)) }
        val alarms = screen.cards(screen.drawer.alarms)
        if (alarms.isNotEmpty()) {
            item { Section("Something broke") }
            items(alarms, key = { "alarm" + it.uuid }) { DeskRow(screen, it, d.most, acts) }
        }
        val frees = screen.cards(d.frees)
        if (frees.isNotEmpty()) {
            item { Section("Frees the most") }
            items(frees, key = { it.uuid }) { DeskRow(screen, it, d.most, acts) }
        }
        val decisions = screen.cards(d.decisions)
        if (decisions.isNotEmpty()) {
            item { Section("Decisions") }
            decisions.forEach { card ->
                item(key = card.uuid) {
                    Column(verticalArrangement = Arrangement.spacedBy(7.dp)) {
                        DeskRow(screen, card, d.most, acts)
                        Options(card.uuid, screen.options[card.uuid].orEmpty(), acts)
                    }
                }
            }
        }
        val hands = screen.cards(d.hands)
        if (hands.isNotEmpty()) {
            item { Section("Your hands, by project") }
            items(hands, key = { it.uuid }) { DeskRow(screen, it, d.most, acts) }
        }
        if (frees.isEmpty() && decisions.isEmpty() && hands.isEmpty()) {
            item { T("Tasks tagged for your hands, and decisions, show up here.", Type.body, c.dim) }
        }
    }
}
