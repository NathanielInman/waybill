package io.github.nathanielinman.waybill

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

/*
 * What `waybill serve` sends, as `waybill_core::wire` builds it. These
 * classes only read it: every rule (which lane a card is in, the order of
 * the Desk, what the drawer has room for, the lamp) was decided on the
 * desk. A field the desk adds and the phone does not know yet is skipped.
 */

val wireJson = Json {
    ignoreUnknownKeys = true
    explicitNulls = false
}

/** The wire version this app reads. The desk raises it for a breaking change. */
const val WIRE = 1

@Serializable
data class Visit(val at: Long, val text: String, val shas: List<String> = emptyList())

@Serializable
data class Hold(val state: String, val who: String = "", val session: String = "")

@Serializable
data class Card(
    val uuid: String,
    val title: String,
    val project: String = "",
    /** "H", "M", "L" or " ". */
    val priority: String = " ",
    val tags: List<String> = emptyList(),
    val yours: Boolean = false,
    val decision: Boolean = false,
    val alarm: Boolean = false,
    val blocked: Boolean = false,
    /** ready, doing, yours, held, parked, done or past. */
    val lane: String,
    val entry: Long,
    val started: Long? = null,
    val ended: Long? = null,
    val urgency: Double = 0.0,
    val reference: String? = null,
    val has_note: Boolean = false,
    val visits: List<Visit> = emptyList(),
    val frees: List<String> = emptyList(),
    val waits_on: List<String> = emptyList(),
    val activity: List<Int> = emptyList(),
    val hold: Hold? = null,
) {
    val short: String get() = uuid.take(8)
    val open: Boolean get() = ended == null
}

@Serializable
data class Stop(
    val uuid: String,
    val title: String,
    /** done, doing, next, waiting or parked. */
    val state: String,
    val yours: Boolean = false,
    val col: Int,
    val track: Int,
)

@Serializable
data class Line(
    val name: String,
    val project: String = "",
    val stops: List<Stop>,
    /** [from, to]: `to` waits on `from`. */
    val links: List<List<Int>> = emptyList(),
    val cols: Int,
    val tracks: Int,
    val open: Int,
)

@Serializable
data class Event(
    val at: Long,
    val uuid: String,
    val title: String,
    val project: String = "",
    /** visit, ruling, done or added. */
    val kind: String,
    val text: String = "",
    val shas: List<String> = emptyList(),
)

@Serializable
data class DayStat(
    val label: String,
    val weekday: Int,
    val done: Int,
    val added: Int,
    val visits: Int,
    val open_yours: Int = 0,
    val open_agents: Int = 0,
)

@Serializable
data class ProjectStat(
    val name: String,
    val open: Int,
    val yours: Int,
    val doing: Int,
    val done_week: Int,
    val done_span: Int,
    val spark: List<Int> = emptyList(),
)

@Serializable
data class Bucket(val label: String, val count: Int)

@Serializable
data class LaneCounts(
    val ready: Int = 0,
    val doing: Int = 0,
    val yours: Int = 0,
    val held: Int = 0,
    val parked: Int = 0,
    val done: Int = 0,
)

@Serializable
data class Pulse(
    val days: List<DayStat> = emptyList(),
    val punch: List<List<Int>> = emptyList(),
    val projects: List<ProjectStat> = emptyList(),
    val cycle: List<Bucket> = emptyList(),
    val lanes: LaneCounts = LaneCounts(),
    val done_week: Int = 0,
    val added_week: Int = 0,
    val visits_week: Int = 0,
    val commit_share: Double = 0.0,
    val decisions_open: Int = 0,
    val decision_hours: Double? = null,
    val holding: Int = 0,
    val held_up: Int = 0,
)

@Serializable
data class Snapshot(
    val now: Long,
    /** Seconds east of UTC: times are shown in the desk's zone. */
    val offset: Long,
    val you: String = "",
    val cards: List<Card> = emptyList(),
    val lines: List<Line> = emptyList(),
    val log: List<Event> = emptyList(),
    val pulse: Pulse = Pulse(),
    val error: String? = null,
)

