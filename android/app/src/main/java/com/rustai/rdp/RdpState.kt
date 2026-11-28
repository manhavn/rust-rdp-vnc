package com.rustai.rdp

import android.graphics.Bitmap
import android.content.Context
import android.content.SharedPreferences
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import android.util.Log
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import androidx.lifecycle.ViewModel

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

    // Native Rust session id. Each Android tab owns an independent backend session.
    var sessionId by mutableStateOf(0L)
    var connectionGeneration by mutableStateOf(0L)

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
        if (state == 0) {
            sessionId = 0L
        }
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


data class RdpWorkspaceTab(
    val id: Long,
    val viewModel: RdpViewModel
)

data class SavedConnectionProfile(
    val id: Long = 0L,
    val name: String = "",
    val host: String = "",
    val port: String = "3389",
    val username: String = "",
    val password: String = "",
    val domain: String = "",
    val connectionMode: String = "RDP",
    val selectedResolution: String = "1920x1080 (FHD)",
    val customWidth: String = "1920",
    val customHeight: String = "1080"
)

/**
 * Holds the Android multi-tab workspace across Activity configuration changes.
 * Like the desktop client, it restores connection forms and the selected tab,
 * but it does not automatically reconnect remote sessions after process restart.
 */
class RdpWorkspaceViewModel : ViewModel() {
    val tabs = mutableStateListOf<RdpWorkspaceTab>()
    val savedConnections = mutableStateListOf<SavedConnectionProfile>()
    var activeTabIndex by mutableIntStateOf(0)
        private set

    private var nextTabId = 1L
    private var nextProfileId = 1L
    private var initialized = false

    fun initialize(context: Context) {
        if (initialized) return

        val prefs = context.applicationContext.getSharedPreferences("rdp_prefs", Context.MODE_PRIVATE)
        val savedCount = prefs.getInt("workspace_tab_count", 0).coerceIn(0, 64)

        if (savedCount > 0) {
            repeat(savedCount) { index ->
                val vm = RdpViewModel()
                vm.restoreFromPreferences(prefs, "workspace_tab_${index}_")
                tabs.add(RdpWorkspaceTab(nextTabId++, vm))
            }
            activeTabIndex = prefs.getInt("workspace_active_tab", 0)
                .coerceIn(0, tabs.lastIndex)
        } else {
            // Migrate the single connection form saved by older Android versions.
            val vm = RdpViewModel()
            vm.restoreFromPreferences(prefs, "", useLegacyKeys = true)
            tabs.add(RdpWorkspaceTab(nextTabId++, vm))
            activeTabIndex = 0
        }

        if (tabs.isEmpty()) {
            tabs.add(RdpWorkspaceTab(nextTabId++, RdpViewModel()))
            activeTabIndex = 0
        }

        val savedProfileCount = prefs.getInt("saved_connection_count", 0).coerceIn(0, 128)
        repeat(savedProfileCount) { index ->
            val prefix = "saved_connection_${index}_"
            val profile = SavedConnectionProfile(
                id = prefs.getLong(prefix + "id", index.toLong() + 1L),
                name = prefs.getString(prefix + "name", "") ?: "",
                host = prefs.getString(prefix + "host", "") ?: "",
                port = prefs.getString(prefix + "port", "3389") ?: "3389",
                username = prefs.getString(prefix + "username", "") ?: "",
                password = prefs.getString(prefix + "password", "") ?: "",
                domain = prefs.getString(prefix + "domain", "") ?: "",
                connectionMode = prefs.getString(prefix + "connectionMode", "RDP") ?: "RDP",
                selectedResolution = prefs.getString(prefix + "resolution", "1920x1080 (FHD)") ?: "1920x1080 (FHD)",
                customWidth = prefs.getString(prefix + "customWidth", "1920") ?: "1920",
                customHeight = prefs.getString(prefix + "customHeight", "1080") ?: "1080"
            )
            if (profile.host.isNotBlank()) savedConnections.add(profile)
        }
        nextProfileId = (savedConnections.maxOfOrNull { it.id } ?: 0L) + 1L

        initialized = true
        activateCurrentTab()
    }

