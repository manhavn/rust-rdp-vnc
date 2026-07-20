mod keys;

use eframe::egui::{self, Align, Color32, ColorImage, Key, Layout, RichText, TextureHandle, TextureOptions, Vec2};
use parking_lot::Mutex;
use rust_rdp::{
    connect_session, disconnect_session, disconnect_session_id, init_runtime, send_key_event,
    send_mouse_event, send_mouse_wheel_event, send_scancode_event, set_active_session,
    SessionCallback,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use keys::{egui_key_to_scancode, is_extended_scancode};

// ── Desktop chrome palette (neutral, not mobile-neon) ───────────────────────
mod theme {
    use eframe::egui::Color32;

    pub const BG: Color32 = Color32::from_rgb(0x24, 0x24, 0x24);
    pub const PANEL: Color32 = Color32::from_rgb(0x2D, 0x2D, 0x2D);
    pub const PANEL_ALT: Color32 = Color32::from_rgb(0x33, 0x33, 0x33);
    pub const BORDER: Color32 = Color32::from_rgb(0x45, 0x45, 0x45);
    pub const TEXT: Color32 = Color32::from_rgb(0xE8, 0xE8, 0xE8);
    pub const TEXT_DIM: Color32 = Color32::from_rgb(0xA0, 0xA0, 0xA0);
    pub const ACCENT: Color32 = Color32::from_rgb(0x35, 0x84, 0xE4);
    pub const ACCENT_HOVER: Color32 = Color32::from_rgb(0x4A, 0x94, 0xF0);
    pub const DANGER: Color32 = Color32::from_rgb(0xE0, 0x1B, 0x24);
    pub const SUCCESS: Color32 = Color32::from_rgb(0x2E, 0xC2, 0x7E);
    pub const WARN: Color32 = Color32::from_rgb(0xF5, 0xC2, 0x11);
    pub const CANVAS: Color32 = Color32::from_rgb(0x12, 0x12, 0x12);
    pub const ERROR_BG: Color32 = Color32::from_rgb(0x3D, 0x1A, 0x1A);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ConnectionState {
    Idle,
    Connecting,
    Connected,
    Failed,
}

impl ConnectionState {
    fn label(self) -> &'static str {
        match self {
            Self::Idle => "Disconnected",
            Self::Connecting => "Connecting…",
            Self::Connected => "Connected",
            Self::Failed => "Failed",
        }
    }

    fn color(self) -> Color32 {
        match self {
            Self::Idle => theme::TEXT_DIM,
            Self::Connecting => theme::WARN,
            Self::Connected => theme::SUCCESS,
            Self::Failed => theme::DANGER,
        }
    }
}

struct FrameBuffer {
    width: i32,
    height: i32,
    pixels: Vec<i32>,
    generation: u64,
}

impl FrameBuffer {
    fn new(width: i32, height: i32) -> Self {
        let size = (width.max(1) * height.max(1)) as usize;
        Self {
            width,
            height,
            pixels: vec![0xFF_12_12_12u32 as i32; size],
            generation: 0,
        }
    }

    fn resize(&mut self, width: i32, height: i32) {
        self.width = width.max(1);
        self.height = height.max(1);
        self.pixels = vec![0xFF_12_12_12u32 as i32; (self.width * self.height) as usize];
        self.generation = self.generation.wrapping_add(1);
    }

    fn set_frame(&mut self, pixels: &[i32], width: i32, height: i32) {
        if width != self.width || height != self.height {
            self.resize(width, height);
        }
        let n = (self.width * self.height) as usize;
        let copy_len = pixels.len().min(n);
        self.pixels[..copy_len].copy_from_slice(&pixels[..copy_len]);
        self.generation = self.generation.wrapping_add(1);
    }

    fn to_color_image(&self) -> ColorImage {
        let w = self.width as usize;
        let h = self.height as usize;
        let mut rgba = vec![0u8; w * h * 4];
        for (i, px) in self.pixels.iter().take(w * h).enumerate() {
            let v = *px as u32;
            let a = ((v >> 24) & 0xFF) as u8;
            let r = ((v >> 16) & 0xFF) as u8;
            let g = ((v >> 8) & 0xFF) as u8;
            let b = (v & 0xFF) as u8;
            let o = i * 4;
            rgba[o] = r;
            rgba[o + 1] = g;
            rgba[o + 2] = b;
            rgba[o + 3] = if a == 0 { 255 } else { a };
        }
        ColorImage::from_rgba_unmultiplied([w, h], &rgba)
    }
}

struct SharedUi {
    state: Mutex<ConnectionState>,
    status: Mutex<String>,
    frame: Mutex<FrameBuffer>,
    dirty: AtomicBool,
}

impl SharedUi {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(ConnectionState::Idle),
            status: Mutex::new("Ready".into()),
            frame: Mutex::new(FrameBuffer::new(1280, 720)),
            dirty: AtomicBool::new(false),
        })
    }
}

struct UiCallback {
    ui: Arc<SharedUi>,
}

impl SessionCallback for UiCallback {
    fn on_state_changed(&self, state: i32, message: &str) {
        let conn = match state {
            1 => ConnectionState::Connecting,
            2 => ConnectionState::Connected,
            3 => ConnectionState::Failed,
            _ => ConnectionState::Idle,
        };
        *self.ui.state.lock() = conn;
        *self.ui.status.lock() = message.to_string();
        log::info!("state={state} msg={message}");
    }

    fn on_frame_decoded(&self, pixels: &[i32], _x: i32, _y: i32, width: i32, height: i32) {
        self.ui.frame.lock().set_frame(pixels, width, height);
        self.ui.dirty.store(true, Ordering::Relaxed);
    }

    fn on_resolution_changed(&self, width: i32, height: i32) {
        self.ui.frame.lock().resize(width, height);
        self.ui.dirty.store(true, Ordering::Relaxed);
        log::info!("resolution -> {width}x{height}");
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FitMode {
    /// Scale to fit the window while preserving aspect ratio
    Fit,
    /// 1:1 pixels (may scroll if larger than window)
    Actual,
    /// Stretch to fill the viewport (ignore aspect)
    Stretch,
}

#[derive(Clone)]
struct Prefs {
    host: String,
    port: String,
    username: String,
    password: String,
    domain: String,
    mode: String,
    width: String,
    height: String,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: "3389".into(),
            username: String::new(),
            password: String::new(),
            domain: String::new(),
            mode: "RDP".into(),
            width: "1920".into(),
            height: "1080".into(),
        }
    }
}

impl Prefs {
    fn path() -> Option<PathBuf> {
        directories::ProjectDirs::from("com", "rustai", "rust-rdp")
            .map(|d| d.config_dir().join("prefs.txt"))
    }

    fn load() -> Self {
        let mut p = Self::default();
        let Some(path) = Self::path() else {
            return p;
        };
        let Ok(text) = std::fs::read_to_string(path) else {
            return p;
        };
        for line in text.lines() {
            if let Some((k, v)) = line.split_once('=') {
                match k {
                    "host" => p.host = v.to_string(),
                    "port" => p.port = v.to_string(),
                    "username" => p.username = v.to_string(),
                    "password" => p.password = v.to_string(),
                    "domain" => p.domain = v.to_string(),
                    "mode" => p.mode = v.to_string(),
                    "width" => p.width = v.to_string(),
                    "height" => p.height = v.to_string(),
                    _ => {}
                }
            }
        }
        p
    }

    /// Silent app prefs (last session) under XDG config.
    fn save_app_prefs(&self) {
        let Some(path) = Self::path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let text = format!(
            "host={}\nport={}\nusername={}\npassword={}\ndomain={}\nmode={}\nwidth={}\nheight={}\n",
            self.host,
            self.port,
            self.username,
            self.password,
            self.domain,
            self.mode,
            self.width,
            self.height
        );
        let _ = std::fs::write(path, text);
    }

