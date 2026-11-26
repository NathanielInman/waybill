package io.github.nathanielinman.waybill

/*
 * Times and counts in the words the board uses (`waybill_core::clock` and
 * the board's `ui.rs`). Times are shown in the desk's zone, from the
 * snapshot's own `now` and `offset`.
 */

private const val HOUR = 3_600L
private const val DAY = 86_400L
private val MONTHS = listOf("Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec")

/** "now", "7 min", "3 h", "2 d". */
fun span(seconds: Long): String {
    val s = seconds.coerceAtLeast(0)
    return when {
        s < 60 -> "now"
        s < HOUR -> "${s / 60} min"
        s < DAY -> "${s / HOUR} h"
        else -> "${s / DAY} d"
    }
}

/** "just now", "7 min ago", "3 d ago". */
fun ago(seconds: Long): String = when (val sp = span(seconds)) {
    "now" -> "just now"
    else -> "$sp ago"
}

/** 09:22 in the desk's zone. */
fun hm(s: Snapshot, ts: Long): String {
    val local = Math.floorMod(ts + s.offset, DAY)
    return "%02d:%02d".format(local / HOUR, local % HOUR / 60)
}

/** "Nov 12" for a day count since 1970 (Howard Hinnant's civil_from_days). */
fun dayLabel(day: Long): String {
    val z = day + 719_468
    val era = Math.floorDiv(z, 146_097L)
    val doe = z - era * 146_097
    val yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365
    val doy = doe - (365 * yoe + yoe / 4 - yoe / 100)
    val mp = (5 * doy + 2) / 153
    val d = doy - (153 * mp + 2) / 5 + 1
    val m = if (mp < 10) mp + 3 else mp - 9
    return "${MONTHS[(m - 1).toInt()]} $d"
}

fun localDay(s: Snapshot, ts: Long): Long = Math.floorDiv(ts + s.offset, DAY)

/** "09:22", "yesterday 17:05" or "Nov 12". */
fun whenText(s: Snapshot, ts: Long): String = when (localDay(s, s.now) - localDay(s, ts)) {
    0L -> hm(s, ts)
    1L -> "yesterday ${hm(s, ts)}"
    else -> dayLabel(localDay(s, ts))
}

/** How long a card has been open, or how long it has been held. */
fun age(s: Snapshot, c: Card): String = span(s.now - (c.started ?: c.entry))

fun plural(n: Int, one: String, many: String = one + "s") = "$n ${if (n == 1) one else many}"

/** Who holds a started card, as the board words it. */
fun holdLine(s: Snapshot, c: Card): Pair<String, Boolean> {
    val hold = c.hold
    val stale = hold?.state == "stale"
    val who = when {
        hold?.state == "held" -> hold.who
        stale && hold.who.isNotEmpty() -> "left by ${hold.who}"
        stale -> "left from before the last boot"
        else -> "no claim"
    }
    val held = when (val a = age(s, c)) {
        "now" -> "just started"
        else -> "held $a"
    }
    return "$held · $who" to stale
}
