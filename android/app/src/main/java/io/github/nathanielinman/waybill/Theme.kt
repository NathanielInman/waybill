package io.github.nathanielinman.waybill

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp

/** `#rrggbbaa` or `#rrggbb` into a color. */
fun hex(s: String?, fallback: Color = Color.Magenta): Color {
    val h = s?.removePrefix("#") ?: return fallback
    val v = h.toLongOrNull(16) ?: return fallback
    return when (h.length) {
        8 -> Color(((v and 0xff) shl 24) or (v shr 8))
        6 -> Color(0xff000000 or v)
        else -> fallback
    }
}

/**
 * The desk's palette, as `waybill_core::theme` worked it out (waybar's
 * colors included), so the phone wears the same colors as the board.
 * Color has fixed meanings: accent is agents' work, warn (amber) is yours,
 * ok (green) is started or finished, violet is a decision, bad (red) is
 * high priority or something broken.
 */
@Immutable
data class Colors(
    val panel: Color,
    val well: Color,
    val tile: Color,
    val tileHover: Color,
    val border: Color,
    val fg: Color,
    val bright: Color,
    val dim: Color,
    val accent: Color,
    val ok: Color,
    val warn: Color,
    val bad: Color,
    val violet: Color,
    val orange: Color,
    val blue: Color,
    val teal: Color,
    val projects: Map<String, Color> = emptyMap(),
) {
    fun project(name: String): Color = projects[name] ?: accent

    /** The lamp's color for its state. */
    fun lamp(state: String): Color = when (state) {
        "needs-you" -> warn
        "alarm", "error" -> bad
        else -> dim
    }

    companion object {
        fun from(p: Palette, projects: Map<String, String> = emptyMap()) = Colors(
            panel = hex(p.panel), well = hex(p.well), tile = hex(p.tile), tileHover = hex(p.tile_hover),
            border = hex(p.border), fg = hex(p.fg), bright = hex(p.bright), dim = hex(p.dim),
            accent = hex(p.accent), ok = hex(p.ok), warn = hex(p.warn), bad = hex(p.bad),
            violet = hex(p.violet), orange = hex(p.orange), blue = hex(p.blue), teal = hex(p.teal),
            projects = projects.mapValues { hex(it.value) },
        )

        /** The board's built-in theme, for before the desk has answered. */
        val default = Colors(
            panel = Color(0xff1e2024), well = Color(0xff17191c), tile = Color(0xff26292e),
            tileHover = Color(0xff2f3339), border = Color(0xff363b43), fg = Color(0xffc5c8c6),
            bright = Color(0xffeceff4), dim = Color(0xff7c838e), accent = Color(0xff88c0d0),
            ok = Color(0xffa3be8c), warn = Color(0xffebcb8b), bad = Color(0xffbf616a),
            violet = Color(0xffb48ead), orange = Color(0xffd08770), blue = Color(0xff81a1c1),
            teal = Color(0xff8fbcbb),
        )
    }
}

val Mono = FontFamily(
    Font(R.font.jetbrains_mono_regular, FontWeight.Normal),
    Font(R.font.jetbrains_mono_bold, FontWeight.Bold),
)

val LocalColors = staticCompositionLocalOf { Colors.default }

object Type {
    val body = TextStyle(fontFamily = Mono, fontSize = 13.sp, lineHeight = 18.sp)
    val small = TextStyle(fontFamily = Mono, fontSize = 11.sp, lineHeight = 15.sp)
    val tiny = TextStyle(fontFamily = Mono, fontSize = 10.sp, lineHeight = 13.sp, letterSpacing = 0.8.sp)
    val title = TextStyle(fontFamily = Mono, fontSize = 14.sp, lineHeight = 19.sp)
    val headline = TextStyle(fontFamily = Mono, fontSize = 19.sp, lineHeight = 26.sp, fontWeight = FontWeight.Bold)
    val big = TextStyle(fontFamily = Mono, fontSize = 24.sp, lineHeight = 28.sp, fontWeight = FontWeight.Bold)
}

@Composable
fun WaybillTheme(colors: Colors, content: @Composable () -> Unit) {
    CompositionLocalProvider(LocalColors provides colors, content = content)
}