    fn file_extension(&self) -> &'static str {
        if self.mode.eq_ignore_ascii_case("VNC") {
            "vnc"
        } else {
            "rdp"
        }
    }

    fn default_filename(&self) -> String {
        let base = if self.host.trim().is_empty() {
            "connection".to_string()
        } else {
            // Sanitize host for use as filename
            self.host
                .trim()
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect()
        };
        format!("{base}.{}", self.file_extension())
    }

    /// RDP/VNC connection file body (compatible with the Android client).
    fn to_connection_file(&self) -> String {
        let mut out = String::new();
        let host = self.host.trim();
        let port = self.port.trim();
        if port.is_empty() {
            out.push_str(&format!("full address:s:{host}\n"));
        } else {
            out.push_str(&format!("full address:s:{host}:{port}\n"));
        }
        if !self.username.is_empty() {
            out.push_str(&format!("username:s:{}\n", self.username));
        }
        if !self.password.is_empty() {
            out.push_str(&format!("password:s:{}\n", self.password));
        }
        if !self.domain.is_empty() {
            out.push_str(&format!("domain:s:{}\n", self.domain));
        }
        out.push_str(&format!("connection mode:s:{}\n", self.mode));
        if !self.width.is_empty() {
            out.push_str(&format!("desktopwidth:i:{}\n", self.width.trim()));
        }
        if !self.height.is_empty() {
            out.push_str(&format!("desktopheight:i:{}\n", self.height.trim()));
        }
        out
    }

    /// Ensure path uses the correct extension for the current protocol.
    fn with_correct_extension(&self, mut path: PathBuf) -> PathBuf {
        let want = self.file_extension();
        let current = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_ascii_lowercase());
        match current.as_deref() {
            Some(ext) if ext == want => {}
            Some("rdp" | "vnc") => {
                // Wrong protocol extension — replace with the active mode.
                path.set_extension(want);
            }
            Some(_) | None => {
                path.set_extension(want);
            }
        }
        path
    }

    fn endpoint_label(&self) -> String {
        if self.host.is_empty() {
            "—".into()
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    /// Parse a `.rdp` / `.vnc` connection file (Android-compatible format).
    /// Existing display size prefs are preserved when the file omits them.
    fn load_from_connection_file(path: &std::path::Path, base: &Self) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("Read failed: {e}"))?;

        let mut host_full = String::new();
        let mut username = String::new();
        let mut password = String::new();
        let mut domain = String::new();
        let mut mode = String::new();
        let mut width = base.width.clone();
        let mut height = base.height.clone();

        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
                continue;
            }
            // Standard "key:type:value" RDP lines
            if let Some((key, rest)) = line.split_once(':') {
                let key = key.trim().to_ascii_lowercase();
                // rest is like "s:value" or "i:123" or plain value
                let value = if let Some((ty, v)) = rest.split_once(':') {
                    let ty = ty.trim().to_ascii_lowercase();
                    if matches!(ty.as_str(), "s" | "i" | "b") {
                        v.trim()
                    } else {
                        // host:port style without type — rejoin
                        rest.trim()
                    }
                } else {
                    rest.trim()
                };

                match key.as_str() {
                    "full address" | "server" | "host" => host_full = value.to_string(),
                    "username" | "user name" => username = value.to_string(),
                    "password" => password = value.to_string(),
                    "domain" => domain = value.to_string(),
                    "connection mode" | "mode" | "protocol" => mode = value.to_string(),
                    "desktopwidth" | "screen mode width" | "width" => width = value.to_string(),
                    "desktopheight" | "screen mode height" | "height" => height = value.to_string(),
                    _ => {}
                }
            } else if let Some((k, v)) = line.split_once('=') {
                // Fallback simple key=value (our app prefs style)
                match k.trim() {
                    "host" => host_full = v.trim().to_string(),
                    "port" => {
                        if !host_full.contains(':') && !host_full.is_empty() {
                            host_full = format!("{}:{}", host_full, v.trim());
                        } else if host_full.is_empty() {
                            // ignore lone port
                        }
                    }
                    "username" => username = v.trim().to_string(),
                    "password" => password = v.trim().to_string(),
                    "domain" => domain = v.trim().to_string(),
                    "mode" => mode = v.trim().to_string(),
                    "width" => width = v.trim().to_string(),
                    "height" => height = v.trim().to_string(),
                    _ => {}
                }
            }
        }

        // Extension hint when mode not specified
        if mode.is_empty() {
            if path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("vnc"))
            {
                mode = "VNC".into();
            } else {
                mode = "RDP".into();
            }
        }
        let mode = if mode.to_ascii_uppercase().contains("VNC") {
            "VNC".to_string()
        } else {
            "RDP".to_string()
        };

        if host_full.trim().is_empty() {
            return Err("File has no host (full address)".into());
        }

        // Split host:port — last colon for IPv4 host:port (simple split)
        let host_full = host_full.trim();
        let (host, port) = if let Some((h, p)) = host_full.rsplit_once(':') {
            // Avoid treating bare IPv6 as host:port; only split if port is numeric
            if p.chars().all(|c| c.is_ascii_digit()) && !p.is_empty() {
                (h.to_string(), p.to_string())
            } else {
                (
                    host_full.to_string(),
                    if mode == "VNC" {
                        "5900".into()
                    } else {
                        "3389".into()
                    },
                )
            }
        } else {
            (
                host_full.to_string(),
                if mode == "VNC" {
                    "5900".into()
                } else {
                    "3389".into()
                },
            )
        };

        Ok(Self {
            host,
            port,
            username,
            password,
            domain,
            mode,
            width,
            height,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ToastKind {
    Success,
    Error,
    Info,
}

struct Toast {
    message: String,
    kind: ToastKind,
    until: Instant,
}

/// One connection form + optional live backend session (multi-tab).
struct ConnectionTab {
    /// Stable id for egui / texture names.
    tab_id: u64,
    prefs: Prefs,
    shared: Arc<SharedUi>,
    /// Backend session while connecting/connected (also kept after Failed until reconnect).
    backend_session_id: Option<u64>,
    texture: Option<TextureHandle>,
    last_frame_gen: u64,
    last_mouse: Option<(i32, i32)>,
    left_down: bool,
    right_down: bool,
    mod_shift: bool,
    mod_ctrl: bool,
    mod_alt: bool,
}

impl ConnectionTab {
    fn new(tab_id: u64, prefs: Prefs) -> Self {
        Self {
            tab_id,
            prefs,
            shared: SharedUi::new(),
            backend_session_id: None,
            texture: None,
            last_frame_gen: 0,
            last_mouse: None,
            left_down: false,
            right_down: false,
            mod_shift: false,
            mod_ctrl: false,
            mod_alt: false,
        }
    }

    fn tab_title(&self) -> String {
        let host = self.prefs.host.trim();
        if host.is_empty() {
            return "New connection".into();
        }
        let label = self.prefs.endpoint_label();
        match *self.shared.state.lock() {
            ConnectionState::Connecting => format!("… {label}"),
            ConnectionState::Failed => format!("! {label}"),
            _ => label,
        }
    }

    fn is_busy(&self) -> bool {
        matches!(
            *self.shared.state.lock(),
            ConnectionState::Connecting | ConnectionState::Connected
        )
    }

    fn can_connect(&self) -> bool {
        !self.prefs.host.trim().is_empty()
            && matches!(
                *self.shared.state.lock(),
                ConnectionState::Idle | ConnectionState::Failed
            )
    }

    fn can_open_file(&self) -> bool {
        matches!(
            *self.shared.state.lock(),
            ConnectionState::Idle | ConnectionState::Failed
        )
    }
}

struct DesktopApp {
    tabs: Vec<ConnectionTab>,
    active_tab: usize,
    next_tab_id: u64,

    // Desktop chrome state
    show_sidebar: bool,
    show_about: bool,
    fit_mode: FitMode,
    zoom: f32,
    /// OS window fullscreen (title bar / taskbar)
    window_fullscreen: bool,
    /// Session view fullscreen: hide menu/toolbar/sidebar/status so only the remote view remains
    view_fullscreen: bool,
    /// Sidebar visibility restored when leaving view fullscreen
    sidebar_before_view_fs: bool,
    /// Auto-hide floating exit hint after entering view fullscreen
    view_fs_hint_until: Option<Instant>,
    /// True when the remote surface should own keyboard (connected + hover/focus, or view FS).
    /// While true, host app shortcuts are disabled — only the host key works.
    remote_input_active: bool,
    toast: Option<Toast>,
    /// Tab id waiting for “close while connected?” confirmation (× / Ctrl+W).
    pending_close_tab_id: Option<u64>,
}

/// Host key (VirtualBox/VMware/Remmina style): leave remote keyboard / exit view fullscreen.
/// Combo: Ctrl + Alt + Enter
const HOST_KEY_HINT: &str = "Ctrl+Alt+Enter";

impl DesktopApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        init_runtime();
        apply_desktop_style(&cc.egui_ctx);

        let first = ConnectionTab::new(1, Prefs::load());
        Self {
            tabs: vec![first],
            active_tab: 0,
            next_tab_id: 2,
            show_sidebar: true,
            show_about: false,
            fit_mode: FitMode::Fit,
            zoom: 1.0,
            window_fullscreen: false,
            view_fullscreen: false,
            sidebar_before_view_fs: true,
            view_fs_hint_until: None,
            remote_input_active: false,
            toast: None,
            pending_close_tab_id: None,
        }
    }

    fn tab(&self) -> &ConnectionTab {
        &self.tabs[self.active_tab]
    }

    fn tab_mut(&mut self) -> &mut ConnectionTab {
        &mut self.tabs[self.active_tab]
    }

    fn select_tab(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        self.active_tab = index;
        if let Some(sid) = self.tabs[index].backend_session_id {
            set_active_session(sid);
        } else {
            set_active_session(0);
        }
    }

    /// Open a blank connection form in a new tab.
    fn new_connection_tab(&mut self) {
        let id = self.next_tab_id;
        self.next_tab_id = self.next_tab_id.wrapping_add(1);
        self.tabs.push(ConnectionTab::new(id, Prefs::default()));
        self.select_tab(self.tabs.len() - 1);
        self.show_sidebar = true;
        if self.view_fullscreen {
            // New form needs chrome; leave immersive mode.
            // Caller may pass ctx — handled where needed.
        }
    }

    /// Request closing a tab. Confirms first when the session is connecting/connected.
    fn request_close_tab(&mut self, index: usize, ctx: &egui::Context) {
        if index >= self.tabs.len() {
            return;
        }
        if self.tabs[index].is_busy() {
            self.pending_close_tab_id = Some(self.tabs[index].tab_id);
            // Ensure chrome is visible so the confirm dialog can be used.
            if self.view_fullscreen {
                self.exit_view_fullscreen(ctx);
            }
            return;
        }
        self.close_tab(index, ctx);
    }

    /// Close a tab: disconnect its backend session, keep at least one tab.
    fn close_tab(&mut self, index: usize, ctx: &egui::Context) {
        if index >= self.tabs.len() {
            return;
        }
        // Drop any pending confirm for this (or another) tab.
        self.pending_close_tab_id = None;

        let was_active = index == self.active_tab;
        let tab = &self.tabs[index];
        if let Some(sid) = tab.backend_session_id {
            disconnect_session_id(sid);
        }

        self.tabs.remove(index);

        if self.tabs.is_empty() {
            let id = self.next_tab_id;
            self.next_tab_id = self.next_tab_id.wrapping_add(1);
            self.tabs.push(ConnectionTab::new(id, Prefs::default()));
            self.active_tab = 0;
            set_active_session(0);
            self.show_sidebar = true;
            if self.view_fullscreen {
                self.exit_view_fullscreen(ctx);
            }
            return;
        }

        if index < self.active_tab {
            self.active_tab -= 1;
        } else if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }

        self.select_tab(self.active_tab);
        if was_active && self.view_fullscreen {
            let busy = self.tab().is_busy();
            if !busy {
                self.exit_view_fullscreen(ctx);
                self.show_sidebar = true;
            }
        }
    }

    /// Keyboard is owned by the remote session (no host shortcuts except host key).
    fn keyboard_grabbed(&self) -> bool {
        self.view_fullscreen || self.remote_input_active
    }

    fn is_host_key_pressed(i: &egui::InputState) -> bool {
        // Ctrl+Alt+Enter — intentional chord that rarely collides with desktop apps
        i.modifiers.ctrl
            && i.modifiers.alt
            && !i.modifiers.shift
            && i.key_pressed(Key::Enter)
    }

    fn is_host_key_chord(modifiers: egui::Modifiers, key: Key) -> bool {
        modifiers.ctrl && modifiers.alt && !modifiers.shift && key == Key::Enter
    }

    /// Hide app chrome so the remote desktop fills the client area.
    /// Also enters OS window fullscreen for an immersive session (like mstsc/Remmina).
    fn enter_view_fullscreen(&mut self, ctx: &egui::Context) {
        if self.view_fullscreen {
            return;
        }
        self.view_fullscreen = true;
        self.sidebar_before_view_fs = self.show_sidebar;
        self.show_sidebar = false;
        self.view_fs_hint_until = Some(Instant::now() + Duration::from_secs(5));
        if !self.window_fullscreen {
            self.window_fullscreen = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
        }
        self.show_toast(
            format!("View fullscreen — host key {HOST_KEY_HINT} to exit"),
            ToastKind::Info,
        );
    }

    fn exit_view_fullscreen(&mut self, ctx: &egui::Context) {
        if !self.view_fullscreen {
            return;
        }
        self.view_fullscreen = false;
        self.show_sidebar = self.sidebar_before_view_fs;
        self.view_fs_hint_until = None;
        if self.window_fullscreen {
            self.window_fullscreen = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        }
    }

    fn toggle_view_fullscreen(&mut self, ctx: &egui::Context) {
        if self.view_fullscreen {
            self.exit_view_fullscreen(ctx);
        } else {
            // Only useful while a session is active, but allow preview of chrome-less canvas
            self.enter_view_fullscreen(ctx);
        }
    }

    fn set_window_fullscreen(&mut self, ctx: &egui::Context, on: bool) {
        self.window_fullscreen = on;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(on));
    }

    fn show_toast(&mut self, message: impl Into<String>, kind: ToastKind) {
        self.toast = Some(Toast {
            message: message.into(),
            kind,
            until: Instant::now() + Duration::from_secs(3),
        });
    }

    /// Open a native file picker, load `.rdp` / `.vnc`, then connect.
    /// If the current tab is busy, opens a new tab first.
    fn open_connection(&mut self) {
        if !self.tab().can_open_file() {
            self.new_connection_tab();
        }

        let Some(path) = rfd::FileDialog::new()
            .set_title("Open connection")
            .add_filter("Connection files", &["rdp", "vnc"])
            .add_filter("RDP connection", &["rdp"])
            .add_filter("VNC connection", &["vnc"])
            .add_filter("All files", &["*"])
            .pick_file()
        else {
            return;
        };

        let base = self.tab().prefs.clone();
        match Prefs::load_from_connection_file(&path, &base) {
            Ok(loaded) => {
                let msg = format!(
                    "Loaded {} — connecting…",
                    path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("connection")
                );
                {
                    let tab = self.tab_mut();
                    tab.prefs = loaded;
                    tab.prefs.save_app_prefs();
                    *tab.shared.status.lock() = msg.clone();
                }
                self.show_toast(msg, ToastKind::Info);
                self.start_connect();
            }
            Err(e) => {
                let msg = format!("Could not open file: {e}");
                *self.tab_mut().shared.status.lock() = msg.clone();
                self.show_toast(msg, ToastKind::Error);
            }
        }
    }

    /// Open a native Save dialog so the user can pick path + filename.
    /// RDP → `*.rdp`, VNC → `*.vnc`. Cancel is silent.
    fn save_connection_as(&mut self) {
        let ext = self.tab().prefs.file_extension();
        let filter_label = if ext == "vnc" {
            "VNC connection"
        } else {
            "RDP connection"
        };
        let default_name = self.tab().prefs.default_filename();

        let Some(picked) = rfd::FileDialog::new()
            .set_title("Save connection")
            .set_file_name(&default_name)
            .add_filter(filter_label, &[ext])
            .add_filter("All files", &["*"])
            .save_file()
        else {
            // User cancelled the dialog
            return;
        };

        let path = self.tab().prefs.with_correct_extension(picked);
        let body = self.tab().prefs.to_connection_file();

        match std::fs::write(&path, body) {
            Ok(()) => {
                // Also refresh local app prefs so next launch remembers fields
                self.tab_mut().prefs.save_app_prefs();
                let msg = format!("Connection saved to {}", path.display());
                *self.tab_mut().shared.status.lock() = msg.clone();
                self.show_toast(msg, ToastKind::Success);
            }
            Err(e) => {
                let msg = format!("Could not save file: {e}");
                *self.tab_mut().shared.status.lock() = msg.clone();
                self.show_toast(msg, ToastKind::Error);
            }
        }
    }

    fn is_busy(&self) -> bool {
        self.tab().is_busy()
    }

    fn can_connect(&self) -> bool {
        self.tab().can_connect()
    }

    /// Reset all connection form fields to defaults and persist app prefs.
    fn clear_form(&mut self) {
        if self.is_busy() {
            return;
        }
        let tab = self.tab_mut();
        tab.prefs = Prefs::default();
        tab.prefs.save_app_prefs();
        *tab.shared.status.lock() = "Form cleared".into();
        self.show_toast("Form cleared", ToastKind::Info);
    }

    fn sync_modifiers(&mut self, modifiers: egui::Modifiers) {
        let tab = self.tab_mut();
        if modifiers.shift != tab.mod_shift {
            send_scancode_event(0x2A, false, if modifiers.shift { 1 } else { 0 });
            tab.mod_shift = modifiers.shift;
        }
        if modifiers.ctrl != tab.mod_ctrl {
            send_scancode_event(0x1D, false, if modifiers.ctrl { 1 } else { 0 });
            tab.mod_ctrl = modifiers.ctrl;
        }
        if modifiers.alt != tab.mod_alt {
            send_scancode_event(0x38, false, if modifiers.alt { 1 } else { 0 });
            tab.mod_alt = modifiers.alt;
        }
    }

    fn start_connect(&mut self) {
        if !self.can_connect() {
            return;
        }

        // Drop any leftover backend session from a prior Failed attempt on this tab.
        if let Some(old) = self.tab_mut().backend_session_id.take() {
            disconnect_session_id(old);
        }

        self.tab_mut().prefs.save_app_prefs();

        let (host, port, username, password, domain, mode, width, height, endpoint, shared) = {
            let tab = self.tab();
            let default_port = if tab.prefs.mode == "VNC" { 5900 } else { 3389 };
            let port = tab.prefs.port.parse::<i32>().unwrap_or(default_port);
            let width = tab.prefs.width.parse::<i32>().unwrap_or(1920).clamp(640, 7680);
            let height = tab
                .prefs
                .height
                .parse::<i32>()
                .unwrap_or(1080)
                .clamp(480, 4320);
            (
                tab.prefs.host.trim().to_string(),
                port,
                tab.prefs.username.clone(),
                tab.prefs.password.clone(),
                tab.prefs.domain.clone(),
                tab.prefs.mode.clone(),
                width,
                height,
                tab.prefs.endpoint_label(),
                tab.shared.clone(),
            )
        };

        {
            let mut frame = shared.frame.lock();
            frame.resize(width, height);
        }
        *shared.state.lock() = ConnectionState::Connecting;
        *shared.status.lock() = format!("Connecting to {endpoint} via {mode}…");

        let cb: Arc<dyn SessionCallback> = Arc::new(UiCallback {
            ui: shared.clone(),
        });

        let session_id = connect_session(
            host, port, username, password, domain, width, height, mode, cb,
        );
        self.tab_mut().backend_session_id = Some(session_id);
    }

    /// Disconnect the active tab's session (keeps the tab / form).
    fn disconnect(&mut self, ctx: &egui::Context) {
        if let Some(sid) = self.tab_mut().backend_session_id.take() {
            disconnect_session_id(sid);
        }
        let tab = self.tab_mut();
        *tab.shared.state.lock() = ConnectionState::Idle;
        *tab.shared.status.lock() = "Disconnected".into();
        tab.left_down = false;
        tab.right_down = false;
        tab.last_mouse = None;
        if self.view_fullscreen {
            self.exit_view_fullscreen(ctx);
        }
        self.show_sidebar = true;
    }

    fn ensure_texture(&mut self, ctx: &egui::Context) {
        let tab = self.tab_mut();
        if !tab.shared.dirty.swap(false, Ordering::Relaxed) && tab.texture.is_some() {
            return;
        }
        let frame = tab.shared.frame.lock();
        if frame.generation == tab.last_frame_gen && tab.texture.is_some() {
            return;
        }
        tab.last_frame_gen = frame.generation;
        let image = frame.to_color_image();
        drop(frame);

        let tex_name = format!("rdp_frame_{}", tab.tab_id);
        match &mut tab.texture {
            Some(tex) => tex.set(image, TextureOptions::LINEAR),
            None => {
                tab.texture = Some(ctx.load_texture(tex_name, image, TextureOptions::LINEAR));
            }
        }
    }

    fn remote_pos(&self, pointer: egui::Pos2, rect: egui::Rect) -> Option<(i32, i32)> {
        if !rect.contains(pointer) {
            return None;
        }
        let frame = self.tab().shared.frame.lock();
        let fw = frame.width as f32;
        let fh = frame.height as f32;
        if fw <= 0.0 || fh <= 0.0 || rect.width() <= 0.0 || rect.height() <= 0.0 {
            return None;
        }
        let local = pointer - rect.min;
        let x = ((local.x / rect.width()) * fw).clamp(0.0, fw - 1.0) as i32;
        let y = ((local.y / rect.height()) * fh).clamp(0.0, fh - 1.0) as i32;
        Some((x, y))
    }

    // ── Menu bar ────────────────────────────────────────────────────────────

    fn ui_menu_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let state = *self.tab().shared.state.lock();
        let connected = state == ConnectionState::Connected;
        let connecting = state == ConnectionState::Connecting;

        egui::menu::bar(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui
                    .button("New connection\tCtrl+T")
                    .on_hover_text("Open a new connection tab")
                    .clicked()
                {
                    self.new_connection_tab();
                    if self.view_fullscreen {
                        self.exit_view_fullscreen(ctx);
                    }
                    ui.close_menu();
                }
                if ui
                    .button("Open connection…\tCtrl+O")
                    .on_hover_text("Open a .rdp / .vnc file (new tab if current session is active)")
                    .clicked()
                {
                    self.open_connection();
                    ui.close_menu();
                }
                if ui
                    .add_enabled(self.can_connect(), egui::Button::new("Connect…\tCtrl+Return"))
                    .clicked()
                {
                    self.start_connect();
                    ui.close_menu();
                }
                if ui
                    .add_enabled(
                        connected || connecting,
                        egui::Button::new("Disconnect\tCtrl+D"),
                    )
                    .clicked()
                {
                    self.disconnect(ctx);
                    ui.close_menu();
                }
                if ui
                    .button("Close tab\tCtrl+W")
                    .on_hover_text("Close this tab and disconnect its session")
                    .clicked()
                {
                    let idx = self.active_tab;
                    self.request_close_tab(idx, ctx);
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Save connection as…\tCtrl+S").clicked() {
                    self.save_connection_as();
                    ui.close_menu();
                }
                if ui
                    .add_enabled(!self.is_busy(), egui::Button::new("Clear form"))
                    .on_hover_text("Reset all connection fields to defaults")
                    .clicked()
                {
                    self.clear_form();
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Quit\tCtrl+Q").clicked() {
                    disconnect_session();
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });

            ui.menu_button("View", |ui| {
                ui.checkbox(&mut self.show_sidebar, "Connection panel");
                ui.separator();
                ui.radio_value(&mut self.fit_mode, FitMode::Fit, "Fit to window");
                ui.radio_value(&mut self.fit_mode, FitMode::Actual, "Actual size (100%)");
                ui.radio_value(&mut self.fit_mode, FitMode::Stretch, "Stretch");
                ui.separator();
                if ui.button("Zoom in\tCtrl++").clicked() {
                    self.zoom = (self.zoom * 1.1).min(4.0);
                    ui.close_menu();
                }
                if ui.button("Zoom out\tCtrl+-").clicked() {
                    self.zoom = (self.zoom / 1.1).max(0.25);
                    ui.close_menu();
                }
                if ui.button("Reset zoom").clicked() {
                    self.zoom = 1.0;
                    ui.close_menu();
                }
                ui.separator();
                if ui
                    .button(if self.view_fullscreen {
                        format!("Exit view fullscreen\t{HOST_KEY_HINT}")
                    } else {
                        format!("View fullscreen\t{HOST_KEY_HINT}")
                    })
                    .on_hover_text(format!(
                        "Hide chrome so only the remote desktop is visible. \
                         While the view has keyboard focus, host shortcuts are disabled. \
                         Press {HOST_KEY_HINT} (host key) to exit."
                    ))
                    .clicked()
                {
                    self.toggle_view_fullscreen(ctx);
                    ui.close_menu();
                }
                if ui
                    .checkbox(&mut self.window_fullscreen, "Window fullscreen")
                    .on_hover_text("Toggle OS window fullscreen only (keeps app chrome)")
                    .changed()
                {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(
                        self.window_fullscreen,
                    ));
                }
            });

            ui.menu_button("Help", |ui| {
                if ui.button("About Rust RDP VNC").clicked() {
                    self.show_about = true;
                    ui.close_menu();
                }
            });
        });
    }

    // ── Toolbar ─────────────────────────────────────────────────────────────

    fn ui_toolbar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let state = *self.tab().shared.state.lock();
        let connected = state == ConnectionState::Connected;
        let connecting = state == ConnectionState::Connecting;
        let can_connect = self.can_connect();
        let tab_id = self.tab().tab_id;

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;

            // Sidebar toggle
            if ui
                .selectable_label(self.show_sidebar, "☰ Panel")
                .on_hover_text("Show or hide the connection panel")
                .clicked()
            {
                self.show_sidebar = !self.show_sidebar;
            }

            ui.separator();

            if ui
                .button("+ New")
                .on_hover_text("New connection tab (Ctrl+T)")
                .clicked()
            {
                self.new_connection_tab();
                if self.view_fullscreen {
                    self.exit_view_fullscreen(ctx);
                }
            }

            ui.separator();

            if connecting {
                if ui
                    .add(egui::Button::new(RichText::new("Cancel").color(theme::DANGER)))
                    .clicked()
                {
                    self.disconnect(ctx);
                }
            } else if connected {
                if ui
                    .add(egui::Button::new(
                        RichText::new("Disconnect").color(theme::DANGER),
                    ))
                    .clicked()
                {
                    self.disconnect(ctx);
                }
            } else {
                if ui
                    .button("Open")
                    .on_hover_text("Open a .rdp / .vnc file and connect (Ctrl+O)")
                    .clicked()
                {
                    self.open_connection();
                }

                let btn = egui::Button::new(RichText::new("Connect").strong().color(Color32::WHITE))
                    .fill(if can_connect {
                        theme::ACCENT
                    } else {
                        theme::BORDER
                    });
                if ui
                    .add_enabled(can_connect, btn)
                    .on_hover_text("Connect to the remote host (Ctrl+Return)")
                    .clicked()
                {
                    self.start_connect();
                }
            }

            ui.separator();

            // Protocol / host / port for the active tab
            let fields_enabled = !(connected || connecting);
            {
                let tab = self.tab_mut();
                ui.label(RichText::new("Protocol").color(theme::TEXT_DIM).small());
                egui::ComboBox::from_id_salt(format!("proto_{tab_id}"))
                    .selected_text(&tab.prefs.mode)
                    .width(72.0)
                    .show_ui(ui, |ui| {
                        for mode in ["RDP", "VNC"] {
                            if ui
                                .selectable_value(&mut tab.prefs.mode, mode.to_string(), mode)
                                .clicked()
                            {
                                if mode == "VNC" && tab.prefs.port == "3389" {
                                    tab.prefs.port = "5900".into();
                                } else if mode == "RDP" && tab.prefs.port == "5900" {
                                    tab.prefs.port = "3389".into();
                                }
                            }
                        }
                    });

                ui.label(RichText::new("Host").color(theme::TEXT_DIM).small());
                ui.add(
                    egui::TextEdit::singleline(&mut tab.prefs.host)
                        .desired_width(180.0)
                        .interactive(fields_enabled),
                );
                ui.label(RichText::new("Port").color(theme::TEXT_DIM).small());
                ui.add(
                    egui::TextEdit::singleline(&mut tab.prefs.port)
                        .desired_width(56.0)
                        .interactive(fields_enabled),
                );
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .selectable_label(self.view_fullscreen, "View FS")
                    .on_hover_text(format!(
                        "Fullscreen remote view only — hides chrome. Host key: {HOST_KEY_HINT}"
                    ))
                    .clicked()
                {
                    self.toggle_view_fullscreen(ctx);
                }
                if ui
                    .selectable_label(self.window_fullscreen, "Win FS")
                    .on_hover_text("OS window fullscreen (keeps menu/toolbar)")
                    .clicked()
                {
                    self.set_window_fullscreen(ctx, !self.window_fullscreen);
                }

                ui.separator();

                if ui.button("1:1").on_hover_text("Actual size").clicked() {
                    self.fit_mode = FitMode::Actual;
                    self.zoom = 1.0;
                }
                if ui.button("Fit").on_hover_text("Fit to window").clicked() {
                    self.fit_mode = FitMode::Fit;
                    self.zoom = 1.0;
                }

                if ui.button("−").on_hover_text("Zoom out").clicked() {
                    self.zoom = (self.zoom / 1.1).max(0.25);
                }
                ui.label(
                    RichText::new(format!("{:.0}%", self.zoom * 100.0))
                        .monospace()
                        .color(theme::TEXT_DIM),
                );
                if ui.button("+").on_hover_text("Zoom in").clicked() {
                    self.zoom = (self.zoom * 1.1).min(4.0);
                }
            });
        });
    }

    // ── Tab bar ─────────────────────────────────────────────────────────────

    fn ui_tab_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let mut select: Option<usize> = None;
        let mut close: Option<usize> = None;
        let mut new_tab = false;

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;

            for (i, tab) in self.tabs.iter().enumerate() {
                let selected = i == self.active_tab;
                let title = tab.tab_title();
                let state = *tab.shared.state.lock();

                let fill = if selected {
                    theme::PANEL_ALT
                } else {
                    theme::PANEL
                };
                let stroke = if selected {
                    egui::Stroke::new(1.0_f32, theme::ACCENT)
                } else {
                    egui::Stroke::new(1.0_f32, theme::BORDER)
                };

                egui::Frame::new()
                    .fill(fill)
                    .stroke(stroke)
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin::symmetric(8, 4))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            // Status dot
                            let (dot, _) =
                                ui.allocate_exact_size(Vec2::splat(7.0), egui::Sense::hover());
                            ui.painter()
                                .circle_filled(dot.center(), 3.0, state.color());

                            let label = RichText::new(title).small();
                            if ui
                                .add(egui::Label::new(if selected {
                                    label.strong().color(theme::TEXT)
                                } else {
                                    label.color(theme::TEXT_DIM)
                                }).sense(egui::Sense::click()))
                                .on_hover_text("Switch to this connection")
                                .clicked()
                            {
                                select = Some(i);
                            }

                            let close_resp = ui
                                .add(
                                    egui::Button::new(RichText::new("×").size(14.0))
                                        .frame(false)
                                        .min_size(Vec2::new(16.0, 16.0)),
                                )
                                .on_hover_text("Close tab and disconnect");
                            if close_resp.clicked() {
                                close = Some(i);
                            }
                        });
                    });
            }

            if ui
                .add(
                    egui::Button::new(RichText::new("+").strong())
                        .min_size(Vec2::new(28.0, 24.0)),
                )
                .on_hover_text("New connection (Ctrl+T)")
                .clicked()
            {
                new_tab = true;
            }
        });

        if let Some(i) = select {
            self.select_tab(i);
        }
        if let Some(i) = close {
            self.request_close_tab(i, ctx);
        }
        if new_tab {
            self.new_connection_tab();
            if self.view_fullscreen {
                self.exit_view_fullscreen(ctx);
            }
        }
    }

    fn ui_close_tab_confirm(&mut self, ctx: &egui::Context) {
        let Some(tab_id) = self.pending_close_tab_id else {
            return;
        };
        let Some(index) = self.tabs.iter().position(|t| t.tab_id == tab_id) else {
            self.pending_close_tab_id = None;
            return;
        };

        // Session may have ended while the dialog was open — close without asking.
        if !self.tabs[index].is_busy() {
            self.close_tab(index, ctx);
            return;
        }

        let title = self.tabs[index].tab_title();
        let state = *self.tabs[index].shared.state.lock();
        let state_label = match state {
            ConnectionState::Connecting => "still connecting",
            ConnectionState::Connected => "connected",
            _ => "active",
        };

        let mut open = true;
        let mut confirmed = false;
        let mut cancelled = false;

        egui::Window::new("Close connection?")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_min_width(340.0);
                ui.label(
                    RichText::new(format!("“{title}” is {state_label}."))
                        .strong(),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new("Closing this tab will disconnect the remote session.")
                        .color(theme::TEXT_DIM),
                );
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let close_btn = egui::Button::new(
                            RichText::new("Close & disconnect")
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(theme::DANGER);
                        if ui.add(close_btn).clicked() {
                            confirmed = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancelled = true;
                        }
                    });
                });
            });

        if confirmed {
            self.close_tab(index, ctx);
        } else if cancelled || !open {
            self.pending_close_tab_id = None;
        }
    }

    // ── Connection sidebar ──────────────────────────────────────────────────

    fn ui_sidebar(&mut self, ui: &mut egui::Ui) {
        let busy = self.is_busy();
        let tab_id = self.tab().tab_id;
        let can_connect = self.can_connect();
        let file_ext = self.tab().prefs.file_extension();
        let is_rdp = self.tab().prefs.mode == "RDP";
        let state = *self.tab().shared.state.lock();
        let fail_msg = if state == ConnectionState::Failed {
            Some(self.tab().shared.status.lock().clone())
        } else {
            None
        };

        ui.add_space(4.0);
        ui.label(RichText::new("Connection").strong().size(14.0));
        ui.label(
            RichText::new("Configure the remote session")
                .small()
                .color(theme::TEXT_DIM),
        );
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);

        {
            let tab = self.tab_mut();
            egui::Grid::new(format!("conn_grid_{tab_id}"))
                .num_columns(2)
                .spacing([12.0, 8.0])
                .min_col_width(80.0)
                .show(ui, |ui| {
                    ui.label(RichText::new("Protocol").color(theme::TEXT_DIM));
                    ui.add_enabled_ui(!busy, |ui| {
                        ui.horizontal(|ui| {
                            for mode in ["RDP", "VNC"] {
                                if ui
                                    .selectable_value(&mut tab.prefs.mode, mode.to_string(), mode)
                                    .clicked()
                                {
                                    if mode == "VNC" && tab.prefs.port == "3389" {
                                        tab.prefs.port = "5900".into();
                                    } else if mode == "RDP" && tab.prefs.port == "5900" {
                                        tab.prefs.port = "3389".into();
                                    }
                                }
                            }
                        });
                    });
                    ui.end_row();

                    ui.label(RichText::new("Host").color(theme::TEXT_DIM));
                    ui.add_enabled(
                        !busy,
                        egui::TextEdit::singleline(&mut tab.prefs.host)
                            .desired_width(f32::INFINITY),
                    );
                    ui.end_row();

                    ui.label(RichText::new("Port").color(theme::TEXT_DIM));
                    ui.add_enabled(
                        !busy,
                        egui::TextEdit::singleline(&mut tab.prefs.port).desired_width(80.0),
                    );
                    ui.end_row();

                    if is_rdp {
                        ui.label(RichText::new("Domain").color(theme::TEXT_DIM));
                        ui.add_enabled(
                            !busy,
                            egui::TextEdit::singleline(&mut tab.prefs.domain)
                                .desired_width(f32::INFINITY)
                                .hint_text("optional"),
                        );
                        ui.end_row();
                    }

                    ui.label(RichText::new("Username").color(theme::TEXT_DIM));
                    ui.add_enabled(
                        !busy,
                        egui::TextEdit::singleline(&mut tab.prefs.username)
                            .desired_width(f32::INFINITY),
                    );
                    ui.end_row();

                    ui.label(RichText::new("Password").color(theme::TEXT_DIM));
                    ui.add_enabled(
                        !busy,
                        egui::TextEdit::singleline(&mut tab.prefs.password)
                            .password(true)
                            .desired_width(f32::INFINITY),
                    );
                    ui.end_row();
                });

            ui.add_space(16.0);
            ui.label(RichText::new("Display").strong().size(14.0));
            ui.add_space(6.0);
            ui.separator();
            ui.add_space(8.0);

            egui::Grid::new(format!("display_grid_{tab_id}"))
                .num_columns(2)
                .spacing([12.0, 8.0])
                .min_col_width(80.0)
                .show(ui, |ui| {
                    ui.label(RichText::new("Width").color(theme::TEXT_DIM));
                    ui.add_enabled(
                        !busy,
                        egui::TextEdit::singleline(&mut tab.prefs.width).desired_width(80.0),
                    );
                    ui.end_row();

                    ui.label(RichText::new("Height").color(theme::TEXT_DIM));
                    ui.add_enabled(
                        !busy,
                        egui::TextEdit::singleline(&mut tab.prefs.height).desired_width(80.0),
                    );
                    ui.end_row();
                });

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add_enabled_ui(!busy, |ui| {
                    for (label, w, h) in [
                        ("1280×720", "1280", "720"),
                        ("1920×1080", "1920", "1080"),
                        ("2560×1440", "2560", "1440"),
                    ] {
                        if ui.small_button(label).clicked() {
                            tab.prefs.width = w.into();
                            tab.prefs.height = h.into();
                        }
                    }
                });
            });
        }

        ui.add_space(12.0);
        ui.horizontal(|ui| {
            let half = (ui.available_width() - ui.spacing().item_spacing.x).max(0.0) / 2.0;
            if ui
                .add(
                    egui::Button::new(format!("Save as .{file_ext}…"))
                        .min_size(Vec2::new(half, 28.0)),
                )
                .on_hover_text(format!("Save a .{file_ext} connection file (Ctrl+S)"))
                .clicked()
            {
                self.save_connection_as();
            }
            if ui
                .add_enabled(
                    !busy,
                    egui::Button::new("Clear form").min_size(Vec2::new(half, 28.0)),
                )
                .on_hover_text("Reset all connection fields to defaults")
                .clicked()
            {
                self.clear_form();
            }
        });

        ui.add_space(12.0);

        match state {
            ConnectionState::Idle | ConnectionState::Failed => {
                ui.horizontal(|ui| {
                    let half = (ui.available_width() - ui.spacing().item_spacing.x).max(0.0) / 2.0;
                    if ui
                        .add(egui::Button::new("Open").min_size(Vec2::new(half, 32.0)))
                        .on_hover_text("Open a .rdp / .vnc file and connect (Ctrl+O)")
                        .clicked()
                    {
                        self.open_connection();
                    }
                    let btn = egui::Button::new(
                        RichText::new("Connect").strong().color(Color32::WHITE),
                    )
                    .fill(theme::ACCENT)
                    .min_size(Vec2::new(half, 32.0));
                    if ui.add_enabled(can_connect, btn).clicked() {
                        self.start_connect();
                    }
                });
            }
            ConnectionState::Connecting => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Connecting…");
                });
                if ui
                    .add(
                        egui::Button::new("Cancel")
                            .min_size(Vec2::new(ui.available_width(), 28.0)),
                    )
                    .clicked()
                {
                    self.disconnect(ui.ctx());
                }
            }
            ConnectionState::Connected => {
                ui.label(
                    RichText::new("Session is active")
                        .color(theme::SUCCESS)
                        .small(),
                );
                if ui
                    .add(
                        egui::Button::new(RichText::new("Disconnect").color(theme::DANGER))
                            .min_size(Vec2::new(ui.available_width(), 28.0)),
                    )
                    .clicked()
                {
                    self.disconnect(ui.ctx());
                }
            }
        }

        if let Some(msg) = fail_msg {
            ui.add_space(10.0);
            egui::Frame::new()
                .fill(theme::ERROR_BG)
                .stroke(egui::Stroke::new(1.0_f32, theme::DANGER))
                .corner_radius(4.0)
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.label(RichText::new("Connection failed").color(theme::DANGER).strong());
                    ui.label(RichText::new(msg).small().color(theme::TEXT));
                });
        }

        ui.add_space(16.0);
        ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
            ui.add_space(8.0);
            ui.label(
                RichText::new(
                    "Ctrl+T            New connection\nCtrl+W            Close tab\nCtrl+O            Open file\nCtrl+Return       Connect\nCtrl+S            Save as…\nCtrl+D            Disconnect\nCtrl+Alt+Enter    Host key (exit view FS)\n\nWhile the remote view has focus,\nhost shortcuts are disabled.",
                )
                .small()
                .monospace()
                .color(theme::TEXT_DIM),
            );
        });
    }

    // ── Status bar ──────────────────────────────────────────────────────────

    fn ui_status_bar(&self, ui: &mut egui::Ui) {
        let tab = self.tab();
        let state = *tab.shared.state.lock();
        let status = tab.shared.status.lock().clone();
        let (fw, fh) = {
            let f = tab.shared.frame.lock();
            (f.width, f.height)
        };
        let mode = tab.prefs.mode.clone();
        let host_empty = tab.prefs.host.is_empty();
        let endpoint = tab.prefs.endpoint_label();
        let tab_n = self.tabs.len();
        let tab_i = self.active_tab + 1;

        ui.horizontal(|ui| {
            // Status indicator
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(8.0), egui::Sense::hover());
            ui.painter()
                .circle_filled(rect.center(), 4.0, state.color());

            ui.label(
                RichText::new(state.label())
                    .strong()
                    .color(state.color())
                    .small(),
            );
            ui.separator();
            ui.label(
                RichText::new(format!("Tab {tab_i}/{tab_n}"))
                    .small()
                    .color(theme::TEXT_DIM),
            );
            ui.separator();
            ui.label(RichText::new(status).small().color(theme::TEXT_DIM));

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!(
                        "{}  ·  {}×{}  ·  zoom {:.0}%",
                        mode,
                        fw,
                        fh,
                        self.zoom * 100.0
                    ))
                    .small()
                    .monospace()
                    .color(theme::TEXT_DIM),
                );
                if !host_empty {
                    ui.separator();
                    ui.label(
                        RichText::new(endpoint)
                            .small()
                            .monospace()
                            .color(theme::TEXT_DIM),
                    );
                }
            });
        });
    }

    // ── Remote viewport ─────────────────────────────────────────────────────

    fn ui_viewport(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let state = *self.tab().shared.state.lock();

        match state {
            ConnectionState::Idle | ConnectionState::Failed => {
                self.ui_empty_canvas(ui);
            }
            ConnectionState::Connecting => {
                let status = self.tab().shared.status.lock().clone();
                ui.centered_and_justified(|ui| {
                    ui.vertical_centered(|ui| {
                        ui.spinner();
                        ui.add_space(12.0);
                        ui.label(RichText::new("Establishing session…").size(15.0));
                        ui.label(
                            RichText::new(status)
                                .color(theme::TEXT_DIM)
                                .small(),
                        );
                    });
                });
            }
            ConnectionState::Connected => {
                self.ui_remote_session(ui, ctx);
            }
        }
    }

    fn ui_empty_canvas(&mut self, ui: &mut egui::Ui) {
        let can_connect = self.can_connect();
        let endpoint = self.tab().prefs.endpoint_label();
        ui.centered_and_justified(|ui| {
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new("Remote Desktop")
                        .size(20.0)
                        .color(theme::TEXT_DIM),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new("Enter a host in the connection panel, then click Connect.")
                        .color(theme::TEXT_DIM)
                        .small(),
                );
                ui.add_space(8.0);
                ui.label(
                    RichText::new("Use + New or Ctrl+T for another concurrent connection.")
                        .color(theme::TEXT_DIM)
                        .small(),
                );
                ui.add_space(16.0);
                if !self.show_sidebar {
                    if ui.button("Show connection panel").clicked() {
                        self.show_sidebar = true;
                    }
                } else if can_connect {
                    let btn = egui::Button::new(
                        RichText::new(format!("Connect to {endpoint}")).color(Color32::WHITE),
                    )
                    .fill(theme::ACCENT);
                    if ui.add(btn).clicked() {
                        self.start_connect();
                    }
                }
            });
        });
    }

    fn ui_remote_session(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.ensure_texture(ctx);

        let Some(tex) = self.tab().texture.clone() else {
            ui.centered_and_justified(|ui| ui.label("Waiting for first frame…"));
            return;
        };

        let frame_size = {
            let f = self.tab().shared.frame.lock();
            Vec2::new(f.width as f32, f.height as f32)
        };

        let available = ui.available_size();
        let display = match self.fit_mode {
            FitMode::Fit => {
                if frame_size.x > 0.0 && frame_size.y > 0.0 {
                    let sx = available.x / frame_size.x;
                    let sy = available.y / frame_size.y;
                    frame_size * sx.min(sy) * self.zoom
                } else {
                    available
                }
            }
            FitMode::Actual => frame_size * self.zoom,
            FitMode::Stretch => available,
        };

        // Read wheel *before* ScrollArea can consume it.
        let raw_scroll = ui.input(|i| i.raw_scroll_delta);

        // Scroll area for oversized desktops — wheel is for the remote host.
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .enable_scrolling(false)
            .drag_to_scroll(false)
            .show(ui, |ui| {
                ui.set_min_size(available);
                ui.centered_and_justified(|ui| {
                    let (rect, response) =
                        ui.allocate_exact_size(display, egui::Sense::click_and_drag());
                    ui.painter().image(
                        tex.id(),
                        rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );

                    if response.hovered() || response.clicked() {
                        response.request_focus();
                    }

                    // No host Ctrl+scroll zoom — all input goes to remote when over the view.
                    self.handle_session_input(ui, &response, rect, raw_scroll);
                });
            });
    }

    fn handle_session_input(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        rect: egui::Rect,
        raw_scroll: Vec2,
    ) {
        let connected = matches!(
            *self.tab().shared.state.lock(),
            ConnectionState::Connected | ConnectionState::Connecting
        );
        let view_focused = response.has_focus() || response.hovered();
        if connected && (view_focused || self.view_fullscreen) {
            self.remote_input_active = true;
        }

        // Prefer hover position so mouse move + wheel work without a button held.
        let pointer = response
            .hover_pos()
            .or_else(|| response.interact_pointer_pos())
            .or_else(|| {
                ui.input(|i| i.pointer.latest_pos().filter(|p| rect.contains(*p)))
            });

        if let Some(pos) = pointer {
            if let Some((x, y)) = self.remote_pos(pos, rect) {
                let tab = self.tab_mut();
                let moved = tab
                    .last_mouse
                    .map(|(lx, ly)| lx != x || ly != y)
                    .unwrap_or(true);
                if moved {
                    send_mouse_event(x, y, 0);
                    tab.last_mouse = Some((x, y));
                }

                let buttons = ui.ctx().input(|i| {
                    (
                        i.pointer.button_pressed(egui::PointerButton::Primary),
                        i.pointer.button_released(egui::PointerButton::Primary),
                        i.pointer.button_pressed(egui::PointerButton::Secondary),
                        i.pointer.button_released(egui::PointerButton::Secondary),
                    )
                });

                if buttons.0 {
                    send_mouse_event(x, y, 1);
                    tab.left_down = true;
                }
                if buttons.1 && tab.left_down {
                    send_mouse_event(x, y, 2);
                    tab.left_down = false;
                }
                if buttons.2 {
                    send_mouse_event(x, y, 3);
                    tab.right_down = true;
                }
                if buttons.3 && tab.right_down {
                    send_mouse_event(x, y, 4);
                    tab.right_down = false;
                }

                // Wheel → always remote when over the surface
                if response.hovered() || self.view_fullscreen {
                    let mut scroll_y = raw_scroll.y;
                    if scroll_y == 0.0 {
                        ui.input(|i| {
                            for ev in &i.events {
                                if let egui::Event::MouseWheel { delta, .. } = ev {
                                    scroll_y += delta.y;
                                }
                            }
                        });
                    }
                    if scroll_y != 0.0 {
                        let sign = if scroll_y > 0.0 { -1 } else { 1 };
                        let notches = (scroll_y.abs() / 8.0).ceil().clamp(1.0, 16.0) as i32;
                        send_mouse_wheel_event(x, y, sign * 120 * notches);
                    }
                }
            }
        }

        if !(response.has_focus() || response.hovered() || self.view_fullscreen) {
            return;
        }

        // Host key must not be injected into the remote session
        let host_key_this_frame = ui.input(|i| Self::is_host_key_pressed(i));
        if host_key_this_frame {
            let tab = self.tab_mut();
            if tab.mod_ctrl {
                send_scancode_event(0x1D, false, 0);
                tab.mod_ctrl = false;
            }
            if tab.mod_alt {
                send_scancode_event(0x38, false, 0);
                tab.mod_alt = false;
            }
            if tab.mod_shift {
                send_scancode_event(0x2A, false, 0);
                tab.mod_shift = false;
            }
            return;
        }

        let modifiers = ui.input(|i| i.modifiers);
        // While holding Ctrl+Alt (host-key prefix), do not forward keys to remote
        if modifiers.ctrl && modifiers.alt && !modifiers.shift {
            return;
        }

        self.sync_modifiers(modifiers);

        let events: Vec<egui::Event> = ui.input(|i| i.events.clone());
        for event in events {
            if let egui::Event::Key {
                key,
                pressed,
                repeat,
                modifiers: modifs,
                ..
            } = event
            {
                if repeat {
                    continue;
                }
                if Self::is_host_key_chord(modifs, key) {
                    continue;
                }
                if let Some((scancode, extended)) = egui_key_to_scancode(key) {
                    let ext = extended || is_extended_scancode(scancode);
                    send_scancode_event(scancode, ext, if pressed { 1 } else { 0 });
                } else if key == Key::Backspace {
                    send_key_event(8, if pressed { 1 } else { 0 });
                } else if key == Key::Enter {
                    send_key_event(13, if pressed { 1 } else { 0 });
                }
            }
        }
    }

    // ── Global shortcuts ────────────────────────────────────────────────────

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        // Host key always works (exit / enter view fullscreen)
        if ctx.input(|i| Self::is_host_key_pressed(i)) {
            if self.view_fullscreen {
                self.exit_view_fullscreen(ctx);
            } else if matches!(*self.tab().shared.state.lock(), ConnectionState::Connected) {
                self.enter_view_fullscreen(ctx);
            }
            return;
        }

        // Remote owns keyboard: no Ctrl+S / Esc / F11 / … host handling
        if self.keyboard_grabbed() {
            return;
        }

        let mut connect = false;
        let mut disconnect = false;
        let mut quit = false;
        let mut save = false;
        let mut open = false;
        let mut new_tab = false;
        let mut close_tab = false;
        let mut zoom_in = false;
        let mut zoom_out = false;

        ctx.input(|i| {
            // Require Ctrl without Alt so host key (Ctrl+Alt+Enter) never means Connect
            if i.modifiers.ctrl && !i.modifiers.alt && i.key_pressed(Key::Enter) {
                connect = true;
            }
            if i.modifiers.ctrl && !i.modifiers.alt && i.key_pressed(Key::D) {
                disconnect = true;
            }
            if i.modifiers.ctrl && !i.modifiers.alt && i.key_pressed(Key::Q) {
                quit = true;
            }
            if i.modifiers.ctrl && !i.modifiers.alt && i.key_pressed(Key::S) {
                save = true;
            }
            if i.modifiers.ctrl && !i.modifiers.alt && i.key_pressed(Key::O) {
                open = true;
            }
            if i.modifiers.ctrl && !i.modifiers.alt && i.key_pressed(Key::T) {
                new_tab = true;
            }
            if i.modifiers.ctrl && !i.modifiers.alt && i.key_pressed(Key::W) {
                close_tab = true;
            }
            if i.modifiers.ctrl
                && !i.modifiers.alt
                && (i.key_pressed(Key::Plus) || i.key_pressed(Key::Equals))
            {
                zoom_in = true;
            }
            if i.modifiers.ctrl && !i.modifiers.alt && i.key_pressed(Key::Minus) {
                zoom_out = true;
            }
        });

        if connect {
            self.start_connect();
        }
        if open {
            self.open_connection();
        }
        if disconnect {
            self.disconnect(ctx);
        }
        if new_tab {
            self.new_connection_tab();
            if self.view_fullscreen {
                self.exit_view_fullscreen(ctx);
            }
        }
        if close_tab {
            let idx = self.active_tab;
            self.request_close_tab(idx, ctx);
        }
        if save {
            self.save_connection_as();
        }
        if quit {
            disconnect_session();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if zoom_in {
            self.zoom = (self.zoom * 1.1).min(4.0);
        }
        if zoom_out {
            self.zoom = (self.zoom / 1.1).max(0.25);
        }
    }

    /// Thin floating bar to exit view fullscreen (mouse to top edge).
    fn ui_view_fullscreen_overlay(&mut self, ctx: &egui::Context) {
        if !self.view_fullscreen {
            return;
        }

        let show_hint = self
            .view_fs_hint_until
            .map(|t| Instant::now() < t)
            .unwrap_or(false);
        if show_hint {
            ctx.request_repaint_after(Duration::from_millis(200));
        }

        let near_top = ctx.input(|i| {
            i.pointer
                .latest_pos()
                .map(|p| p.y < 48.0)
                .unwrap_or(false)
        });

        if !show_hint && !near_top {
            return;
        }

        egui::Area::new(egui::Id::new("view_fs_overlay"))
            .anchor(egui::Align2::CENTER_TOP, [0.0, 8.0])
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(Color32::from_black_alpha(200))
                    .stroke(egui::Stroke::new(1.0_f32, theme::BORDER))
                    .corner_radius(6.0)
                    .inner_margin(egui::Margin::symmetric(12, 6))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!(
                                    "Keyboard captured — host key {HOST_KEY_HINT}"
                                ))
                                .small()
                                .color(theme::TEXT_DIM),
                            );
                            if ui.button(format!("Exit ({HOST_KEY_HINT})")).clicked() {
                                self.exit_view_fullscreen(ctx);
                            }
                        });
                    });
            });
    }

    fn ui_toast(&mut self, ctx: &egui::Context) {
        // Drop expired toast
        if let Some(toast) = &self.toast {
            if Instant::now() >= toast.until {
                self.toast = None;
                return;
            }
        }
        let Some(toast) = &self.toast else {
            return;
        };

        let (bg, border, fg) = match toast.kind {
            ToastKind::Success => (
                Color32::from_rgb(0x1A, 0x3D, 0x2A),
                theme::SUCCESS,
                theme::SUCCESS,
            ),
            ToastKind::Error => (theme::ERROR_BG, theme::DANGER, theme::DANGER),
            ToastKind::Info => (
                Color32::from_rgb(0x1A, 0x2A, 0x3D),
                theme::ACCENT,
                theme::TEXT,
            ),
        };
        let message = toast.message.clone();

        // Keep repainting so the toast can expire cleanly
        ctx.request_repaint_after(Duration::from_millis(100));

        egui::Area::new(egui::Id::new("toast_banner"))
            .anchor(egui::Align2::CENTER_TOP, [0.0, 52.0])
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(bg)
                    .stroke(egui::Stroke::new(1.0_f32, border))
                    .corner_radius(6.0)
                    .inner_margin(egui::Margin::symmetric(16, 10))
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 4],
                        blur: 12,
                        spread: 0,
                        color: Color32::from_black_alpha(120),
                    })
                    .show(ui, |ui| {
                        ui.set_max_width(520.0);
                        ui.label(RichText::new(message).color(fg).strong());
                    });
            });
    }
}

