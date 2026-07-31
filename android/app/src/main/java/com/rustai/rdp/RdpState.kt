package com.rustai.rdp

import android.graphics.Bitmap
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import android.util.Log
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch

enum class ConnectionState {
    IDLE,
    CONNECTING,
    CONNECTED,
    FAILED
}

class RdpViewModel : RdpClient.Callback {
    var connectionState by mutableStateOf(ConnectionState.IDLE)

    var statusMessage by mutableStateOf("Disconnected")

    var screenWidth by mutableStateOf(1920)
    var screenHeight by mutableStateOf(1080)

    var screenBitmap by mutableStateOf<Bitmap?>(null)

    // Trigger state to let Compose know we have a new frame
    var frameTrigger by mutableStateOf(0)

    // Connection parameters as mutable state
    var host by mutableStateOf("")
    var port by mutableStateOf("3389")
    var username by mutableStateOf("")
    var password by mutableStateOf("")
    var domain by mutableStateOf("")
    var connectionMode by mutableStateOf("RDP")

    var cursorType by mutableStateOf(0)
    var cursorBitmap: Bitmap? by mutableStateOf(null)
    var cursorHotX by mutableStateOf(0)
    var cursorHotY by mutableStateOf(0)

    fun initBitmap(width: Int, height: Int) {
        screenWidth = width
        screenHeight = height
        val bmp = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        // Fill initially with charcoal black
        bmp.eraseColor(android.graphics.Color.rgb(20, 20, 25))
        screenBitmap = bmp
        frameTrigger = 0
    }

    override fun onStateChanged(state: Int, message: String) {
        connectionState = when (state) {
            0 -> ConnectionState.IDLE
            1 -> ConnectionState.CONNECTING
            2 -> ConnectionState.CONNECTED
            3 -> ConnectionState.FAILED
            else -> ConnectionState.IDLE
        }
        statusMessage = message
    }

    override fun onFrameDecoded(pixels: IntArray, x: Int, y: Int, width: Int, height: Int) {
        val currentBitmap = screenBitmap ?: return
        if (frameTrigger < 10) {
        }
        try {
            // Ensure bounds check to avoid crashes
            if (x >= 0 && y >= 0 && x + width <= screenWidth && y + height <= screenHeight) {
                currentBitmap.setPixels(pixels, 0, width, x, y, width, height)
                frameTrigger++
            } else {
                Log.w("RdpViewModel", "Frame bounds out of range: rect=($x,$y,$width,$height) bitmap=(${screenWidth}x${screenHeight})")
            }
        } catch (e: Exception) {
            Log.e("RdpViewModel", "Error setting pixels: ${e.message}")
        }
    }

    override fun onResolutionChanged(width: Int, height: Int) {
        CoroutineScope(Dispatchers.Main).launch {
            initBitmap(width, height)
        }
    }

    override fun onCursorChanged(cursorType: Int) {
        if (cursorType == 0 || cursorType == 1) {
            this.cursorBitmap = null
        }
        this.cursorType = cursorType
    }

    override fun onCursorBitmap(width: Int, height: Int, hotX: Int, hotY: Int, pixels: IntArray) {
        try {
            if (width > 0 && height > 0 && pixels.size >= width * height) {
                val bmp = Bitmap.createBitmap(pixels, width, height, Bitmap.Config.ARGB_8888)
                this.cursorBitmap = bmp
                this.cursorHotX = hotX
                this.cursorHotY = hotY
            }
        } catch (e: Exception) {
            Log.e("RdpViewModel", "Error creating cursor bitmap: ${e.message}")
        }
    }
}
