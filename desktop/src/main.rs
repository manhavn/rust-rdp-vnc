#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod input_capture;
mod keys;

use eframe::egui::{
    self, Align, Color32, ColorImage, Key, Layout, RichText, TextureHandle, TextureOptions, Vec2,
};
use input_capture::SystemInputCapture;
use parking_lot::Mutex;
use rust_rdp::{
    connect_session, disconnect_session, disconnect_session_id, init_runtime, send_key_event,
    send_mouse_event, send_mouse_horizontal_wheel_event, send_mouse_wheel_event,
    send_scancode_event, set_active_session, SessionCallback,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use keys::{char_to_scancode, egui_key_to_scancode, is_extended_scancode, RemoteKeyboardState};

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
        let total_pixels = w * h;
        let mut rgba = vec![0u8; total_pixels * 4];

        for (px, out) in self
            .pixels
            .iter()
            .take(total_pixels)
            .zip(rgba.chunks_exact_mut(4))
        {
            let v = *px as u32;
            let a = ((v >> 24) & 0xFF) as u8;
            out[0] = ((v >> 16) & 0xFF) as u8; // R
            out[1] = ((v >> 8) & 0xFF) as u8; // G
            out[2] = (v & 0xFF) as u8; // B
            out[3] = if a == 0 { 255 } else { a };
        }
        ColorImage::from_rgba_unmultiplied([w, h], &rgba)
    }
}

#[derive(Clone)]
struct CustomCursorData {
    width: i32,
    height: i32,
    hot_x: i32,
    hot_y: i32,
    pixels: Vec<i32>,
}

struct SharedUi {
    state: Mutex<ConnectionState>,
    status: Mutex<String>,
    frame: Mutex<FrameBuffer>,
    dirty: AtomicBool,
    current_cursor: Mutex<egui::CursorIcon>,
    custom_cursor: Mutex<Option<CustomCursorData>>,
}

impl SharedUi {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(ConnectionState::Idle),
            status: Mutex::new("Ready".into()),
            frame: Mutex::new(FrameBuffer::new(1280, 720)),
            dirty: AtomicBool::new(false),
            current_cursor: Mutex::new(egui::CursorIcon::Default),
            custom_cursor: Mutex::new(None),
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

    fn on_cursor_changed(&self, cursor_type: i32) {
        let icon = match cursor_type {
            0 => egui::CursorIcon::Default,
            1 => egui::CursorIcon::None,
            2 => egui::CursorIcon::Text,
            3 => egui::CursorIcon::PointingHand,
            4 => egui::CursorIcon::ResizeNwSe,
            5 => egui::CursorIcon::ResizeNeSw,
            6 => egui::CursorIcon::ResizeEast,
            7 => egui::CursorIcon::ResizeNorth,
            8 => egui::CursorIcon::Wait,
            9 => egui::CursorIcon::Crosshair,
            10 => egui::CursorIcon::Move,
            11 => egui::CursorIcon::NotAllowed,
            12 => egui::CursorIcon::Grab,
            13 => egui::CursorIcon::Grabbing,
            14 => egui::CursorIcon::Help,
            15 => egui::CursorIcon::Progress,
            _ => egui::CursorIcon::Default,
        };
        *self.ui.current_cursor.lock() = icon;
    }

    fn on_cursor_bitmap(&self, width: i32, height: i32, hot_x: i32, hot_y: i32, pixels: &[i32]) {
        *self.ui.custom_cursor.lock() = Some(CustomCursorData {
            width,
            height,
            hot_x,
            hot_y,
            pixels: pixels.to_vec(),
        });
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
    enable_hover_throttle: bool,
    hover_send_interval_ms: u64,
    disable_rust_log: bool,
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
            enable_hover_throttle: false,
            hover_send_interval_ms: 1000,
            disable_rust_log: false,
        }
    }
}

struct AppSession {
    active_tab: usize,
    enable_hover_throttle: bool,
    hover_send_interval_ms: u64,
    disable_rust_log: bool,
    tabs: Vec<Prefs>,
}

impl Default for AppSession {
    fn default() -> Self {
        Self {
            active_tab: 0,
            enable_hover_throttle: false,
            hover_send_interval_ms: 1000,
            disable_rust_log: false,
            tabs: vec![Prefs::default()],
        }
    }
}

impl AppSession {
    fn load() -> Self {
        let Some(path) = Prefs::path() else {
            return Self::default();
        };
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        Self::parse(&text)
    }

    fn parse(text: &str) -> Self {
        let mut session = Self {
            active_tab: 0,
            enable_hover_throttle: false,
            hover_send_interval_ms: 1000,
            disable_rust_log: false,
            tabs: Vec::new(),
        };

        let mut current_tab: Option<Prefs> = None;
        let mut fallback_single = Prefs::default();
        let mut saw_tab_section = false;

        for line in text.lines() {
            let line = line.trim_matches(|c| c == '\r' || c == '\n');
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            if trimmed.eq_ignore_ascii_case("[tab]") {
                saw_tab_section = true;
                if let Some(t) = current_tab.take() {
                    session.tabs.push(t);
                }
                current_tab = Some(Prefs::default());
                continue;
            }

            if let Some((k, v)) = line.split_once('=') {
                let k = k.trim();
                let v = v.trim();

                if let Some(ref mut p) = current_tab {
                    match k {
                        "host" => p.host = v.to_string(),
                        "port" => p.port = v.to_string(),
                        "username" => p.username = v.to_string(),
                        "password" => {
                            p.password = v.trim_matches(|c| c == '\r' || c == '\n').to_string()
                        }
                        "domain" => p.domain = v.to_string(),
                        "mode" => p.mode = v.to_string(),
                        "width" => p.width = v.to_string(),
                        "height" => p.height = v.to_string(),
                        "enable_hover_throttle" => {
                            p.enable_hover_throttle = v.parse().unwrap_or(false)
                        }
                        "hover_send_interval_ms" => {
                            p.hover_send_interval_ms = v.parse().unwrap_or(1000)
                        }
                        "disable_rust_log" | "disable_frame_complete_log" => {
                            p.disable_rust_log = v.parse().unwrap_or(false)
                        }
                        _ => {}
                    }
                } else {
                    match k {
                        "active_tab" => session.active_tab = v.parse().unwrap_or(0),
                        "enable_hover_throttle" => {
                            let b = v.parse().unwrap_or(false);
                            session.enable_hover_throttle = b;
                            fallback_single.enable_hover_throttle = b;
                        }
                        "hover_send_interval_ms" => {
                            let ms = v.parse().unwrap_or(1000);
                            session.hover_send_interval_ms = ms;
                            fallback_single.hover_send_interval_ms = ms;
                        }
                        "disable_rust_log" | "disable_frame_complete_log" => {
                            let b = v.parse().unwrap_or(false);
                            session.disable_rust_log = b;
                            fallback_single.disable_rust_log = b;
                        }
                        "host" => fallback_single.host = v.to_string(),
                        "port" => fallback_single.port = v.to_string(),
                        "username" => fallback_single.username = v.to_string(),
                        "password" => {
                            fallback_single.password =
                                v.trim_matches(|c| c == '\r' || c == '\n').to_string()
                        }
                        "domain" => fallback_single.domain = v.to_string(),
                        "mode" => fallback_single.mode = v.to_string(),
                        "width" => fallback_single.width = v.to_string(),
                        "height" => fallback_single.height = v.to_string(),
                        _ => {}
                    }
                }
            }
        }

        if let Some(t) = current_tab.take() {
            session.tabs.push(t);
        }

        if !saw_tab_section && session.tabs.is_empty() {
            session.tabs.push(fallback_single);
        }

        if session.tabs.is_empty() {
            session.tabs.push(Prefs::default());
        }

        if session.active_tab >= session.tabs.len() {
            session.active_tab = session.tabs.len().saturating_sub(1);
        }

        session
    }
}

