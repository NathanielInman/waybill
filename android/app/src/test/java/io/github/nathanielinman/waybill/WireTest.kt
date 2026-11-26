package io.github.nathanielinman.waybill

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The phone reads what the desk writes. A rename on the desk fails here. */
class WireTest {
    @Test
    fun readsTheDesksScreen() {
        val s = Fixtures.screen
        assertEquals(WIRE, s.wire)
        assertEquals("needs-you", s.lamp.state)
        assertEquals("12 tasks wait on you.", s.headline.count)
        assertEquals(listOf("board", "desk", "lines", "log", "pulse"), s.views.map { it.id })
        assertEquals("ready", s.columns.first().lane)
        // Every uuid the desk's plan names is a card in the snapshot.
        val named = s.columns.flatMap { it.cards } + s.desk.frees + s.desk.decisions + s.desk.hands
        named.forEach { assertNotNull("no card $it", s.card(it)) }
        assertEquals("Playtest the lantern level", s.card(s.desk.frees.first())?.title)
        assertTrue(s.snapshot.lines.any { it.name == "Checkout" })
        assertEquals(42, s.snapshot.pulse.days.size)
    }

    @Test
    fun readsTheGlanceAndPages() {
        val g = Fixtures.glance
        assertEquals(14, g.spark.size)
        val launch = g.decisions.first { it.title == "Pick the launch discount" }
        assertEquals(
            listOf("10% for the first week", "Free shipping, no discount", "No offer at launch"),
            g.options[launch.uuid],
        )
        val page = Fixtures.pages.getValue(Fixtures.card("Playtest the lantern level").uuid)
        assertEquals("Start", page.offers.first().label)
        assertTrue(page.note.any { it.check == false })
        val rule = Fixtures.pages.getValue(launch.uuid).offers.first { it.label == "No offer at launch" }
        assertEquals("rule No offer at launch", rule.words)
    }

    @Test
    fun aPairLinkCarriesTheServerAndToken() {
        val token = "a".repeat(64)
        val p = Pairing.parse("waybill://pair?url=https%3A%2F%2Fbox.lan%3A8446&token=$token")
        assertEquals(Pairing("https://box.lan:8446", token), p)
        assertEquals(null, Pairing.parse("https://box.lan/pair"))
        assertEquals(null, Pairing.parse("waybill://pair?url=ftp%3A%2F%2Fx&token=$token"))
    }

    @Test
    fun wordsTheBoardUses() {
        assertEquals("now", span(30))
        assertEquals("7 min", span(7 * 60))
        assertEquals("3 h", span(3 * 3600 + 5))
        assertEquals("2 d ago", ago(2 * 86_400 + 1))
        assertEquals("Nov 12", dayLabel(20_404))
        val offer = Offer(label = "Visit", words = "visit", ask = "What happened", group = "act")
        assertEquals("visit tried it", offer.with(" tried it "))
        assertEquals("done", Offer(label = "Done", words = "done", group = "act").with(""))
    }
}
