package com.rustai.rdp

import android.os.Bundle
import android.view.InputDevice
import android.view.MotionEvent
import android.view.KeyEvent as AndroidKeyEvent
import androidx.activity.ComponentActivity
import androidx.lifecycle.ViewModelProvider
import androidx.activity.compose.setContent
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.material.icons.filled.Menu
import androidx.compose.material.icons.filled.Settings
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectDragGesturesAfterLongPress
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.calculateCentroidSize
import androidx.compose.foundation.gestures.calculatePan
import androidx.compose.foundation.gestures.calculateZoom
import androidx.compose.foundation.gestures.waitForUpOrCancellation
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.drawscope.withTransform
import androidx.compose.ui.input.pointer.PointerInputScope
import kotlin.math.abs
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.clickable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalConfiguration
import android.content.res.Configuration
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.foundation.focusable
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.input.key.*
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.TextRange
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.List
import androidx.compose.material.icons.filled.ArrowForward
import androidx.compose.material.icons.filled.Send
import androidx.compose.material.icons.filled.Menu
import androidx.compose.ui.graphics.Brush

import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.nativeCanvas
import android.graphics.Paint
import android.graphics.PorterDuff
import android.graphics.PorterDuffXfermode
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material.icons.automirrored.filled.List
import androidx.compose.material.icons.filled.ArrowForward
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.pointerInteropFilter
import androidx.compose.ui.input.pointer.positionChange
import androidx.compose.ui.input.pointer.positionChanged
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.material3.Text
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import kotlinx.coroutines.Job
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.coroutines.withContext
import kotlin.math.roundToInt
import android.content.Context
import android.content.Intent
import androidx.compose.ui.platform.LocalContext
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Refresh
import android.net.Uri
import java.io.File
import androidx.activity.result.contract.ActivityResultContracts

enum class MouseMode {
    MOVE, DRAG, CURSOR
}

enum class FullKeyboardMode {
    ROW_SCROLL, ALL_SCROLL, FIT_SCREEN
}

class MainActivity : ComponentActivity() {
    private lateinit var workspace: RdpWorkspaceViewModel

    private fun triggerConnection(
        targetViewModel: RdpViewModel,
        host: String,
        port: String,
        username: String,
        password: String,
        domain: String,
        connectionMode: String
    ) {
        val (targetWidth, targetHeight) = targetViewModel.calculateTargetResolution(resources.displayMetrics)

        if (targetViewModel.sessionId != 0L && RdpClient.loadError == null) {
            RdpClient.disconnectSession(targetViewModel.sessionId)
        }
        targetViewModel.sessionId = 0L
        targetViewModel.connectionGeneration += 1L
        val attemptGeneration = targetViewModel.connectionGeneration
        targetViewModel.onStateChanged(1, "Negotiating $connectionMode Connection...")
        targetViewModel.initBitmap(targetWidth, targetHeight)

        val defaultPort = if (connectionMode == "VNC") 5900 else 3389

        CoroutineScope(Dispatchers.IO).launch {
            try {
                val sessionId = RdpClient.connect(
                    host,
                    port.toIntOrNull() ?: defaultPort,
                    username,
                    password,
                    domain,
                    targetWidth,
                    targetHeight,
                    connectionMode,
                    targetViewModel
                )
                withContext(Dispatchers.Main) {
                    if (targetViewModel.connectionGeneration != attemptGeneration ||
                        workspace.tabs.none { it.viewModel === targetViewModel }
                    ) {
                        RdpClient.disconnectSession(sessionId)
                        if (targetViewModel.connectionGeneration == attemptGeneration) {
                            targetViewModel.sessionId = 0L
                            targetViewModel.onStateChanged(0, "Cancelled")
                        }
                    } else {
                        targetViewModel.sessionId = sessionId
                    }
                    RdpClient.setActiveSession(workspace.activeViewModel()?.sessionId ?: 0L)
                    workspace.save(this@MainActivity)
                }
            } catch (e: Exception) {
                withContext(Dispatchers.Main) {
                    if (targetViewModel.connectionGeneration == attemptGeneration) {
                        targetViewModel.onStateChanged(3, "Failed: ${e.message}")
                    }
                }
            } catch (e: LinkageError) {
                withContext(Dispatchers.Main) {
                    if (targetViewModel.connectionGeneration == attemptGeneration) {
                        targetViewModel.onStateChanged(3, "LinkageError: ${e.message}")
                    }
                }
            }
        }
    }

    private fun handleIntent(intent: Intent?) {
        intent?.let {
            val hostVal = it.getStringExtra("host")
            val portVal = it.getStringExtra("port")
            val usernameVal = it.getStringExtra("username")
            val passwordVal = it.getStringExtra("password")
            val domainVal = it.getStringExtra("domain")
            val connModeVal = it.getStringExtra("connectionMode") ?: "RDP"
            val resolutionVal = it.getStringExtra("resolution")
            val widthVal = it.getStringExtra("width") ?: it.getStringExtra("desktopwidth")
            val heightVal = it.getStringExtra("height") ?: it.getStringExtra("desktopheight")
            val autoconnectVal = it.getBooleanExtra("autoconnect", false) || it.getStringExtra("autoconnect") == "true"

            if (!hostVal.isNullOrEmpty()) {
                var targetViewModel = workspace.activeViewModel() ?: return
                if (targetViewModel.connectionState == ConnectionState.CONNECTED ||
                    targetViewModel.connectionState == ConnectionState.CONNECTING
                ) {
                    workspace.addTab(this)
                    targetViewModel = workspace.activeViewModel() ?: return
                }
                targetViewModel.host = hostVal
                if (portVal != null) targetViewModel.port = portVal
                if (usernameVal != null) targetViewModel.username = usernameVal
                if (passwordVal != null) targetViewModel.password = passwordVal
                if (domainVal != null) targetViewModel.domain = domainVal
                if (widthVal != null) targetViewModel.customWidth = widthVal
                if (heightVal != null) targetViewModel.customHeight = heightVal
                if (resolutionVal != null) {
                    targetViewModel.setResolution(resolutionVal)
                } else if (widthVal != null && heightVal != null) {
                    targetViewModel.selectedResolution = "Custom"
                }
                targetViewModel.connectionMode = connModeVal
                workspace.save(this)

                if (autoconnectVal) {
                    val port = portVal ?: if (connModeVal == "VNC") "5900" else "3389"
                    val username = usernameVal ?: ""
                    val password = passwordVal ?: ""
                    val domain = domainVal ?: ""
                    triggerConnection(targetViewModel, hostVal, port, username, password, domain, connModeVal)
                }
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        handleIntent(intent)
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        // Hide system status bar to make the app fullscreen
        WindowCompat.setDecorFitsSystemWindows(window, false)
        val insetsController = WindowCompat.getInsetsController(window, window.decorView)
        insetsController.hide(WindowInsetsCompat.Type.statusBars())
        insetsController.systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE

        workspace = ViewModelProvider(this)[RdpWorkspaceViewModel::class.java]

        // Initialize JNI Rust Logging and tokio runtime
        try {
            if (RdpClient.loadError != null) {
            }
            RdpClient.initJni()
        } catch (e: Exception) {
        } catch (e: LinkageError) {
        }

        workspace.initialize(this)

        if (savedInstanceState == null) {
            handleIntent(intent)
        }

        setContent {
            RdpAppTheme {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = MaterialTheme.colorScheme.background
                ) {
                    RdpAppScreen(workspace) { targetViewModel ->
                        triggerConnection(
                            targetViewModel,
                            targetViewModel.host,
                            targetViewModel.port,
                            targetViewModel.username,
                            targetViewModel.password,
                            targetViewModel.domain,
                            targetViewModel.connectionMode
                        )
                    }
                }
            }
        }
    }

    override fun onStop() {
        workspace.save(this)
        super.onStop()
    }

    override fun onDestroy() {
        if (isFinishing) {
            RdpClient.disconnect()
        }
        super.onDestroy()
    }
}

private data class RdpPhysicalKey(val scanCode: Int, val extended: Boolean = false)

/** Maps Android HID key events to PC/AT set-1 scan codes understood by the Rust backend. */
private fun mapAndroidKeyToRdp(keyCode: Int): RdpPhysicalKey? = when (keyCode) {
    AndroidKeyEvent.KEYCODE_A -> RdpPhysicalKey(0x1E)
    AndroidKeyEvent.KEYCODE_B -> RdpPhysicalKey(0x30)
    AndroidKeyEvent.KEYCODE_C -> RdpPhysicalKey(0x2E)
    AndroidKeyEvent.KEYCODE_D -> RdpPhysicalKey(0x20)
    AndroidKeyEvent.KEYCODE_E -> RdpPhysicalKey(0x12)
    AndroidKeyEvent.KEYCODE_F -> RdpPhysicalKey(0x21)
    AndroidKeyEvent.KEYCODE_G -> RdpPhysicalKey(0x22)
    AndroidKeyEvent.KEYCODE_H -> RdpPhysicalKey(0x23)
    AndroidKeyEvent.KEYCODE_I -> RdpPhysicalKey(0x17)
    AndroidKeyEvent.KEYCODE_J -> RdpPhysicalKey(0x24)
    AndroidKeyEvent.KEYCODE_K -> RdpPhysicalKey(0x25)
    AndroidKeyEvent.KEYCODE_L -> RdpPhysicalKey(0x26)
    AndroidKeyEvent.KEYCODE_M -> RdpPhysicalKey(0x32)
    AndroidKeyEvent.KEYCODE_N -> RdpPhysicalKey(0x31)
    AndroidKeyEvent.KEYCODE_O -> RdpPhysicalKey(0x18)
    AndroidKeyEvent.KEYCODE_P -> RdpPhysicalKey(0x19)
    AndroidKeyEvent.KEYCODE_Q -> RdpPhysicalKey(0x10)
    AndroidKeyEvent.KEYCODE_R -> RdpPhysicalKey(0x13)
    AndroidKeyEvent.KEYCODE_S -> RdpPhysicalKey(0x1F)
    AndroidKeyEvent.KEYCODE_T -> RdpPhysicalKey(0x14)
    AndroidKeyEvent.KEYCODE_U -> RdpPhysicalKey(0x16)
    AndroidKeyEvent.KEYCODE_V -> RdpPhysicalKey(0x2F)
    AndroidKeyEvent.KEYCODE_W -> RdpPhysicalKey(0x11)
    AndroidKeyEvent.KEYCODE_X -> RdpPhysicalKey(0x2D)
    AndroidKeyEvent.KEYCODE_Y -> RdpPhysicalKey(0x15)
    AndroidKeyEvent.KEYCODE_Z -> RdpPhysicalKey(0x2C)
    AndroidKeyEvent.KEYCODE_1 -> RdpPhysicalKey(0x02)
    AndroidKeyEvent.KEYCODE_2 -> RdpPhysicalKey(0x03)
    AndroidKeyEvent.KEYCODE_3 -> RdpPhysicalKey(0x04)
    AndroidKeyEvent.KEYCODE_4 -> RdpPhysicalKey(0x05)
    AndroidKeyEvent.KEYCODE_5 -> RdpPhysicalKey(0x06)
    AndroidKeyEvent.KEYCODE_6 -> RdpPhysicalKey(0x07)
    AndroidKeyEvent.KEYCODE_7 -> RdpPhysicalKey(0x08)
    AndroidKeyEvent.KEYCODE_8 -> RdpPhysicalKey(0x09)
    AndroidKeyEvent.KEYCODE_9 -> RdpPhysicalKey(0x0A)
    AndroidKeyEvent.KEYCODE_0 -> RdpPhysicalKey(0x0B)
    AndroidKeyEvent.KEYCODE_GRAVE -> RdpPhysicalKey(0x29)
    AndroidKeyEvent.KEYCODE_MINUS -> RdpPhysicalKey(0x0C)
    AndroidKeyEvent.KEYCODE_EQUALS -> RdpPhysicalKey(0x0D)
    AndroidKeyEvent.KEYCODE_LEFT_BRACKET -> RdpPhysicalKey(0x1A)
    AndroidKeyEvent.KEYCODE_RIGHT_BRACKET -> RdpPhysicalKey(0x1B)
    AndroidKeyEvent.KEYCODE_BACKSLASH -> RdpPhysicalKey(0x2B)
    AndroidKeyEvent.KEYCODE_SEMICOLON -> RdpPhysicalKey(0x27)
    AndroidKeyEvent.KEYCODE_APOSTROPHE -> RdpPhysicalKey(0x28)
    AndroidKeyEvent.KEYCODE_COMMA -> RdpPhysicalKey(0x33)
    AndroidKeyEvent.KEYCODE_PERIOD -> RdpPhysicalKey(0x34)
    AndroidKeyEvent.KEYCODE_SLASH -> RdpPhysicalKey(0x35)
    AndroidKeyEvent.KEYCODE_SPACE -> RdpPhysicalKey(0x39)
    AndroidKeyEvent.KEYCODE_ENTER -> RdpPhysicalKey(0x1C)
    AndroidKeyEvent.KEYCODE_NUMPAD_ENTER -> RdpPhysicalKey(0x1C, true)
    AndroidKeyEvent.KEYCODE_TAB -> RdpPhysicalKey(0x0F)
    AndroidKeyEvent.KEYCODE_DEL -> RdpPhysicalKey(0x0E)
    AndroidKeyEvent.KEYCODE_FORWARD_DEL -> RdpPhysicalKey(0x53, true)
    AndroidKeyEvent.KEYCODE_ESCAPE -> RdpPhysicalKey(0x01)
    AndroidKeyEvent.KEYCODE_CAPS_LOCK -> RdpPhysicalKey(0x3A)
    AndroidKeyEvent.KEYCODE_SCROLL_LOCK -> RdpPhysicalKey(0x46)
    AndroidKeyEvent.KEYCODE_NUM_LOCK -> RdpPhysicalKey(0x45)
    AndroidKeyEvent.KEYCODE_SYSRQ -> RdpPhysicalKey(0x37, true)
    AndroidKeyEvent.KEYCODE_INSERT -> RdpPhysicalKey(0x52, true)
    AndroidKeyEvent.KEYCODE_MOVE_HOME -> RdpPhysicalKey(0x47, true)
    AndroidKeyEvent.KEYCODE_MOVE_END -> RdpPhysicalKey(0x4F, true)
    AndroidKeyEvent.KEYCODE_PAGE_UP -> RdpPhysicalKey(0x49, true)
    AndroidKeyEvent.KEYCODE_PAGE_DOWN -> RdpPhysicalKey(0x51, true)
    AndroidKeyEvent.KEYCODE_DPAD_UP -> RdpPhysicalKey(0x48, true)
    AndroidKeyEvent.KEYCODE_DPAD_DOWN -> RdpPhysicalKey(0x50, true)
    AndroidKeyEvent.KEYCODE_DPAD_LEFT -> RdpPhysicalKey(0x4B, true)
    AndroidKeyEvent.KEYCODE_DPAD_RIGHT -> RdpPhysicalKey(0x4D, true)
    AndroidKeyEvent.KEYCODE_SHIFT_LEFT -> RdpPhysicalKey(0x2A)
    AndroidKeyEvent.KEYCODE_SHIFT_RIGHT -> RdpPhysicalKey(0x36)
    AndroidKeyEvent.KEYCODE_CTRL_LEFT -> RdpPhysicalKey(0x1D)
    AndroidKeyEvent.KEYCODE_CTRL_RIGHT -> RdpPhysicalKey(0x1D, true)
    AndroidKeyEvent.KEYCODE_ALT_LEFT -> RdpPhysicalKey(0x38)
    AndroidKeyEvent.KEYCODE_ALT_RIGHT -> RdpPhysicalKey(0x38, true)
    AndroidKeyEvent.KEYCODE_META_LEFT -> RdpPhysicalKey(0x5B, true)
    AndroidKeyEvent.KEYCODE_META_RIGHT -> RdpPhysicalKey(0x5C, true)
    AndroidKeyEvent.KEYCODE_F1 -> RdpPhysicalKey(0x3B)
    AndroidKeyEvent.KEYCODE_F2 -> RdpPhysicalKey(0x3C)
    AndroidKeyEvent.KEYCODE_F3 -> RdpPhysicalKey(0x3D)
    AndroidKeyEvent.KEYCODE_F4 -> RdpPhysicalKey(0x3E)
    AndroidKeyEvent.KEYCODE_F5 -> RdpPhysicalKey(0x3F)
    AndroidKeyEvent.KEYCODE_F6 -> RdpPhysicalKey(0x40)
    AndroidKeyEvent.KEYCODE_F7 -> RdpPhysicalKey(0x41)
    AndroidKeyEvent.KEYCODE_F8 -> RdpPhysicalKey(0x42)
    AndroidKeyEvent.KEYCODE_F9 -> RdpPhysicalKey(0x43)
    AndroidKeyEvent.KEYCODE_F10 -> RdpPhysicalKey(0x44)
    AndroidKeyEvent.KEYCODE_F11 -> RdpPhysicalKey(0x57)
    AndroidKeyEvent.KEYCODE_F12 -> RdpPhysicalKey(0x58)
    AndroidKeyEvent.KEYCODE_NUMPAD_0 -> RdpPhysicalKey(0x52)
    AndroidKeyEvent.KEYCODE_NUMPAD_1 -> RdpPhysicalKey(0x4F)
    AndroidKeyEvent.KEYCODE_NUMPAD_2 -> RdpPhysicalKey(0x50)
    AndroidKeyEvent.KEYCODE_NUMPAD_3 -> RdpPhysicalKey(0x51)
    AndroidKeyEvent.KEYCODE_NUMPAD_4 -> RdpPhysicalKey(0x4B)
    AndroidKeyEvent.KEYCODE_NUMPAD_5 -> RdpPhysicalKey(0x4C)
    AndroidKeyEvent.KEYCODE_NUMPAD_6 -> RdpPhysicalKey(0x4D)
    AndroidKeyEvent.KEYCODE_NUMPAD_7 -> RdpPhysicalKey(0x47)
    AndroidKeyEvent.KEYCODE_NUMPAD_8 -> RdpPhysicalKey(0x48)
    AndroidKeyEvent.KEYCODE_NUMPAD_9 -> RdpPhysicalKey(0x49)
    AndroidKeyEvent.KEYCODE_NUMPAD_DIVIDE -> RdpPhysicalKey(0x35, true)
    AndroidKeyEvent.KEYCODE_NUMPAD_MULTIPLY -> RdpPhysicalKey(0x37)
    AndroidKeyEvent.KEYCODE_NUMPAD_SUBTRACT -> RdpPhysicalKey(0x4A)
    AndroidKeyEvent.KEYCODE_NUMPAD_ADD -> RdpPhysicalKey(0x4E)
    AndroidKeyEvent.KEYCODE_NUMPAD_DOT -> RdpPhysicalKey(0x53)
    else -> null
}

@Composable
fun RdpAppTheme(content: @Composable () -> Unit) {
    val darkColors = darkColorScheme(
        primary = Color(0xFF66FCF1),
        secondary = Color(0xFF45A29E),
        background = Color(0xFF0F1219),
        surface = Color(0xFF1B2230),
        onPrimary = Color(0xFF0F1219),
        onSecondary = Color(0xFFFFFFFF),
        onBackground = Color(0xFFE2E8F0),
        onSurface = Color(0xFFCBD5E1),
        primaryContainer = Color(0xFF1E293B),
        error = Color(0xFFEF4444)
    )
    MaterialTheme(
        colorScheme = darkColors,
        content = content
    )
}

@Composable
fun RdpAppScreen(
    workspace: RdpWorkspaceViewModel,
    onConnect: (RdpViewModel) -> Unit
) {
    val context = LocalContext.current
    val configuration = LocalConfiguration.current
    val isTabletLayout = configuration.screenWidthDp >= 600
    val activeIndex = workspace.activeTabIndex
    val activeViewModel = workspace.activeViewModel()
    var sidebarVisible by remember(isTabletLayout) { mutableStateOf(isTabletLayout) }

    if (activeViewModel == null) return

    LaunchedEffect(
        activeIndex,
        activeViewModel.host,
        activeViewModel.port,
        activeViewModel.username,
        activeViewModel.password,
        activeViewModel.domain,
        activeViewModel.connectionMode,
        activeViewModel.selectedResolution,
        activeViewModel.customWidth,
        activeViewModel.customHeight
    ) {
        delay(300)
        workspace.save(context)
    }

    Row(
        modifier = Modifier
            .fillMaxSize()
            .background(Color(0xFF0F1219))
    ) {
        if (isTabletLayout && sidebarVisible) {
            ConnectionsSidebar(
                workspace = workspace,
                activeViewModel = activeViewModel,
                onConnect = onConnect
            )
            Box(
                modifier = Modifier
                    .fillMaxHeight()
                    .width(1.dp)
                    .background(Color(0xFF334155))
            )
        }

        Column(
            modifier = Modifier
                .weight(1f)
                .fillMaxHeight()
        ) {
            WorkspaceTabBar(
                workspace = workspace,
                isTabletLayout = isTabletLayout,
                onToggleSidebar = { sidebarVisible = !sidebarVisible }
            )
            Box(modifier = Modifier.weight(1f).fillMaxWidth()) {
                when (activeViewModel.connectionState) {
                    ConnectionState.IDLE, ConnectionState.FAILED -> {
                        ConnectionDashboard(activeViewModel) { onConnect(activeViewModel) }
                    }
                    ConnectionState.CONNECTING -> {
                        ConnectingScreen(activeViewModel)
                    }
                    ConnectionState.CONNECTED -> {
                        RdpSessionScreen(activeViewModel)
                    }
                }
            }
        }
    }
}

@Composable
private fun ConnectionsSidebar(
    workspace: RdpWorkspaceViewModel,
    activeViewModel: RdpViewModel,
    onConnect: (RdpViewModel) -> Unit
) {
    val context = LocalContext.current
    var searchQuery by remember { mutableStateOf("") }
    var editingProfile by remember { mutableStateOf<SavedConnectionProfile?>(null) }
    val filteredProfiles = workspace.savedConnections.filter { profile ->
        searchQuery.isBlank() ||
            profile.name.contains(searchQuery, ignoreCase = true) ||
            profile.host.contains(searchQuery, ignoreCase = true)
    }

    Column(
        modifier = Modifier
            .width(248.dp)
            .fillMaxHeight()
            .background(Color(0xFF111827))
            .padding(horizontal = 12.dp, vertical = 14.dp)
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween
        ) {
            Column(modifier = Modifier.weight(1f)) {
                Text("Connections", color = Color.White, fontSize = 18.sp, fontWeight = FontWeight.Bold)
                Text("Saved servers", color = Color(0xFF94A3B8), fontSize = 11.sp)
            }
            IconButton(
                onClick = { editingProfile = SavedConnectionProfile() },
                modifier = Modifier.size(38.dp).background(Color(0xFF263548), RoundedCornerShape(9.dp))
            ) {
                Icon(Icons.Default.Add, contentDescription = "Add saved connection", tint = Color(0xFF66FCF1))
            }
        }

        Spacer(Modifier.height(12.dp))
        OutlinedTextField(
            value = searchQuery,
            onValueChange = { searchQuery = it },
            modifier = Modifier.fillMaxWidth(),
            singleLine = true,
            placeholder = { Text("Search servers", color = Color(0xFF64748B), fontSize = 13.sp) },
            leadingIcon = { Text("⌕", color = Color(0xFF94A3B8), fontSize = 20.sp) },
            colors = OutlinedTextFieldDefaults.colors(
                focusedBorderColor = Color(0xFF66FCF1),
                unfocusedBorderColor = Color(0xFF334155),
                focusedTextColor = Color.White,
                unfocusedTextColor = Color.White,
                cursorColor = Color(0xFF66FCF1)
            )
        )

        Spacer(Modifier.height(10.dp))
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween
        ) {
            Text("SAVED (${workspace.savedConnections.size})", color = Color(0xFF94A3B8), fontSize = 10.sp, fontWeight = FontWeight.Bold)
            TextButton(
                onClick = {
                    editingProfile = activeViewModel.toSavedConnectionProfile(
                        activeViewModel.host.ifBlank { "New server" }
                    )
                },
                enabled = activeViewModel.host.isNotBlank(),
                contentPadding = PaddingValues(horizontal = 4.dp, vertical = 0.dp)
            ) {
                Text("Save current", color = if (activeViewModel.host.isNotBlank()) Color(0xFF66FCF1) else Color(0xFF64748B), fontSize = 11.sp)
            }
        }

