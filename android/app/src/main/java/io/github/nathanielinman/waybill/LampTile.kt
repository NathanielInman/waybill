package io.github.nathanielinman.waybill

import android.annotation.SuppressLint
import android.app.PendingIntent
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.os.Build
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService

/**
 * The lamp in Quick Settings: lit while something waits on you, with how
 * much in its subtitle. A tap opens the Desk.
 */
class LampTile : TileService() {
    override fun onStartListening() {
        val tile = qsTile ?: return
        val g = Store.glance(this)
        tile.label = "Waybill"
        if (g == null) {
            tile.state = Tile.STATE_INACTIVE
            tile.subtitle = "not paired"
        } else {
            tile.state = if (g.lamp.state == "calm") Tile.STATE_INACTIVE else Tile.STATE_ACTIVE
            tile.subtitle = when {
                g.alarms.isNotEmpty() -> plural(g.alarms.size, "alarm")
                g.decisions.isNotEmpty() -> plural(g.decisions.size, "decision") + " · ${g.counts.yours} on you"
                else -> "${g.counts.yours} on you"
            }
        }
        tile.updateTile()
    }

    @SuppressLint("StartActivityAndCollapseDeprecated")
    override fun onClick() {
        val i = Intent(this, MainActivity::class.java).setFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        if (Build.VERSION.SDK_INT >= 34) {
            startActivityAndCollapse(PendingIntent.getActivity(this, 0, i, PendingIntent.FLAG_IMMUTABLE))
        } else {
            @Suppress("DEPRECATION")
            startActivityAndCollapse(i)
        }
    }

    companion object {
        fun refresh(ctx: Context) {
            requestListeningState(ctx, ComponentName(ctx, LampTile::class.java))
        }
    }
}
