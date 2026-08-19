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
    var selectedResolution by mutableStateOf("1920x1080 (FHD)")
    var customWidth by mutableStateOf("1920")
    var customHeight by mutableStateOf("1080")

    var cursorType by mutableStateOf(0)
    var cursorBitmap: Bitmap? by mutableStateOf(null)
    var cursorHotX by mutableStateOf(0)
    var cursorHotY by mutableStateOf(0)

    fun setResolution(resStr: String) {
        if (resStr.isEmpty()) return
        val knownPresets = listOf(
            "1920x1080 (FHD)",
            "1280x720 (HD)",
            "2560x1440 (2K)",
            "1024x768 (SD)",
            "Native Display"
        )
        if (resStr in knownPresets) {
            selectedResolution = resStr
        } else if (resStr == "Custom" || resStr.startsWith("Custom")) {
            selectedResolution = "Custom"
        } else if (resStr.contains("x")) {
            try {
                val cleanStr = resStr.split(" ")[0]
                val parts = cleanStr.split("x")
                val w = parts[0].trim().toInt()
                val h = parts[1].trim().toInt()
                customWidth = w.toString()
                customHeight = h.toString()
                selectedResolution = "Custom"
            } catch (e: Exception) {
                selectedResolution = "Custom"
            }
        } else {
            selectedResolution = resStr
        }
    }

    fun calculateTargetResolution(metrics: android.util.DisplayMetrics): Pair<Int, Int> {
        val option = selectedResolution
        return when {
            option.contains("Native") -> {
                val rawW = maxOf(metrics.widthPixels, metrics.heightPixels)
                val rawH = minOf(metrics.widthPixels, metrics.heightPixels)
                // Round and align width and height to multiples of 4 for video codec and bitmap stride safety
                val alignedW = ((rawW + 3) / 4) * 4
                val alignedH = ((rawH + 3) / 4) * 4
                Pair(maxOf(alignedW, 640), maxOf(alignedH, 480))
            }
            option == "Custom" || option.startsWith("Custom") -> {
                val w = customWidth.toIntOrNull()?.coerceIn(320, 8192) ?: 1920
                val h = customHeight.toIntOrNull()?.coerceIn(240, 8192) ?: 1080
                Pair(w, h)
            }
            option.contains("x") -> {
                try {
                    val cleanStr = option.split(" ")[0]
                    val parts = cleanStr.split("x")
                    val w = parts[0].trim().toInt()
                    val h = parts[1].trim().toInt()
                    Pair(w, h)
                } catch (e: Exception) {
                    Pair(1920, 1080)
                }
            }
            else -> Pair(1920, 1080)
        }
    }

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