impl Prefs {
    fn path() -> Option<PathBuf> {
        directories::ProjectDirs::from("com", "rustai", "rust-rdp-vnc")
            .map(|d| d.config_dir().join("prefs.txt"))
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
            ..Self::default()
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
    show_password: bool,
    /// Backend session while connecting/connected (also kept after Failed until reconnect).
    backend_session_id: Option<u64>,
    texture: Option<TextureHandle>,
    last_frame_gen: u64,
    last_mouse: Option<(i32, i32)>,
    last_mouse_send_time: Option<std::time::Instant>,
    left_down: bool,
    left_down_pos: Option<(i32, i32)>,
    left_down_dragged: bool,
    right_down: bool,
    middle_down: bool,
    extra1_down: bool,
    extra2_down: bool,
    mod_shift: bool,
    mod_ctrl: bool,
    mod_alt: bool,
    mod_super: bool,
    remote_keys_down: Vec<Key>,
    /// Fractional RDP scroll distance carried between input frames.
    rdp_scroll_remainder: f32,
    rdp_hscroll_remainder: f32,
    keyboard_state: RemoteKeyboardState,
}

impl ConnectionTab {
    fn new(tab_id: u64, prefs: Prefs) -> Self {
        Self {
            tab_id,
            prefs,
            shared: SharedUi::new(),
            show_password: false,
            backend_session_id: None,
            texture: None,
            last_frame_gen: 0,
            last_mouse: None,
            last_mouse_send_time: None,
            left_down: false,
            left_down_pos: None,
            left_down_dragged: false,
            right_down: false,
            middle_down: false,
            extra1_down: false,
            extra2_down: false,
            mod_shift: false,
            mod_ctrl: false,
            mod_alt: false,
            mod_super: false,
            remote_keys_down: Vec::new(),
            rdp_scroll_remainder: 0.0,
            rdp_hscroll_remainder: 0.0,
            keyboard_state: RemoteKeyboardState::default(),
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
    /// True when the remote surface should own keyboard (connected + hover/focus, or view FS).
    /// While true, host app shortcuts are disabled.
    remote_input_active: bool,
    /// Whether the remote effectively owned input at the end of the previous frame.
    remote_input_owned_last_frame: bool,
    /// Native X11/Wayland shortcut inhibition while the remote view owns input.
    system_input_capture: SystemInputCapture,
    /// Exact local-only hitbox of the floating Exit control.
    view_exit_overlay_rect: Option<egui::Rect>,
    /// Toggle state of quick tab list in view fullscreen mode.
    show_fullscreen_tabs: bool,
    toast: Option<Toast>,
    /// Tab id waiting for “close while connected?” confirmation (× / Ctrl+W).
    pending_close_tab_id: Option<u64>,
    enable_hover_throttle: bool,
    hover_send_interval_ms: u64,
    disable_rust_log: bool,
}

/// Small top-center hotspot that reveals the compact Exit control.
const VIEW_EXIT_REVEAL_HEIGHT: f32 = 32.0;
const VIEW_EXIT_REVEAL_WIDTH_FRACTION: f32 = 0.10;

/// egui normalizes one native line-wheel tick to 40 points; using 32 here
/// makes RDP scrolling 25% faster while retaining smooth-delta accumulation.
const RDP_SCROLL_POINTS_PER_NOTCH: f32 = 32.0;
/// VNC servers expose wheel input as button clicks; 3 points per notch keeps
/// macOS responsive without changing the Android VNC gesture path.
const VNC_SCROLL_POINTS_PER_NOTCH: f32 = 3.0;
const MAX_WHEEL_NOTCHES_PER_FRAME: i32 = 16;

fn remote_wheel_units(scroll_y: f32, is_vnc: bool, rdp_remainder: &mut f32) -> Option<i32> {
    if scroll_y == 0.0 {
        return None;
    }

    if is_vnc {
        *rdp_remainder = 0.0;
        // egui and VNC button 4/5 use the same positive-up direction.
        let direction = if scroll_y > 0.0 { 1 } else { -1 };
        let notches = (scroll_y.abs() / VNC_SCROLL_POINTS_PER_NOTCH)
            .ceil()
            .clamp(1.0, MAX_WHEEL_NOTCHES_PER_FRAME as f32) as i32;
        return Some(direction * 120 * notches);
    }

    // RDP follows egui's wheel direction. Accumulate high-resolution point
    // deltas so a touchpad does not turn every tiny event into a full notch.
    if *rdp_remainder != 0.0 && rdp_remainder.signum() != scroll_y.signum() {
        *rdp_remainder = 0.0;
    }
    let total = *rdp_remainder + scroll_y;
    let available_notches = (total.abs() / RDP_SCROLL_POINTS_PER_NOTCH).floor() as i32;
    if available_notches == 0 {
        *rdp_remainder = total;
        return None;
    }

    let direction = if total > 0.0 { 1 } else { -1 };
    let notches = available_notches.min(MAX_WHEEL_NOTCHES_PER_FRAME);
    *rdp_remainder = direction as f32 * (total.abs() % RDP_SCROLL_POINTS_PER_NOTCH);
    Some(direction * 120 * notches)
}

impl DesktopApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        init_runtime();
        apply_desktop_style(&cc.egui_ctx);
        // egui otherwise consumes Ctrl/Cmd +/- for local GUI zoom before the
        // remote view can own the combination.
        cc.egui_ctx
            .options_mut(|options| options.zoom_with_keyboard = false);

        let session = AppSession::load();
        let enable_hover_throttle = session.enable_hover_throttle;
        let hover_send_interval_ms = session.hover_send_interval_ms;
        let disable_rust_log = session.disable_rust_log;
        rust_rdp::set_disable_rust_log(disable_rust_log);

        let mut tabs = Vec::new();
        let mut next_id = 1u64;
        for tab_prefs in session.tabs {
            tabs.push(ConnectionTab::new(next_id, tab_prefs));
            next_id += 1;
        }

        let active_tab = session.active_tab.min(tabs.len().saturating_sub(1));

        Self {
            tabs,
            active_tab,
            next_tab_id: next_id,
            show_sidebar: true,
            show_about: false,
            fit_mode: FitMode::Fit,
            zoom: 1.0,
            window_fullscreen: false,
            view_fullscreen: false,
            sidebar_before_view_fs: true,
            remote_input_active: false,
            remote_input_owned_last_frame: false,
            system_input_capture: SystemInputCapture::new(cc),
            view_exit_overlay_rect: None,
            show_fullscreen_tabs: false,
            toast: None,
            pending_close_tab_id: None,
            enable_hover_throttle,
            hover_send_interval_ms,
            disable_rust_log,
        }
    }

    /// Save current session state (all open tabs + active tab) under XDG config.
    fn save_app_prefs(&self) {
        let Some(path) = Prefs::path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let mut text = String::new();
        text.push_str(&format!("active_tab={}\n", self.active_tab));
        text.push_str(&format!("enable_hover_throttle={}\n", self.enable_hover_throttle));
        text.push_str(&format!("hover_send_interval_ms={}\n", self.hover_send_interval_ms));
        text.push_str(&format!("disable_rust_log={}\n", self.disable_rust_log));
        text.push_str("\n");

        for tab in &self.tabs {
            text.push_str("[tab]\n");
            text.push_str(&format!("host={}\n", tab.prefs.host));
            text.push_str(&format!("port={}\n", tab.prefs.port));
            text.push_str(&format!("username={}\n", tab.prefs.username));
            text.push_str(&format!("password={}\n", tab.prefs.password));
            text.push_str(&format!("domain={}\n", tab.prefs.domain));
            text.push_str(&format!("mode={}\n", tab.prefs.mode));
            text.push_str(&format!("width={}\n", tab.prefs.width));
            text.push_str(&format!("height={}\n", tab.prefs.height));
            text.push_str(&format!("enable_hover_throttle={}\n", tab.prefs.enable_hover_throttle));
            text.push_str(&format!("hover_send_interval_ms={}\n", tab.prefs.hover_send_interval_ms));
            text.push_str(&format!("disable_rust_log={}\n", tab.prefs.disable_rust_log));
            text.push_str("\n");
        }

        let _ = std::fs::write(path, text);
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
        self.save_app_prefs();
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
        self.save_app_prefs();
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
            self.save_app_prefs();
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
        self.save_app_prefs();
    }

    /// Keyboard is owned by the remote session (no host shortcuts).
    fn keyboard_grabbed(&self) -> bool {
        self.view_fullscreen || self.remote_input_active
    }

    /// Hide app chrome so the remote desktop fills the client area.
    /// Also enters OS window fullscreen for an immersive session (like mstsc/Remmina).
    fn enter_view_fullscreen(&mut self, ctx: &egui::Context) {
        if self.view_fullscreen {
            return;
        }
        self.view_fullscreen = true;
        self.view_exit_overlay_rect = None;
        self.sidebar_before_view_fs = self.show_sidebar;
        self.show_sidebar = false;
        if !self.window_fullscreen {
            self.window_fullscreen = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
        }
        if !self.system_input_capture.set_captured(true) {
            self.show_toast(
                "The desktop compositor refused keyboard capture; some host shortcuts may remain active",
                ToastKind::Error,
            );
        }
    }

