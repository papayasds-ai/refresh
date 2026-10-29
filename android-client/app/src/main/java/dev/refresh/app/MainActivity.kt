package dev.refresh.app

import android.graphics.BitmapFactory
import android.os.Bundle
import android.view.MotionEvent
import android.widget.Button
import android.widget.EditText
import android.widget.ImageView
import android.widget.TextView
import androidx.appcompat.app.AlertDialog
import androidx.appcompat.app.AppCompatActivity
import dev.refresh.ffi.RefreshClient
import dev.refresh.ffi.RefreshSession
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * Minimal reference UI for the Refresh Android client. It intentionally
 * does the least amount of app-layer work possible: all cryptography,
 * pairing-trust logic, and the wire protocol live in refresh-mobile-ffi
 * (Rust), reused unchanged from the Windows host. This activity's only
 * job is to show the screen stream and forward touches.
 */
class MainActivity : AppCompatActivity() {

    private lateinit var client: RefreshClient
    private var session: RefreshSession? = null
    private var frameLoopJob: Job? = null
    private val scope = CoroutineScope(Dispatchers.Main + Job())

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)

        // Each device's identity key lives in the app's private storage,
        // never leaves it, and is never uploaded anywhere.
        client = RefreshClient(filesDir.absolutePath)

        findViewById<TextView>(R.id.fingerprintText).text =
            "This device's fingerprint: ${client.fingerprint()}\n" +
            "Show this on the PC when pairing from there, or use 'Pair this device' below to trust the PC."

        findViewById<Button>(R.id.pairButton).setOnClickListener { showPairDialog() }
        findViewById<Button>(R.id.connectButton).setOnClickListener { connect() }

        findViewById<ImageView>(R.id.screenView).setOnTouchListener { view, event ->
            val s = session ?: return@setOnTouchListener false
            val xNorm = (event.x / view.width).coerceIn(0f, 1f)
            val yNorm = (event.y / view.height).coerceIn(0f, 1f)
            val phase = when (event.action) {
                MotionEvent.ACTION_DOWN -> "start"
                MotionEvent.ACTION_UP -> "end"
                else -> "move"
            }
            s.sendTouch(xNorm, yNorm, phase)
            true
        }
    }

    private fun showPairDialog() {
        val input = EditText(this)
        input.hint = "Paste the PC's pairing code"
        AlertDialog.Builder(this)
            .setTitle("Pair with PC")
            .setMessage(
                "On the PC, run: pair show\n" +
                "Compare the fingerprint it prints with what you expect, " +
                "then paste its pairing code here."
            )
            .setView(input)
            .setPositiveButton("Trust") { _, _ ->
                val code = input.text.toString().trim()
                scope.launch {
                    try {
                        withContext(Dispatchers.IO) {
                            client.trustDevice(code, "This PC")
                        }
                    } catch (e: Exception) {
                        AlertDialog.Builder(this@MainActivity)
                            .setTitle("Pairing failed")
                            .setMessage(e.message)
                            .show()
                    }
                }
            }
            .setNegativeButton("Cancel", null)
            .show()
    }

    private fun connect() {
        val host = findViewById<EditText>(R.id.hostInput).text.toString().trim()
        if (host.isEmpty()) return

        scope.launch {
            val newSession = try {
                withContext(Dispatchers.IO) { client.connect(host, 7878u) }
            } catch (e: Exception) {
                AlertDialog.Builder(this@MainActivity)
                    .setTitle("Connection failed")
                    .setMessage(e.message)
                    .show()
                return@launch
            }
            session?.close()
            session = newSession
            startFrameLoop(newSession)
        }
    }

    private fun startFrameLoop(s: RefreshSession) {
        frameLoopJob?.cancel()
        frameLoopJob = scope.launch {
            val imageView = findViewById<ImageView>(R.id.screenView)
            while (isActive()) {
                val bytes = withContext(Dispatchers.IO) { s.nextFrame(1000u) }
                if (bytes.isNotEmpty()) {
                    val bmp = BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
                    if (bmp != null) imageView.setImageBitmap(bmp)
                }
            }
        }
    }

    private fun CoroutineScope.isActive() = this.coroutineContext[Job]?.isActive ?: false

    override fun onDestroy() {
        super.onDestroy()
        session?.close()
        scope.coroutineContext[Job]?.cancel()
    }
}
