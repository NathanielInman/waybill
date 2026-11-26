package io.github.nathanielinman.waybill

/**
 * The demo list as `waybill serve` would send it, written by the Rust
 * test `wire::tests::the_phone_fixtures_are_current` from the same
 * made-up moment the desk's own tests use.
 */
object Fixtures {
    private fun read(name: String): String =
        requireNotNull(Fixtures::class.java.getResource("/demo/$name")) { "no fixture $name" }.readText()

    val screen: Screen by lazy { wireJson.decodeFromString(read("screen.json")) }
    val glance: Glance by lazy { wireJson.decodeFromString(read("glance.json")) }
    val pages: Map<String, Page> by lazy { wireJson.decodeFromString(read("pages.json")) }

    fun card(title: String): Card = screen.snapshot.cards.first { it.title == title }
}
