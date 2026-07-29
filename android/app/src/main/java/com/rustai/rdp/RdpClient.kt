package com.rustai.rdp

import android.util.Log

object RdpClient {
    private const val TAG = "RdpClient"

    var loadError: String? = null

    init {
        try {
            System.loadLibrary("rust_rdp")
            Log.i(TAG, "Rust RDP VNC Native Library Loaded Successfully")
        } catch (e: UnsatisfiedLinkError) {
            loadError = e.message ?: e.toString()
            Log.e(TAG, "Failed to load rust_rdp library: ${e.message}")
        } catch (e: Exception) {
            loadError = e.message ?: e.toString()
            Log.e(TAG, "Failed to load rust_rdp library: ${e.message}")
        }
    }

    interface Callback {
        fun onStateChanged(state: Int, message: String)
        fun onFrameDecoded(pixels: IntArray, x: Int, y: Int, width: Int, height: Int)
        fun onResolutionChanged(width: Int, height: Int)
    }

    // Initialize Rust backend logging and runtime
    @JvmStatic external fun initJni()

    // Connect to RDP Server
    @JvmStatic external fun connect(
        host: String,
        port: Int,
        username: String,
        password: String,
        domain: String,
        width: Int,
        height: Int,
        connectionMode: String,
        callback: Callback
    )

    // Disconnect from RDP Server
    @JvmStatic external fun disconnect()

    // Send mouse input events
    // action: 0 = move, 1 = left click down, 2 = left click up, 3 = right click down, 4 = right click up
    @JvmStatic external fun sendMouseEvent(x: Int, y: Int, action: Int)

    // Send vertical mouse wheel input. Positive units scroll up, negative units scroll down.
    @JvmStatic external fun sendMouseWheelEvent(x: Int, y: Int, units: Int)

    // Send horizontal mouse wheel input. Positive units scroll right, negative units scroll left.
    @JvmStatic external fun sendMouseHorizontalWheelEvent(x: Int, y: Int, units: Int)

    // Send keyboard input events
    // pressed: 1 = pressed, 0 = released
    @JvmStatic external fun sendKeyEvent(keycode: Int, pressed: Int)

    // Send raw keyboard scan code events
    // pressed: 1 = pressed, 0 = released
    // isExtended: true if it's an extended key (like arrows, etc.)
    @JvmStatic external fun sendScancodeEvent(scancode: Int, isExtended: Boolean, pressed: Int)
}
