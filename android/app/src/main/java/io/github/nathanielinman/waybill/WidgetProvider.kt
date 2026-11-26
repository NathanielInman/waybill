package io.github.nathanielinman.waybill

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.view.View
import android.widget.RemoteViews

/**
 * The drawer on the home screen: the lamp, the headline, the top of your
 * queue (decisions first) and the day's count, from the last glance.
 */
class WidgetProvider : AppWidgetProvider() {
    override fun onUpdate(ctx: Context, manager: AppWidgetManager, ids: IntArray) {
        ids.forEach { manager.updateAppWidget(it, views(ctx)) }
        Watch.now(ctx)
    }

    companion object {
        private val ROWS = listOf(R.id.row1, R.id.row2, R.id.row3)

        fun update(ctx: Context) {
            val manager = AppWidgetManager.getInstance(ctx)
            val ids = manager.getAppWidgetIds(ComponentName(ctx, WidgetProvider::class.java))
            if (ids.isNotEmpty()) ids.forEach { manager.updateAppWidget(it, views(ctx)) }
        }

        private fun views(ctx: Context): RemoteViews {
            val v = RemoteViews(ctx.packageName, R.layout.widget)
            val open = PendingIntent.getActivity(
                ctx, 0, Intent(ctx, MainActivity::class.java).setFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
                PendingIntent.FLAG_IMMUTABLE,
            )
            v.setOnClickPendingIntent(R.id.widget, open)
            val g = Store.glance(ctx)
            if (g == null) {
                v.setTextViewText(R.id.count, "")
                v.setTextViewText(R.id.headline, if (Store.pairing(ctx) == null) "Open Waybill to pair it with your desk." else "Waiting for the desk…")
                ROWS.forEach { v.setViewVisibility(it, View.GONE) }
                return v
            }
            val lamp = when (g.lamp.state) {
                "needs-you" -> 0xffebcb8b
                "alarm", "error" -> 0xffbf616a
                else -> 0xff7c838e
            }.toInt()
            v.setTextColor(R.id.lamp, lamp)
            v.setTextViewText(R.id.counts, "${g.counts.ready} ready · ${g.counts.doing} doing")
            v.setTextViewText(R.id.count, g.counts.yours.toString())
            v.setTextColor(R.id.count, lamp)
            v.setTextViewText(R.id.headline, listOfNotNull(g.headline.broke, g.headline.count, g.headline.holding).joinToString(" "))
            val rows = g.alarms.map { "! ${it.title}" } +
                g.decisions.map { "◆ ${it.title}" } +
                g.rest.map { if (it.frees.isNotEmpty()) "${it.frees.size} ${it.title}" else "■ ${it.title}" }
            ROWS.forEachIndexed { i, id ->
                val text = rows.getOrNull(i)
                v.setViewVisibility(id, if (text == null) View.GONE else View.VISIBLE)
                v.setTextViewText(id, text ?: "")
            }
            v.setTextViewText(R.id.today, "${g.counts.done} finished today, ${g.done_week} this week")
            return v
        }
    }
}