    fn exit_view_fullscreen(&mut self, ctx: &egui::Context) {
        if !self.view_fullscreen {
            return;
        }
        self.release_remote_input_state();
        self.system_input_capture.set_captured(false);
        self.view_fullscreen = false;
        self.view_exit_overlay_rect = None;
        self.show_fullscreen_tabs = false;
        self.remote_input_active = false;
        self.remote_input_owned_last_frame = false;
        self.show_sidebar = self.sidebar_before_view_fs;
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

    #[allow(dead_code)]
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
                    *tab.shared.status.lock() = msg.clone();
                }
                self.save_app_prefs();
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
                self.save_app_prefs();
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
        self.tab_mut().prefs = Prefs::default();
        *self.tab().shared.status.lock() = "Form cleared".into();
        self.save_app_prefs();
        self.show_toast("Form cleared", ToastKind::Info);
    }

    fn sync_modifiers(&mut self, modifiers: egui::Modifiers, native_super: bool) {
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
        let is_super = modifiers.mac_cmd || native_super;
        if is_super != tab.mod_super {
            send_scancode_event(0x5B, true, if is_super { 1 } else { 0 });
            tab.mod_super = is_super;
        }
    }

    /// Release every input state that could otherwise remain pressed on the
    /// remote host when the user leaves fullscreen with the mouse.
    fn release_remote_input_state(&mut self) {
        let tab = self.tab_mut();

        for key in tab.remote_keys_down.drain(..) {
            if let Some((scancode, extended)) = egui_key_to_scancode(key) {
                send_scancode_event(scancode, extended || is_extended_scancode(scancode), 0);
            } else if key == Key::Backspace {
                send_key_event(8, 0);
            } else if key == Key::Enter {
                send_key_event(13, 0);
            }
        }

        if tab.mod_shift {
            send_scancode_event(0x2A, false, 0);
            tab.mod_shift = false;
        }
        if tab.mod_ctrl {
            send_scancode_event(0x1D, false, 0);
            tab.mod_ctrl = false;
        }
        if tab.mod_alt {
            send_scancode_event(0x38, false, 0);
            tab.mod_alt = false;
        }
        if tab.mod_super {
            send_scancode_event(0x5B, true, 0);
            tab.mod_super = false;
        }

        if let Some((x, y)) = tab.last_mouse {
            if tab.left_down {
                send_mouse_event(x, y, 2);
                tab.left_down = false;
            }
            if tab.right_down {
                send_mouse_event(x, y, 4);
                tab.right_down = false;
            }
            if tab.middle_down {
                send_mouse_event(x, y, 6);
                tab.middle_down = false;
            }
            if tab.extra1_down {
                send_mouse_event(x, y, 8);
                tab.extra1_down = false;
            }
            if tab.extra2_down {
                send_mouse_event(x, y, 10);
                tab.extra2_down = false;
            }
        }
        tab.keyboard_state = RemoteKeyboardState::default();
    }

