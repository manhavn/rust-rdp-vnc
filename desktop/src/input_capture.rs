//! Block host desktop shortcuts while the remote view owns input.
//!
//! Consuming `egui` key events is not sufficient on Linux: X11 window managers
//! and Wayland compositors see their global shortcuts before the application.
//! This module uses the native input-inhibition mechanism of the active window
//! backend and releases it when the remote view gives up keyboard focus.

#[cfg(target_os = "linux")]
mod linux {
    use eframe::CreationContext;
    use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};

    pub struct SystemInputCapture {
        backend: Backend,
        capture_failed: bool,
    }

    enum Backend {
        X11(Box<X11Capture>),
        Wayland(Box<WaylandCapture>),
        Unsupported,
    }

    impl SystemInputCapture {
        pub fn new(cc: &CreationContext<'_>) -> Self {
            let window = cc.window_handle().map(|handle| handle.as_raw());
            let display = cc.display_handle().map(|handle| handle.as_raw());

            let backend = match (window, display) {
                (Ok(RawWindowHandle::Xlib(window)), Ok(RawDisplayHandle::Xlib(display))) => {
                    match display.display {
                        Some(display) => {
                            match X11Capture::new(display.as_ptr().cast(), window.window) {
                                Ok(capture) => Backend::X11(Box::new(capture)),
                                Err(error) => {
                                    log::warn!("X11 keyboard capture is unavailable: {error}");
                                    Backend::Unsupported
                                }
                            }
                        }
                        None => {
                            log::warn!("X11 keyboard capture is unavailable: null Xlib display");
                            Backend::Unsupported
                        }
                    }
                }
                (Ok(RawWindowHandle::Wayland(window)), Ok(RawDisplayHandle::Wayland(display))) => {
                    match WaylandCapture::new(display.display.as_ptr(), window.surface.as_ptr()) {
                        Ok(capture) => Backend::Wayland(Box::new(capture)),
                        Err(error) => {
                            log::warn!("Wayland shortcut inhibition is unavailable: {error}");
                            Backend::Unsupported
                        }
                    }
                }
                (Ok(window), Ok(display)) => {
                    log::warn!(
                        "Keyboard capture is unsupported for window backend {window:?}/{display:?}"
                    );
                    Backend::Unsupported
                }
                (Err(error), _) | (_, Err(error)) => {
                    log::warn!("Could not obtain the native window handle: {error}");
                    Backend::Unsupported
                }
            };

            Self {
                backend,
                capture_failed: false,
            }
        }

        /// Capture or release host keyboard shortcuts.
        ///
        /// Returns `true` only when the requested state is effective. An
        /// unsupported compositor therefore never claims that input is safe.
        pub fn set_captured(&mut self, captured: bool) -> bool {
            if captured && self.capture_failed {
                return false;
            }
            if !captured {
                // Permit one fresh attempt the next time the remote surface
                // takes ownership.
                self.capture_failed = false;
            }

            let result = match &mut self.backend {
                Backend::X11(capture) => capture.set_captured(captured),
                Backend::Wayland(capture) => capture.set_captured(captured),
                Backend::Unsupported => return !captured,
            };

            if let Err(error) = result {
                log::warn!("Could not change host keyboard capture: {error}");
                if captured {
                    self.capture_failed = true;
                }
                false
            } else {
                true
            }
        }

        /// Read the native Linux Super/Windows modifier state.
        ///
        /// egui intentionally exposes `mac_cmd` only on macOS, so Linux needs
        /// a backend-specific path for the Super key.
        pub fn super_pressed(&mut self) -> bool {
            match &mut self.backend {
                Backend::X11(capture) => capture.super_pressed(),
                Backend::Wayland(capture) => capture.super_pressed(),
                Backend::Unsupported => false,
            }
        }
    }

    struct X11Capture {
        xlib: x11_dl::xlib::Xlib,
        display: *mut x11_dl::xlib::Display,
        window: std::os::raw::c_ulong,
        super_keycodes: [u8; 2],
        captured: bool,
    }

    impl X11Capture {
        fn new(
            display: *mut x11_dl::xlib::Display,
            window: std::os::raw::c_ulong,
        ) -> Result<Self, String> {
            let xlib = x11_dl::xlib::Xlib::open().map_err(|error| error.to_string())?;
            // SAFETY: `display` is eframe's live Xlib connection.
            let super_keycodes = unsafe {
                [
                    (xlib.XKeysymToKeycode)(
                        display,
                        x11_dl::keysym::XK_Super_L as std::os::raw::c_ulong,
                    ),
                    (xlib.XKeysymToKeycode)(
                        display,
                        x11_dl::keysym::XK_Super_R as std::os::raw::c_ulong,
                    ),
                ]
            };

            Ok(Self {
                xlib,
                display,
                window,
                super_keycodes,
                captured: false,
            })
        }

        fn set_captured(&mut self, captured: bool) -> Result<(), String> {
            if captured == self.captured {
                return Ok(());
            }

            if captured {
                // SAFETY: `display` and `window` remain valid for the app
                // lifetime. Async modes keep the X event loop responsive.
                let status = unsafe {
                    (self.xlib.XGrabKeyboard)(
                        self.display,
                        self.window,
                        x11_dl::xlib::True,
                        x11_dl::xlib::GrabModeAsync,
                        x11_dl::xlib::GrabModeAsync,
                        x11_dl::xlib::CurrentTime,
                    )
                };
                if status != x11_dl::xlib::GrabSuccess {
                    return Err(format!("XGrabKeyboard failed with status {status}"));
                }
            } else {
                // SAFETY: this releases the active grab owned by `display`.
                unsafe {
                    (self.xlib.XUngrabKeyboard)(self.display, x11_dl::xlib::CurrentTime);
                }
            }

            // SAFETY: flush the requests sent through our valid X connection.
            unsafe {
                (self.xlib.XFlush)(self.display);
            }
            self.captured = captured;
            Ok(())
        }

        fn super_pressed(&self) -> bool {
            let mut keymap = [0 as std::os::raw::c_char; 32];
            // SAFETY: XQueryKeymap writes exactly 32 bytes to the supplied
            // buffer and `display` remains valid for the app lifetime.
            unsafe {
                (self.xlib.XQueryKeymap)(self.display, keymap.as_mut_ptr());
            }

            self.super_keycodes
                .iter()
                .copied()
                .any(|keycode| keycode_is_pressed(&keymap, keycode))
        }
    }

    fn keycode_is_pressed(keymap: &[std::os::raw::c_char; 32], keycode: u8) -> bool {
        keycode != 0 && (keymap[usize::from(keycode / 8)] as u8 & (1_u8 << (keycode % 8))) != 0
    }

    #[cfg(test)]
    mod x11_tests {
        use super::keycode_is_pressed;

        #[test]
        fn reads_x11_keymap_bits() {
            let mut keymap = [0 as std::os::raw::c_char; 32];
            let keycode = 133_u8;
            keymap[usize::from(keycode / 8)] = (1_u8 << (keycode % 8)) as _;

            assert!(keycode_is_pressed(&keymap, keycode));
            assert!(!keycode_is_pressed(&keymap, 134));
            assert!(!keycode_is_pressed(&keymap, 0));
        }
    }

    impl Drop for X11Capture {
        fn drop(&mut self) {
            let _ = self.set_captured(false);
        }
    }

    mod wayland {
        use std::ffi::c_void;

        use wayland_backend::client::{Backend, ObjectId};
        use wayland_client::{
            globals::{registry_queue_init, GlobalListContents},
            protocol::{wl_keyboard, wl_registry, wl_seat, wl_surface},
            Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum,
        };
        use wayland_protocols::wp::keyboard_shortcuts_inhibit::zv1::client::{
            zwp_keyboard_shortcuts_inhibit_manager_v1::ZwpKeyboardShortcutsInhibitManagerV1,
            zwp_keyboard_shortcuts_inhibitor_v1::{self, ZwpKeyboardShortcutsInhibitorV1},
        };

        pub(super) struct WaylandCapture {
            connection: Connection,
            queue: EventQueue<WaylandState>,
            state: WaylandState,
            manager: ZwpKeyboardShortcutsInhibitManagerV1,
            seat: wl_seat::WlSeat,
            keyboard: wl_keyboard::WlKeyboard,
            surface: wl_surface::WlSurface,
            inhibitor: Option<ZwpKeyboardShortcutsInhibitorV1>,
        }

        #[derive(Default)]
        struct WaylandState {
            active: bool,
            super_keys_down: [bool; 2],
        }

        const KEY_LEFTMETA: u32 = 125;
        const KEY_RIGHTMETA: u32 = 126;

        impl WaylandCapture {
            pub(super) fn new(
                display_ptr: *mut c_void,
                surface_ptr: *mut c_void,
            ) -> Result<Self, String> {
                // SAFETY: the raw handles are borrowed from eframe and remain
                // valid for the lifetime of the application. The foreign
                // backend never disconnects or owns the display.
                let backend = unsafe { Backend::from_foreign_display(display_ptr.cast()) };
                let connection = Connection::from_backend(backend);
                let (globals, queue) =
                    registry_queue_init::<WaylandState>(&connection).map_err(display_error)?;
                let qh = queue.handle();

                let manager = globals
                    .bind::<ZwpKeyboardShortcutsInhibitManagerV1, _, _>(&qh, 1..=1, ())
                    .map_err(display_error)?;
                let seat = globals
                    .bind::<wl_seat::WlSeat, _, _>(
                        &qh,
                        1..=wl_seat::WlSeat::interface().version,
                        (),
                    )
                    .map_err(display_error)?;

                // SAFETY: this is the live wl_surface from eframe's Wayland
                // window. We only pass it as an object argument; ownership and
                // event dispatch remain with winit.
                let surface_id = unsafe {
                    ObjectId::from_ptr(wl_surface::WlSurface::interface(), surface_ptr.cast())
                }
                .map_err(display_error)?;
                let surface = wl_surface::WlSurface::from_id(&connection, surface_id)
                    .map_err(display_error)?;
                let keyboard = seat.get_keyboard(&qh, ());
                connection.flush().map_err(display_error)?;

                Ok(Self {
                    connection,
                    queue,
                    state: WaylandState::default(),
                    manager,
                    seat,
                    keyboard,
                    surface,
                    inhibitor: None,
                })
            }

            pub(super) fn set_captured(&mut self, captured: bool) -> Result<(), String> {
                if captured == self.inhibitor.is_some() {
                    return Ok(());
                }

                if captured {
                    self.state.active = false;
                    let inhibitor = self.manager.inhibit_shortcuts(
                        &self.surface,
                        &self.seat,
                        &self.queue.handle(),
                        (),
                    );
                    self.inhibitor = Some(inhibitor);
                    self.queue
                        .roundtrip(&mut self.state)
                        .map_err(display_error)?;
                    if !self.state.active {
                        if let Some(inhibitor) = self.inhibitor.take() {
                            inhibitor.destroy();
                            let _ = self.connection.flush();
                        }
                        return Err("the compositor did not activate shortcut inhibition".into());
                    }
                } else if let Some(inhibitor) = self.inhibitor.take() {
                    inhibitor.destroy();
                    self.connection.flush().map_err(display_error)?;
                    self.state.active = false;
                }

                Ok(())
            }

            pub(super) fn super_pressed(&mut self) -> bool {
                if let Err(error) = self.queue.dispatch_pending(&mut self.state) {
                    log::warn!("Could not dispatch Wayland keyboard state: {error}");
                }
                self.state.super_keys_down.iter().any(|pressed| *pressed)
            }
        }

        impl Drop for WaylandCapture {
            fn drop(&mut self) {
                let _ = self.set_captured(false);
                if self.keyboard.version() >= 3 {
                    self.keyboard.release();
                }
                self.manager.destroy();
                if self.seat.version() >= 5 {
                    self.seat.release();
                }
                let _ = self.connection.flush();
            }
        }

        impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for WaylandState {
            fn event(
                _: &mut Self,
                _: &wl_registry::WlRegistry,
                _: wl_registry::Event,
                _: &GlobalListContents,
                _: &Connection,
                _: &QueueHandle<Self>,
            ) {
            }
        }

        wayland_client::delegate_noop!(WaylandState: ignore wl_seat::WlSeat);
        wayland_client::delegate_noop!(
            WaylandState: ZwpKeyboardShortcutsInhibitManagerV1
        );

        impl Dispatch<wl_keyboard::WlKeyboard, ()> for WaylandState {
            fn event(
                state: &mut Self,
                _: &wl_keyboard::WlKeyboard,
                event: wl_keyboard::Event,
                _: &(),
                _: &Connection,
                _: &QueueHandle<Self>,
            ) {
                match event {
                    wl_keyboard::Event::Enter { keys, .. } => {
                        state.super_keys_down = [false; 2];
                        for bytes in keys.chunks_exact(4) {
                            let key = u32::from_ne_bytes(
                                bytes.try_into().expect("four-byte Wayland keycode"),
                            );
                            state.set_super_key(key, true);
                        }
                    }
                    wl_keyboard::Event::Leave { .. } => {
                        state.super_keys_down = [false; 2];
                    }
                    wl_keyboard::Event::Key {
                        key,
                        state: key_state,
                        ..
                    } => {
                        let pressed =
                            matches!(key_state, WEnum::Value(wl_keyboard::KeyState::Pressed));
                        state.set_super_key(key, pressed);
                    }
                    _ => {}
                }
            }
        }

        impl Dispatch<ZwpKeyboardShortcutsInhibitorV1, ()> for WaylandState {
            fn event(
                state: &mut Self,
                _: &ZwpKeyboardShortcutsInhibitorV1,
                event: zwp_keyboard_shortcuts_inhibitor_v1::Event,
                _: &(),
                _: &Connection,
                _: &QueueHandle<Self>,
            ) {
                match event {
                    zwp_keyboard_shortcuts_inhibitor_v1::Event::Active => state.active = true,
                    zwp_keyboard_shortcuts_inhibitor_v1::Event::Inactive => state.active = false,
                    _ => {}
                }
            }
        }

        fn display_error(error: impl std::fmt::Display) -> String {
            error.to_string()
        }

        impl WaylandState {
            fn set_super_key(&mut self, key: u32, pressed: bool) {
                match key {
                    KEY_LEFTMETA => self.super_keys_down[0] = pressed,
                    KEY_RIGHTMETA => self.super_keys_down[1] = pressed,
                    _ => {}
                }
            }
        }

        #[cfg(test)]
        mod tests {
            use super::{WaylandState, KEY_LEFTMETA, KEY_RIGHTMETA};

            #[test]
            fn tracks_both_wayland_super_keys() {
                let mut state = WaylandState::default();

                state.set_super_key(KEY_LEFTMETA, true);
                state.set_super_key(KEY_RIGHTMETA, true);
                assert_eq!(state.super_keys_down, [true, true]);

                state.set_super_key(KEY_LEFTMETA, false);
                assert_eq!(state.super_keys_down, [false, true]);
            }
        }
    }

    use wayland::WaylandCapture;
}

#[cfg(target_os = "linux")]
pub use linux::SystemInputCapture;

#[cfg(not(target_os = "linux"))]
pub struct SystemInputCapture;

#[cfg(not(target_os = "linux"))]
impl SystemInputCapture {
    pub fn new(_: &eframe::CreationContext<'_>) -> Self {
        Self
    }

    pub fn set_captured(&mut self, captured: bool) -> bool {
        !captured
    }

    pub fn super_pressed(&mut self) -> bool {
        false
    }
}
