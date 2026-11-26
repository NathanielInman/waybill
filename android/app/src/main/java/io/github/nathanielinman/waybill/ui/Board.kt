package io.github.nathanielinman.waybill.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.SwipeToDismissBox
import androidx.compose.material3.SwipeToDismissBoxValue
import androidx.compose.material3.rememberSwipeToDismissBoxState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import io.github.nathanielinman.waybill.Card
import io.github.nathanielinman.waybill.Colors
import io.github.nathanielinman.waybill.LocalColors
import io.github.nathanielinman.waybill.Screen
import io.github.nathanielinman.waybill.Type
import kotlinx.coroutines.launch

fun laneColor(c: Colors, lane: String): Color = when (lane) {
    "ready" -> c.accent
    "doing" -> c.ok
    "yours" -> c.warn
    "done" -> c.ok.copy(alpha = 0.6f)
    else -> c.dim
}

/**
 * A card that acts when swiped: right finishes it, left starts it (or
 * stops it, once started). The words are the board's own `/` words.
 */
@Composable
fun SwipeCard(screen: Screen, card: Card, acts: Acts) {
    val c = LocalColors.current
    if (!card.open) {
        CardTile(screen.snapshot, card, acts)
        return
    }
    val left = if (card.lane == "doing") "stop" else "start"
    val state = rememberSwipeToDismissBoxState()
    // A swipe that lands says its words, and the card springs back to wait
    // for the desk to move it.
    LaunchedEffect(state.currentValue) {
        when (state.currentValue) {
            SwipeToDismissBoxValue.StartToEnd -> acts.say("done", card.uuid)
            SwipeToDismissBoxValue.EndToStart -> acts.say(left, card.uuid)
            SwipeToDismissBoxValue.Settled -> return@LaunchedEffect
        }
        state.reset()
    }
    SwipeToDismissBox(
        state,
        backgroundContent = {
            val toEnd = state.dismissDirection == SwipeToDismissBoxValue.StartToEnd
            Box(
                Modifier.fillMaxSize().clip(RoundedCornerShape(10.dp))
                    .background(if (toEnd) c.ok else c.accent)
                    .padding(horizontal = 16.dp),
                contentAlignment = if (toEnd) Alignment.CenterStart else Alignment.CenterEnd,
            ) {
                T(if (toEnd) "✓ Done" else if (left == "stop") "Stop" else "Start", Type.title, c.well, bold = true)
            }
        },
    ) {
        CardTile(screen.snapshot, card, acts)
    }
}

/** Board: every lane, one page each, with a strip of lanes to jump between. */
@Composable
fun BoardView(screen: Screen, acts: Acts, firstLane: Int = 0) {
    val c = LocalColors.current
    val columns = screen.columns
    val pager = rememberPagerState(initialPage = firstLane) { columns.size }
    val scope = rememberCoroutineScope()
    val total = columns.sumOf { it.cards.size }.coerceAtLeast(1)
    Column {
        LazyRow(
            contentPadding = PaddingValues(horizontal = 14.dp),
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            itemsIndexed(columns) { i, col ->
                val on = pager.currentPage == i
                val tint = laneColor(c, col.lane)
                Row(
                    Modifier.clip(RoundedCornerShape(14.dp))
                        .background(if (on) tint else c.tile)
                        .clickable { scope.launch { pager.animateScrollToPage(i) } }
                        .padding(horizontal = 11.dp, vertical = 6.dp),
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    T(col.title, Type.small, if (on) c.well else c.dim)
                    T(col.cards.size.toString(), Type.small, if (on) c.well else c.fg, bold = true)
                }
            }
        }
        // The share of the open work in each lane, as one bar.
        Row(Modifier.fillMaxWidth().padding(horizontal = 14.dp, vertical = 8.dp), horizontalArrangement = Arrangement.spacedBy(2.dp)) {
            columns.filter { it.cards.isNotEmpty() }.forEach { col ->
                Box(
                    Modifier.weight(col.cards.size.toFloat() / total).height(3.dp)
                        .clip(RoundedCornerShape(2.dp)).background(laneColor(c, col.lane)),
                )
            }
        }
        HorizontalPager(pager, Modifier.fillMaxSize(), beyondViewportPageCount = 1, verticalAlignment = Alignment.Top) { page ->
            val col = columns[page]
            val cards = screen.cards(col.cards)
            LazyColumn(
                contentPadding = PaddingValues(start = 14.dp, end = 14.dp, bottom = 14.dp),
                verticalArrangement = Arrangement.spacedBy(7.dp),
            ) {
                if (cards.isEmpty()) {
                    item { T(col.empty, Type.body, c.dim, Modifier.padding(top = 16.dp)) }
                }
                items(cards, key = { it.uuid }) { SwipeCard(screen, it, acts) }
                if (cards.isNotEmpty() && col.lane != "done") {
                    item {
                        T(
                            "Swipe a card right to finish it, left to ${if (col.lane == "doing") "stop" else "start"} it.",
                            Type.small, c.dim, Modifier.fillMaxWidth().padding(top = 6.dp),
                        )
                    }
                }
            }
        }
    }
}