    fun activeViewModel(): RdpViewModel? = tabs.getOrNull(activeTabIndex)?.viewModel

    fun addTab(context: Context) {
        tabs.add(RdpWorkspaceTab(nextTabId++, RdpViewModel()))
        activeTabIndex = tabs.lastIndex
        activateCurrentTab()
        save(context)
    }

    fun selectTab(context: Context, index: Int) {
        if (index !in tabs.indices || index == activeTabIndex) return
        releaseActiveInput()
        activeTabIndex = index
        activateCurrentTab()
        save(context)
    }

    fun moveTab(context: Context, fromIndex: Int, toIndex: Int) {
        if (fromIndex !in tabs.indices || toIndex !in tabs.indices || fromIndex == toIndex) return
        val activeId = tabs.getOrNull(activeTabIndex)?.id
        val moved = tabs.removeAt(fromIndex)
        tabs.add(toIndex, moved)
        activeTabIndex = tabs.indexOfFirst { it.id == activeId }.coerceAtLeast(0)
        activateCurrentTab()
        save(context)
    }

    fun saveProfile(context: Context, profile: SavedConnectionProfile) {
        if (profile.host.isBlank()) return
        val existingIndex = if (profile.id != 0L) {
            savedConnections.indexOfFirst { it.id == profile.id }
        } else {
            savedConnections.indexOfFirst {
                it.host.equals(profile.host, ignoreCase = true) &&
                    it.port == profile.port && it.connectionMode == profile.connectionMode
            }
        }
        val finalProfile = profile.copy(
            id = if (existingIndex >= 0) savedConnections[existingIndex].id else nextProfileId++,
            name = profile.name.ifBlank { profile.host }
        )
        if (existingIndex >= 0) {
            savedConnections[existingIndex] = finalProfile
        } else {
            savedConnections.add(finalProfile)
        }
        persistSavedConnections(context)
    }

    fun deleteSavedConnection(context: Context, profileId: Long) {
        savedConnections.removeAll { it.id == profileId }
        persistSavedConnections(context)
    }

    private fun persistSavedConnections(context: Context) {
        val prefs = context.applicationContext.getSharedPreferences("rdp_prefs", Context.MODE_PRIVATE)
        val editor = prefs.edit().putInt("saved_connection_count", savedConnections.size)
        savedConnections.forEachIndexed { index, profile ->
            val prefix = "saved_connection_${index}_"
            editor.putLong(prefix + "id", profile.id)
                .putString(prefix + "name", profile.name)
                .putString(prefix + "host", profile.host)
                .putString(prefix + "port", profile.port)
                .putString(prefix + "username", profile.username)
                .putString(prefix + "password", profile.password)
                .putString(prefix + "domain", profile.domain)
                .putString(prefix + "connectionMode", profile.connectionMode)
                .putString(prefix + "resolution", profile.selectedResolution)
                .putString(prefix + "customWidth", profile.customWidth)
                .putString(prefix + "customHeight", profile.customHeight)
        }
        editor.apply()
    }

    fun closeTab(context: Context, index: Int) {
        if (index !in tabs.indices) return

        val closingActive = index == activeTabIndex
        val closingViewModel = tabs[index].viewModel
        closingViewModel.connectionGeneration += 1L
        if (closingActive) releaseActiveInput()
        if (closingViewModel.sessionId != 0L) {
            if (RdpClient.loadError == null) {
                RdpClient.disconnectSession(closingViewModel.sessionId)
            }
            closingViewModel.sessionId = 0L
        }

        tabs.removeAt(index)
        if (tabs.isEmpty()) {
            tabs.add(RdpWorkspaceTab(nextTabId++, RdpViewModel()))
            activeTabIndex = 0
        } else if (index < activeTabIndex) {
            activeTabIndex -= 1
        } else if (activeTabIndex >= tabs.size) {
            activeTabIndex = tabs.lastIndex
        }

        activateCurrentTab()
        save(context)
    }