@Serializable
data class LampLine(
    /** calm, needs-you, alarm or error. */
    val state: String,
    val why: String = "",
)

@Serializable
data class Headline(val count: String, val holding: String? = null, val broke: String? = null)

@Serializable
data class Column(val lane: String, val title: String, val empty: String, val cards: List<String>)

@Serializable
data class DeskPlan(
    val frees: List<String> = emptyList(),
    val decisions: List<String> = emptyList(),
    val hands: List<String> = emptyList(),
    val most: Int = 1,
)

@Serializable
data class DrawerPlan(
    val alarms: List<String> = emptyList(),
    val decisions: List<String> = emptyList(),
    val rest: List<String> = emptyList(),
    val more: Int = 0,
)

@Serializable
data class ViewLine(val id: String, val name: String, val question: String)

/** Colors as `#rrggbbaa`. */
@Serializable
data class Palette(
    val panel: String,
    val well: String,
    val tile: String,
    val tile_hover: String,
    val border: String,
    val fg: String,
    val bright: String,
    val dim: String,
    val accent: String,
    val ok: String,
    val warn: String,
    val bad: String,
    val violet: String,
    val orange: String,
    val blue: String,
    val teal: String,
    val font: String = "monospace",
)

@Serializable
data class Screen(
    val wire: Int,
    val version: Long,
    val read_only: Boolean = false,
    val snapshot: Snapshot,
    val lamp: LampLine,
    val headline: Headline,
    val columns: List<Column>,
    val desk: DeskPlan,
    val drawer: DrawerPlan,
    val views: List<ViewLine>,
    val options: Map<String, List<String>> = emptyMap(),
    val palette: Palette,
    val projects: Map<String, String> = emptyMap(),
) {
    private val byUuid: Map<String, Card> by lazy { snapshot.cards.associateBy { it.uuid } }

    fun card(uuid: String): Card? = byUuid[uuid]

    fun cards(uuids: List<String>): List<Card> = uuids.mapNotNull { byUuid[it] }
}

/** The short version, for the widget, the tile and notifications. */
@Serializable
data class Glance(
    val wire: Int,
    val version: Long,
    val lamp: LampLine,
    val headline: Headline,
    val counts: LaneCounts = LaneCounts(),
    val done_week: Int = 0,
    val added_week: Int = 0,
    val spark: List<Int> = emptyList(),
    val alarms: List<Card> = emptyList(),
    val decisions: List<Card> = emptyList(),
    val rest: List<Card> = emptyList(),
    val more: Int = 0,
    val options: Map<String, List<String>> = emptyMap(),
)

@Serializable
data class Span(val text: String, val code: Boolean = false, val bold: Boolean = false)

@Serializable
data class NoteBlock(
    /** heading, para, bullet or code. */
    val kind: String,
    val level: Int = 0,
    val check: Boolean? = null,
    val spans: List<Span> = emptyList(),
)

/**
 * One button a card offers. Pressing it sends [words] to the desk; when
 * [ask] is set the phone first asks for a line, which goes after them.
 */
@Serializable
data class Offer(
    val label: String,
    val key: String? = null,
    val words: String,
    val ask: String? = null,
    /** act, priority or decide. */
    val group: String,
    /** "go", "choice" or "". */
    val style: String = "",
    val on: Boolean = false,
) {
    fun with(line: String): String = "$words ${line.trim()}".trim()
}

@Serializable
data class Page(val uuid: String, val note: List<NoteBlock> = emptyList(), val offers: List<Offer> = emptyList())

@Serializable
data class Hello(val wire: Int, val version: Long, val you: String = "", val read_only: Boolean = false)

/** What the desk said back to words: what it did, or a view or a new title. */
@Serializable
data class Said(
    val said: String = "",
    val result: String? = null,
    val view: String? = null,
    val new: String? = null,
    val version: Long? = null,
)
