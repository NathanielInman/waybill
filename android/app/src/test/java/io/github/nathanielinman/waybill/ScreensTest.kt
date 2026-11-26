package io.github.nathanielinman.waybill

import androidx.compose.runtime.Composable
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onRoot
import com.github.takahirom.roborazzi.captureRoboImage
import io.github.nathanielinman.waybill.ui.Acts
import io.github.nathanielinman.waybill.ui.Shell
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * Every view drawn from the demo list. `./gradlew recordRoborazziDebug`
 * writes them to docs/assets/screens/android for the README; a plain test
 * run only checks that each one composes.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [35], qualifiers = "w412dp-h892dp-xhdpi")
class ScreensTest {
    @get:Rule
    val compose = createComposeRule()

    private val screen = Fixtures.screen

    private fun shot(name: String, content: @Composable () -> Unit) {
        compose.setContent {
            WaybillTheme(Colors.from(screen.palette, screen.projects)) { content() }
        }
        compose.waitForIdle()
        compose.onRoot().captureRoboImage("../../docs/assets/screens/android/$name.png")
    }

    @Composable
    private fun app(tab: String, open: Card? = null, since: Long = 0, lane: Int = 2) = Shell(
        screen = screen,
        tab = tab,
        onTab = {},
        openCard = open?.uuid,
        page = open?.let { Fixtures.pages[it.uuid] },
        acts = Acts.none,
        onClose = {},
        since = since,
        firstLane = lane,
    )

    @Test
    fun desk() = shot("desk") { app("desk") }

    @Test
    fun board() = shot("board") { app("board", lane = 1) }

    @Test
    fun card() = shot("card") { app("desk", open = Fixtures.card("Playtest the lantern level")) }

    @Test
    fun decision() = shot("decision") { app("board", open = Fixtures.card("Pick the launch discount"), lane = 2) }

    @Test
    fun lines() = shot("lines") { app("lines") }

    @Test
    fun log() = shot("log") { app("log", since = screen.snapshot.now - 3 * 3600) }

    @Test
    fun pulse() = shot("pulse") { app("pulse") }

    @Test
    fun pair() = shot("pair") { PairScreen(null) }
}