    fn start_connect(&mut self) {
        if !self.can_connect() {
            return;
        }

        // Drop any leftover backend session from a prior Failed attempt on this tab.
        if let Some(old) = self.tab_mut().backend_session_id.take() {
            disconnect_session_id(old);
        }

        self.save_app_prefs();

        let (host, port, username, password, domain, mode, width, height, endpoint, shared) = {
            let tab = self.tab();
            let default_port = if tab.prefs.mode == "VNC" { 5900 } else { 3389 };
            let port = tab.prefs.port.parse::<i32>().unwrap_or(default_port);
            let width = tab
                .prefs
                .width
                .parse::<i32>()
                .unwrap_or(1920)
                .clamp(640, 7680);
            let height = tab
                .prefs
                .height
                .parse::<i32>()
                .unwrap_or(1080)
                .clamp(480, 4320);
            (
                tab.prefs.host.trim().to_string(),
                port,
                tab.prefs.username.trim().to_string(),
                tab.prefs
                    .password
                    .trim_matches(|c| c == '\r' || c == '\n')
                    .to_string(),
                tab.prefs.domain.trim().to_string(),
                tab.prefs.mode.trim().to_string(),
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

        let cb: Arc<dyn SessionCallback> = Arc::new(UiCallback { ui: shared.clone() });

        let session_id = connect_session(
            host, port, username, password, domain, width, height, mode, cb,
        );
        self.tab_mut().backend_session_id = Some(session_id);
    }

    /// Disconnect the active tab's session (keeps the tab / form).
    fn disconnect(&mut self, ctx: &egui::Context) {
        if self.view_fullscreen {
            self.exit_view_fullscreen(ctx);
        } else {
            self.release_remote_input_state();
        }
        if let Some(sid) = self.tab_mut().backend_session_id.take() {
            disconnect_session_id(sid);
        }
        let tab = self.tab_mut();
        *tab.shared.state.lock() = ConnectionState::Idle;
        *tab.shared.status.lock() = "Disconnected".into();
        tab.left_down = false;
        tab.right_down = false;
        tab.middle_down = false;
        tab.extra1_down = false;
        tab.extra2_down = false;
        tab.last_mouse = None;
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
                    .add_enabled(
                        self.can_connect(),
                        egui::Button::new("Connect…\tCtrl+Return"),
                    )
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
                        "Exit view fullscreen"
                    } else {
                        "View fullscreen"
                    })
                    .on_hover_text(
                        "Hide chrome and capture the keyboard for the remote desktop. \
                         Move the pointer to the top edge and click Exit to leave.",
                    )
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

            ui.menu_button("Actions", |ui| {
                if ui.button("Send Ctrl+Alt+Del\tCtrl+Alt+End").clicked() {
                    send_scancode_event(0x1D, true, 1);
                    send_scancode_event(0x38, false, 1);
                    send_scancode_event(0x53, true, 1);
                    send_scancode_event(0x53, true, 0);
                    send_scancode_event(0x38, false, 0);
                    send_scancode_event(0x1D, true, 0);
                    ui.close_menu();
                }
                if ui.button("Send PrintScreen (PrtScn)").clicked() {
                    send_scancode_event(0x37, true, 1);
                    send_scancode_event(0x37, true, 0);
                    ui.close_menu();
                }
                if ui.button("Send Context Menu").clicked() {
                    send_scancode_event(0x5D, true, 1);
                    send_scancode_event(0x5D, true, 0);
                    ui.close_menu();
                }

                ui.separator();

                ui.menu_button("Volume & Sound", |ui| {
                    if ui.button("Volume Up (Vol+)").clicked() {
                        send_scancode_event(0x30, true, 1);
                        send_scancode_event(0x30, true, 0);
                        ui.close_menu();
                    }
                    if ui.button("Volume Down (Vol-)").clicked() {
                        send_scancode_event(0x2E, true, 1);
                        send_scancode_event(0x2E, true, 0);
                        ui.close_menu();
                    }
                    if ui.button("Mute Sound").clicked() {
                        send_scancode_event(0x20, true, 1);
                        send_scancode_event(0x20, true, 0);
                        ui.close_menu();
                    }
                });

                ui.menu_button("Display & Brightness", |ui| {
                    if ui.button("Increase Brightness (Bri+)").clicked() {
                        send_scancode_event(0x67, true, 1);
                        send_scancode_event(0x67, true, 0);
                        ui.close_menu();
                    }
                    if ui.button("Decrease Brightness (Bri-)").clicked() {
                        send_scancode_event(0x66, true, 1);
                        send_scancode_event(0x66, true, 0);
                        ui.close_menu();
                    }
                    if ui.button("Project / Display Switch (Win+P)").clicked() {
                        send_scancode_event(0x5B, true, 1);
                        send_scancode_event(0x19, false, 1);
                        send_scancode_event(0x19, false, 0);
                        send_scancode_event(0x5B, true, 0);
                        ui.close_menu();
                    }
                });

                ui.menu_button("Media Controls", |ui| {
                    if ui.button("Play / Pause").clicked() {
                        send_scancode_event(0x22, true, 1);
                        send_scancode_event(0x22, true, 0);
                        ui.close_menu();
                    }
                    if ui.button("Stop").clicked() {
                        send_scancode_event(0x24, true, 1);
                        send_scancode_event(0x24, true, 0);
                        ui.close_menu();
                    }
                    if ui.button("Next Track").clicked() {
                        send_scancode_event(0x19, true, 1);
                        send_scancode_event(0x19, true, 0);
                        ui.close_menu();
                    }
                    if ui.button("Previous Track").clicked() {
                        send_scancode_event(0x10, true, 1);
                        send_scancode_event(0x10, true, 0);
                        ui.close_menu();
                    }
                });

                ui.separator();
                if ui
                    .checkbox(&mut self.disable_rust_log, "Disable [Rust Log]")
                    .on_hover_text(
                        "Disable all '[Rust Log]' status messages (frame_id, bitmap updates, PDU logs, etc.)",
                    )
                    .changed()
                {
                    rust_rdp::set_disable_rust_log(self.disable_rust_log);
                    self.tab_mut().prefs.disable_rust_log = self.disable_rust_log;
                    self.save_app_prefs();
                }
            });

            let prev_throttle = self.enable_hover_throttle;
            let prev_interval = self.hover_send_interval_ms;
            ui.menu_button("Hover Throttle", |ui| {
                ui.checkbox(&mut self.enable_hover_throttle, "Limit Hover Frequency");
                if self.enable_hover_throttle {
                    ui.add(
                        egui::Slider::new(&mut self.hover_send_interval_ms, 50..=1000)
                            .step_by(10.0)
                            .suffix(" ms"),
                    );
                    self.hover_send_interval_ms = self.hover_send_interval_ms.clamp(50, 1000);
                } else {
                    ui.label(
                        RichText::new("Mode: Immediate (Real-time)")
                            .small()
                            .color(theme::TEXT_DIM),
                    );
                }
            });
            if self.enable_hover_throttle != prev_throttle
                || self.hover_send_interval_ms != prev_interval
            {
                self.tab_mut().prefs.enable_hover_throttle = self.enable_hover_throttle;
                self.tab_mut().prefs.hover_send_interval_ms = self.hover_send_interval_ms;
                self.save_app_prefs();
            }

            ui.menu_button("Help", |ui| {
                if ui.button("About Rust RDP VNC").clicked() {
                    self.show_about = true;
                    ui.close_menu();
                }
            });

            // ── Right side block pushed to topbar ──
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .selectable_label(self.view_fullscreen, "Fullscreen (F11)")
                    .on_hover_text(
                        "Fullscreen remote-only view (F11). Move pointer to top edge to exit.",
                    )
                    .clicked()
                {
                    self.toggle_view_fullscreen(ctx);
                }

                ui.separator();

                if ui
                    .button("1:1")
                    .on_hover_text("Actual size (100%)")
                    .clicked()
                {
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

    // ── Toolbar ─────────────────────────────────────────────────────────────

    fn ui_toolbar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;

            // 1. Panel (Sidebar toggle)
            if ui
                .selectable_label(self.show_sidebar, "☰ Panel")
                .on_hover_text("Show or hide the connection panel")
                .clicked()
            {
                self.show_sidebar = !self.show_sidebar;
            }

            ui.separator();

            // 2. + New
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

            // 3. Open
            if ui
                .button("Open")
                .on_hover_text("Open a .rdp / .vnc file and connect (Ctrl+O)")
                .clicked()
            {
                self.open_connection();
            }

            ui.separator();

            // 4. Session tabs & [ + ] button
            self.ui_tab_bar(ui, ctx);
        });
    }

    // ── Tab bar ─────────────────────────────────────────────────────────────

    fn ui_tab_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let mut select: Option<usize> = None;
        let mut double_click_tab: Option<usize> = None;
        let mut close: Option<usize> = None;
        let mut new_tab = false;

        egui::ScrollArea::horizontal()
            .id_salt("session_tabs_scroll")
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
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
                            .inner_margin(egui::Margin::symmetric(6, 2))
                            .show(ui, |ui| {
                                ui.horizontal_centered(|ui| {
                                    ui.spacing_mut().item_spacing.x = 4.0;

                                    let tab_title_resp = ui
                                        .horizontal(|ui| {
                                            ui.spacing_mut().item_spacing.x = 4.0;
                                            // Status dot
                                            let (dot, _) = ui.allocate_exact_size(
                                                Vec2::splat(7.0),
                                                egui::Sense::hover(),
                                            );
                                            ui.painter().circle_filled(dot.center(), 3.0, state.color());

                                            let label = RichText::new(title).small();
                                            ui.add(egui::Label::new(if selected {
                                                label.strong().color(theme::TEXT)
                                            } else {
                                                label.color(theme::TEXT_DIM)
                                            }));
                                        })
                                        .response;

                                    let title_resp = ui
                                        .interact(
                                            tab_title_resp.rect,
                                            ui.id().with(tab.tab_id),
                                            egui::Sense::click(),
                                        )
                                        .on_hover_text("Switch to this connection (double-click for fullscreen)");

                                    if title_resp.clicked() {
                                        select = Some(i);
                                    }
                                    if title_resp.double_clicked() {
                                        double_click_tab = Some(i);
                                    }

                                    let close_resp = ui
                                        .add(
                                            egui::Button::new(RichText::new("×").size(13.0))
                                                .frame(false)
                                                .min_size(Vec2::new(14.0, 14.0)),
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
                                .min_size(Vec2::new(24.0, 22.0)),
                        )
                        .on_hover_text("New connection (Ctrl+T)")
                        .clicked()
                    {
                        new_tab = true;
                    }
                });
            });

        if let Some(i) = double_click_tab {
            self.select_tab(i);
            self.enter_view_fullscreen(ctx);
        } else if let Some(i) = select {
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
                ui.label(RichText::new(format!("“{title}” is {state_label}.")).strong());
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

        ui.add_space(2.0);
        ui.label(RichText::new("Connection").strong().size(14.0));
        ui.label(
            RichText::new("Configure the remote session")
                .small()
                .color(theme::TEXT_DIM),
        );
        ui.add_space(4.0);
        ui.separator();
        ui.add_space(6.0);

        {
            let tab = self.tab_mut();
            egui::Grid::new(format!("conn_grid_{tab_id}"))
                .num_columns(2)
                .spacing([12.0, 5.0])
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
                                .hint_text(RichText::new("optional").color(Color32::from_rgb(0x60, 0x65, 0x70))),
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
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        ui.spacing_mut().button_padding.x = 2.0;
                        ui.spacing_mut().button_padding.y = 0.0;
                        let text_w = (ui.available_width() - 34.0).max(40.0);
                        let text_res = ui.add_enabled(
                            !busy,
                            egui::TextEdit::singleline(&mut tab.prefs.password)
                                .password(!tab.show_password)
                                .desired_width(text_w),
                        );
                        let btn_h = text_res.rect.height();
                        let icon = if tab.show_password { "🙈" } else { "👁" };
                        let tooltip = if tab.show_password {
                            "Hide password"
                        } else {
                            "Show password"
                        };
                        if ui
                            .add_enabled(
                                !busy,
                                egui::Button::new(icon).min_size(egui::vec2(22.0, btn_h)),
                            )
                            .on_hover_text(tooltip)
                            .clicked()
                        {
                            tab.show_password = !tab.show_password;
                        }
                    });
                    ui.end_row();
                });

            ui.add_space(8.0);
            ui.label(RichText::new("Display").strong().size(14.0));
            ui.add_space(4.0);
            ui.separator();
            ui.add_space(6.0);

            egui::Grid::new(format!("display_grid_{tab_id}"))
                .num_columns(2)
                .spacing([12.0, 5.0])
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

            ui.add_space(4.0);
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
                    let btn =
                        egui::Button::new(RichText::new("Connect").strong().color(Color32::WHITE))
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
                        egui::Button::new("Cancel").min_size(Vec2::new(ui.available_width(), 28.0)),
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
                    ui.label(
                        RichText::new("Connection failed")
                            .color(theme::DANGER)
                            .strong(),
                    );
                    ui.label(RichText::new(msg).small().color(theme::TEXT));
                });
        }

        ui.add_space(16.0);
        ui.separator();
        ui.add_space(8.0);
        ui.label(
            RichText::new(
                "Ctrl+T            New connection\nCtrl+W            Close tab\nCtrl+O            Open file\nCtrl+Return       Connect\nCtrl+S            Save as…\nCtrl+D            Disconnect\n\nIn View Fullscreen, all keyboard input\ngoes to remote. Move to the top edge\nand click Exit to leave.",
            )
            .small()
            .monospace()
            .color(theme::TEXT_DIM),
        );
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
                        ui.label(RichText::new(status).color(theme::TEXT_DIM).small());
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
        let view_fullscreen = self.view_fullscreen;
        let view_focused = response.has_focus() || response.hovered();
        let custom = self.tab().shared.custom_cursor.lock().clone();
        if view_focused || view_fullscreen {
            if custom.is_some() && response.hovered() {
                ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::None);
            } else {
                let cursor = *self.tab().shared.current_cursor.lock();
                ui.output_mut(|o| o.cursor_icon = cursor);
            }
        } else {
            ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::Default);
        }

        if let Some(c) = custom {
            if let Some(pos) = response.hover_pos() {
                let cursor_rect = egui::Rect::from_min_size(
                    pos - egui::vec2(c.hot_x as f32, c.hot_y as f32),
                    egui::vec2(c.width as f32, c.height as f32),
                );
                let color_pixels: Vec<egui::Color32> = c
                    .pixels
                    .iter()
                    .map(|&p| {
                        let u = p as u32;
                        let a = ((u >> 24) & 0xFF) as u8;
                        let r = ((u >> 16) & 0xFF) as u8;
                        let g = ((u >> 8) & 0xFF) as u8;
                        let b = (u & 0xFF) as u8;
                        egui::Color32::from_rgba_unmultiplied(r, g, b, a)
                    })
                    .collect();
                let color_image = egui::ColorImage {
                    size: [c.width as usize, c.height as usize],
                    pixels: color_pixels,
                };
                let texture = ui.ctx().load_texture(
                    "custom_cursor",
                    color_image,
                    egui::TextureOptions::NEAREST,
                );
                ui.painter().image(
                    texture.id(),
                    cursor_rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }
        }
        if connected && (view_focused || view_fullscreen) {
            self.remote_input_active = true;
            let modifiers = ui.input(|i| i.modifiers);
            let native_super = self.system_input_capture.super_pressed();
            self.sync_modifiers(modifiers, native_super);

            let native_events = self.system_input_capture.poll_native_events();
            for (scancode, ext, pressed) in native_events {
                send_scancode_event(scancode, ext, if pressed { 1 } else { 0 });
            }
        }

        // Prefer hover position so mouse move + wheel work without a button held.
        let pointer = response
            .hover_pos()
            .or_else(|| response.interact_pointer_pos())
            .or_else(|| ui.input(|i| i.pointer.latest_pos().filter(|p| rect.contains(*p))));

        let pointer_over_exit = view_fullscreen
            && pointer
                .and_then(|position| {
                    self.view_exit_overlay_rect
                        .map(|overlay| overlay.contains(position))
                })
                .unwrap_or(false);

        let enable_hover_throttle = self.enable_hover_throttle;
        let hover_send_interval_ms = self.hover_send_interval_ms;

        if let Some(pos) = pointer.filter(|_| !pointer_over_exit) {
            if let Some((x, y)) = self.remote_pos(pos, rect) {
                let tab = self.tab_mut();
                let moved = tab
                    .last_mouse
                    .map(|(lx, ly)| lx != x || ly != y)
                    .unwrap_or(true);

                let is_touchpad_jitter = if tab.left_down && !tab.left_down_dragged {
                    if let Some((ox, oy)) = tab.left_down_pos {
                        let dx = (x - ox).abs();
                        let dy = (y - oy).abs();
                        if dx > 8 || dy > 8 {
                            tab.left_down_dragged = true;
                            false
                        } else {
                            true
                        }
                    } else {
                        false
                    }
                } else {
                    false
                };

                if moved && !is_touchpad_jitter {
                    let is_hover = !tab.left_down
                        && !tab.right_down
                        && !tab.middle_down
                        && !tab.extra1_down
                        && !tab.extra2_down;
                    if enable_hover_throttle && is_hover {
                        let min_interval = std::time::Duration::from_millis(
                            hover_send_interval_ms.clamp(50, 1000),
                        );
                        let now = std::time::Instant::now();
                        let can_send = tab
                            .last_mouse_send_time
                            .map(|t| now.duration_since(t) >= min_interval)
                            .unwrap_or(true);
                        if can_send {
                            send_mouse_event(x, y, 0);
                            tab.last_mouse = Some((x, y));
                            tab.last_mouse_send_time = Some(now);
                        }
                    } else {
                        send_mouse_event(x, y, 0);
                        tab.last_mouse = Some((x, y));
                        tab.last_mouse_send_time = Some(std::time::Instant::now());
                    }
                }

                let buttons = ui.ctx().input(|i| {
                    (
                        i.pointer.button_pressed(egui::PointerButton::Primary),
                        i.pointer.button_released(egui::PointerButton::Primary),
                        i.pointer.button_pressed(egui::PointerButton::Secondary),
                        i.pointer.button_released(egui::PointerButton::Secondary),
                        i.pointer.button_pressed(egui::PointerButton::Middle),
                        i.pointer.button_released(egui::PointerButton::Middle),
                        i.pointer.button_pressed(egui::PointerButton::Extra1),
                        i.pointer.button_released(egui::PointerButton::Extra1),
                        i.pointer.button_pressed(egui::PointerButton::Extra2),
                        i.pointer.button_released(egui::PointerButton::Extra2),
                    )
                });

                if buttons.0 {
                    if tab.left_down {
                        send_mouse_event(x, y, 2);
                        tab.left_down = false;
                        tab.left_down_pos = None;
                        tab.left_down_dragged = false;
                    }
                    send_mouse_event(x, y, 1);
                    tab.left_down = true;
                    tab.left_down_pos = Some((x, y));
                    tab.left_down_dragged = false;
                }
                if buttons.1 && tab.left_down {
                    if buttons.0 {
                        // Defer release to next frame so mouse down has non-zero duration on remote OS
                        ui.ctx().request_repaint();
                    } else {
                        send_mouse_event(x, y, 2);
                        tab.left_down = false;
                        tab.left_down_pos = None;
                        tab.left_down_dragged = false;
                    }
                }
                if !buttons.0
                    && !buttons.1
                    && tab.left_down
                    && !ui
                        .ctx()
                        .input(|i| i.pointer.button_down(egui::PointerButton::Primary))
                {
                    send_mouse_event(x, y, 2);
                    tab.left_down = false;
                    tab.left_down_pos = None;
                    tab.left_down_dragged = false;
                }

                if buttons.2 {
                    send_mouse_event(x, y, 3);
                    tab.right_down = true;
                }
                if buttons.3 && tab.right_down {
                    if buttons.2 {
                        ui.ctx().request_repaint();
                    } else {
                        send_mouse_event(x, y, 4);
                        tab.right_down = false;
                    }
                }
                if !buttons.2
                    && !buttons.3
                    && tab.right_down
                    && !ui
                        .ctx()
                        .input(|i| i.pointer.button_down(egui::PointerButton::Secondary))
                {
                    send_mouse_event(x, y, 4);
                    tab.right_down = false;
                }
                if buttons.4 {
                    send_mouse_event(x, y, 5);
                    tab.middle_down = true;
                }
                if buttons.5 && tab.middle_down {
                    send_mouse_event(x, y, 6);
                    tab.middle_down = false;
                }
                if buttons.6 {
                    send_mouse_event(x, y, 7);
                    tab.extra1_down = true;
                }
                if buttons.7 && tab.extra1_down {
                    send_mouse_event(x, y, 8);
                    tab.extra1_down = false;
                }
                if buttons.8 {
                    send_mouse_event(x, y, 9);
                    tab.extra2_down = true;
                }
                if buttons.9 && tab.extra2_down {
                    send_mouse_event(x, y, 10);
                    tab.extra2_down = false;
                }

                // Wheel → always remote when over the surface.
                if response.hovered() || view_fullscreen {
                    let mut scroll_y = raw_scroll.y;
                    let mut scroll_x = raw_scroll.x;
                    if scroll_y == 0.0 && scroll_x == 0.0 {
                        ui.input(|i| {
                            for ev in &i.events {
                                if let egui::Event::MouseWheel { delta, .. } = ev {
                                    scroll_y += delta.y;
                                    scroll_x += delta.x;
                                }
                            }
                        });
                    }
                    let is_vnc = tab.prefs.mode.eq_ignore_ascii_case("VNC");
                    if let Some(units) =
                        remote_wheel_units(scroll_y, is_vnc, &mut tab.rdp_scroll_remainder)
                    {
                        send_mouse_wheel_event(x, y, units);
                    }
                    if let Some(units_h) =
                        remote_wheel_units(scroll_x, is_vnc, &mut tab.rdp_hscroll_remainder)
                    {
                        send_mouse_horizontal_wheel_event(x, y, units_h);
                    }
                }
            }
        }

        if !(response.has_focus() || response.hovered() || self.view_fullscreen) {
            return;
        }

        // Keep keyboard focus on the remote view while active
        response.request_focus();

        let modifiers = ui.input(|i| i.modifiers);
        let events: Vec<egui::Event> = ui.input(|i| i.events.clone());
        let mut processed_scancodes = Vec::new();

        for event in &events {
            let transitions = self.tab_mut().keyboard_state.transitions(event);
            for transition in transitions {
                {
                    let keys_down = &mut self.tab_mut().remote_keys_down;
                    if transition.pressed {
                        if !keys_down.contains(&transition.key) {
                            keys_down.push(transition.key);
                        }
                    } else {
                        keys_down.retain(|key| *key != transition.key);
                    }
                }

                let effective_key = transition
                    .physical_key
                    .filter(|k| egui_key_to_scancode(*k).is_some())
                    .unwrap_or(transition.key);

                // Map Ctrl+Alt+End to remote Ctrl+Alt+Delete to safely send Ctrl+Alt+Del without locking host OS
                let (scancode, ext) =
                    if modifiers.ctrl && modifiers.alt && transition.key == Key::End {
                        (0x53, true)
                    } else if let Some((code, ext)) = egui_key_to_scancode(effective_key) {
                        (code, ext || is_extended_scancode(code))
                    } else {
                        (0, false)
                    };

                if scancode != 0 {
                    send_scancode_event(scancode, ext, if transition.pressed { 1 } else { 0 });
                    if transition.pressed {
                        processed_scancodes.push(scancode);
                    }
                } else if transition.key == Key::Backspace {
                    send_key_event(8, if transition.pressed { 1 } else { 0 });
                } else if transition.key == Key::Enter {
                    send_key_event(13, if transition.pressed { 1 } else { 0 });
                }
            }

            // Fallback for character text events (e.g. !, {, }, @, #, $, %, ^, &, *, etc.) if not caught by Key events
            if let egui::Event::Text(ref text) = event {
                for ch in text.chars() {
                    if let Some((scancode, ext, needs_shift)) = char_to_scancode(ch) {
                        if !processed_scancodes.contains(&scancode) {
                            if needs_shift && !modifiers.shift {
                                send_scancode_event(0x2A, false, 1);
                            }
                            send_scancode_event(scancode, ext, 1);
                            send_scancode_event(scancode, ext, 0);
                            if needs_shift && !modifiers.shift {
                                send_scancode_event(0x2A, false, 0);
                            }
                        }
                    }
                }
            }
        }
    }

    // ── Global shortcuts ────────────────────────────────────────────────────

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
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
            // App shortcuts are only available while the remote view does not
            // own the keyboard.
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

    /// Thin floating bar to exit view fullscreen or quick switch tabs (mouse to top edge).
    fn ui_view_fullscreen_overlay(&mut self, ctx: &egui::Context) {
        if !self.view_fullscreen {
            self.show_fullscreen_tabs = false;
            return;
        }

        let (pointer, screen_rect) =
            ctx.input(|input| (input.pointer.latest_pos(), input.screen_rect()));
        let reveal_rect = egui::Rect::from_center_size(
            egui::pos2(
                screen_rect.center().x,
                screen_rect.top() + VIEW_EXIT_REVEAL_HEIGHT / 2.0,
            ),
            egui::vec2(
                screen_rect.width() * VIEW_EXIT_REVEAL_WIDTH_FRACTION,
                VIEW_EXIT_REVEAL_HEIGHT,
            ),
        );
        let near_exit = pointer
            .map(|position| {
                reveal_rect.contains(position)
                    || self
                        .view_exit_overlay_rect
                        .map(|overlay| overlay.contains(position))
                        .unwrap_or(false)
            })
            .unwrap_or(false);

        let mut exit_clicked = false;
        let mut toggle_tabs_clicked = false;
        let mut select_tab_index: Option<usize> = None;
        let mut close_popup = false;

        // Top edge floating bar: [ Exit ] [ • ]
        if near_exit {
            let overlay = egui::Area::new(egui::Id::new("view_fs_overlay"))
                .anchor(egui::Align2::CENTER_TOP, [0.0, 4.0])
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(Color32::from_black_alpha(220))
                        .stroke(egui::Stroke::new(1.0_f32, theme::BORDER))
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::symmetric(6, 3))
                        .show(ui, |ui| {
                            ui.horizontal_centered(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;

                                let exit_res = ui
                                    .small_button("Exit")
                                    .on_hover_text("Exit fullscreen view");
                                if exit_res.clicked() {
                                    exit_clicked = true;
                                }

                                let exit_h = exit_res.rect.height();

                                let dot_btn = ui
                                    .add_sized(
                                        Vec2::splat(exit_h),
                                        egui::Button::new(RichText::new("•").size(10.0).strong())
                                            .small(),
                                    )
                                    .on_hover_text(if self.show_fullscreen_tabs {
                                        "Close tab manager"
                                    } else {
                                        "Quick tab manager"
                                    });

                                if dot_btn.clicked() {
                                    toggle_tabs_clicked = true;
                                }
                            });
                        });
                });
            self.view_exit_overlay_rect = Some(overlay.response.rect);
        } else {
            self.view_exit_overlay_rect = None;
        }

        // Separate fixed 90% width x 90% height popup modal for switching tabs inside fullscreen mode
        if self.show_fullscreen_tabs {
            let active_tab = self.active_tab;
            let popup_w = (screen_rect.width() * 0.90).max(280.0);
            let popup_h = (screen_rect.height() * 0.90).max(200.0);
            let mut close_tab_index: Option<usize> = None;

            egui::Area::new(egui::Id::new("fs_tabs_popup_area"))
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(Color32::from_black_alpha(248))
                        .stroke(egui::Stroke::new(1.5_f32, theme::ACCENT))
                        .corner_radius(8.0)
                        .inner_margin(egui::Margin::symmetric(16, 12))
                        .show(ui, |ui| {
                            ui.set_min_size(Vec2::new(popup_w, popup_h));
                            ui.set_max_size(Vec2::new(popup_w, popup_h));
                            ui.vertical(|ui| {
                                // Header row
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(format!(
                                            "Switch Connection ({} tabs)",
                                            self.tabs.len()
                                        ))
                                        .strong()
                                        .size(15.0)
                                        .color(theme::TEXT),
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui
                                                .add(
                                                    egui::Button::new(
                                                        RichText::new("×").size(18.0).strong(),
                                                    )
                                                    .frame(false)
                                                    .min_size(Vec2::new(24.0, 24.0)),
                                                )
                                                .on_hover_text("Close modal")
                                                .clicked()
                                            {
                                                close_popup = true;
                                            }
                                        },
                                    );
                                });

                                ui.add_space(6.0);
                                ui.separator();
                                ui.add_space(10.0);

                                // Vertical scrollable grid of uniform 220x80 tab cards
                                egui::ScrollArea::vertical()
                                    .id_salt("fs_tabs_popup_scroll")
                                    .show(ui, |ui| {
                                        ui.horizontal_wrapped(|ui| {
                                            ui.spacing_mut().item_spacing = Vec2::new(12.0, 12.0);
                                            for (i, tab) in self.tabs.iter().enumerate() {
                                                let selected = i == active_tab;
                                                let title = tab.tab_title();
                                                let state = *tab.shared.state.lock();

                                                let fill = if selected {
                                                    theme::PANEL_ALT
                                                } else {
                                                    theme::PANEL
                                                };
                                                let stroke = if selected {
                                                    egui::Stroke::new(2.0_f32, theme::ACCENT)
                                                } else {
                                                    egui::Stroke::new(1.0_f32, theme::BORDER)
                                                };

                                                let card_w = 230.0;
                                                let card_h = 88.0;
                                                let mut card_close_clicked = false;

                                                let (card_rect, card_response) = ui.allocate_exact_size(
                                                    Vec2::new(card_w, card_h),
                                                    egui::Sense::click(),
                                                );

                                                ui.painter().rect_filled(card_rect, 6.0, fill);
                                                ui.painter().rect_stroke(card_rect, 6.0, stroke, egui::StrokeKind::Outside);

                                                let mut child_ui = ui.new_child(
                                                    egui::UiBuilder::new()
                                                        .max_rect(card_rect.shrink2(Vec2::new(12.0, 10.0)))
                                                        .layout(egui::Layout::top_down(egui::Align::LEFT)),
                                                );

                                                child_ui.horizontal(|ui| {
                                                    let (dot, _) = ui.allocate_exact_size(
                                                        Vec2::splat(9.0),
                                                        egui::Sense::hover(),
                                                    );
                                                    ui.painter().circle_filled(
                                                        dot.center(),
                                                        4.0,
                                                        state.color(),
                                                    );
                                                    ui.label(
                                                        RichText::new(state.label())
                                                            .small()
                                                            .color(state.color()),
                                                    );

                                                    ui.with_layout(
                                                        egui::Layout::right_to_left(egui::Align::Center),
                                                        |ui| {
                                                            let (close_rect, close_resp) = ui
                                                                .allocate_exact_size(
                                                                    Vec2::splat(18.0),
                                                                    egui::Sense::click(),
                                                                );
                                                            let close_resp = close_resp
                                                                .on_hover_text("Close tab and disconnect");
                                                            let is_close_hovered =
                                                                close_resp.hovered();

                                                            if is_close_hovered {
                                                                ui.painter().circle_filled(
                                                                    close_rect.center(),
                                                                    8.0,
                                                                    Color32::from_rgb(210, 50, 50),
                                                                );
                                                            }

                                                            let x_color = if is_close_hovered {
                                                                Color32::WHITE
                                                            } else {
                                                                theme::TEXT_DIM
                                                            };

                                                            ui.painter().text(
                                                                close_rect.center(),
                                                                egui::Align2::CENTER_CENTER,
                                                                "×",
                                                                egui::FontId::proportional(14.0),
                                                                x_color,
                                                            );

                                                            if close_resp.clicked() {
                                                                card_close_clicked = true;
                                                            }

                                                            ui.add_space(4.0);
                                                            ui.label(
                                                                RichText::new(format!("• {}", tab.prefs.mode))
                                                                    .small()
                                                                    .monospace()
                                                                    .color(theme::TEXT_DIM),
                                                            );
                                                        },
                                                    );
                                                });

                                                child_ui.add_space(4.0);
                                                let label = RichText::new(&title).strong().size(13.0);
                                                child_ui.add(
                                                    egui::Label::new(if selected {
                                                        label.color(theme::TEXT)
                                                    } else {
                                                        label.color(theme::TEXT_DIM)
                                                    })
                                                    .truncate(),
                                                );

                                                if !tab.prefs.username.is_empty() {
                                                    child_ui.label(
                                                        RichText::new(format!("User: {}", tab.prefs.username))
                                                            .small()
                                                            .color(theme::TEXT_DIM),
                                                    );
                                                }

                                                if card_close_clicked {
                                                    close_tab_index = Some(i);
                                                } else if card_response.clicked() {
                                                    select_tab_index = Some(i);
                                                    close_popup = true;
                                                }
                                            }
                                        });
                                    });
                            });
                        });
                });

            if let Some(i) = close_tab_index {
                self.request_close_tab(i, ctx);
            }
        }

        if toggle_tabs_clicked {
            self.show_fullscreen_tabs = !self.show_fullscreen_tabs;
        }

        if close_popup {
            self.show_fullscreen_tabs = false;
        }

        if let Some(i) = select_tab_index {
            self.select_tab(i);
        }

        if exit_clicked {
            self.exit_view_fullscreen(ctx);
        }
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

    // Set double-click options optimal for both mouse and touchpad taps (350ms delay, 25.0px tolerance)
    ctx.options_mut(|o| {
        o.input_options.max_double_click_delay = 0.35;
        o.input_options.max_click_dist = 25.0;
    });

    // Compact, readable desktop density
    if let Some(font_id) = style.text_styles.get_mut(&egui::TextStyle::Body) {
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
        ctx.input_mut(|i| {
            ensure_text_events_for_keyboard_input(&mut i.events);
        });

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
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
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
                        RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION")))
                            .small()
                            .color(theme::TEXT_DIM),
                    );
                });
        }

        // Close-tab confirm when session is still connecting/connected
        self.ui_close_tab_confirm(ctx);

        // Floating toast (save feedback, errors, …)
        self.ui_toast(ctx);

        // Keep compositor/window-manager shortcuts inhibited whenever the
        // connected remote surface owns keyboard focus, not only in fullscreen.
        let window_focused = ctx.input(|input| input.viewport().focused.unwrap_or(true));
        let remote_owns_input = self.keyboard_grabbed() && window_focused;
        if self.remote_input_owned_last_frame && !remote_owns_input {
            self.release_remote_input_state();
        }
        self.system_input_capture.set_captured(remote_owns_input);
        self.remote_input_owned_last_frame = remote_owns_input;

        // App shortcuts after the view has marked keyboard ownership.
        self.handle_shortcuts(ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_app_prefs();
        self.release_remote_input_state();
        self.system_input_capture.set_captured(false);
        disconnect_session();
    }
}

