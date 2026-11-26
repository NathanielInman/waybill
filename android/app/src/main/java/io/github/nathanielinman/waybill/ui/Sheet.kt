package io.github.nathanielinman.waybill.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.nathanielinman.waybill.Card
import io.github.nathanielinman.waybill.LocalColors
import io.github.nathanielinman.waybill.NoteBlock
import io.github.nathanielinman.waybill.Offer
import io.github.nathanielinman.waybill.Page
import io.github.nathanielinman.waybill.Screen
import io.github.nathanielinman.waybill.Type
import io.github.nathanielinman.waybill.dayLabel
import io.github.nathanielinman.waybill.localDay
import io.github.nathanielinman.waybill.span

/** A dimmed backdrop with a sheet rising from the bottom, drawn in the same window. */
@Composable
fun SheetFrame(onClose: () -> Unit, content: @Composable BoxScope.() -> Unit) {
    val c = LocalColors.current
    BackHandler(onBack = onClose)
    Box(Modifier.fillMaxSize()) {
        Box(
            Modifier.fillMaxSize().background(Color(0xa0080909))
                .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null, onClick = onClose),
        )
        Box(
            Modifier.align(Alignment.BottomCenter).fillMaxWidth().fillMaxHeight(0.86f)
                .clip(RoundedCornerShape(topStart = 20.dp, topEnd = 20.dp))
                .background(c.panel)
                .border(1.dp, c.border, RoundedCornerShape(topStart = 20.dp, topEnd = 20.dp)),
            content = content,
        )
    }
}

@Composable
fun Button(text: String, style: String, on: Boolean = false, onClick: () -> Unit) {
    val c = LocalColors.current
    val (fg, bg, line) = when {
        on -> Triple(c.well, c.accent, c.accent)
        style == "go" -> Triple(c.ok, c.ok.copy(alpha = 0.08f), c.ok.copy(alpha = 0.6f))
        style == "choice" -> Triple(c.bright, c.violet.copy(alpha = 0.08f), c.violet)
        else -> Triple(c.bright, c.tile, c.border)
    }
    Box(
        Modifier.clip(RoundedCornerShape(9.dp)).border(1.dp, line, RoundedCornerShape(9.dp)).background(bg)
            .clickable(onClick = onClick).padding(horizontal = 13.dp, vertical = 9.dp),
    ) { T(text, Type.body, fg) }
}

/** A one-line prompt for a button that asks first ("A closing line, or just Enter"). */
@Composable
fun Prompt(hint: String, onSubmit: (String) -> Unit, onCancel: () -> Unit) {
    val c = LocalColors.current
    var text by remember { mutableStateOf("") }
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(10.dp)).background(c.well)
            .border(1.dp, c.accent, RoundedCornerShape(10.dp)).padding(horizontal = 12.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.weight(1f)) {
            if (text.isEmpty()) T(hint, Type.body, c.dim)
            BasicTextField(
                text, { text = it },
                textStyle = Type.body.copy(color = c.bright),
                cursorBrush = SolidColor(c.accent),
                singleLine = true,
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Send),
                keyboardActions = KeyboardActions(onSend = { onSubmit(text) }),
                modifier = Modifier.fillMaxWidth(),
            )
        }
        T("Send", Type.body, c.accent, Modifier.clickable { onSubmit(text) }.padding(start = 10.dp))
        T("✕", Type.body, c.dim, Modifier.clickable(onClick = onCancel).padding(start = 12.dp))
    }
}

@Composable
fun NoteView(blocks: List<NoteBlock>) {
    val c = LocalColors.current
    Column(verticalArrangement = Arrangement.spacedBy(5.dp)) {
        blocks.forEach { b ->
            val text = buildAnnotatedString {
                b.spans.forEach { sp ->
                    withStyle(
                        SpanStyle(
                            fontWeight = if (sp.bold) FontWeight.Bold else null,
                            color = if (sp.code) c.accent else Color.Unspecified,
                        ),
                    ) { append(sp.text) }
                }
            }
            when (b.kind) {
                "heading" -> if (b.level > 1) Text(text, style = Type.body.copy(fontWeight = FontWeight.Bold), color = c.bright)
                "code" -> Box(Modifier.fillMaxWidth().clip(RoundedCornerShape(8.dp)).background(c.well).padding(10.dp)) {
                    Text(text, style = Type.small, color = c.fg)
                }
                "bullet" -> Row(Modifier.padding(start = (b.level * 14).dp)) {
                    when (b.check) {
                        true -> T("☑", Type.body, c.ok, Modifier.width(20.dp))
                        false -> T("☐", Type.body, c.dim, Modifier.width(20.dp))
                        null -> T("•", Type.body, c.dim, Modifier.width(16.dp))
                    }
                    Text(text, style = Type.body, color = c.fg)
                }
                else -> Text(text, style = Type.body, color = c.fg)
            }
        }
    }
}

