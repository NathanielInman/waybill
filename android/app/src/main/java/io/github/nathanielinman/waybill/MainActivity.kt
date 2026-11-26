package io.github.nathanielinman.waybill

import android.Manifest
import android.app.Application
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import io.github.nathanielinman.waybill.ui.Acts
import io.github.nathanielinman.waybill.ui.Dot
import io.github.nathanielinman.waybill.ui.Shell
import io.github.nathanielinman.waybill.ui.T
import kotlinx.coroutines.launch

class WaybillApp : Application() {
    override fun onCreate() {
        super.onCreate()
        Notices.channels(this)
        if (Store.pairing(this) != null) Watch.schedule(this)
    }
}

class MainActivity : ComponentActivity() {
    private val model: Model by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        handle(intent)
        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) { model.follow() }
        }
        setContent {
            val st by model.state.collectAsStateWithLifecycle()
            val screen = st.screen
            WaybillTheme(if (screen != null) Colors.from(screen.palette, screen.projects) else Colors.default) {
                when {
                    st.pairing == null -> PairScreen(model)
                    screen == null -> Waiting(st.status.problem)
                    else -> Shell(
                        screen = screen,
                        tab = st.tab,
                        onTab = model::tab,
                        openCard = st.openCard,
                        page = st.page,
                        acts = object : Acts {
                            override fun open(uuid: String) = model.open(uuid)
                            override fun say(text: String, card: String?) = model.say(text, card)
                            override fun view(id: String) = model.tab(id)
                        },
                        onClose = { model.open(null) },
                        status = st.status,
                        since = st.since,
                    )
                }
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handle(intent)
    }

    override fun onStart() {
        super.onStart()
        // Opening the app brings the widget, the tile and notifications up to date.
        if (Store.pairing(this) != null) Watch.now(this)
        if (Store.pairing(this) != null && checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
            requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 1)
        }
    }

    /** A pair link from the camera, or a notification's card. */
    private fun handle(intent: Intent?) {
        intent ?: return
        intent.data?.toString()?.takeIf { it.startsWith("waybill://pair") }?.let { link ->
            model.pair(link) { }
        }
        intent.getStringExtra(Notices.EXTRA_CARD)?.let { model.open(it) }
    }
}

@Composable
private fun Waiting(problem: String?) {
    val c = LocalColors.current
    Box(Modifier.fillMaxSize().background(c.panel).windowInsetsPadding(WindowInsets.safeDrawing).padding(24.dp), contentAlignment = Alignment.Center) {
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Dot(c.dim, 10.dp)
            T(problem ?: "Reading the list from the desk…", Type.body, if (problem != null) c.bad else c.dim)
        }
    }
}

/** First run: pair with the desk through the link `waybill pair` shows. */
@Composable
fun PairScreen(model: Model?) {
    val c = LocalColors.current
    var link by remember { mutableStateOf("") }
    var problem by remember { mutableStateOf<String?>(null) }
    Column(
        Modifier.fillMaxSize().background(c.panel).windowInsetsPadding(WindowInsets.safeDrawing).padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        T("WAYBILL", Type.small.copy(letterSpacing = 3.sp), c.bright, bold = true)
        T("Pair with your desk", Type.headline, c.bright)
        T(
            "On the desk, run waybill serve and keep it running, then run waybill pair. " +
                "Scan its QR code with this phone's camera, or paste the link it prints here.",
            Type.body, c.fg,
        )
        Box(
            Modifier.fillMaxWidth().clip(RoundedCornerShape(10.dp)).background(c.tile)
                .border(1.dp, c.border, RoundedCornerShape(10.dp)).padding(12.dp),
        ) {
            if (link.isEmpty()) T("waybill://pair?url=…&token=…", Type.body, c.dim)
            BasicTextField(link, { link = it }, textStyle = Type.body.copy(color = c.bright), cursorBrush = SolidColor(c.accent), modifier = Modifier.fillMaxWidth())
        }
        problem?.let { T(it, Type.body, c.bad) }
        Box(
            Modifier.clip(RoundedCornerShape(10.dp)).background(c.accent)
                .clickable { model?.pair(link) { problem = it } }.padding(horizontal = 18.dp, vertical = 11.dp),
        ) { T("Pair", Type.title, c.well, bold = true) }
        T(
            "The phone shows what the desk's board shows and sends back the same words its / line reads. " +
                "It keeps no copy of the list.",
            Type.small, c.dim,
        )
    }
}