fn load_app_icon() -> egui::IconData {
    let bytes = include_bytes!("../assets/icon-512.png");
    if let Ok(img) = image::load_from_memory_with_format(bytes, image::ImageFormat::Png) {
        let rgba = img.to_rgba8();
        let (width, height) = rgba.dimensions();
        egui::IconData {
            rgba: rgba.into_raw(),
            width,
            height,
        }
    } else {
        egui::IconData::default()
    }
}

fn main() -> eframe::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_app_id("io.github.manhavn.rust-rdp-vnc")
            .with_icon(load_app_icon())
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([960.0, 600.0])
            .with_title("Rust RDP VNC"),
        hardware_acceleration: eframe::HardwareAcceleration::Preferred,
        renderer: eframe::Renderer::Glow,
        vsync: true,
        multisampling: 4,
        ..Default::default()
    };

    let result = eframe::run_native(
        "Rust RDP VNC",
        options.clone(),
        Box::new(|cc| Ok(Box::new(DesktopApp::new(cc)))),
    );

    #[cfg(target_os = "linux")]
    if let Err(ref err) = result {
        let err_str = format!("{err:?}");
        if err_str.contains("WaylandError")
            || err_str.contains("NoWaylandLib")
            || err_str.contains("WinitEventLoop")
        {
            log::warn!("Wayland event loop initialization failed ({err_str}), falling back to X11 backend...");
            std::env::set_var("WINIT_UNIX_BACKEND", "x11");
            return eframe::run_native(
                "Rust RDP VNC",
                options,
                Box::new(|cc| Ok(Box::new(DesktopApp::new(cc)))),
            );
        }
    }

    result
}

