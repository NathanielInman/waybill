package io.github.nathanielinman.waybill

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.app.RemoteInput
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.util.concurrent.TimeUnit

/**
 * Looks at the desk now and then while the app is closed: the glance
 * feeds the widget and the tile, and a new decision or alarm becomes a
 * notification whose buttons are the decision's options.
 */
class Watch(ctx: Context, params: WorkerParameters) : CoroutineWorker(ctx, params) {

    override suspend fun doWork(): Result {
        val pairing = Store.pairing(applicationContext) ?: return Result.success()
        val glance = runCatching { withContext(Dispatchers.IO) { Api(pairing).glance() } }.getOrNull()
            ?: return Result.success()
        Store.setGlance(applicationContext, glance)
        Notices.show(applicationContext, glance)
        WidgetProvider.update(applicationContext)
        LampTile.refresh(applicationContext)
        return Result.success()
    }

    companion object {
        private const val PERIODIC = "watch"
        private const val ONCE = "watch-now"

        private val online = Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build()

        /** Every 15 minutes, the shortest Android allows. */
        fun schedule(ctx: Context) {
            val req = PeriodicWorkRequestBuilder<Watch>(15, TimeUnit.MINUTES).setConstraints(online).build()
            WorkManager.getInstance(ctx).enqueueUniquePeriodicWork(PERIODIC, ExistingPeriodicWorkPolicy.KEEP, req)
        }

        /** Right away, after something changed from this phone. */
        fun now(ctx: Context) {
            val req = OneTimeWorkRequestBuilder<Watch>().setConstraints(online).build()
            WorkManager.getInstance(ctx).enqueueUniqueWork(ONCE, ExistingWorkPolicy.REPLACE, req)
        }
    }
}

/** The notifications: one per open decision and per alarm. */
object Notices {
    const val DECISIONS = "decisions"
    const val ALARMS = "alarms"
    const val EXTRA_CARD = "card"
    const val EXTRA_WORDS = "words"
    const val EXTRA_NOTICE = "notice"
    const val REPLY = "reply"

    fun channels(ctx: Context) {
        val nm = ctx.getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(
            NotificationChannel(DECISIONS, "Decisions", NotificationManager.IMPORTANCE_HIGH).apply {
                description = "A decision an agent filed for you, with its options as buttons"
            },
        )
        nm.createNotificationChannel(
            NotificationChannel(ALARMS, "Alarms", NotificationManager.IMPORTANCE_HIGH).apply {
                description = "Something broke and should be looked at first"
            },
        )
    }

    fun id(uuid: String) = uuid.hashCode()

    /** Opens the app on a card. */
    fun openCard(ctx: Context, uuid: String): PendingIntent {
        val i = Intent(ctx, MainActivity::class.java)
            .setFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
            .putExtra(EXTRA_CARD, uuid)
        return PendingIntent.getActivity(ctx, uuid.hashCode(), i, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
    }

    private fun sayIntent(ctx: Context, uuid: String, words: String, code: Int, mutable: Boolean = false): PendingIntent {
        val i = Intent(ctx, SayReceiver::class.java)
            .putExtra(EXTRA_CARD, uuid)
            .putExtra(EXTRA_WORDS, words)
            .putExtra(EXTRA_NOTICE, id(uuid))
        val flags = PendingIntent.FLAG_UPDATE_CURRENT or if (mutable) PendingIntent.FLAG_MUTABLE else PendingIntent.FLAG_IMMUTABLE
        return PendingIntent.getBroadcast(ctx, code, i, flags)
    }

    fun show(ctx: Context, g: Glance) {
        if (ctx.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) return
        val nm = NotificationManagerCompat.from(ctx)
        val seen = Store.seen(ctx)
        val now = (g.alarms + g.decisions).map { it.uuid }.toSet()
        // What closed since: its notification goes too.
        (seen - now).forEach { nm.cancel(id(it)) }
        g.alarms.filter { it.uuid !in seen }.forEach { card ->
            val n = NotificationCompat.Builder(ctx, ALARMS)
                .setSmallIcon(R.drawable.ic_lamp)
                .setColor(0xffbf616a.toInt())
                .setContentTitle("Something broke.")
                .setContentText(card.title)
                .setSubText(card.project)
                .setCategory(NotificationCompat.CATEGORY_ERROR)
                .setContentIntent(openCard(ctx, card.uuid))
                .setAutoCancel(true)
                .addAction(0, "Take it", sayIntent(ctx, card.uuid, "mine", id(card.uuid) + 1))
                .build()
            nm.notify(id(card.uuid), n)
        }
        g.decisions.filter { it.uuid !in seen }.forEach { card ->
            val options = g.options[card.uuid].orEmpty()
            val b = NotificationCompat.Builder(ctx, DECISIONS)
                .setSmallIcon(R.drawable.ic_lamp)
                .setColor(0xffebcb8b.toInt())
                .setContentTitle(card.title)
                .setContentText(options.joinToString(" · ").ifEmpty { "A decision waits on you." })
                .setSubText(card.project)
                .setCategory(NotificationCompat.CATEGORY_REMINDER)
                .setContentIntent(openCard(ctx, card.uuid))
                .setAutoCancel(true)
            // Android shows three buttons: the options, or two and a reply in your own words.
            val shown = if (options.size >= 3) options.take(3) else options
            shown.forEachIndexed { i, option ->
                b.addAction(0, option, sayIntent(ctx, card.uuid, "rule $option", id(card.uuid) + 1 + i))
            }
            if (shown.size < 3) {
                val input = RemoteInput.Builder(REPLY).setLabel("Your ruling").build()
                val reply = NotificationCompat.Action.Builder(
                    0, if (options.isEmpty()) "Rule" else "Something else",
                    sayIntent(ctx, card.uuid, "rule", id(card.uuid) + 9, mutable = true),
                ).addRemoteInput(input).build()
                b.addAction(reply)
            }
            nm.notify(id(card.uuid), b.build())
        }
        Store.setSeen(ctx, now)
    }
}
