package io.github.nathanielinman.waybill

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import java.io.IOException
import java.net.ConnectException
import java.net.HttpURLConnection
import java.net.NoRouteToHostException
import java.net.SocketTimeoutException
import java.net.URI
import java.net.URLEncoder
import java.net.UnknownHostException

/** The desk answered with an error, in its own words. */
class ApiError(val status: Int, message: String) : IOException(message)

/** The desk is not there: asleep, off the network, or `waybill serve` is not running. */
class Unreachable(message: String, cause: Throwable) : IOException(message, cause)

/** Where this phone reads the list: the server and the token `waybill pair` showed. */
data class Pairing(val url: String, val token: String) {
    companion object {
        /** `waybill://pair?url=https%3A%2F%2Fbox%3A8446&token=...`, as `waybill pair` prints it. */
        fun parse(link: String): Pairing? {
            val uri = runCatching { URI(link.trim()) }.getOrNull() ?: return null
            if (uri.scheme != "waybill" || uri.host != "pair") return null
            val params = (uri.rawQuery ?: return null).split('&').mapNotNull {
                val (k, v) = it.split('=', limit = 2).takeIf { p -> p.size == 2 } ?: return@mapNotNull null
                k to java.net.URLDecoder.decode(v, "UTF-8")
            }.toMap()
            val url = params["url"]?.trimEnd('/') ?: return null
            val token = params["token"] ?: return null
            if (!(url.startsWith("http://") || url.startsWith("https://")) || token.length < 32) return null
            return Pairing(url, token)
        }
    }
}

/**
 * Talks to `waybill serve` on the desk. Every call blocks; run it off the
 * main thread.
 */
class Api(private val pairing: Pairing) {

    fun hello(): Hello = wireJson.decodeFromString(get("/v1/hello"))

    /** The screen; with [after], waits on the desk until something newer exists. */
    fun screen(after: Long? = null): Screen {
        val path = if (after == null) "/v1/screen" else "/v1/screen?after=$after"
        return wireJson.decodeFromString(get(path, readTimeoutMs = if (after == null) 20_000 else 40_000))
    }

    fun glance(): Glance = wireJson.decodeFromString(get("/v1/glance"))

    fun page(uuid: String): Page =
        wireJson.decodeFromString(get("/v1/card/" + URLEncoder.encode(uuid, "UTF-8")))

    /** Hands words to the desk's `/` line, about [card] when one is open. */
    fun say(text: String, card: String?): Said {
        val body = buildJsonObject {
            put("text", JsonPrimitive(text))
            if (card != null) put("card", JsonPrimitive(card))
        }
        return wireJson.decodeFromString(post("/v1/say", body))
    }

    fun add(title: String, project: String, priority: String, mine: Boolean, note: String): Said {
        val body = buildJsonObject {
            put("title", JsonPrimitive(title))
            put("project", JsonPrimitive(project))
            put("priority", JsonPrimitive(priority))
            put("mine", JsonPrimitive(mine))
            put("note", JsonPrimitive(note))
        }
        return wireJson.decodeFromString(post("/v1/add", body))
    }

    private fun get(path: String, readTimeoutMs: Int = 20_000): String = call("GET", path, null, readTimeoutMs)

    private fun post(path: String, body: JsonObject): String = call("POST", path, body.toString(), 60_000)

    private fun call(method: String, path: String, body: String?, readTimeoutMs: Int): String {
        val url = URI(pairing.url + path).toURL()
        val conn = url.openConnection() as HttpURLConnection
        try {
            conn.requestMethod = method
            conn.connectTimeout = 6_000
            conn.readTimeout = readTimeoutMs
            conn.useCaches = false
            conn.setRequestProperty("Authorization", "Bearer ${pairing.token}")
            conn.setRequestProperty("Accept", "application/json")
            if (body != null) {
                conn.doOutput = true
                conn.setRequestProperty("Content-Type", "application/json")
                conn.outputStream.use { it.write(body.toByteArray()) }
            }
            val status = conn.responseCode
            val stream = if (status in 200..299) conn.inputStream else conn.errorStream
            val text = stream?.use { it.readBytes().decodeToString() }.orEmpty()
            if (status !in 200..299) {
                val message = runCatching {
                    wireJson.parseToJsonElement(text).jsonObject["error"]?.jsonPrimitive?.content
                }.getOrNull()
                throw ApiError(status, message ?: "The desk answered $status.")
            }
            return text
        } catch (e: UnknownHostException) {
            throw Unreachable("Can't find ${url.host}. Is this phone on the desk's network?", e)
        } catch (e: ConnectException) {
            throw Unreachable("The desk isn't answering. Is waybill serve running?", e)
        } catch (e: NoRouteToHostException) {
            throw Unreachable("Can't reach the desk. Is this phone on its network?", e)
        } catch (e: SocketTimeoutException) {
            throw Unreachable("The desk took too long to answer.", e)
        } finally {
            conn.disconnect()
        }
    }
}