/** One card in full: its buttons (from the desk), what it frees and waits on, its note, its visits. */
@Composable
fun CardSheet(screen: Screen, card: Card, page: Page?, acts: Acts, onClose: () -> Unit) {
    val c = LocalColors.current
    val s = screen.snapshot
    var asking by remember(card.uuid) { mutableStateOf<Offer?>(null) }
    SheetFrame(onClose) {
        Column(
            Modifier.fillMaxSize().verticalScroll(rememberScrollState()).imePadding()
                .padding(horizontal = 16.dp, vertical = 10.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Box(Modifier.align(Alignment.CenterHorizontally).width(38.dp).height(4.dp).clip(RoundedCornerShape(2.dp)).background(c.border))
            FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Project(card.project)
                Priority(card.priority)
                if (card.alarm) Chip("alarm", "bad")
                if (card.decision) Chip("decision", "violet")
                card.tags.forEach { Chip(it, tagKind(it)) }
            }
            T(card.title, Type.headline.copy(fontSize = 18.sp, lineHeight = 24.sp), c.bright)

            val offers = page?.offers.orEmpty()
            listOf("act", "priority").forEach { group ->
                val row = offers.filter { it.group == group }
                if (row.isNotEmpty()) {
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(7.dp), verticalArrangement = Arrangement.spacedBy(7.dp)) {
                        row.forEach { o ->
                            Button(o.label, o.style, o.on) {
                                if (o.ask != null) asking = o else acts.say(o.words, card.uuid)
                            }
                        }
                    }
                }
            }
            val decide = offers.filter { it.group == "decide" }
            if (decide.isNotEmpty()) {
                Section("Decide")
                decide.forEach { o ->
                    Button(o.label, o.style) { if (o.ask != null) asking = o else acts.say(o.words, card.uuid) }
                }
            }
            asking?.let { o ->
                Prompt(o.ask.orEmpty(), onSubmit = { line ->
                    acts.say(o.with(line), card.uuid)
                    asking = null
                }, onCancel = { asking = null })
            }

            val (state, tint) = when (card.lane) {
                "ready" -> "ready for an agent" to c.accent
                "doing" -> "started" to c.ok
                "yours" -> (if (card.blocked) "yours, once the line reaches it" else "needs you") to c.warn
                "held" -> "held by another task" to c.dim
                "parked" -> "parked until later" to c.dim
                else -> "finished" to c.ok
            }
            Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
                Kv("state") { Dot(tint, 6.dp); T(state, Type.small, c.fg) }
                Kv("added") { T("${span(s.now - card.entry)} ago", Type.small, c.fg) }
                Kv("uuid") { T(card.short, Type.small, c.fg) }
            }
            Links("Frees when it closes", screen.cards(card.frees), acts)
            Links("Waits on", screen.cards(card.waits_on), acts)
            if (page != null && page.note.isNotEmpty()) {
                Section("Note")
                NoteView(page.note)
            }
            Section("Visits (${card.visits.size})")
            if (card.visits.isEmpty()) T("Nobody has logged a visit yet.", Type.small, c.dim)
            card.visits.sortedByDescending { it.at }.forEach { v ->
                Row(Modifier.fillMaxWidth()) {
                    Box(Modifier.width(2.dp).height(34.dp).background(c.accent))
                    Column(Modifier.padding(start = 10.dp)) {
                        Meta {
                            T(dayLabel(localDay(s, v.at)), Type.small, c.dim)
                            v.shas.firstOrNull()?.let { Chip(it, "accent") }
                        }
                        T(v.text, Type.body, c.fg)
                    }
                }
            }
        }
    }
}

@Composable
private fun Kv(key: String, value: @Composable () -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        T(key, Type.small, LocalColors.current.dim, Modifier.width(56.dp))
        value()
    }
}

@Composable
private fun Links(title: String, cards: List<Card>, acts: Acts) {
    if (cards.isEmpty()) return
    val c = LocalColors.current
    Section(title)
    cards.forEach { other ->
        Row(
            Modifier.fillMaxWidth().clip(RoundedCornerShape(7.dp)).background(c.tile)
                .clickable { acts.open(other.uuid) }.padding(horizontal = 10.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Dot(c.project(other.project), 6.dp)
            T(other.title, Type.body, c.bright, lines = 1)
        }
    }
}
