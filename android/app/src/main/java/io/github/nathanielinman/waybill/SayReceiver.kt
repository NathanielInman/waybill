package io.github.nathanielinman.waybill

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.widget.Toast
import androidx.core.app.NotificationManagerCompat
import androidx.core.app.RemoteInput
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * A notification's button: its words go to the desk as if typed on the
 * `/` line with the card open ("rule 10% for the first week").
 */
class SayReceiver : BroadcastReceiver() {
    override fun onReceive(ctx: Context, intent: Intent) {
        val card = intent.getStringExtra(Notices.EXTRA_CARD) ?: return
        var words = intent.getStringExtra(Notices.EXTRA_WORDS) ?: return
        RemoteInput.getResultsFromIntent(intent)?.getCharSequence(Notices.REPLY)?.let {
            words = "$words ${it.toString().trim()}"
        }
        val notice = intent.getIntExtra(Notices.EXTRA_NOTICE, 0)
        val pairing = Store.pairing(ctx) ?: return
        val pending = goAsync()
        CoroutineScope(Dispatchers.IO).launch {
            val result = runCatching { Api(pairing).say(words, card) }
            withContext(Dispatchers.Main) {
                result.onSuccess {
                    NotificationManagerCompat.from(ctx).cancel(notice)
                    Toast.makeText(ctx, it.result ?: it.said, Toast.LENGTH_SHORT).show()
                    Watch.now(ctx)
                }.onFailure {
                    Toast.makeText(ctx, it.message ?: "The desk did not take it.", Toast.LENGTH_LONG).show()
                }
                pending.finish()
            }
        }
    }
}
