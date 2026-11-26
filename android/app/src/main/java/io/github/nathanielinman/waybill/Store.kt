package io.github.nathanielinman.waybill

import android.content.Context
import android.content.SharedPreferences
import androidx.core.content.edit

/**
 * What the app keeps between runs, in its private preferences (no backup:
 * a token is good for this install only).
 */
object Store {
    private const val URL = "url"
    private const val TOKEN = "token"
    private const val SEEN = "seen"
    private const val LAST_LOOK = "last_look"
    private const val GLANCE = "glance"

    private fun prefs(ctx: Context): SharedPreferences =
        ctx.applicationContext.getSharedPreferences("waybill", Context.MODE_PRIVATE)

    fun pairing(ctx: Context): Pairing? {
        val p = prefs(ctx)
        val url = p.getString(URL, null) ?: return null
        val token = p.getString(TOKEN, null) ?: return null
        return Pairing(url, token)
    }

    fun pair(ctx: Context, pairing: Pairing) = prefs(ctx).edit {
        putString(URL, pairing.url)
        putString(TOKEN, pairing.token)
        remove(SEEN)
        remove(GLANCE)
    }

    fun forget(ctx: Context) = prefs(ctx).edit { clear() }

    /** The decisions and alarms a notification has already been shown for. */
    fun seen(ctx: Context): Set<String> = prefs(ctx).getStringSet(SEEN, emptySet()) ?: emptySet()

    fun setSeen(ctx: Context, uuids: Set<String>) = prefs(ctx).edit { putStringSet(SEEN, uuids) }

    /** When the Log was last looked at, in the desk's seconds, for "since you left". */
    fun lastLook(ctx: Context): Long = prefs(ctx).getLong(LAST_LOOK, 0)

    fun setLastLook(ctx: Context, at: Long) = prefs(ctx).edit { putLong(LAST_LOOK, at) }

    /** The last glance, so the widget and the tile draw something while the desk is away. */
    fun glance(ctx: Context): Glance? =
        prefs(ctx).getString(GLANCE, null)?.let { runCatching { wireJson.decodeFromString<Glance>(it) }.getOrNull() }

    fun setGlance(ctx: Context, g: Glance) =
        prefs(ctx).edit { putString(GLANCE, wireJson.encodeToString(Glance.serializer(), g)) }
}
