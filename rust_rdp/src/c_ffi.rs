use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::sync::Arc;

use crate::{
    connect_session, disconnect_session, init_runtime, send_key_event, send_mouse_event,
    send_mouse_horizontal_wheel_event, send_mouse_wheel_event, send_scancode_event,
    SessionCallback,
};

/// C function pointer signatures for callback notifications to C / Swift.
pub type StateChangedCallback =
    extern "C" fn(user_data: *mut c_void, state: c_int, message: *const c_char);
pub type FrameDecodedCallback = extern "C" fn(
    user_data: *mut c_void,
    pixels: *const i32,
    len: c_int,
    x: c_int,
    y: c_int,
    width: c_int,
    height: c_int,
);
pub type ResolutionChangedCallback =
    extern "C" fn(user_data: *mut c_void, width: c_int, height: c_int);

struct CCallback {
    user_data: *mut c_void,
    on_state_changed: StateChangedCallback,
    on_frame_decoded: FrameDecodedCallback,
    on_resolution_changed: ResolutionChangedCallback,
}

unsafe impl Send for CCallback {}
unsafe impl Sync for CCallback {}

impl SessionCallback for CCallback {
    fn on_state_changed(&self, state: i32, message: &str) {
        let c_msg = CString::new(message).unwrap_or_default();
        (self.on_state_changed)(self.user_data, state as c_int, c_msg.as_ptr());
    }

    fn on_frame_decoded(&self, pixels: &[i32], x: i32, y: i32, width: i32, height: i32) {
        (self.on_frame_decoded)(
            self.user_data,
            pixels.as_ptr(),
            pixels.len() as c_int,
            x as c_int,
            y as c_int,
            width as c_int,
            height as c_int,
        );
    }

    fn on_resolution_changed(&self, width: i32, height: i32) {
        (self.on_resolution_changed)(self.user_data, width as c_int, height as c_int);
    }
}

/// Initialize the Tokio runtime for iOS / C clients.
#[no_mangle]
pub extern "C" fn rust_rdp_init() {
    init_runtime();
}

/// Connect to an RDP or VNC server.
/// Returns a monotonic session ID.
#[no_mangle]
pub extern "C" fn rust_rdp_connect(
    host: *const c_char,
    port: c_int,
    username: *const c_char,
    password: *const c_char,
    domain: *const c_char,
    width: c_int,
    height: c_int,
    conn_mode: *const c_char,
    user_data: *mut c_void,
    on_state_changed: StateChangedCallback,
    on_frame_decoded: FrameDecodedCallback,
    on_resolution_changed: ResolutionChangedCallback,
) -> u64 {
    let host_str = if host.is_null() {
        ""
    } else {
        unsafe { CStr::from_ptr(host) }.to_str().unwrap_or("")
    };
    let user_str = if username.is_null() {
        ""
    } else {
        unsafe { CStr::from_ptr(username) }.to_str().unwrap_or("")
    };
    let pass_str = if password.is_null() {
        ""
    } else {
        unsafe { CStr::from_ptr(password) }.to_str().unwrap_or("")
    };
    let domain_str = if domain.is_null() {
        ""
    } else {
        unsafe { CStr::from_ptr(domain) }.to_str().unwrap_or("")
    };
    let conn_mode_str = if conn_mode.is_null() {
        "RDP"
    } else {
        unsafe { CStr::from_ptr(conn_mode) }
            .to_str()
            .unwrap_or("RDP")
    };

    let cb: Arc<dyn SessionCallback> = Arc::new(CCallback {
        user_data,
        on_state_changed,
        on_frame_decoded,
        on_resolution_changed,
    });

    connect_session(
        host_str.to_string(),
        port as i32,
        user_str.to_string(),
        pass_str.to_string(),
        domain_str.to_string(),
        width as i32,
        height as i32,
        conn_mode_str.to_string(),
        cb,
    )
}

/// Disconnect all active sessions.
#[no_mangle]
pub extern "C" fn rust_rdp_disconnect() {
    disconnect_session();
}

/// Send mouse position and click action.
#[no_mangle]
pub extern "C" fn rust_rdp_send_mouse_event(x: c_int, y: c_int, action: c_int) {
    send_mouse_event(x as i32, y as i32, action as i32);
}

/// Send vertical mouse wheel scroll.
#[no_mangle]
pub extern "C" fn rust_rdp_send_mouse_wheel_event(x: c_int, y: c_int, units: c_int) {
    send_mouse_wheel_event(x as i32, y as i32, units as i32);
}

/// Send horizontal mouse wheel scroll.
#[no_mangle]
pub extern "C" fn rust_rdp_send_mouse_horizontal_wheel_event(x: c_int, y: c_int, units: c_int) {
    send_mouse_horizontal_wheel_event(x as i32, y as i32, units as i32);
}

/// Send character / key event.
#[no_mangle]
pub extern "C" fn rust_rdp_send_key_event(keycode: c_int, pressed: c_int) {
    send_key_event(keycode as i32, pressed as i32);
}

/// Send hardware scancode event.
#[no_mangle]
pub extern "C" fn rust_rdp_send_scancode_event(
    scancode: c_int,
    is_extended: c_int,
    pressed: c_int,
) {
    send_scancode_event(scancode as i32, is_extended != 0, pressed as i32);
}