fn apply_desktop_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.visuals.dark_mode = true;

    // Panels
    style.visuals.panel_fill = theme::PANEL;
    style.visuals.window_fill = theme::PANEL_ALT;
    style.visuals.extreme_bg_color = theme::BG;
    style.visuals.faint_bg_color = theme::PANEL_ALT;
    style.visuals.code_bg_color = theme::BG;

    // Text
    style.visuals.override_text_color = Some(theme::TEXT);

    // Widgets — flat desktop look
    style.visuals.widgets.inactive.bg_fill = theme::PANEL_ALT;
    style.visuals.widgets.inactive.weak_bg_fill = theme::PANEL_ALT;
    style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, theme::BORDER);
    style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, theme::TEXT);

    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(0x3A, 0x3A, 0x3A);
    style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x3A, 0x3A, 0x3A);
    style.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, theme::ACCENT_HOVER);
    style.visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0_f32, theme::TEXT);

    style.visuals.widgets.active.bg_fill = theme::ACCENT;
    style.visuals.widgets.active.weak_bg_fill = theme::ACCENT;
    style.visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0_f32, theme::ACCENT);
    style.visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0_f32, Color32::WHITE);

    style.visuals.widgets.open.bg_fill = theme::PANEL_ALT;
    style.visuals.widgets.open.bg_stroke = egui::Stroke::new(1.0_f32, theme::ACCENT);

    style.visuals.selection.bg_fill = theme::ACCENT.linear_multiply(0.35);
    style.visuals.selection.stroke = egui::Stroke::new(1.0_f32, theme::ACCENT);

    style.visuals.hyperlink_color = theme::ACCENT;
    style.visuals.window_stroke = egui::Stroke::new(1.0_f32, theme::BORDER);

    style.spacing.item_spacing = Vec2::new(8.0, 6.0);
    style.spacing.button_padding = Vec2::new(10.0, 4.0);
    style.spacing.menu_margin = egui::Margin::same(6);
    style.spacing.window_margin = egui::Margin::same(10);

    // Compact, readable desktop density
    if let Some(font_id) = style
        .text_styles
        .get_mut(&egui::TextStyle::Body)
    {
        font_id.size = 13.0;
    }
    if let Some(font_id) = style.text_styles.get_mut(&egui::TextStyle::Button) {
        font_id.size = 13.0;
    }
    if let Some(font_id) = style.text_styles.get_mut(&egui::TextStyle::Small) {
        font_id.size = 11.0;
    }

    ctx.set_style(style);
}