    fun save(context: Context) {
        if (tabs.isEmpty()) return
        val prefs = context.applicationContext.getSharedPreferences("rdp_prefs", Context.MODE_PRIVATE)
        val editor = prefs.edit()
            .putInt("workspace_tab_count", tabs.size)
            .putInt("workspace_active_tab", activeTabIndex.coerceIn(0, tabs.lastIndex))

        tabs.forEachIndexed { index, tab ->
            tab.viewModel.saveToPreferences(editor, "workspace_tab_${index}_")
        }

        // Keep the last active connection in the legacy keys for compatibility
        // with external intents and older builds that still read these values.
        activeViewModel()?.saveToPreferences(editor, "")
        editor.apply()
    }

    private fun activateCurrentTab() {
        if (RdpClient.loadError == null) {
            RdpClient.setActiveSession(activeViewModel()?.sessionId ?: 0L)
        }
    }

    private fun releaseActiveInput() {
        val sessionId = activeViewModel()?.sessionId ?: return
        if (sessionId == 0L) return

        if (RdpClient.loadError != null) return
        RdpClient.setActiveSession(sessionId)
        // Release common held inputs before switching the native active session.
        RdpClient.sendMouseEvent(0, 0, 2)
        RdpClient.sendScancodeEvent(0x1D, false, 0) // Ctrl
        RdpClient.sendScancodeEvent(0x38, false, 0) // Alt
        RdpClient.sendScancodeEvent(0x2A, false, 0) // Shift
        RdpClient.sendScancodeEvent(0x5B, true, 0)  // Meta / Windows
    }
}

fun RdpViewModel.toSavedConnectionProfile(name: String = host): SavedConnectionProfile =
    SavedConnectionProfile(
        name = name.ifBlank { host },
        host = host,
        port = port,
        username = username,
        password = password,
        domain = domain,
        connectionMode = connectionMode,
        selectedResolution = selectedResolution,
        customWidth = customWidth,
        customHeight = customHeight
    )

fun RdpViewModel.applySavedConnection(profile: SavedConnectionProfile) {
    host = profile.host
    port = profile.port
    username = profile.username
    password = profile.password
    domain = profile.domain
    connectionMode = profile.connectionMode
    selectedResolution = profile.selectedResolution
    customWidth = profile.customWidth
    customHeight = profile.customHeight
}

fun RdpViewModel.restoreFromPreferences(
    prefs: SharedPreferences,
    prefix: String,
    useLegacyKeys: Boolean = false
) {
    fun savedString(key: String, defaultValue: String): String {
        val fallback = if (useLegacyKeys) prefs.getString(key, defaultValue) ?: defaultValue else defaultValue
        return prefs.getString(prefix + key, fallback) ?: fallback
    }

    host = savedString("host", "")
    port = savedString("port", "3389")
    username = savedString("username", "")
    password = savedString("password", "")
    domain = savedString("domain", "")
    connectionMode = savedString("connectionMode", "RDP")
    customWidth = savedString("customWidth", "1920")
    customHeight = savedString("customHeight", "1080")
    setResolution(savedString("resolution", "1920x1080 (FHD)"))
}

fun RdpViewModel.saveToPreferences(editor: SharedPreferences.Editor, prefix: String) {
    editor.putString(prefix + "host", host)
        .putString(prefix + "port", port)
        .putString(prefix + "username", username)
        .putString(prefix + "password", password)
        .putString(prefix + "domain", domain)
        .putString(prefix + "connectionMode", connectionMode)
        .putString(prefix + "resolution", selectedResolution)
        .putString(prefix + "customWidth", customWidth)
        .putString(prefix + "customHeight", customHeight)
}