        Column(
            modifier = Modifier
                .weight(1f)
                .fillMaxWidth()
                .verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(8.dp)
        ) {
            if (filteredProfiles.isEmpty()) {
                Text(
                    if (workspace.savedConnections.isEmpty()) "No saved connections yet. Add a server or save the current connection." else "No matching servers.",
                    color = Color(0xFF64748B),
                    fontSize = 12.sp,
                    modifier = Modifier.padding(vertical = 16.dp)
                )
            }

            filteredProfiles.forEach { profile ->
                Card(
                    modifier = Modifier.fillMaxWidth(),
                    colors = CardDefaults.cardColors(containerColor = Color(0xFF1B2230)),
                    border = BorderStroke(1.dp, Color(0xFF263548))
                ) {
                    Column(modifier = Modifier.fillMaxWidth().padding(10.dp)) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Box(
                                modifier = Modifier.size(8.dp).background(
                                    if (profile.connectionMode == "VNC") Color(0xFFF59E0B) else Color(0xFF38BDF8),
                                    CircleShape
                                )
                            )
                            Spacer(Modifier.width(8.dp))
                            Column(modifier = Modifier.weight(1f)) {
                                Text(profile.name, color = Color.White, fontSize = 13.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                                Text("${profile.host}:${profile.port} · ${profile.connectionMode}", color = Color(0xFF94A3B8), fontSize = 10.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            }
                        }
                        Spacer(Modifier.height(6.dp))
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(2.dp)
                        ) {
                            TextButton(
                                onClick = {
                                    var target = workspace.activeViewModel() ?: activeViewModel
                                    if (target.connectionState == ConnectionState.CONNECTED || target.connectionState == ConnectionState.CONNECTING) {
                                        workspace.addTab(context)
                                        target = workspace.activeViewModel() ?: target
                                    }
                                    target.applySavedConnection(profile)
                                    workspace.save(context)
                                    onConnect(target)
                                },
                                contentPadding = PaddingValues(horizontal = 7.dp, vertical = 0.dp)
                            ) { Text("Connect", color = Color(0xFF66FCF1), fontSize = 11.sp) }
                            TextButton(
                                onClick = { editingProfile = profile },
                                contentPadding = PaddingValues(horizontal = 7.dp, vertical = 0.dp)
                            ) { Text("Edit", color = Color(0xFFCBD5E1), fontSize = 11.sp) }
                            TextButton(
                                onClick = { workspace.deleteSavedConnection(context, profile.id) },
                                contentPadding = PaddingValues(horizontal = 7.dp, vertical = 0.dp)
                            ) { Text("Delete", color = Color(0xFFF87171), fontSize = 11.sp) }
                        }
                    }
                }
            }
        }

        Text(
            "Hold a tab and drag it to reorder.",
            color = Color(0xFF64748B),
            fontSize = 10.sp,
            modifier = Modifier.padding(top = 10.dp)
        )
    }

    editingProfile?.let { profile ->
        SavedConnectionEditor(
            profile = profile,
            onDismiss = { editingProfile = null },
            onSave = { saved ->
                workspace.saveProfile(context, saved)
                editingProfile = null
            }
        )
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun SavedConnectionEditor(
    profile: SavedConnectionProfile,
    onDismiss: () -> Unit,
    onSave: (SavedConnectionProfile) -> Unit
) {
    var name by remember(profile.id, profile.name, profile.host) { mutableStateOf(profile.name) }
    var host by remember(profile.id, profile.name, profile.host) { mutableStateOf(profile.host) }
    var port by remember(profile.id, profile.name, profile.host) { mutableStateOf(profile.port) }
    var username by remember(profile.id, profile.name, profile.host) { mutableStateOf(profile.username) }
    var password by remember(profile.id, profile.name, profile.host) { mutableStateOf(profile.password) }
    var domain by remember(profile.id, profile.name, profile.host) { mutableStateOf(profile.domain) }
    var mode by remember(profile.id, profile.name, profile.host) { mutableStateOf(profile.connectionMode) }
    val scrollState = rememberScrollState()

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(if (profile.id == 0L) "Add saved connection" else "Edit saved connection") },
        text = {
            Column(
                modifier = Modifier.heightIn(max = 500.dp).verticalScroll(scrollState),
                verticalArrangement = Arrangement.spacedBy(8.dp)
            ) {
                OutlinedTextField(name, { name = it }, label = { Text("Display name") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                OutlinedTextField(host, { host = it }, label = { Text("Host / IP address") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                OutlinedTextField(port, { port = it.filter(Char::isDigit).take(5) }, label = { Text("Port") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    FilterChip(
                        selected = mode == "RDP",
                        onClick = {
                            if (mode != "RDP" && port == "5900") port = "3389"
                            mode = "RDP"
                        },
                        label = { Text("RDP") }
                    )
                    FilterChip(
                        selected = mode == "VNC",
                        onClick = {
                            if (mode != "VNC" && port == "3389") port = "5900"
                            mode = "VNC"
                        },
                        label = { Text("VNC") }
                    )
                }
                OutlinedTextField(username, { username = it }, label = { Text("Username") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                OutlinedTextField(password, { password = it }, label = { Text("Password") }, singleLine = true, visualTransformation = PasswordVisualTransformation(), modifier = Modifier.fillMaxWidth())
                OutlinedTextField(domain, { domain = it }, label = { Text("Domain (optional)") }, singleLine = true, modifier = Modifier.fillMaxWidth())
            }
        },
        confirmButton = {
            TextButton(
                enabled = host.isNotBlank() && (port.toIntOrNull()?.let { it in 1..65535 } == true),
                onClick = {
                    onSave(
                        profile.copy(
                            name = name.ifBlank { host },
                            host = host.trim(),
                            port = port,
                            username = username,
                            password = password,
                            domain = domain,
                            connectionMode = mode
                        )
                    )
                }
            ) { Text("Save") }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
        containerColor = Color(0xFF1B2230),
        titleContentColor = Color.White,
        textContentColor = Color(0xFFCBD5E1)
    )
}

@Composable
private fun WorkspaceTabBar(
    workspace: RdpWorkspaceViewModel,
    isTabletLayout: Boolean,
    onToggleSidebar: () -> Unit
) {
    val context = LocalContext.current
    val density = LocalDensity.current
    var pendingCloseIndex by remember { mutableStateOf<Int?>(null) }
    val tabHeight = if (isTabletLayout) 64.dp else 60.dp
    val tabWidth = if (isTabletLayout) 190.dp else 150.dp
    val dragStepPx = with(density) { (tabWidth + 6.dp).toPx() * 0.5f }

    Row(
        modifier = Modifier
            .fillMaxWidth()
            .height(tabHeight)
            .background(Color(0xFF111827))
            .padding(horizontal = if (isTabletLayout) 12.dp else 6.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        if (isTabletLayout) {
            IconButton(
                onClick = onToggleSidebar,
                modifier = Modifier.size(36.dp).clip(RoundedCornerShape(8.dp)).background(Color(0xFF263548))
            ) {
                Icon(Icons.Default.Menu, contentDescription = "Toggle connections sidebar", tint = Color(0xFFCBD5E1))
            }
            Spacer(Modifier.width(8.dp))
        }

        Row(
            modifier = Modifier
                .weight(1f)
                .fillMaxHeight()
                .horizontalScroll(rememberScrollState()),
            verticalAlignment = Alignment.CenterVertically
        ) {
            workspace.tabs.forEachIndexed { index, tab ->
                val viewModel = tab.viewModel
                val selected = index == workspace.activeTabIndex
                val statusColor = when (viewModel.connectionState) {
                    ConnectionState.CONNECTED -> Color(0xFF10B981)
                    ConnectionState.CONNECTING -> Color(0xFFF59E0B)
                    ConnectionState.FAILED -> Color(0xFFEF4444)
                    ConnectionState.IDLE -> Color(0xFF64748B)
                }
                val statusLabel = when (viewModel.connectionState) {
                    ConnectionState.CONNECTED -> "Connected"
                    ConnectionState.CONNECTING -> "Connecting"
                    ConnectionState.FAILED -> "Failed"
                    ConnectionState.IDLE -> "Disconnected"
                }

                Row(
                    modifier = Modifier
                        .padding(end = 6.dp)
                        .width(tabWidth)
                        .fillMaxHeight()
                        .clip(RoundedCornerShape(9.dp))
                        .background(if (selected) Color(0xFF263548) else Color(0xFF1B2230))
                        .border(
                            width = if (selected) 1.dp else 0.dp,
                            color = if (selected) Color(0xFF66FCF1).copy(alpha = 0.65f) else Color.Transparent,
                            shape = RoundedCornerShape(9.dp)
                        )
                        .pointerInput(tab.id, workspace.tabs.size) {
                            var currentIndex = workspace.tabs.indexOfFirst { it.id == tab.id }
                            var accumulatedDrag = 0f
                            detectDragGesturesAfterLongPress(
                                onDragStart = {
                                    currentIndex = workspace.tabs.indexOfFirst { it.id == tab.id }
                                    accumulatedDrag = 0f
                                },
                                onDrag = { change, dragAmount ->
                                    accumulatedDrag += dragAmount.x
                                    while (abs(accumulatedDrag) >= dragStepPx && currentIndex >= 0) {
                                        val direction = if (accumulatedDrag > 0f) 1 else -1
                                        val targetIndex = (currentIndex + direction).coerceIn(0, workspace.tabs.lastIndex)
                                        if (targetIndex == currentIndex) {
                                            accumulatedDrag = 0f
                                            break
                                        }
                                        workspace.moveTab(context, currentIndex, targetIndex)
                                        currentIndex = targetIndex
                                        accumulatedDrag -= direction * dragStepPx
                                    }
                                    change.consume()
                                }
                            )
                        }
                        .clickable { workspace.selectTab(context, index) }
                        .padding(start = 10.dp, end = 4.dp),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Box(modifier = Modifier.size(8.dp).background(statusColor, CircleShape))
                    Spacer(modifier = Modifier.width(8.dp))
                    Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.Center) {
                        Text(
                            text = viewModel.host.ifBlank { "Tab ${index + 1}" },
                            color = if (selected) Color.White else Color(0xFFCBD5E1),
                            fontSize = if (isTabletLayout) 13.sp else 12.sp,
                            fontWeight = if (selected) FontWeight.SemiBold else FontWeight.Medium,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis
                        )
                        Text(
                            text = statusLabel,
                            color = if (selected) Color(0xFF94A3B8) else Color(0xFF64748B),
                            fontSize = 10.sp,
                            maxLines = 1
                        )
                    }
                    IconButton(
                        onClick = {
                            if (viewModel.connectionState == ConnectionState.CONNECTED ||
                                viewModel.connectionState == ConnectionState.CONNECTING
                            ) {
                                pendingCloseIndex = workspace.tabs.indexOfFirst { it.id == tab.id }
                            } else {
                                workspace.closeTab(context, workspace.tabs.indexOfFirst { it.id == tab.id })
                            }
                        },
                        modifier = Modifier.size(30.dp)
                    ) {
                        Icon(
                            imageVector = Icons.Default.Close,
                            contentDescription = "Close tab",
                            tint = if (selected) Color(0xFFCBD5E1) else Color(0xFF64748B),
                            modifier = Modifier.size(15.dp)
                        )
                    }
                }
            }
        }

        Spacer(modifier = Modifier.width(6.dp))
        IconButton(
            onClick = { workspace.addTab(context) },
            modifier = Modifier
                .size(if (isTabletLayout) 42.dp else 36.dp)
                .clip(RoundedCornerShape(9.dp))
                .background(Color(0xFF263548))
        ) {
            Icon(Icons.Default.Add, contentDescription = "New connection tab", tint = Color(0xFF66FCF1))
        }
    }

    val closeIndex = pendingCloseIndex
    if (closeIndex != null && closeIndex in workspace.tabs.indices) {
        val closingViewModel = workspace.tabs[closeIndex].viewModel
        AlertDialog(
            onDismissRequest = { pendingCloseIndex = null },
            title = { Text("Close connection tab?") },
            text = {
                Text(
                    if (closingViewModel.connectionState == ConnectionState.CONNECTING) {
                        "This connection is still starting. Closing the tab will cancel it."
                    } else {
                        "This will disconnect ${closingViewModel.host.ifBlank { "this session" }}."
                    }
                )
            },
            confirmButton = {
                TextButton(
                    onClick = {
                        workspace.closeTab(context, closeIndex)
                        pendingCloseIndex = null
                    }
                ) { Text("Close") }
            },
            dismissButton = { TextButton(onClick = { pendingCloseIndex = null }) { Text("Cancel") } },
            containerColor = Color(0xFF1B2230),
            titleContentColor = Color.White,
            textContentColor = Color(0xFFCBD5E1)
        )
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ConnectionDashboard(viewModel: RdpViewModel, onConnect: () -> Unit) {
    val context = LocalContext.current
    val isTabletLayout = LocalConfiguration.current.screenWidthDp >= 600
    val launcher = androidx.activity.compose.rememberLauncherForActivityResult(
        ActivityResultContracts.GetContent()
    ) { uri: Uri? ->
        if (uri != null) {
            try {
                val inputStream = context.contentResolver.openInputStream(uri)
                val lines = inputStream?.bufferedReader()?.readLines() ?: emptyList()
                var rdpHost = ""
                var rdpUser = ""
                var rdpPass = ""
                var rdpDomain = ""
                var detectedMode = "RDP"
                var fileWidth = ""
                var fileHeight = ""
                var fileRes = ""
                for (line in lines) {
                    if (line.startsWith("full address:s:")) {
                        rdpHost = line.substringAfter("full address:s:")
                    } else if (line.startsWith("username:s:")) {
                        rdpUser = line.substringAfter("username:s:")
                    } else if (line.startsWith("password:s:")) {
                        rdpPass = line.substringAfter("password:s:")
                    } else if (line.startsWith("domain:s:")) {
                        rdpDomain = line.substringAfter("domain:s:")
                    } else if (line.startsWith("connection mode:s:")) {
                        detectedMode = line.substringAfter("connection mode:s:")
                    } else if (line.startsWith("mode:s:")) {
                        detectedMode = line.substringAfter("mode:s:")
                    } else if (line.startsWith("desktopwidth:i:")) {
                        fileWidth = line.substringAfter("desktopwidth:i:")
                    } else if (line.startsWith("desktopheight:i:")) {
                        fileHeight = line.substringAfter("desktopheight:i:")
                    } else if (line.startsWith("resolution:s:")) {
                        fileRes = line.substringAfter("resolution:s:")
                    }
                }
                
                // Try to detect by file extension from Uri
                var isVncFromFile = false
                val cursor = context.contentResolver.query(uri, null, null, null, null)
                cursor?.use { c ->
                    val nameIndex = c.getColumnIndex(android.provider.OpenableColumns.DISPLAY_NAME)
                    if (nameIndex != -1 && c.moveToFirst()) {
                        val displayName = c.getString(nameIndex)
                        if (displayName != null && displayName.endsWith(".vnc", ignoreCase = true)) {
                            isVncFromFile = true
                        }
                    }
                }
                if (isVncFromFile) {
                    detectedMode = "VNC"
                }

                if (rdpHost.isNotEmpty()) {
                    val parts = rdpHost.split(":")
                    val h = parts[0]
                    val defaultPort = if (detectedMode == "VNC") "5900" else "3389"
                    val p = if (parts.size > 1) parts[1] else defaultPort

                    viewModel.host = h
                    viewModel.port = p
                    viewModel.username = rdpUser
                    viewModel.password = rdpPass
                    viewModel.domain = rdpDomain
                    viewModel.connectionMode = detectedMode
                    if (fileRes.isNotEmpty()) {
                        viewModel.setResolution(fileRes)
                    } else if (fileWidth.isNotEmpty() && fileHeight.isNotEmpty()) {
                        viewModel.customWidth = fileWidth
                        viewModel.customHeight = fileHeight
                        viewModel.selectedResolution = "Custom"
                    }
                    
                    // Auto connect the imported profile in this tab.
                    onConnect()
                }
            } catch (e: Exception) {
            }
        }
    }

    val saveLauncher = androidx.activity.compose.rememberLauncherForActivityResult(
        ActivityResultContracts.CreateDocument("application/octet-stream")
    ) { uri: Uri? ->
        if (uri != null) {
            try {
                context.contentResolver.openOutputStream(uri)?.use { outputStream ->
                    outputStream.bufferedWriter().use { writer ->
                        writer.write("full address:s:${viewModel.host}:${viewModel.port}\n")
                        if (viewModel.username.isNotEmpty()) {
                            writer.write("username:s:${viewModel.username}\n")
                        }
                        if (viewModel.password.isNotEmpty()) {
                            writer.write("password:s:${viewModel.password}\n")
                        }
                        if (viewModel.domain.isNotEmpty()) {
                            writer.write("domain:s:${viewModel.domain}\n")
                        }
                        writer.write("connection mode:s:${viewModel.connectionMode}\n")
                        writer.write("resolution:s:${viewModel.selectedResolution}\n")
                        val (rw, rh) = viewModel.calculateTargetResolution(context.resources.displayMetrics)
                        writer.write("desktopwidth:i:${rw}\n")
                        writer.write("desktopheight:i:${rh}\n")
                    }
                }
            } catch (e: Exception) {
            }
        }
    }

    val scrollState = rememberScrollState()

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(
                Brush.verticalGradient(
                    colors = listOf(Color(0xFF0F1219), Color(0xFF1B2230))
                )
            ),
        contentAlignment = Alignment.Center
    ) {
        Column(
            modifier = Modifier
                .widthIn(max = if (isTabletLayout) 760.dp else 480.dp)
                .fillMaxWidth(if (isTabletLayout) 0.82f else 0.9f)
                .padding(vertical = 12.dp)
                .background(Color(0xFF1E293B), shape = RoundedCornerShape(16.dp))
                .border(1.dp, Color(0xFF334155), shape = RoundedCornerShape(16.dp))
                .verticalScroll(scrollState)
                .padding(24.dp),
            horizontalAlignment = Alignment.CenterHorizontally
        ) {
            Text(
                text = "RUST RDP VNC",
                fontSize = 24.sp,
                fontWeight = FontWeight.Bold,
                color = MaterialTheme.colorScheme.primary,
                letterSpacing = 1.5.sp
            )
            Text(
                text = "Rust Backend Core",
                fontSize = 12.sp,
                color = MaterialTheme.colorScheme.secondary,
                modifier = Modifier.padding(bottom = 24.dp)
            )

            Row(
                modifier = Modifier.fillMaxWidth().padding(bottom = 16.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp)
            ) {
                Button(
                    onClick = { launcher.launch("*/*") },
                    modifier = Modifier.weight(1f).height(50.dp),
                    colors = ButtonDefaults.buttonColors(containerColor = Color(0xFF334155))
                ) {
                    Icon(Icons.AutoMirrored.Filled.List, contentDescription = "Open", tint = Color.White)
                    Spacer(modifier = Modifier.width(4.dp))
                    Text("Open", color = Color.White)
                }

                Button(
                    onClick = {
                        val defaultFilename = if (viewModel.connectionMode == "VNC") "connection.vnc" else "connection.rdp"
                        saveLauncher.launch(defaultFilename)
                    },
                    modifier = Modifier.weight(1f).height(50.dp),
                    colors = ButtonDefaults.buttonColors(containerColor = Color(0xFF334155))
                ) {
                    Icon(Icons.Default.Add, contentDescription = "Save", tint = Color.White)
                    Spacer(modifier = Modifier.width(4.dp))
                    Text("Save", color = Color.White)
                }
            }

            if (viewModel.connectionState == ConnectionState.FAILED) {
                Card(
                    colors = CardDefaults.cardColors(containerColor = Color(0xFF7F1D1D)),
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(bottom = 16.dp)
                ) {
                    Text(
                        text = "Connection Failed: ${viewModel.statusMessage}",
                        color = Color(0xFFFCA5A5),
                        fontSize = 13.sp,
                        modifier = Modifier.padding(12.dp)
                    )
                }
            }

            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(bottom = 12.dp)
                    .background(Color(0xFF0F172A), shape = RoundedCornerShape(8.dp))
                    .padding(4.dp),
                horizontalArrangement = Arrangement.SpaceEvenly
            ) {
                val modes = listOf("RDP", "VNC")
                modes.forEach { mode ->
                    val isSelected = viewModel.connectionMode == mode
                    Box(
                        modifier = Modifier
                            .weight(1f)
                            .height(36.dp)
                            .background(
                                color = if (isSelected) MaterialTheme.colorScheme.primary else Color.Transparent,
                                shape = RoundedCornerShape(6.dp)
                            )
                            .clickable {
                                viewModel.connectionMode = mode
                                // Change default port automatically
                                if (mode == "VNC" && viewModel.port == "3389") {
                                    viewModel.port = "5900"
                                } else if (mode == "RDP" && viewModel.port == "5900") {
                                    viewModel.port = "3389"
                                }
                            },
                        contentAlignment = Alignment.Center
                    ) {
                        Text(
                            text = mode,
                            color = if (isSelected) Color.White else Color(0xFF94A3B8),
                            fontWeight = FontWeight.Bold,
                            fontSize = 14.sp
                        )
                    }
                }
            }

            OutlinedTextField(
                value = viewModel.host,
                onValueChange = { viewModel.host = it },
                label = { Text("Remote PC IP Address") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth().padding(bottom = 8.dp),
                colors = OutlinedTextFieldDefaults.colors(
                    focusedBorderColor = MaterialTheme.colorScheme.primary,
                    unfocusedBorderColor = Color(0xFF475569)
                )
            )

            if (viewModel.connectionMode == "RDP") {
                Row(modifier = Modifier.fillMaxWidth().padding(bottom = 8.dp)) {
                    OutlinedTextField(
                        value = viewModel.port,
                        onValueChange = { viewModel.port = it },
                        label = { Text("Port") },
                        singleLine = true,
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
                        modifier = Modifier.weight(1f).padding(end = 8.dp),
                        colors = OutlinedTextFieldDefaults.colors(
                            focusedBorderColor = MaterialTheme.colorScheme.primary,
                            unfocusedBorderColor = Color(0xFF475569)
                        )
                    )

                    OutlinedTextField(
                        value = viewModel.domain,
                        onValueChange = { viewModel.domain = it },
                        label = { Text("Domain (optional)") },
                        singleLine = true,
                        modifier = Modifier.weight(2f),
                        colors = OutlinedTextFieldDefaults.colors(
                            focusedBorderColor = MaterialTheme.colorScheme.primary,
                            unfocusedBorderColor = Color(0xFF475569)
                        )
                    )
                }
            } else {
                OutlinedTextField(
                    value = viewModel.port,
                    onValueChange = { viewModel.port = it },
                    label = { Text("Port") },
                    singleLine = true,
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
                    modifier = Modifier.fillMaxWidth().padding(bottom = 8.dp),
                    colors = OutlinedTextFieldDefaults.colors(
                        focusedBorderColor = MaterialTheme.colorScheme.primary,
                        unfocusedBorderColor = Color(0xFF475569)
                    )
                )
            }

            OutlinedTextField(
                value = viewModel.username,
                onValueChange = { viewModel.username = it },
                label = { Text(if (viewModel.connectionMode == "VNC") "Username (optional)" else "Username") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth().padding(bottom = 8.dp),
                colors = OutlinedTextFieldDefaults.colors(
                    focusedBorderColor = MaterialTheme.colorScheme.primary,
                    unfocusedBorderColor = Color(0xFF475569)
                )
            )

            var showPassword by remember { mutableStateOf(false) }

            OutlinedTextField(
                value = viewModel.password,
                onValueChange = { viewModel.password = it },
                label = { Text("Password") },
                singleLine = true,
                visualTransformation = if (showPassword) VisualTransformation.None else PasswordVisualTransformation(),
                trailingIcon = {
                    IconButton(onClick = { showPassword = !showPassword }) {
                        Text(
                            text = if (showPassword) "🙈" else "👁",
                            fontSize = 16.sp
                        )
                    }
                },
                modifier = Modifier.fillMaxWidth().padding(bottom = 12.dp),
                colors = OutlinedTextFieldDefaults.colors(
                    focusedBorderColor = MaterialTheme.colorScheme.primary,
                    unfocusedBorderColor = Color(0xFF475569)
                )
            )

            // Resolution Dropdown Selector
            var resolutionExpanded by remember { mutableStateOf(false) }
            val resolutionOptions = listOf(
                "1920x1080 (FHD)",
                "1280x720 (HD)",
                "2560x1440 (2K)",
                "1024x768 (SD)",
                "Native Display",
                "Custom"
            )

            Box(modifier = Modifier.fillMaxWidth().padding(bottom = 12.dp)) {
                OutlinedButton(
                    onClick = { resolutionExpanded = true },
                    modifier = Modifier.fillMaxWidth().height(54.dp),
                    shape = RoundedCornerShape(4.dp),
                    border = BorderStroke(1.dp, Color(0xFF475569)),
                    colors = ButtonDefaults.outlinedButtonColors(contentColor = Color.White)
                ) {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Column {
                            Text("Resolution", fontSize = 10.sp, color = Color(0xFF94A3B8))
                            Text(
                                text = if (viewModel.selectedResolution == "Custom") {
                                    "Custom (${viewModel.customWidth}x${viewModel.customHeight})"
                                } else {
                                    viewModel.selectedResolution
                                },
                                color = Color.White,
                                fontSize = 14.sp
                            )
                        }
                        Text("▼", color = Color(0xFF94A3B8), fontSize = 12.sp)
                    }
                }

                DropdownMenu(
                    expanded = resolutionExpanded,
                    onDismissRequest = { resolutionExpanded = false },
                    modifier = Modifier.background(Color(0xFF1E293B))
                ) {
                    resolutionOptions.forEach { option ->
                        DropdownMenuItem(
                            text = {
                                Text(
                                    text = option,
                                    color = if (viewModel.selectedResolution == option) MaterialTheme.colorScheme.primary else Color.White
                                )
                            },
                            onClick = {
                                viewModel.selectedResolution = option
                                resolutionExpanded = false
                            }
                        )
                    }
                }
            }

            if (viewModel.selectedResolution == "Custom") {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(bottom = 16.dp),
                    horizontalArrangement = Arrangement.spacedBy(12.dp)
                ) {
                    OutlinedTextField(
                        value = viewModel.customWidth,
                        onValueChange = { viewModel.customWidth = it.filter { c -> c.isDigit() } },
                        label = { Text("Width", color = Color(0xFF94A3B8)) },
                        modifier = Modifier.weight(1f),
                        singleLine = true,
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
                        colors = OutlinedTextFieldDefaults.colors(
                            focusedBorderColor = MaterialTheme.colorScheme.primary,
                            unfocusedBorderColor = Color(0xFF475569),
                            focusedTextColor = Color.White,
                            unfocusedTextColor = Color.White
                        )
                    )
                    OutlinedTextField(
                        value = viewModel.customHeight,
                        onValueChange = { viewModel.customHeight = it.filter { c -> c.isDigit() } },
                        label = { Text("Height", color = Color(0xFF94A3B8)) },
                        modifier = Modifier.weight(1f),
                        singleLine = true,
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
                        colors = OutlinedTextFieldDefaults.colors(
                            focusedBorderColor = MaterialTheme.colorScheme.primary,
                            unfocusedBorderColor = Color(0xFF475569),
                            focusedTextColor = Color.White,
                            unfocusedTextColor = Color.White
                        )
                    )
                }
            }

            Button(
                onClick = { onConnect() },
                modifier = Modifier
                    .fillMaxWidth()
                    .height(50.dp),
                colors = ButtonDefaults.buttonColors(containerColor = MaterialTheme.colorScheme.primary),
                shape = RoundedCornerShape(8.dp)
            ) {
                Text(
                    text = "CONNECT",
                    fontWeight = FontWeight.Bold,
                    color = Color(0xFF0F1219),
                    fontSize = 16.sp
                )
            }
        }
    }
}

@Composable
fun ConnectingScreen(viewModel: RdpViewModel) {
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Color(0xFF0F1219)),
        contentAlignment = Alignment.Center
    ) {
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            CircularProgressIndicator(
                color = MaterialTheme.colorScheme.primary,
                strokeWidth = 4.dp,
                modifier = Modifier.size(56.dp)
            )
            Spacer(modifier = Modifier.height(24.dp))
            Text(
                text = "Connecting to Remote Host...",
                fontWeight = FontWeight.Bold,
                fontSize = 18.sp,
                color = Color.White
            )
            Spacer(modifier = Modifier.height(8.dp))
            Text(
                text = viewModel.statusMessage,
                fontSize = 14.sp,
                color = Color(0xFF94A3B8)
            )
            Spacer(modifier = Modifier.height(32.dp))
            OutlinedButton(
                onClick = {
                    viewModel.connectionGeneration += 1L
                    if (viewModel.sessionId != 0L) {
                        RdpClient.disconnectSession(viewModel.sessionId)
                        viewModel.sessionId = 0L
                    }
                    viewModel.onStateChanged(0, "Cancelled")
                },
                border = BorderStroke(1.dp, Color(0xFFEF4444)),
                colors = ButtonDefaults.outlinedButtonColors(contentColor = Color(0xFFEF4444))
            ) {
                Text("CANCEL")
            }
        }
    }
}

suspend fun PointerInputScope.detectPinchZoomPan(
    onZoom: (zoom: Float) -> Unit,
    onScroll: (pan: Offset) -> Unit,
    onTwoFingerTap: () -> Unit
) {
    awaitEachGesture {
        var pastTouchSlop = false
        var zoom = 1f
        var pan = Offset.Zero
        var hadTwoPointers = false
        var twoFingerMoved = false
        var sentScroll = false
        val touchSlop = viewConfiguration.touchSlop
        var allPointersUp = false
        awaitFirstDown(requireUnconsumed = false)
        do {
            val event = awaitPointerEvent()
            val canceled = event.changes.any { it.isConsumed }
            if (!canceled) {
                val pointersCount = event.changes.filter { it.pressed }.size
                if (pointersCount >= 2) {
                    hadTwoPointers = true
                    val zoomChange = event.calculateZoom()
                    val panChange = event.calculatePan()

                    if (abs(1f - zoomChange) <= 0.01f && panChange != Offset.Zero) {
                        pan += panChange
                        if (pan.getDistance() > touchSlop / 2f) {
                            twoFingerMoved = true
                        }
                        sentScroll = true
                        onScroll(panChange)
                        event.changes.forEach {
                            if (it.positionChanged()) {
                                it.consume()
                            }
                        }
                        continue
                    }

                    if (!pastTouchSlop) {
                        zoom *= zoomChange
                        pan += panChange
                        val centroidSize = event.calculateCentroidSize(useCurrent = false)
                        val zoomMotion = abs(1 - zoom) * centroidSize
                        val panMotion = pan.getDistance()

                        if (zoomMotion > touchSlop || panMotion > touchSlop) {
                            pastTouchSlop = true
                            twoFingerMoved = true
                        }
                    }

                    if (pastTouchSlop) {
                        if (abs(1f - zoomChange) > 0.01f) {
                            onZoom(zoomChange)
                        } else if (panChange != Offset.Zero) {
                            onScroll(panChange)
                        }
                        event.changes.forEach {
                            if (it.positionChanged()) {
                                it.consume()
                            }
                        }
                    }
                }
            }
            if (!event.changes.any { it.pressed }) {
                allPointersUp = true
            }
        } while (!canceled && event.changes.any { it.pressed })

        if (hadTwoPointers && !twoFingerMoved && !sentScroll && allPointersUp) {
            onTwoFingerTap()
        }
    }
}

suspend fun PointerInputScope.detectImmediateTapGestures(
    onTap: (Offset) -> Unit,
    onDoubleTapDown: (Offset) -> Unit,
    onDoubleTapDrag: (position: Offset, dragAmount: Offset) -> Unit,
    onDoubleTapUp: (Offset) -> Unit,
    onGestureStart: () -> Unit
) {
    awaitEachGesture {
        awaitFirstDown(requireUnconsumed = false)
        onGestureStart()
        
        var hasMultiplePointers = false
        var upChange: androidx.compose.ui.input.pointer.PointerInputChange? = null
        
        while (true) {
            val event = awaitPointerEvent(androidx.compose.ui.input.pointer.PointerEventPass.Main)
            if (event.changes.filter { it.pressed }.size >= 2) {
                hasMultiplePointers = true
            }
            if (event.changes.all { !it.pressed }) {
                upChange = event.changes.firstOrNull()
                break
            }
            if (event.changes.any { it.isConsumed }) {
                break // Canceled
            }
        }
        
        if (upChange == null || hasMultiplePointers) {
            return@awaitEachGesture
        }
        
        // Tap 1 is released. Send Down to start the gesture sequence.
        onDoubleTapDown(upChange.position)

        // Wait for the second tap down within 200ms
        val secondDown = withTimeoutOrNull(200L) {
            awaitFirstDown(requireUnconsumed = false)
        }

        if (secondDown == null || hasMultiplePointers) {
            // No second down, release the mouse button
            onDoubleTapUp(upChange.position)
            return@awaitEachGesture
        }

        val touchSlop = viewConfiguration.touchSlop / 2f
        var isDragStarted = false
        var lastPosition = secondDown.position
        val startPosition = secondDown.position

        do {
            val event = awaitPointerEvent()
            if (event.changes.filter { it.pressed }.size >= 2) {
                hasMultiplePointers = true
                break
            }
            val change = event.changes.firstOrNull() ?: break
            if (change.pressed) {
                val dragAmount = change.position - lastPosition
                val totalDrag = change.position - startPosition
                if (totalDrag.getDistance() > touchSlop) {
                    isDragStarted = true
                    onDoubleTapDrag(change.position, dragAmount)
                    change.consume()
                    lastPosition = change.position
                }
            }
        } while (event.changes.any { it.pressed })

        if (!hasMultiplePointers) {
            if (!isDragStarted) {
                // No drag: perform double-click by toggling mouse button state
                onDoubleTapUp(secondDown.position)
                onDoubleTapDown(secondDown.position)
                onDoubleTapUp(lastPosition)
            } else {
                // Drag completed: release the mouse button
                onDoubleTapUp(lastPosition)
            }
        }
    }
}

suspend fun PointerInputScope.detectSingleFingerDragGestures(
    onDragStart: (Offset) -> Unit,
    onDragEnd: () -> Unit,
    onDragCancel: () -> Unit,
    onDrag: (change: androidx.compose.ui.input.pointer.PointerInputChange, dragAmount: Offset) -> Unit
) {
    awaitEachGesture {
        val down = awaitFirstDown(requireUnconsumed = false)
        var drag: androidx.compose.ui.input.pointer.PointerInputChange? = null
        val startPos = down.position
        val touchSlop = viewConfiguration.touchSlop
        var isCanceled = false
        
        do {
            val event = awaitPointerEvent()
            if (event.changes.filter { it.pressed }.size >= 2) {
                isCanceled = true
                break
            }
            val change = event.changes.firstOrNull { it.id == down.id }
            if (change == null || !change.pressed) {
                break
            }
            val dragAmount = change.position - startPos
            if (dragAmount.getDistance() > touchSlop) {
                drag = change
            }
        } while (drag == null)
        
        if (drag != null && !isCanceled) {
            onDragStart(drag.position)
            var currentPos = drag.position
            do {
                val event = awaitPointerEvent()
                if (event.changes.filter { it.pressed }.size >= 2) {
                    isCanceled = true
                    break
                }
                val change = event.changes.firstOrNull { it.id == down.id }
                if (change == null || !change.pressed) {
                    break
                }
                val dragAmount = change.position - currentPos
                onDrag(change, dragAmount)
                currentPos = change.position
            } while (event.changes.any { it.pressed })
            
            if (isCanceled) {
                onDragCancel()
            } else {
                onDragEnd()
            }
        }
    }
}

@Composable
fun RepeatingButton(
    modifier: Modifier = Modifier,
    onClick: () -> Unit,
    onDown: () -> Unit,
    onUp: () -> Unit,
    containerColor: Color = Color(0xFF1E293B),
    pressedColor: Color = Color(0xFF6366F1), // Bright Indigo highlighting
    contentPadding: PaddingValues = ButtonDefaults.ContentPadding,
    shape: androidx.compose.ui.graphics.Shape = ButtonDefaults.shape,
    content: @Composable RowScope.() -> Unit
) {
    val coroutineScope = rememberCoroutineScope()
    var isPressed by remember { mutableStateOf(false) }

    Button(
        onClick = {},
        colors = ButtonDefaults.buttonColors(
            containerColor = if (isPressed) pressedColor else containerColor
        ),
        contentPadding = contentPadding,
        shape = shape,
        modifier = modifier.pointerInput(Unit) {
            val touchSlop = viewConfiguration.touchSlop
            awaitEachGesture {
                val down = awaitFirstDown(requireUnconsumed = false)
                isPressed = true
                var isLongPress = false
                var isCancelled = false
                
                // Start a job to detect long press
                val job = coroutineScope.launch {
                    delay(400) // 400ms long press threshold
                    isLongPress = true
                    onDown()
                    while (true) {
                        onClick()
                        delay(50)
                    }
                }
                
                try {
                    // Wait for release or cancellation
                    var upChange: androidx.compose.ui.input.pointer.PointerInputChange? = null
                    val startPos = down.position
                    
                    do {
                        val event = awaitPointerEvent()
                        val change = event.changes.firstOrNull()
                        if (change != null) {
                            // Check if finger moved too far (scroll/drag)
                            if ((change.position - startPos).getDistance() > touchSlop) {
                                isCancelled = true
                                isPressed = false
                                job.cancel()
                            }
                            if (!change.pressed) {
                                upChange = change
                                isPressed = false
                            }
                        }
                    } while (upChange == null && !isCancelled)
                    
                    if (!isCancelled && !isLongPress) {
                        // Short tap: send Down and Up back-to-back on release
                        onDown()
                        onUp()
                    }
                } finally {
                    isPressed = false
                    job.cancel()
                    if (isLongPress) {
                        onUp()
                    }
                }
            }
        }
    ) {
        content()
    }
}

@Composable
fun HighlightButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    containerColor: Color = Color(0xFF1E293B),
    pressedColor: Color = Color(0xFF6366F1), // Bright Indigo highlighting
    contentPadding: PaddingValues = ButtonDefaults.ContentPadding,
    shape: androidx.compose.ui.graphics.Shape = ButtonDefaults.shape,
    content: @Composable RowScope.() -> Unit
) {
    val interactionSource = remember { MutableInteractionSource() }
    val isPressed by interactionSource.collectIsPressedAsState()
    Button(
        onClick = onClick,
        interactionSource = interactionSource,
        colors = ButtonDefaults.buttonColors(
            containerColor = if (isPressed) pressedColor else containerColor
        ),
        contentPadding = contentPadding,
        shape = shape,
        modifier = modifier,
        content = content
    )
}

@Composable
fun KeyboardButton(
    label: String,
    scancodePair: Pair<Int, Boolean>,
    keyHeight: androidx.compose.ui.unit.Dp,
    keyPadding: PaddingValues,
    fontSize: androidx.compose.ui.unit.TextUnit,
    minWidth: androidx.compose.ui.unit.Dp = 42.dp,
    containerColor: Color = Color(0xFF1E293B),
    modifier: Modifier = Modifier
) {
    RepeatingButton(
        onClick = { RdpClient.sendScancodeEvent(scancodePair.first, scancodePair.second, 1) },
        onDown = { RdpClient.sendScancodeEvent(scancodePair.first, scancodePair.second, 1) },
        onUp = { RdpClient.sendScancodeEvent(scancodePair.first, scancodePair.second, 0) },
        containerColor = containerColor,
        pressedColor = Color(0xFF6366F1),
        contentPadding = keyPadding,
        modifier = modifier.height(keyHeight).widthIn(min = minWidth),
        shape = RoundedCornerShape(4.dp)
    ) {
        Text(
            text = label,
            fontSize = if (label == "▲" && minWidth > 0.dp) 16.sp else fontSize,
            color = Color.White,
            maxLines = 1,
            softWrap = false,
            overflow = TextOverflow.Clip,
            textAlign = TextAlign.Center
        )
    }
}

@Composable
fun KeyboardModifierButton(
    label: String,
    isPressed: Boolean,
    onClick: () -> Unit,
    keyHeight: androidx.compose.ui.unit.Dp,
    keyPadding: PaddingValues,
    fontSize: androidx.compose.ui.unit.TextUnit,
    minWidth: androidx.compose.ui.unit.Dp,
    boldWhenActive: Boolean = true,
    modifier: Modifier = Modifier
) {
    Button(
        onClick = onClick,
        colors = ButtonDefaults.buttonColors(
            containerColor = if (isPressed) MaterialTheme.colorScheme.primary else Color(0xFF334155)
        ),
        contentPadding = keyPadding,
        modifier = modifier.height(keyHeight).widthIn(min = minWidth),
        shape = RoundedCornerShape(4.dp)
    ) {
        Text(
            text = label,
            fontSize = fontSize,
            color = if (isPressed) Color.Black else Color.White,
            fontWeight = if (boldWhenActive && isPressed) FontWeight.Bold else FontWeight.Normal,
            maxLines = 1,
            softWrap = false,
            overflow = TextOverflow.Clip,
            textAlign = TextAlign.Center
        )
    }
}

@OptIn(ExperimentalComposeUiApi::class)
@Composable
fun RdpSessionScreen(viewModel: RdpViewModel) {
    var keyboardInput by remember { mutableStateOf("") }
    var showKeyboardInput by remember { mutableStateOf(false) }
    var mouseMode by remember { mutableStateOf(MouseMode.CURSOR) }
    var scale by remember { mutableStateOf(1f) }
    var offset by remember { mutableStateOf(Offset.Zero) }
    
    var lastMouseX by remember { mutableStateOf(0) }
    var lastMouseY by remember { mutableStateOf(0) }
    var cursorX by remember { mutableStateOf(viewModel.screenWidth / 2f) }
    var cursorY by remember { mutableStateOf(viewModel.screenHeight / 2f) }
    val context = LocalContext.current
    val sharedPrefs = remember { context.getSharedPreferences("rdp_prefs", Context.MODE_PRIVATE) }
    var pendingWheelDeltaY by remember { mutableStateOf(0f) }
    var pendingWheelDeltaX by remember { mutableStateOf(0f) }
    var isMouseHeld by remember { mutableStateOf(false) }
    var isHoverThrottleEnabled by remember { mutableStateOf(sharedPrefs.getBoolean("isHoverThrottleEnabled", false)) }
    var hoverIntervalMs by remember { mutableStateOf(sharedPrefs.getLong("hoverIntervalMs", 1000L)) }
    var lastMouseSendTime by remember { mutableStateOf(0L) }
    var isToolbarVisible by remember { mutableStateOf(true) }
    var dotOffsetX by remember { mutableStateOf(0f) }
    var dotOffsetY by remember { mutableStateOf(0f) }
    var initialCursorX by remember { mutableStateOf(0f) }
    var initialCursorY by remember { mutableStateOf(0f) }

    fun smoothOffsetTo(target: Offset) {
        offset = Offset(
            offset.x + (target.x - offset.x) * 0.35f,
            offset.y + (target.y - offset.y) * 0.35f
        )
    }

    var isCtrlPressed by remember { mutableStateOf(false) }
    var isAltPressed by remember { mutableStateOf(false) }
    var isShiftPressed by remember { mutableStateOf(false) }
    var isWinPressed by remember { mutableStateOf(false) }
    var canvasSize by remember { mutableStateOf(IntSize.Zero) }
    val physicalKeyboardFocusRequester = remember { FocusRequester() }

    val focusRequester = remember { FocusRequester() }
    val focusManager = LocalFocusManager.current
    val keyboardController = LocalSoftwareKeyboardController.current
    var isInputFocused by remember { mutableStateOf(false) }
    var isRealtimeKeyboardActive by remember { mutableStateOf(false) }
    var showFullKeyboard by remember { mutableStateOf(false) }
    var fullKeyboardMode by remember {
        val savedMode = sharedPrefs.getString("fullKeyboardMode", FullKeyboardMode.ALL_SCROLL.name)
        val initialMode = try {
            FullKeyboardMode.valueOf(savedMode ?: FullKeyboardMode.ALL_SCROLL.name)
        } catch (e: Exception) {
            FullKeyboardMode.ALL_SCROLL
        }
        mutableStateOf(initialMode)
    }

    fun releaseAllModifiers() {
        if (isMouseHeld) {
            RdpClient.sendMouseEvent(cursorX.roundToInt(), cursorY.roundToInt(), 2)
            isMouseHeld = false
        }
        if (isCtrlPressed) {
            RdpClient.sendScancodeEvent(0x1D, false, 0)
            isCtrlPressed = false
        }
        if (isAltPressed) {
            RdpClient.sendScancodeEvent(0x38, false, 0)
            isAltPressed = false
        }
        if (isShiftPressed) {
            RdpClient.sendScancodeEvent(0x2A, false, 0)
            isShiftPressed = false
        }
        if (isWinPressed) {
            RdpClient.sendScancodeEvent(0x5B, true, 0)
            isWinPressed = false
        }
    }

    val isAnyKeyboardActive = isRealtimeKeyboardActive || showKeyboardInput || showFullKeyboard
    androidx.activity.compose.BackHandler(enabled = isAnyKeyboardActive) {
        if (showKeyboardInput && isInputFocused) {
            keyboardController?.hide()
            focusManager.clearFocus()
        } else {
            isRealtimeKeyboardActive = false
            showKeyboardInput = false
            showFullKeyboard = false
            releaseAllModifiers()
        }
    }

    var realtimeKeyboardImeShown by remember { mutableStateOf(false) }
    var showKeyboardImeShown by remember { mutableStateOf(false) }
    @OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)
    val isImeVisible = WindowInsets.isImeVisible

    LaunchedEffect(isRealtimeKeyboardActive) {
        if (!isRealtimeKeyboardActive) {
            realtimeKeyboardImeShown = false
        }
    }

    LaunchedEffect(showKeyboardInput) {
        if (!showKeyboardInput) {
            showKeyboardImeShown = false
            isInputFocused = false
            releaseAllModifiers()
        }
    }

    LaunchedEffect(isImeVisible) {
        if (isImeVisible) {
            if (isRealtimeKeyboardActive) {
                realtimeKeyboardImeShown = true
            }
            if (showKeyboardInput) {
                showKeyboardImeShown = true
            }
        } else {
            if (isRealtimeKeyboardActive && realtimeKeyboardImeShown) {
                isRealtimeKeyboardActive = false
                releaseAllModifiers()
            }
            if (showKeyboardInput && showKeyboardImeShown) {
                focusManager.clearFocus()
            }
            realtimeKeyboardImeShown = false
            showKeyboardImeShown = false
        }
    }

    val isKeyboardOpened = isAnyKeyboardActive || isImeVisible
    val density = LocalDensity.current
    val toolbarHeightPx = with(density) { 48.dp.toPx() }
    val configuration = LocalConfiguration.current
    val isPortrait = configuration.orientation == Configuration.ORIENTATION_PORTRAIT

    fun mapHardwarePointerToRemote(x: Float, y: Float): Offset {
        val width = canvasSize.width.toFloat().coerceAtLeast(1f)
        val height = canvasSize.height.toFloat().coerceAtLeast(1f)
        val fitScale = minOf(width / viewModel.screenWidth.toFloat(), height / viewModel.screenHeight.toFloat())
        val drawWidth = viewModel.screenWidth * fitScale
        val drawHeight = viewModel.screenHeight * fitScale
        val drawLeft = (width - drawWidth) / 2f
        val drawTop = if (isPortrait && isKeyboardOpened) (if (isToolbarVisible) toolbarHeightPx else 0f) else (height - drawHeight) / 2f
        val pivotX = width / 2f
        val pivotY = if (isPortrait && isKeyboardOpened) drawTop else height / 2f
        val canvasX = ((x - offset.x - pivotX) / scale) + pivotX
        val canvasY = ((y - offset.y - pivotY) / scale) + pivotY
        val remoteX = ((canvasX - drawLeft) / drawWidth) * viewModel.screenWidth
        val remoteY = ((canvasY - drawTop) / drawHeight) * viewModel.screenHeight
        return Offset(
            remoteX.coerceIn(0f, (viewModel.screenWidth - 1).toFloat()),
            remoteY.coerceIn(0f, (viewModel.screenHeight - 1).toFloat())
        )
    }

    fun updateHardwarePointer(event: MotionEvent) {
        val remote = mapHardwarePointerToRemote(event.x, event.y)
        cursorX = remote.x
        cursorY = remote.y
        lastMouseX = remote.x.roundToInt()
        lastMouseY = remote.y.roundToInt()
        RdpClient.sendMouseEvent(lastMouseX, lastMouseY, 0)
    }

    LaunchedEffect(viewModel.sessionId) {
        if (viewModel.sessionId != 0L) {
            delay(100)
            runCatching { physicalKeyboardFocusRequester.requestFocus() }
        }
    }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Color.Black)
            .focusRequester(physicalKeyboardFocusRequester)
            .focusable()
            .onPreviewKeyEvent { composeEvent ->
                val nativeEvent = composeEvent.nativeKeyEvent
                val mappedKey = mapAndroidKeyToRdp(nativeEvent.keyCode)
                if (mappedKey == null) {
                    false
                } else {
                    when (nativeEvent.action) {
                        AndroidKeyEvent.ACTION_DOWN -> RdpClient.sendScancodeEvent(mappedKey.scanCode, mappedKey.extended, 1)
                        AndroidKeyEvent.ACTION_UP -> RdpClient.sendScancodeEvent(mappedKey.scanCode, mappedKey.extended, 0)
                    }
                    nativeEvent.action == AndroidKeyEvent.ACTION_DOWN || nativeEvent.action == AndroidKeyEvent.ACTION_UP
                }
            }
    ) {
        // RDP Desktop Stream Canvas
        Box(
            modifier = Modifier
                .fillMaxSize()
                .align(Alignment.Center)
        ) {
            // Invisible TextField for Realtime Keyboard Capture
            if (isRealtimeKeyboardActive) {
                // Use a very long dummy string so software keyboards can send continuous Backspaces on hold
                val dummyText = " ".repeat(2000)
                var realtimeTextFieldValue by remember { mutableStateOf(TextFieldValue(dummyText, TextRange(dummyText.length))) }
                BasicTextField(
                    value = realtimeTextFieldValue,
                    onValueChange = { newValue ->
                        if (newValue.text.length < realtimeTextFieldValue.text.length) {
                            // Text got shorter: User pressed Backspace
                            val deletes = realtimeTextFieldValue.text.length - newValue.text.length
                            for (i in 0 until deletes) {
                                RdpClient.sendKeyEvent(8, 1)
                                RdpClient.sendKeyEvent(8, 0)
                            }
                            realtimeTextFieldValue = newValue
                        } else if (newValue.text.length > realtimeTextFieldValue.text.length) {
                            // Text got longer: User typed new characters
                            val diffIndex = realtimeTextFieldValue.text.length
                            if (newValue.text.length > diffIndex) {
                                val addedChars = newValue.text.substring(diffIndex)
                                for (char in addedChars) {
                                    if (char == '\n') {
                                        RdpClient.sendKeyEvent(13, 1)
                                        RdpClient.sendKeyEvent(13, 0)
                                    } else {
                                        RdpClient.sendKeyEvent(char.code, 1)
                                        RdpClient.sendKeyEvent(char.code, 0)
                                    }
                                }
                            }
                            realtimeTextFieldValue = newValue
                        }

                        // Refill dummy text if running low (so user can hold backspace indefinitely)
                        if (realtimeTextFieldValue.text.length < 500) {
                            realtimeTextFieldValue = TextFieldValue(dummyText, TextRange(dummyText.length))
                        }
                    },
                    keyboardOptions = KeyboardOptions(
                        keyboardType = KeyboardType.Password,
                        autoCorrect = false
                    ),
                    modifier = Modifier
                        .fillMaxSize()
                        .alpha(0.01f) // Needs to be slightly visible for reliable focus
                        .focusRequester(focusRequester)
                        .onKeyEvent { keyEvent ->
                            if (keyEvent.type == KeyEventType.KeyDown) {
                                val keyCode = keyEvent.key.keyCode
                                // Hardware or custom keyboards might still send KeyEvents
                                if (keyCode == Key.Backspace.keyCode) {
                                    RdpClient.sendKeyEvent(8, 1)
                                    RdpClient.sendKeyEvent(8, 0)
                                    return@onKeyEvent true
                                }
                                if (keyCode == Key.Enter.keyCode) {
                                    RdpClient.sendKeyEvent(13, 1)
                                    RdpClient.sendKeyEvent(13, 0)
                                    return@onKeyEvent true
                                }
                            }
                            false
                        }
                )
            }

            Canvas(
                modifier = Modifier
                    .fillMaxSize()
                    .onSizeChanged { canvasSize = it }
                    .pointerInteropFilter { event ->
                        if (!event.isFromSource(InputDevice.SOURCE_MOUSE)) {
                            false
                        } else {
                            when (event.actionMasked) {
                                MotionEvent.ACTION_HOVER_ENTER,
                                MotionEvent.ACTION_HOVER_MOVE,
                                MotionEvent.ACTION_MOVE -> updateHardwarePointer(event)

                                MotionEvent.ACTION_DOWN -> {
                                    updateHardwarePointer(event)
                                    RdpClient.sendMouseEvent(lastMouseX, lastMouseY, 1)
                                }

                                MotionEvent.ACTION_UP -> {
                                    updateHardwarePointer(event)
                                    RdpClient.sendMouseEvent(lastMouseX, lastMouseY, 2)
                                }

                                MotionEvent.ACTION_BUTTON_PRESS -> {
                                    updateHardwarePointer(event)
                                    when (event.actionButton) {
                                        MotionEvent.BUTTON_PRIMARY -> RdpClient.sendMouseEvent(lastMouseX, lastMouseY, 1)
                                        MotionEvent.BUTTON_SECONDARY -> RdpClient.sendMouseEvent(lastMouseX, lastMouseY, 3)
                                        MotionEvent.BUTTON_TERTIARY -> RdpClient.sendMouseEvent(lastMouseX, lastMouseY, 5)
                                        MotionEvent.BUTTON_BACK -> RdpClient.sendMouseEvent(lastMouseX, lastMouseY, 7)
                                        MotionEvent.BUTTON_FORWARD -> RdpClient.sendMouseEvent(lastMouseX, lastMouseY, 9)
                                    }
                                }

                                MotionEvent.ACTION_BUTTON_RELEASE -> {
                                    updateHardwarePointer(event)
                                    when (event.actionButton) {
                                        MotionEvent.BUTTON_PRIMARY -> RdpClient.sendMouseEvent(lastMouseX, lastMouseY, 2)
                                        MotionEvent.BUTTON_SECONDARY -> RdpClient.sendMouseEvent(lastMouseX, lastMouseY, 4)
                                        MotionEvent.BUTTON_TERTIARY -> RdpClient.sendMouseEvent(lastMouseX, lastMouseY, 6)
                                        MotionEvent.BUTTON_BACK -> RdpClient.sendMouseEvent(lastMouseX, lastMouseY, 8)
                                        MotionEvent.BUTTON_FORWARD -> RdpClient.sendMouseEvent(lastMouseX, lastMouseY, 10)
                                    }
                                }

                                MotionEvent.ACTION_SCROLL -> {
                                    updateHardwarePointer(event)
                                    val vertical = (event.getAxisValue(MotionEvent.AXIS_VSCROLL) * 120f).roundToInt()
                                    val horizontal = (event.getAxisValue(MotionEvent.AXIS_HSCROLL) * 120f).roundToInt()
                                    if (vertical != 0) RdpClient.sendMouseWheelEvent(lastMouseX, lastMouseY, vertical)
                                    if (horizontal != 0) RdpClient.sendMouseHorizontalWheelEvent(lastMouseX, lastMouseY, horizontal)
                                }

                                MotionEvent.ACTION_CANCEL -> {
                                    RdpClient.sendMouseEvent(lastMouseX, lastMouseY, 2)
                                    RdpClient.sendMouseEvent(lastMouseX, lastMouseY, 4)
                                    RdpClient.sendMouseEvent(lastMouseX, lastMouseY, 6)
                                    RdpClient.sendMouseEvent(lastMouseX, lastMouseY, 8)
                                    RdpClient.sendMouseEvent(lastMouseX, lastMouseY, 10)
                                }
                            }
                            true
                        }
                    }
                    .pointerInput(Unit) {
                        val followCursor = {
                            val fitScale = minOf(size.width / viewModel.screenWidth.toFloat(), size.height / viewModel.screenHeight.toFloat())
                            val drawWidth = viewModel.screenWidth * fitScale
                            val drawHeight = viewModel.screenHeight * fitScale
                            val drawLeft = (size.width - drawWidth) / 2f
                            val drawTop = if (isPortrait && isKeyboardOpened) (if (isToolbarVisible) toolbarHeightPx else 0f) else (size.height - drawHeight) / 2f
                            val pivotX = size.width / 2f
                            val pivotY = if (isPortrait && isKeyboardOpened) drawTop else size.height / 2f
                            val cursorCanvasX = drawLeft + (cursorX / viewModel.screenWidth.toFloat()) * drawWidth
                            val cursorCanvasY = drawTop + (cursorY / viewModel.screenHeight.toFloat()) * drawHeight
                            val cursorScreenX = (cursorCanvasX - pivotX) * scale + pivotX + offset.x
                            val cursorScreenY = (cursorCanvasY - pivotY) * scale + pivotY + offset.y
                            val margin = 140f
                            val maxOffsetX = ((drawWidth * scale) - size.width).coerceAtLeast(0f) / 2f
                            val maxOffsetY = ((drawHeight * scale) - size.height).coerceAtLeast(0f) / 2f
                            var nextOffsetX = offset.x
                            var nextOffsetY = offset.y

                            if (cursorScreenX < margin) nextOffsetX += (margin - cursorScreenX) * 0.45f
                            if (cursorScreenX > size.width - margin) nextOffsetX -= (cursorScreenX - (size.width - margin)) * 0.45f
                            if (cursorScreenY < margin) nextOffsetY += (margin - cursorScreenY) * 0.45f
                            if (cursorScreenY > size.height - margin) nextOffsetY -= (cursorScreenY - (size.height - margin)) * 0.45f

                            smoothOffsetTo(Offset(
                                nextOffsetX.coerceIn(-maxOffsetX, maxOffsetX),
                                nextOffsetY.coerceIn(-maxOffsetY, maxOffsetY)
                            ))
                        }

                        detectPinchZoomPan(
                            onZoom = { zoom ->
                            scale = (scale * zoom).coerceIn(1f, 5f)

                            val fitScale = minOf(size.width / viewModel.screenWidth.toFloat(), size.height / viewModel.screenHeight.toFloat())
                            val drawWidth = viewModel.screenWidth * fitScale
                            val drawHeight = viewModel.screenHeight * fitScale

                            val maxOffsetX = ((drawWidth * scale) - size.width).coerceAtLeast(0f) / 2f
                            val maxOffsetY = ((drawHeight * scale) - size.height).coerceAtLeast(0f) / 2f
                            
                            offset = Offset(
                                offset.x.coerceIn(-maxOffsetX, maxOffsetX),
                                offset.y.coerceIn(-maxOffsetY, maxOffsetY)
                            )
                            followCursor()
                            },
                            onScroll = { pan ->
                                pendingWheelDeltaY += pan.y
                                val wheelStep = 6f
                                while (abs(pendingWheelDeltaY) >= wheelStep) {
                                    val direction = if (pendingWheelDeltaY > 0f) 1 else -1
                                    pendingWheelDeltaY -= direction * wheelStep
                                    RdpClient.sendMouseWheelEvent(cursorX.roundToInt(), cursorY.roundToInt(), direction * 120)
                                }

                                pendingWheelDeltaX += pan.x
                                while (abs(pendingWheelDeltaX) >= wheelStep) {
                                    val direction = if (pendingWheelDeltaX > 0f) 1 else -1
                                    pendingWheelDeltaX -= direction * wheelStep
                                    RdpClient.sendMouseHorizontalWheelEvent(cursorX.roundToInt(), cursorY.roundToInt(), direction * 120)
                                }
                            },
                            onTwoFingerTap = {
                                RdpClient.sendMouseEvent(cursorX.roundToInt(), cursorY.roundToInt(), 3) // Right Down
                                RdpClient.sendMouseEvent(cursorX.roundToInt(), cursorY.roundToInt(), 4) // Right Up
                            }
                        )
                    }
                    .pointerInput(mouseMode, scale) {
                        val followCursor = {
                            val fitScale = minOf(size.width / viewModel.screenWidth.toFloat(), size.height / viewModel.screenHeight.toFloat())
                            val drawWidth = viewModel.screenWidth * fitScale
                            val drawHeight = viewModel.screenHeight * fitScale
                            val drawLeft = (size.width - drawWidth) / 2f
                            val drawTop = if (isPortrait && isKeyboardOpened) (if (isToolbarVisible) toolbarHeightPx else 0f) else (size.height - drawHeight) / 2f
                            val pivotX = size.width / 2f
                            val pivotY = if (isPortrait && isKeyboardOpened) drawTop else size.height / 2f
                            val cursorCanvasX = drawLeft + (cursorX / viewModel.screenWidth.toFloat()) * drawWidth
                            val cursorCanvasY = drawTop + (cursorY / viewModel.screenHeight.toFloat()) * drawHeight
                            val cursorScreenX = (cursorCanvasX - pivotX) * scale + pivotX + offset.x
                            val cursorScreenY = (cursorCanvasY - pivotY) * scale + pivotY + offset.y
                            val margin = 140f
                            val maxOffsetX = ((drawWidth * scale) - size.width).coerceAtLeast(0f) / 2f
                            val maxOffsetY = ((drawHeight * scale) - size.height).coerceAtLeast(0f) / 2f
                            var nextOffsetX = offset.x
                            var nextOffsetY = offset.y

                            if (cursorScreenX < margin) nextOffsetX += (margin - cursorScreenX) * 0.45f
                            if (cursorScreenX > size.width - margin) nextOffsetX -= (cursorScreenX - (size.width - margin)) * 0.45f
                            if (cursorScreenY < margin) nextOffsetY += (margin - cursorScreenY) * 0.45f
                            if (cursorScreenY > size.height - margin) nextOffsetY -= (cursorScreenY - (size.height - margin)) * 0.45f

                            smoothOffsetTo(Offset(
                                nextOffsetX.coerceIn(-maxOffsetX, maxOffsetX),
                                nextOffsetY.coerceIn(-maxOffsetY, maxOffsetY)
                            ))
                        }

                        val mapOffsetToRdp = { tapOffset: Offset, sizeWidth: Float, sizeHeight: Float ->
                            val fitScale = minOf(sizeWidth / viewModel.screenWidth.toFloat(), sizeHeight / viewModel.screenHeight.toFloat())
                            val drawWidth = viewModel.screenWidth * fitScale
                            val drawHeight = viewModel.screenHeight * fitScale
                            val drawLeft = (sizeWidth - drawWidth) / 2f
                            val drawTop = if (isPortrait && isKeyboardOpened) (if (isToolbarVisible) toolbarHeightPx else 0f) else (sizeHeight - drawHeight) / 2f

                            val pivotX = sizeWidth / 2f
                            val pivotY = if (isPortrait && isKeyboardOpened) drawTop else sizeHeight / 2f

                            val translatedX = tapOffset.x - offset.x
                            val translatedY = tapOffset.y - offset.y

                            val canvasX = (translatedX - pivotX) / scale + pivotX
                            val canvasY = (translatedY - pivotY) / scale + pivotY

                            val rx = ((canvasX - drawLeft) / drawWidth).coerceIn(0f, 1f) * viewModel.screenWidth
                            val ry = ((canvasY - drawTop) / drawHeight).coerceIn(0f, 1f) * viewModel.screenHeight
                            Offset(rx, ry)
                        }

                        detectImmediateTapGestures(
                            onTap = { tapOffset ->
                                val rdpPos = if (mouseMode == MouseMode.CURSOR) {
                                    Offset(initialCursorX, initialCursorY)
                                } else {
                                    mapOffsetToRdp(tapOffset, size.width.toFloat(), size.height.toFloat())
                                }
                                if (mouseMode == MouseMode.CURSOR) {
                                    cursorX = initialCursorX
                                    cursorY = initialCursorY
                                } else {
                                    cursorX = rdpPos.x
                                    cursorY = rdpPos.y
                                    followCursor()
                                }

                                if (isMouseHeld) {
                                    RdpClient.sendMouseEvent(rdpPos.x.roundToInt(), rdpPos.y.roundToInt(), 2) // Left Up
                                    isMouseHeld = false
                                } else {
                                    RdpClient.sendMouseEvent(rdpPos.x.roundToInt(), rdpPos.y.roundToInt(), 1) // Down
                                    RdpClient.sendMouseEvent(rdpPos.x.roundToInt(), rdpPos.y.roundToInt(), 2) // Up
                                }
                            },
                            onDoubleTapDown = { tapOffset ->
                                val rdpPos = if (mouseMode == MouseMode.CURSOR) {
                                    Offset(initialCursorX, initialCursorY)
                                } else {
                                    mapOffsetToRdp(tapOffset, size.width.toFloat(), size.height.toFloat())
                                }
                                if (mouseMode == MouseMode.CURSOR) {
                                    cursorX = initialCursorX
                                    cursorY = initialCursorY
                                } else {
                                    cursorX = rdpPos.x
                                    cursorY = rdpPos.y
                                    followCursor()
                                }

                                RdpClient.sendMouseEvent(rdpPos.x.roundToInt(), rdpPos.y.roundToInt(), 1) // Left Down
                                isMouseHeld = true
                            },
                            onDoubleTapDrag = { position, dragAmount ->
                                val rdpPos = if (mouseMode == MouseMode.CURSOR) {
                                    val fitScale = minOf(size.width / viewModel.screenWidth.toFloat(), size.height / viewModel.screenHeight.toFloat())
                                    val sensitivity = 1.2f / (fitScale * scale)
                                    Offset(
                                        (cursorX + dragAmount.x * sensitivity).coerceIn(0f, (viewModel.screenWidth - 1).toFloat()),
                                        (cursorY + dragAmount.y * sensitivity).coerceIn(0f, (viewModel.screenHeight - 1).toFloat())
                                    )
                                } else {
                                    mapOffsetToRdp(position, size.width.toFloat(), size.height.toFloat())
                                }
                                cursorX = rdpPos.x
                                cursorY = rdpPos.y
                                followCursor()
                                lastMouseX = cursorX.roundToInt()
                                lastMouseY = cursorY.roundToInt()
                                RdpClient.sendMouseEvent(lastMouseX, lastMouseY, 0) // Move with left button held
                            },
                            onDoubleTapUp = { tapOffset ->
                                val rdpPos = if (mouseMode == MouseMode.CURSOR) {
                                    Offset(cursorX, cursorY)
                                } else {
                                    mapOffsetToRdp(tapOffset, size.width.toFloat(), size.height.toFloat())
                                }
                                RdpClient.sendMouseEvent(rdpPos.x.roundToInt(), rdpPos.y.roundToInt(), 2) // Left Up
                                isMouseHeld = false
                            },
                            onGestureStart = {
                                initialCursorX = cursorX
                                initialCursorY = cursorY
                            }
                        )
                    }
                    .pointerInput(mouseMode, scale) {
                        val followCursor = {
                            val fitScale = minOf(size.width / viewModel.screenWidth.toFloat(), size.height / viewModel.screenHeight.toFloat())
                            val drawWidth = viewModel.screenWidth * fitScale
                            val drawHeight = viewModel.screenHeight * fitScale
                            val drawLeft = (size.width - drawWidth) / 2f
                            val drawTop = if (isPortrait && isKeyboardOpened) (if (isToolbarVisible) toolbarHeightPx else 0f) else (size.height - drawHeight) / 2f
                            val pivotX = size.width / 2f
                            val pivotY = if (isPortrait && isKeyboardOpened) drawTop else size.height / 2f
                            val cursorCanvasX = drawLeft + (cursorX / viewModel.screenWidth.toFloat()) * drawWidth
                            val cursorCanvasY = drawTop + (cursorY / viewModel.screenHeight.toFloat()) * drawHeight
                            val cursorScreenX = (cursorCanvasX - pivotX) * scale + pivotX + offset.x
                            val cursorScreenY = (cursorCanvasY - pivotY) * scale + pivotY + offset.y
                            val margin = 140f
                            val maxOffsetX = ((drawWidth * scale) - size.width).coerceAtLeast(0f) / 2f
                            val maxOffsetY = ((drawHeight * scale) - size.height).coerceAtLeast(0f) / 2f
                            var nextOffsetX = offset.x
                            var nextOffsetY = offset.y

                            if (cursorScreenX < margin) nextOffsetX += (margin - cursorScreenX) * 0.45f
                            if (cursorScreenX > size.width - margin) nextOffsetX -= (cursorScreenX - (size.width - margin)) * 0.45f
                            if (cursorScreenY < margin) nextOffsetY += (margin - cursorScreenY) * 0.45f
                            if (cursorScreenY > size.height - margin) nextOffsetY -= (cursorScreenY - (size.height - margin)) * 0.45f

                            smoothOffsetTo(Offset(
                                nextOffsetX.coerceIn(-maxOffsetX, maxOffsetX),
                                nextOffsetY.coerceIn(-maxOffsetY, maxOffsetY)
                            ))
                        }

                        val mapOffsetToRdp = { tapOffset: Offset, sizeWidth: Float, sizeHeight: Float ->
                            val fitScale = minOf(sizeWidth / viewModel.screenWidth.toFloat(), sizeHeight / viewModel.screenHeight.toFloat())
                            val drawWidth = viewModel.screenWidth * fitScale
                            val drawHeight = viewModel.screenHeight * fitScale
                            val drawLeft = (sizeWidth - drawWidth) / 2f
                            val drawTop = if (isPortrait && isKeyboardOpened) (if (isToolbarVisible) toolbarHeightPx else 0f) else (sizeHeight - drawHeight) / 2f

                            val pivotX = sizeWidth / 2f
                            val pivotY = if (isPortrait && isKeyboardOpened) drawTop else sizeHeight / 2f

                            val translatedX = tapOffset.x - offset.x
                            val translatedY = tapOffset.y - offset.y

                            val canvasX = (translatedX - pivotX) / scale + pivotX
                            val canvasY = (translatedY - pivotY) / scale + pivotY

                            val rx = ((canvasX - drawLeft) / drawWidth).coerceIn(0f, 1f) * viewModel.screenWidth
                            val ry = ((canvasY - drawTop) / drawHeight).coerceIn(0f, 1f) * viewModel.screenHeight
                            Offset(rx, ry)
                        }

                        detectSingleFingerDragGestures(
                            onDragStart = { startOffset ->
                                if (!isMouseHeld) {
                                    val rdpPos = if (mouseMode == MouseMode.CURSOR) {
                                        Offset(cursorX, cursorY)
                                    } else {
                                        mapOffsetToRdp(startOffset, size.width.toFloat(), size.height.toFloat())
                                    }
                                    cursorX = rdpPos.x
                                    cursorY = rdpPos.y
                                    if (mouseMode != MouseMode.CURSOR) {
                                        followCursor()
                                    }
                                    lastMouseX = rdpPos.x.roundToInt()
                                    lastMouseY = rdpPos.y.roundToInt()
                                    if (mouseMode == MouseMode.DRAG && !isMouseHeld) {
                                        RdpClient.sendMouseEvent(rdpPos.x.roundToInt(), rdpPos.y.roundToInt(), 1) // Left Down
                                    }
                                }
                            },
                            onDragEnd = {
                                if (mouseMode == MouseMode.DRAG && !isMouseHeld) {
                                    RdpClient.sendMouseEvent(lastMouseX, lastMouseY, 2) // Left Up
                                }
                            },
                            onDragCancel = {
                                if (mouseMode == MouseMode.DRAG && !isMouseHeld) {
                                    RdpClient.sendMouseEvent(lastMouseX, lastMouseY, 2) // Left Up
                                }
                            },
                            onDrag = { change, _ ->
                                if (!isMouseHeld) {
                                    val rdpPos = if (mouseMode == MouseMode.CURSOR) {
                                        val fitScale = minOf(size.width / viewModel.screenWidth.toFloat(), size.height / viewModel.screenHeight.toFloat())
                                        val sensitivity = 1.2f / (fitScale * scale)
                                        Offset(
                                            (cursorX + change.positionChange().x * sensitivity).coerceIn(0f, (viewModel.screenWidth - 1).toFloat()),
                                            (cursorY + change.positionChange().y * sensitivity).coerceIn(0f, (viewModel.screenHeight - 1).toFloat())
                                        )
                                    } else {
                                        mapOffsetToRdp(change.position, size.width.toFloat(), size.height.toFloat())
                                    }
                                    cursorX = rdpPos.x
                                    cursorY = rdpPos.y
                                    val nextMouseX = rdpPos.x.roundToInt()
                                    val nextMouseY = rdpPos.y.roundToInt()
                                    if (nextMouseX != lastMouseX || nextMouseY != lastMouseY) {
                                        followCursor()
                                        lastMouseX = nextMouseX
                                        lastMouseY = nextMouseY
                                        val now = System.currentTimeMillis()
                                        if (isHoverThrottleEnabled && !isMouseHeld) {
                                            val minInterval = hoverIntervalMs.coerceIn(50L, 1000L)
                                            if (now - lastMouseSendTime >= minInterval) {
                                                RdpClient.sendMouseEvent(lastMouseX, lastMouseY, 0) // Move
                                                lastMouseSendTime = now
                                            }
                                        } else {
                                            RdpClient.sendMouseEvent(lastMouseX, lastMouseY, 0) // Move
                                            lastMouseSendTime = now
                                        }
                                    }
                                    change.consume()
                                }
                            }
                        )
                    }
            ) {
                // Read frameTrigger to recompose when frame changes
                val trigger = viewModel.frameTrigger
                viewModel.screenBitmap?.let { bmp ->
                    val fitScale = minOf(size.width / viewModel.screenWidth.toFloat(), size.height / viewModel.screenHeight.toFloat())
                    val drawWidth = viewModel.screenWidth * fitScale
                    val drawHeight = viewModel.screenHeight * fitScale
                    val drawLeft = (size.width - drawWidth) / 2f
                    val drawTop = if (isPortrait && isKeyboardOpened) (if (isToolbarVisible) toolbarHeightPx else 0f) else (size.height - drawHeight) / 2f
                    val pivotY = if (isPortrait && isKeyboardOpened) drawTop else size.height / 2f

                    withTransform({
                        translate(offset.x, offset.y)
                        scale(scale, scale, pivot = Offset(size.width / 2f, pivotY))
                    }) {
                        drawImage(
                            image = bmp.asImageBitmap(),
                            dstOffset = IntOffset(drawLeft.roundToInt(), drawTop.roundToInt()),
                            dstSize = IntSize(drawWidth.roundToInt(), drawHeight.roundToInt())
                        )
                        val cursorCanvasX = drawLeft + (cursorX / viewModel.screenWidth.toFloat()) * drawWidth
                        val cursorCanvasY = drawTop + (cursorY / viewModel.screenHeight.toFloat()) * drawHeight
                        
                        // Custom RDP Cursor Bitmap or Vector Cursor Shape
                        viewModel.cursorBitmap?.let { customBmp ->
                            val hotX = viewModel.cursorHotX
                            val hotY = viewModel.cursorHotY
                            val bmpWidth = customBmp.width.toFloat()
                            val bmpHeight = customBmp.height.toFloat()

                            val curLeft = cursorCanvasX - (hotX / viewModel.screenWidth.toFloat()) * drawWidth
                            val curTop = cursorCanvasY - (hotY / viewModel.screenHeight.toFloat()) * drawHeight
                            val curWidth = (bmpWidth / viewModel.screenWidth.toFloat()) * drawWidth
                            val curHeight = (bmpHeight / viewModel.screenHeight.toFloat()) * drawHeight

                            drawImage(
                                image = customBmp.asImageBitmap(),
                                dstOffset = IntOffset(curLeft.roundToInt(), curTop.roundToInt()),
                                dstSize = IntSize(curWidth.roundToInt().coerceAtLeast(1), curHeight.roundToInt().coerceAtLeast(1))
                            )
                        } ?: run {
                            when (viewModel.cursorType) {
                                2 -> { // Text I-Beam
                                    val p = Path().apply {
                                        moveTo(cursorCanvasX - 3f / scale, cursorCanvasY - 8f / scale)
                                        lineTo(cursorCanvasX + 3f / scale, cursorCanvasY - 8f / scale)
                                        moveTo(cursorCanvasX, cursorCanvasY - 8f / scale)
                                        lineTo(cursorCanvasX, cursorCanvasY + 8f / scale)
                                        moveTo(cursorCanvasX - 3f / scale, cursorCanvasY + 8f / scale)
                                        lineTo(cursorCanvasX + 3f / scale, cursorCanvasY + 8f / scale)
                                    }
                                    drawPath(p, color = Color.Black, style = Stroke(width = 3f / scale))
                                    drawPath(p, color = Color.White, style = Stroke(width = 1.5f / scale))
                                }
                                3 -> { // Pointing Hand
                                    val p = Path().apply {
                                        moveTo(cursorCanvasX, cursorCanvasY)
                                        lineTo(cursorCanvasX + 4f / scale, cursorCanvasY + 4f / scale)
                                        lineTo(cursorCanvasX + 2f / scale, cursorCanvasY + 12f / scale)
                                        lineTo(cursorCanvasX - 4f / scale, cursorCanvasY + 12f / scale)
                                        close()
                                    }
                                    drawPath(p, color = Color.White)
                                    drawPath(p, color = Color.Black, style = Stroke(width = 1f / scale))
                                }
                                4 -> { // Resize NW-SE
                                    val p = Path().apply {
                                        moveTo(cursorCanvasX - 6f / scale, cursorCanvasY - 6f / scale)
                                        lineTo(cursorCanvasX + 6f / scale, cursorCanvasY + 6f / scale)
                                    }
                                    drawPath(p, color = Color.Black, style = Stroke(width = 3f / scale))
                                    drawPath(p, color = Color.White, style = Stroke(width = 1.5f / scale))
                                }
                                5 -> { // Resize NE-SW
                                    val p = Path().apply {
                                        moveTo(cursorCanvasX + 6f / scale, cursorCanvasY - 6f / scale)
                                        lineTo(cursorCanvasX - 6f / scale, cursorCanvasY + 6f / scale)
                                    }
                                    drawPath(p, color = Color.Black, style = Stroke(width = 3f / scale))
                                    drawPath(p, color = Color.White, style = Stroke(width = 1.5f / scale))
                                }
                                6 -> { // Resize EW
                                    val p = Path().apply {
                                        moveTo(cursorCanvasX - 8f / scale, cursorCanvasY)
                                        lineTo(cursorCanvasX + 8f / scale, cursorCanvasY)
                                    }
                                    drawPath(p, color = Color.Black, style = Stroke(width = 3f / scale))
                                    drawPath(p, color = Color.White, style = Stroke(width = 1.5f / scale))
                                }
                                7 -> { // Resize NS
                                    val p = Path().apply {
                                        moveTo(cursorCanvasX, cursorCanvasY - 8f / scale)
                                        lineTo(cursorCanvasX, cursorCanvasY + 8f / scale)
                                    }
                                    drawPath(p, color = Color.Black, style = Stroke(width = 3f / scale))
                                    drawPath(p, color = Color.White, style = Stroke(width = 1.5f / scale))
                                }
                                else -> { // Default Arrow (0)
                                    val cursorPath = Path().apply {
                                        moveTo(cursorCanvasX, cursorCanvasY)
                                        lineTo(cursorCanvasX, cursorCanvasY + 20f / scale)
                                        lineTo(cursorCanvasX + 5f / scale, cursorCanvasY + 15f / scale)
                                        lineTo(cursorCanvasX + 12f / scale, cursorCanvasY + 15f / scale)
                                        close()
                                    }
                                    drawPath(cursorPath, color = Color.White)
                                    drawPath(cursorPath, color = Color.Black, style = Stroke(width = 1f / scale))
                                }
                            }
                        }
                    }
                }
            }
        }

        // Overlay Toolbar
        if (isToolbarVisible) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .background(Color(0x990F1219))
                    .padding(horizontal = 16.dp, vertical = 6.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.SpaceBetween
            ) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier.padding(start = 8.dp)
                ) {
                    Box(
                        modifier = Modifier
                            .size(16.dp)
                            .background(Color(0xFF10B981), shape = CircleShape)
                            .border(1.dp, Color.White.copy(alpha = 0.5f), CircleShape)
                            .clickable { isToolbarVisible = false }
                    )

                    Spacer(modifier = Modifier.width(16.dp))

                    // Disconnect icon button (Moved to left)
                    IconButton(
                        onClick = {
                            releaseAllModifiers()
                            if (viewModel.sessionId != 0L) {
                                RdpClient.disconnectSession(viewModel.sessionId)
                                viewModel.sessionId = 0L
                            }
                            viewModel.onStateChanged(0, "Disconnected")
                        },
                        colors = IconButtonDefaults.iconButtonColors(containerColor = Color(0xFFEF4444)),
                        modifier = Modifier.size(36.dp)
                    ) {
                        Icon(
                            imageVector = Icons.Default.Close,
                            contentDescription = "Disconnect",
                            tint = Color.White,
                            modifier = Modifier.size(20.dp)
                        )
                    }

                    Spacer(modifier = Modifier.width(16.dp))
                    
                    Button(
                        onClick = {
                            mouseMode = when (mouseMode) {
                                MouseMode.MOVE -> MouseMode.DRAG
                                MouseMode.DRAG -> MouseMode.CURSOR
                                MouseMode.CURSOR -> MouseMode.MOVE
                            }
                        },
                        contentPadding = PaddingValues(horizontal = 8.dp, vertical = 0.dp),
                        shape = RoundedCornerShape(6.dp),
                        colors = ButtonDefaults.buttonColors(
                            containerColor = Color.Transparent
                        ),
                        modifier = Modifier.height(32.dp),
                    ) {
                        Text(
                            text = when (mouseMode) { 
                                MouseMode.MOVE -> "Hover"
                                MouseMode.DRAG -> "Drag"
                                MouseMode.CURSOR -> "Cursor"
                            },
                            color = MaterialTheme.colorScheme.primary,
                            fontSize = 14.sp,
                            fontWeight = FontWeight.Bold
                        )
                    }

                    Spacer(modifier = Modifier.width(8.dp))

                    Button(
                        onClick = {
                            if (!isHoverThrottleEnabled) {
                                isHoverThrottleEnabled = true
                                hoverIntervalMs = 50L
                            } else {
                                when (hoverIntervalMs) {
                                    50L -> hoverIntervalMs = 100L
                                    100L -> hoverIntervalMs = 250L
                                    250L -> hoverIntervalMs = 500L
                                    500L -> hoverIntervalMs = 1000L
                                    else -> {
                                        isHoverThrottleEnabled = false
                                        hoverIntervalMs = 1000L
                                    }
                                }
                            }
                            sharedPrefs.edit().apply {
                                putBoolean("isHoverThrottleEnabled", isHoverThrottleEnabled)
                                putLong("hoverIntervalMs", hoverIntervalMs)
                                apply()
                            }
                        },
                        contentPadding = PaddingValues(horizontal = 6.dp, vertical = 0.dp),
                        shape = RoundedCornerShape(6.dp),
                        colors = ButtonDefaults.buttonColors(
                            containerColor = if (isHoverThrottleEnabled) Color(0xFF334155) else Color.Transparent
                        ),
                        modifier = Modifier.height(32.dp),
                    ) {
                        Text(
                            text = if (isHoverThrottleEnabled) "${hoverIntervalMs}ms" else "Realtime",
                            color = if (isHoverThrottleEnabled) Color(0xFF38BDF8) else Color.Gray,
                            fontSize = 12.sp,
                            fontWeight = FontWeight.Bold
                        )
                    }
                }

                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier.padding(end = 8.dp)
                ) {
                    // Realtime Keyboard Toggle
                    Box(
                        modifier = Modifier
                            .size(36.dp)
                            .padding(end = 6.dp)
                            .background(
                                color = if (isRealtimeKeyboardActive) MaterialTheme.colorScheme.primary else Color(0xFF334155),
                                shape = CircleShape
                            )
                            .clickable {
                                isRealtimeKeyboardActive = !isRealtimeKeyboardActive
                                if (isRealtimeKeyboardActive) {
                                    showKeyboardInput = false
                                    showFullKeyboard = false
                                }
                            },
                        contentAlignment = Alignment.Center
                    ) {
                        Text(
                            text = "A1",
                            color = if (isRealtimeKeyboardActive) Color.Black else Color.White,
                            fontSize = 12.sp,
                            fontWeight = FontWeight.Bold
                        )
                    }

                    // Standard Keyboard Toggle (Original Input Panel)
                    Box(
                        modifier = Modifier
                            .size(36.dp)
                            .padding(end = 6.dp)
                            .background(
                                color = if (showKeyboardInput) MaterialTheme.colorScheme.primary else Color(0xFF334155),
                                shape = CircleShape
                            )
                            .clickable {
                                showKeyboardInput = !showKeyboardInput 
                                if (showKeyboardInput) {
                                    isRealtimeKeyboardActive = false
                                    showFullKeyboard = false
                                }
                            },
                        contentAlignment = Alignment.Center
                    ) {
                        Icon(
                            imageVector = Icons.AutoMirrored.Filled.Send,
                            contentDescription = "Input Panel",
                            tint = if (showKeyboardInput) Color.Black else Color.White,
                            modifier = Modifier.size(20.dp)
                        )
                    }

                    // Full Keyboard Toggle
                Box(
                    modifier = Modifier
                        .size(36.dp)
                        .padding(end = 6.dp)
                        .background(
                            color = if (showFullKeyboard) MaterialTheme.colorScheme.primary else Color(0xFF334155),
                            shape = CircleShape
                        )
                        .clickable {
                            showFullKeyboard = !showFullKeyboard 
                            if (showFullKeyboard) {
                                isRealtimeKeyboardActive = false
                                showKeyboardInput = false
                            }
                        },
                    contentAlignment = Alignment.Center
                ) {
                    Text(
                        text = "KB",
                        color = if (showFullKeyboard) Color.Black else Color.White,
                        fontSize = 12.sp,
                        fontWeight = FontWeight.Bold
                    )
                }
            }
        }
        } else {
            // Minimized dot
            Box(
                modifier = Modifier
                    .offset { IntOffset(dotOffsetX.roundToInt(), dotOffsetY.roundToInt()) }
                    .padding(16.dp)
                    .size(24.dp)
                    .background(Color(0xFF10B981).copy(alpha = 0.5f), shape = CircleShape)
                    .border(1.dp, Color.White.copy(alpha = 0.5f), CircleShape)
                    .pointerInput(Unit) {
                        detectDragGestures { change, dragAmount ->
                            change.consume()
                            dotOffsetX += dragAmount.x
                            dotOffsetY += dragAmount.y
                        }
                    }
                    .clickable { 
                        isToolbarVisible = true
                        dotOffsetX = 0f
                        dotOffsetY = 0f
                    }
            )
        }

        // Show keyboard and request focus when Realtime Keyboard is active
        LaunchedEffect(isRealtimeKeyboardActive, showKeyboardInput) {
            if (isRealtimeKeyboardActive) {
                try {
                    focusRequester.requestFocus()
                    keyboardController?.show()
                } catch (e: Exception) {
                    // Focus target might not be placed yet
                }
            } else if (!showKeyboardInput) {
                keyboardController?.hide()
            }
        }

        // Full Virtual Keyboard Panel
        if (showFullKeyboard) {
            Column(
                modifier = Modifier
                    .align(Alignment.BottomCenter)
                    .fillMaxWidth()
                    .background(Color(0xE60F1219))
                    .border(1.dp, Color(0xFF334155), RoundedCornerShape(topStart = 16.dp, topEnd = 16.dp))
                    .padding(8.dp),
                verticalArrangement = Arrangement.spacedBy(6.dp)
            ) {
                val isFitScreen = fullKeyboardMode == FullKeyboardMode.FIT_SCREEN
                val isAllScroll = fullKeyboardMode == FullKeyboardMode.ALL_SCROLL

                val keyHeight = if (isFitScreen && isPortrait) 32.dp else 36.dp
                val keyPadding = if (isFitScreen) {
                    PaddingValues(horizontal = 1.dp, vertical = 0.dp)
                } else {
                    PaddingValues(horizontal = 4.dp, vertical = 0.dp)
                }
                val keySpacing = if (isFitScreen) 2.dp else 4.dp
                val fontSize = 13.sp
                val allScrollState = rememberScrollState()

                // US-layout shifted characters, so the key labels show what the
                // remote side will actually receive while Shift is active.
                fun shiftedKeyLabel(label: String): String = when (label) {
                    "`" -> "~"
                    "1" -> "!"
                    "2" -> "@"
                    "3" -> "#"
                    "4" -> "$"
                    "5" -> "%"
                    "6" -> "^"
                    "7" -> "&"
                    "8" -> "*"
                    "9" -> "("
                    "0" -> ")"
                    "-" -> "_"
                    "=" -> "+"
                    "[" -> "{"
                    "]" -> "}"
                    "\\" -> "|"
                    ";" -> ":"
                    "'" -> "\""
                    "," -> "<"
                    "." -> ">"
                    "/" -> "?"
                    else -> label
                }

                fun keyLabel(label: String): String =
                    if (isShiftPressed) shiftedKeyLabel(label) else label

                fun fitKeyFontSize(label: String): androidx.compose.ui.unit.TextUnit {
                    return when {
                        label.length >= 7 -> if (isPortrait) 6.sp else 8.sp
                        label.length >= 5 -> if (isPortrait) 7.sp else 9.sp
                        label.length >= 3 -> if (isPortrait) 8.sp else 10.sp
                        else -> if (isPortrait) 10.sp else 12.sp
                    }
                }

                @Composable
                fun fullKeyRowModifier(): Modifier {
                    val rowScrollState = rememberScrollState()
                    return when (fullKeyboardMode) {
                        FullKeyboardMode.ROW_SCROLL -> Modifier.fillMaxWidth().horizontalScroll(rowScrollState)
                        FullKeyboardMode.ALL_SCROLL -> Modifier
                        FullKeyboardMode.FIT_SCREEN -> Modifier.fillMaxWidth()
                    }
                }

                // Mode Switcher Bar: Row Scroll (default) | Scroll All | Fit Screen
                val modeOptions = listOf(
                    FullKeyboardMode.ROW_SCROLL to "Row Scroll",
                    FullKeyboardMode.ALL_SCROLL to "Scroll All",
                    FullKeyboardMode.FIT_SCREEN to "Fit Screen"
                )
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .background(Color(0xFF1E293B), RoundedCornerShape(6.dp))
                        .padding(2.dp),
                    horizontalArrangement = Arrangement.spacedBy(2.dp)
                ) {
                    for ((mode, modeLabel) in modeOptions) {
                        val selected = fullKeyboardMode == mode
                        Box(
                            modifier = Modifier
                                .weight(1f)
                                .height(26.dp)
                                .background(
                                    color = if (selected) MaterialTheme.colorScheme.primary else Color.Transparent,
                                    shape = RoundedCornerShape(4.dp)
                                )
                                .clickable {
                                    fullKeyboardMode = mode
                                    sharedPrefs.edit().putString("fullKeyboardMode", mode.name).apply()
                                },
                            contentAlignment = Alignment.Center
                        ) {
                            Text(
                                text = modeLabel,
                                color = if (selected) Color.Black else Color(0xFF94A3B8),
                                fontSize = 11.sp,
                                fontWeight = if (selected) FontWeight.Bold else FontWeight.Normal,
                                maxLines = 1
                            )
                        }
                    }
                }

                Column(
                    modifier = if (isAllScroll) {
                        Modifier.fillMaxWidth().horizontalScroll(allScrollState)
                    } else {
                        Modifier.fillMaxWidth()
                    },
                    verticalArrangement = Arrangement.spacedBy(if (isFitScreen) 4.dp else 6.dp)
                ) {
                    // Control & Media Row: Volume, Brightness, Play/Pause, Mouse Next/Prev, etc.
                    Row(
                        modifier = fullKeyRowModifier(),
                        horizontalArrangement = Arrangement.spacedBy(keySpacing)
                    ) {
                        val controlKeys = listOf(
                            "Mute" to Pair(0x20, true), "Vol-" to Pair(0x2E, true), "Vol+" to Pair(0x30, true),
                            "Prev" to Pair(0x10, true), "Play/Pause" to Pair(0x22, true), "Next" to Pair(0x19, true), "Stop" to Pair(0x24, true),
                            "Mic Mute" to Pair(0x65, true), // Extended Scancode for Mic Mute (varies by OS)
                            "Bri-" to Pair(0x66, true), "Bri+" to Pair(0x67, true), // Extended Scancodes for Brightness (Note: Often system-specific)
                            "Multi-Mon" to Pair(0x5D, true), // Extended Scancode for Display Switch (often E0 5D or similar)
                            "MousePrev" to Pair(0x6A, true), "MouseNext" to Pair(0x69, true), // Extended Scancodes for Browser Back (E0 6A) and Forward (E0 69)
                            "PrtScn" to Pair(0x37, true), "Insert" to Pair(0x52, true)
                        )
                        for ((label, scancodePair) in controlKeys) {
                            val weight = when {
                                label.length >= 9 -> 1.4f
                                label.length >= 6 -> 1.2f
                                else -> 1f
                            }
                            KeyboardButton(
                                label = label,
                                scancodePair = scancodePair,
                                keyHeight = keyHeight,
                                keyPadding = keyPadding,
                                fontSize = if (isFitScreen) fitKeyFontSize(label) else 11.sp,
                                minWidth = if (isFitScreen) 0.dp else 46.dp,
                                containerColor = Color(0xFF334155),
                                modifier = if (isFitScreen) Modifier.weight(weight) else Modifier
                            )
                        }
                    }

                    // Row 1: ESC, 1-0, BACKSP
                    Row(
                        modifier = fullKeyRowModifier(),
                        horizontalArrangement = Arrangement.spacedBy(keySpacing)
                    ) {
                        val row1Keys = listOf(
                            "ESC" to Pair(0x01, false), "`" to Pair(0x29, false), "1" to Pair(0x02, false), "2" to Pair(0x03, false),
                            "3" to Pair(0x04, false), "4" to Pair(0x05, false), "5" to Pair(0x06, false), "6" to Pair(0x07, false),
                            "7" to Pair(0x08, false), "8" to Pair(0x09, false), "9" to Pair(0x0A, false), "0" to Pair(0x0B, false),
                            "-" to Pair(0x0C, false), "=" to Pair(0x0D, false), "BACKSP" to Pair(0x0E, false)
                        )
                        for ((label, scancodePair) in row1Keys) {
                            val weight = when (label) {
                                "BACKSP" -> 1.6f
                                "ESC" -> 1.2f
                                else -> 1f
                            }
                            KeyboardButton(
                                label = keyLabel(label),
                                scancodePair = scancodePair,
                                keyHeight = keyHeight,
                                keyPadding = keyPadding,
                                fontSize = if (isFitScreen) fitKeyFontSize(label) else fontSize,
                                minWidth = if (isFitScreen) 0.dp else if (label == "BACKSP") 64.dp else 42.dp,
                                modifier = if (isFitScreen) Modifier.weight(weight) else Modifier
                            )
                        }
                    }

                    // Row 2: TAB, Q-P, [, ], \, DEL, HOME
                    Row(
                        modifier = fullKeyRowModifier(),
                        horizontalArrangement = Arrangement.spacedBy(keySpacing)
                    ) {
                        val row2Keys = listOf(
                            "TAB" to Pair(0x0F, false), "Q" to Pair(0x10, false), "W" to Pair(0x11, false), "E" to Pair(0x12, false),
                            "R" to Pair(0x13, false), "T" to Pair(0x14, false), "Y" to Pair(0x15, false), "U" to Pair(0x16, false),
                            "I" to Pair(0x17, false), "O" to Pair(0x18, false), "P" to Pair(0x19, false), "[" to Pair(0x1A, false),
                            "]" to Pair(0x1B, false), "\\" to Pair(0x2B, false), "DEL" to Pair(0x53, true), "HOME" to Pair(0x47, true)
                        )
                        for ((label, scancodePair) in row2Keys) {
                            val weight = when (label) {
                                "TAB" -> 1.4f
                                "HOME" -> 1.3f
                                "DEL" -> 1.2f
                                else -> 1f
                            }
                            KeyboardButton(
                                label = keyLabel(label),
                                scancodePair = scancodePair,
                                keyHeight = keyHeight,
                                keyPadding = keyPadding,
                                fontSize = if (isFitScreen) fitKeyFontSize(label) else fontSize,
                                minWidth = if (isFitScreen) 0.dp else if (label == "TAB" || label == "HOME") 60.dp else 42.dp,
                                modifier = if (isFitScreen) Modifier.weight(weight) else Modifier
                            )
                        }
                    }

                    // Row 3: CAPS, A-L, ;, ', ENTER, END, PGUP
                    Row(
                        modifier = fullKeyRowModifier(),
                        horizontalArrangement = Arrangement.spacedBy(keySpacing),
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        // Caps Lock (Toggle)
                        var isCapsPressed by remember { mutableStateOf(false) }
                        KeyboardModifierButton(
                            label = "CAPS",
                            isPressed = isCapsPressed,
                            onClick = {
                                isCapsPressed = !isCapsPressed
                                RdpClient.sendScancodeEvent(0x3A, false, 1)
                                RdpClient.sendScancodeEvent(0x3A, false, 0)
                            },
                            keyHeight = keyHeight,
                            keyPadding = keyPadding,
                            fontSize = if (isFitScreen) fitKeyFontSize("CAPS") else fontSize,
                            minWidth = if (isFitScreen) 0.dp else 64.dp,
                            boldWhenActive = false,
                            modifier = if (isFitScreen) Modifier.weight(1.5f) else Modifier
                        )

                        val row3Keys = listOf(
                            "A" to Pair(0x1E, false), "S" to Pair(0x1F, false), "D" to Pair(0x20, false), "F" to Pair(0x21, false),
                            "G" to Pair(0x22, false), "H" to Pair(0x23, false), "J" to Pair(0x24, false), "K" to Pair(0x25, false),
                            "L" to Pair(0x26, false), ";" to Pair(0x27, false), "'" to Pair(0x28, false), "ENTER" to Pair(0x1C, false),
                            "END" to Pair(0x4F, true), "PGUP" to Pair(0x49, true)
                        )
                        for ((label, scancodePair) in row3Keys) {
                            val weight = when (label) {
                                "ENTER" -> 1.6f
                                "PGUP" -> 1.3f
                                "END" -> 1.2f
                                else -> 1f
                            }
                            KeyboardButton(
                                label = keyLabel(label),
                                scancodePair = scancodePair,
                                keyHeight = keyHeight,
                                keyPadding = keyPadding,
                                fontSize = if (isFitScreen) fitKeyFontSize(label) else fontSize,
                                minWidth = if (isFitScreen) 0.dp else if (label == "ENTER" || label == "PGUP") 64.dp else 42.dp,
                                modifier = if (isFitScreen) Modifier.weight(weight) else Modifier
                            )
                        }
                    }

                    // Row 4: SHIFT, Z-M, ,, ., /, UP, PGDN
                    Row(
                        modifier = fullKeyRowModifier(),
                        horizontalArrangement = Arrangement.spacedBy(keySpacing),
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        // Shift Toggle
                        KeyboardModifierButton(
                            label = "Shift",
                            isPressed = isShiftPressed,
                            onClick = {
                                isShiftPressed = !isShiftPressed
                                RdpClient.sendScancodeEvent(0x2A, false, if (isShiftPressed) 1 else 0)
                            },
                            keyHeight = keyHeight,
                            keyPadding = keyPadding,
                            fontSize = if (isFitScreen) fitKeyFontSize("Shift") else fontSize,
                            minWidth = if (isFitScreen) 0.dp else 64.dp,
                            modifier = if (isFitScreen) Modifier.weight(1.8f) else Modifier
                        )

                        val row4Keys = listOf(
                            "Z" to Pair(0x2C, false), "X" to Pair(0x2D, false), "C" to Pair(0x2E, false), "V" to Pair(0x2F, false),
                            "B" to Pair(0x30, false), "N" to Pair(0x31, false), "M" to Pair(0x32, false), "," to Pair(0x33, false),
                            "." to Pair(0x34, false), "/" to Pair(0x35, false), "▲" to Pair(0x48, true), "PGDN" to Pair(0x51, true)
                        )
                        for ((label, scancodePair) in row4Keys) {
                            val weight = if (label == "PGDN") 1.4f else 1f
                            KeyboardButton(
                                label = keyLabel(label),
                                scancodePair = scancodePair,
                                keyHeight = keyHeight,
                                keyPadding = keyPadding,
                                fontSize = if (isFitScreen) fitKeyFontSize(label) else fontSize,
                                minWidth = if (isFitScreen) 0.dp else if (label == "PGDN") 64.dp else 42.dp,
                                modifier = if (isFitScreen) Modifier.weight(weight) else Modifier
                            )
                        }
                    }

                    // Row 5: Modifiers, Space, Arrows, F1-F12
                    Row(
                        modifier = fullKeyRowModifier(),
                        horizontalArrangement = Arrangement.spacedBy(keySpacing),
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        // Ctrl
                        KeyboardModifierButton(
                            label = "Ctrl",
                            isPressed = isCtrlPressed,
                            onClick = {
                                isCtrlPressed = !isCtrlPressed
                                RdpClient.sendScancodeEvent(0x1D, false, if (isCtrlPressed) 1 else 0)
                            },
                            keyHeight = keyHeight,
                            keyPadding = keyPadding,
                            fontSize = if (isFitScreen) fitKeyFontSize("Ctrl") else fontSize,
                            minWidth = if (isFitScreen) 0.dp else 54.dp,
                            modifier = if (isFitScreen) Modifier.weight(1.1f) else Modifier
                        )

                        // Win
                        KeyboardModifierButton(
                            label = "Win",
                            isPressed = isWinPressed,
                            onClick = {
                                isWinPressed = !isWinPressed
                                RdpClient.sendScancodeEvent(0x5B, true, if (isWinPressed) 1 else 0)
                            },
                            keyHeight = keyHeight,
                            keyPadding = keyPadding,
                            fontSize = if (isFitScreen) fitKeyFontSize("Win") else fontSize,
                            minWidth = if (isFitScreen) 0.dp else 54.dp,
                            modifier = if (isFitScreen) Modifier.weight(1.1f) else Modifier
                        )

                        // Alt
                        KeyboardModifierButton(
                            label = "Alt",
                            isPressed = isAltPressed,
                            onClick = {
                                isAltPressed = !isAltPressed
                                RdpClient.sendScancodeEvent(0x38, false, if (isAltPressed) 1 else 0)
                            },
                            keyHeight = keyHeight,
                            keyPadding = keyPadding,
                            fontSize = if (isFitScreen) fitKeyFontSize("Alt") else fontSize,
                            minWidth = if (isFitScreen) 0.dp else 54.dp,
                            modifier = if (isFitScreen) Modifier.weight(1.1f) else Modifier
                        )

                        // Space
                        KeyboardButton(
                            label = "SPACE",
                            scancodePair = Pair(0x39, false),
                            keyHeight = keyHeight,
                            keyPadding = keyPadding,
                            fontSize = if (isFitScreen) fitKeyFontSize("SPACE") else fontSize,
                            minWidth = if (isFitScreen) 0.dp else 160.dp,
                            modifier = if (isFitScreen) Modifier.weight(2.5f) else Modifier
                        )

                        if (!isFitScreen) {
                            Spacer(modifier = Modifier.width(4.dp))
                        }

                        // Left, Down, Right Arrows
                        val arrows = listOf("◀" to Pair(0x4B, true), "▼" to Pair(0x50, true), "▶" to Pair(0x4D, true))
                        for ((label, scancodePair) in arrows) {
                            KeyboardButton(
                                label = label,
                                scancodePair = scancodePair,
                                keyHeight = keyHeight,
                                keyPadding = keyPadding,
                                fontSize = if (isFitScreen) fitKeyFontSize(label) else 16.sp,
                                minWidth = if (isFitScreen) 0.dp else 42.dp,
                                modifier = if (isFitScreen) Modifier.weight(1f) else Modifier
                            )
                        }

                        if (!isFitScreen) {
                            Spacer(modifier = Modifier.width(4.dp))
                        }

                        // F1-F12
                        val fKeys = listOf(
                            "F1" to Pair(0x3B, false), "F2" to Pair(0x3C, false), "F3" to Pair(0x3D, false), "F4" to Pair(0x3E, false),
                            "F5" to Pair(0x3F, false), "F6" to Pair(0x40, false), "F7" to Pair(0x41, false), "F8" to Pair(0x42, false),
                            "F9" to Pair(0x43, false), "F10" to Pair(0x44, false), "F11" to Pair(0x57, false), "F12" to Pair(0x58, false)
                        )
                        for ((label, scancodePair) in fKeys) {
                            KeyboardButton(
                                label = label,
                                scancodePair = scancodePair,
                                keyHeight = keyHeight,
                                keyPadding = keyPadding,
                                fontSize = if (isFitScreen) fitKeyFontSize(label) else fontSize,
                                minWidth = if (isFitScreen) 0.dp else 42.dp,
                                modifier = if (isFitScreen) Modifier.weight(1f) else Modifier
                            )
                        }
                    }
                }
            }
        }

        // Keyboard text sender panel
        if (showKeyboardInput) {
            Column(
                modifier = Modifier
                    .align(Alignment.BottomCenter)
                    .fillMaxWidth()
                    .background(Color(0xE60F1219))
                    .border(1.dp, Color(0xFF334155), RoundedCornerShape(topStart = 16.dp, topEnd = 16.dp))
                    .padding(16.dp)
            ) {
                // First Row: Input text field & Send button
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier.fillMaxWidth()
                ) {
                    OutlinedTextField(
                        value = keyboardInput,
                        onValueChange = { keyboardInput = it },
                        label = { Text("Type here to send keystrokes") },
                        singleLine = true,
                        keyboardOptions = KeyboardOptions(imeAction = ImeAction.Send),
                        keyboardActions = KeyboardActions(
                            onSend = {
                                if (keyboardInput.isNotEmpty()) {
                                    // Send characters to Rust backend
                                    for (char in keyboardInput) {
                                        RdpClient.sendKeyEvent(char.code, 1) // Down
                                        RdpClient.sendKeyEvent(char.code, 0) // Up
                                    }
                                    keyboardInput = ""
                                }
                            }
                        ),
                        modifier = Modifier
                            .weight(1f)
                            .onFocusChanged { isInputFocused = it.isFocused },
                        colors = OutlinedTextFieldDefaults.colors(
                            focusedBorderColor = MaterialTheme.colorScheme.primary,
                            unfocusedBorderColor = Color(0xFF475569)
                        )
                    )
                    Spacer(modifier = Modifier.width(12.dp))
                    HighlightButton(
                        onClick = {
                            if (keyboardInput.isNotEmpty()) {
                                for (char in keyboardInput) {
                                    RdpClient.sendKeyEvent(char.code, 1)
                                    RdpClient.sendKeyEvent(char.code, 0)
                                }
                                keyboardInput = ""
                            }
                        },
                        containerColor = MaterialTheme.colorScheme.primary,
                        pressedColor = Color(0xFF818CF8),
                        shape = RoundedCornerShape(8.dp)
                    ) {
                        Text("Send", color = Color.Black, fontWeight = FontWeight.Bold)
                    }
                }

                Spacer(modifier = Modifier.height(8.dp))

                // Second Row: Special controls (Esc, Tab, Del, Backspace, Enter)
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(6.dp)
                ) {
                    // Esc button
                    HighlightButton(
                        onClick = {
                            RdpClient.sendScancodeEvent(0x01, false, 1)
                            RdpClient.sendScancodeEvent(0x01, false, 0)
                        },
                        containerColor = Color(0xFF1E293B),
                        contentPadding = PaddingValues(horizontal = 10.dp, vertical = 4.dp),
                        modifier = Modifier.weight(1f),
                        shape = RoundedCornerShape(8.dp)
                    ) {
                        Text("ESC", fontSize = 11.sp, color = Color.White)
                    }

                    // Tab button
                    HighlightButton(
                        onClick = {
                            RdpClient.sendScancodeEvent(0x0F, false, 1)
                            RdpClient.sendScancodeEvent(0x0F, false, 0)
                        },
                        containerColor = Color(0xFF1E293B),
                        contentPadding = PaddingValues(horizontal = 10.dp, vertical = 4.dp),
                        modifier = Modifier.weight(1f),
                        shape = RoundedCornerShape(8.dp)
                    ) {
                        Text("TAB", fontSize = 11.sp, color = Color.White)
                    }

                    // Del button
                    HighlightButton(
                        onClick = {
                            RdpClient.sendScancodeEvent(0x53, true, 1)
                            RdpClient.sendScancodeEvent(0x53, true, 0)
                        },
                        containerColor = Color(0xFF1E293B),
                        contentPadding = PaddingValues(horizontal = 10.dp, vertical = 4.dp),
                        modifier = Modifier.weight(1f),
                        shape = RoundedCornerShape(8.dp)
                    ) {
                        Text("DEL", fontSize = 11.sp, color = Color.White)
                    }

                    // Backspace button
                    HighlightButton(
                        onClick = {
                            RdpClient.sendKeyEvent(8, 1) // Backspace keycode
                            RdpClient.sendKeyEvent(8, 0)
                        },
                        containerColor = Color(0xFF334155),
                        contentPadding = PaddingValues(horizontal = 10.dp, vertical = 4.dp),
                        modifier = Modifier.weight(1.2f),
                        shape = RoundedCornerShape(8.dp)
                    ) {
                        Text("Backsp", fontSize = 11.sp, color = Color.White)
                    }

                    // Enter button
                    HighlightButton(
                        onClick = {
                            RdpClient.sendKeyEvent(13, 1) // Enter keycode
                            RdpClient.sendKeyEvent(13, 0)
                        },
                        containerColor = MaterialTheme.colorScheme.secondary,
                        pressedColor = MaterialTheme.colorScheme.primary,
                        contentPadding = PaddingValues(horizontal = 10.dp, vertical = 4.dp),
                        modifier = Modifier.weight(1.2f),
                        shape = RoundedCornerShape(8.dp)
                    ) {
                        Text("Enter", fontSize = 11.sp, color = Color.White)
                    }
                }

                Spacer(modifier = Modifier.height(8.dp))

                // Third Row: Modifiers and Arrows
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    // Modifiers
                    Row(
                        horizontalArrangement = Arrangement.spacedBy(4.dp),
                        modifier = Modifier.weight(1.8f)
                    ) {
                        // Ctrl Toggle
                        val ctrlColor = if (isCtrlPressed) MaterialTheme.colorScheme.primary else Color(0xFF1E293B)
                        val ctrlTextColor = if (isCtrlPressed) Color.Black else Color.White
                        Button(
                            onClick = {
                                isCtrlPressed = !isCtrlPressed
                                RdpClient.sendScancodeEvent(0x1D, false, if (isCtrlPressed) 1 else 0)
                            },
                            colors = ButtonDefaults.buttonColors(containerColor = ctrlColor),
                            contentPadding = PaddingValues(horizontal = 6.dp, vertical = 4.dp),
                            modifier = Modifier.weight(1f),
                            shape = RoundedCornerShape(8.dp)
                        ) {
                            Text("Ctrl", fontSize = 11.sp, color = ctrlTextColor, fontWeight = if (isCtrlPressed) FontWeight.Bold else FontWeight.Normal)
                        }

                        // Alt Toggle
                        val altColor = if (isAltPressed) MaterialTheme.colorScheme.primary else Color(0xFF1E293B)
                        val altTextColor = if (isAltPressed) Color.Black else Color.White
                        Button(
                            onClick = {
                                isAltPressed = !isAltPressed
                                RdpClient.sendScancodeEvent(0x38, false, if (isAltPressed) 1 else 0)
                            },
                            colors = ButtonDefaults.buttonColors(containerColor = altColor),
                            contentPadding = PaddingValues(horizontal = 6.dp, vertical = 4.dp),
                            modifier = Modifier.weight(1f),
                            shape = RoundedCornerShape(8.dp)
                        ) {
                            Text("Alt", fontSize = 11.sp, color = altTextColor, fontWeight = if (isAltPressed) FontWeight.Bold else FontWeight.Normal)
                        }

                        // Shift Toggle
                        val shiftColor = if (isShiftPressed) MaterialTheme.colorScheme.primary else Color(0xFF1E293B)
                        val shiftTextColor = if (isShiftPressed) Color.Black else Color.White
                        Button(
                            onClick = {
                                isShiftPressed = !isShiftPressed
                                RdpClient.sendScancodeEvent(0x2A, false, if (isShiftPressed) 1 else 0)
                            },
                            colors = ButtonDefaults.buttonColors(containerColor = shiftColor),
                            contentPadding = PaddingValues(horizontal = 6.dp, vertical = 4.dp),
                            modifier = Modifier.weight(1f),
                            shape = RoundedCornerShape(8.dp)
                        ) {
                            Text("Shift", fontSize = 11.sp, color = shiftTextColor, fontWeight = if (isShiftPressed) FontWeight.Bold else FontWeight.Normal)
                        }

                        // Win Toggle
                        val winColor = if (isWinPressed) MaterialTheme.colorScheme.primary else Color(0xFF1E293B)
                        val winTextColor = if (isWinPressed) Color.Black else Color.White
                        Button(
                            onClick = {
                                isWinPressed = !isWinPressed
                                RdpClient.sendScancodeEvent(0x5B, true, if (isWinPressed) 1 else 0)
                            },
                            colors = ButtonDefaults.buttonColors(containerColor = winColor),
                            contentPadding = PaddingValues(horizontal = 6.dp, vertical = 4.dp),
                            modifier = Modifier.weight(1f),
                            shape = RoundedCornerShape(8.dp)
                        ) {
                            Text("Win", fontSize = 11.sp, color = winTextColor, fontWeight = if (isWinPressed) FontWeight.Bold else FontWeight.Normal)
                        }
                    }

                    Spacer(modifier = Modifier.width(12.dp))

                    // Arrow Keys Grid
                    Row(
                        horizontalArrangement = Arrangement.spacedBy(4.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        modifier = Modifier.weight(1.2f)
                    ) {
                        // Left
                        HighlightButton(
                            onClick = {
                                RdpClient.sendScancodeEvent(0x4B, true, 1)
                                RdpClient.sendScancodeEvent(0x4B, true, 0)
                            },
                            containerColor = Color(0xFF1E293B),
                            contentPadding = PaddingValues(0.dp),
                            modifier = Modifier.size(36.dp),
                            shape = RoundedCornerShape(8.dp)
                        ) {
                            Text("◀", fontSize = 12.sp, color = Color.White)
                        }

                        // Up and Down Column
                        Column(
                            verticalArrangement = Arrangement.spacedBy(4.dp),
                            horizontalAlignment = Alignment.CenterHorizontally
                        ) {
                            HighlightButton(
                                onClick = {
                                    RdpClient.sendScancodeEvent(0x48, true, 1)
                                    RdpClient.sendScancodeEvent(0x48, true, 0)
                                },
                                containerColor = Color(0xFF1E293B),
                                contentPadding = PaddingValues(0.dp),
                                modifier = Modifier.size(36.dp),
                                shape = RoundedCornerShape(8.dp)
                            ) {
                                Text("▲", fontSize = 12.sp, color = Color.White)
                            }
                            HighlightButton(
                                onClick = {
                                    RdpClient.sendScancodeEvent(0x50, true, 1)
                                    RdpClient.sendScancodeEvent(0x50, true, 0)
                                },
                                containerColor = Color(0xFF1E293B),
                                contentPadding = PaddingValues(0.dp),
                                modifier = Modifier.size(36.dp),
                                shape = RoundedCornerShape(8.dp)
                            ) {
                                Text("▼", fontSize = 12.sp, color = Color.White)
                            }
                        }

                        // Right
                        HighlightButton(
                            onClick = {
                                RdpClient.sendScancodeEvent(0x4D, true, 1)
                                RdpClient.sendScancodeEvent(0x4D, true, 0)
                            },
                            containerColor = Color(0xFF1E293B),
                            contentPadding = PaddingValues(0.dp),
                            modifier = Modifier.size(36.dp),
                            shape = RoundedCornerShape(8.dp)
                        ) {
                            Text("▶", fontSize = 12.sp, color = Color.White)
                        }
                    }
                }
            }
        }
    }
}