impl eframe::App for DesktopApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Repaint while any tab has an active session (background tabs still receive frames).
        let any_live = self.tabs.iter().any(|t| {
            matches!(
                *t.shared.state.lock(),
                ConnectionState::Connected | ConnectionState::Connecting
            )
        });
        if any_live {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }

        let state = *self.tab().shared.state.lock();

        // Reset each frame; remote view sets this when it owns keyboard focus.
        self.remote_input_active = false;

        // Dynamic window title
        let title = if state == ConnectionState::Connected {
            format!(
                "{} — {} — Rust RDP VNC",
                self.tab().prefs.endpoint_label(),
                self.tab().prefs.mode
            )
        } else if self.tabs.len() > 1 {
            format!("Rust RDP VNC ({} tabs)", self.tabs.len())
        } else {
            "Rust RDP VNC".into()
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));

        // View fullscreen: only the remote canvas — no menu / toolbar / sidebar / status.
        if !self.view_fullscreen {
            // ── Menu + toolbar + tabs ───────────────────────────────────────
            egui::TopBottomPanel::top("chrome")
                .frame(
                    egui::Frame::new()
                        .fill(theme::PANEL)
                        .inner_margin(egui::Margin::symmetric(8, 4))
                        .stroke(egui::Stroke::new(0.0_f32, Color32::TRANSPARENT)),
                )
                .show(ctx, |ui| {
                    self.ui_menu_bar(ui, ctx);
                    ui.add_space(2.0);
                    ui.separator();
                    ui.add_space(2.0);
                    self.ui_toolbar(ui, ctx);
                    ui.add_space(2.0);
                    ui.separator();
                    ui.add_space(2.0);
                    self.ui_tab_bar(ui, ctx);
                });

            // ── Status bar ──────────────────────────────────────────────────
            egui::TopBottomPanel::bottom("status")
                .exact_height(24.0)
                .frame(
                    egui::Frame::new()
                        .fill(theme::PANEL_ALT)
                        .inner_margin(egui::Margin::symmetric(8, 3))
                        .stroke(egui::Stroke::new(1.0_f32, theme::BORDER)),
                )
                .show(ctx, |ui| {
                    self.ui_status_bar(ui);
                });

            // ── Side panel (connection settings) ────────────────────────────
            if self.show_sidebar {
                egui::SidePanel::left("sidebar")
                    .default_width(300.0)
                    .min_width(260.0)
                    .max_width(400.0)
                    .resizable(true)
                    .frame(
                        egui::Frame::new()
                            .fill(theme::PANEL)
                            .inner_margin(egui::Margin::symmetric(14, 10))
                            .stroke(egui::Stroke::new(1.0_f32, theme::BORDER)),
                    )
                    .show(ctx, |ui| {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            self.ui_sidebar(ui);
                        });
                    });
            }
        }

        // ── Main viewport (always) ──────────────────────────────────────────
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::CANVAS).inner_margin(0.0))
            .show(ctx, |ui| {
                self.ui_viewport(ui, ctx);
            });

        if self.view_fullscreen {
            self.ui_view_fullscreen_overlay(ctx);
        }

        // ── About dialog ────────────────────────────────────────────────────
        if self.show_about && !self.view_fullscreen {
            egui::Window::new("About Rust RDP VNC")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut self.show_about)
                .show(ctx, |ui| {
                    ui.set_min_width(320.0);
                    ui.label(RichText::new("Rust RDP VNC").size(16.0).strong());
                    ui.label(RichText::new("Desktop client for Linux").color(theme::TEXT_DIM));
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);
                    ui.label("Remote desktop client built with IronRDP + egui.");
                    ui.label("Supports Microsoft RDP and VNC protocols.");
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("Version 0.1.0")
                            .small()
                            .color(theme::TEXT_DIM),
                    );
                });
        }

        // Close-tab confirm when session is still connecting/connected
        self.ui_close_tab_confirm(ctx);

        // Floating toast (save feedback, errors, …)
        self.ui_toast(ctx);

        // Host key / shortcuts after the view has marked keyboard grab state
        self.handle_shortcuts(ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        disconnect_session();
    }
}

fn main() -> eframe::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([960.0, 600.0])
            .with_title("Rust RDP VNC"),
        ..Default::default()
    };

    eframe::run_native(
        "Rust RDP VNC",
        options,
        Box::new(|cc| Ok(Box::new(DesktopApp::new(cc)))),
    )
}