/// Synthesizes missing `egui::Event::Text` for printable keyboard inputs (such as
/// letters, numbers, shift symbols, punctuation, and space) when Linux backend/winit fails to
/// emit character text for key presses.
fn ensure_text_events_for_keyboard_input(events: &mut Vec<egui::Event>) {
    let mut synthetic_text = Vec::new();
    for event in events.iter() {
        if let egui::Event::Key {
            key,
            pressed: true,
            modifiers,
            ..
        } = event
        {
            if !modifiers.ctrl && !modifiers.alt && !modifiers.command && !modifiers.mac_cmd {
                let text_symbol = match key {
                    // Letters
                    Key::A => Some(if modifiers.shift { "A" } else { "a" }),
                    Key::B => Some(if modifiers.shift { "B" } else { "b" }),
                    Key::C => Some(if modifiers.shift { "C" } else { "c" }),
                    Key::D => Some(if modifiers.shift { "D" } else { "d" }),
                    Key::E => Some(if modifiers.shift { "E" } else { "e" }),
                    Key::F => Some(if modifiers.shift { "F" } else { "f" }),
                    Key::G => Some(if modifiers.shift { "G" } else { "g" }),
                    Key::H => Some(if modifiers.shift { "H" } else { "h" }),
                    Key::I => Some(if modifiers.shift { "I" } else { "i" }),
                    Key::J => Some(if modifiers.shift { "J" } else { "j" }),
                    Key::K => Some(if modifiers.shift { "K" } else { "k" }),
                    Key::L => Some(if modifiers.shift { "L" } else { "l" }),
                    Key::M => Some(if modifiers.shift { "M" } else { "m" }),
                    Key::N => Some(if modifiers.shift { "N" } else { "n" }),
                    Key::O => Some(if modifiers.shift { "O" } else { "o" }),
                    Key::P => Some(if modifiers.shift { "P" } else { "p" }),
                    Key::Q => Some(if modifiers.shift { "Q" } else { "q" }),
                    Key::R => Some(if modifiers.shift { "R" } else { "r" }),
                    Key::S => Some(if modifiers.shift { "S" } else { "s" }),
                    Key::T => Some(if modifiers.shift { "T" } else { "t" }),
                    Key::U => Some(if modifiers.shift { "U" } else { "u" }),
                    Key::V => Some(if modifiers.shift { "V" } else { "v" }),
                    Key::W => Some(if modifiers.shift { "W" } else { "w" }),
                    Key::X => Some(if modifiers.shift { "X" } else { "x" }),
                    Key::Y => Some(if modifiers.shift { "Y" } else { "y" }),
                    Key::Z => Some(if modifiers.shift { "Z" } else { "z" }),

                    // Numbers & Top row shift symbols
                    Key::Num1 => Some(if modifiers.shift { "!" } else { "1" }),
                    Key::Num2 => Some(if modifiers.shift { "@" } else { "2" }),
                    Key::Num3 => Some(if modifiers.shift { "#" } else { "3" }),
                    Key::Num4 => Some(if modifiers.shift { "$" } else { "4" }),
                    Key::Num5 => Some(if modifiers.shift { "%" } else { "5" }),
                    Key::Num6 => Some(if modifiers.shift { "^" } else { "6" }),
                    Key::Num7 => Some(if modifiers.shift { "&" } else { "7" }),
                    Key::Num8 => Some(if modifiers.shift { "*" } else { "8" }),
                    Key::Num9 => Some(if modifiers.shift { "(" } else { "9" }),
                    Key::Num0 => Some(if modifiers.shift { ")" } else { "0" }),

                    // Punctuation & Symbols
                    Key::Period => Some(if modifiers.shift { ">" } else { "." }),
                    Key::Comma => Some(if modifiers.shift { "<" } else { "," }),
                    Key::Minus => Some(if modifiers.shift { "_" } else { "-" }),
                    Key::Equals => Some(if modifiers.shift { "+" } else { "=" }),
                    Key::Plus => Some("+"),
                    Key::Slash => Some(if modifiers.shift { "?" } else { "/" }),
                    Key::Questionmark => Some("?"),
                    Key::Backslash => Some(if modifiers.shift { "|" } else { "\\" }),
                    Key::Pipe => Some("|"),
                    Key::Semicolon => Some(if modifiers.shift { ":" } else { ";" }),
                    Key::Colon => Some(":"),
                    Key::Quote => Some(if modifiers.shift { "\"" } else { "'" }),
                    Key::Backtick => Some(if modifiers.shift { "~" } else { "`" }),
                    Key::OpenBracket => Some(if modifiers.shift { "{" } else { "[" }),
                    Key::CloseBracket => Some(if modifiers.shift { "}" } else { "]" }),
                    Key::Space => Some(" "),
                    _ => None,
                };

                if let Some(symbol) = text_symbol {
                    let already_has_text = events.iter().any(|e| match e {
                        egui::Event::Text(t) => t == symbol,
                        _ => false,
                    });
                    if !already_has_text {
                        synthetic_text.push(egui::Event::Text(symbol.to_string()));
                    }
                }
            }
        }
    }
    events.extend(synthetic_text);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthesizes_missing_text_event_for_period_key() {
        let mut events = vec![egui::Event::Key {
            key: Key::Period,
            physical_key: Some(Key::Period),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }];
        ensure_text_events_for_keyboard_input(&mut events);
        assert_eq!(events.len(), 2);
        assert_eq!(events[1], egui::Event::Text(".".to_string()));
    }

    #[test]
    fn synthesizes_shift_period_key_as_greater_than() {
        let mut events = vec![egui::Event::Key {
            key: Key::Period,
            physical_key: Some(Key::Period),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::SHIFT,
        }];
        ensure_text_events_for_keyboard_input(&mut events);
        assert_eq!(events.len(), 2);
        assert_eq!(events[1], egui::Event::Text(">".to_string()));
    }

    #[test]
    fn does_not_duplicate_existing_text_event_for_period_key() {
        let mut events = vec![
            egui::Event::Key {
                key: Key::Period,
                physical_key: Some(Key::Period),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::Text(".".to_string()),
        ];
        ensure_text_events_for_keyboard_input(&mut events);
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn synthesizes_shift_number_two_as_at_symbol() {
        let mut events = vec![egui::Event::Key {
            key: Key::Num2,
            physical_key: Some(Key::Num2),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::SHIFT,
        }];
        ensure_text_events_for_keyboard_input(&mut events);
        assert_eq!(events.len(), 2);
        assert_eq!(events[1], egui::Event::Text("@".to_string()));
    }

    #[test]
    fn synthesizes_letter_keys_correctly() {
        let mut events = vec![
            egui::Event::Key {
                key: Key::A,
                physical_key: Some(Key::A),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::Key {
                key: Key::B,
                physical_key: Some(Key::B),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::SHIFT,
            },
        ];
        ensure_text_events_for_keyboard_input(&mut events);
        assert_eq!(events.len(), 4);
        assert_eq!(events[2], egui::Event::Text("a".to_string()));
        assert_eq!(events[3], egui::Event::Text("B".to_string()));
    }

    #[test]
    fn vnc_wheel_uses_correct_direction_and_faster_speed() {
        let mut remainder = 0.0;
        assert_eq!(remote_wheel_units(40.0, true, &mut remainder), Some(1680));
        assert_eq!(remote_wheel_units(-40.0, true, &mut remainder), Some(-1680));
    }

    #[test]
    fn rdp_wheel_uses_one_notch_per_native_line_and_correct_direction() {
        let mut remainder = 0.0;
        assert_eq!(remote_wheel_units(40.0, false, &mut remainder), Some(120));
        assert_eq!(remote_wheel_units(-40.0, false, &mut remainder), Some(-120));
    }

    #[test]
    fn rdp_wheel_accumulates_high_resolution_deltas() {
        let mut remainder = 0.0;
        assert_eq!(remote_wheel_units(15.0, false, &mut remainder), None);
        assert_eq!(remote_wheel_units(15.0, false, &mut remainder), None);
        assert_eq!(remote_wheel_units(15.0, false, &mut remainder), Some(120));
        assert_eq!(remainder, 13.0);
    }

    #[test]
    fn rdp_wheel_is_twenty_five_percent_faster_than_native_lines() {
        let mut remainder = 0.0;
        let units: i32 = (0..4)
            .filter_map(|_| remote_wheel_units(40.0, false, &mut remainder))
            .sum();
        assert_eq!(units, 600);
    }

    #[test]
    fn app_session_parses_multi_tab_config() {
        let input = r#"
active_tab=1
enable_hover_throttle=true
hover_send_interval_ms=500
disable_rust_log=true

[tab]
host=192.168.1.10
port=3389
username=admin
mode=RDP

[tab]
host=10.0.0.5
port=5900
username=vncuser
mode=VNC
"#;
        let session = AppSession::parse(input);
        assert_eq!(session.active_tab, 1);
        assert_eq!(session.enable_hover_throttle, true);
        assert_eq!(session.hover_send_interval_ms, 500);
        assert_eq!(session.disable_rust_log, true);
        assert_eq!(session.tabs.len(), 2);
        assert_eq!(session.tabs[0].host, "192.168.1.10");
        assert_eq!(session.tabs[0].mode, "RDP");
        assert_eq!(session.tabs[1].host, "10.0.0.5");
        assert_eq!(session.tabs[1].mode, "VNC");
    }

    #[test]
    fn app_session_parses_legacy_single_tab_config() {
        let input = "host=192.168.1.100\nport=3389\nusername=user\nmode=RDP\n";
        let session = AppSession::parse(input);
        assert_eq!(session.active_tab, 0);
        assert_eq!(session.tabs.len(), 1);
        assert_eq!(session.tabs[0].host, "192.168.1.100");
        assert_eq!(session.tabs[0].port, "3389");
    }
}
